use super::backends::get_storage_path;
use super::dto::*;
use super::image_ops::process_image;
use crate::hooks::trigger_void_hook;
use crate::utils::{BaseUrl, check_storage_quota, extract_log_meta};
use crate::{AppError, AppState, DatabaseConnection, StorageConnection};
use apexkit_core::auth::Claims;
use apexkit_core::realtime::EventScope;
use apexkit_core::storage::{LocalStorage, StorageBackend};
use axum::extract::ConnectInfo;
use axum::{
    Extension, Json,
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use std::net::SocketAddr;

#[utoipa::path(
    post,
    path = "/api/v1/storage/upload",
    request_body(content = FileUploadRequest, content_type = "multipart/form-data"),
    responses((status = 201, body = FileResponse))
)]
pub async fn upload_file(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    StorageConnection(storage): StorageConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<FileResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let user_id = claims.as_ref().map(|c| c.uid);
    if user_id.is_none() {
        return Err(AppError::Unauthorized("Login required".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    check_storage_quota(&state, &event_scope).await?;

    trigger_void_hook(
        &state,
        "before_file_upload",
        serde_json::json!({}),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::InputValidation(validator::ValidationErrors::new()))?
    {
        let original_name = field.file_name().unwrap_or("unknown.bin").to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let data = field
            .bytes()
            .await
            .map_err(|_| AppError::UnknownError("Failed bytes".into()))?;
        let size = data.len() as i64;

        let extension = std::path::Path::new(&original_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("bin");
        let filename = format!("{}.{}", uuid::Uuid::new_v4(), extension);

        storage
            .save(&filename, &data, &content_type)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let id = db
            .create_file_metadata(&filename, &original_name, &content_type, size, user_id)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let meta = extract_log_meta(
            &headers,
            Some(addr),
            serde_json::json!({ "filename": filename, "original": original_name, "size": size }),
        );
        let _ = db
            .log_audit_event("info", "File Uploaded", "storage", Some(meta))
            .await;

        let url = format!("{}{}", storage.get_public_url_base(), filename);

        let _ = trigger_void_hook(
            &state,
            "after_file_upload",
            serde_json::json!({ "id": id, "filename": filename }),
            claims.as_ref(),
            Some(&event_scope.clone()),
            Some(base_url.clone()),
        )
        .await;

        return Ok(Json(FileResponse { id, url, filename }));
    }
    Err(AppError::InputValidation(validator::ValidationErrors::new()))
}

#[utoipa::path(
    get,
    path = "/api/v1/storage/file/{filename}",
    params(FilenamePath, FileParams),
    responses((status = 200, description = "File Content"))
)]
pub async fn serve_file(
    StorageConnection(storage): StorageConnection,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<FilenamePath>,
    Query(params): Query<FileParams>,
) -> Result<Response, AppError> {
    if path.filename.contains("..") {
        return Err(AppError::Forbidden("Invalid path".into()));
    }
    let clean_filename = path.filename.trim_start_matches('/');

    let mime_type = if clean_filename.ends_with(".m4s") {
        "video/iso.segment".to_string()
    } else if clean_filename.ends_with(".mpd") {
        "application/dash+xml".to_string()
    } else if clean_filename.ends_with(".m3u8") {
        "application/vnd.apple.mpegurl".to_string()
    } else if clean_filename.ends_with(".ts") {
        "video/mp2t".to_string()
    } else {
        mime_guess::from_path(clean_filename)
            .first_or_octet_stream()
            .to_string()
    };

    // [CRITICAL STORAGE OPTIMIZATION]: Preemptive Cache Check
    // Prevents reading large image files from mounted disks/S3 into memory if their transformed thumbnail is already cached.
    let is_transform_requested = params.thumb.is_some()
        || params.format.is_some()
        || params.quality.is_some()
        || params.blur.is_some();

    if is_transform_requested && !mime_type.contains("svg") {
        let step = state
            .db
            .get_config("general")
            .await
            .ok()
            .flatten()
            .and_then(|v| v.get("image_processing_step").and_then(|n| n.as_f64()))
            .unwrap_or(10.0);
        let quality = params
            .quality
            .map(|q| {
                let q_step = step.max(1.0);
                ((q as f64 / q_step).round() * q_step) as u8
            })
            .unwrap_or(80)
            .clamp(1, 100);

        let req_blur = params.blur.map(|b| {
            let b_step = step as f32;
            ((b / b_step).round() * b_step).clamp(0.1, 50.0)
        });

        let target_mime = match params
            .format
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "webp" => "image/webp",
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "avif" => "image/avif",
            "gif" => "image/gif",
            _ => mime_type.as_str(),
        };

        let dim_part = params.thumb.as_deref().unwrap_or("orig");
        let fmt_part = target_mime.split('/').next_back().unwrap_or("bin");
        let blur_part = req_blur
            .map(|b| format!("_blur{:.1}", b))
            .unwrap_or_default();
        let full_cache_key = format!(
            "{}_{}_{}_q{}{}",
            clean_filename, dim_part, fmt_part, quality, blur_part
        );

        // Preemptively serve from RAM if hit
        if let Some(cached_bytes) = state.thumb_cache.get(&full_cache_key).await {
            let thumb_etag = format!("\"{:x}\"", md5::compute(cached_bytes.as_ref()));

            if let Some(if_none_match) = headers
                .get(axum::http::header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok())
            {
                if if_none_match == thumb_etag {
                    return Ok(Response::builder()
                        .status(StatusCode::NOT_MODIFIED)
                        .body(axum::body::Body::empty())
                        .unwrap());
                }
            }

            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header(axum::http::header::CONTENT_TYPE, target_mime)
                .header(
                    axum::http::header::CACHE_CONTROL,
                    "public, max-age=31536000, immutable",
                )
                .header(axum::http::header::ETAG, thumb_etag)
                .body(axum::body::Body::from(cached_bytes.as_ref().clone()))
                .unwrap());
        }
    }

    // Disk/S3 Load Strategy (Only triggers on a cache miss or non-transformed raw asset download)
    let mut original_bytes = storage.get(clean_filename).await;

    if original_bytes.is_err() {
        tracing::warn!(
            "Primary storage failed for '{}'. Attempting Root Local Fallback...",
            clean_filename
        );
        let root_local = LocalStorage::new(&get_storage_path("storage/system/uploads"), "/").await;
        original_bytes = root_local.get(clean_filename).await;
    }

    let data = original_bytes.map_err(|e| {
        tracing::error!("Storage failure for {}: {}", clean_filename, e);
        AppError::NotFound("File not found".into())
    })?;

    process_image(
        &state,
        &headers,
        data,
        &mime_type,
        clean_filename.to_string(),
        params.thumb,
        params.format,
        params.quality,
        params.blur,
    )
    .await
}

