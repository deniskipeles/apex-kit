use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct AiAction {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub template: String,
    #[schema(value_type = Object)]
    pub config: serde_json::Value, // For temp, top_k, etc.
}

#[derive(Deserialize, ToSchema)]
pub struct CreateActionReq {
    pub slug: String,
    pub name: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub template: String,
}