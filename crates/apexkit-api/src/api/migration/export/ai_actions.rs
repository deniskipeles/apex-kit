use crate::AppError;
use crate::DatabaseConnection;
use apexkit_core::auth::Claims;
use axum::{Extension, http::header, response::Response};
// use apexkit_core::ai_models::AiAction;
// use apexkit_core::script_models::Script;
// use apexkit_core::models::Template;

// Handler: Export AI Actions
#[utoipa::path(
    get,
    path = "/api/v1/admin/export-ai-actions",
    responses((status = 200, description = "AI Actions JSON"))
)]
pub async fn export_ai_actions_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let actions = db
        .list_ai_actions()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let json_bytes = serde_json::to_vec_pretty(&actions)
        .map_err(|e| AppError::UnknownError(format!("Serialization Error: {}", e)))?;

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"ai_actions.json\"",
        )
        .body(json_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}
