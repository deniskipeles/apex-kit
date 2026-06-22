use crate::AppError;
use crate::AppState;
use crate::DatabaseConnection;
use apexkit_core::Db;
use apexkit_core::models::CreateActionReq;
use apexkit_core::models::CreateScriptReq;
use apexkit_core::models::CreateTemplateReq;
use apexkit_core::{auth::Claims, realtime::EventScope};
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{Extension, Json, extract::Multipart};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::error;
use tracing::info;
use walkdir::WalkDir;

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
        EventScope::Sandbox(id) => {
            PathBuf::from(format!("storage/sandboxes/session_{}/public", id))
        }
        // Safety fallback, though usually unreachable if middleware works
        _ => PathBuf::from("storage/tmp"),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/site/deploy",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses((status = 200, description = "Full App Bundle deployed"))
)]
pub async fn deploy_site_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    DatabaseConnection(db): DatabaseConnection, // The scoped DB
    scope: Option<Extension<EventScope>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let public_dir = get_public_dir(&event_scope);

    // 1. Resolve Limits from Root
    let general_settings = state
        .db
        .get_config("general")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let max_mb = general_settings
        .and_then(|v| v.get("max_site_size_mb").and_then(|n| n.as_u64()))
        .unwrap_or(50);
    let max_bytes = max_mb * 1024 * 1024;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::UnknownError("Multipart error".into()))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|_| AppError::UnknownError("Read failed".into()))?;
            if (data.len() as u64) > max_bytes {
                return Err(AppError::UnknownError("Limit exceeded".into()));
            }

            // 2. Create Staging Area
            let staging_id = uuid::Uuid::new_v4();
            let staging_dir = PathBuf::from(format!("storage/tmp/deploy_{}", staging_id));
            fs::create_dir_all(&staging_dir).map_err(|e| AppError::UnknownError(e.to_string()))?;

            // 3. Extract Entire ZIP to Staging
            let cursor = Cursor::new(data);
            let mut archive = zip::ZipArchive::new(cursor)
                .map_err(|e| AppError::UnknownError(format!("Invalid ZIP: {}", e)))?;
            archive
                .extract(&staging_dir)
                .map_err(|e| AppError::UnknownError(format!("Extraction failed: {}", e)))?;

            // 4. SMART DISCOVERY
            let mut web_root = staging_dir.clone();

            // A. Check for index.html at root vs dist
            if !staging_dir.join("index.html").exists() {
                if staging_dir.join("dist").join("index.html").exists() {
                    info!("[Deploy] index.html found in /dist, using as web root.");
                    web_root = staging_dir.join("dist");
                } else if staging_dir.join("public").join("index.html").exists() {
                    info!("[Deploy] index.html found in /public, using as web root.");
                    web_root = staging_dir.join("public");
                }
            }

            // B. Database Metadata Discovery (Apex Bundle)
            // Look for apex_*.json files in the original staging root
            let metadata_files = [
                ("apex_schema.json", "schema"),
                ("apex_scripts.json", "scripts"),
                ("apex_templates.json", "templates"),
                ("apex_ai_actions.json", "ai_actions"),
            ];

            let mut db_updates = Vec::new();
            for (fname, label) in metadata_files {
                let p = staging_dir.join(fname);
                if p.exists()
                    && let Ok(content) = fs::read(&p)
                {
                    info!("[Deploy] Found metadata: {}. Deploying to DB...", fname);
                    if let Err(e) = deploy_metadata_item(&db, label, &content).await {
                        error!("[Deploy] Failed to deploy {}: {}", fname, e);
                        db_updates.push(format!("Error {}: {}", fname, e));
                    } else {
                        db_updates.push(format!("Success: {}", fname));
                    }
                }
            }

            // 5. Finalize Static Site Move
            if public_dir.exists() {
                fs::remove_dir_all(&public_dir).ok();
            }
            fs::create_dir_all(&public_dir).ok();

            // Copy files from detected web_root to the public_dir
            copy_dir_recursive(&web_root, &public_dir)
                .map_err(|e| AppError::UnknownError(e.to_string()))?;

            // 6. Cleanup Staging
            let _ = fs::remove_dir_all(&staging_dir);

            info!("Full Bundle deployed to {:?}", public_dir);
            return Ok(Json(json!({
                "success": true,
                "web_root_detected": web_root.strip_prefix(&staging_dir).unwrap_or(Path::new(".")),
                "database_updates": db_updates
            })));
        }
    }

    Err(AppError::UnknownError("No file".into()))
}

/// Helper to handle different metadata types
async fn deploy_metadata_item(db: &Arc<dyn Db>, label: &str, content: &[u8]) -> Result<(), String> {
    match label {
        "schema" => {
            // Reuses logic from Import Schema (Strategy: Overwrite)
            let req: crate::api::migration::import::ImportSchemaRequestDto =
                serde_json::from_slice(content).map_err(|e| e.to_string())?;
            for col in req.collections {
                let existing = db
                    .list_collections()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .find(|c| c.name == col.name);
                if let Some(e) = existing {
                    db.update_collection(e.id, None, col.schema)
                        .await
                        .map_err(|e| e.to_string())?;
                } else {
                    db.create_collection(&col.name, &col.schema, col.index)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        "scripts" => {
            let items: Vec<apexkit_core::models::Script> =
                serde_json::from_slice(content).map_err(|e| e.to_string())?;
            for s in items {
                db.create_script(CreateScriptReq {
                    name: s.name,
                    trigger_type: s.trigger_type,
                    target_collection: s.target_collection,
                    code: s.code,
                    active: s.active,
                    visibility: s.visibility,
                })
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        "templates" => {
            let items: Vec<apexkit_core::models::Template> =
                serde_json::from_slice(content).map_err(|e| e.to_string())?;
            for t in items {
                db.create_template(CreateTemplateReq {
                    slug: t.slug,
                    content: t.content,
                    script_id: t.script_id,
                })
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        "ai_actions" => {
            let items: Vec<apexkit_core::models::AiAction> =
                serde_json::from_slice(content).map_err(|e| e.to_string())?;
            for a in items {
                db.create_ai_action(CreateActionReq {
                    name: a.name,
                    slug: a.slug,
                    model: a.model,
                    system_prompt: a.system_prompt,
                    template: a.template,
                    config: a.config,
                })
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
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
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let public_dir = get_public_dir(&event_scope);

    let mut files = Vec::new();

    if public_dir.exists() {
        for entry in WalkDir::new(&public_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                // Return path relative to public root (e.g. "index.html", "css/style.css")
                let path = entry
                    .path()
                    .strip_prefix(&public_dir)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
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
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

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
        return Err(AppError::Forbidden(
            "Access denied: Path traversal detected".into(),
        ));
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
