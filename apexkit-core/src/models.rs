use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use serde_json::Value;
use crate::schema::CollectionSchema;


// --- ADD NEW MODELS ---

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DashboardStats {
    pub total_requests: i64, // Based on audit logs count
    pub db_size_mb: f64,
    pub collections_count: i64,
    pub total_records: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)] // Added Clone
pub struct ChartPoint {
    pub name: String,
    pub requests: i64,
    pub errors: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DashboardData {
    pub stats: DashboardStats,
    pub chart: Vec<ChartPoint>,
    pub recent_logs: Vec<serde_json::Value>,
}

/// Represents a generic Data Record (JSON document)
#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Record {
    #[serde(skip_deserializing)] // ID is handled by DB on create
    pub id: Option<i64>, 
    #[schema(value_type = Object)]
    pub data: Value,
    // Skip deserializing timestamps (Client doesn't send them)
    // Use default value ("") when reading from JSON payload
    #[serde(skip_deserializing)]
    #[serde(default)] 
    pub created: String,
    #[serde(skip_deserializing)]
    #[serde(default)]
    pub updated: String,
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

// Templates
#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Template {
    pub id: i64,
    pub slug: String,         // e.g. "chat-room" or "components/like-button"
    pub content: String,      // The HTML/Tera code
    pub script_id: Option<i64>, // The script that provides data for this template
    pub created_at: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateTemplateReq {
    pub slug: String,
    pub content: String,
    pub script_id: Option<i64>,
}

// --- APP MANIFEST (AI Architect) ---

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct AppManifest {
    pub app_name: String,
    pub collections: Vec<ManifestCollection>,
    pub scripts: Vec<ManifestScript>,
    pub templates: Vec<ManifestTemplate>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ManifestCollection {
    pub name: String,
    pub schema: CollectionSchema,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ManifestScript {
    pub name: String,       // e.g. "create-task"
    pub trigger_type: String, // "manual", "before_create", etc.
    pub code: String,       // JS Code
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ManifestTemplate {
    pub slug: String,       // e.g. "dashboard/tasks"
    pub content: String,    // HTML + Tera
    pub loader_script: Option<String>, // Name of script to run before rendering
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct ConfigItem {
    pub key: String,
    pub value: Option<String>, // Masked if encrypted, otherwise the raw string
    pub encrypted: bool,
    pub updated_at: String,
}