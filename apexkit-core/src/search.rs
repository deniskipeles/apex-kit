use tantivy::collector::TopDocs;
// use tantivy::query::QueryParser;
use tantivy::schema::{
    Schema, STORED, INDEXED, FAST, TEXT, Term, Field, Value, 
    FieldType as TantivyFieldType
};
use tantivy::{Index, IndexWriter, ReloadPolicy, TantivyDocument};
use tantivy::directory::MmapDirectory;
use tantivy::query::{QueryParser, BooleanQuery, FuzzyTermQuery, Query, Occur}; // Ensure these are imported
// use tantivy::Term;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::fs; // Added for file system operations
use crate::schema::{CollectionSchema, FieldType};
use crate::models::InstantResult;
use serde_json::{Map, Value as JsonValue};

/// Manages Tantivy Indexes for multiple collections
pub struct SearchManager {
    base_path: PathBuf,
    // Map collection_id -> (Index, IndexWriter)
    // Wrapped in Mutex because IndexWriter is !Sync
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

    /// Initialize or load index for a collection
    /// Includes Auto-Healing for schema mismatches and Lock caching
    pub fn load_index(&self, collection_id: i64, schema: &CollectionSchema) -> Result<(), String> {
        // 1. CHECK CACHE FIRST (Fixes LockBusy)
        // If we already have a writer for this collection, do nothing.
        {
            let w_lock = self.writers.lock().unwrap();
            if w_lock.contains_key(&collection_id) {
                return Ok(());
            }
        }

        let index_path = self.base_path.join(collection_id.to_string());
        fs::create_dir_all(&index_path).map_err(|e| e.to_string())?;

        // 2. Build Tantivy Schema
        let mut schema_builder = Schema::builder();
        // Always add a system field for record ID
        schema_builder.add_i64_field("record_id", STORED | INDEXED | FAST);

        // [FIX] Sort fields alphabetically to ensure Deterministic Field IDs
        // This prevents "Schema Mismatch" errors on app restart due to HashMap randomization
        let mut sorted_fields: Vec<_> = schema.fields.iter().collect();
        sorted_fields.sort_by_key(|(name, _)| *name);

        for (name, def) in sorted_fields {
            if def.ose_indexed {
                match def.r#type {
                    FieldType::String | FieldType::Text => {
                        schema_builder.add_text_field(name, TEXT | STORED);
                    },
                    FieldType::Number => {
                        schema_builder.add_f64_field(name, STORED | INDEXED | FAST);
                    },
                    FieldType::Boolean => {
                        schema_builder.add_u64_field(name, STORED | INDEXED | FAST);
                    },
                    FieldType::GeoPoint => {
                        schema_builder.add_f64_field(&format!("{}_lat", name), STORED | INDEXED | FAST);
                        schema_builder.add_f64_field(&format!("{}_lng", name), STORED | INDEXED | FAST);
                    },
                    _ => {} 
                }
            }
        }
        let tantivy_schema = schema_builder.build();

        // 3. Open or Create Index (With Auto-Healing)
        let dir = MmapDirectory::open(&index_path).map_err(|e| e.to_string())?;
        
