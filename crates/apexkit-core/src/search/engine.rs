use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tantivy::directory::MmapDirectory;
use tantivy::schema::{Schema, Term};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

use super::schema::build_tantivy_schema;
use crate::models::schema::CollectionSchema;

pub const WRITER_HEAP_BYTES: usize = 50_000_000;
pub const FUZZY_MIN_LEN: usize = 4;
pub const FUZZY_DISTANCE: u8 = 1;

pub struct CollectionIndex {
    pub index: Index,
    pub reader: IndexReader,
    pub writer: IndexWriter,
}

pub struct SearchManager {
    pub base_path: PathBuf,
    pub collections: Arc<RwLock<HashMap<i64, CollectionIndex>>>,
}

impl SearchManager {
    pub fn new(path: &str) -> Self {
        let base_path = PathBuf::from(path);
        fs::create_dir_all(&base_path).expect("Failed to create search base directory");
        Self {
            base_path,
            collections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn load_index(&self, collection_id: i64, schema: &CollectionSchema) -> Result<(), String> {
        // Fast path
        if self
            .collections
            .read()
            .unwrap()
            .contains_key(&collection_id)
        {
            return Ok(());
        }

        let index_path = self.base_path.join(collection_id.to_string());
        fs::create_dir_all(&index_path).map_err(|e| e.to_string())?;

        let tantivy_schema = build_tantivy_schema(schema);
        let index = Self::open_or_recreate(&index_path, tantivy_schema).map_err(|e| {
            format!(
                "Failed to open index for collection {}: {}",
                collection_id, e
            )
        })?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| e.to_string())?;

        let writer = index.writer(WRITER_HEAP_BYTES).map_err(|e| e.to_string())?;

        self.collections.write().unwrap().insert(
            collection_id,
            CollectionIndex {
                index,
                reader,
                writer,
            },
        );

        Ok(())
    }

    pub fn delete_index(&self, collection_id: i64) -> Result<(), String> {
        self.collections.write().unwrap().remove(&collection_id);

        let index_path = self.base_path.join(collection_id.to_string());
        if index_path.exists() {
            fs::remove_dir_all(&index_path)
                .map_err(|e| format!("Failed to delete index dir: {}", e))?;
        }
        Ok(())
    }

    pub fn get_doc_count(&self, collection_id: i64) -> Result<u64, String> {
        let lock = self.collections.read().unwrap();
        match lock.get(&collection_id) {
            Some(ci) => Ok(ci.reader.searcher().num_docs()),
            None => Ok(0),
        }
    }

    pub fn index_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &JsonValue,
        schema: &CollectionSchema,
    ) -> Result<(), String> {
        let mut lock = self.collections.write().unwrap();
        let ci = lock.get_mut(&collection_id).ok_or("Index not loaded")?;

        let id_field = ci
            .index
            .schema()
            .get_field("record_id")
            .map_err(|_| "record_id field missing from schema")?;

        ci.writer
            .delete_term(Term::from_field_i64(id_field, record_id));

        let doc = Self::build_document(&ci.index.schema(), record_id, data, schema);
        ci.writer.add_document(doc).map_err(|e| e.to_string())?;
        ci.writer.commit().map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn index_batch(
        &self,
        collection_id: i64,
        records: &[(i64, JsonValue)],
        schema: &CollectionSchema,
    ) -> Result<(), String> {
        if records.is_empty() {
            return Ok(());
        }

        let mut lock = self.collections.write().unwrap();
        let ci = lock.get_mut(&collection_id).ok_or("Index not loaded")?;

        let index_schema = ci.index.schema();
        let id_field = index_schema
            .get_field("record_id")
            .map_err(|_| "record_id field missing from schema")?;

        for (record_id, data) in records {
            ci.writer
                .delete_term(Term::from_field_i64(id_field, *record_id));
            let doc = Self::build_document(&index_schema, *record_id, data, schema);
            ci.writer.add_document(doc).map_err(|e| e.to_string())?;
        }

        ci.writer.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_record(&self, collection_id: i64, record_id: i64) -> Result<(), String> {
        let mut lock = self.collections.write().unwrap();
        if let Some(ci) = lock.get_mut(&collection_id) {
            let id_field = ci
                .index
                .schema()
                .get_field("record_id")
                .map_err(|_| "Schema error")?;
            ci.writer
                .delete_term(Term::from_field_i64(id_field, record_id));
            ci.writer.commit().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn open_or_recreate(
        index_path: &std::path::Path,
        schema: Schema,
    ) -> Result<Index, tantivy::TantivyError> {
        let dir = MmapDirectory::open(index_path)?;
        match Index::open_or_create(dir, schema.clone()) {
            Ok(idx) => Ok(idx),
            Err(e) if e.to_string().contains("schema does not match") => {
                println!(
                    "[Search] Schema mismatch at {:?} – rebuilding index.",
                    index_path
                );
                let _ = fs::remove_dir_all(index_path);
                fs::create_dir_all(index_path)
                    .map_err(|io| tantivy::TantivyError::IoError(std::sync::Arc::new(io)))?;
                let new_dir = MmapDirectory::open(index_path)?;
                Index::create(new_dir, schema, tantivy::IndexSettings::default())
            }
            Err(e) => Err(e),
        }
    }

    fn build_document(
        index_schema: &Schema,
        record_id: i64,
        data: &JsonValue,
        collection_schema: &CollectionSchema,
    ) -> TantivyDocument {
        let mut doc = TantivyDocument::default();

        let id_field = index_schema
            .get_field("record_id")
            .expect("record_id must exist");
        doc.add_i64(id_field, record_id);

        for (name, def) in &collection_schema.fields {
            if !def.ose_indexed {
                continue;
            }

            match def.r#type {
                crate::models::schema::FieldType::GeoPoint => {
                    let Some(obj) = data.get(name).and_then(|v| v.as_object()) else {
                        continue;
                    };
                    let lat = obj.get("lat").and_then(|v| v.as_f64());
                    let lng = obj
                        .get("lng")
                        .or_else(|| obj.get("lon"))
                        .and_then(|v| v.as_f64());

                    if let (Some(lat), Some(lng)) = (lat, lng) {
                        if let Ok(f) = index_schema.get_field(&format!("{}_lat", name)) {
                            doc.add_f64(f, lat);
                        }
                        if let Ok(f) = index_schema.get_field(&format!("{}_lng", name)) {
                            doc.add_f64(f, lng);
                        }
                    }
                }
                _ => {
                    let Ok(field) = index_schema.get_field(name) else {
                        continue;
                    };
                    let Some(val) = data.get(name) else {
                        continue;
                    };

                    match def.r#type {
                        crate::models::schema::FieldType::Number => {
                            if let Some(n) = val.as_f64() {
                                doc.add_f64(field, n);
                            }
                        }
                        crate::models::schema::FieldType::Boolean => {
                            if let Some(b) = val.as_bool() {
                                doc.add_u64(field, u64::from(b));
                            }
                        }
                        _ => {
                            let text = if let Some(s) = val.as_str() {
                                s.to_string()
                            } else if val.is_null() {
                                return doc;
                            } else {
                                val.to_string()
                            };
                            if !text.is_empty() {
                                doc.add_text(field, &text);
                            }
                        }
                    }
                }
            }
        }
        doc
    }
}
