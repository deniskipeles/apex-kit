// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/ai_architect.rs ===========================
use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde_json::{json, Value};
use tinybase_core::{
    auth::Claims,
    models::{AppManifest, CreateTemplateReq},
    script_models::{self},
    ai_models::{AiSession, ChatMessage, CreateSessionReq, ChatReq, Plugin},
    Db, // Import Db trait
};
use crate::{AppState, AppError, sandbox_manager::SandboxManager};
use tracing::{info, error, warn};
use std::sync::Arc;

// --- DOCS & KNOWLEDGE BASE ---
const TINYBASE_DOCS_CORE: &str = r#"
## TINYBASE REFERENCE MANUAL (CORE)

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
Scripts run on the server. They must export a default async function.
**Signature**: `export default async function(req) { ... }`
**Routes**: To run a script from HTMX/Frontend, use **`/run/{script_name}`**.

**Global Objects**:
- **$db**: Database Access
  - `await $db.find_one(col_name, id)` -> Object | null
  - `await $db.find(col_name, filter_object)` -> Array<Object>
  - `await $db.insert(col_name, data_object)` -> ID (number)
  - `await $db.update(col_name, id, data_object)` -> Object
  - `await $db.delete(col_name, id)` -> bool
- **$http**: External Requests
  - `await $http.get(url)` -> String
  - `await $http.post(url, json_body)` -> String
- **$util**: 
  - `$util.uuid()` -> String

**Request/Response**:
- `req.json()` -> Promise<Object>
- `req.body` -> Object (if pre-parsed)
- `return new Response(body_object, { status: 200 })`

### 3. TEMPLATING (Tera / HTML)
Templates are HTML files rendered on the server.
- **Data Context**: 
  - If a **Script** is linked (`loader_script`), the JSON object returned by the script's `Response` is merged into the template context.
  - Example: Script returns `{ "tasks": [...] }` -> Template uses `{% for t in tasks %}`.
- **Helpers**:
  - `{% set users = db_find(col='users', filter=null) %}` (Fetch all)
  - `{% set item = db_find_one(col='items', id=1) %}`
  - **IMPORTANT**: Use keyword arguments (e.g. `col=`, `id=`) inside helper functions.
- **Variables**: `{{ params.slug }}`, `{{ headers['user-agent'] }}`, `{{ user.email }}`.

### 4. ARCHITECTURE PATTERNS
- **HTMX**: Use `hx-get="/render/my-page"` for dynamic partials.
- **Form Handling**: 
  1. Create a Script `handle_form` that processes input and returns `{ success: true }`.
  2. Create an Action/Endpoint via Script trigger `manual`.
  3. In HTML: `<form hx-post="/run/handle_form">`.
"#;

const ARCHITECT_SYSTEM_PROMPT: &str = r#"
You are the TinyBase Architect (v2.3). Your goal is to build functioning, full-stack web apps.

