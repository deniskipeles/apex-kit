use axum::{
    extract::{Multipart, State, Path, Query},
    response::{Response},
    http::{StatusCode, header},
    Json, Extension,
    body::Body,
};
use serde::{Serialize, Deserialize};
use utoipa::{ToSchema, IntoParams};
use std::io::Cursor;
use image::imageops::FilterType;
use std::sync::Arc;
use tokio::sync::RwLock; 
use crate::{trigger_void_hook, extract_log_meta};
use axum::extract::ConnectInfo;
use std::net::SocketAddr;

use apexkit_core::{
    auth::Claims, 
    models::StoredFile, 
    storage::{StorageBackend, LocalStorage, S3Storage}, 
    security::{Vault, EncryptedValue},
    Db
};
use crate::{AppState, AppError, assets::Assets, settings::StorageConfigDto, DatabaseConnection, StorageConnection};
use apexkit_core::realtime::EventScope;
use crate::BaseUrl;

use async_trait::async_trait;

#[derive(Serialize, ToSchema)]
pub struct FileResponse {
    id: i64,
    url: String,
    filename: String,
}

#[derive(Deserialize, ToSchema, IntoParams)] 
pub struct FileListQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Deserialize, IntoParams)]
pub struct FileParams {
    pub thumb: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct FileListResponse {
    items: Vec<StoredFile>,
    total: i64,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct FileUploadRequest {
    #[schema(value_type = String, format = Binary)]
    file: Vec<u8>,
}

#[derive(Deserialize, ToSchema)]
pub struct TestS3ConfigReq {
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct MigrateStorageReq {
    #[schema(example = "local")]
    pub source: String, // "local" or "s3"
    #[schema(example = "s3")]
    pub destination: String, // "local" or "s3"
}

#[derive(Serialize, ToSchema)]
pub struct MigrationResult {
    pub success: bool,
    pub processed: usize,
    pub errors: usize,
    pub message: String,
}

// --- PATH STRUCTS FOR NESTED ROUTING ---
#[derive(Deserialize)]
pub struct FilenamePath {
    pub filename: String,
}

#[derive(Deserialize)]
pub struct FileIdPath {
    pub id: i64,
}

// --- DYNAMIC STORAGE PROXY ---

pub struct DynamicStorage {
    db: Arc<dyn Db>,
    vault: Arc<Vault>,
    backend_cache: RwLock<Option<Arc<dyn StorageBackend>>>,
    last_update: RwLock<std::time::Instant>,
    fs_root_override: Option<String>,
    public_url_prefix: Option<String>, 
}

impl DynamicStorage {
    pub fn new(
        db: Arc<dyn Db>, 
        vault: Arc<Vault>,
        fs_root_override: Option<String>,
        public_url_prefix: String
    ) -> Self {
        Self { 
            db, 
            vault,
            backend_cache: RwLock::new(None),
            last_update: RwLock::new(std::time::Instant::now()),
            fs_root_override,
            public_url_prefix: Some(public_url_prefix)
        }
    }

    async fn resolve_backend(&self) -> Result<Arc<dyn StorageBackend>, Box<dyn std::error::Error + Send + Sync>> {
        {
            let cache = self.backend_cache.read().await;
            let time = self.last_update.read().await;
            if let Some(backend) = cache.as_ref() {
                if time.elapsed() < std::time::Duration::from_secs(60) {
                    return Ok(backend.clone());
                }
            }
        }

        let mut cache_write = self.backend_cache.write().await;
        let mut time_write = self.last_update.write().await;
        
        if let Some(backend) = cache_write.as_ref() {
             if time_write.elapsed() < std::time::Duration::from_secs(60) {
                 return Ok(backend.clone());
             }
        }

        let settings_json = self.db.get_config("storage").await?;
        
        let config: StorageConfigDto = if let Some(val) = settings_json {
            serde_json::from_value(val).unwrap_or_else(|_| StorageConfigDto {
                active_driver: "local".to_string(),
                s3: Default::default(),
            })
        } else {
            StorageConfigDto {
                active_driver: "local".to_string(),
                s3: Default::default(),
            }
        };

        let backend: Arc<dyn StorageBackend> = if config.active_driver == "s3" && config.s3.enabled {
            let secret_key = if let Some(encrypted_str) = config.s3.secret_key {
                if !encrypted_str.is_empty() {
                    let enc: EncryptedValue = serde_json::from_str(&encrypted_str)
                        .map_err(|_| "Invalid encrypted secret key format")?;
                    self.vault.decrypt(&enc).map_err(|_| "Failed to decrypt secret key")?
                } else {
                    return Err("S3 Secret Key is empty".into());
                }
            } else {
                return Err("S3 Secret Key missing".into());
            };

            let s3 = S3Storage::new_with_creds(
                &config.s3.bucket,
                &config.s3.region,
                &config.s3.endpoint, // <--- Added this argument
                &self.get_public_url_base(), // Pass base URL or use config public URL if you have it
                &config.s3.access_key,
                &secret_key
            ).await;
            
            Arc::new(s3)
        } else {
            let path = self.fs_root_override.clone().unwrap_or_else(|| "./uploads".to_string());
            let url_base = self.public_url_prefix.clone().unwrap_or_else(|| "/api/v1/storage/file/".to_string());
            
            Arc::new(LocalStorage::new(&path, &url_base).await)
        };

        *cache_write = Some(backend.clone());
        *time_write = std::time::Instant::now();

        Ok(backend)
    }
}

#[async_trait]
impl StorageBackend for DynamicStorage {
    async fn save(&self, filename: &str, data: &[u8], content_type: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let backend = self.resolve_backend().await?;
        backend.save(filename, data, content_type).await
    }

    async fn get(&self, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        match self.resolve_backend().await {
            Ok(backend) => {
                match backend.get(filename).await {
                    Ok(data) => Ok(data),
                    Err(_) => Err("File not found".into())
                }
            }
            Err(e) => Err(e)
        }
    }

    async fn delete(&self, filename: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let backend = self.resolve_backend().await?;
        backend.delete(filename).await
    }

    fn get_public_url_base(&self) -> String {
        self.public_url_prefix.clone().unwrap_or_else(|| "/api/v1/storage/file/".to_string())
    }
}

// --- HANDLERS ---
// --- HANDLER: Test S3 Connection ---
// Uses payload if present, otherwise falls back to DB settings
#[utoipa::path(
    post,
    path = "/api/v1/admin/storage/test",
    request_body = TestS3ConfigReq,
    responses(
        (status = 200, description = "Connection successful"),
        (status = 400, description = "Connection failed"),
        (status = 403, description = "Admin only")
    )
)]
pub async fn test_s3_connection(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection, // <--- Need DB to fetch saved config
    State(state): State<AppState>,              // <--- Need Vault to decrypt saved secret
    Json(payload): Json<TestS3ConfigReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // 1. Retrieve Saved Settings
    let saved_json = db.get_config("storage").await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let saved_config: Option<StorageConfigDto> = if let Some(val) = saved_json {
        serde_json::from_value(val).ok()
    } else {
        None
    };
    
    let s3_saved = saved_config.map(|c| c.s3).unwrap_or_default();

    // 2. Resolve Values (Payload > Saved > Error)
    let bucket = payload.bucket.filter(|s| !s.is_empty()).unwrap_or(s3_saved.bucket);
    let region = payload.region.filter(|s| !s.is_empty()).unwrap_or(s3_saved.region);
    let endpoint = payload.endpoint.filter(|s| !s.is_empty()).unwrap_or(s3_saved.endpoint);
    let access_key = payload.access_key.filter(|s| !s.is_empty()).unwrap_or(s3_saved.access_key);
    
    // 3. Resolve Secret Key (Handle Masking & Encryption)
    // If payload has a value and it's NOT "******", use it (Raw)
    // If payload is empty or "******", try to load from DB (Encrypted)
    let raw_secret_key = if let Some(pk) = payload.secret_key.filter(|s| !s.is_empty() && s != "******") {
        pk
    } else if let Some(encrypted_str) = s3_saved.secret_key {
         if !encrypted_str.is_empty() {
             let enc: EncryptedValue = serde_json::from_str(&encrypted_str)
                .map_err(|_| AppError::UnknownError("Saved secret key is corrupted".into()))?;
             state.vault.decrypt(&enc)
                .map_err(|_| AppError::UnknownError("Failed to decrypt saved secret key".into()))?
         } else {
             return Err(AppError::UnknownError("Secret key is empty".into()));
         }
    } else {
        return Err(AppError::UnknownError("Secret key missing in request and database".into()));
    };

    if bucket.is_empty() { return Err(AppError::UnknownError("Bucket is required".into())); }

    // 4. Initialize Backend
    let s3 = S3Storage::new_with_creds(
        &bucket,
        &region,
        &endpoint,
        "", // public_url_base irrelevant for connection test
        &access_key,
        &raw_secret_key
    ).await;

    let filename = ".apexkit_test_connectivity";

    // 5. Try Upload
    s3.save(filename, b"connection_verified", "text/plain").await
        .map_err(|e| AppError::UnknownError(format!("Write failed: {}", e)))?;

    // 6. Try Delete (Cleanup)
    s3.delete(filename).await
        .map_err(|e| AppError::UnknownError(format!("Delete failed: {}", e)))?;

    Ok(Json(serde_json::json!({ 
        "success": true, 
        "message": "Successfully connected, uploaded, and deleted test file." 
    })))
}

// --- NEW HANDLER: Migrate Storage ---
#[utoipa::path(
    post,
    path = "/api/v1/admin/storage/migrate",
    request_body = MigrateStorageReq,
    responses(
        (status = 200, body = MigrationResult),
        (status = 403, description = "Admin only")
    )
)]
pub async fn migrate_storage(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Json(payload): Json<MigrateStorageReq>,
) -> Result<Json<MigrationResult>, AppError> {
    // 1. Auth Check
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    if payload.source == payload.destination {
        return Err(AppError::UnknownError("Source and Destination cannot be the same".into()));
    }

    // 2. Load Settings to configure backends
    let settings_json = db.get_config("storage").await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let config: StorageConfigDto = if let Some(val) = settings_json {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        return Err(AppError::UnknownError("Storage not configured".into()));
    };

    // 3. Helper to build a backend instance dynamically
    let build_backend = |req_type: &str| -> Result<Arc<dyn StorageBackend>, AppError> {
        match req_type {
            "local" => {
                // Use default paths
                let path = "./uploads"; 
                let url_base = "/api/v1/storage/file/";
                // We have to call async new, so we wrap in async block if needed, but here we await immediately
                // However, LocalStorage::new is async.
                Ok(Arc::new(futures::executor::block_on(LocalStorage::new(path, url_base))))
            },
            "s3" => {
                if !config.s3.enabled { return Err(AppError::UnknownError("S3 is disabled in settings".into())); }
                
                let raw_secret = config.s3.secret_key.as_ref().ok_or(AppError::UnknownError("S3 Secret missing".into()))?;
                let secret_key = if raw_secret.starts_with('{') {
                     // Decrypt
                     let enc: EncryptedValue = serde_json::from_str(raw_secret).map_err(|_| AppError::UnknownError("Bad key format".into()))?;
                     state.vault.decrypt(&enc).map_err(|_| AppError::UnknownError("Decrypt fail".into()))?
                } else {
                    raw_secret.clone()
                };

                let s3 = futures::executor::block_on(S3Storage::new_with_creds(
                    &config.s3.bucket,
                    &config.s3.region,
                    &config.s3.endpoint,
                    "", // public_url irrelevant for migration
                    &config.s3.access_key,
                    &secret_key
                ));
                Ok(Arc::new(s3))
            },
            _ => Err(AppError::UnknownError("Invalid storage type".into()))
        }
    };

    let source_backend = build_backend(&payload.source)?;
    let dest_backend = build_backend(&payload.destination)?;

    // 4. Batch Process Files
    let mut offset = 0;
    let limit = 50; // Process in chunks to avoid memory spikes
    let mut processed_count = 0;
    let mut error_count = 0;

    loop {
        // Fetch metadata from DB
        let files = db.list_files(limit, offset).await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        if files.is_empty() { break; }

        for file in files {
            // A. Read from Source
            match source_backend.get(&file.filename).await {
                Ok(data) => {
                    // B. Write to Destination
                    // We rely on DB mime_type
                    if let Err(e) = dest_backend.save(&file.filename, &data, &file.mime_type).await {
                        tracing::error!("Migration Write Error {}: {}", file.filename, e);
                        error_count += 1;
                    } else {
                        processed_count += 1;
                    }
                },
                Err(e) => {
                    // If file missing in source, log and skip
                    tracing::warn!("Migration Read Error (Missing in Source) {}: {}", file.filename, e);
                    error_count += 1;
                }
            }
        }

        offset += limit;
    }

    Ok(Json(MigrationResult {
        success: true,
        processed: processed_count,
        errors: error_count,
        message: format!("Processed {} files. {} errors.", processed_count, error_count)
    }))
}

