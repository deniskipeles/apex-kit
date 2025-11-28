use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Script {
    pub id: i64,
    pub name: String,         // Unique Identifier e.g. "send-slack-notification"
    pub trigger_type: String, // "manual", "before_create", "after_create", etc.
    pub code: String,         // The JavaScript code
    pub active: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateScriptReq {
    pub name: String,
    pub trigger_type: String,
    pub code: String,
}
