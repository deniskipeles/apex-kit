use axum::{
    extract::{Multipart},
    Extension,
    Json,
};
use serde::Serialize;
use apexkit_core::{auth::Claims, realtime::EventScope};
use crate::{AppError};
use std::path::{PathBuf};
use std::fs;
use std::io::Cursor;
use walkdir::WalkDir;
use tracing::{info};

#[derive(Serialize, utoipa::ToSchema)]
pub struct SiteFile {
    pub path: String,
    pub size: u64,
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
    // [SCOPE AWARE] Capture scope from middleware
    scope: Option<Extension<EventScope>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let public_dir = get_public_dir(&event_scope);

    // 1. Process Upload
    while let Some(field) = multipart.next_field().await.map_err(|_| AppError::UnknownError("Multipart error".into()))? {
        if field.name() == Some("file") {
            let data = field.bytes().await.map_err(|_| AppError::UnknownError("Read failed".into()))?;
            
            // 2. Clear Existing Public Dir (Scoped)
            if public_dir.exists() {
                fs::remove_dir_all(&public_dir).map_err(|e| AppError::UnknownError(format!("Cleanup failed: {}", e)))?;
            }
            fs::create_dir_all(&public_dir).map_err(|e| AppError::UnknownError(format!("Create dir failed: {}", e)))?;

            // 3. Extract ZIP
            let cursor = Cursor::new(data);
            let mut archive = zip::ZipArchive::new(cursor).map_err(|e| AppError::UnknownError(format!("Invalid ZIP: {}", e)))?;

            for i in 0..archive.len() {
                let mut file = archive.by_index(i).unwrap();
                
                // Sanitize path to prevent Zip Slip
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

            info!("Site deployed to {:?}", public_dir);
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