use crate::models::InstantResult;
use crate::schema::{CollectionSchema, FieldType};
use serde_json::{Map, Value as JsonValue};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, FuzzyTermQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{
    FAST, Field, FieldType as TantivyFieldType, INDEXED, IndexRecordOption, STORED, Schema, TEXT,
    Term, Value,
};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Tantivy heap size per writer (50 MB).
const WRITER_HEAP_BYTES: usize = 50_000_000;

/// Fuzzy edit distance for terms longer than this threshold.
const FUZZY_MIN_LEN: usize = 4;

/// Maximum fuzzy edit distance applied to long terms.
const FUZZY_DISTANCE: u8 = 1;

// ─── Per-collection state ──────────────────────────────────────────────────────

/// Bundles everything needed to read/write one collection's index.
struct CollectionIndex {
    index: Index,
    /// Persistent reader – refreshed on every commit via `OnCommitWithDelay`.
    reader: IndexReader,
    writer: IndexWriter,
}

// ─── SearchManager ─────────────────────────────────────────────────────────────

/// Thread-safe search engine backed by Tantivy.
///
/// Uses a single `RwLock<HashMap>` instead of two separate `Mutex` locks,
/// eliminating the risk of lock-ordering deadlocks and reducing contention for
/// read-heavy workloads.
pub struct SearchManager {
    base_path: PathBuf,
    collections: Arc<RwLock<HashMap<i64, CollectionIndex>>>,
}

impl SearchManager {
    // ── Construction ──────────────────────────────────────────────────────────

