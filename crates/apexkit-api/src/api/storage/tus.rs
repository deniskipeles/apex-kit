use crate::hooks::trigger_void_hook;
use crate::utils::{
    BaseUrl, check_storage_quota, check_temp_quota, extract_log_meta, get_temp_path,
};
use crate::{AppError, AppState, DatabaseConnection, StorageConnection};
use apexkit_core::auth::Claims;
use apexkit_core::realtime::EventScope;
use axum::Extension;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

const TUS_VERSION: &str = "1.0.0";
const TUS_EXTENSIONS: &str = "creation,creation-with-upload,termination";
const DEFAULT_MAX_UPLOAD_SIZE: u64 = 10 * 1024 * 1024 * 1024; // 10 GB

#[derive(Deserialize)]
pub struct TusPathParams {
    pub upload_id: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TusUploadInfo {
    pub upload_id: String,
    pub total_size: u64,
    pub current_offset: u64,
    pub original_name: String,
    pub mime_type: String,
    pub user_id: Option<i64>,
    pub created_at: String,
}

/// Resolves the temp path directly inside the current tenant/sandbox/system tmp directory
fn get_scoped_tus_path(scope: &EventScope, upload_id: &str, ext: &str) -> PathBuf {
    let subpath = match scope {
        EventScope::Root => format!("system/tmp/tus/{}.{}", upload_id, ext),
        EventScope::Tenant(id) => format!("tenants/{}/tmp/tus/{}.{}", id, upload_id, ext),
        EventScope::Sandbox(id) => {
            format!("sandboxes/session_{}/tmp/tus/{}.{}", id, upload_id, ext)
        }
        _ => format!("system/tmp/tus/{}.{}", upload_id, ext),
    };
    get_temp_path(&subpath)
}

fn parse_tus_metadata(header_val: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for part in header_val.split(',') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once(' ') {
            if let Ok(decoded_bytes) = STANDARD.decode(v.trim()) {
                if let Ok(decoded_str) = String::from_utf8(decoded_bytes) {
                    map.insert(k.trim().to_string(), decoded_str);
                }
            }
        } else if !part.is_empty() {
            map.insert(part.to_string(), String::new());
        }
    }
    map
}

pub async fn tus_options() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Tus-Resumable", HeaderValue::from_static(TUS_VERSION));
    headers.insert("Tus-Version", HeaderValue::from_static(TUS_VERSION));
    headers.insert("Tus-Extension", HeaderValue::from_static(TUS_EXTENSIONS));
    headers.insert(
        "Tus-Max-Size",
        HeaderValue::from_str(&DEFAULT_MAX_UPLOAD_SIZE.to_string()).unwrap(),
    );
    headers.insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static(
            "Location, Upload-Offset, Upload-Length, Tus-Resumable, Tus-Version, Tus-Extension, Tus-Max-Size",
        ),
    );
    (StatusCode::NO_CONTENT, headers)
}

