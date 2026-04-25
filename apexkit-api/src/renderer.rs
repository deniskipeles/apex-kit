use axum::{
    extract::{Path, State, Query},
    response::{Html, IntoResponse, Response},
    http::{HeaderMap, StatusCode}, 
    Extension, 
    Json, 
};
use serde_json::{json, Value};
use tera::{Tera}; 
use crate::{AppState, AppError, DatabaseConnection};
use std::collections::HashMap;
use std::sync::Arc;
use apexkit_core::Db;
use tracing::{warn, info};
use crate::BaseUrl;
use apexkit_core::realtime::EventScope;
use regex::Regex;
use apexkit_core::auth::Claims; // [NEW] Import Claims

// --- HELPERS ---
fn headers_to_map(headers: &HeaderMap) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            map.insert(k.to_string(), val.to_string());
        }
    }
    map
}

fn merge_json(a: &mut Value, b: Value) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b { a.insert(k, v); }
        }
        (a, b) => *a = b,
    }
}

fn extract_ssr_js(content: &str) -> (Option<String>, String) {
    if let Ok(re) = Regex::new(r"(?s)<script[^>]*>\s*//\s*---@@ssr\s*(.*?)\s*//\s*---@@ssr\s*</script>") {
        if let Some(caps) = re.captures(content) {
            if let Some(js_match) = caps.get(1) {
                let js_code = js_match.as_str().trim().to_string();
                let html_content = re.replace(content, "").to_string();
                return (Some(js_code), html_content);
            }
        }
    }

    let trimmed = content.trim_start();
    if trimmed.starts_with("---") {
        if let Some(end_idx) = trimmed[3..].find("\n---") {
            let js_code = trimmed[3..3+end_idx].trim().to_string();
            let html_start = 3 + end_idx + 4; 
            let html_content = trimmed[html_start..].to_string();
            return (Some(js_code), html_content);
        }
    }

    (None, content.to_string())
}

// --- CORE RENDERER ---

