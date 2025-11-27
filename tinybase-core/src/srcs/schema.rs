use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CollectionSchema {
    pub fields: HashMap<String, FieldDefinition>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FieldDefinition {
    pub r#type: FieldType,
    pub required: bool,
    pub default: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Text,
    Number,
    Boolean,
    Json,
}