        let index = match Index::open_or_create(dir.clone(), tantivy_schema.clone()) {
            Ok(idx) => idx,
            Err(e) => {
                let err_msg = e.to_string();
                // If schema mismatch, nuke the folder and recreate
                if err_msg.contains("schema does not match") {
                    println!("[Search] Schema mismatch detected for col {}. Rebuilding index...", collection_id);
                    
                    // Attempt to clean up old files
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

        // 4. Create Writer (50MB buffer)
        let writer = index.writer(50_000_000).map_err(|e| e.to_string())?;

        // 5. Store in Map
        let mut w_lock = self.writers.lock().unwrap();
        let mut i_lock = self.indexes.lock().unwrap();
        w_lock.insert(collection_id, writer);
        i_lock.insert(collection_id, index);

        Ok(())
    }

    /// Completely removes an index from memory and disk
    pub fn delete_index(&self, collection_id: i64) -> Result<(), String> {
        // 1. Remove from memory maps to drop file locks
        {
            let mut w_lock = self.writers.lock().unwrap();
            w_lock.remove(&collection_id);
        }
        {
            let mut i_lock = self.indexes.lock().unwrap();
            i_lock.remove(&collection_id);
        }

        // 2. Delete directory from disk
        let index_path = self.base_path.join(collection_id.to_string());
        if index_path.exists() {
            fs::remove_dir_all(&index_path).map_err(|e| format!("Failed to delete index dir: {}", e))?;
        }
        Ok(())
    }

    /// Returns the number of documents in the index.
    /// Used for health checks and startup recovery.
    pub fn get_doc_count(&self, collection_id: i64) -> Result<u64, String> {
        let lock = self.indexes.lock().unwrap();
        if let Some(index) = lock.get(&collection_id) {
            let reader = index.reader_builder()
                .reload_policy(ReloadPolicy::Manual)
                .try_into()
                .map_err(|e| e.to_string())?;
            return Ok(reader.searcher().num_docs());
        }
        // If index isn't loaded or doesn't exist, count is 0
        Ok(0)
    }

    pub fn index_record(&self, collection_id: i64, record_id: i64, data: &JsonValue, schema: &CollectionSchema) -> Result<(), String> {
        let mut lock = self.writers.lock().unwrap();
        let writer = lock.get_mut(&collection_id).ok_or("Index not loaded")?;
        
        let index_schema = writer.index().schema();
        let mut doc = TantivyDocument::default();

        // Add System ID
        let id_field = index_schema.get_field("record_id").map_err(|_| "Field not found")?;
        doc.add_i64(id_field, record_id);

        // Add User Fields
        for (name, def) in &schema.fields {
            if def.ose_indexed {
                if let Ok(field) = index_schema.get_field(name) {
                    if let Some(val) = data.get(name) {
                        match def.r#type {
                            FieldType::String | FieldType::Text => {
                                if let Some(s) = val.as_str() {
                                    doc.add_text(field, s);
                                }
                            },
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
                            // --- OPTIMIZATION: INJECT FLATTENED DATA ---
                            FieldType::GeoPoint => {
                                if let Some(obj) = val.as_object() {
                                    let lat = obj.get("lat").and_then(|v| v.as_f64());
                                    let lng = obj.get("lng").or_else(|| obj.get("lon")).and_then(|v| v.as_f64());

                                    if let (Some(l), Some(g)) = (lat, lng) {
                                        // Find the schema fields we created in load_index
                                        if let Ok(field_lat) = index_schema.get_field(&format!("{}_lat", name)) {
                                            doc.add_f64(field_lat, l);
                                        }
                                        if let Ok(field_lng) = index_schema.get_field(&format!("{}_lng", name)) {
                                            doc.add_f64(field_lng, g);
                                        }
                                    }
                                }
                            },
                            _ => {}
                        }
                    }
                }
            }
        }

        // Delete existing doc for this ID first (Update logic)
        writer.delete_term(Term::from_field_i64(id_field, record_id));
        writer.add_document(doc).map_err(|e| e.to_string())?;
        writer.commit().map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Optimized method for re-indexing: Adds multiple documents and commits ONCE.
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
                            // ... (Copy your field mapping logic from index_record here) ...
                            // START COPY
                            match def.r#type {
                                FieldType::String | FieldType::Text => {
                                    if let Some(s) = val.as_str() { doc.add_text(field, s); }
                                },
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
                                _ => {}
                            }
                            // END COPY
                        }
                    }
                }
            }
            writer.add_document(doc).map_err(|e| e.to_string())?;
        }

        // ONE COMMIT FOR THE WHOLE BATCH
        writer.commit().map_err(|e| e.to_string())?;
        
        Ok(())
    }

    pub fn delete_record(&self, collection_id: i64, record_id: i64) -> Result<(), String> {
        let mut lock = self.writers.lock().unwrap();
        // If index isn't loaded, we can't delete from it, but that's fine (it might not exist)
        if let Some(writer) = lock.get_mut(&collection_id) {
            let id_field = writer.index().schema().get_field("record_id").map_err(|_| "Schema error")?;
            writer.delete_term(Term::from_field_i64(id_field, record_id));
            writer.commit().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Returns a list of Record IDs matching the query (For Full Search)
    /// Reuses the logic from instant_search to ensure consistency.
    pub fn search(&self, collection_id: i64, query_str: &str, limit: usize) -> Result<Vec<i64>, String> {
        // Reuse instant_search logic to get ranked results
        let results = self.instant_search(collection_id, query_str, limit)?;
        
        // Extract just the IDs, preserving the relevance order
        Ok(results.into_iter().map(|r| r.id).collect())
    }

    /// Returns lightweight results directly from Index (For Instant Search)
    /// Returns lightweight results directly from Index (For Instant Search)
    pub fn instant_search(&self, collection_id: i64, query_str: &str, limit: usize) -> Result<Vec<InstantResult>, String> {
        let lock = self.indexes.lock().unwrap();
        let index = lock.get(&collection_id).ok_or("Index not loaded")?;

        let reader = index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into().map_err(|e| e.to_string())?;
        let searcher = reader.searcher();
        let schema = index.schema();

        let default_fields: Vec<Field> = schema.fields()
            .filter(|(_, entry)| matches!(entry.field_type(), TantivyFieldType::Str(_)))
            .map(|(f, _)| f)
            .collect();
        
        if default_fields.is_empty() { return Ok(vec![]); }

        let trimmed = query_str.trim();
        if trimmed.is_empty() { return Ok(vec![]); }
        
        // --- MANUAL QUERY CONSTRUCTION ---
        // We bypass the string parser to ensure strict "Fuzzy OR Prefix" logic per word.
        
        let terms: Vec<&str> = trimmed.split_whitespace().collect();
        let mut top_level_subqueries: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        // We use a temporary parser helper just for creating correct Prefix Queries (handling casing/analyzers)
        let parser = QueryParser::for_index(index, default_fields.clone());

        for term in terms {
            let mut term_subqueries: Vec<(Occur, Box<dyn Query>)> = Vec::new();
            
            // Logic: This specific word (e.g. "harr") MUST match...
            // ... at least one FIELD ...
            // ... via either FUZZY (typo) OR PREFIX (autocomplete).
            
            for field in &default_fields {
                let field_name = schema.get_field_name(*field);
                
                // 1. Fuzzy Match (Typo tolerance)
                // Only apply if term is long enough (>2 chars) to avoid noise on short words like "is" -> "if"
                if term.len() > 2 {
                    // Note: We create a raw Term. 
                    // For "standard" tokenizer, it lowercases. We manually lowercase here to match the index term.
                    let term_lower = term.to_lowercase(); 
                    let term_val = Term::from_field_text(*field, &term_lower);
                    
                    let fuzzy_q = FuzzyTermQuery::new(term_val, 1, true); // 1 edit distance, transpositions=true
                    term_subqueries.push((Occur::Should, Box::new(fuzzy_q)));
                }

                // 2. Prefix Match (Autocomplete behavior)
                // We ask the parser to generate the query for "field:term*" 
                // This ensures the analyzer (lowercasing) runs correctly on the prefix.
                let prefix_str = format!("{}:{}*", field_name, term);
                if let Ok(prefix_q) = parser.parse_query(&prefix_str) {
                    term_subqueries.push((Occur::Should, prefix_q));
                }
            }

            // If we generated any subqueries for this term, add them to the main AND group
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
                    if let Some(s) = val.as_str() { snippet.insert(name.to_string(), JsonValue::String(s.to_string())); }
                }
            }
            results.push(InstantResult { id: doc_id, score, snippet: JsonValue::Object(snippet) });
        }
        Ok(results)
    }
}