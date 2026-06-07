use crate::{AppError, AppState, DatabaseConnection};
use apexkit_core::{
    Db,
    ai_models::{AiSession, ChatMessage, ChatReq, Plugin},
    auth::Claims,
    models::{AppManifest, CreateTemplateReq},
    script_models::{self},
};
use axum::{Extension, Json, extract::State};
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::info;

// --- SYSTEM PROMPT ---
const APEXKIT_DOCS_CORE: &str = r#"
## APEXKIT REFERENCE MANUAL (CORE)

### 1. DATA MODEL (Collections)
- **Field Types**: `string`, `text` (multiline), `number`, `boolean`, `email`, `url`, `date`, `json`.
- **Relations**: 
  - Type: `relation`. Properties: `relationTo` (collection name).
  - Naming convention: Use `_id` suffix (e.g., `author_id`).
  - Access in Templates: Use `expand` in scripts or queries.
- **Special**: 
  - `owner`: Links to User ID automatically.
  - `file`: Stores a file path string.
  - `select`: Requires `options` array.

### 2. SCRIPTING API (Server-Side JS)
Scripts run on the server. You must export a default async function.
Signature: `export default async function(req) { ... }`

Global Objects:
- **$db**: Database Access
  - `await $db.records.get(col_name, id)`
  - `await $db.records.list(col_name, filter_object)`
  - `await $db.records.create(col_name, data_object)`
  - `await $db.records.update(col_name, id, data_object)`
  - `await $db.records.delete(col_name, id)`
- **$http**: `await $http.get(url)`
- **$ai**: `await $ai.embed(text)`
- **$fs**: `await $fs.read(path)`

### 3. TEMPLATING (Server-Side Rendering)
Templates use an Astro-like Frontmatter block at the top for Server-Side Javascript:
```html
---
export default async function(req) {
    const data = await req.json();
    const post = await $db.records.get('posts', data.params.id);
    return { post };
}
---
<div>
    <h1>{{ post.title }}</h1>
</div>
```
"#;

const ARCHITECT_SYSTEM_PROMPT: &str = r#"
You are the ApexKit Architect. Your goal is to build full-stack web applications.
Analyze request -> output an AppManifest containing collections, scripts, and templates.

RULES:
- Output ONLY valid, strict JSON. No markdown wrappers if possible.
- Include base assets in all HTML templates:
    `<script src="/static/js/htmx.js"></script>`
    `<script src="/static/js/alpine.js" defer></script>`
    `<script src="/static/js/apex.js"></script>`
    `<link rel="stylesheet" href="/styles.css">`
"#;

// --- API HANDLERS ---

#[utoipa::path(
    get, 
    path = "/api/v1/admin/ai/session", 
    responses((status = 200, body = AiSession))
)]
pub async fn get_session(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, // Contextual Sandbox DB
) -> Result<Json<AiSession>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let session = db
        .get_ai_session("default")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .unwrap_or_else(|| AiSession {
            id: "default".into(),
            name: "Architect".into(),
            messages: vec![],
            current_manifest: None,
            pending_manifest: None,
            diff_summary: None,
            last_error: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        });
    Ok(Json(session))
}

#[utoipa::path(
    post, 
    path = "/api/v1/admin/ai/chat", 
    request_body = ChatReq,
    responses((status = 200, body = AiSession))
)]
pub async fn chat_handler_api(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, // Contextual Sandbox DB
    State(state): State<AppState>,
    Json(req): Json<ChatReq>,
) -> Result<Json<AiSession>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    process_ai_chat(
        "default",
        db,
        state,
        req.prompt,
        req.model.unwrap_or("gemini-2.5-flash".into()),
    )
    .await
}