#[utoipa::path(get, path = "/api/v1/storage/files", params(FileListQuery), responses((status = 200, body = FileListResponse)))]
pub async fn list_files(
    DatabaseConnection(db): DatabaseConnection,
    Query(params): Query<FileListQuery>,
) -> Result<Json<FileListResponse>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * limit;
    let files = db
        .list_files(limit, offset)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let total = db
        .count_files()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(FileListResponse {
        items: files,
        total,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/storage/files/{id}",
    params(
        ("id" = String, Path, description = "File ID or Filename"),
        GetFileQuery
    ),
    responses((status = 200, body = StoredFileWithSignedUrl))
)]
pub async fn get_file(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    StorageConnection(storage): StorageConnection,
    Path(params): Path<GetFileParams>,
    Query(q): Query<GetFileQuery>,
) -> Result<Json<StoredFileWithSignedUrl>, AppError> {
    let _claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?;
    let id_or_name = params.id;

    let file_meta = if let Ok(id) = id_or_name.parse::<i64>() {
        db.get_file_metadata(id).await
    } else {
        db.get_file_by_filename(&id_or_name).await
    }
    .map_err(|e| AppError::UnknownError(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("File not found".into()))?;

    let expires = q.expires_in.unwrap_or(3600);
    let signed_url = storage
        .get_signed_url(&file_meta.filename, expires)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(StoredFileWithSignedUrl {
        id: file_meta.id,
        filename: file_meta.filename,
        original_name: file_meta.original_name,
        mime_type: file_meta.mime_type,
        size: file_meta.size,
        created_at: file_meta.created_at,
        signed_url,
    }))
}

#[utoipa::path(delete, path = "/api/v1/storage/files/{id}", params(FileIdPath), responses((status = 204, description = "File deleted")))]
pub async fn delete_file(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    StorageConnection(storage): StorageConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<FileIdPath>,
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let user_id = claims.as_ref().map(|c| c.uid);
    if user_id.is_none() {
        return Err(AppError::Unauthorized("Login required".into()));
    }
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    trigger_void_hook(
        &state,
        "before_file_delete",
        serde_json::json!({ "id": path.id }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    let file = db
        .get_file_metadata(path.id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("File".into()))?;
    storage
        .delete(&file.filename)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    db.delete_file_metadata(path.id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let meta = extract_log_meta(
        &headers,
        Some(addr),
        serde_json::json!({ "id": path.id, "filename": file.filename }),
    );
    let _ = db
        .log_audit_event("warning", "File Deleted", "storage", Some(meta))
        .await;

    let _ = trigger_void_hook(
        &state,
        "after_file_delete",
        serde_json::json!({ "id": path.id, "filename": file.filename }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
