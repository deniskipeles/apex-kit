use crate::AppError;
use crate::DatabaseConnection;
use apexkit_core::auth::Claims;
use axum::{Extension, http::header, response::Response};

// Handler: Export Scripts
#[utoipa::path(
    get,
    path = "/api/v1/admin/export-scripts",
    responses((status = 200, description = "Scripts JSON"))
)]
pub async fn export_scripts_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let scripts = db
        .list_scripts()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let json_bytes = serde_json::to_vec_pretty(&scripts)
        .map_err(|e| AppError::UnknownError(format!("Serialization Error: {}", e)))?;

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"scripts.json\"",
        )
        .body(json_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}