pub async fn tus_create(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    headers: HeaderMap,
    body_bytes: axum::body::Bytes,
) -> Result<Response, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let user_id = claims.as_ref().map(|c| c.uid);
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    let total_size = headers
        .get("Upload-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| AppError::JsonError("Missing or invalid Upload-Length header".into()))?;

    if total_size > DEFAULT_MAX_UPLOAD_SIZE {
        return Err(AppError::Forbidden(
            "Upload size exceeds maximum allowed size".into(),
        ));
    }

    check_storage_quota(&state, &event_scope).await?;
    check_temp_quota(&state, &event_scope, total_size).await?;

    let metadata_header = headers
        .get("Upload-Metadata")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let metadata = parse_tus_metadata(metadata_header);

    let original_name = metadata
        .get("filename")
        .or_else(|| metadata.get("name"))
        .cloned()
        .unwrap_or_else(|| "unnamed_upload.bin".to_string());

    let mime_type = metadata
        .get("filetype")
        .or_else(|| metadata.get("contentType"))
        .cloned()
        .unwrap_or_else(|| {
            mime_guess::from_path(&original_name)
                .first_or_octet_stream()
                .to_string()
        });

    let upload_id = uuid::Uuid::new_v4().to_string();

    let info_path = get_scoped_tus_path(&event_scope, &upload_id, "info");
    let part_path = get_scoped_tus_path(&event_scope, &upload_id, "part");

    if let Some(parent) = info_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    let mut current_offset = 0u64;

    if !body_bytes.is_empty() {
        let mut file = tokio::fs::File::create(&part_path)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
        file.write_all(&body_bytes)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
        current_offset = body_bytes.len() as u64;
    } else {
        tokio::fs::File::create(&part_path)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    let upload_info = TusUploadInfo {
        upload_id: upload_id.clone(),
        total_size,
        current_offset,
        original_name,
        mime_type,
        user_id,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    tokio::fs::write(&info_path, serde_json::to_string(&upload_info).unwrap())
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let location = match &event_scope {
        EventScope::Tenant(id) => format!(
            "{}/tenant/{}/api/v1/storage/upload/tus/{}",
            base_url, id, upload_id
        ),
        EventScope::Sandbox(id) => format!(
            "{}/sandbox/{}/api/v1/storage/upload/tus/{}",
            base_url, id, upload_id
        ),
        _ => format!("{}/api/v1/storage/upload/tus/{}", base_url, upload_id),
    };

    let mut res_headers = HeaderMap::new();
    res_headers.insert("Tus-Resumable", HeaderValue::from_static(TUS_VERSION));
    res_headers.insert("Location", HeaderValue::from_str(&location).unwrap());
    res_headers.insert(
        "Upload-Offset",
        HeaderValue::from_str(&current_offset.to_string()).unwrap(),
    );
    res_headers.insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static("Location, Upload-Offset, Tus-Resumable"),
    );

    Ok((StatusCode::CREATED, res_headers).into_response())
}

pub async fn tus_head(
    scope: Option<Extension<EventScope>>,
    Path(params): Path<TusPathParams>,
) -> Result<Response, AppError> {
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let upload_id = params.upload_id;

    let info_path = get_scoped_tus_path(&event_scope, &upload_id, "info");
    let part_path = get_scoped_tus_path(&event_scope, &upload_id, "part");

    if !info_path.exists() || !part_path.exists() {
        return Err(AppError::NotFound("Tus upload session not found".into()));
    }

    let info_data = tokio::fs::read_to_string(&info_path)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let info: TusUploadInfo =
        serde_json::from_str(&info_data).map_err(|e| AppError::UnknownError(e.to_string()))?;

    let meta = tokio::fs::metadata(&part_path)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let actual_offset = meta.len();

    let mut headers = HeaderMap::new();
    headers.insert("Tus-Resumable", HeaderValue::from_static(TUS_VERSION));
    headers.insert(
        "Upload-Offset",
        HeaderValue::from_str(&actual_offset.to_string()).unwrap(),
    );
    headers.insert(
        "Upload-Length",
        HeaderValue::from_str(&info.total_size.to_string()).unwrap(),
    );
    headers.insert("Cache-Control", HeaderValue::from_static("no-store"));
    headers.insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static("Upload-Offset, Upload-Length, Tus-Resumable, Cache-Control"),
    );

    Ok((StatusCode::OK, headers).into_response())
}

