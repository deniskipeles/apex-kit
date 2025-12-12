// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/script_routes.rs ===========================
use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde_json::{json, Value};
use tinybase_core::{auth::Claims, script_models::{Script, CreateScriptReq}, Db};
use crate::{AppState, AppError, DatabaseConnection};
use std::sync::Arc;
use tracing::info;

// --- CRUD HANDLERS ---
#[utoipa::path(get, path = "/api/v1/admin/scripts", responses((status = 200, body = Vec<Script>)))]
pub async fn list_scripts(Extension(claims): Extension<Claims>, DatabaseConnection(db): DatabaseConnection, State(_state): State<AppState>) -> Result<Json<Vec<Script>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let scripts = db.list_scripts().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(scripts))
}

#[utoipa::path(post, path = "/api/v1/admin/scripts", request_body = CreateScriptReq, responses((status = 200, body = Value)))]
pub async fn create_script(Extension(claims): Extension<Claims>, DatabaseConnection(db): DatabaseConnection, State(_state): State<AppState>, Json(payload): Json<CreateScriptReq>) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let id = db.create_script(payload).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

#[utoipa::path(delete, path = "/api/v1/admin/scripts/{id}", responses((status = 200, body = Value)))]
pub async fn delete_script(Extension(claims): Extension<Claims>, DatabaseConnection(db): DatabaseConnection, State(_state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    db.delete_script(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "success": true })))
}

// --- EXECUTION CORE ---

async fn run_script_core(
    db: Arc<dyn Db>,
    state: AppState,
    script_name: String,
    payload: Value,
    source: &str,
) -> Result<Json<Value>, AppError> {
    info!("[ScriptRunner] Running '{}' in {}", script_name, source);
    
    let script = db.get_script_by_name(&script_name).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Script not found".into()))?;

    if !script.active {
        return Err(AppError::Forbidden("Script is inactive".into()));
    }

    let result = state.script_engine.run_script(&script.code, payload, db, state.embedder.clone(), state.vector_provider.clone()).await
        .map_err(|e| AppError::UnknownError(format!("Script Execution Error: {}", e)))?;

    Ok(Json(result))
}

// --- PUBLIC HANDLERS ---

#[utoipa::path(post, path = "/api/v1/run/{script_name}", request_body = Value, responses((status = 200, body = Value)))]
pub async fn run_script(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Path(script_name): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    // Fix: Use db.0 is NOT needed if DatabaseConnection implements logic to return Arc directly, 
    // but based on my previous fix we access the inner tuple field via `.0` if struct is `pub struct DatabaseConnection(pub Arc<dyn Db>);`
    // However, in the previous turn, we destructured it in function args: `DatabaseConnection(db)`. 
    // So `db` IS the `Arc<dyn Db>`.
    run_script_core(db, state, script_name, payload, "Production").await
}

// Sandbox Handler: 2 Params
pub async fn run_sandbox_script(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Path((_session_id, script_name)): Path<(String, String)>, // Expects 2 args
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    run_script_core(db, state, script_name, payload, "Sandbox").await
}