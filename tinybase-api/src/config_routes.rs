// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/config_routes.rs start here ===========================
use axum::{
    extract::{State, Json},
    http::StatusCode,
    Extension,
};
use serde::Deserialize;
use tinybase_core::auth::Claims;
use crate::{AppState, AppError};
use utoipa::ToSchema; // Added

#[derive(Deserialize, ToSchema)] // Added ToSchema
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
    State(state): State<AppState>,
    Json(payload): Json<SetConfigRequest>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Only admins can configure system secrets".into()));
    }

    // 1. Encrypt in memory
    let encrypted = state.vault.encrypt(&payload.value)
        .map_err(|e| AppError::UnknownError(e))?;

    // 2. Store in DB
    state.db.set_system_config(&payload.key, &encrypted).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/config_routes.rs ends here ===========================