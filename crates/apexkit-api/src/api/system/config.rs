use crate::{AppError, AppState, DatabaseConnection, utils::trigger_scope_reload};
use apexkit_core::{auth::Claims, models::ConfigItem, realtime::EventScope};
use axum::{
    Extension,
    extract::{Json, Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct SetConfigRequest {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub encrypt: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/config",
    responses((status = 200, body = Vec<ConfigItem>))
)]
pub async fn list_configs(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<Vec<ConfigItem>>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let configs = db
        .list_configs()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(configs))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/config",
    request_body = SetConfigRequest,
    responses((status = 204, description = "Configuration updated"))
)]
pub async fn set_config(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Json(payload): Json<SetConfigRequest>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Only admins".into()));
    }

    let json_val;

    if payload.encrypt {
        let encrypted = state
            .vault
            .encrypt(&payload.value)
            .map_err(AppError::UnknownError)?;
        json_val =
            serde_json::to_value(&encrypted).map_err(|e| AppError::UnknownError(e.to_string()))?;
    } else {
        json_val = serde_json::Value::String(payload.value);
    }

    db.set_config(&payload.key, &json_val, payload.encrypt)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [CRITICAL CACHE FIX]
    // If a policy or security setting changes, instantly rebuild the GraphQL Schema
    if payload.key.starts_with("policy_") || payload.key == "security" {
        let state_clone = state.clone();
        let scope_clone = scope.map(|s| s.0).unwrap_or(EventScope::Root);
        tokio::spawn(async move {
            trigger_scope_reload(state_clone, scope_clone).await;
        });
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/config/{key}",
    responses((status = 204, description = "Configuration deleted"))
)]
pub async fn delete_config(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Path(key): Path<String>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Only admins".into()));
    }

    db.delete_config(&key)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    if key.starts_with("policy_") || key == "security" {
        let state_clone = state.clone();
        let scope_clone = scope.map(|s| s.0).unwrap_or(EventScope::Root);
        tokio::spawn(async move {
            trigger_scope_reload(state_clone, scope_clone).await;
        });
    }

    Ok(StatusCode::NO_CONTENT)
}
