// =========================== /teamspace/studios/this_studio/apex/apex-kit/apexkit-api/src/backup_routes.rs ===========================
use axum::{
    extract::{Multipart, State, Path}, 
    Extension, 
    Json
};
use serde::{Serialize, Deserialize};
use apexkit_core::auth::Claims;
use crate::{AppState, AppError};
use std::io::Write;
use crate::settings::BackupConfigDto;
use axum::response::Response;
use std::path::Path as FilePath; // Rename to avoid conflict with Axum Path
use chrono::Utc;
use axum::body::Body;
use utoipa::ToSchema;

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

#[utoipa::path(
    post,
    path = "/api/v1/admin/backup",
    responses((status = 200, description = "Backup started"))
)]
pub async fn trigger_backup_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // Fetch config to know destination
    let backup_setting = state.db.get_config("backups").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let config: BackupConfigDto = if let Some(val) = backup_setting {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        // Default to local if not configured
        BackupConfigDto { enabled: true, destination: "local".into(), ..Default::default() }
    };

    // Run in background
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::backup::perform_backup(state_clone.db.clone(), state_clone.vault.clone(), config).await {
            tracing::error!("Manual backup failed: {}", e);
        }
    });

    Ok(Json(serde_json::json!({ "message": "Backup job started in background." })))
}

#[derive(Serialize, ToSchema)]
pub struct BackupFile {
    pub name: String,
    pub size: u64,
    pub created: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/backups",
    responses((status = 200, body = Vec<BackupFile>))
)]
pub async fn list_backups_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
) -> Result<Json<Vec<BackupFile>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let backup_setting = state.db.get_config("backups").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let config: BackupConfigDto = if let Some(val) = backup_setting {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        BackupConfigDto::default()
    };

    let mut backups = Vec::new();

    if config.destination == "s3" {
        // S3 Listing logic here if implemented
    } else {
        // Local Listing
        let path = std::path::Path::new("storage/backups");
        if path.exists() {
            let entries = std::fs::read_dir(path).map_err(|e| AppError::UnknownError(e.to_string()))?;
            for entry in entries {
                if let Ok(e) = entry {
                    if let Ok(meta) = e.metadata() {
                        if meta.is_file() {
                            let created: chrono::DateTime<Utc> = meta.created().unwrap_or(std::time::SystemTime::now()).into();
                            backups.push(BackupFile {
                                name: e.file_name().to_string_lossy().to_string(),
                                size: meta.len(),
                                created: created.to_rfc3339(),
                            });
                        }
                    }
                }
            }
        }
    }
    
    // Sort desc by time
    backups.sort_by(|a, b| b.created.cmp(&a.created));

    Ok(Json(backups))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/backups/{filename}",
    responses((status = 200, description = "Backup File"))
)]
pub async fn download_backup_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<Response, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // Sanitize filename
    if filename.contains("..") || filename.contains("/") {
        return Err(AppError::InputValidation(validator::ValidationErrors::new()));
    }

    let path = std::path::Path::new("storage/backups").join(&*filename);
    if !path.exists() {
        return Err(AppError::NotFound("Backup not found".into()));
    }

    let bytes = std::fs::read(&path).map_err(|e| AppError::UnknownError(e.to_string()))?;

    Response::builder()
        .header("Content-Type", "application/gzip")
        .header("Content-Disposition", format!("attachment; filename=\"{}\"", filename))
        .body(Body::from(bytes))
        .map_err(|e| AppError::UnknownError(e.to_string()))
}

#[derive(Deserialize, ToSchema)]
pub struct RestoreRequest {
    pub filename: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/restore-file",
    request_body = RestoreRequest,
    responses((status = 200, description = "Restore triggered"))
)]
pub async fn restore_from_file_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Json(payload): Json<RestoreRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    // Path resolution logic (Local vs S3)
    let path = format!("storage/backups/{}", payload.filename); // Assuming local for now
    
    // Call the shared restore logic
    crate::backup::restore_backup(&path, false, state.db.clone(), state.vault.clone()).await
         .map_err(|e| AppError::UnknownError(e))?;

    // Trigger Restart
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        std::process::exit(0);
    });

    Ok(Json(serde_json::json!({ "message": "Restoring..." })))
}
// =========================== /teamspace/studios/this_studio/apex/apex-kit/apexkit-api/src/backup_routes.rs ends here ===========================