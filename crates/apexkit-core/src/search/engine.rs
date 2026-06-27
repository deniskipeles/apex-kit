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

/// Heap size (in bytes) allocated to each Tantivy `IndexWriter`.
/// 50MB is a reasonable default for moderate-sized collections; increase
/// if you start seeing frequent forced segment flushes under heavy write load.
pub const WRITER_HEAP_BYTES: usize = 50_000_000;

/// Minimum token length before we bother running a fuzzy (typo-tolerant) match.
/// Short words (e.g. "in", "is", "it") produce too many false positives under
/// edit-distance-1 fuzzy matching, so we gate fuzzy matching behind this length.
pub const FUZZY_MIN_LEN: usize = 4;

/// Maximum Levenshtein edit distance allowed for fuzzy term matching.
pub const FUZZY_DISTANCE: u8 = 1;

/// Bundles together everything needed to interact with a single collection's
/// on-disk Tantivy index: the `Index` handle itself, a long-lived `IndexReader`
/// (which Tantivy uses to serve searches against a consistent snapshot), and
/// the `IndexWriter` used for all mutations (add/delete/commit).
pub struct CollectionIndex {
    pub index: Index,
    pub reader: IndexReader,
    pub writer: IndexWriter,
}

/// Top-level manager that owns all per-collection Tantivy indexes for the
/// whole application. Indexes are lazily loaded into the `collections` map
/// on first use (see `load_index`) and kept resident afterward.
///
/// `RwLock<HashMap<...>>` was chosen over per-collection locks so that:
///   - Many concurrent readers (searches) can proceed without blocking each other.
///   - Mutations (index/delete) take a write lock only for the duration of the
///     map lookup + write, not for the whole search lifecycle.
pub struct SearchManager {
    /// Root directory under which each collection gets its own subdirectory,
    /// named after its `collection_id`.
    pub base_path: PathBuf,
    /// In-memory registry of currently loaded collection indexes, keyed by
    /// collection ID. Protected by RwLock for thread-safe concurrent access.
    pub collections: Arc<RwLock<HashMap<i64, CollectionIndex>>>,
}

impl SearchManager {
    /// Construct a new `SearchManager` rooted at `path`, creating the directory
    /// if it doesn't already exist. Does NOT eagerly load any indexes — that
    /// happens lazily via `load_index`.
    pub fn new(path: &str) -> Self {
        let base_path = PathBuf::from(path);
        fs::create_dir_all(&base_path).expect("Failed to create search base directory");
        Self {
            base_path,
            collections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Ensure the index for `collection_id` is loaded into memory, opening it
    /// from disk (or creating it fresh) if necessary.
    ///
    /// This is idempotent — if the index is already loaded, it's a cheap
    /// read-lock check and immediate return ("fast path").
    pub fn load_index(&self, collection_id: i64, schema: &CollectionSchema) -> Result<(), String> {
        // Fast path: take a read lock only, so concurrent calls to load_index
        // for already-loaded collections don't contend with each other.
        if self
            .collections
            .read()
            .unwrap()
            .contains_key(&collection_id)
        {
            return Ok(());
        }

        // Not loaded yet — figure out where this collection's index lives on disk
        // and make sure the directory exists.
        let index_path = self.base_path.join(collection_id.to_string());
        fs::create_dir_all(&index_path).map_err(|e| e.to_string())?;

        // Translate our app-level CollectionSchema into a Tantivy Schema.
        let tantivy_schema = build_tantivy_schema(schema);

        // Open the existing on-disk index, or create a new one. This also
        // transparently handles the case where the on-disk schema doesn't
        // match what we expect anymore (see open_or_recreate below).
        let index = Self::open_or_recreate(&index_path, tantivy_schema).map_err(|e| {
            format!(
                "Failed to open index for collection {}: {}",
                collection_id, e
            )
        })?;

        // Build a reader that automatically picks up new commits shortly after
        // they happen (rather than requiring a manual reader.reload() call).
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| e.to_string())?;

        // Each collection gets its own dedicated writer instance.
        let writer = index.writer(WRITER_HEAP_BYTES).map_err(|e| e.to_string())?;

        // Register this newly-opened index/reader/writer trio in the shared map
        // under a write lock, so subsequent calls hit the fast path above.
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

    /// Fully remove a collection's search index: drop it from the in-memory
    /// map and delete its on-disk directory. Used when a collection itself
    /// is deleted from the app.
    pub fn delete_index(&self, collection_id: i64) -> Result<(), String> {
        // Drop the in-memory handle first so nothing else can touch it
        // while we're deleting the underlying files.
        self.collections.write().unwrap().remove(&collection_id);

        let index_path = self.base_path.join(collection_id.to_string());
        if index_path.exists() {
            fs::remove_dir_all(&index_path)
                .map_err(|e| format!("Failed to delete index dir: {}", e))?;
        }
        Ok(())
    }

    /// Return the number of documents currently visible to readers for a
    /// given collection. Returns 0 (rather than an error) if the collection
    /// isn't loaded — treating "not loaded" as "empty" for callers that just
    /// want a count.
    pub fn get_doc_count(&self, collection_id: i64) -> Result<u64, String> {
        let lock = self.collections.read().unwrap();
        match lock.get(&collection_id) {
            Some(ci) => Ok(ci.reader.searcher().num_docs()),
            None => Ok(0),
        }
    }

    /// Index (or re-index) a single record.
    ///
    /// We always delete any existing document with the same record_id before
    /// adding the new one — Tantivy doesn't support in-place updates, so this
    /// delete+add is the standard "upsert" pattern.
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

        // Remove any prior version of this record (upsert semantics).
        ci.writer
            .delete_term(Term::from_field_i64(id_field, record_id));

        // Build the new Tantivy document from the JSON payload and add it.
        let doc = Self::build_document(&ci.index.schema(), record_id, data, schema);
        ci.writer.add_document(doc).map_err(|e| e.to_string())?;

        // Commit immediately so the change is visible to readers (subject to
        // the reader's reload policy/delay).
        ci.writer.commit().map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Bulk version of `index_record`: index multiple records in one writer
    /// transaction, committing only once at the end. This is significantly
    /// faster than calling `index_record` in a loop because commits are
    /// relatively expensive (they flush segments to disk).
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

        // Same upsert pattern as index_record, but batched: delete+add for
        // every record, then a single commit at the very end.
        for (record_id, data) in records {
            ci.writer
                .delete_term(Term::from_field_i64(id_field, *record_id));
            let doc = Self::build_document(&index_schema, *record_id, data, schema);
            ci.writer.add_document(doc).map_err(|e| e.to_string())?;
        }

        ci.writer.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Delete a single record from the index by its record_id. No-op (not an
    /// error) if the collection isn't currently loaded.
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

    /// Open the Tantivy index at `index_path`, or create it if it doesn't
    /// exist yet. Critically, this also handles **schema drift**: if the
    /// on-disk index was built with a different schema than the one we're
    /// asking for now (e.g. the user added/removed a field), Tantivy's
    /// `Index::open_or_create` will fail with a "schema does not match" error.
    /// In that case we nuke the old index directory and rebuild from scratch
    /// with the new schema, rather than propagating the error up.
    ///
    /// NOTE: this means schema changes cause full re-indexing — there's no
    /// migration path, just rebuild-on-mismatch.
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
                // Wipe the stale index directory entirely...
                let _ = fs::remove_dir_all(index_path);
                fs::create_dir_all(index_path)
                    .map_err(|io| tantivy::TantivyError::IoError(std::sync::Arc::new(io)))?;
                // ...and recreate it fresh with the current schema.
                let new_dir = MmapDirectory::open(index_path)?;
                Index::create(new_dir, schema, tantivy::IndexSettings::default())
            }
            // Any other error (permissions, corruption, etc.) is propagated as-is.
            Err(e) => Err(e),
        }
    }

