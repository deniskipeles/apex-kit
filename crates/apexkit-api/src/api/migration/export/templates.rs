use crate::AppError;
use crate::DatabaseConnection;
use apexkit_core::auth::Claims;
use axum::{Extension, http::header, response::Response};

// Handler: Export Templates
#[utoipa::path(
    get,
    path = "/api/v1/admin/export-templates",
    responses((status = 200, description = "Templates JSON"))
)]
pub async fn export_templates_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let templates = db
        .list_templates()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let json_bytes = serde_json::to_vec_pretty(&templates)
        .map_err(|e| AppError::UnknownError(format!("Serialization Error: {}", e)))?;

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"templates.json\"",
        )
        .body(json_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}