### INSTRUCTIONS
1. **Analyze** the user request.
2. **Design** the database schema, backend logic (scripts), and frontend (templates).
3. **Reference** the `TINYBASE_DOCS` below for strict syntax rules.
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
      "content": "{% extends \"todos/base.html\" %}\r\n\r\n{% block title %}\r\nHome Page\r\n{% endblock %}\r\n\r\n{% block content %}\r\n\r\n<!-- {% include 'components/header' %} -->\r\n<main class=\"container mx-auto mt-8\">\r\n    <div hx-get=\"/render/products/list\" hx-trigger=\"load\">\r\n        <!-- Content will be loaded here via HTMX -->\r\n    </div>\r\n</main>\r\n{% endblock %}"
    },
    {
      "slug": "todos/base.html",
      "content": "<!DOCTYPE html>\n<html lang=\"en\" hx-ext=\"morphdom\">\n<head>\n    <meta charset=\"UTF-8\">\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n    <title>{% block title %}Electronic Shop{% endblock %}</title>\n    \n    <!-- Scripts -->\n    <script src=\"/static/js/htmx.js\"></script>\n    <script src=\"/static/js/alpine.js\" defer></script>\n    \n    <!-- Tailwind CSS + Configuration -->\n    <link rel=\"stylesheet\" href=\"/styles.css\" />\n    \n    <!-- Custom Styles -->\n    <style>\n        /* Smooth transitions */\n        * {\n            /* Avoid transition on page load */\n            @apply transition-colors duration-200;\n        }\n        \n        .mobile-menu {\n            max-height: 0;\n            overflow: hidden;\n            transition: max-height 0.3s ease-out;\n        }\n        \n        .mobile-menu.open {\n            max-height: 300px;\n        }\n    </style>\n</head>\n\n<!-- \n    NOTE: We move the x-data to <html> so the class is applied at the root level, \n    which is best practice for Tailwind dark mode, but putting it on <body> works too \n    if configured correctly. Tailwind looks up the tree.\n-->\n<body \n    x-data=\"{\n        darkMode: localStorage.getItem('darkMode') === 'true' || \n                 (localStorage.getItem('darkMode') === null && window.matchMedia('(prefers-color-scheme: dark)').matches),\n        mobileMenuOpen: false\n    }\"\n    :class=\"{ 'dark': darkMode }\"\n    class=\"min-h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100 flex flex-col\"\n>\n    <!-- NAVBAR -->\n    <nav class=\"bg-gradient-to-r from-yellow-500 to-yellow-600 dark:from-yellow-700 dark:to-yellow-800 shadow-lg sticky top-0 z-50\">\n        <div class=\"container mx-auto px-4\">\n            <div class=\"flex justify-between items-center py-4\">\n                <!-- Brand -->\n                <a href=\"/render/index\" class=\"text-2xl font-bold tracking-wide text-white hover:text-yellow-200 transition\">\n                    ⚡ Electronic Shop\n                </a>\n\n                <!-- Desktop Links + Theme Toggle -->\n                <div class=\"hidden md:flex items-center space-x-6 text-white\">\n                    <a href=\"/render/products/create\" class=\"hover:text-yellow-200 transition font-medium\">Add Product</a>\n                    <a href=\"/render/categories/list\" class=\"hover:text-yellow-200 transition font-medium\">Categories</a>\n                    \n                    <!-- Theme Toggle -->\n                    <button\n                        @click=\"darkMode = !darkMode; localStorage.setItem('darkMode', darkMode)\"\n                        class=\"p-2 rounded-full bg-white/20 hover:bg-white/30 transition-all duration-300 focus:outline-none focus:ring-2 focus:ring-yellow-300\"\n                        aria-label=\"Toggle theme\"\n                    >\n                        <!-- Sun icon (Shown when NOT dark) -->\n                        <svg\n                            x-show=\"!darkMode\"\n                            xmlns=\"http://www.w3.org/2000/svg\"\n                            class=\"h-6 w-6 text-yellow-300\"\n                            fill=\"none\"\n                            viewBox=\"0 0 24 24\"\n                            stroke=\"currentColor\"\n                            stroke-width=\"2\"\n                        >\n                            <path\n                                stroke-linecap=\"round\"\n                                stroke-linejoin=\"round\"\n                                d=\"M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z\"\n                            />\n                        </svg>\n\n                        <!-- Moon icon (Shown when dark) -->\n                        <svg\n                            x-show=\"darkMode\"\n                            style=\"display: none;\" \n                            xmlns=\"http://www.w3.org/2000/svg\"\n                            class=\"h-6 w-6 text-indigo-200\"\n                            fill=\"none\"\n                            viewBox=\"0 0 24 24\"\n                            stroke=\"currentColor\"\n                            stroke-width=\"2\"\n                        >\n                            <path\n                                stroke-linecap=\"round\"\n                                stroke-linejoin=\"round\"\n                                d=\"M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z\"\n                            />\n                        </svg>\n                    </button>\n                </div>\n\n                <!-- Mobile menu button -->\n                <button\n                    @click=\"mobileMenuOpen = !mobileMenuOpen\"\n                    class=\"md:hidden p-2 rounded-md text-white hover:bg-white/20 transition\"\n                >\n                    <svg class=\"h-6 w-6\" fill=\"none\" viewBox=\"0 0 24 24\" stroke=\"currentColor\">\n                        <path x-show=\"!mobileMenuOpen\" stroke-linecap=\"round\" stroke-linejoin=\"round\" stroke-width=\"2\" d=\"M4 6h16M4 12h16M4 18h16\" />\n                        <path x-show=\"mobileMenuOpen\" style=\"display: none;\" stroke-linecap=\"round\" stroke-linejoin=\"round\" stroke-width=\"2\" d=\"M6 18L18 6M6 6l12 12\" />\n                    </svg>\n                </button>\n            </div>\n\n            <!-- Mobile menu -->\n            <div\n                x-show=\"mobileMenuOpen\"\n                style=\"display: none;\"\n                x-transition\n                class=\"md:hidden pb-4 text-white\"\n            >\n                <div class=\"flex flex-col space-y-3\">\n                    <a href=\"/render/products/create\" class=\"hover:text-yellow-200 transition font-medium py-2\">Add Product</a>\n                    <a href=\"/render/categories/list\" class=\"hover:text-yellow-200 transition font-medium py-2\">Categories</a>\n                    \n                    <button\n                        @click=\"darkMode = !darkMode; localStorage.setItem('darkMode', darkMode)\"\n                        class=\"flex items-center space-x-2 p-2 rounded-md hover:bg-white/20 transition text-left\"\n                    >\n                        <span>Switch Theme</span>\n                    </button>\n                </div>\n            </div>\n        </div>\n    </nav>\n\n    <!-- MAIN CONTENT -->\n    <main class=\"container mx-auto px-4 py-8 flex-grow\">\n        {% block content %}{% endblock %}\n    </main>\n\n    <!-- FOOTER -->\n    <footer class=\"bg-white dark:bg-gray-800 border-t border-gray-200 dark:border-gray-700 mt-auto\">\n        <div class=\"container mx-auto px-4 py-8\">\n            <div class=\"text-center text-gray-600 dark:text-gray-400\">\n                <p>&copy; 2024 Electronic Shop. All rights reserved.</p>\n                <p class=\"mt-2 text-sm\">Powered by TinyBase</p>\n            </div>\n        </div>\n    </footer>\n</body>\n</html>"
    },
    {
      "slug": "todos/list",
      "script_name": "get_todos", 
      "content": "<ul> {% for t in todos %} <li class='p-2 border'>{{t.title}}</li> {% endfor %} </ul>"
    }
  ]
}
"#;

