use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde::{Deserialize};
use serde_json::{json, Value};
use apexkit_core::{auth::Claims, models::{Template, CreateTemplateReq}};
use crate::{AppState, AppError, DatabaseConnection}; // [FIX] Added DatabaseConnection

// DTO for Updates
#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateTemplateReq {
    pub content: String,
    pub script_id: Option<i64>,
}

// --- CRUD HANDLERS ---

#[utoipa::path(
    get,
    path = "/api/v1/admin/templates",
    responses((status = 200, body = Vec<Template>))
)]
pub async fn list_templates(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, // [FIX] Use contextual DB
    State(_state): State<AppState>
) -> Result<Json<Vec<Template>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    // [FIX] Use db (Sandbox/Tenant) instead of state.db (Root)
    let templates = db.list_templates().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    Ok(Json(templates))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/templates",
    request_body = CreateTemplateReq,
    responses((status = 200, body = Value))
)]
pub async fn create_template(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, // [FIX] Use contextual DB
    State(state): State<AppState>, // Keep state for css_cache
    Json(payload): Json<CreateTemplateReq>
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    // [FIX] Use db
    let id = db.create_template(payload).await
        .map_err(|e| AppError::UnknownError(format!("Failed to create template: {}", e)))?;
    
    // Clear global CSS cache (this is fine to be global as styles might be shared or recompiled)
    {
        let mut cache = state.css_cache.write().await;
        *cache = String::new(); 
    }
        
    Ok(Json(json!({ "id": id })))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/templates/{id}",
    request_body = UpdateTemplateReq,
    responses((status = 200, body = Value))
)]
pub async fn update_template(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, // [FIX] Use contextual DB
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateTemplateReq>
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    // [FIX] Use db
    db.update_template(id, payload.content, payload.script_id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    {
        let mut cache = state.css_cache.write().await;
        *cache = String::new(); 
    }
        
    Ok(Json(json!({ "success": true })))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/templates/{id}",
    responses((status = 200, body = Value))
)]
pub async fn delete_template(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, // [FIX] Use contextual DB
    State(state): State<AppState>,
    Path(id): Path<i64>
) -> Result<Json<Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    // [FIX] Use db
    db.delete_template(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    {
        let mut cache = state.css_cache.write().await;
        *cache = String::new();
    }

    Ok(Json(json!({ "success": true })))
}