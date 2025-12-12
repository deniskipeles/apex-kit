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

use crate::models::AppManifest;

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct AiSession {
    pub id: String, // UUID
    pub name: String, // "Todo App Project"
    pub messages: Vec<ChatMessage>,
    pub current_manifest: Option<AppManifest>,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct ChatMessage {
    pub role: String, // "user" | "assistant" | "system"
    pub content: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub manifest: AppManifest,
    pub description: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateSessionReq {
    pub name: String,
    pub initial_prompt: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ChatReq {
    pub prompt: String,
    pub model: Option<String>,
}