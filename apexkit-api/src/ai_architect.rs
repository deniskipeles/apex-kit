use crate::sandbox_manager::CloneStrategy;
use crate::{AppError, AppState};
use apexkit_core::{
    Db,
    ai_models::{AiSession, ChatMessage, ChatReq, CreateSessionReq, Plugin},
    auth::Claims,
    models::{AppManifest, CreateTemplateReq},
    script_models::{self},
};
use axum::{
    Extension,
    extract::{Json, Path, State},
};
use serde::Deserialize; // Added Deserialize
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::{error, info};

// --- CONSTANTS & PROMPTS ---

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
Scripts run on the server using V8/Boa. You must export a default async function.
**Signature**: `export default async function(req) { ... }`

**Global Objects available in Scripts AND Templates:**
- **$db**: Database Access
  - `await $db.records.get(col_name, id)` -> Object | null
  - `await $db.records.list(col_name, filter_object)` -> { items: [], total: 0 }
  - `await $db.records.create(col_name, data_object)` -> { id: number }
  - `await $db.records.update(col_name, id, data_object)` -> Object
  - `await $db.records.delete(col_name, id)` -> bool
- **$http**: `await $http.get(url)`
- **$ai**: `await $ai.embed(text)`
- **$fs**: `await $fs.read(path)`
- **ApexKit**: `const client = new ApexKit();` (The full TS/JS SDK is injected server-side!)

### 3. TEMPLATING (Server-Side Rendering)
Templates support Server-Side Javascript via a Frontmatter block at the very top (similar to Astro).
The returned JSON is injected into the Tera HTML context.

```html
---
export default async function(req) {
    const data = await req.json(); // contains { params, headers }
    
    // Fetch data using the exact same $db API!
    const post = await $db.records.get('posts', data.params.id);
    const comments = await $db.records.list('comments', { filter: { post_id: data.params.id } });
    
    return { post, comments: comments.items };
}
---
<div>
    <h1>{{ post.title }}</h1>
    <ul>
        {% for comment in comments %}
            <li>{{ comment.text }}</li>
        {% endfor %}
    </ul>
</div>
```

### 4. ARCHITECTURE PATTERNS
- **HTMX**: Use `hx-get="/render/my-page"` for dynamic partials.
- **Form Handling**: 
  1. Create a Script `handle_form` that processes input and returns `{ success: true }`.
  2. Create an Action/Endpoint via Script trigger `manual`.
  3. In HTML: `<form hx-post="/run/handle_form">`.
"#;

const ARCHITECT_SYSTEM_PROMPT: &str = r#"
You are the ApexKit Architect (v0.1.0). Your goal is to build functioning, full-stack web apps.

### INSTRUCTIONS
1. **Analyze** the user request.
2. **Design** the database schema, backend logic (scripts), and frontend (templates).
3. **Reference** the `APEXKIT_DOCS` below for strict syntax rules.
4. **Output** a JSON `AppManifest` that deploys the entire application.

