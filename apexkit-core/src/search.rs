use tantivy::collector::TopDocs;
use tantivy::schema::{
    Schema, STORED, INDEXED, FAST, TEXT, Term, Field, Value, 
    FieldType as TantivyFieldType
};
use tantivy::{Index, IndexWriter, ReloadPolicy, TantivyDocument};
use tantivy::directory::MmapDirectory;
use tantivy::query::{QueryParser, BooleanQuery, FuzzyTermQuery, TermQuery, Query, Occur}; // Added TermQuery
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::fs; 
use crate::schema::{CollectionSchema, FieldType};
use crate::models::InstantResult;
use serde_json::{Map, Value as JsonValue};

pub struct SearchManager {
    base_path: PathBuf,
    writers: Arc<Mutex<HashMap<i64, IndexWriter>>>,
    indexes: Arc<Mutex<HashMap<i64, Index>>>,
}

impl SearchManager {
    pub fn new(path: &str) -> Self {
        let base_path = PathBuf::from(path);
        fs::create_dir_all(&base_path).unwrap();
        Self {
            base_path,
            writers: Arc::new(Mutex::new(HashMap::new())),
            indexes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn load_index(&self, collection_id: i64, schema: &CollectionSchema) -> Result<(), String> {
        {
            let w_lock = self.writers.lock().unwrap();
            if w_lock.contains_key(&collection_id) {
                return Ok(());
            }
        }

        let index_path = self.base_path.join(collection_id.to_string());
        fs::create_dir_all(&index_path).map_err(|e| e.to_string())?;

        let mut schema_builder = Schema::builder();
        schema_builder.add_i64_field("record_id", STORED | INDEXED | FAST);

        let mut sorted_fields: Vec<_> = schema.fields.iter().collect();
        sorted_fields.sort_by_key(|(name, _)| *name);

        for (name, def) in sorted_fields {
            if def.ose_indexed {
                match def.r#type {
                    FieldType::Number => {
                        // Stored as F64
                        schema_builder.add_f64_field(name, STORED | INDEXED | FAST);
                    },
                    FieldType::Boolean => {
                        schema_builder.add_u64_field(name, STORED | INDEXED | FAST);
                    },
                    FieldType::GeoPoint => {
                        schema_builder.add_f64_field(&format!("{}_lat", name), STORED | INDEXED | FAST);
                        schema_builder.add_f64_field(&format!("{}_lng", name), STORED | INDEXED | FAST);
                    },
                    _ => {
                        schema_builder.add_text_field(name, TEXT | STORED);
                    }
                }
            }
        }
        let tantivy_schema = schema_builder.build();

        let dir = MmapDirectory::open(&index_path).map_err(|e| e.to_string())?;
        
        let index = match Index::open_or_create(dir.clone(), tantivy_schema.clone()) {
            Ok(idx) => idx,
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("schema does not match") {
                    println!("[Search] Schema mismatch detected for col {}. Rebuilding index...", collection_id);
                    let _ = fs::remove_dir_all(&index_path);
                    let _ = fs::create_dir_all(&index_path);
                    let new_dir = MmapDirectory::open(&index_path).map_err(|e| e.to_string())?;
                    Index::create(new_dir, tantivy_schema, tantivy::IndexSettings::default())
                        .map_err(|e| format!("Failed to recreate index: {}", e))?
                } else {
                    return Err(format!("Tantivy Error: {}", err_msg));
                }
            }
        };

        let writer = index.writer(50_000_000).map_err(|e| e.to_string())?;
        let mut w_lock = self.writers.lock().unwrap();
        let mut i_lock = self.indexes.lock().unwrap();
        w_lock.insert(collection_id, writer);
        i_lock.insert(collection_id, index);

        Ok(())
    }

    pub fn delete_index(&self, collection_id: i64) -> Result<(), String> {
        {
            let mut w_lock = self.writers.lock().unwrap();
            w_lock.remove(&collection_id);
        }
        {
            let mut i_lock = self.indexes.lock().unwrap();
            i_lock.remove(&collection_id);
        }
        let index_path = self.base_path.join(collection_id.to_string());
        if index_path.exists() {
            fs::remove_dir_all(&index_path).map_err(|e| format!("Failed to delete index dir: {}", e))?;
        }
        Ok(())
    }