    /// Convert a JSON record (`data`) into a `TantivyDocument` ready for
    /// indexing, according to the field definitions in `collection_schema`.
    ///
    /// Only fields marked `ose_indexed` in the collection schema are added —
    /// fields not flagged for search indexing are silently skipped, even if
    /// present in `data`.
    fn build_document(
        index_schema: &Schema,
        record_id: i64,
        data: &JsonValue,
        collection_schema: &CollectionSchema,
    ) -> TantivyDocument {
        let mut doc = TantivyDocument::default();

        // Every document carries its app-level record_id so we can map
        // search hits back to the original record.
        let id_field = index_schema
            .get_field("record_id")
            .expect("record_id must exist");
        doc.add_i64(id_field, record_id);

        for (name, def) in &collection_schema.fields {
            // Skip fields not opted into search indexing.
            if !def.ose_indexed {
                continue;
            }

            match def.r#type {
                // GeoPoint fields are special-cased: they're stored as two
                // separate scalar sub-fields (`{name}_lat` / `{name}_lng`)
                // rather than a single Tantivy field, since Tantivy has no
                // native geo type we're using here.
                crate::models::schema::FieldType::GeoPoint => {
                    let Some(obj) = data.get(name).and_then(|v| v.as_object()) else {
                        continue;
                    };
                    let lat = obj.get("lat").and_then(|v| v.as_f64());
                    // Accept either "lng" or "lon" as the longitude key, to
                    // be tolerant of different naming conventions in input data.
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
                // All other field types map directly onto a single Tantivy field.
                _ => {
                    let Ok(field) = index_schema.get_field(name) else {
                        continue;
                    };
                    let Some(val) = data.get(name) else {
                        continue;
                    };

                    match def.r#type {
                        // Numbers go in as f64 regardless of whether they were
                        // ints or floats in the source JSON.
                        crate::models::schema::FieldType::Number => {
                            if let Some(n) = val.as_f64() {
                                doc.add_f64(field, n);
                            }
                        }
                        // Booleans are stored as u64 0/1 since Tantivy has no
                        // dedicated boolean field type.
                        crate::models::schema::FieldType::Boolean => {
                            if let Some(b) = val.as_bool() {
                                doc.add_u64(field, u64::from(b));
                            }
                        }
                        // Everything else (strings, and any type we don't
                        // special-case) is treated as text.
                        _ => {
                            let text = if let Some(s) = val.as_str() {
                                s.to_string()
                            } else if val.is_null() {
                                // Null value: bail out of building this doc
                                // entirely rather than skipping just this field.
                                // NOTE: this early-returns the whole document,
                                // discarding any fields not yet processed —
                                // preserved as-is per "do not change logic".
                                return doc;
                            } else {
                                // Non-string, non-null JSON value (e.g. an
                                // object or array) — stringify it so it's at
                                // least searchable as text.
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