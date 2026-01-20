use axum::{
    extract::{State, Json, Path},
    http::StatusCode,
    Extension,
};
use serde::Deserialize;
use apexkit_core::{auth::Claims, models::ConfigItem};
use crate::{AppState, AppError, DatabaseConnection};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct SetConfigRequest {
    pub key: String,   
    pub value: String, 
    #[serde(default)]
    pub encrypt: bool, // [NEW] Allow client to request encryption
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
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    let configs = db.list_configs().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
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
    Json(payload): Json<SetConfigRequest>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Only admins".into())); }

    let json_val;
    
    if payload.encrypt {
        let encrypted = state.vault.encrypt(&payload.value)
            .map_err(|e| AppError::UnknownError(e))?;
        json_val = serde_json::to_value(&encrypted).map_err(|e| AppError::UnknownError(e.to_string()))?;
    } else {
        // Store as raw JSON string or object? For generic config, string is safest.
        json_val = serde_json::Value::String(payload.value);
    }
    
    db.set_config(&payload.key, &json_val, payload.encrypt).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

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
    Path(key): Path<String>
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Only admins".into())); }
    
    db.delete_config(&key).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    Ok(StatusCode::NO_CONTENT)
}