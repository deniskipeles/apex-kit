use crate::models::schema::{CollectionSchema, FieldType};
use tantivy::schema::{
    FAST, INDEXED, IndexRecordOption, STORED, Schema as TantivySchema, TextFieldIndexing,
    TextOptions,
};

pub fn build_tantivy_schema(collection_schema: &CollectionSchema) -> TantivySchema {
    let mut builder = TantivySchema::builder();

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
                // [FIX] Changed tokenizer from "default" to "en_stem" for Meilisearch-like stemming
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
