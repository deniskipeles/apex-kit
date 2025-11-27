use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tinybase_core::{auth::Claims, ai_models::{AiAction, CreateActionReq}};
use crate::{AppState, AppError, settings::AiConfigDto};
use regex::Regex;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ExecutePromptReq {
    pub variables: Value, // { "text": "hello" }
}

// --- ADMIN MANAGEMENT ---

pub async fn list_actions(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>
) -> Result<Json<Vec<AiAction>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let actions = state.db.list_ai_actions().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(actions))
}

pub async fn create_action(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Json(payload): Json<CreateActionReq>
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let id = state.db.create_ai_action(payload).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

pub async fn delete_action(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Path(id): Path<i64>
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    state.db.delete_ai_action(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "success": true })))
}

// --- EXECUTION (PUBLIC/AUTH) ---

pub async fn run_action(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(payload): Json<ExecutePromptReq>,
) -> Result<Json<Value>, AppError> {
    
    // 1. Get Action Config
    let action = state.db.get_ai_action(&slug).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Action not found".into()))?;

    // 2. Get API Key from Settings
    let ai_settings_json = state.db.get_setting("ai").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let ai_config: AiConfigDto = if let Some(val) = ai_settings_json {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        return Err(AppError::UnknownError("AI not configured".into()));
    };

    if !ai_config.enabled {
        return Err(AppError::Forbidden("AI features disabled".into()));
    }

    let api_key_enc = ai_config.api_key.ok_or(AppError::UnknownError("API Key missing".into()))?;
    // Decrypt the key (assuming it was stored as encrypted string JSON)
    let api_key = state.vault.decrypt(&api_key_enc).map_err(|_| AppError::UnknownError("Failed to decrypt API Key".into()))?;

    // 3. Template Substitution
    // Replace {{key}} with value from payload.variables
    let mut final_prompt = action.template.clone();
    let re = Regex::new(r"\{\{(\w+)\}\}").unwrap(); // Matches {{word}}
    
    final_prompt = re.replace_all(&final_prompt, |caps: &regex::Captures| {
        let key = &caps[1];
        // Look up key in variables
        payload.variables.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("") // Replace missing vars with empty string
            .to_string()
    }).to_string();

    // 4. Call Gemini API
    let client = reqwest::Client::new();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        action.model, api_key
    );

    // Construct Gemini Body
    let body = json!({
        "contents": [{
            "parts": [{ "text": final_prompt }]
        }]
    });

    let res = client.post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::UnknownError(format!("Gemini Req Failed: {}", e)))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        return Err(AppError::UnknownError(format!("Gemini API Error: {}", err_text)));
    }

    let response_json: Value = res.json().await.map_err(|_| AppError::JsonError("Invalid response".into()))?;
    
    // Extract text from Gemini response structure
    let text = response_json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(Json(json!({ "result": text })))
}