pub async fn tus_patch(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    StorageConnection(storage): StorageConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(params): Path<TusPathParams>,
    headers: HeaderMap,
    body_bytes: axum::body::Bytes,
) -> Result<Response, AppError> {
    let upload_id = params.upload_id;
    let claims = auth.map(|Extension(c)| c);
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    let info_path = get_scoped_tus_path(&event_scope, &upload_id, "info");
    let part_path = get_scoped_tus_path(&event_scope, &upload_id, "part");

    if !info_path.exists() || !part_path.exists() {
        return Err(AppError::NotFound(
            "Tus upload session expired or not found".into(),
        ));
    }

    let info_data = tokio::fs::read_to_string(&info_path)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let mut info: TusUploadInfo =
        serde_json::from_str(&info_data).map_err(|e| AppError::UnknownError(e.to_string()))?;

    let client_offset = headers
        .get("Upload-Offset")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| AppError::JsonError("Missing Upload-Offset header".into()))?;

    let meta = tokio::fs::metadata(&part_path)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let current_offset = meta.len();

    if client_offset != current_offset {
        return Err(AppError::JsonError(format!(
            "Offset mismatch: expected {}, got {}",
            current_offset, client_offset
        )));
    }

    let chunk_size = body_bytes.len() as u64;
    {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&part_path)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
        file.write_all(&body_bytes)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    let new_offset = current_offset + chunk_size;
    info.current_offset = new_offset;

    if new_offset >= info.total_size {
        tracing::info!(
            "✅ [Tus] Upload {} complete ({} bytes) in scope {:?}. Moving from tmp to persistent storage...",
            upload_id,
            info.total_size,
            event_scope
        );

        let assembled_bytes = tokio::fs::read(&part_path)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        // Convert extension to lowercase to prevent uppercase extension files (.PNG, .JPG, .JPEG)
        let raw_ext = std::path::Path::new(&info.original_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("bin");
        let extension = raw_ext.to_lowercase();

        // Persistent filename will always have a lowercase extension
        // Use the upload_id as the persistent filename so it matches the Tus upload session URL
        let persistent_filename = format!("{}.{}", upload_id, extension);

        storage
            .save(&persistent_filename, &assembled_bytes, &info.mime_type)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let id = db
            .create_file_metadata(
                &persistent_filename,
                &info.original_name,
                &info.mime_type,
                info.total_size as i64,
                info.user_id,
            )
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let file_url = format!("{}{}", storage.get_public_url_base(), persistent_filename);

        let log_meta = extract_log_meta(
            &headers,
            Some(addr),
            serde_json::json!({
                "filename": persistent_filename,
                "original": info.original_name,
                "size": info.total_size,
                "protocol": "tus"
            }),
        );
        let _ = db
            .log_audit_event(
                "info",
                "Resumable File Upload Completed",
                "storage",
                Some(log_meta),
            )
            .await;

        let _ = trigger_void_hook(
            &state,
            "after_file_upload",
            serde_json::json!({ "id": id, "filename": persistent_filename, "url": file_url }),
            claims.as_ref(),
            Some(&event_scope.clone()),
            Some(base_url),
        )
        .await;

        // Clean up partial chunks & metadata from tmp
        let _ = tokio::fs::remove_file(&part_path).await;
        let _ = tokio::fs::remove_file(&info_path).await;

        let mut res_headers = HeaderMap::new();
        res_headers.insert("Tus-Resumable", HeaderValue::from_static(TUS_VERSION));
        res_headers.insert(
            "Upload-Offset",
            HeaderValue::from_str(&new_offset.to_string()).unwrap(),
        );
        res_headers.insert("X-File-Id", HeaderValue::from_str(&id.to_string()).unwrap());
        res_headers.insert("X-File-Url", HeaderValue::from_str(&file_url).unwrap());
        res_headers.insert(
            "X-Storage-Filename",
            HeaderValue::from_str(&persistent_filename).unwrap(),
        );
        res_headers.insert(
            "Access-Control-Expose-Headers",
            HeaderValue::from_static(
                "Upload-Offset, Tus-Resumable, X-File-Id, X-File-Url, X-Storage-Filename",
            ),
        );

        return Ok((StatusCode::NO_CONTENT, res_headers).into_response());
    }

    tokio::fs::write(&info_path, serde_json::to_string(&info).unwrap())
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let mut res_headers = HeaderMap::new();
    res_headers.insert("Tus-Resumable", HeaderValue::from_static(TUS_VERSION));
    res_headers.insert(
        "Upload-Offset",
        HeaderValue::from_str(&new_offset.to_string()).unwrap(),
    );
    res_headers.insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static("Upload-Offset, Tus-Resumable"),
    );

    Ok((StatusCode::NO_CONTENT, res_headers).into_response())
}

pub async fn tus_delete(
    scope: Option<Extension<EventScope>>,
    Path(params): Path<TusPathParams>,
) -> Result<Response, AppError> {
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let upload_id = params.upload_id;

    let info_path = get_scoped_tus_path(&event_scope, &upload_id, "info");
    let part_path = get_scoped_tus_path(&event_scope, &upload_id, "part");

    let _ = tokio::fs::remove_file(&info_path).await;
    let _ = tokio::fs::remove_file(&part_path).await;

    let mut headers = HeaderMap::new();
    headers.insert("Tus-Resumable", HeaderValue::from_static(TUS_VERSION));
    headers.insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static("Tus-Resumable"),
    );

    Ok((StatusCode::NO_CONTENT, headers).into_response())
}