// Helper to load external docs
fn get_docs() -> String {
    let mut docs = String::from(TINYBASE_DOCS_CORE);
    
    // Check ./docs folder relative to running binary/cwd
    let docs_path = std::path::Path::new("./docs");
    
    if let Ok(entries) = std::fs::read_dir(docs_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "md") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        docs.push_str("\n\n--- EXTERNAL DOC: ");
                        docs.push_str(file_name);
                        docs.push_str(" ---\n");
                        docs.push_str(&content);
                    }
                }
            }
        }
    }
    docs
}

// --- 1. START SESSION ---
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
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let id = uuid::Uuid::new_v4().to_string();
    info!("AI Architect: Initializing Sandbox Session '{}'", id);

    // 1. Create Physical Sandbox DB (Cloned from Prod)
    let _ = SandboxManager::create_sandbox(&id).await
        .map_err(|e| AppError::UnknownError(e))?;

    // 2. Create Session Record in MAIN DB (Metadata)
    let session = AiSession {
        id: id.clone(),
        name: req.name,
        messages: vec![],
        current_manifest: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state.db.create_ai_session(&session).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 3. If prompt exists, run it against SANDBOX
    if let Some(prompt) = req.initial_prompt {
        // PASS THE MODEL FROM THE REQUEST
        return chat_handler(id, prompt, req.model, state, true).await;
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
    Path(id): Path<String>,
    Json(req): Json<ChatReq>,
) -> Result<Json<AiSession>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    // PASS THE MODEL FROM THE REQUEST
    chat_handler(id, req.prompt, req.model, state, true).await
}

// --- 3. EXPORT AS PLUGIN (COMMIT) ---
#[utoipa::path(
    post, 
    path = "/api/v1/ai/sessions/{id}/publish",
    responses((status = 200, body = Plugin))
)]
pub async fn publish_plugin(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Plugin>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // 1. Get Session
    let session = state.db.get_ai_session(&id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Session not found".into()))?;

    let manifest = session.current_manifest.ok_or_else(|| {
        AppError::Validation(vec![]) 
    })?;

    info!("AI Architect: Committing Sandbox {} to Production...", id);

    // 2. Deploy Manifest to MAIN DB (Production)
    deploy_manifest(state.db.clone(), &manifest).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 3. Create Plugin Record
    let plugin = Plugin {
        id: uuid::Uuid::new_v4().to_string(),
        name: manifest.app_name.clone(),
        version: "1.0.0".to_string(),
        manifest: manifest,
        description: Some(format!("Exported from session: {}", session.name)),
    };

    state.db.save_plugin(&plugin).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    // 4. Cleanup Sandbox
    SandboxManager::cleanup_sandbox(&id);
    
    info!("AI Architect: Plugin '{}' published successfully.", plugin.name);

    Ok(Json(plugin))
}

// --- 4. LIST PLUGINS ---
#[utoipa::path(
    get,
    path = "/api/v1/ai/plugins",
    responses((status = 200, body = Vec<Plugin>))
)]
pub async fn list_plugins(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Plugin>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let plugins = state.db.list_plugins().await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(plugins))
}

