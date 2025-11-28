// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/schema.rs ===========================
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema, Default)]
pub struct CollectionSchema {
    pub fields: HashMap<String, FieldDefinition>,
    #[serde(default)]
    pub policies: CollectionPolicies,
    #[serde(default)]
    pub relations: HashMap<String, RelationDefinition>,
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct RelationDefinition {
    pub target_collection: String,
    pub relation_type: RelationType,
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RelationType {
    One,
    Many,
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct CollectionPolicies {
    pub read: String,
    pub create: String,
    pub update: String,
    pub delete: String,
}

impl Default for CollectionPolicies {
    fn default() -> Self {
        Self {
            read: "public".to_string(),
            create: "auth".to_string(),
            update: "admin".to_string(),
            delete: "admin".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct FieldDefinition {
    pub r#type: FieldType,
    pub required: bool,
    #[schema(value_type = Option<Object>)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub indexed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Text,
    Number,
    Boolean,
    Json,
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/schema.rs ends here ===========================