use crate::models::schema::{CollectionSchema, FieldType};
use tantivy::schema::{
    FAST, INDEXED, IndexRecordOption, STORED, Schema as TantivySchema, TextFieldIndexing,
    TextOptions,
};

/// Build a Tantivy `Schema` from our app-level `CollectionSchema`.
///
/// Field ordering is made deterministic (sorted by name) so that two builds
/// of the "same" schema always produce byte-identical Tantivy schemas — this
/// matters because `open_or_recreate` relies on schema equality checks to
/// detect drift.
pub fn build_tantivy_schema(collection_schema: &CollectionSchema) -> TantivySchema {
    let mut builder = TantivySchema::builder();

    // Internal record-id field – stored (so we can retrieve it in results),
    // indexed (so we can term-query on it for delete_term), and fast
    // (so deletes/lookups by ID are efficient via the fast-field columnar storage).
    builder.add_i64_field("record_id", STORED | INDEXED | FAST);

    // Sort fields by name for deterministic schema fingerprinting across
    // restarts/rebuilds, regardless of HashMap iteration order.
    let mut sorted: Vec<_> = collection_schema.fields.iter().collect();
    sorted.sort_by_key(|(name, _)| *name);

    for (name, def) in sorted {
        // Only fields flagged for search get a corresponding Tantivy field.
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
                // Stored as two separate F64 sub-fields rather than a single
                // composite geo field, mirroring build_document's handling.
                builder.add_f64_field(&format!("{}_lat", name), STORED | INDEXED | FAST);
                builder.add_f64_field(&format!("{}_lng", name), STORED | INDEXED | FAST);
            }
            _ => {
                // [FIX] Changed tokenizer from "default" to "en_stem" for Meilisearch-like stemming.
                // "en_stem" applies English stemming (e.g. "running" -> "run")
                // so queries match morphological variants of indexed words.
                let text_indexing = TextFieldIndexing::default()
                    .set_tokenizer("en_stem")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions);
                let options = TextOptions::default()
                    .set_indexing_options(text_indexing)
                    .set_stored();
                builder.add_text_field(name, options);
            }
        }
    }

    builder.build()
}