// --- 5. LIST SESSIONS ---
#[utoipa::path(
    get,
    path = "/api/v1/admin/ai/sessions",
    responses((status = 200, body = Vec<AiSession>))
)]
pub async fn list_sessions(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
) -> Result<Json<Vec<AiSession>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let sessions = state.db.list_ai_sessions().await
        .map_err(|e| {
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
    use_sandbox: bool
) -> Result<Json<AiSession>, AppError> {
    info!("AI Architect: Processing prompt for session {}", session_id);

    // 1. Fetch Session (From Main DB)
    let mut session = state.db.get_ai_session(&session_id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Session not found".into()))?;

    // 2. Prepare Context
    let current_state_str = if let Some(m) = &session.current_manifest {
        serde_json::to_string(m).unwrap_or_else(|_| "{}".into())
    } else {
        "null".to_string()
    };

    // Load full documentation dynamically
    let docs = get_docs();

    // Combine Prompt with Docs and State
    let full_prompt = format!(
        "{}\n\n{}\n\n### CURRENT APP MANIFEST:\n{}\n\n### USER REQUEST:\n{}", 
        ARCHITECT_SYSTEM_PROMPT,
        docs,
        current_state_str, 
        user_prompt
    );

    // 3. Call LLM (First Attempt)
    let api_key = get_api_key(&state).await?; 
    let model = model_override.unwrap_or("gemini-2.5-flash".to_string());
    
    let response_text = call_llm(api_key.clone(), &model, &full_prompt).await?;

    // 4. Determine Target DB
    let target_db = if use_sandbox {
        // Try to load sandbox connection
        match SandboxManager::get_sandbox(&session_id).await {
            Ok(db) => db,
            Err(_) => {
                // If sandbox missing (expired/deleted), recreate from Prod
                warn!("Sandbox {} missing, recreating...", session_id);
                SandboxManager::create_sandbox(&session_id).await.map_err(|e| AppError::UnknownError(e))?
            }
        }
    } else {
        state.db.clone()
    };

    // 5. Try Parse & Deploy (With Self-Healing)
    let final_manifest = match process_and_deploy(target_db.clone(), &response_text).await {
        Ok(manifest) => manifest,
        Err(e) => {
            warn!("AI Architect: Deployment failed ({}). Attempting self-correction...", e);
            
            // --- SELF CORRECTION LOOP ---
            let error_prompt = format!(
                "You generated a JSON manifest that caused a deployment error:\n\nError: {}\n\nYour previous JSON response:\n{}\n\nFix the JSON errors and output ONLY the valid AppManifest JSON again.",
                e, response_text
            );
            
            let retry_response = call_llm(api_key.clone(), &model, &error_prompt).await?;
            
            match process_and_deploy(target_db, &retry_response).await {
                Ok(fixed_manifest) => {
                    info!("AI Architect: Self-correction successful.");
                    fixed_manifest
                },
                Err(final_err) => {
                    error!("AI Architect: Self-correction failed: {}", final_err);
                    
                    // Update session with the error so user sees it
                    session.messages.push(ChatMessage { role: "user".into(), content: user_prompt });
                    session.messages.push(ChatMessage { 
                        role: "assistant".into(), 
                        content: format!("I tried to build that, but encountered an error I couldn't fix:\n\n{}", final_err) 
                    });
                    state.db.update_ai_session(&session).await.ok();
                    
                    return Err(AppError::Validation(vec![])); // Return generic error to UI
                }
            }
        }
    };

    // 6. Success - Update History
    session.messages.push(ChatMessage { role: "user".into(), content: user_prompt });
    
    let msg_content = if use_sandbox {
        format!("Changes applied to SANDBOX. Preview at: /sandbox/{}/render/...", session_id)
    } else {
        format!("Changes applied. Updated '{}' successfully.", final_manifest.app_name)
    };

    session.messages.push(ChatMessage { 
        role: "assistant".into(), 
        content: msg_content
    });
    session.current_manifest = Some(final_manifest);

    state.db.update_ai_session(&session).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(session))
}

// --- HELPERS ---

async fn process_and_deploy(db: Arc<dyn Db>, llm_response: &str) -> Result<AppManifest, String> {
    // 1. Sanitize Markdown
    let clean_json = llm_response
        .trim()
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();

    // 2. Parse
    let manifest: AppManifest = serde_json::from_str(&clean_json)
        .map_err(|e| format!("Invalid JSON format: {}", e))?;

    // 3. Deploy to specific DB (Sandbox or Prod)
    deploy_manifest(db, &manifest).await.map_err(|e| e.to_string())?;

    Ok(manifest)
}

async fn get_api_key(state: &AppState) -> Result<String, AppError> {
    let ai_settings = state.db.get_setting("ai").await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    match ai_settings {
        Some(val) => {
            let conf: crate::settings::AiConfigDto = serde_json::from_value(val).unwrap_or_default();
            if !conf.enabled { return Err(AppError::Forbidden("AI disabled in settings".into())); }
            
            let raw = conf.api_key.ok_or(AppError::UnknownError("AI Key missing".into()))?;
            let enc: tinybase_core::security::EncryptedValue = serde_json::from_str(&raw)
                .map_err(|_| AppError::UnknownError("Bad key format".into()))?;
            
            state.vault.decrypt(&enc).map_err(|_| AppError::UnknownError("Decrypt fail".into()))
        },
        None => Err(AppError::UnknownError("AI not configured".into()))
    }
}

async fn call_llm(api_key: String, model: &str, prompt: &str) -> Result<String, AppError> {
    let client = reqwest::Client::new();
    // Inject model into URL
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}", model, api_key);
    
    let body = json!({ 
        "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
        "generationConfig": { "responseMimeType": "application/json" }
    });

    let res = client.post(url).json(&body).send().await
        .map_err(|e| AppError::UnknownError(format!("Network Error: {}", e)))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(AppError::UnknownError(format!("LLM API Error: {}", err)));
    }

    let json: Value = res.json().await
        .map_err(|e| AppError::UnknownError(format!("Invalid Response: {}", e)))?;
    
    Ok(json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("{}")
        .to_string())
}