    pub fn get_doc_count(&self, collection_id: i64) -> Result<u64, String> {
        let lock = self.indexes.lock().unwrap();
        if let Some(index) = lock.get(&collection_id) {
            let reader = index.reader_builder()
                .reload_policy(ReloadPolicy::Manual)
                .try_into()
                .map_err(|e| e.to_string())?;
            return Ok(reader.searcher().num_docs());
        }
        Ok(0)
    }

    pub fn index_record(&self, collection_id: i64, record_id: i64, data: &JsonValue, schema: &CollectionSchema) -> Result<(), String> {
        let mut lock = self.writers.lock().unwrap();
        let writer = lock.get_mut(&collection_id).ok_or("Index not loaded")?;
        
        let index_schema = writer.index().schema();
        let mut doc = TantivyDocument::default();

        let id_field = index_schema.get_field("record_id").map_err(|_| "Field not found")?;
        doc.add_i64(id_field, record_id);

        for (name, def) in &schema.fields {
            if def.ose_indexed {
                if let Ok(field) = index_schema.get_field(name) {
                    if let Some(val) = data.get(name) {
                        match def.r#type {
                            FieldType::Number => {
                                if let Some(n) = val.as_f64() {
                                    doc.add_f64(field, n);
                                }
                            },
                            FieldType::Boolean => {
                                if let Some(b) = val.as_bool() {
                                    doc.add_u64(field, if b { 1 } else { 0 });
                                }
                            },
                            FieldType::GeoPoint => {
                                if let Some(obj) = val.as_object() {
                                    let lat = obj.get("lat").and_then(|v| v.as_f64());
                                    let lng = obj.get("lng").or_else(|| obj.get("lon")).and_then(|v| v.as_f64());

                                    if let (Some(l), Some(g)) = (lat, lng) {
                                        if let Ok(field_lat) = index_schema.get_field(&format!("{}_lat", name)) {
                                            doc.add_f64(field_lat, l);
                                        }
                                        if let Ok(field_lng) = index_schema.get_field(&format!("{}_lng", name)) {
                                            doc.add_f64(field_lng, g);
                                        }
                                    }
                                }
                            },
                            _ => {
                                let text_val = if let Some(s) = val.as_str() {
                                    s.to_string()
                                } else if val.is_null() {
                                    "".to_string()
                                } else {
                                    val.to_string()
                                };
                                
                                if !text_val.is_empty() {
                                    doc.add_text(field, &text_val);
                                }
                            }
                        }
                    }
                }
            }
        }

