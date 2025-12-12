use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tinybase_core::{
    auth::Claims, 
    ai_models::{AiAction, CreateActionReq},
    security::EncryptedValue
};
use crate::{AppState, AppError, settings::AiConfigDto};
use regex::Regex;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ExecutePromptReq {
    pub variables: Value, // { "text": "...", "image": "data:image/png;base64,..." }
}

// Helper to parse "data:image/png;base64,ABC..." into ("image/png", "ABC...")
fn parse_data_uri(uri: &str) -> Option<(String, String)> {
    if uri.starts_with("data:") {
        let parts: Vec<&str> = uri.splitn(2, ',').collect();
        if parts.len() == 2 {
            let meta = parts[0]; // "data:image/png;base64"
            let data = parts[1];
            
            let mime_parts: Vec<&str> = meta.split(';').collect();
            if let Some(mime_raw) = mime_parts.first() {
                let mime = mime_raw.trim_start_matches("data:").to_string();
                return Some((mime, data.to_string()));
            }
        }
    }
    None
}

// --- ADMIN MANAGEMENT ---

#[utoipa::path(
    get,
    path = "/api/v1/admin/ai/actions",
    responses((status = 200, body = Vec<AiAction>))
)]
pub async fn list_actions(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>
) -> Result<Json<Vec<AiAction>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let actions = state.db.list_ai_actions().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(actions))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/ai/actions",
    request_body = CreateActionReq,
    responses((status = 200, body = Value))
)]
pub async fn create_action(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Json(payload): Json<CreateActionReq>
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let id = state.db.create_ai_action(payload).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/ai/actions/{id}",
    responses((status = 200, body = Value))
)]
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

// --- EXECUTION (PUBLIC/AUTH) - UPDATED FOR MULTIMODAL ---

#[utoipa::path(
    post,
    path = "/api/v1/ai/run/{slug}",
    request_body = ExecutePromptReq,
    responses((status = 200, body = Value))
)]
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

    let api_key_str = ai_config.api_key.ok_or(AppError::UnknownError("API Key missing".into()))?;
    
    let encrypted_val: EncryptedValue = serde_json::from_str(&api_key_str)
        .map_err(|_| AppError::UnknownError("Invalid encrypted key format".into()))?;
        
    let api_key = state.vault.decrypt(&encrypted_val)
        .map_err(|_| AppError::UnknownError("Failed to decrypt API Key".into()))?;

    // 3. Construct Request Parts (Multimodal Support)
    let mut content_parts = Vec::new();

    // A. Handle Text Template
    let mut final_prompt = action.template.clone();
    let re = Regex::new(r"\{\{(\w+)\}\}").unwrap(); 
    
    final_prompt = re.replace_all(&final_prompt, |caps: &regex::Captures| {
        let key = &caps[1];
        // Only replace text variables here
        payload.variables.get(key)
            .and_then(|v| v.as_str())
            // Ignore data URIs in text replacement to avoid massive logs/errors
            .filter(|s| !s.starts_with("data:")) 
            .unwrap_or("") 
            .to_string()
    }).to_string();

    if !final_prompt.trim().is_empty() {
        content_parts.push(json!({ "text": final_prompt }));
    }

    // B. Handle Image Inputs (e.g. for Editing or Vision)
    // Look for variables that contain Data URIs (base64 images)
    if let Some(obj) = payload.variables.as_object() {
        for (_key, value) in obj {
            if let Some(str_val) = value.as_str() {
                if let Some((mime, data)) = parse_data_uri(str_val) {
                    content_parts.push(json!({
                        "inline_data": {
                            "mime_type": mime,
                            "data": data
                        }
                    }));
                }
            }
        }
    }

    // 4. Construct Gemini API Request Body
    let client = reqwest::Client::new();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        action.model, api_key
    );

    let mut request_body = json!({
        "contents": [{
            "parts": content_parts
        }]
    });

    // Add System Instructions if present
    if let Some(sys_prompt) = action.system_prompt {
        if !sys_prompt.trim().is_empty() {
            request_body["system_instruction"] = json!({
                "parts": [{ "text": sys_prompt }]
            });
        }
    }

    // 5. Execute Request
    let res = client.post(url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| AppError::UnknownError(format!("Gemini Req Failed: {}", e)))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        return Err(AppError::UnknownError(format!("Gemini API Error: {}", err_text)));
    }

    let response_json: Value = res.json().await.map_err(|_| AppError::JsonError("Invalid response".into()))?;
    
    // 6. Parse Response (Handle Text OR Image Output)
    let candidate = &response_json["candidates"][0]["content"]["parts"][0];
    
    let result = if let Some(text) = candidate["text"].as_str() {
        // Text response
        text.to_string()
    } else if let Some(inline_data) = candidate["inline_data"].as_object() {
        // Image response (Base64)
        let mime = inline_data["mime_type"].as_str().unwrap_or("image/png");
        let data = inline_data["data"].as_str().unwrap_or("");
        format!("data:{};base64,{}", mime, data)
    } else {
        return Err(AppError::UnknownError("Unsupported Gemini response format".into()));
    };

    // Extract Metadata (Grounding/Search results)
    let metadata = response_json["candidates"][0]["groundingMetadata"].clone();

    Ok(Json(json!({ 
        "result": result,
        "metadata": metadata
    })))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CodeEditReq {
    pub prompt: String,
    pub current_code: String,
    pub context_type: String,  // "script" or "template"
    pub model: String, //  Model Field
}

