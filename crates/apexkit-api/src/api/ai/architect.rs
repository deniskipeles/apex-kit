use crate::{AppError, AppState, DatabaseConnection};
use apexkit_core::{
    Db,
    auth::Claims,
    models::ai::{AiSession, ChatMessage, ChatReq, Plugin},
    models::script as script_models,
    models::{AppManifest, CreateTemplateReq},
};
use axum::{Extension, Json, extract::State};
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::{error, info};

// --- SYSTEM PROMPT ---
const APEXKIT_DOCS_CORE: &str = r#"
## APEXKIT REFERENCE MANUAL (CORE)

### 1. DATA MODEL (Collections)
- **Field Types**: `string`,`text` (multiline),`number`,`boolean`,`email`,`url`,`date`,`json`.
- **Relations**: 
  - Type: `relation`. Properties: `relationTo` (collection name).
  - Naming convention: Use `_id` suffix (e.g.,`author_id`).
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
  - `await $db.records.get(col_name,id)`
  - `await $db.records.list(col_name,filter_object)`
  - `await $db.records.create(col_name,data_object)`
  - `await $db.records.update(col_name,id,data_object)`
  - `await $db.records.delete(col_name,id)`
- **$http**: `await $http.get(url)`
- **$ai**: `await $ai.embed(text)`
- **$fs**: `await $fs.read(path)`

