use crate::BaseUrl;
use crate::{AppError, AppState, DatabaseConnection, system::dto::AiConfigDto};
use crate::{
    hooks::{trigger_filter_hook, trigger_void_hook},
    utils::extract_log_meta,
};
use apexkit_core::realtime::EventScope;
use apexkit_core::{
    auth::Claims,
    models::ai::{AiAction, CreateActionReq},
    security::vault::EncryptedValue,
};
use axum::extract::ConnectInfo;
use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use axum::{
    Extension,
    extract::{Json, Path, State},
    http::StatusCode,
};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::net::SocketAddr;
use tokio::io::AsyncBufReadExt;
use utoipa::{IntoParams, ToSchema};

#[derive(Deserialize, ToSchema)]
pub struct ExecutePromptReq {
    pub variables: Value, // { "text": "...", "image": "data:image/png;base64,..." }
}

#[derive(Deserialize, IntoParams)]
pub struct RunActionPath {
    pub slug: String,
}

#[derive(Deserialize, IntoParams)]
pub struct DelActionPath {
    pub id: i64,
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
    DatabaseConnection(db): DatabaseConnection,
    State(_state): State<AppState>,
) -> Result<Json<Vec<AiAction>>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    let actions = db
        .list_ai_actions()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
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
    DatabaseConnection(db): DatabaseConnection,
    State(_state): State<AppState>,
    Json(payload): Json<CreateActionReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    let id = db
        .create_ai_action(payload)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/ai/actions/{id}",
    params(DelActionPath),
    responses((status = 200, body = Value))
)]
pub async fn delete_action(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    State(_state): State<AppState>,
    Path(path): Path<DelActionPath>,
) -> Result<Json<serde_json::Value>, AppError> {
    let id = path.id;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    db.delete_ai_action(id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "success": true })))
}

// --- EXECUTION (PUBLIC/AUTH) ---