async fn render_view_core(
    db: Arc<dyn Db>,
    state: AppState,
    slug: String,
    params: HashMap<String, String>,
    headers: HeaderMap,
    body: String,
    source_label: &str,
    _base_url: Option<String>,
    scope: EventScope,
    auth: Option<Claims> // [NEW] Accept Auth Claims
) -> Result<Response, AppError> {
    info!("[Renderer] Serving '{}' from source: {}", slug, source_label);

    let all_templates = db.list_templates().await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let target_template = all_templates.iter()
        .find(|t| t.slug == slug)
        .ok_or_else(|| AppError::NotFound(format!("Template '{}' not found", slug)))?;

    let is_htmx = headers.contains_key("HX-Request");
    
    // [NEW] Inject Auth object into the JSON payload accessible by the SSR script
    let mut context_data = json!({
        "params": params,
        "headers": headers_to_map(&headers),
        "is_htmx": is_htmx,
        "auth": auth.map(|c| json!({ "id": c.uid, "email": c.sub, "role": c.role })),
    });

    let base_url = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .map(|h| format!("http://{}", h)); 

    if !body.is_empty() {
        if let Ok(j) = serde_json::from_str::<Value>(&body) { merge_json(&mut context_data, json!({"body": j})); } 
        else if let Ok(f) = serde_qs::from_str::<HashMap<String, String>>(&body) { merge_json(&mut context_data, json!({"body": f})); }
        else { merge_json(&mut context_data, json!({"body_raw": body})); }
    }

    let context = Arc::new(crate::ScopedScriptContext {
        state: state.clone(),
        scope: scope.clone(),
    });

    // 1. RUN LINKED SCRIPT
    if let Some(script_id) = target_template.script_id {
        let scripts = db.list_scripts().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        if let Some(script) = scripts.into_iter().find(|s| s.id == script_id) {
            let script_res = state.script_engine.run_script(
                &script.code, 
                context_data.clone(), 
                context.clone(),
                base_url.clone(), 
                Some(headers_to_map(&headers))
            ).await.map_err(|e| AppError::UnknownError(format!("Linked Script Error: {}", e)))?;
            
            // If the script returns an error response object (e.g. { error: "unauthorized" }), forward it
            if let Some(obj) = script_res.as_object() {
                if obj.contains_key("error") && obj.contains_key("status") {
                    let status = StatusCode::from_u16(obj.get("status").unwrap().as_u64().unwrap_or(400) as u16).unwrap_or(StatusCode::BAD_REQUEST);
                    return Ok((status, Json(script_res)).into_response());
                }
            }
            merge_json(&mut context_data, script_res);
        }
    }

    // 2. EXTRACT & RUN INLINE SSR JS
    let (inline_js, _) = extract_ssr_js(&target_template.content);
    if let Some(js_code) = inline_js {
        let script_res = state.script_engine.run_script(
            &js_code, 
            context_data.clone(), 
            context.clone(),
            base_url.clone(), 
            Some(headers_to_map(&headers))
        ).await.map_err(|e| AppError::UnknownError(format!("Template JS Error: {}", e)))?;
        
        // Handle explicit Responses (e.g. redirect or unauthorized)
        if let Some(obj) = script_res.as_object() {
            if obj.contains_key("error") && obj.contains_key("status") {
                let status = StatusCode::from_u16(obj.get("status").unwrap().as_u64().unwrap_or(400) as u16).unwrap_or(StatusCode::BAD_REQUEST);
                return Ok((status, Json(script_res)).into_response());
            }
        }
        merge_json(&mut context_data, script_res);
    }

    // 3. TERA SETUP
    let mut tera = Tera::default();
    let register_helpers = |t: &mut Tera| {
        t.register_filter("debug", |value: &Value, _: &HashMap<String, Value>| {
            Ok(serde_json::to_string_pretty(value).unwrap().into())
        });
    };
    register_helpers(&mut tera);

    // 4. STRIP SSR AND LOAD TERA
    let template_vec: Vec<(String, String)> = all_templates.iter()
        .map(|t| {
            let (_, html) = extract_ssr_js(&t.content);
            (t.slug.clone(), html)
        })
        .collect();
    
    if let Err(e) = tera.add_raw_templates(template_vec.clone()) {
        warn!("Batch template load failed: {}. Switching to resilient loading.", e);
        tera = Tera::default(); 
        register_helpers(&mut tera);
        
        let mut pending = template_vec;
        let mut passes = 0;
        
        while !pending.is_empty() && passes < 3 {
            let mut next_pending = Vec::new();
            let mut made_progress = false;

            for (tslug, content) in pending {
                if let Err(_) = tera.add_raw_template(&tslug, &content) {
                    next_pending.push((tslug, content));
                } else {
                    made_progress = true;
                }
            }
            pending = next_pending;
            passes += 1;
            if !made_progress { break; }
        }

        if pending.iter().any(|(s, _)| s == &slug) {
             if let Some((_, content)) = pending.iter().find(|(s, _)| s == &slug) {
                 if let Err(err) = tera.add_raw_template(&slug, content) {
                     return Err(AppError::UnknownError(format!("Template Compilation Error ('{}'): {}", slug, err)));
                 }
             }
        }
    }

    // 5. RENDER HTML
    let context = tera::Context::from_value(context_data)
        .map_err(|e| AppError::UnknownError(format!("Context Error: {}", e)))?;

    let mut rendered = tera.render(&slug, &context)
        .map_err(|e| AppError::UnknownError(format!("Render Error: {}", e)))?;

    // --- [NEW] AUTO-INJECT APEX.JS ---
    // We only inject if it's a full HTML document (contains </head> or </body>).
    // This prevents injecting it multiple times into small HTMX partials.
    if !rendered.contains("apex.js") {
        let script_tag = "\n    <script src=\"/static/js/apex.js\"></script>";
        if rendered.contains("</head>") {
            rendered = rendered.replace("</head>", &format!("{}</head>", script_tag));
        } else if rendered.contains("</body>") {
            rendered = rendered.replace("</body>", &format!("{}</body>", script_tag));
        } else if rendered.to_lowercase().contains("<html") {
            rendered.push_str(script_tag);
        }
    }

    Ok(Html(rendered).into_response())
}

// --- PUBLIC HANDLERS ---

pub async fn render_view(
    auth: Option<Extension<Claims>>, // [NEW]
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    Path(slug): Path<String>,
    Query(params): Query<HashMap<String, String>>, 
    headers: HeaderMap,
    body: String, 
) -> Result<Response, AppError> {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let claims = auth.map(|e| e.0);
    render_view_core(db, state, slug, params, headers, body, "Root App", Some(base_url), event_scope, claims).await
}

pub async fn render_sandbox_view(
    auth: Option<Extension<Claims>>, // [NEW]
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    Path((session_id, slug)): Path<(String, String)>, 
    Query(params): Query<HashMap<String, String>>, 
    headers: HeaderMap,
    body: String, 
) -> Result<Response, AppError> {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let claims = auth.map(|e| e.0);
    let label = format!("Sandbox {}", session_id);
    render_view_core(db, state, slug, params, headers, body, &label, Some(base_url), event_scope, claims).await
}

pub async fn render_tenant_view(
    auth: Option<Extension<Claims>>, // [NEW]
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    Path((tenant_id, slug)): Path<(String, String)>, 
    Query(params): Query<HashMap<String, String>>, 
    headers: HeaderMap,
    body: String, 
) -> Result<Response, AppError> {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let claims = auth.map(|e| e.0);
    let label = format!("Tenant {}", tenant_id);
    render_view_core(db, state, slug, params, headers, body, &label, Some(base_url), event_scope, claims).await
}