// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/models.rs ===========================
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use serde_json::Value;
use crate::schema::CollectionSchema;

/// Represents a generic Data Record (JSON document)
#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Record {
    #[serde(skip_deserializing)] // ID is handled by DB on create
    pub id: Option<i64>, 
    #[schema(value_type = Object)]
    pub data: Value,
}

/// Represents a Collection definition
#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Collection {
    #[serde(skip_deserializing)]
    pub id: Option<i64>,
    pub name: String,
    pub schema: Option<CollectionSchema>,
}

/// Represents a File uploaded to Storage
#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct StoredFile {
    pub id: i64,
    pub filename: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: i64,
    pub created_at: String,
}

/// Represents a Scheduled Background Task
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: String, // e.g. "0 0 * * *"
    pub payload: String,  // URL path or command identifier
    pub active: bool,
}

/// Represents a lightweight search hit (from Tantivy Index)
#[derive(Serialize, ToSchema, Clone, Debug)]
pub struct InstantResult {
    pub id: i64,
    pub score: f32, // Relevance score
    #[schema(value_type = Object)]
    pub snippet: serde_json::Value, // Stored fields only (Title, Name, etc.)
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/models.rs ends here ===========================