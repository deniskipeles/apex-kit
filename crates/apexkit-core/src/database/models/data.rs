use super::schema::CollectionSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DashboardStats {
    pub total_requests: i64,
    pub db_size_mb: f64,
    pub collections_count: i64,
    pub total_records: i64,
    pub total_vectors: i64,
    pub indexes_size_mb: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
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

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Record {
    #[serde(skip_deserializing)]
    pub id: i64,
    #[schema(value_type = Object)]
    pub data: Value,
    #[serde(skip_deserializing)]
    #[serde(default)]
    pub created: String,
    #[serde(skip_deserializing)]
    #[serde(default)]
    pub updated: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand: Option<Value>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Collection {
    #[serde(skip_deserializing)]
    pub id: i64,
    pub name: String,
    pub schema: Option<CollectionSchema>,
    #[serde(default)]
    pub index: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ListResult {
    pub items: Vec<Record>,
    pub total: i64,
}

#[derive(Clone, Debug)]
pub struct ChangesetEvent {
    pub scope: String,
    pub db_name: String,
    pub changeset: Vec<u8>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct StoredFile {
    pub id: i64,
    pub filename: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub payload: String,
    pub active: bool,
}

#[derive(Serialize, ToSchema, Clone, Debug)]
pub struct InstantResult {
    pub id: i64,
    pub score: f32,
    #[schema(value_type = Object)]
    pub snippet: serde_json::Value,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Template {
    pub id: i64,
    pub slug: String,
    pub content: String,
    pub script_id: Option<i64>,
    pub created_at: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateTemplateReq {
    pub slug: String,
    pub content: String,
    pub script_id: Option<i64>,
}

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
    pub name: String,
    pub trigger_type: String,
    pub code: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ManifestTemplate {
    pub slug: String,
    pub content: String,
    pub loader_script: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct ConfigItem {
    pub key: String,
    pub value: Option<String>,
    pub encrypted: bool,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct VectorRecord {
    pub field_name: String,
    pub vector: Vec<f32>,
    pub model: String,
}

// --- NEW COMPOSITE API KEY MODEL ---
#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ApiKey {
    pub id: i64,
    pub name: String,
    pub tenant_id: String,
    pub key_id: String,
    pub issuer: String,
    pub env_type: String, // 'sys', 'tnnt', 'sk', 'pk'
    pub roles: Vec<String>,
    pub status: String,
    pub bypass_cors: bool,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Tenant {
    pub id: String,
    pub name: Option<String>,
    pub status: String,
    pub tier: String,
    pub stats: TenantStats,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct TenantStats {
    pub storage_mb: f64,
    pub max_storage_mb: i64,
    pub vector_count: i64,
    pub max_vectors: i64,
    pub ai_requests: i64,
    pub max_ai_requests: i64,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema, Clone, Debug)]
pub struct SandboxMetadata {
    pub id: String,
    pub name: Option<String>,
    pub status: String,
    pub expires_at: Option<String>,
    pub scope: String,
    pub tenant_id: Option<String>,
    pub current_storage_mb: f64,
    pub max_storage_mb: i64,
}
