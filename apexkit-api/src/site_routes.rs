use axum::{
    extract::{Multipart},
    Extension,
    Json,
};
use serde::{Serialize, Deserialize};
use apexkit_core::{auth::Claims, realtime::EventScope};
use crate::{AppError};
use std::path::{PathBuf};
use std::fs;
use std::io::Cursor;
use walkdir::WalkDir;
use tracing::{info};
use axum::http::StatusCode;
use axum::extract::Query;
use crate::AppState;
use crate::State;

#[derive(Serialize, utoipa::ToSchema)]
pub struct SiteFile {
    pub path: String,
    pub size: u64,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DeleteSiteFileReq {
    pub path: String,
}

// [SCOPE AWARE] Helper to resolve public dir
pub fn get_public_dir(scope: &EventScope) -> PathBuf {
    match scope {
        EventScope::Root => PathBuf::from("storage/system/public"),
        EventScope::Tenant(id) => PathBuf::from(format!("storage/tenants/{}/public", id)),
        EventScope::Sandbox(id) => PathBuf::from(format!("storage/sandboxes/session_{}/public", id)),
        // Safety fallback, though usually unreachable if middleware works
        _ => PathBuf::from("storage/tmp"),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/site/deploy",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses((status = 200, description = "Site deployed"))
)]
pub async fn deploy_site_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>, // [CRITICAL] Access Root DB via State
    scope: Option<Extension<EventScope>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let public_dir = get_public_dir(&event_scope);

    // 1. Check Max Size Config (FROM ROOT DB)
    // We explicitly use state.db here, not a context-aware db connection
    let general_settings = state.db.get_config("general").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let max_mb = general_settings
        .and_then(|v| v.get("max_site_size_mb").and_then(|n| n.as_u64()))
        .unwrap_or(50); // Default 50MB if root hasn't set it

    let max_bytes = max_mb * 1024 * 1024;

    // 2. Process Upload
    while let Some(field) = multipart.next_field().await.map_err(|_| AppError::UnknownError("Multipart error".into()))? {
        if field.name() == Some("file") {
            let data = field.bytes().await.map_err(|_| AppError::UnknownError("Read failed".into()))?;
            
            // Enforce Root Limit
            if (data.len() as u64) > max_bytes {
                return Err(AppError::Validation(vec![
                    apexkit_core::validation::ValidationError::ConstraintViolation(
                        "file".into(), 
                        format!("Upload size ({} MB) exceeds the Root limit of {} MB.", (data.len() / 1024 / 1024), max_mb)
                    )
                ]));
            }

            // 3. Clear Existing Public Dir (Scoped)
            if public_dir.exists() {
                fs::remove_dir_all(&public_dir).map_err(|e| AppError::UnknownError(format!("Cleanup failed: {}", e)))?;
            }
            fs::create_dir_all(&public_dir).map_err(|e| AppError::UnknownError(format!("Create dir failed: {}", e)))?;

            // 4. Extract ZIP
            let cursor = Cursor::new(data);
            let mut archive = zip::ZipArchive::new(cursor).map_err(|e| AppError::UnknownError(format!("Invalid ZIP: {}", e)))?;

            for i in 0..archive.len() {
                let mut file = archive.by_index(i).unwrap();
                
                let outpath = match file.enclosed_name() {
                    Some(path) => public_dir.join(path),
                    None => continue,
                };

                if file.name().ends_with('/') {
                    fs::create_dir_all(&outpath).ok();
                } else {
                    if let Some(p) = outpath.parent() {
                        if !p.exists() { fs::create_dir_all(p).ok(); }
                    }
                    let mut outfile = fs::File::create(&outpath).map_err(|e| AppError::UnknownError(e.to_string()))?;
                    std::io::copy(&mut file, &mut outfile).map_err(|e| AppError::UnknownError(e.to_string()))?;
                }
            }

            info!("Site deployed to {:?} (Size Limit: {} MB)", public_dir, max_mb);
            return Ok(Json(serde_json::json!({ "success": true, "message": "Site deployed successfully" })));
        }
    }

    Err(AppError::UnknownError("No file provided".into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/site/files",
    responses((status = 200, body = Vec<SiteFile>))
)]
pub async fn list_site_files_handler(
    Extension(claims): Extension<Claims>,
    // [SCOPE AWARE]
    scope: Option<Extension<EventScope>>,
) -> Result<Json<Vec<SiteFile>>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let public_dir = get_public_dir(&event_scope);
    
    let mut files = Vec::new();

    if public_dir.exists() {
        for entry in WalkDir::new(&public_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                // Return path relative to public root (e.g. "index.html", "css/style.css")
                let path = entry.path().strip_prefix(&public_dir).unwrap().to_string_lossy().to_string();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                files.push(SiteFile { path, size });
            }
        }
    }

    Ok(Json(files))
}

// [NEW] Delete Site File Handler
#[utoipa::path(
    delete,
    path = "/api/v1/admin/site/files",
    params(
        ("path" = String, Query, description = "Relative path of file/folder to delete")
    ),
    responses((status = 204, description = "File deleted"))
)]
pub async fn delete_site_file_handler(
    Extension(claims): Extension<Claims>,
    scope: Option<Extension<EventScope>>,
    Query(params): Query<DeleteSiteFileReq>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let public_dir = get_public_dir(&event_scope);

    // Sanitize Path: Prevent traversal (../)
    let safe_path = params.path.trim_start_matches('/').replace("..", ""); 
    
    // Safety check: ensure we aren't deleting the root public dir via empty path
    if safe_path.is_empty() || safe_path == "." || safe_path == "./" {
         return Err(AppError::Forbidden("Cannot delete root directory".into()));
    }

    let target_path = public_dir.join(&safe_path);

    // Verify it is strictly inside public_dir
    if !target_path.starts_with(&public_dir) {
        return Err(AppError::Forbidden("Access denied: Path traversal detected".into()));
    }

    if !target_path.exists() {
        return Err(AppError::NotFound("File not found".into()));
    }

    if target_path.is_dir() {
        fs::remove_dir_all(target_path).map_err(|e| AppError::UnknownError(e.to_string()))?;
    } else {
        fs::remove_file(target_path).map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    Ok(StatusCode::NO_CONTENT)
}