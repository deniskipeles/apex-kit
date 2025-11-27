use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{
    Schema, STORED, INDEXED, FAST, TEXT, Term, Field, Value, 
    FieldType as TantivyFieldType
};
use tantivy::{Index, IndexWriter, ReloadPolicy, TantivyDocument};
use tantivy::directory::MmapDirectory;
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

        for (name, def) in &schema.fields {
            if def.indexed {
                match def.r#type {
                    FieldType::String | FieldType::Text => {
                        schema_builder.add_text_field(name, TEXT | STORED);
                    },
                    FieldType::Number => {
                        schema_builder.add_f64_field(name, STORED | INDEXED | FAST);
                    },
                    FieldType::Boolean => {
                        // Store boolean as u64 (0 or 1)
                        schema_builder.add_u64_field(name, STORED | INDEXED | FAST);
                    },
                    _ => {} // Json not searchable yet
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
                    
                    // Re-open directory on fresh folder
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
            if def.indexed {
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
    pub fn search(&self, collection_id: i64, query_str: &str, limit: usize) -> Result<Vec<i64>, String> {
        let lock = self.indexes.lock().unwrap();
        let index = lock.get(&collection_id).ok_or("Index not loaded")?;

        let reader = index.reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e| e.to_string())?;
        
        let searcher = reader.searcher();
        let schema = index.schema();

        // Filter text fields
        let default_fields: Vec<Field> = schema.fields()
            .filter(|(_, entry)| matches!(entry.field_type(), TantivyFieldType::Str(_)))
            .map(|(f, _)| f)
            .collect();
        
        if default_fields.is_empty() {
            return Ok(vec![]);
        }

        let query_parser = QueryParser::for_index(index, default_fields);
        let query = query_parser.parse_query(query_str).map_err(|e| e.to_string())?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit)).map_err(|e| e.to_string())?;

        let id_field = schema.get_field("record_id").unwrap();
        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address).map_err(|e| e.to_string())?;
            if let Some(val) = retrieved_doc.get_first(id_field) {
                if let Some(id) = val.as_i64() {
                    results.push(id);
                }
            }
        }

        Ok(results)
    }

    /// Returns lightweight results directly from Index (For Instant Search)
    pub fn instant_search(&self, collection_id: i64, query_str: &str, limit: usize) -> Result<Vec<InstantResult>, String> {
        let lock = self.indexes.lock().unwrap();
        let index = lock.get(&collection_id).ok_or("Index not loaded")?;

        // Create a reader for the latest commit
        let reader = index.reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e| e.to_string())?;
        
        let searcher = reader.searcher();
        let schema = index.schema();

        // 1. Get ALL Indexed Text Fields
        // This ensures we search Title AND Description AND Email, etc.
        let default_fields: Vec<Field> = schema.fields()
            .filter(|(_, entry)| matches!(entry.field_type(), TantivyFieldType::Str(_)))
            .map(|(f, _)| f)
            .collect();
        
        if default_fields.is_empty() {
            return Ok(vec![]);
        }

        // 2. Construct Prefix Query
        // If user types "Ap", we make it "Ap*"
        // If user types "Ap Pi", we make it "Ap Pi*" (Last word is prefix)
        let trimmed = query_str.trim();
        let clean_query = if trimmed.is_empty() { 
            "*".to_string() 
        } else {
            // Escape special characters that might break Tantivy syntax (like :, -, +)
            // Then append * to make it a prefix search
            // Simple approach: Just append * to the raw string. 
            // Tantivy QueryParser is smart enough to distribute the * to fields.
            format!("{}*", trimmed)
        };
        
        let query_parser = QueryParser::for_index(index, default_fields);
        
        // 3. Parse Query
        // This automatically expands to: (title:Ap* OR description:Ap*)
        let query = match query_parser.parse_query(&clean_query) {
            Ok(q) => q,
            Err(_) => return Ok(vec![]) // Return empty on invalid syntax
        };

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit)).map_err(|e| e.to_string())?;

        let id_field = schema.get_field("record_id").unwrap();
        let mut results = Vec::new();

        for (score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address).map_err(|e| e.to_string())?;
            
            let mut doc_id = 0;
            if let Some(val) = retrieved_doc.get_first(id_field) {
                if let Some(id) = val.as_i64() {
                    doc_id = id;
                }
            }

            let mut snippet = Map::new();
            for (field, entry) in schema.fields() {
                let name = entry.name();
                if name == "record_id" { continue; }

                if let Some(val) = retrieved_doc.get_first(field) {
                    if let Some(s) = val.as_str() {
                        snippet.insert(name.to_string(), JsonValue::String(s.to_string()));
                    }
                }
            }

            results.push(InstantResult {
                id: doc_id,
                score,
                snippet: JsonValue::Object(snippet),
            });
        }

        Ok(results)
    }
}