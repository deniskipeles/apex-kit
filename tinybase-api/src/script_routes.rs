// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/script_routes.rs ===========================
use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde_json::{json, Value};
use tinybase_core::{auth::Claims, script_models::{Script, CreateScriptReq}};
use crate::{AppState, AppError};

// --- CRUD ---

#[utoipa::path(
    get, 
    path = "/api/v1/admin/scripts", 
    responses((status = 200, body = Vec<Script>))
)]
pub async fn list_scripts(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>
) -> Result<Json<Vec<Script>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let scripts = state.db.list_scripts().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(scripts))
}

#[utoipa::path(
    post, 
    path = "/api/v1/admin/scripts", 
    request_body = CreateScriptReq, 
    responses((status = 200, body = Value))
)]
pub async fn create_script(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Json(payload): Json<CreateScriptReq>
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let id = state.db.create_script(payload).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

#[utoipa::path(
    delete, 
    path = "/api/v1/admin/scripts/{id}", 
    responses((status = 200, body = Value))
)]
pub async fn delete_script(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Path(id): Path<i64>
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    state.db.delete_script(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(json!({ "success": true })))
}

// --- RUNNER ---

#[utoipa::path(
    post, 
    path = "/api/v1/run/{script_name}", 
    request_body = Value,
    responses((status = 200, body = Value))
)]
pub async fn run_script(
    State(state): State<AppState>,
    Path(script_name): Path<String>,
    Json(payload): Json<Value>, // Input variables
) -> Result<Json<Value>, AppError> {
    
    // 1. Fetch Script
    let script = state.db.get_script_by_name(&script_name).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Script not found".into()))?;

    if !script.active {
        return Err(AppError::Forbidden("Script is inactive".into()));
    }

    // 2. Run
    let result = state.script_engine.run_script(&script.code, payload, state.db.clone()).await
        .map_err(|e| AppError::UnknownError(format!("Script Error: {}", e)))?;

    Ok(Json(result))
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/script_routes.rs ends here ===========================