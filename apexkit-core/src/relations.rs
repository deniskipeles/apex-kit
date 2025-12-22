use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub id: i64,
    pub origin_collection_id: i64,
    pub origin_record_id: i64,
    pub target_collection_id: i64,
    pub target_record_id: i64,
    pub rel_name: String, // e.g., "author", "comments"
    pub properties: Option<Value>, // Metadata: e.g. {"role": "editor"}
}

/// Helper to merge expanded relations into the main record data
pub fn merge_expansion(original_data: &mut Value, relation_name: &str, related_records: Vec<Value>) {
    if let Some(obj) = original_data.as_object_mut() {
        let expand = obj.entry("expand").or_insert(json!({}));
        
        if let Some(expand_obj) = expand.as_object_mut() {
            // Always overwrite with latest data
            expand_obj.insert(relation_name.to_string(), json!(related_records));
        }
    }
}