// --- HANDLER: Upload File ---
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>, // Capture IP
    headers: axum::http::HeaderMap, // Capture Headers
    mut multipart: Multipart,
) -> Result<Json<FileResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let user_id = claims.as_ref().map(|c| c.uid);

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    // [TRIGGER] Before Upload
    trigger_void_hook(&state, "before_file_upload", serde_json::json!({}), claims.as_ref(), Some(&event_scope.clone()), Some(base_url.clone())).await?;

    while let Some(field) = multipart.next_field().await.map_err(|_| AppError::InputValidation(validator::ValidationErrors::new()))? {
        let original_name = field.file_name().unwrap_or("unknown.bin").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
        let data = field.bytes().await.map_err(|_| AppError::UnknownError("Failed bytes".into()))?;
        let size = data.len() as i64;
        let filename = format!("{}.{}", uuid::Uuid::new_v4(), "bin");

        storage.save(&filename, &data, &content_type).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

        let id = db.create_file_metadata(&filename, &original_name, &content_type, size, user_id).await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        // [LOG]
        let meta = extract_log_meta(&headers, Some(addr), serde_json::json!({ 
            "filename": filename, "original": original_name, "size": size 
        }));
        let _ = db.log_audit_event("info", "File Uploaded", "storage", Some(meta)).await;

        let url = format!("{}{}", storage.get_public_url_base(), filename);

        // [TRIGGER] After Upload
        let _ = trigger_void_hook(&state, "after_file_upload", serde_json::json!({ "id": id, "filename": filename }), claims.as_ref(),  Some(&event_scope.clone()), Some(base_url.clone())).await;

        return Ok(Json(FileResponse { id, url, filename }));
    }
    Err(AppError::InputValidation(validator::ValidationErrors::new()))
}


