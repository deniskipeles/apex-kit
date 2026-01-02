use axum::{
    extract::{State, Json},
    http::StatusCode,
    Extension,
};
use serde::Deserialize;
use apexkit_core::auth::Claims;
use crate::{AppState, AppError, DatabaseConnection}; // Added DatabaseConnection
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct SetConfigRequest {
    pub key: String,   
    pub value: String, 
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/config",
    request_body = SetConfigRequest,
    responses(
        (status = 204, description = "Configuration updated successfully"),
        (status = 403, description = "Admin privileges required")
    )
)]
pub async fn set_config(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, // <--- FIXED: Tenant/Sandbox Aware
    State(state): State<AppState>,              // <--- Needed for Vault (Encryption)
    Json(payload): Json<SetConfigRequest>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Only admins can configure system secrets".into()));
    }

    // 1. Encrypt in memory using Global Master Key
    let encrypted = state.vault.encrypt(&payload.value)
        .map_err(|e| AppError::UnknownError(e))?;

    // Note: We wrap EncryptedValue in serde_json::to_value to match signature
    let json_val = serde_json::to_value(&encrypted).map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 2. Store in the Context-Specific DB (Root, Tenant, or Sandbox)
    db.set_config(&payload.key, &json_val, true).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}