#[utoipa::path(
    post,
    path = "/api/v1/ai/edit-code",
    request_body = CodeEditReq,
    responses((status = 200, body = Value))
)]
pub async fn edit_code(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Json(req): Json<CodeEditReq>,
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // 1. Get Config
    // Reuse the get_ai_config logic or duplicate it here for now
    let ai_settings = state.db.get_setting("ai").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let (api_key, model) = match ai_settings {
        Some(val) => {
            let conf: crate::settings::AiConfigDto = serde_json::from_value(val).unwrap_or_default();
            if !conf.enabled { return Err(AppError::Forbidden("AI disabled".into())); }
            let raw = conf.api_key.ok_or(AppError::UnknownError("AI Key missing".into()))?;
            let enc: tinybase_core::security::EncryptedValue = serde_json::from_str(&raw).map_err(|_| AppError::UnknownError("Bad key".into()))?;
            let key = state.vault.decrypt(&enc).map_err(|_| AppError::UnknownError("Decrypt fail".into()))?;
            let modl = conf.model.unwrap_or("gemini-2.5-flash".to_string());
            (key, modl)
        },
        None => return Err(AppError::UnknownError("AI not configured".into()))
    };

    // 2. Construct Prompt
    let system_context = if req.context_type == "script" {
        "You are a JavaScript expert for the TinyBase runtime. Globals available: $db, $http, console. Return ONLY the updated code code."
    } else {
        "You are an HTML/Tera expert. Use Tailwind CSS. Return ONLY the updated HTML code."
    };

    let full_prompt = format!(
        "{}\n\nExisting Code:\n```\n{}\n```\n\nUser Instruction: {}\n\nOutput only the code, no markdown fencing.",
        system_context, req.current_code, req.prompt
    );

    // 3. Call LLM (Standard Text generation, not JSON mode)
    // Call LLM using req.model
    let client = reqwest::Client::new();
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}", req.model, api_key);
    
    let body = json!({ 
        "contents": [{ "role": "user", "parts": [{ "text": full_prompt }] }]
    });

    let res = client.post(url).json(&body).send().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let res_json: Value = res.json().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let mut code = res_json["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or("").to_string();
    
    // Cleanup markdown if AI ignores instructions
    code = code.trim().trim_start_matches("```javascript").trim_start_matches("```html").trim_start_matches("```").trim_end_matches("```").to_string();

    Ok(Json(json!({ "code": code })))
}