#[utoipa::path(
    post,
    path = "/api/v1/ai/run/{slug}",
    request_body = ExecutePromptReq,
    params(RunActionPath),
    responses((status = 200, body = Value))
)]
pub async fn run_action(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    Path(path): Path<RunActionPath>,
    Json(payload): Json<ExecutePromptReq>,
) -> Result<axum::response::Response, AppError> {
    let slug = path.slug;
    let claims = auth.map(|Extension(c)| c);
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);

    // 1. Get Action Config (From Tenant/Sandbox DB)
    let action = db
        .get_ai_action(&slug)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Action not found".into()))?;

    // 2. Get API Key from Settings (From Tenant/Sandbox DB)
    let ai_settings_json = db
        .get_config("ai")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let ai_config: AiConfigDto = if let Some(val) = ai_settings_json {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        return Err(AppError::UnknownError("AI not configured".into()));
    };

    if !ai_config.enabled {
        return Err(AppError::Forbidden("AI features disabled".into()));
    }

    let api_key_str = ai_config
        .api_key
        .ok_or(AppError::UnknownError("API Key missing".into()))?;

    // Decrypt using Global Vault
    let encrypted_val: EncryptedValue = serde_json::from_str(&api_key_str)
        .map_err(|_| AppError::UnknownError("Invalid encrypted key format".into()))?;

    let api_key = state
        .vault
        .decrypt(&encrypted_val)
        .map_err(|_| AppError::UnknownError("Failed to decrypt API Key".into()))?;

    // 3. Resolve Config Variables safely from Option<serde_json::Value>
    let config = action.config.as_ref();
    let provider = config
        .and_then(|c| c.get("provider"))
        .and_then(|v| v.as_str())
        .unwrap_or("gemini");
    let grounding = config
        .and_then(|c| c.get("grounding"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let url_context = config
        .and_then(|c| c.get("url_context"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let streaming = config
        .and_then(|c| c.get("streaming"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // --- [NEW]: DATA INJECTION HOOK ---
    // Trigger Filter Hook to allow scripts to inject context (e.g., Vector Search results)
    let hook_payload = json!({ "slug": slug, "vars": payload.variables });
    let modified_payload = trigger_filter_hook(
        &state,
        "before_ai_run",
        hook_payload,
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    // Extract the potentially modified variables
    let final_vars = modified_payload.get("vars").unwrap_or(&payload.variables);

    // 4. Construct Final Templated Prompt
    let mut final_prompt = action.template.clone();
    let re_vars = regex::Regex::new(r"\{\{(\w+)\}\}").unwrap();

    final_prompt = re_vars
        .replace_all(&final_prompt, |caps: &regex::Captures| {
            let key = &caps[1];
            final_vars
                .get(key)
                .map(|v| {
                    // Automatically stringify objects/arrays so RAG context maps cleanly
                    if v.is_object() || v.is_array() {
                        serde_json::to_string(v).unwrap_or_default()
                    } else if let Some(s) = v.as_str() {
                        s.to_string()
                    } else {
                        v.to_string() // Handles numbers/booleans natively
                    }
                })
                .filter(|s| !s.starts_with("data:")) // Ignore binary/image base64 strings in templates
                .unwrap_or_default()
        })
        .to_string();

    // [TRIGGER] Before AI Run
    trigger_void_hook(
        &state,
        "before_ai_run",
        json!({ "slug": slug, "vars": payload.variables }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    let client = reqwest::Client::new();

    // 5. CALL AI PROVIDER LAYER
    if provider == "gemini" {
        // --- GOOGLE GEMINI EXECUTION INGRESS ---
        let mut content_parts = vec![json!({ "text": final_prompt })];

        // Capture base64 inline images if present
        if let Some(obj) = payload.variables.as_object() {
            for (_key, value) in obj {
                if let Some(str_val) = value.as_str()
                    && let Some((mime, data)) = parse_data_uri(str_val)
                {
                    content_parts.push(json!({
                        "inline_data": {
                            "mime_type": mime,
                            "data": data
                        }
                    }));
                }
            }
        }

        let mut request_body = json!({
            "contents": [{ "parts": content_parts }]
        });

        // Setup tools array
        let mut tools = vec![];
        if grounding {
            tools.push(json!({"google_search": {}}));
        }
        if url_context {
            tools.push(json!({"urlContext": {}}));
        }
        if !tools.is_empty() {
            request_body["tools"] = json!(tools);
        }

        // Add System Instructions if present
        if let Some(sys_prompt) = &action.system_prompt
            && !sys_prompt.trim().is_empty()
        {
            request_body["system_instruction"] = json!({
                "parts": [{ "text": sys_prompt }]
            });
        }

        if streaming {
            // --- STREAMING PIPELINE (SSE via unified lines reader) ---
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?key={}&alt=sse",
                action.model, api_key
            );

            let res = client
                .post(url)
                .json(&request_body)
                .send()
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;

            let stream = res.bytes_stream();
            let stream_reader = tokio_util::io::StreamReader::new(
                stream.map(|res| res.map_err(std::io::Error::other)),
            );
            let mut buf_reader = tokio::io::BufReader::new(stream_reader);

            let response_stream = async_stream::stream! {
                let mut line = String::new();
                while let Ok(n) = buf_reader.read_line(&mut line).await {
                    if n == 0 { break; }
                    let trimmed = line.trim();
                    if trimmed.starts_with("data:") {
                        let data_val = trimmed.strip_prefix("data:").unwrap().trim();
                        if data_val == "[DONE]" { break; }
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data_val)
                            && let Some(chunk) = parsed["candidates"][0]["content"]["parts"][0]["text"].as_str() {
                                // Explicitly typed as Result<Event, std::io::Error>
                                yield Ok::<Event, std::io::Error>(Event::default().data(chunk));
                            }
                    }
                    line.clear();
                }
            };

            let sse =
                Sse::new(response_stream).keep_alive(axum::response::sse::KeepAlive::default());
            Ok(sse.into_response())
        } else {
            // --- STANDARD SINGLE RESPONSE ---
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                action.model, api_key
            );

            let res = client
                .post(url)
                .json(&request_body)
                .send()
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;

            let response_json: serde_json::Value = res
                .json()
                .await
                .map_err(|_| AppError::JsonError("Invalid response".into()))?;

            let candidate = &response_json["candidates"][0]["content"]["parts"][0];
            let result = candidate["text"].as_str().unwrap_or("").to_string();
            let metadata = response_json["candidates"][0]["groundingMetadata"].clone();

            // [LOG]
            let meta = extract_log_meta(&headers, Some(addr), json!({ "slug": slug }));
            let _ = db
                .log_audit_event("info", "AI Action Run", "ai", Some(meta))
                .await;

            // [TRIGGER] After AI Run
            let _ = trigger_void_hook(
                &state,
                "after_ai_run",
                json!({ "slug": slug, "result": result, "metadata": metadata }),
                claims.as_ref(),
                Some(&event_scope.clone()),
                Some(base_url.clone()),
            )
            .await;

            Ok(axum::Json(json!({
                "result": result,
                "metadata": metadata
            }))
            .into_response())
        }
    } else {
        // --- OPENAI-COMPATIBLE (GROQ / OPENAI) EXECUTION INGRESS ---
        let endpoint = if provider == "groq" {
            "https://api.groq.com/openai/v1/chat/completions"
        } else {
            "https://api.openai.com/v1/chat/completions"
        };

        let mut messages = vec![];
        if let Some(sys) = &action.system_prompt
            && !sys.trim().is_empty()
        {
            messages.push(json!({ "role": "system", "content": sys }));
        }
        messages.push(json!({ "role": "user", "content": final_prompt }));

        let mut request_body = json!({
            "model": action.model,
            "messages": messages,
            "stream": streaming
        });

        // Resolve advanced parameters (Action config overrides Global AI config)
        let temp = config
            .and_then(|c| c.get("temperature").and_then(|v| v.as_f64()))
            .map(|v| v as f32)
            .or(ai_config.temperature);
        let max_tok = config
            .and_then(|c| c.get("max_tokens").and_then(|v| v.as_u64()))
            .map(|v| v as u32)
            .or(ai_config.max_tokens);
        let top_p_val = config
            .and_then(|c| c.get("top_p").and_then(|v| v.as_f64()))
            .map(|v| v as f32)
            .or(ai_config.top_p);

        if let Some(obj) = request_body.as_object_mut() {
            if let Some(t) = temp {
                obj.insert("temperature".to_string(), json!(t));
            }
            if let Some(m) = max_tok {
                if provider == "groq" {
                    // Groq specifically expects `max_completion_tokens` for newer models
                    obj.insert("max_completion_tokens".to_string(), json!(m));
                } else {
                    obj.insert("max_tokens".to_string(), json!(m));
                }
            }
            if let Some(tp) = top_p_val {
                obj.insert("top_p".to_string(), json!(tp));
            }

            // Explicitly set stop to null as required by some strict API validations
            obj.insert("stop".to_string(), Value::Null);
        }

        if streaming {
            // --- OPENAI SSE COMPATIBLE STREAM PARSING ---
            let res = client
                .post(endpoint)
                .bearer_auth(&api_key)
                .json(&request_body)
                .send()
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;

            let stream = res.bytes_stream();
            let stream_reader = tokio_util::io::StreamReader::new(
                stream.map(|res| res.map_err(std::io::Error::other)),
            );
            let mut buf_reader = tokio::io::BufReader::new(stream_reader);

            let response_stream = async_stream::stream! {
                let mut line = String::new();
                while let Ok(n) = buf_reader.read_line(&mut line).await {
                    if n == 0 { break; }
                    let trimmed = line.trim();
                    if trimmed.starts_with("data:") {
                        let data_val = trimmed.strip_prefix("data:").unwrap().trim();
                        if data_val == "[DONE]" { break; }
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data_val)
                            && let Some(delta) = parsed["choices"][0]["delta"]["content"].as_str() {
                                // Explicitly typed as Result<Event, std::io::Error>
                                yield Ok::<Event, std::io::Error>(Event::default().data(delta));
                            }
                    }
                    line.clear();
                }
            };

            let sse =
                Sse::new(response_stream).keep_alive(axum::response::sse::KeepAlive::default());
            Ok(sse.into_response())
        } else {
            // --- STANDARD SINGLE RESPONSE ---
            let res = client
                .post(endpoint)
                .bearer_auth(&api_key)
                .json(&request_body)
                .send()
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;

            if !res.status().is_success() {
                let err_text = res.text().await.unwrap_or_default();
                return Err(AppError::UnknownError(format!(
                    "AI Provider Error: {}",
                    err_text
                )));
            }

            let response_json: serde_json::Value = res
                .json()
                .await
                .map_err(|_| AppError::JsonError("Invalid response from AI provider".into()))?;

            let result = response_json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string();

            // [LOG]
            let meta = extract_log_meta(&headers, Some(addr), json!({ "slug": slug }));
            let _ = db
                .log_audit_event("info", "AI Action Run", "ai", Some(meta))
                .await;

            // [TRIGGER] After AI Run
            let _ = trigger_void_hook(
                &state,
                "after_ai_run",
                json!({ "slug": slug, "result": result, "metadata": null }),
                claims.as_ref(),
                Some(&event_scope.clone()),
                Some(base_url.clone()),
            )
            .await;

            Ok(axum::Json(json!({ "result": result, "metadata": null })).into_response())
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CodeEditReq {
    pub prompt: String,
    pub current_code: String,
    pub context_type: String, // "script" or "template"
    pub model: String,        //  Model Field
}

#[utoipa::path(
    post,
    path = "/api/v1/ai/edit-code",
    request_body = CodeEditReq,
    responses((status = 200, body = Value))
)]
pub async fn edit_code(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Json(req): Json<CodeEditReq>,
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // 1. Get Config (From Tenant/Sandbox DB)
    let ai_settings = db
        .get_config("ai")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let (api_key, _model) = match ai_settings {
        Some(val) => {
            let conf: AiConfigDto = serde_json::from_value(val).unwrap_or_default();
            if !conf.enabled {
                return Err(AppError::Forbidden("AI disabled".into()));
            }
            let raw = conf
                .api_key
                .ok_or(AppError::UnknownError("AI Key missing".into()))?;
            let enc: EncryptedValue =
                serde_json::from_str(&raw).map_err(|_| AppError::UnknownError("Bad key".into()))?;
            let key = state
                .vault
                .decrypt(&enc)
                .map_err(|_| AppError::UnknownError("Decrypt fail".into()))?;
            let modl = conf.model.unwrap_or("gemini-2.5-flash".to_string());
            (key, modl)
        }
        None => return Err(AppError::UnknownError("AI not configured".into())),
    };

    // 2. Construct Prompt
    let system_context = if req.context_type == "script" {
        "You are a JavaScript expert for the ApexKit runtime. Globals available: $db, $http, console. Return ONLY the updated code code."
    } else {
        "You are an HTML/Tera expert. Use Tailwind CSS. Return ONLY the updated HTML code."
    };

    let full_prompt = format!(
        "{}\n\nExisting Code:\n```\n{}\n```\n\nUser Instruction: {}\n\nOutput only the code, no markdown fencing.",
        system_context, req.current_code, req.prompt
    );

    // 3. Call LLM
    let client = reqwest::Client::new();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        req.model, api_key
    );

    let body = json!({
        "contents": [{ "role": "user", "parts": [{ "text": full_prompt }] }]
    });

    let res = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let res_json: Value = res
        .json()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let mut code = res_json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    code = code
        .trim()
        .trim_start_matches("```javascript")
        .trim_start_matches("```html")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .to_string();

    Ok(Json(json!({ "code": code })))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/ai/sessions/{id}",
    responses((status = 204, description = "Session deleted"))
)]
pub async fn delete_session(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    state
        .db
        .delete_ai_session(&id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    state
        .db
        .delete_sandbox_metadata(&id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    state.sandbox_manager.cleanup_sandbox(&id);

    Ok(StatusCode::NO_CONTENT)
}