// --- HANDLER: Serve Generic File ---
#[utoipa::path(
    get,
    path = "/api/v1/storage/file/{filename}",
    params(FileParams),
    responses((status = 200, description = "File Content"))
)]
pub async fn serve_file(
    StorageConnection(storage): StorageConnection,
    State(state): State<AppState>, 
    Path(path): Path<FilenamePath>, // <--- FIXED: Use Struct for Path
    Query(params): Query<FileParams>,
) -> Result<Response, AppError> {
    
    let original_bytes = storage.get(&path.filename).await
        .map_err(|_| AppError::NotFound("File not found".into()))?;

    let mime_type = mime_guess::from_path(&path.filename).first_or_octet_stream();

    process_image(
        &state, 
        original_bytes, 
        mime_type.as_ref(), 
        path.filename, 
        params.thumb
    ).await
}

// --- HANDLER: List Files ---
#[utoipa::path(
    get,
    path = "/api/v1/storage/files",
    params(FileListQuery),
    responses((status = 200, body = FileListResponse))
)]
pub async fn list_files(
    DatabaseConnection(db): DatabaseConnection, 
    Query(params): Query<FileListQuery>,
) -> Result<Json<FileListResponse>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * limit;

    // 1. Get items
    let files = db.list_files(limit, offset).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    // 2. Get real total count
    let total = db.count_files().await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(FileListResponse { items: files, total }))
}

