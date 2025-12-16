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
use tokio::sync::RwLock; // Import RwLock

use tinybase_core::{
    auth::Claims, 
    models::StoredFile, 
    storage::{StorageBackend, LocalStorage, S3Storage}, 
    security::{Vault, EncryptedValue},
    Db
};
use crate::{AppState, AppError, assets::Assets, settings::StorageConfigDto};

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

// --- DYNAMIC STORAGE PROXY ---

pub struct DynamicStorage {
    db: Arc<dyn Db>,
    vault: Arc<Vault>,
    // Cache the resolved backend to avoid re-init on every request
    // Wrapped in Arc to be cloneable if needed, RwLock for thread safety
    backend_cache: RwLock<Option<Arc<dyn StorageBackend>>>,
    // Timestamp of last cache update to allow config refresh
    last_update: RwLock<std::time::Instant>,
}

impl DynamicStorage {
    pub fn new(db: Arc<dyn Db>, vault: Arc<Vault>) -> Self {
        Self { 
            db, 
            vault,
            backend_cache: RwLock::new(None),
            last_update: RwLock::new(std::time::Instant::now())
        }
    }

    /// Resolves the concrete backend (S3 or Local) based on DB settings
    /// Caches the result for 60 seconds to reduce DB hits
    async fn resolve_backend(&self) -> Result<Arc<dyn StorageBackend>, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Check Cache (Read Lock)
        {
            let cache = self.backend_cache.read().await;
            let time = self.last_update.read().await;
            if let Some(backend) = cache.as_ref() {
                if time.elapsed() < std::time::Duration::from_secs(60) {
                    return Ok(backend.clone());
                }
            }
        }

        // 2. Fetch Settings (Write Lock)
        // We re-check condition after acquiring write lock to avoid race conditions
        let mut cache_write = self.backend_cache.write().await;
        let mut time_write = self.last_update.write().await;
        
        // Double-check optimization
        if let Some(backend) = cache_write.as_ref() {
             if time_write.elapsed() < std::time::Duration::from_secs(60) {
                 return Ok(backend.clone());
             }
        }

        let settings_json = self.db.get_setting("storage").await?;
        
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

        // 3. Initialize Backend
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
                &config.s3.endpoint,
                &config.s3.access_key,
                &secret_key
            ).await;
            
            Arc::new(s3)
        } else {
            let local = LocalStorage::new("./uploads", "/api/v1/storage/file/").await;
            Arc::new(local)
        };

        // 4. Update Cache
        *cache_write = Some(backend.clone());
        *time_write = std::time::Instant::now();

        Ok(backend)
    }
}

#[async_trait]
impl StorageBackend for DynamicStorage {
    async fn save(&self, filename: &str, data: &[u8]) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let backend = self.resolve_backend().await?;
        backend.save(filename, data).await
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
        "/api/v1/storage/file/".to_string() 
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/storage/upload",
    request_body(content = FileUploadRequest, content_type = "multipart/form-data"),
    responses((status = 201, body = FileResponse))
)]
pub async fn upload_file(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<FileResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let user_id = claims.map(|c| c.uid);

    while let Some(field) = multipart.next_field().await.map_err(|_| AppError::InputValidation(validator::ValidationErrors::new()))? {
        let original_name = field.file_name().unwrap_or("unknown.bin").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
        
        let data = field.bytes().await.map_err(|_| AppError::UnknownError("Failed to read bytes".into()))?;
        
        let size = data.len() as i64;
        let ext = std::path::Path::new(&original_name)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("bin");
        
        let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);

        state.storage.save(&filename, &data).await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let id = state.db.create_file_metadata(&filename, &original_name, &content_type, size, user_id).await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let url = format!("{}{}", state.storage.get_public_url_base(), filename);

        return Ok(Json(FileResponse {
            id,
            url,
            filename,
        }));
    }

    Err(AppError::InputValidation(validator::ValidationErrors::new()))
}

// --- HELPER: Centralized Image Resizing Logic ---
async fn process_image(
    state: &AppState,
    original_bytes: Vec<u8>,
    mime_type: &str,
    cache_key: String,
    dim_str: Option<String>
) -> Result<Response, AppError> {

    // 1. If no thumb param or it's SVG (vector), return original immediately
    if dim_str.is_none() || mime_type.contains("svg") {
         return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_type)
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(original_bytes))
            .unwrap());
    }

    // 2. Check Cache
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

    // 3. Process Image
    let (w, h) = parse_dimensions(&dim).unwrap_or((0, 0));
    
    if w == 0 && h == 0 {
         // Invalid dimensions, return original
         return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_type)
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(original_bytes))
            .unwrap());
    }

    // Load image (CPU intensive)
    let format = image::ImageFormat::from_mime_type(mime_type)
        .unwrap_or(image::ImageFormat::Png);

    // Clone the bytes for the background thread
    // This leaves 'original_bytes' valid for the Err case below
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
                // If encoding fails, return error
                return Err(AppError::UnknownError("Image encoding failed".into()));
            }

            let thumb_bytes = buffer.into_inner();
            
            // Cache it
            state.thumb_cache.insert(full_cache_key, Arc::new(thumb_bytes.clone())).await;

            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(thumb_bytes))
                .unwrap())
        },
        Err(_) => {
             // If load/resize fails (e.g. corrupted image), return original
             // 'original_bytes' is valid here because we cloned it above
             Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(original_bytes)) 
                .unwrap())
        }
    }
}

// --- HANDLER: Serve Generic File ---
#[utoipa::path(
    get,
    path = "/api/v1/storage/file/{filename}",
    params(FileParams),
    responses((status = 200, description = "File Content"))
)]
pub async fn serve_file(
    State(state): State<AppState>,
    Path(filename): Path<String>,
    Query(params): Query<FileParams>,
) -> Result<Response, AppError> {
    
    let original_bytes = state.storage.get(&filename).await
        .map_err(|_| AppError::NotFound("File not found".into()))?;

    let mime_type = mime_guess::from_path(&filename).first_or_octet_stream();

    process_image(
        &state, 
        original_bytes, 
        mime_type.as_ref(), 
        filename, 
        params.thumb
    ).await
}

// --- HANDLER: Serve App Logo ---
#[utoipa::path(
    get,
    path = "/logo",
    params(FileParams),
    responses((status = 200, description = "App Logo"))
)]
pub async fn serve_app_logo(
    State(state): State<AppState>,
    Query(params): Query<FileParams>,
) -> Result<Response, AppError> {
    
    let settings = state.db.get_setting("general").await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let (bytes, mime, cache_key_base) = if let Some(val) = settings {
        if let Some(logo_filename) = val.get("app_logo").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            match state.storage.get(logo_filename).await {
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
    let default_path = "images/tinybase-logo.svg"; 
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

#[utoipa::path(
    get,
    path = "/api/v1/storage/files",
    params(FileListQuery),
    responses((status = 200, body = FileListResponse))
)]
pub async fn list_files(
    State(state): State<AppState>,
    Query(params): Query<FileListQuery>,
) -> Result<Json<FileListResponse>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * limit;

    let files = state.db.list_files(limit, offset).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let total = files.len() as i64; 

    Ok(Json(FileListResponse { items: files, total }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/storage/files/{id}",
    responses((status = 204, description = "File deleted"))
)]
pub async fn delete_file(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);
    if let Some(claims) = claims {
        if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    } else {
        return Err(AppError::Unauthorized("Login required".into()));
    }

    let file = state.db.get_file_metadata(id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("File not found".into()))?;

    state.storage.delete(&file.filename).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    state.db.delete_file_metadata(id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}