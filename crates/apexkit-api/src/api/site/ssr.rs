use crate::BaseUrl;
use crate::{AppError, AppState, DatabaseConnection};
use apexkit_core::Db;
use apexkit_core::auth::Claims;
use apexkit_core::realtime::EventScope;
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
};
use regex::Regex;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tera::Tera;
use tracing::{info, warn};

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
            for (k, v) in b {
                a.insert(k, v);
            }
        }
        (a, b) => *a = b,
    }
}

fn extract_ssr_js(content: &str) -> (Option<String>, String) {
    // 1. NEW: Clean <script type="server/js"> syntax
    if let Ok(re) = Regex::new(r#"(?is)<script[^>]*type=["']server/js["'][^>]*>(.*?)</script>"#) {
        if let Some(caps) = re.captures(content) {
            if let Some(js_match) = caps.get(1) {
                let js_code = js_match.as_str().trim().to_string();
                let html_content = re.replace(content, "").to_string();
                return (Some(js_code), html_content);
            }
        }
    }

    // 2. Legacy: // ---@@ssr
    if let Ok(re) =
        Regex::new(r"(?s)<script[^>]*>\s*//\s*---@@ssr\s*(.*?)\s*//\s*---@@ssr\s*</script>")
    {
        if let Some(caps) = re.captures(content) {
            if let Some(js_match) = caps.get(1) {
                let js_code = js_match.as_str().trim().to_string();
                let html_content = re.replace(content, "").to_string();
                return (Some(js_code), html_content);
            }
        }
    }

    // 3. Legacy: Frontmatter (---)
    let trimmed = content.trim_start();
    if trimmed.starts_with("---")
        && let Some(end_idx) = trimmed[3..].find("\n---")
    {
        let js_code = trimmed[3..3 + end_idx].trim().to_string();
        let html_start = 3 + end_idx + 4;
        let html_content = trimmed[html_start..].to_string();
        return (Some(js_code), html_content);
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
    auth: Option<Claims>,
) -> Result<Response, AppError> {
    info!(
        "[Renderer] Serving '{}' from source: {}",
        slug, source_label
    );

    let all_templates = db
        .list_templates()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let target_template = all_templates
        .iter()
        .find(|t| t.slug == slug)
        .ok_or_else(|| AppError::NotFound(format!("Template '{}' not found", slug)))?;

    let is_htmx = headers.contains_key("HX-Request");

    let auth_obj = auth.map(|c| json!({ "id": c.uid, "email": c.sub, "role": c.role }));

    let mut context_data = json!({
        "params": params,
        "headers": headers_to_map(&headers),
        "is_htmx": is_htmx,
        "auth": auth_obj.clone(),
    });

    // Auto-injected SSR state for the frontend global window variable
    let mut ssr_state = json!({
        "params": params,
        "auth": auth_obj
    });

    let base_url = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .map(|h| format!("http://{}", h));

    if !body.is_empty() {
        if let Ok(j) = serde_json::from_str::<Value>(&body) {
            merge_json(&mut context_data, json!({"body": j}));
        } else if let Ok(f) = serde_qs::from_str::<HashMap<String, String>>(&body) {
            merge_json(&mut context_data, json!({"body": f}));
        } else {
            merge_json(&mut context_data, json!({"body_raw": body}));
        }
    }

    let context = Arc::new(crate::ScopedScriptContext {
        state: state.clone(),
        scope: scope.clone(),
    });

    // 1. RUN LINKED SCRIPT
    if let Some(script_id) = target_template.script_id {
        let scripts = db
            .list_scripts()
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
        if let Some(script) = scripts.into_iter().find(|s| s.id == script_id) {
            let script_res = state
                .script_engine
                .run_script(
                    &script.code,
                    context_data.clone(),
                    context.clone(),
                    base_url.clone(),
                    Some(headers_to_map(&headers)),
                )
                .await
                .map_err(|e| AppError::UnknownError(format!("Linked Script Error: {}", e)))?;

            // THE FIX: Handle new __is_apex_response format
            if let Some(obj) = script_res.as_object()
                && obj
                    .get("__is_apex_response")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            {
                let status_code = obj.get("status").and_then(|v| v.as_u64()).unwrap_or(400) as u16;
                let body = obj.get("body").cloned().unwrap_or_default();
                let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::BAD_REQUEST);

                if status.is_client_error() || status.is_server_error() {
                    return Ok((status, Json(body)).into_response());
                } else {
                    merge_json(&mut context_data, body.clone());
                    merge_json(&mut ssr_state, body);
                }
            } else {
                merge_json(&mut context_data, script_res.clone());
                merge_json(&mut ssr_state, script_res);
            }
        }
    }

    // 2. EXTRACT & RUN INLINE SSR JS
    let (inline_js, _) = extract_ssr_js(&target_template.content);
    if let Some(js_code) = inline_js {
        let script_res = state
            .script_engine
            .run_script(
                &js_code,
                context_data.clone(),
                context.clone(),
                base_url.clone(),
                Some(headers_to_map(&headers)),
            )
            .await
            .map_err(|e| AppError::UnknownError(format!("Template JS Error: {}", e)))?;

        // THE FIX: Handle new __is_apex_response format
        if let Some(obj) = script_res.as_object()
            && obj
                .get("__is_apex_response")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        {
            let status_code = obj.get("status").and_then(|v| v.as_u64()).unwrap_or(400) as u16;
            let body = obj.get("body").cloned().unwrap_or_default();
            let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::BAD_REQUEST);

            if status.is_client_error() || status.is_server_error() {
                return Ok((status, Json(body)).into_response());
            } else {
                merge_json(&mut context_data, body.clone());
                merge_json(&mut ssr_state, body);
            }
        } else {
            merge_json(&mut context_data, script_res.clone());
            merge_json(&mut ssr_state, script_res);
        }
    }

    // 3. TERA SETUP
    let mut tera = Tera::default();
    let register_helpers = |t: &mut Tera| {
        t.register_filter("debug", |value: &Value, _: &HashMap<String, Value>| {
            Ok(serde_json::to_string_pretty(value).unwrap().into())
        });
        // Extra helper to easily stringify json if needed explicitly in Tera
        t.register_filter("json", |value: &Value, _: &HashMap<String, Value>| {
            Ok(serde_json::to_string(value).unwrap().into())
        });
    };
    register_helpers(&mut tera);

    let template_vec: Vec<(String, String)> = all_templates
        .iter()
        .map(|t| {
            let (_, html) = extract_ssr_js(&t.content);
            (t.slug.clone(), html)
        })
        .collect();

    // A. RESILIENT FALLBACK COMPILATION ERROR HANDLING
    if let Err(e) = tera.add_raw_templates(template_vec.clone()) {
        warn!(
            "Batch template load failed: {}. Switching to resilient loading.",
            e
        );
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
            if !made_progress {
                break;
            }
        }

        if pending.iter().any(|(s, _)| s == &slug)
            && let Some((_, content)) = pending.iter().find(|(s, _)| s == &slug)
            && let Err(err) = tera.add_raw_template(&slug, content)
        {
            // Extract compilation errors
            let mut chain = vec![err.to_string()];
            let mut current: &dyn std::error::Error = &err;
            while let Some(source) = current.source() {
                chain.push(source.to_string());
                current = source;
            }
            return Err(AppError::RenderError {
                template: slug.clone(),
                error: err.to_string(),
                details: serde_json::json!(chain),
            });
        }
    }

    // B. RUNTIME RENDERING ERROR HANDLING
    let context = tera::Context::from_value(context_data)
        .map_err(|e| AppError::UnknownError(format!("Context Error: {}", e)))?;

    let mut rendered = tera.render(&slug, &context).map_err(|e| {
        // Recursively extract the exact stack trace and line numbers
        let mut chain = vec![e.to_string()];
        let mut current: &dyn std::error::Error = &e;
        while let Some(source) = current.source() {
            chain.push(source.to_string());
            current = source;
        }
        AppError::RenderError {
            template: slug.clone(),
            error: e.to_string(),
            details: serde_json::json!(chain),
        }
    })?;

    // --- AUTO-INJECT SCRIPTS ---
    // Inject the combined SSR State globally so scripts can access it instantly
    let state_script = format!(
        "\n    <script>window.__SSR_STATE__ = {};</script>",
        serde_json::to_string(&ssr_state).unwrap_or_else(|_| "{}".to_string())
    );
    let apex_script = "\n    <script src=\"/static/js/apex.js\"></script>";

    let injection = if rendered.contains("apex.js") {
        state_script
    } else {
        format!("{}{}", state_script, apex_script)
    };

    if rendered.contains("</head>") {
        rendered = rendered.replace("</head>", &format!("{}</head>", injection));
    } else if rendered.contains("</body>") {
        rendered = rendered.replace("</body>", &format!("{}</body>", injection));
    } else if rendered.to_lowercase().contains("<html") {
        rendered.push_str(&injection);
    } else {
        rendered.push_str(&injection);
    }

    // --- DYNAMIC PATH-BASED SCOPE REWRITING ---
    let mut scope_prefix = String::new();
    let root_domain = std::env::var("APEX_ROOT_DOMAIN").unwrap_or_default();
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Path-based routing is required if Host matches the Root Domain
    let is_path_routing = match &scope {
        EventScope::Tenant(_) | EventScope::Sandbox(_) => {
            root_domain.is_empty() || host.contains(&root_domain)
        }
        _ => false,
    };

    if is_path_routing {
        scope_prefix = match &scope {
            EventScope::Tenant(id) => format!("/tenant/{}", id),
            EventScope::Sandbox(id) => format!("/sandbox/{}", id),
            _ => "".to_string(),
        };
    }

    if !scope_prefix.is_empty() {
        // Rewrite "/render/" and "/styles.css" links recursively to prepend scope prefix
        let re_links = Regex::new(
            r#"(href|action|hx-get|hx-post|hx-put|hx-patch|hx-delete)="(/render/[^"]*|/styles\.css)""#,
        )
        .unwrap();
        rendered = re_links
            .replace_all(&rendered, |caps: &regex::Captures| {
                let attr = &caps[1];
                let path = &caps[2];
                format!("{}=\"{}{}\"", attr, scope_prefix, path)
            })
            .to_string();
    }

    Ok(Html(rendered).into_response())
}

// --- PUBLIC HANDLERS ---

pub async fn render_view(
    auth: Option<Extension<Claims>>,
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
    render_view_core(
        db,
        state,
        slug,
        params,
        headers,
        body,
        "Root App",
        Some(base_url),
        event_scope,
        claims,
    )
    .await
}

pub async fn render_sandbox_view(
    auth: Option<Extension<Claims>>,
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
    render_view_core(
        db,
        state,
        slug,
        params,
        headers,
        body,
        &label,
        Some(base_url),
        event_scope,
        claims,
    )
    .await
}

pub async fn render_tenant_view(
    auth: Option<Extension<Claims>>,
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
    render_view_core(
        db,
        state,
        slug,
        params,
        headers,
        body,
        &label,
        Some(base_url),
        event_scope,
        claims,
    )
    .await
}