#[utoipa::path(
    post, 
    path = "/api/v1/admin/ai/apply", 
    responses((status = 200, body = AiSession))
)]
pub async fn apply_changes(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, // Contextual Sandbox DB
) -> Result<Json<AiSession>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let mut session = db
        .get_ai_session("default")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Session not found".into()))?;

    if let Some(pending) = &session.pending_manifest {
        deploy_manifest(db.clone(), pending).await?;

        session.current_manifest = Some(pending.clone());
        session.pending_manifest = None;
        session.diff_summary = None;
        session.last_error = None;
        session.messages.push(ChatMessage {
            role: "assistant".into(),
            content: "Changes applied to database.".into(),
        });

        db.create_ai_session(&session)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
        Ok(Json(session))
    } else {
        Err(AppError::Validation(vec![]))
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/ai/plugins",
    responses((status = 200, body = Vec<Plugin>))
)]
pub async fn list_plugins(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<Vec<Plugin>>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    let plugins = db
        .list_plugins()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(plugins))
}

// --- CORE UTILS ---

pub async fn process_ai_chat(
    session_id: &str,
    sandbox_db: Arc<dyn Db>,
    state: AppState,
    user_prompt: String,
    model: String,
) -> Result<Json<AiSession>, AppError> {
    let mut session = sandbox_db
        .get_ai_session(session_id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .unwrap_or_else(|| AiSession {
            id: session_id.into(),
            name: "Architect".into(),
            messages: vec![],
            current_manifest: None,
            pending_manifest: None,
            diff_summary: None,
            last_error: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        });

    let base_manifest = session
        .pending_manifest
        .as_ref()
        .or(session.current_manifest.as_ref());
    let current_state_str = if let Some(m) = base_manifest {
        serde_json::to_string(m).unwrap_or_else(|_| "{}".into())
    } else {
        "null".to_string()
    };

    let docs = String::from(APEXKIT_DOCS_CORE);
    let full_prompt = format!(
        "{}\n\n{}\n\n### CURRENT APP MANIFEST (JSON):\n{}\n\n### USER INSTRUCTION:\n{}",
        ARCHITECT_SYSTEM_PROMPT, docs, current_state_str, user_prompt
    );

    let api_key = get_api_key(&state).await?;
    let response_text = call_llm(api_key, &model, &full_prompt).await?;

    match parse_manifest(&response_text) {
        Ok(new_manifest) => {
            let diff = generate_diff(session.current_manifest.as_ref(), &new_manifest);
            session.messages.push(ChatMessage {
                role: "user".into(),
                content: user_prompt,
            });
            session.messages.push(ChatMessage {
                role: "assistant".into(),
                content: "I have drafted the changes. Select 'Apply' to merge them.".into(),
            });
            session.pending_manifest = Some(new_manifest);
            session.diff_summary = Some(diff);
            session.last_error = None;
        }
        Err(e) => {
            session.messages.push(ChatMessage {
                role: "user".into(),
                content: user_prompt,
            });
            session.last_error = Some(format!("Failed to parse response: {}", e));
            session.messages.push(ChatMessage {
                role: "error".into(),
                content: format!("Failed to parse structure:\n{}", e),
            });
        }
    }

    sandbox_db
        .create_ai_session(&session)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(session))
}

pub async fn deploy_manifest(db: Arc<dyn Db>, manifest: &AppManifest) -> Result<(), AppError> {
    info!("AI Architect: Deploying Manifest '{}'", manifest.app_name);

    // 1. Collections
    let existing_cols = db.list_collections().await.unwrap_or_default();
    for col in &manifest.collections {
        if let Some(existing) = existing_cols.iter().find(|c| c.name == col.name) {
            db.update_collection(existing.id, None, Some(col.schema.clone()))
                .await
                .map_err(|e| AppError::UnknownError(format!("Schema Update Error: {}", e)))?;
        } else {
            db.create_collection(&col.name, &Some(col.schema.clone()), None)
                .await
                .map_err(|e| AppError::UnknownError(format!("Schema Creation Error: {}", e)))?;
        }
    }

    // 2. Scripts
    for script in &manifest.scripts {
        db.create_script(script_models::CreateScriptReq {
            name: script.name.clone(),
            trigger_type: script.trigger_type.clone(),
            target_collection: None,
            code: script.code.clone(),
            active: true,
            visibility: "private".to_string(),
        })
        .await
        .map_err(|e| AppError::UnknownError(format!("Script Deployment Error: {}", e)))?;
    }

    // 3. Templates
    for tmpl in &manifest.templates {
        let mut script_id = None;
        if let Some(s_name) = &tmpl.loader_script
            && let Some(s) = db.get_script_by_name(s_name).await.unwrap_or(None)
        {
            script_id = Some(s.id);
        }

        db.create_template(CreateTemplateReq {
            slug: tmpl.slug.clone(),
            content: tmpl.content.clone(),
            script_id,
        })
        .await
        .map_err(|e| AppError::UnknownError(format!("Template Deployment Error: {}", e)))?;
    }

    Ok(())
}