        writer.delete_term(Term::from_field_i64(id_field, record_id));
        writer.add_document(doc).map_err(|e| e.to_string())?;
        writer.commit().map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn index_batch(&self, collection_id: i64, records: &[(i64, serde_json::Value)], schema: &CollectionSchema) -> Result<(), String> {
        let mut lock = self.writers.lock().unwrap();
        let writer = lock.get_mut(&collection_id).ok_or("Index not loaded")?;
        
        let index_schema = writer.index().schema();
        let id_field = index_schema.get_field("record_id").map_err(|_| "Field not found")?;

        for (record_id, data) in records {
            let mut doc = TantivyDocument::default();
            doc.add_i64(id_field, *record_id);

            for (name, def) in &schema.fields {
                if def.ose_indexed {
                    if let Ok(field) = index_schema.get_field(name) {
                        if let Some(val) = data.get(name) {
                            match def.r#type {
                                FieldType::Number => {
                                    if let Some(n) = val.as_f64() { doc.add_f64(field, n); }
                                },
                                FieldType::Boolean => {
                                    if let Some(b) = val.as_bool() { doc.add_u64(field, if b { 1 } else { 0 }); }
                                },
                                FieldType::GeoPoint => {
                                    if let Some(obj) = val.as_object() {
                                        let lat = obj.get("lat").and_then(|v| v.as_f64());
                                        let lng = obj.get("lng").or_else(|| obj.get("lon")).and_then(|v| v.as_f64());
                                        if let (Some(l), Some(g)) = (lat, lng) {
                                            if let Ok(f_lat) = index_schema.get_field(&format!("{}_lat", name)) { doc.add_f64(f_lat, l); }
                                            if let Ok(f_lng) = index_schema.get_field(&format!("{}_lng", name)) { doc.add_f64(f_lng, g); }
                                        }
                                    }
                                },
                                _ => {
                                    let text_val = if let Some(s) = val.as_str() { s.to_string() } 
                                    else if val.is_null() { "".to_string() } 
                                    else { val.to_string() };
                                    
                                    if !text_val.is_empty() { doc.add_text(field, &text_val); }
                                }
                            }
                        }
                    }
                }
            }
            writer.add_document(doc).map_err(|e| e.to_string())?;
        }
        writer.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_record(&self, collection_id: i64, record_id: i64) -> Result<(), String> {
        let mut lock = self.writers.lock().unwrap();
        if let Some(writer) = lock.get_mut(&collection_id) {
            let id_field = writer.index().schema().get_field("record_id").map_err(|_| "Schema error")?;
            writer.delete_term(Term::from_field_i64(id_field, record_id));
            writer.commit().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn search(&self, collection_id: i64, query_str: &str, limit: usize) -> Result<Vec<i64>, String> {
        let results = self.instant_search(collection_id, query_str, limit)?;
        Ok(results.into_iter().map(|r| r.id).collect())
    }

    pub fn instant_search(&self, collection_id: i64, query_str: &str, limit: usize) -> Result<Vec<InstantResult>, String> {
        let lock = self.indexes.lock().unwrap();
        let index = lock.get(&collection_id).ok_or("Index not loaded")?;

        let reader = index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into().map_err(|e| e.to_string())?;
        let searcher = reader.searcher();
        let schema = index.schema();

        // 1. Separate String fields and Number fields
        let text_fields: Vec<Field> = schema.fields()
            .filter(|(_, entry)| matches!(entry.field_type(), TantivyFieldType::Str(_)))
            .map(|(f, _)| f)
            .collect();

        let number_fields: Vec<Field> = schema.fields()
            .filter(|(_, entry)| matches!(entry.field_type(), TantivyFieldType::F64(_)))
            .map(|(f, _)| f)
            .collect();
        
        let trimmed = query_str.trim();
        if trimmed.is_empty() { return Ok(vec![]); }
        
        let terms: Vec<&str> = trimmed.split_whitespace().collect();
        let mut top_level_subqueries: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        let parser = QueryParser::for_index(index, text_fields.clone());

        for term in terms {
            let mut term_subqueries: Vec<(Occur, Box<dyn Query>)> = Vec::new();
            
            // A. Check Text Fields (Fuzzy or Prefix)
            for field in &text_fields {
                let field_name = schema.get_field_name(*field);
                
                if term.len() > 2 {
                    let term_lower = term.to_lowercase(); 
                    let term_val = Term::from_field_text(*field, &term_lower);
                    let fuzzy_q = FuzzyTermQuery::new(term_val, 1, true); 
                    term_subqueries.push((Occur::Should, Box::new(fuzzy_q)));
                }

                let prefix_str = format!("{}:{}*", field_name, term);
                if let Ok(prefix_q) = parser.parse_query(&prefix_str) {
                    term_subqueries.push((Occur::Should, prefix_q));
                }
            }

            // B. Check Number Fields (Exact Match)
            // If the search term is a number, we check if it matches any numeric column
            if let Ok(num_val) = term.parse::<f64>() {
                for field in &number_fields {
                    let term_val = Term::from_field_f64(*field, num_val);
                    let exact_q = TermQuery::new(term_val, tantivy::schema::IndexRecordOption::Basic);
                    term_subqueries.push((Occur::Should, Box::new(exact_q)));
                }
            }

            if !term_subqueries.is_empty() {
                top_level_subqueries.push((Occur::Should, Box::new(BooleanQuery::new(term_subqueries))));
            }
        }

        let query = BooleanQuery::new(top_level_subqueries);

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit)).map_err(|e| e.to_string())?;

        let id_field = schema.get_field("record_id").unwrap();
        let mut results = Vec::new();

        for (score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address).map_err(|e| e.to_string())?;
            let mut doc_id = 0;
            if let Some(val) = retrieved_doc.get_first(id_field) {
                if let Some(id) = val.as_i64() { doc_id = id; }
            }
            let mut snippet = Map::new();
            for (field, entry) in schema.fields() {
                let name = entry.name();
                if name == "record_id" { continue; }
                if let Some(val) = retrieved_doc.get_first(field) {
                    // Handle Numbers and Strings in snippet
                    if let Some(s) = val.as_str() { 
                        snippet.insert(name.to_string(), JsonValue::String(s.to_string())); 
                    } else if let Some(f) = val.as_f64() {
                        snippet.insert(name.to_string(), JsonValue::Number(serde_json::Number::from_f64(f).unwrap()));
                    }
                }
            }
            results.push(InstantResult { id: doc_id, score, snippet: JsonValue::Object(snippet) });
        }
        Ok(results)
    }
}