// --- HANDLER: Delete File ---
#[utoipa::path(
    delete,
    path = "/api/v1/storage/files/{id}",
    responses((status = 204, description = "File deleted"))
)]
pub async fn delete_file(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    StorageConnection(storage): StorageConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(path): Path<FileIdPath>,
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    // [TRIGGER] Before Delete
    trigger_void_hook(&state, "before_file_delete", serde_json::json!({ "id": path.id }), claims.as_ref(), Some(&event_scope.clone()), Some(base_url.clone())).await?;

    let file = db.get_file_metadata(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("File".into()))?;
    storage.delete(&file.filename).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    db.delete_file_metadata(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOG]
    let meta = extract_log_meta(&headers, Some(addr), serde_json::json!({ "id": path.id, "filename": file.filename }));
    let _ = db.log_audit_event("warning", "File Deleted", "storage", Some(meta)).await;

    Ok(StatusCode::NO_CONTENT)
}

// --- HELPER: Centralized Image Resizing Logic ---
async fn process_image(
    state: &AppState,
    original_bytes: Vec<u8>,
    mime_type: &str,
    cache_key: String,
    dim_str: Option<String>
) -> Result<Response, AppError> {

    if dim_str.is_none() || mime_type.contains("svg") {
         return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_type)
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(original_bytes))
            .unwrap());
    }

    let dim = dim_str.unwrap();
    let full_cache_key = format!("{}_{}", cache_key, dim);

    if let Some(cached_bytes) = state.thumb_cache.get(&full_cache_key).await {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_type)
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(cached_bytes.as_ref().clone()))
            .unwrap());
    }

    let (w, h) = parse_dimensions(&dim).unwrap_or((0, 0));
    
    if w == 0 && h == 0 {
         return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_type)
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(original_bytes))
            .unwrap());
    }

    let format = image::ImageFormat::from_mime_type(mime_type)
        .unwrap_or(image::ImageFormat::Png);

    let bytes_for_processing = original_bytes.clone();

    let img_result = tokio::task::spawn_blocking(move || {
        image::load_from_memory(&bytes_for_processing)
    }).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    match img_result {
        Ok(img) => {
            let target_w = if w == 0 { u32::MAX } else { w };
            let target_h = if h == 0 { u32::MAX } else { h };

            let scaled = img.resize(target_w, target_h, FilterType::Lanczos3);
            
            let mut buffer = Cursor::new(Vec::new());
            if let Err(_) = scaled.write_to(&mut buffer, format) {
                return Err(AppError::UnknownError("Image encoding failed".into()));
            }

            let thumb_bytes = buffer.into_inner();
            
            state.thumb_cache.insert(full_cache_key, Arc::new(thumb_bytes.clone())).await;

            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(thumb_bytes))
                .unwrap())
        },
        Err(_) => {
             Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(original_bytes)) 
                .unwrap())
        }
    }
}

