use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Script {
    pub id: i64,
    pub name: String,
    // e.g., "manual", "before_create", "after_create", "before_update", "after_update", "before_delete", "after_delete"
    pub trigger_type: String,
    // New: If set, only runs for this collection. If None, runs for all (global hook).
    pub target_collection: Option<String>,
    pub code: String,
    pub active: bool,
    //  'private' (default) or 'public' (shared with tenants)
    #[serde(default)]
    pub visibility: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateScriptReq {
    pub name: String,
    pub trigger_type: String,
    pub target_collection: Option<String>, // Added
    pub code: String,
    pub active: bool,
    #[serde(default = "default_visibility")]
    pub visibility: String,
}

fn default_visibility() -> String {
    "private".to_string()
}
