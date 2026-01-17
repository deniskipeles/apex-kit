use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde::Deserialize;
use serde_json::{json, Value};
use apexkit_core::{auth::Claims, script_models::{Script, CreateScriptReq}, Db};
use crate::{AppState, AppError, DatabaseConnection};
use std::sync::Arc;
use tracing::info;
use apexkit_core::realtime::EventScope;
use crate::BaseUrl;

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
}

// --- CRUD HANDLERS ---

#[utoipa::path(get, path = "/api/v1/admin/scripts", responses((status = 200, body = Vec<Script>)))]
pub async fn list_scripts(
    Extension(claims): Extension<Claims>, 
    DatabaseConnection(db): DatabaseConnection, 
    State(_state): State<AppState>
) -> Result<Json<Vec<Script>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let scripts = db.list_scripts().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(scripts))
}

#[utoipa::path(post, path = "/api/v1/admin/scripts", request_body = CreateScriptReq, responses((status = 200, body = Value)))]
pub async fn create_script(
    Extension(claims): Extension<Claims>, 
    DatabaseConnection(db): DatabaseConnection, 
    State(_state): State<AppState>, 
    Json(payload): Json<CreateScriptReq>
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let id = db.create_script(payload).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

#[utoipa::path(delete, path = "/api/v1/admin/scripts/{id}", responses((status = 200, body = Value)))]
pub async fn delete_script(
    Extension(claims): Extension<Claims>, 
    DatabaseConnection(db): DatabaseConnection, 
    State(_state): State<AppState>, 
    Path(path): Path<IdPath> // FIX: Use struct
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    db.delete_script(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
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
    scope: EventScope 
) -> Result<Json<Value>, AppError> {
    info!("[ScriptRunner] Running '{}' in {}", script_name, source);
    
    let script = db.get_script_by_name(&script_name).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Script not found".into()))?;

    if !script.active {
        return Err(AppError::Forbidden("Script is inactive".into()));
    }

    let result = state.script_engine.run_script(
        &script.code, 
        payload, 
        Arc::new(state.clone()), // Pass AppState
        base_url,
        scope
    ).await.map_err(|e| AppError::UnknownError(format!("Script Execution Error: {}", e)))?;

    Ok(Json(result))
}

// --- PUBLIC HANDLERS ---

#[utoipa::path(post, path = "/api/v1/run/{script_name}", request_body = Value, responses((status = 200, body = Value)))]
pub async fn run_script(
    BaseUrl(base_url): BaseUrl,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Path(path): Path<ScriptNamePath>, // FIX: Use struct
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    run_script_core(db, state, path.script_name, payload, "API", Some(base_url), event_scope).await
}

// Sandbox Handler
// With the Struct-based Path, this is actually identical to run_script now, 
// but kept distinct if you want different logging/logic later.
pub async fn run_sandbox_script(
    BaseUrl(base_url): BaseUrl,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Path(path): Path<ScriptNamePath>, // FIX: Use struct (auto-ignores session_id)
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    run_script_core(db, state, path.script_name, payload, "Sandbox", Some(base_url), event_scope).await
}