// --- HANDLER: Serve App Logo ---
#[utoipa::path(
    get,
    path = "/logo",
    params(FileParams),
    responses((status = 200, description = "App Logo"))
)]
pub async fn serve_app_logo(
    DatabaseConnection(db): DatabaseConnection, // <--- FIXED: Tenant-aware metadata
    StorageConnection(storage): StorageConnection, // <--- FIXED: Tenant-aware files
    State(state): State<AppState>,
    Query(params): Query<FileParams>,
) -> Result<Response, AppError> {
    
    // 1. Get settings from Tenant DB
    let settings = db.get_config("general").await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let (bytes, mime, cache_key_base) = if let Some(val) = settings {
        if let Some(logo_filename) = val.get("app_logo").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            // 2. Fetch file from Tenant Storage
            match storage.get(logo_filename).await {
                Ok(b) => {
                     let m = mime_guess::from_path(logo_filename).first_or_octet_stream();
                     (b, m.to_string(), logo_filename.to_string())
                },
                Err(_) => get_default_logo()?
            }
        } else {
            get_default_logo()?
        }
    } else {
        get_default_logo()?
    };

    process_image(
        &state,
        bytes,
        &mime,
        format!("logo_{}", cache_key_base), 
        params.thumb
    ).await
}

fn get_default_logo() -> Result<(Vec<u8>, String, String), AppError> {
    let default_path = "images/apexkit-logo.svg"; 
    match Assets::get(default_path) {
        Some(content) => {
            let mime = mime_guess::from_path(default_path).first_or_octet_stream();
            Ok((content.data.to_vec(), mime.to_string(), "default".to_string()))
        },
        None => Err(AppError::NotFound("Default logo asset missing".into()))
    }
}

fn parse_dimensions(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse::<u32>().ok()?;
        let h = parts[1].parse::<u32>().ok()?;
        return Some((w, h));
    }
    None
}