async fn get_api_key(state: &AppState) -> Result<String, AppError> {
    let ai_settings_json = state
        .db
        .get_config("ai")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    match ai_settings_json {
        Some(val) => {
            let conf: crate::settings::AiConfigDto =
                serde_json::from_value(val).unwrap_or_default();
            if !conf.enabled {
                return Err(AppError::Forbidden(
                    "AI features are disabled in global settings".into(),
                ));
            }

            let raw = conf
                .api_key
                .ok_or_else(|| AppError::UnknownError("AI Config Key missing".into()))?;
            let enc: apexkit_core::security::EncryptedValue = serde_json::from_str(&raw)
                .map_err(|_| AppError::UnknownError("Invalid key wrapping".into()))?;

            state
                .vault
                .decrypt(&enc)
                .map_err(|_| AppError::UnknownError("Failed to decrypt API Key".into()))
        }
        None => Err(AppError::UnknownError(
            "AI has not been configured by the administrator".into(),
        )),
    }
}

async fn call_llm(api_key: String, model: &str, prompt: &str) -> Result<String, AppError> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let body = json!({
        "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
        "generationConfig": { "responseMimeType": "application/json" }
    });

    let res = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::UnknownError(format!("Inference request failed: {}", e)))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(AppError::UnknownError(format!(
            "Inference Engine Error: {}",
            err
        )));
    }

    let json: Value = res
        .json()
        .await
        .map_err(|e| AppError::UnknownError(format!("Response parsing failed: {}", e)))?;
    Ok(json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("{}")
        .to_string())
}

fn parse_manifest(llm_response: &str) -> Result<AppManifest, String> {
    let clean_json = llm_response
        .trim()
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();
    serde_json::from_str(&clean_json).map_err(|e| format!("JSON parsing failed: {}", e))
}

fn generate_diff(current: Option<&AppManifest>, new: &AppManifest) -> String {
    let mut diffs = Vec::new();
    let empty_cols = vec![];
    let old_cols = current.map(|m| &m.collections).unwrap_or(&empty_cols);
    for new_col in &new.collections {
        if old_cols.iter().any(|c| c.name == new_col.name) {
            diffs.push(format!("~ Modify Collection: {}", new_col.name));
        } else {
            diffs.push(format!("+ Create Collection: {}", new_col.name));
        }
    }

    let empty_scripts = vec![];
    let old_scripts = current.map(|m| &m.scripts).unwrap_or(&empty_scripts);
    for s in &new.scripts {
        if old_scripts.iter().any(|os| os.name == s.name) {
            diffs.push(format!("~ Update Script: {}", s.name));
        } else {
            diffs.push(format!("+ Create Script: {}", s.name));
        }
    }

    let empty_tmpls = vec![];
    let old_tmpls = current.map(|m| &m.templates).unwrap_or(&empty_tmpls);
    for t in &new.templates {
        if old_tmpls.iter().any(|ot| ot.slug == t.slug) {
            diffs.push(format!("~ Update Page: {}", t.slug));
        } else {
            diffs.push(format!("+ Create Page: {}", t.slug));
        }
    }

    if diffs.is_empty() {
        "No structural changes detected.".to_string()
    } else {
        diffs.join("\n")
    }
}
