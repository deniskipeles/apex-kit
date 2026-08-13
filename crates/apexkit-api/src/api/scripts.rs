use crate::BaseUrl;
use crate::{AppError, AppState, DatabaseConnection};
use apexkit_core::realtime::EventScope;
use apexkit_core::{Db, auth::Claims, models::script::CreateScriptReq};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{
    Extension,
    extract::{Json, Path, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

// --- PATH DTOS ---
// These allow Axum to pick specific params by name and ignore
// parent params (like tenant_id or session_id) automatically.

#[derive(Deserialize)]
pub struct IdPath {
    pub id: i64,
}

#[derive(Deserialize)]
pub struct ScriptNamePath {
    pub script_name: String,
    pub subpath: Option<String>,
}

// --- CRUD HANDLERS ---

#[utoipa::path(get, path = "/api/v1/admin/scripts", responses((status = 200, body = Value)))]
pub async fn list_scripts(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // 1. Fetch Local Scripts (For this Tenant/Sandbox or Root)
    let local_scripts = db
        .list_scripts()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let mut shared = Vec::new();
    let mut root_total = 0;
    let mut transparency_enabled = false;

    let current_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    // 2. If we are inside a Tenant/Sandbox, inject Root Scripts
    if !matches!(current_scope, EventScope::Root) {
        let root_scripts = state.db.list_scripts().await.unwrap_or_default();
        root_total = root_scripts.len();

        let sec_config = state.db.get_config("security").await.unwrap_or_default();
        if let Some(val) = sec_config {
            transparency_enabled = val
                .get("tenant_transparency")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        }

        for s in root_scripts {
            if s.visibility == "public" {
                shared.push(s.clone()); // Public scripts share code freely
            } else if transparency_enabled {
                // Transparency Mode: Expose existence, redact the code
                let mut redacted = s.clone();
                redacted.code = "// [TRANSPARENCY MODE]\n// Code redacted by Host Provider to protect secrets.\n// Script is active and running in the Root context.".to_string();
                shared.push(redacted);
            }
        }
    }

    Ok(Json(json!({
        "local": local_scripts,
        "shared": shared,
        "root_total": root_total,
        "transparency_enabled": transparency_enabled
    })))
}

#[utoipa::path(post, path = "/api/v1/admin/scripts", request_body = CreateScriptReq, responses((status = 200, body = Value)))]
pub async fn create_script(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    State(_state): State<AppState>,
    Json(payload): Json<CreateScriptReq>,
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    let id = db
        .create_script(payload)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

#[utoipa::path(delete, path = "/api/v1/admin/scripts/{id}", responses((status = 200, body = Value)))]
pub async fn delete_script(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    State(_state): State<AppState>,
    Path(path): Path<IdPath>, // FIX: Use struct
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    db.delete_script(path.id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "success": true })))
}

// --- EXECUTION CORE ---

async fn run_script_core(
    db: Arc<dyn Db>,
    state: AppState,
    script_name: String,
    payload: Value,
    source: &str,
    base_url: Option<String>,
    scope: EventScope,
    headers: Option<HashMap<String, String>>,
    method: Option<String>,
    url: Option<String>,
) -> Result<Response, AppError> {
    info!("[ScriptRunner] Running '{}' in {}", script_name, source);

    let script = db
        .get_script_by_name(&script_name)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Script not found".into()))?;

    if !script.active {
        return Err(AppError::Forbidden("Script is inactive".into()));
    }

    let context = Arc::new(crate::ScopedScriptContext {
        state: state.clone(),
        scope: scope.clone(),
    });

    let result = state
        .script_engine
        .run_script(
            &script.code,
            payload,
            context, // Pass AppState
            base_url,
            headers,
            method,
            url,
        )
        .await
        .map_err(|e| AppError::UnknownError(format!("Script Execution Error: {}", e)))?;

    // THE FIX: Unpack the intercepted Response object and apply the correct HTTP Status
    if let Some(obj) = result.as_object() {
        if obj
            .get("__is_apex_response")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let status_code = obj.get("status").and_then(|v| v.as_u64()).unwrap_or(200) as u16;
            let body = obj.get("body").cloned().unwrap_or(serde_json::Value::Null);

            let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK);
            return Ok((status, Json(body)).into_response());
        }
    }

    // Default return
    Ok(Json(result).into_response())
}

// --- PUBLIC HANDLERS ---
// --- HELPER ---
fn headers_to_map(headers: &HeaderMap) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            map.insert(k.to_string(), val.to_string());
        }
    }
    map
}

#[utoipa::path(post, path = "/api/v1/run/{script_name}", request_body = Value, responses((status = 200, body = Value)))]
pub async fn run_script(
    BaseUrl(base_url): BaseUrl,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: HeaderMap,
    Path(path): Path<ScriptNamePath>,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let headers_map = headers_to_map(&headers);

    let payload = if body.is_empty() {
        json!({})
    } else {
        let body_str = String::from_utf8_lossy(&body);
        serde_json::from_str(&body_str).unwrap_or_else(|_| json!(body_str.to_string()))
    };

    let full_url = format!("{}{}", base_url, uri.to_string());

    run_script_core(
        db,
        state,
        path.script_name,
        payload,
        "API",
        Some(base_url.clone()),
        event_scope,
        Some(headers_map),
        Some(method.to_string()),
        Some(full_url),
    )
    .await
}

pub async fn run_sandbox_script(
    BaseUrl(base_url): BaseUrl,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: HeaderMap,
    Path(path): Path<ScriptNamePath>,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let headers_map = headers_to_map(&headers);

    let payload = if body.is_empty() {
        json!({})
    } else {
        let body_str = String::from_utf8_lossy(&body);
        serde_json::from_str(&body_str).unwrap_or_else(|_| json!(body_str.to_string()))
    };

    let full_url = format!("{}{}", base_url, uri.to_string());

    run_script_core(
        db,
        state,
        path.script_name,
        payload,
        "Sandbox",
        Some(base_url.clone()),
        event_scope,
        Some(headers_map),
        Some(method.to_string()),
        Some(full_url),
    )
    .await
}
