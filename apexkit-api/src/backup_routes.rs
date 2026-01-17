use axum::{extract::{Multipart, State}, Extension, Json};
use apexkit_core::auth::Claims;
use crate::{AppState, AppError, DatabaseConnection};
use std::io::Write;

// [NEW] Handler: Restore from Upload
#[utoipa::path(
    post,
    path = "/api/v1/admin/restore",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses((status = 200, description = "Restore successful, server restarting"))
)]
pub async fn restore_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // 1. Save uploaded file to temp
    let mut file_path = String::new();
    while let Some(field) = multipart.next_field().await.unwrap() {
        if field.name() == Some("file") {
            let data = field.bytes().await.unwrap();
            let temp_path = format!("storage/tmp/restore_upload_{}.tar.gz", uuid::Uuid::new_v4());
            let mut file = std::fs::File::create(&temp_path).map_err(|e| AppError::UnknownError(e.to_string()))?;
            file.write_all(&data).map_err(|e| AppError::UnknownError(e.to_string()))?;
            file_path = temp_path;
        }
    }

    if file_path.is_empty() {
        return Err(AppError::InputValidation(validator::ValidationErrors::new()));
    }

    // 2. Run Restore Logic
    crate::backup::restore_backup(&file_path, false, state.db.clone(), state.vault.clone()).await
        .map_err(|e| AppError::UnknownError(e))?;

    // 3. Trigger Shutdown/Restart
    // In a process manager environment (Systemd/Docker), exiting causes a restart.
    // We spawn a thread to exit after sending response.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        std::process::exit(0);
    });

    Ok(Json(serde_json::json!({ "message": "Restoration successful. Server restarting..." })))
}