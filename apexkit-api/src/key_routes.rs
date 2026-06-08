// apexkit-api/src/key_routes.rs
use crate::{AppError, DatabaseConnection};
use apexkit_core::{auth::Claims, models::ApiKey, realtime::EventScope};
use axum::{
    Extension,
    extract::{Json, Path},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct CreateKeyReq {
    pub name: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default = "default_scope")]
    pub scope: String, // "root", "tenant:id", "*"
    #[serde(default)]
    pub bypass_cors: bool,
}

fn default_scope() -> String {
    "root".to_string()
}
fn default_role() -> String {
    "admin".to_string()
}

#[derive(Serialize, ToSchema)]
pub struct CreateKeyResponse {
    pub key: String,
    pub info: ApiKey,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateKeyReq {
    pub name: Option<String>,
    pub role: Option<String>,
    pub scope: Option<String>,
    pub bypass_cors: Option<bool>,
}

#[derive(Deserialize)]
pub struct KeyPath {
    pub id: i64,
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/keys",
    responses((status = 200, body = Vec<ApiKey>))
)]
pub async fn list_keys(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<Vec<ApiKey>>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    let keys = db
        .list_api_keys()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(keys))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/keys",
    request_body = CreateKeyReq,
    responses((status = 201, body = CreateKeyResponse))
)]
pub async fn create_key(
    Extension(claims): Extension<Claims>,
    scope: Option<Extension<EventScope>>, // [ADDED] Extract the active request scope
    DatabaseConnection(db): DatabaseConnection,
    Json(payload): Json<CreateKeyReq>,
) -> Result<Json<CreateKeyResponse>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // [FIXED] Override the scope dynamically if requested under a Tenant or Sandbox context
    let event_scope = scope.map(|Extension(s)| s).unwrap_or(EventScope::Root);
    let final_scope = match event_scope {
        EventScope::Tenant(id) => format!("tenant:{}", id),
        EventScope::Sandbox(id) => format!("sandbox:{}", id),
        _ => payload.scope.clone(), // Allow Root Admins to specify the scope explicitly
    };

    let (key, info) = db
        .create_api_key(
            &payload.name,
            &payload.role,
            &final_scope, // [FIXED]
            payload.bypass_cors,
        )
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(CreateKeyResponse { key, info }))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/keys/{id}",
    request_body = UpdateKeyReq,
    responses((status = 200, body = Value))
)]
pub async fn update_key(
    Extension(claims): Extension<Claims>,
    scope: Option<Extension<EventScope>>, // [ADDED] Extract the active request scope
    DatabaseConnection(db): DatabaseConnection,
    Path(params): Path<KeyPath>,
    Json(payload): Json<UpdateKeyReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // [FIXED] Force-override the scope update if requested under a Tenant or Sandbox context
    let event_scope = scope.map(|Extension(s)| s).unwrap_or(EventScope::Root);
    let final_scope = match event_scope {
        EventScope::Tenant(id) => Some(format!("tenant:{}", id)),
        EventScope::Sandbox(id) => Some(format!("sandbox:{}", id)),
        _ => payload.scope.clone(), // Allow Root Admins to update the scope explicitly
    };

    db.update_api_key(
        params.id,
        payload.name,
        payload.role,
        final_scope, // [FIXED]
        payload.bypass_cors,
    )
    .await
    .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(json!({ "success": true })))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/keys/{id}",
    responses((status = 204, description = "Key deleted"))
)]
pub async fn delete_key(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    Path(params): Path<KeyPath>, // [FIXED] Switched from scalar Path<i64> to struct Path<KeyPath>
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    db.delete_api_key(params.id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}