    pub fn new(path: &str) -> Self {
        let base_path = PathBuf::from(path);
        fs::create_dir_all(&base_path).expect("Failed to create search base directory");
        Self {
            base_path,
            collections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ── Schema helpers ────────────────────────────────────────────────────────

    /// Build a Tantivy `Schema` that mirrors the given `CollectionSchema`.
    fn build_tantivy_schema(collection_schema: &CollectionSchema) -> Schema {
        let mut builder = Schema::builder();

        // Internal record-id field – stored + indexed + fast for term deletes.
        builder.add_i64_field("record_id", STORED | INDEXED | FAST);

        // Sort for deterministic schema fingerprinting across restarts.
        let mut sorted: Vec<_> = collection_schema.fields.iter().collect();
        sorted.sort_by_key(|(name, _)| *name);

        for (name, def) in sorted {
            if !def.ose_indexed {
                continue;
            }
            match def.r#type {
                FieldType::Number => {
                    builder.add_f64_field(name, STORED | INDEXED | FAST);
                }
                FieldType::Boolean => {
                    builder.add_u64_field(name, STORED | INDEXED | FAST);
                }
                FieldType::GeoPoint => {
                    // Stored as two separate F64 sub-fields.
                    builder.add_f64_field(&format!("{}_lat", name), STORED | INDEXED | FAST);
                    builder.add_f64_field(&format!("{}_lng", name), STORED | INDEXED | FAST);
                }
                _ => {
                    builder.add_text_field(name, TEXT | STORED);
                }
            }
        }

        builder.build()
    }

    // ── Index lifecycle ───────────────────────────────────────────────────────

    /// Open (or create) the Tantivy index for `collection_id`.
    ///
    /// Idempotent: calling it multiple times is safe and cheap.
    pub fn load_index(&self, collection_id: i64, schema: &CollectionSchema) -> Result<(), String> {
        // Fast path – already loaded.
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

        let tantivy_schema = Self::build_tantivy_schema(schema);
        let index = Self::open_or_recreate(&index_path, tantivy_schema).map_err(|e| {
            format!(
                "Failed to open index for collection {}: {}",
                collection_id, e
            )
        })?;

        // `OnCommitWithDelay` – the reader auto-refreshes shortly after each
        // commit, so searches always reflect the latest writes without a manual
        // reload() call.
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

    /// Remove the in-memory state and delete the on-disk index directory.
    pub fn delete_index(&self, collection_id: i64) -> Result<(), String> {
        // Drop the writer/reader before deleting files.
        self.collections.write().unwrap().remove(&collection_id);

        let index_path = self.base_path.join(collection_id.to_string());
        if index_path.exists() {
            fs::remove_dir_all(&index_path)
                .map_err(|e| format!("Failed to delete index dir: {}", e))?;
        }
        Ok(())
    }

    // ── Document count ────────────────────────────────────────────────────────

    pub fn get_doc_count(&self, collection_id: i64) -> Result<u64, String> {
        let lock = self.collections.read().unwrap();
        match lock.get(&collection_id) {
            Some(ci) => Ok(ci.reader.searcher().num_docs()),
            None => Ok(0),
        }
    }

    // ── Indexing ──────────────────────────────────────────────────────────────

    /// Index (or re-index) a single record.
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

        // Delete any existing version of this record first.
        ci.writer
            .delete_term(Term::from_field_i64(id_field, record_id));

        let doc = Self::build_document(&ci.index.schema(), record_id, data, schema);
        ci.writer.add_document(doc).map_err(|e| e.to_string())?;
        ci.writer.commit().map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Index a batch of records in a single commit (much faster than one-by-one).
    ///
    /// Each record is upserted: any existing document with the same `record_id`
    /// is deleted before the new version is added.
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
            // FIX: delete before insert to prevent duplicate accumulation.
            ci.writer
                .delete_term(Term::from_field_i64(id_field, *record_id));
            let doc = Self::build_document(&index_schema, *record_id, data, schema);
            ci.writer.add_document(doc).map_err(|e| e.to_string())?;
        }

        ci.writer.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Remove a single record from the index.
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

    // ── Search ────────────────────────────────────────────────────────────────

    /// Returns matching record IDs (thin wrapper around `instant_search`).
    pub fn search(
        &self,
        collection_id: i64,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<i64>, String> {
        Ok(self
            .instant_search(collection_id, query_str, limit)?
            .into_iter()
            .map(|r| r.id)
            .collect())
    }

    /// Full-text + fuzzy search.  Returns scored results with field snippets.
    ///
    /// Query strategy per token
    /// ─────────────────────────
    ///  • Short token (≤ FUZZY_MIN_LEN chars): exact text match + prefix query.
    ///  • Long  token (>  FUZZY_MIN_LEN chars): fuzzy  (edit-distance 1) + prefix.
    ///  • Numeric token: exact F64 match on every non-geo numeric field.
    ///
    /// All clauses for *one* token are `Should`-combined so a document only
    /// needs to match via *any* field.  Tokens themselves are `Must`-combined,
    /// so multi-word queries require every word to appear somewhere in the doc.
    pub fn instant_search(
        &self,
        collection_id: i64,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<InstantResult>, String> {
        let lock = self.collections.read().unwrap();
        let ci = lock.get(&collection_id).ok_or("Index not loaded")?;

        let searcher = ci.reader.searcher();
        let schema = ci.index.schema();

        // ── Categorise fields ───────────────────────────────────────────────

        // Text fields (used for full-text / fuzzy / prefix).
        let text_fields: Vec<Field> = schema
            .fields()
            .filter(|(_, e)| matches!(e.field_type(), TantivyFieldType::Str(_)))
            .map(|(f, _)| f)
            .collect();

        // Numeric fields EXCLUDING geo sub-fields (lat/lng).
        // FIX: geo fields are f64 but should not be hit by arbitrary numeric
        // tokens – they would produce nonsensical results.
        let number_fields: Vec<Field> = schema
            .fields()
            .filter(|(_, e)| {
                let name = e.name();
                matches!(e.field_type(), TantivyFieldType::F64(_))
                    && !name.ends_with("_lat")
                    && !name.ends_with("_lng")
            })
            .map(|(f, _)| f)
            .collect();

        // ── Tokenise query ──────────────────────────────────────────────────

        let trimmed = query_str.trim();
        if trimmed.is_empty() {
            return Ok(vec![]);
        }

        let parser = QueryParser::for_index(&ci.index, text_fields.clone());
        let mut top_clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        for token in trimmed.split_whitespace() {
            let token_lower = token.to_lowercase(); // FIX: normalise for both fuzzy and prefix
            let mut per_token: Vec<(Occur, Box<dyn Query>)> = Vec::new();

            // ── A. Text fields ──────────────────────────────────────────────
            for field in &text_fields {
                let field_name = schema.get_field_name(*field);

                // Fuzzy or exact term depending on token length.
                // FIX: short terms use distance-0 (exact) so they aren't skipped.
                let distance = if token_lower.len() > FUZZY_MIN_LEN {
                    FUZZY_DISTANCE
                } else {
                    0
                };
                let term_val = Term::from_field_text(*field, &token_lower);
                let fuzzy_q = FuzzyTermQuery::new(term_val, distance, true);
                per_token.push((Occur::Should, Box::new(fuzzy_q)));

                // Prefix query – catches partial tokens (e.g. "jo" → "john").
                // FIX: use lowercased token so prefix aligns with tokeniser output.
                let prefix_str = format!("{}:{}*", field_name, token_lower);
                if let Ok(prefix_q) = parser.parse_query(&prefix_str) {
                    per_token.push((Occur::Should, prefix_q));
                }
            }

            // ── B. Numeric fields ───────────────────────────────────────────
            if let Ok(num_val) = token.parse::<f64>() {
                for field in &number_fields {
                    let term_val = Term::from_field_f64(*field, num_val);
                    let exact_q = TermQuery::new(term_val, IndexRecordOption::Basic);
                    per_token.push((Occur::Should, Box::new(exact_q)));
                }
            }

            if !per_token.is_empty() {
                // FIX: use Must at the top level so every token must match
                // *somewhere* – prevents the all-Should / match-nothing trap.
                top_clauses.push((Occur::Must, Box::new(BooleanQuery::new(per_token))));
            }
        }

        if top_clauses.is_empty() {
            return Ok(vec![]);
        }

        let query = BooleanQuery::new(top_clauses);
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| e.to_string())?;

        // ── Assemble results ────────────────────────────────────────────────

        let id_field = schema
            .get_field("record_id")
            .map_err(|_| "record_id missing")?;
        let mut results = Vec::with_capacity(top_docs.len());

        for (score, doc_addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_addr).map_err(|e| e.to_string())?;

            let doc_id = doc
                .get_first(id_field)
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let snippet = Self::build_snippet(&schema, &doc);

            results.push(InstantResult {
                id: doc_id,
                score,
                snippet: JsonValue::Object(snippet),
            });
        }

        Ok(results)
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Open an existing index or create a fresh one.
    ///
    /// On schema mismatch the index is wiped and recreated automatically.
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
                    .map_err(|io| tantivy::TantivyError::IoError(Arc::new(io)))?;
                let new_dir = MmapDirectory::open(index_path)?;
                Index::create(new_dir, schema, tantivy::IndexSettings::default())
            }
            Err(e) => Err(e),
        }
    }

    /// Build a `TantivyDocument` from a JSON record, applying correct type
    /// coercions for each field defined in the `CollectionSchema`.
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
                FieldType::GeoPoint => {
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
                        FieldType::Number => {
                            if let Some(n) = val.as_f64() {
                                doc.add_f64(field, n);
                            }
                        }
                        FieldType::Boolean => {
                            if let Some(b) = val.as_bool() {
                                doc.add_u64(field, u64::from(b));
                            }
                        }
                        _ => {
                            // Stringify whatever we received.
                            let text = if let Some(s) = val.as_str() {
                                s.to_string()
                            } else if val.is_null() {
                                return doc; // skip null text fields
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

    /// Extract field values from a retrieved `TantivyDocument` into a JSON map
    /// suitable for returning as a result snippet.
    fn build_snippet(schema: &Schema, doc: &TantivyDocument) -> Map<String, JsonValue> {
        let mut map = Map::new();

        for (field, entry) in schema.fields() {
            let name = entry.name();
            if name == "record_id" {
                continue;
            }
            let Some(val) = doc.get_first(field) else {
                continue;
            };

            // Map each Tantivy value type to its JSON equivalent.
            let json_val = if let Some(s) = val.as_str() {
                JsonValue::String(s.to_string())
            } else if let Some(f) = val.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            } else if let Some(u) = val.as_u64() {
                JsonValue::Number(serde_json::Number::from(u))
            } else if let Some(i) = val.as_i64() {
                JsonValue::Number(serde_json::Number::from(i))
            } else {
                continue;
            };

            map.insert(name.to_string(), json_val);
        }

        map
    }
}