### 3. TEMPLATING (Server-Side Rendering)
Templates use an Astro-like Frontmatter block at the top for Server-Side Javascript:
```html
---
export default async function(req) {
    const data = await req.json();
    const post = await $db.records.get('posts',data.params.id);
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
Analyze request -> output an AppManifest containing collections,scripts,and templates.

RULES:
- Output ONLY valid,strict JSON. No markdown wrappers if possible.
- Include base assets in all HTML templates:
    `<script src="/static/js/htmx.js"></script>`
    `<script src="/static/js/alpine.js" defer></script>`
    `<script src="/static/js/apex.js"></script>`
    `<script src="https://cdn.tailwindcss.com"></script>`

JSON MANIFEST FORMAT:
{
  "app_name": "My App",
  "collections": [
    {
      "name": "posts",
      "schema": {
        "fields": {
          "title": { "type": "string","required": true },
          "content": { "type": "text","required": false }
        }
      }
    }
  ],
  "scripts": [
    {
      "name": "create-post",
      "trigger_type": "manual",
      "code": "export default async function(req) { ... }"
    }
  ],
  "templates": [
    {
      "slug": "index.html",
      "content": "<h1>Hello World</h1>"
    }
  ]
}
"#;

// --- API HANDLERS ---

#[utoipa::path(
    get,
    path = "/api/v1/admin/ai/session",
    responses((status = 200,body = AiSession))
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
    responses((status = 200,body = AiSession))
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
    responses((status = 200,body = AiSession))
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

        // [FIXED]: Call update_ai_session instead of create_ai_session to prevent UNIQUE constraint errors
        db.update_ai_session(&session)
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
    responses((status = 200,body = Vec<Plugin>))
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
    let existing_session = sandbox_db
        .get_ai_session(session_id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let is_new_session = existing_session.is_none();

    let mut session = existing_session.unwrap_or_else(|| AiSession {
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

    let api_key = get_api_key(sandbox_db.clone(), &state).await?;
    let response_text = call_llm(api_key, &model, &full_prompt).await?;

    // --- DEBUG LOG FOR RAW LLM OUTPUT ---
    info!(
        "AI Architect: Raw LLM Response received:\n{}",
        response_text
    );

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

    if is_new_session {
        sandbox_db
            .create_ai_session(&session)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    } else {
        sandbox_db
            .update_ai_session(&session)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    Ok(Json(session))
}

fn sanitize_json_control_chars(input: &str) -> String {
    let mut output = String::new();
    let mut in_string = false;
    let mut escaped = false;

    for c in input.chars() {
        if in_string {
            if escaped {
                output.push(c);
                escaped = false;
            } else if c == '\\' {
                output.push(c);
                escaped = true;
            } else if c == '"' {
                output.push(c);
                in_string = false;
            } else if c == '\n' {
                output.push_str("\\n");
            } else if c == '\r' {
                output.push_str("\\r");
            } else if c == '\t' {
                output.push_str("\\t");
            } else if c.is_control() {
                // Skip other non-printable control characters
            } else {
                output.push(c);
            }
        } else {
            if c == '"' {
                in_string = true;
            }
            output.push(c);
        }
    }
    output
}

fn normalize_manifest_value(val: &mut Value) {
    if let Some(obj) = val.as_object_mut() {
        // 1. Ensure app_name exists
        if !obj.contains_key("app_name") {
            obj.insert("app_name".to_string(), json!("AI Architect App"));
        }

        // 2. Normalize Collections (fields array -> schema fields map)
        if let Some(cols) = obj.get_mut("collections").and_then(|v| v.as_array_mut()) {
            for col_val in cols {
                if let Some(col_obj) = col_val.as_object_mut()
                    && !col_obj.contains_key("schema")
                {
                    let mut fields_map = serde_json::Map::new();

                    // Parse fields list if defined as array
                    if let Some(fields_arr) = col_obj.get("fields").and_then(|v| v.as_array()) {
                        for f in fields_arr {
                            if let Some(f_obj) = f.as_object()
                                && let Some(f_name) = f_obj.get("name").and_then(|v| v.as_str())
                            {
                                let mut f_def = f_obj.clone();
                                f_def.remove("name"); // Remove name from within details

                                if !f_def.contains_key("required") {
                                    f_def.insert("required".to_string(), json!(false));
                                }
                                if !f_def.contains_key("uid") {
                                    // Generate an 8-character hex uid from a random UUID v4
                                    let uid_str =
                                        uuid::Uuid::new_v4().to_string()[0..8].to_string();
                                    f_def.insert("uid".to_string(), json!(uid_str));
                                }

                                fields_map.insert(f_name.to_string(), Value::Object(f_def));
                            }
                        }
                    }
                    // Fallback: If fields is already a map
                    else if let Some(fields_json_map) =
                        col_obj.get("fields").and_then(|v| v.as_object())
                    {
                        fields_map = fields_json_map.clone();
                    }

                    let schema_obj = json!({
                        "fields": fields_map,
                        "policies": {
                            "read": "public",
                            "create": "auth",
                            "update": "admin",
                            "delete": "admin"
                        }
                    });
                    col_obj.insert("schema".to_string(), schema_obj);
                    col_obj.remove("fields");
                }
            }
        }

        // 3. Normalize Scripts (path -> name,deduce trigger_type)
        if let Some(scripts) = obj.get_mut("scripts").and_then(|v| v.as_array_mut()) {
            for scr_val in scripts {
                if let Some(scr_obj) = scr_val.as_object_mut() {
                    let path_str = scr_obj
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    if !scr_obj.contains_key("name") {
                        let clean_name = path_str
                            .replace("api/", "")
                            .replace("/", "_")
                            .replace(".js", "")
                            .replace("{", "")
                            .replace("}", "");
                        scr_obj.insert("name".to_string(), json!(clean_name));
                    }
                    if !scr_obj.contains_key("trigger_type") {
                        let trigger = if path_str.starts_with("api/") {
                            "manual".to_string()
                        } else {
                            "before_create_record".to_string()
                        };
                        scr_obj.insert("trigger_type".to_string(), json!(trigger));
                    }
                }
            }
        }

        // 4. Normalize Templates (path -> slug,code -> content)
        if let Some(templates) = obj.get_mut("templates").and_then(|v| v.as_array_mut()) {
            for tmpl_val in templates {
                if let Some(tmpl_obj) = tmpl_val.as_object_mut() {
                    if !tmpl_obj.contains_key("slug")
                        && let Some(path) = tmpl_obj.get("path")
                    {
                        tmpl_obj.insert("slug".to_string(), path.clone());
                    }
                    if !tmpl_obj.contains_key("content")
                        && let Some(code) = tmpl_obj.get("code")
                    {
                        tmpl_obj.insert("content".to_string(), code.clone());
                    }
                }
            }
        }
    }
}

fn parse_manifest(llm_response: &str) -> Result<AppManifest, String> {
    let mut clean_json = llm_response
        .trim()
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();

    // Sanitize unescaped Control characters inside string values
    clean_json = sanitize_json_control_chars(&clean_json);

    // Parse into intermediate JSON Value to perform normalizations
    let mut raw_val: Value = serde_json::from_str(&clean_json).map_err(|e| {
        error!(
            "AI Architect: JSON Deserialization failed: {}. Cleaned JSON was:\n{}",
            e, clean_json
        );
        format!("JSON parsing failed: {}", e)
    })?;

    // Perform Schema Self-Healing
    normalize_manifest_value(&mut raw_val);

    // Map normalized value to AppManifest target struct
    serde_json::from_value(raw_val).map_err(|e| {
        error!("AI Architect: Schema Mapping from JSON failed: {}", e);
        format!("Manifest schema mapping failed: {}", e)
    })
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
            metadata: None,
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

async fn get_api_key(db: Arc<dyn Db>, state: &AppState) -> Result<String, AppError> {
    let mut settings_json = db
        .get_config("ai")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let mut use_root = false;
    if let Some(val) = &settings_json {
        let conf: crate::system::dto::AiConfigDto =
            serde_json::from_value(val.clone()).unwrap_or_default();
        if !conf.enabled || conf.api_key.is_none() {
            use_root = true;
        }
    } else {
        use_root = true;
    }

    if use_root {
        settings_json = state
            .db
            .get_config("ai")
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    match settings_json {
        Some(val) => {
            let conf: crate::system::dto::AiConfigDto =
                serde_json::from_value(val).unwrap_or_default();
            if !conf.enabled {
                return Err(AppError::Forbidden(
                    "AI features are disabled in settings".into(),
                ));
            }

            let raw = conf
                .api_key
                .ok_or_else(|| AppError::UnknownError("AI Config Key missing".into()))?;
            let enc: apexkit_core::security::vault::EncryptedValue = serde_json::from_str(&raw)
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
        "contents": [{ "role": "user","parts": [{ "text": prompt }] }],
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
