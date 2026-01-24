use axum::{
    extract::{Multipart, State, Path}, 
    Extension, 
    Json
};
use serde::{Serialize, Deserialize};
use apexkit_core::{auth::Claims, realtime::EventScope};
use crate::{AppState, AppError, DatabaseConnection}; // [FIX] Use DatabaseConnection extractor
use std::io::Write;
use crate::settings::BackupConfigDto;
use axum::response::Response;
use chrono::Utc;
use axum::body::Body;
use utoipa::ToSchema;
use apexkit_core::security::EncryptedValue;

// Restore from Upload (Scope Aware)
#[utoipa::path(
    post,
    path = "/api/v1/admin/restore",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses((status = 200, description = "Restore successful, server restarting"))
)]
pub async fn restore_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    
    // Determine restore path based on scope
    let base_dir = match &event_scope {
        EventScope::Root => "storage/tmp".to_string(),
        EventScope::Tenant(id) => format!("storage/tenants/{}/tmp", id),
        EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/tmp", id),
        _ => return Err(AppError::Forbidden("Invalid scope".into())),
    };
    std::fs::create_dir_all(&base_dir).map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 1. Save uploaded file
    let mut file_path = String::new();
    while let Some(field) = multipart.next_field().await.unwrap() {
        if field.name() == Some("file") {
            let data = field.bytes().await.unwrap();
            let temp_path = format!("{}/restore_upload_{}.tar.gz", base_dir, uuid::Uuid::new_v4());
            let mut file = std::fs::File::create(&temp_path).map_err(|e| AppError::UnknownError(e.to_string()))?;
            file.write_all(&data).map_err(|e| AppError::UnknownError(e.to_string()))?;
            file_path = temp_path;
        }
    }

    if file_path.is_empty() {
        return Err(AppError::InputValidation(validator::ValidationErrors::new()));
    }

    // 2. Run Restore Logic
    // We pass the scope to restore_backup so it knows which DBs to swap
    let db = match &event_scope {
        EventScope::Root => state.db.clone(),
        EventScope::Tenant(id) => state.tenant_manager.get_tenant(id.clone()).await.map_err(|e| AppError::UnknownError(e))?,
        EventScope::Sandbox(id) => state.sandbox_manager.get_sandbox(id).await.map_err(|e| AppError::UnknownError(e))?,
        _ => return Err(AppError::Forbidden("Invalid scope".into())),
    };

    crate::backup::restore_backup(&file_path, false, db, state.vault.clone(), event_scope.clone()).await
        .map_err(|e| AppError::UnknownError(e))?;

    // 3. Trigger Restart (Only if Root, otherwise just ack)
    if matches!(event_scope, EventScope::Root) {
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            std::process::exit(0);
        });
        Ok(Json(serde_json::json!({ "message": "Restoration successful. Server restarting..." })))
    } else {
        Ok(Json(serde_json::json!({ "message": "Restoration successful. Tenant data updated." })))
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/backup",
    responses((status = 200, description = "Backup started"))
)]
pub async fn trigger_backup_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, 
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    // 1. Fetch Config from Scoped DB
    let backup_setting = db.get_config("backups").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let config: BackupConfigDto = if let Some(val) = backup_setting {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        BackupConfigDto { enabled: true, destination: "local".into(), ..Default::default() }
    };

    // 2. Security Check for Non-Root Local Backup
    if !matches!(event_scope, EventScope::Root) && config.destination == "local" {
        // [FIX] Check Independent Root Config Key
        let allow_val = state.db.get_config("ALLOW_NON_ROOT_BACKUP").await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
            
        let allowed = allow_val.and_then(|v| v.as_str().map(|s| s == "true")).unwrap_or(false);
        
        if !allowed {
            return Err(AppError::Forbidden("Local backups disabled for tenants by Root Policy. Configure S3.".into()));
        }
    }

    // 3. Run Background Job
    let state_clone = state.clone();
    let db_clone = db.clone(); 

    tokio::spawn(async move {
        if let Err(e) = crate::backup::perform_backup(db_clone, state_clone.vault.clone(), config, event_scope).await {
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
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>, 
    scope: Option<Extension<EventScope>>,
) -> Result<Json<Vec<BackupFile>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    
    let backup_setting = db.get_config("backups").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let config: BackupConfigDto = if let Some(val) = backup_setting {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        BackupConfigDto::default()
    };

    let mut backups = Vec::new();

    if config.destination == "s3" {
        // [FIX] S3 Listing Implementation
        // We must reconstruct the S3 backend dynamically because `state.storage` points to `uploads/` logic, 
        // but backups use different credentials/buckets potentially defined in `config`
        
        let storage_settings = db.get_config("storage").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        if let Some(val) = storage_settings {
             let s_conf: crate::settings::StorageConfigDto = serde_json::from_value(val).unwrap_or_default();
             if s_conf.s3.enabled {
                 let secret = if let Some(enc_str) = s_conf.s3.secret_key {
                     let enc: EncryptedValue = serde_json::from_str(&enc_str).map_err(|_| AppError::UnknownError("Bad secret format".into()))?;
                     state.vault.decrypt(&enc).map_err(|_| AppError::UnknownError("Decrypt fail".into()))?
                 } else { String::new() };

                 let s3 = apexkit_core::storage::S3Storage::new_with_creds(
                    &s_conf.s3.bucket,
                    &s_conf.s3.region,
                    &s_conf.s3.endpoint,
                    "",
                    &s_conf.s3.access_key,
                    &secret
                 ).await;
                 
                 // Determine Prefix based on scope
                 let prefix = match &event_scope {
                    EventScope::Root => "backups/".to_string(),
                    EventScope::Tenant(id) => format!("tenants/{}/backups/", id),
                    EventScope::Sandbox(id) => format!("sandboxes/{}/backups/", id),
                    _ => return Ok(Json(vec![])),
                 };
                 
                 // Call list_prefix on S3 impl directly (StorageBackend trait)
                 use apexkit_core::storage::StorageBackend;
                 let raw_files = s3.list_prefix(&prefix).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
                 
                 for (key, size, time) in raw_files {
                     // Filter out "directories" or keys that ARE the prefix
                     if key == prefix || key.ends_with('/') { continue; }
                     
                     let name = key.strip_prefix(&prefix).unwrap_or(&key).to_string();
                     backups.push(BackupFile {
                         name,
                         size,
                         created: time // S3 time string might need parsing if you want strict format, or pass as is
                     });
                 }
             }
        }
    } else {
        // [FIX] Check Permission if not root
        if !matches!(event_scope, EventScope::Root) {
            let allow_val = state.db.get_config("ALLOW_NON_ROOT_BACKUP").await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;
            let allowed = allow_val.and_then(|v| v.as_str().map(|s| s == "true")).unwrap_or(false);
            
            if !allowed { return Ok(Json(vec![])); }
        }

        // Determine Path
        let backup_dir = match &event_scope {
            EventScope::Root => "storage/backups".to_string(),
            EventScope::Tenant(id) => format!("storage/tenants/{}/backups", id),
            EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/backups", id),
            _ => return Ok(Json(vec![])),
        };

        let path = std::path::Path::new(&backup_dir);
        if path.exists() {
            let entries = std::fs::read_dir(path).map_err(|e| AppError::UnknownError(e.to_string()))?;
            for entry in entries {
                if let Ok(e) = entry {
                    if let Ok(meta) = e.metadata() {
                        if meta.is_file() && e.file_name().to_string_lossy().ends_with(".tar.gz") {
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
    
    // Sort by creation date (desc)
    // Basic string compare works for ISO dates, S3 dates might differ format so this is best effort
    backups.sort_by(|a, b| b.created.cmp(&a.created));

    Ok(Json(backups))
}

// Struct to handle path params robustly (ignores parent params like tenant_id)
#[derive(Deserialize, ToSchema)]
pub struct BackupDownloadPath {
    pub filename: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/backups/{filename}",
    responses((status = 200, description = "Backup File"))
)]
pub async fn download_backup_handler(
    Extension(claims): Extension<Claims>,
    State(_state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    // [FIX] Use struct extractor instead of Path<String>
    Path(path_params): Path<BackupDownloadPath>, 
) -> Result<Response, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let filename = path_params.filename;

    if filename.contains("..") || filename.contains("/") {
        return Err(AppError::InputValidation(validator::ValidationErrors::new()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    let backup_dir = match &event_scope {
        EventScope::Root => "storage/backups".to_string(),
        EventScope::Tenant(id) => format!("storage/tenants/{}/backups", id),
        EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/backups", id),
        _ => return Err(AppError::NotFound("Invalid scope".into())),
    };

    let path = std::path::Path::new(&backup_dir).join(&filename);
    
    if !path.exists() {
        return Err(AppError::NotFound("Backup not found locally".into()));
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
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Json(payload): Json<RestoreRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    let backup_dir = match &event_scope {
        EventScope::Root => "storage/backups".to_string(),
        EventScope::Tenant(id) => format!("storage/tenants/{}/backups", id),
        EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/backups", id),
        _ => return Err(AppError::NotFound("Invalid scope".into())),
    };

    let path = format!("{}/{}", backup_dir, payload.filename);
    
    crate::backup::restore_backup(&path, false, db, state.vault.clone(), event_scope).await
         .map_err(|e| AppError::UnknownError(e))?;

    // Only restart if root
    if path.contains("storage/backups/") { // Rudimentary check for root
         tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            std::process::exit(0);
        });
    }

    Ok(Json(serde_json::json!({ "message": "Restoration successful." })))
}