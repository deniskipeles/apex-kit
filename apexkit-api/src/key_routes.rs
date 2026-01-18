// apexkit-api/src/key_routes.rs
use axum::{
    extract::{Path, State, Json},
    Extension,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use apexkit_core::{auth::Claims, models::ApiKey};
use crate::{AppState, AppError, DatabaseConnection};
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

fn default_scope() -> String { "root".to_string() }
fn default_role() -> String { "admin".to_string() }

#[derive(Serialize, ToSchema)]
pub struct CreateKeyResponse {
    pub key: String,
    pub info: ApiKey,
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
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let keys = db.list_api_keys().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
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
    DatabaseConnection(db): DatabaseConnection,
    Json(payload): Json<CreateKeyReq>,
) -> Result<Json<CreateKeyResponse>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    let (key, info) = db.create_api_key(&payload.name, &payload.role, &payload.scope, payload.bypass_cors).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(CreateKeyResponse { key, info }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/keys/{id}",
    responses((status = 204, description = "Key deleted"))
)]
pub async fn delete_key(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    db.delete_api_key(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}