async fn deploy_manifest(db: Arc<dyn Db>, manifest: &AppManifest) -> Result<(), AppError> {
    info!("AI Architect: Deploying Manifest '{}'", manifest.app_name);

     // A. Collections (Upsert Logic)
     let existing_cols = db.list_collections().await.unwrap_or_default();
     for col in &manifest.collections {
         if let Some(existing) = existing_cols.iter().find(|c| c.name == col.name) {
             info!("Updating collection: {}", col.name);
             db.update_collection(existing.id, None, Some(col.schema.clone())).await
                 .map_err(|e| AppError::UnknownError(format!("DB Update Error on col {}: {}", col.name, e)))?;
         } else {
             info!("Creating collection: {}", col.name);
             db.create_collection(&col.name, &Some(col.schema.clone())).await
                 .map_err(|e| AppError::UnknownError(format!("DB Create Error on col {}: {}", col.name, e)))?;
         }
     }

    // B. Scripts (Upsert via ON CONFLICT)
    for script in &manifest.scripts {
        info!("Deploying script: {}", script.name);
        
        // Validation: Check script syntax basic
        if !script.code.contains("export default") {
             return Err(AppError::UnknownError(format!("Script '{}' missing 'export default' definition", script.name)));
        }

        db.create_script(script_models::CreateScriptReq {
            name: script.name.clone(),
            trigger_type: script.trigger_type.clone(),
            code: script.code.clone()
        }).await.map_err(|e| AppError::UnknownError(format!("Script Error {}: {}", script.name, e)))?;
    }

    // C. Templates (Upsert via ON CONFLICT)
    for tmpl in &manifest.templates {
        info!("Deploying template: {}", tmpl.slug);
        
        // Link Loader Script ID if present
        let mut script_id = None;
        if let Some(s_name) = &tmpl.loader_script {
            if let Some(s) = db.get_script_by_name(s_name).await.unwrap_or(None) {
                script_id = Some(s.id);
            } else {
                warn!("Template '{}' references missing script '{}'", tmpl.slug, s_name);
                // We proceed anyway, assuming user might fix script later or it's a hallucination we can live with
            }
        }

        db.create_template(CreateTemplateReq {
            slug: tmpl.slug.clone(),
            content: tmpl.content.clone(),
            script_id
        }).await.map_err(|e| AppError::UnknownError(format!("Template Error {}: {}", tmpl.slug, e)))?;
    }

    info!("AI Architect: Deployment Complete.");
    Ok(())
}