### RULES
- **Strict JSON**: Output ONLY valid JSON. No markdown fencing around the JSON if possible, or simple ```json blocks.
- **Interconnectivity**: 
  - To show data on a page, create a **Script** that fetches data (using `$db`) and returns it.
  - Then create a **Template** and set its `loader_script` to that script's name.
- **Styling**: Use Tailwind CSS classes.
- **Header**: Main pages MUST include:
            `<script src="/static/js/htmx.js"></script>`
            `<script src="/static/js/alpine.js" defer></script>`
            `<script src="/static/js/petite-vue.js" defer></script>`
            `<script src="/static/js/microframe.js" defer></script>`
            `<link rel="stylesheet" href="/styles.css">`

### OUTPUT FORMAT
You must output **STRICT JSON** matching this `AppManifest` structure. No Markdown formatting needed around the JSON.

{
  "app_name": "Todo App",
  "collections": [
    { 
      "name": "todos", 
      "schema": { 
        "fields": { 
          "title": { "type": "string", "required": true, "min_length": 3 }, 
          "description": { "type": "text", "required": false },
          "is_completed": { "type": "boolean", "default": false, "required": false },
          "priority": { "type": "select", "options": ["low", "high"], "required": false },
          "owner_id": { "type": "owner", "required": true }
        } 
      } 
    }
  ],
  "scripts": [
    { 
      "name": "toggle_todo", 
      "trigger_type": "manual", 
      "code": "export default async function(req) { const { id } = await req.json(); const row = await $db.find_one('todos', id); if (!row) return new Response({error:'Not found'}, {status:404}); await $db.update('todos', id, {is_completed: !row.is_completed}); return new Response({success:true}); };" 
    },
    {
      "name": "get_todos",
      "trigger_type": "manual", 
      "code": "export default async function(req) { const todos = await $db.find('todos', {}); return new Response({ todos }); };"
    }
  ],
  "templates": [
    {
      "slug": "todos/index",
      "content": "..."
    }
  ]
}
"#;

// Helper to load external docs from disk
fn get_docs() -> String {
    let mut docs = String::from(APEXKIT_DOCS_CORE);
    let docs_path = std::path::Path::new("./docs");

    if let Ok(entries) = std::fs::read_dir(docs_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md")
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Some(file_name) = path.file_name().and_then(|n| n.to_str())
            {
                docs.push_str("\n\n--- EXTERNAL DOC: ");
                docs.push_str(file_name);
                docs.push_str(" ---\n");
                docs.push_str(&content);
            }
        }
    }
    docs
}

// --- DTO for Path Extraction ---
// This allows Axum to extract {id} safely even if extra params (like session_id) exist
#[derive(Deserialize)]
pub struct SessionIdPath {
    pub id: String,
}

// --- 1. START SESSION ---
#[utoipa::path(
    post, 
    path = "/api/v1/ai/sessions", 
    request_body = CreateSessionReq,
    responses((status = 200, body = AiSession))
)]
pub async fn start_session(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Json(req): Json<CreateSessionReq>,
) -> Result<Json<AiSession>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    info!("AI Architect: Initializing Sandbox Session '{}'", id);

    // [NEW] Parse clone strategy from request
    let strategy = match req.clone_strategy.as_deref() {
        Some("schema") => CloneStrategy::SchemaOnly,
        Some("partial") => CloneStrategy::Partial(req.clone_record_limit.unwrap_or(100)),
        Some("full") => CloneStrategy::Full,
        _ => CloneStrategy::None,
    };

    // 1. Create Physical Sandbox (Manager)
    let _ = state
        .sandbox_manager
        .create_sandbox(&id, strategy, state.db.clone())
        .await
        .map_err(|e| AppError::UnknownError(e))?;

    // 2. [NEW] Register in Root DB _sandboxes table
    // Capture user ID from claims if available (e.g., if we allow non-admins to create sandboxes later)
    let owner_id = claims.uid;

    // Default expiry: 24 hours
    let expires_at = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .map(|d| d.to_rfc3339());

    state
        .db
        .register_sandbox(&id, Some(owner_id), Some(req.name.clone()), expires_at)
        .await
        .map_err(|e| {
            AppError::UnknownError(format!("Failed to register sandbox metadata: {}", e))
        })?;

    // 3. Create Session Record (AI Context)
    let session = AiSession {
        id: id.clone(),
        name: req.name,
        messages: vec![],
        current_manifest: None,
        pending_manifest: None,
        diff_summary: None,
        last_error: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state
        .db
        .create_ai_session(&session)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 4. If prompt exists, run generation
    if let Some(prompt) = req.initial_prompt {
        return chat_handler(id, prompt, req.model, state).await;
    }

    Ok(Json(session))
}

// --- 2. CHAT / ITERATE ---
#[utoipa::path(
    post, 
    path = "/api/v1/ai/sessions/{id}/chat", 
    request_body = ChatReq,
    responses((status = 200, body = AiSession))
)]
pub async fn continue_chat(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Path(path): Path<SessionIdPath>, // FIXED: Use struct
    Json(req): Json<ChatReq>,
) -> Result<Json<AiSession>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    chat_handler(path.id, req.prompt, req.model, state).await
}

// --- 3. APPLY CHANGES (NEW) ---
#[utoipa::path(
    post, 
    path = "/api/v1/ai/sessions/{id}/apply", 
    responses((status = 200, body = AiSession))
)]
pub async fn apply_changes(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Path(path): Path<SessionIdPath>, // FIXED: Use struct
) -> Result<Json<AiSession>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let id = path.id;
    let mut session = state
        .db
        .get_ai_session(&id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Session not found".into()))?;

    if let Some(pending) = &session.pending_manifest {
        // 1. Load Sandbox DB -> USE STATE MANAGER
        let sandbox_db = state
            .sandbox_manager
            .get_sandbox(&id)
            .await
            .map_err(|_| AppError::NotFound("Sandbox not initialized".into()))?;

        // 2. Deploy to Sandbox
        deploy_manifest(sandbox_db, pending)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        // 3. Update Session State
        session.current_manifest = Some(pending.clone());
        session.pending_manifest = None;
        session.diff_summary = None;
        session.last_error = None;

        session.messages.push(ChatMessage {
            role: "assistant".into(),
            content: "Changes have been applied to the Sandbox environment.".into(),
        });

        state
            .db
            .update_ai_session(&session)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        Ok(Json(session))
    } else {
        Err(AppError::Validation(vec![])) // No pending changes
    }
}

// --- 4. EXPORT AS PLUGIN (COMMIT) ---
#[utoipa::path(
    post, 
    path = "/api/v1/ai/sessions/{id}/publish",
    responses((status = 200, body = Plugin))
)]
pub async fn publish_plugin(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Path(path): Path<SessionIdPath>, // FIXED: Use struct
) -> Result<Json<Plugin>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let id = path.id;

    // 1. Get Session
    let session = state
        .db
        .get_ai_session(&id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Session not found".into()))?;

    let manifest = session
        .current_manifest
        .ok_or_else(|| AppError::Validation(vec![]))?;

    info!("AI Architect: Committing Sandbox {} to Production...", id);

    // 2. Deploy Manifest to MAIN DB (Production)
    // Note: This applies schema changes to the root database
    deploy_manifest(state.db.clone(), &manifest)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 3. Create Plugin Record
    let plugin = Plugin {
        id: uuid::Uuid::new_v4().to_string(),
        name: manifest.app_name.clone(),
        version: "1.0.0".to_string(),
        manifest,
        description: Some(format!("Exported from session: {}", session.name)),
    };

    state
        .db
        .save_plugin(&plugin)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 4. Cleanup Sandbox -> USE STATE MANAGER
    state.sandbox_manager.cleanup_sandbox(&id);

    info!(
        "AI Architect: Plugin '{}' published successfully.",
        plugin.name
    );

    Ok(Json(plugin))
}

// --- 5. LIST PLUGINS ---
#[utoipa::path(
    get,
    path = "/api/v1/ai/plugins",
    responses((status = 200, body = Vec<Plugin>))
)]
pub async fn list_plugins(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Plugin>>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let plugins = state
        .db
        .list_plugins()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(plugins))
}

// --- 6. LIST SESSIONS ---
#[utoipa::path(
    get,
    path = "/api/v1/admin/ai/sessions",
    responses((status = 200, body = Vec<AiSession>))
)]
pub async fn list_sessions(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
) -> Result<Json<Vec<AiSession>>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let sessions = state.db.list_ai_sessions().await.map_err(|e| {
        error!("Failed to list AI sessions: {}", e);
        AppError::UnknownError(e.to_string())
    })?;

    Ok(Json(sessions))
}

// --- INTERNAL LOGIC ---

async fn chat_handler(
    session_id: String,
    user_prompt: String,
    model_override: Option<String>,
    state: AppState,
) -> Result<Json<AiSession>, AppError> {
    info!("AI Architect: Processing prompt for session {}", session_id);

    // 1. Fetch Session
    let mut session = state
        .db
        .get_ai_session(&session_id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Session not found".into()))?;

    // 2. Prepare Context (Use Pending if exists, else Current)
    let base_manifest = session
        .pending_manifest
        .as_ref()
        .or(session.current_manifest.as_ref());

    let current_state_str = if let Some(m) = base_manifest {
        serde_json::to_string(m).unwrap_or_else(|_| "{}".into())
    } else {
        "null".to_string()
    };

    let docs = get_docs();
    let full_prompt = format!(
        "{}\n\n{}\n\n### CURRENT APP MANIFEST (JSON):\n{}\n\n### USER INSTRUCTION:\n{}",
        ARCHITECT_SYSTEM_PROMPT, docs, current_state_str, user_prompt
    );

    // 3. Call LLM
    let api_key = get_api_key(&state).await?;
    let model = model_override.unwrap_or("gemini-2.0-flash".to_string());

    let response_text = call_llm(api_key.clone(), &model, &full_prompt).await?;

    // 4. Parse & Diff
    match parse_manifest(&response_text) {
        Ok(new_manifest) => {
            // Calculate Diff
            let diff = generate_diff(session.current_manifest.as_ref(), &new_manifest);

            session.messages.push(ChatMessage {
                role: "user".into(),
                content: user_prompt,
            });
            session.messages.push(ChatMessage { 
                role: "assistant".into(), 
                content: "I have drafted the changes. Check the **Preview** tab to see the diff, then click **Apply** to deploy.".into() 
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
            session.last_error = Some(format!("Failed to parse AI response: {}", e));
            session.messages.push(ChatMessage { 
                role: "assistant".into(), 
                content: format!("I encountered an error generating the manifest:\n\n{}\n\nPlease try rephrasing your request.", e) 
            });
        }
    }

    state
        .db
        .update_ai_session(&session)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(session))
}

// --- HELPERS ---

fn parse_manifest(llm_response: &str) -> Result<AppManifest, String> {
    let clean_json = llm_response
        .trim()
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();

    serde_json::from_str(&clean_json).map_err(|e| format!("JSON Error: {}", e))
}

fn generate_diff(current: Option<&AppManifest>, new: &AppManifest) -> String {
    let mut diffs = Vec::new();

    // 1. Collections
    let empty_cols = vec![];
    let old_cols = current.map(|m| &m.collections).unwrap_or(&empty_cols);
    for new_col in &new.collections {
        if let Some(_) = old_cols.iter().find(|c| c.name == new_col.name) {
            diffs.push(format!("~ Modify Collection: {}", new_col.name));
        } else {
            diffs.push(format!("+ Create Collection: {}", new_col.name));
        }
    }

    // 2. Scripts
    let empty_scripts = vec![];
    let old_scripts = current.map(|m| &m.scripts).unwrap_or(&empty_scripts);
    for s in &new.scripts {
        if !old_scripts.iter().any(|os| os.name == s.name) {
            diffs.push(format!("+ Create Script: {}", s.name));
        } else {
            diffs.push(format!("~ Update Script: {}", s.name));
        }
    }

    // 3. Templates
    let empty_tmpls = vec![];
    let old_tmpls = current.map(|m| &m.templates).unwrap_or(&empty_tmpls);
    for t in &new.templates {
        if !old_tmpls.iter().any(|ot| ot.slug == t.slug) {
            diffs.push(format!("+ Create Page: {}", t.slug));
        } else {
            diffs.push(format!("~ Update Page: {}", t.slug));
        }
    }

    if diffs.is_empty() {
        "No major structural changes detected.".to_string()
    } else {
        diffs.join("\n")
    }
}

async fn deploy_manifest(db: Arc<dyn Db>, manifest: &AppManifest) -> Result<(), AppError> {
    info!("AI Architect: Deploying Manifest '{}'", manifest.app_name);

    // A. Collections (Upsert Logic)
    let existing_cols = db.list_collections().await.unwrap_or_default();
    for col in &manifest.collections {
        if let Some(existing) = existing_cols.iter().find(|c| c.name == col.name) {
            info!("Updating collection: {}", col.name);
            db.update_collection(existing.id, None, Some(col.schema.clone()))
                .await
                .map_err(|e| {
                    AppError::UnknownError(format!("DB Update Error on col {}: {}", col.name, e))
                })?;
        } else {
            info!("Creating collection: {}", col.name);
            db.create_collection(&col.name, &Some(col.schema.clone()), None)
                .await
                .map_err(|e| {
                    AppError::UnknownError(format!("DB Create Error on col {}: {}", col.name, e))
                })?;
        }
    }

    // B. Scripts
    for script in &manifest.scripts {
        info!("Deploying script: {}", script.name);

        db.create_script(script_models::CreateScriptReq {
            name: script.name.clone(),
            trigger_type: script.trigger_type.clone(),
            target_collection: None, // Explicitly set target_collection (required by struct)
            code: script.code.clone(),
            active: true,
            visibility: "private".to_string(),
        })
        .await
        .map_err(|e| AppError::UnknownError(format!("Script Error {}: {}", script.name, e)))?;
    }

    // C. Templates
    for tmpl in &manifest.templates {
        info!("Deploying template: {}", tmpl.slug);

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
        .map_err(|e| AppError::UnknownError(format!("Template Error {}: {}", tmpl.slug, e)))?;
    }

    info!("AI Architect: Deployment Complete.");
    Ok(())
}

async fn get_api_key(state: &AppState) -> Result<String, AppError> {
    let ai_settings = state
        .db
        .get_config("ai")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    match ai_settings {
        Some(val) => {
            let conf: crate::settings::AiConfigDto =
                serde_json::from_value(val).unwrap_or_default();
            if !conf.enabled {
                return Err(AppError::Forbidden("AI disabled in settings".into()));
            }

            let raw = conf
                .api_key
                .ok_or(AppError::UnknownError("AI Key missing".into()))?;
            let enc: apexkit_core::security::EncryptedValue = serde_json::from_str(&raw)
                .map_err(|_| AppError::UnknownError("Bad key format".into()))?;

            state
                .vault
                .decrypt(&enc)
                .map_err(|_| AppError::UnknownError("Decrypt fail".into()))
        }
        None => Err(AppError::UnknownError("AI not configured".into())),
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
        .map_err(|e| AppError::UnknownError(format!("Network Error: {}", e)))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(AppError::UnknownError(format!("LLM API Error: {}", err)));
    }

    let json: Value = res
        .json()
        .await
        .map_err(|e| AppError::UnknownError(format!("Invalid Response: {}", e)))?;

    Ok(json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("{}")
        .to_string())
}
