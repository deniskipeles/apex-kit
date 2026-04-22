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
use image::codecs::jpeg::JpegEncoder;
use image::codecs::avif::AvifEncoder;
use image::ImageEncoder;
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

// --- DTOs ---
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
    pub format: Option<String>,
    pub quality: Option<u8>,
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
    pub source: String, 
    pub destination: String, 
}

#[derive(Serialize, ToSchema)]
pub struct MigrationResult {
    pub success: bool,
    pub processed: usize,
    pub errors: usize,
    pub message: String,
}

#[derive(Deserialize, IntoParams)]
pub struct FilenamePath {
    pub filename: String,
}

#[derive(Deserialize, IntoParams)]
pub struct FileIdPath {
    pub id: i64,
}

// --- DYNAMIC STORAGE PROXY (Root Only) ---
pub struct DynamicStorage {
    db: Arc<dyn Db>,
    vault: Arc<Vault>,
    // Cached tuple: (Backend, is_local)
    backend_cache: RwLock<Option<(Arc<dyn StorageBackend>, bool)>>,
    last_update: RwLock<std::time::Instant>,
    fs_root_override: Option<String>,
    public_url_prefix: Option<String>, 
}

impl DynamicStorage {
    pub fn new(db: Arc<dyn Db>, vault: Arc<Vault>, fs_root_override: Option<String>, public_url_prefix: String) -> Self {
        Self { db, vault, backend_cache: RwLock::new(None), last_update: RwLock::new(std::time::Instant::now()), fs_root_override, public_url_prefix: Some(public_url_prefix) }
    }

    async fn resolve_backend(&self) -> Result<(Arc<dyn StorageBackend>, bool), Box<dyn std::error::Error + Send + Sync>> {
        {
            let cache = self.backend_cache.read().await;
            let time = self.last_update.read().await;
            if let Some(cached) = cache.as_ref() {
                if time.elapsed() < std::time::Duration::from_secs(60) { 
                    return Ok(cached.clone()); 
                }
            }
        }

        let mut cache_write = self.backend_cache.write().await;
        let mut time_write = self.last_update.write().await;

        let settings_json = self.db.get_config("storage").await?;
        let config: StorageConfigDto = if let Some(val) = settings_json { 
            serde_json::from_value(val).unwrap_or_default() 
        } else { 
            StorageConfigDto::default() 
        };

        let (backend, is_local): (Arc<dyn StorageBackend>, bool) = if config.active_driver == "s3" && config.s3.enabled {
            let secret_key = if let Some(encrypted_str) = config.s3.secret_key {
                let enc: EncryptedValue = serde_json::from_str(&encrypted_str)?;
                self.vault.decrypt(&enc)?
            } else { 
                String::new() 
            };

            let s3 = S3Storage::new_with_creds(
                &config.s3.bucket, 
                &config.s3.region, 
                &config.s3.endpoint, 
                &self.get_public_url_base(), 
                &config.s3.access_key, 
                &secret_key, 
                ""
            ).await;
            (Arc::new(s3), false)
        } else {
            let path = self.fs_root_override.clone().unwrap_or_else(|| "./storage/system/uploads".to_string());
            let local = LocalStorage::new(&path, &self.get_public_url_base()).await;
            (Arc::new(local), true)
        };

        *cache_write = Some((backend.clone(), is_local));
        *time_write = std::time::Instant::now();
        
        Ok((backend, is_local))
    }
}

#[async_trait]
impl StorageBackend for DynamicStorage {
    async fn save(&self, name: &str, data: &[u8], content_type: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> { 
        let (backend, is_local) = self.resolve_backend().await?;
        let res = backend.save(name, data, content_type).await?;
        
        // [REPLICA PUSH] Forward local file uploads to Master via gRPC
        if is_local {
            if let Ok(master_url) = std::env::var("APEX_MASTER_URL") {
                if !master_url.is_empty() {
                    let data_clone = data.to_vec();
                    let name_clone = name.to_string();
                    let mime_clone = content_type.to_string();
                    
                    tokio::spawn(async move {
                        if let Err(e) = crate::replication::forward_file_to_master(&master_url, "root", &name_clone, &mime_clone, &data_clone).await {
                            tracing::error!("Failed to sync file to master via gRPC: {}", e);
                        }
                    });
                }
            }
        }
        Ok(res)
    }
    
    async fn get(&self, name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> { 
        let (backend, _is_local) = self.resolve_backend().await?;
        match backend.get(name).await {
            Ok(data) => Ok(data),
            Err(e) => {
                // [LAZY REPLICATION] Pull file from Master if missing on Replica
                if let Ok(master_url) = std::env::var("APEX_MASTER_URL") {
                    if !master_url.is_empty() {
                        tracing::info!("☁️ File '{}' missing locally on Replica. Fetching from Master...", name);
                        let url_path = self.get_public_url_base();
                        let full_url = format!("{}{}{}", master_url.trim_end_matches('/'), url_path, name);
                        
                        let res = reqwest::Client::new().get(&full_url).send().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                        if res.status().is_success() {
                            let bytes = res.bytes().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?.to_vec();
                            let mime = mime_guess::from_path(name).first_or_octet_stream().to_string();
                            // Cache locally for future requests
                            let _ = backend.save(name, &bytes, &mime).await;
                            return Ok(bytes);
                        }
                    }
                }
                Err(e)
            }
        }
    }
    
    async fn delete(&self, name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> { self.resolve_backend().await?.0.delete(name).await }
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<(String, u64, String)>, Box<dyn std::error::Error + Send + Sync>> { self.resolve_backend().await?.0.list_prefix(prefix).await }
    async fn get_signed_url(&self, name: &str, ttl: u64) -> Result<String, Box<dyn std::error::Error + Send + Sync>> { self.resolve_backend().await?.0.get_signed_url(name, ttl).await }
    fn get_public_url_base(&self) -> String { self.public_url_prefix.clone().unwrap_or_else(|| "/api/v1/storage/file/".to_string()) }
}

// --- SCOPED DYNAMIC STORAGE ---
pub struct ScopedDynamicStorage {
    state: AppState,
    scope: EventScope,
}

impl ScopedDynamicStorage {
    pub fn new(state: AppState, scope: EventScope) -> Self {
        Self { state, scope }
    }

    async fn track_op(&self, op_type: &str) {
        if let EventScope::Tenant(tenant_id) = &self.scope {
            let key = format!("usage:{}:{}", tenant_id, op_type);
            let current = self.state.root_script_cache.get(&key).await
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
                
            self.state.root_script_cache.insert(key, (current + 1).to_string()).await;
        }
    }

    // Returns: (Backend, is_reseller, is_local)
    async fn resolve(&self) -> Result<(Arc<dyn StorageBackend>, bool, bool), Box<dyn std::error::Error + Send + Sync>> {
        let db = match &self.scope {
            EventScope::Root => return Ok((Arc::new(DynamicStorage::new(self.state.db.clone(), self.state.vault.clone(), None, "/api/v1/storage/file/".to_string())), false, true)),
            EventScope::Tenant(id) => self.state.tenant_manager.get_tenant(id.clone()).await.map_err(|e| e.to_string())?,
            EventScope::Sandbox(id) => self.state.sandbox_manager.get_sandbox(id).await.map_err(|e| e.to_string())?,
            _ => return Err("Invalid scope".into()),
        };

        let url_prefix = match &self.scope {
             EventScope::Tenant(id) => format!("/tenant/{}/api/v1/storage/file/", id),
             EventScope::Sandbox(id) => format!("/sandbox/{}/api/v1/storage/file/", id),
             _ => "/api/v1/storage/file/".to_string(),
        };

        let tenant_settings = db.get_config("storage").await?;
        if let Some(val) = tenant_settings {
            let tenant_config: StorageConfigDto = serde_json::from_value(val).unwrap_or_default();
            if tenant_config.active_driver == "s3" && tenant_config.s3.enabled {
                 let secret_key = if let Some(enc_str) = tenant_config.s3.secret_key {
                     let enc: EncryptedValue = serde_json::from_str(&enc_str)?;
                     self.state.vault.decrypt(&enc)?
                 } else { String::new() };
                 
                 let s3 = S3Storage::new_with_creds(&tenant_config.s3.bucket, &tenant_config.s3.region, &tenant_config.s3.endpoint, &url_prefix, &tenant_config.s3.access_key, &secret_key, "").await;
                 return Ok((Arc::new(s3), false, false));
            }
        }

        let root_settings = self.state.db.get_config("storage").await?;
        if let Some(val) = root_settings {
            let root_config: StorageConfigDto = serde_json::from_value(val).unwrap_or_default();
            if root_config.active_driver == "s3" && root_config.s3.enabled {
                 let secret_key = if let Some(enc_str) = root_config.s3.secret_key {
                     let enc: EncryptedValue = serde_json::from_str(&enc_str)?;
                     self.state.vault.decrypt(&enc)?
                 } else { String::new() };

                 let isolation_prefix = match &self.scope {
                     EventScope::Tenant(id) => format!("tenants/{}/uploads/", id),
                     EventScope::Sandbox(id) => format!("sandboxes/session_{}/uploads/", id),
                     _ => "".to_string(),
                 };

                 let s3 = S3Storage::new_with_creds(&root_config.s3.bucket, &root_config.s3.region, &root_config.s3.endpoint, &url_prefix, &root_config.s3.access_key, &secret_key, &isolation_prefix).await;
                 return Ok((Arc::new(s3), true, false));
            }
        }

        let fs_root = match &self.scope {
             EventScope::Tenant(id) => format!("storage/tenants/{}/uploads", id),
             EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/uploads", id),
             _ => "./storage/tmp".to_string(),
        };
        Ok((Arc::new(LocalStorage::new(&fs_root, &url_prefix).await), false, true))
    }
}

#[async_trait]
impl StorageBackend for ScopedDynamicStorage {
    async fn save(&self, name: &str, data: &[u8], content_type: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> { 
        let (backend, is_reseller, is_local) = self.resolve().await?;
        if is_reseller { self.track_op("s3_put").await; } 
        let res = backend.save(name, data, content_type).await?;

        // [REPLICA PUSH] Forward local file uploads to Master via gRPC
        if is_local {
            if let Ok(master_url) = std::env::var("APEX_MASTER_URL") {
                if !master_url.is_empty() {
                    let data_clone = data.to_vec();
                    let name_clone = name.to_string();
                    let mime_clone = content_type.to_string();
                    let scope_str = match &self.scope {
                        EventScope::Root => "root".to_string(),
                        EventScope::Tenant(id) => format!("tenant:{}", id),
                        EventScope::Sandbox(id) => format!("sandbox:{}", id),
                        _ => "root".to_string(),
                    };
                    
                    tokio::spawn(async move {
                        if let Err(e) = crate::replication::forward_file_to_master(&master_url, &scope_str, &name_clone, &mime_clone, &data_clone).await {
                            tracing::error!("Failed to sync file to master via gRPC: {}", e);
                        }
                    });
                }
            }
        }
        
        Ok(res)
    }

    async fn get(&self, name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> { 
        let (backend, is_reseller, _is_local) = self.resolve().await?;
        
        let mut result = backend.get(name).await;

        // If S3 failed, try local fallback first
        if result.is_err() && is_reseller {
             let fs_root = match &self.scope {
                  EventScope::Tenant(id) => format!("storage/tenants/{}/uploads", id),
                  EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/uploads", id),
                  _ => "./storage/system/uploads".to_string(),
             };
             let local = LocalStorage::new(&fs_root, "/").await;
             result = local.get(name).await;
        }

        // [LAZY REPLICATION] If still failed, and we are a replica, fetch from Master HTTP
        if result.is_err() {
            if let Ok(master_url) = std::env::var("APEX_MASTER_URL") {
                if !master_url.is_empty() {
                    tracing::info!("☁️ File '{}' missing locally on Replica (Scope: {:?}). Fetching from Master...", name, self.scope);
                    let url_path = self.get_public_url_base();
                    let full_url = format!("{}{}{}", master_url.trim_end_matches('/'), url_path, name);
                    
                    let res = reqwest::Client::new().get(&full_url).send().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                    if res.status().is_success() {
                        let bytes = res.bytes().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?.to_vec();
                        let mime = mime_guess::from_path(name).first_or_octet_stream().to_string();
                        
                        // Cache locally for future requests
                        let _ = backend.save(name, &bytes, &mime).await;
                        
                        if is_reseller { self.track_op("s3_get").await; }
                        return Ok(bytes);
                    } else {
                        tracing::warn!("Failed to fetch file from Master: HTTP {}", res.status());
                    }
                }
            }
        }

        match result {
            Ok(data) => {
                if is_reseller { self.track_op("s3_get").await; }
                Ok(data)
            },
            Err(e) => Err(e)
        }
    }

    async fn delete(&self, name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> { 
        let (active, is_reseller, _is_local) = self.resolve().await?;
        if is_reseller { self.track_op("s3_del").await; }
        
        let _ = active.delete(name).await;
        
        let fs_root = match &self.scope {
             EventScope::Tenant(id) => format!("storage/tenants/{}/uploads", id),
             EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/uploads", id),
             _ => "./storage/system/uploads".to_string(),
        };
        let local = LocalStorage::new(&fs_root, "/").await;
        let _ = local.delete(name).await;
        Ok(())
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<(String, u64, String)>, Box<dyn std::error::Error + Send + Sync>> { 
        self.resolve().await?.0.list_prefix(prefix).await 
    }
    
    async fn get_signed_url(&self, name: &str, ttl: u64) -> Result<String, Box<dyn std::error::Error + Send + Sync>> { 
        let (backend, is_reseller, _is_local) = self.resolve().await?;
        if is_reseller { self.track_op("s3_get").await; } 
        backend.get_signed_url(name, ttl).await 
    }

    fn get_public_url_base(&self) -> String {
        match &self.scope {
            EventScope::Root => "/api/v1/storage/file/".to_string(),
            EventScope::Tenant(id) => format!("/tenant/{}/api/v1/storage/file/", id),
            EventScope::Sandbox(id) => format!("/sandbox/{}/api/v1/storage/file/", id),
            _ => "/api/v1/storage/file/".to_string(),
        }
    }
}

// --- HANDLER: Test S3 Connection ---
#[utoipa::path(
    post,
    path = "/api/v1/admin/storage/test",
    request_body = TestS3ConfigReq,
    responses((status = 200, description = "Connection successful"), (status = 400, description = "Connection failed"), (status = 403, description = "Admin only"))
)]
pub async fn test_s3_connection(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, State(state): State<AppState>, Json(payload): Json<TestS3ConfigReq>) -> Result<Json<serde_json::Value>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let saved_json = db.get_config("storage").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let saved_config: Option<StorageConfigDto> = if let Some(val) = saved_json { serde_json::from_value(val).ok() } else { None };
    let s3_saved = saved_config.map(|c| c.s3).unwrap_or_default();

    let bucket = payload.bucket.filter(|s| !s.is_empty()).unwrap_or(s3_saved.bucket);
    let region = payload.region.filter(|s| !s.is_empty()).unwrap_or(s3_saved.region);
    let endpoint = payload.endpoint.filter(|s| !s.is_empty()).unwrap_or(s3_saved.endpoint);
    let access_key = payload.access_key.filter(|s| !s.is_empty()).unwrap_or(s3_saved.access_key);
    
    let raw_secret_key = if let Some(pk) = payload.secret_key.filter(|s| !s.is_empty() && s != "******") {
        pk
    } else if let Some(encrypted_str) = s3_saved.secret_key {
         if !encrypted_str.is_empty() {
             let enc: EncryptedValue = serde_json::from_str(&encrypted_str).map_err(|_| AppError::JsonError("Saved secret key format is invalid".into()))?;
             state.vault.decrypt(&enc).map_err(|_| AppError::UnknownError("Failed to decrypt saved secret key. Master Key mismatch?".into()))?
         } else { return Err(AppError::JsonError("Secret key is empty in database. Please enter it.".into())); }
    } else { return Err(AppError::JsonError("Secret key is missing. Please enter it explicitly.".into())); };

    if bucket.is_empty() { return Err(AppError::JsonError("Bucket is required".into())); }

    let final_region = if region.is_empty() { "auto".to_string() } else { region };
    
    let s3 = S3Storage::new_with_creds(&bucket, &final_region, &endpoint, "", &access_key, &raw_secret_key, "").await;

    let filename = ".apexkit_test_connectivity";

    s3.save(filename, b"connection_verified", "text/plain").await.map_err(|e| AppError::JsonError(format!("Connection failed: {}", e)))?;
    s3.delete(filename).await.map_err(|e| AppError::JsonError(format!("Write succeeded but Delete failed: {}. Check permissions.", e)))?;

    Ok(Json(serde_json::json!({ "success": true, "message": "Successfully connected, uploaded, and deleted test file." })))
}

// --- HANDLER: Migrate Storage ---
#[utoipa::path(
    post,
    path = "/api/v1/admin/storage/migrate",
    request_body = MigrateStorageReq,
    responses((status = 200, body = MigrationResult), (status = 403, description = "Admin only"))
)]
pub async fn migrate_storage(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, State(state): State<AppState>, Json(payload): Json<MigrateStorageReq>) -> Result<Json<MigrationResult>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    if payload.source == payload.destination { return Err(AppError::UnknownError("Source and Destination cannot be the same".into())); }

    let settings_json = db.get_config("storage").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let config: StorageConfigDto = if let Some(val) = settings_json { serde_json::from_value(val).unwrap_or_default() } else { return Err(AppError::UnknownError("Storage not configured".into())); };

    let build_backend = |req_type: &str| -> Result<Arc<dyn StorageBackend>, AppError> {
        match req_type {
            "local" => Ok(Arc::new(futures::executor::block_on(LocalStorage::new("./storage/system/uploads", "/api/v1/storage/file/")))),
            "s3" => {
                if !config.s3.enabled { return Err(AppError::UnknownError("S3 is disabled in settings".into())); }
                let raw_secret = config.s3.secret_key.as_ref().ok_or(AppError::UnknownError("S3 Secret missing".into()))?;
                let secret_key = if raw_secret.starts_with('{') {
                     let enc: EncryptedValue = serde_json::from_str(raw_secret).map_err(|_| AppError::UnknownError("Bad key format".into()))?;
                     state.vault.decrypt(&enc).map_err(|_| AppError::UnknownError("Decrypt fail".into()))?
                } else { raw_secret.clone() };
                
                let s3 = futures::executor::block_on(S3Storage::new_with_creds(&config.s3.bucket, &config.s3.region, &config.s3.endpoint, "", &config.s3.access_key, &secret_key, ""));
                Ok(Arc::new(s3))
            },
            _ => Err(AppError::UnknownError("Invalid storage type".into()))
        }
    };

    let source_backend = build_backend(&payload.source)?;
    let dest_backend = build_backend(&payload.destination)?;

    let mut offset = 0;
    let limit = 50; 
    let mut processed_count = 0;
    let mut error_count = 0;

    loop {
        let files = db.list_files(limit, offset).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        if files.is_empty() { break; }

        for file in files {
            match source_backend.get(&file.filename).await {
                Ok(data) => {
                    if let Err(_) = dest_backend.save(&file.filename, &data, &file.mime_type).await { error_count += 1; } 
                    else { processed_count += 1; }
                },
                Err(_) => { error_count += 1; }
            }
        }
        offset += limit;
    }

    Ok(Json(MigrationResult { success: true, processed: processed_count, errors: error_count, message: format!("Processed {} files. {} errors.", processed_count, error_count) }))
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>, 
    headers: axum::http::HeaderMap, 
    mut multipart: Multipart,
) -> Result<Json<FileResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let user_id = claims.as_ref().map(|c| c.uid);
    if user_id.is_none() { return Err(AppError::Unauthorized("Login required".into())); }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    trigger_void_hook(&state, "before_file_upload", serde_json::json!({}), claims.as_ref(), Some(&event_scope.clone()), Some(base_url.clone())).await?;

    while let Some(field) = multipart.next_field().await.map_err(|_| AppError::InputValidation(validator::ValidationErrors::new()))? {
        let original_name = field.file_name().unwrap_or("unknown.bin").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
        let data = field.bytes().await.map_err(|_| AppError::UnknownError("Failed bytes".into()))?;
        let size = data.len() as i64;
        
        let extension = std::path::Path::new(&original_name).extension().and_then(|ext| ext.to_str()).unwrap_or("bin");
        let filename = format!("{}.{}", uuid::Uuid::new_v4(), extension);

        storage.save(&filename, &data, &content_type).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

        let id = db.create_file_metadata(&filename, &original_name, &content_type, size, user_id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

        let meta = extract_log_meta(&headers, Some(addr), serde_json::json!({ "filename": filename, "original": original_name, "size": size }));
        let _ = db.log_audit_event("info", "File Uploaded", "storage", Some(meta)).await;

        let url = format!("{}{}", storage.get_public_url_base(), filename);

        let _ = trigger_void_hook(&state, "after_file_upload", serde_json::json!({ "id": id, "filename": filename }), claims.as_ref(),  Some(&event_scope.clone()), Some(base_url.clone())).await;

        return Ok(Json(FileResponse { id, url, filename }));
    }
    Err(AppError::InputValidation(validator::ValidationErrors::new()))
}

// --- HANDLER: Serve Generic File ---
#[utoipa::path(
    get,
    path = "/api/v1/storage/file/{filename}",
    params(FilenamePath, FileParams),
    responses((status = 200, description = "File Content"))
)]
pub async fn serve_file(
    StorageConnection(storage): StorageConnection,
    State(state): State<AppState>, 
    Path(path): Path<FilenamePath>, 
    Query(params): Query<FileParams>,
) -> Result<Response, AppError> {
    
    if path.filename.contains("..") { return Err(AppError::Forbidden("Invalid path".into())); }
    let clean_filename = path.filename.trim_start_matches('/');

    let mut original_bytes = storage.get(clean_filename).await;

    if original_bytes.is_err() {
        tracing::warn!("Primary storage failed for '{}'. Attempting Root Local Fallback...", clean_filename);
        let root_local = LocalStorage::new("./storage/system/uploads", "/").await;
        original_bytes = root_local.get(clean_filename).await;
    }

    let data = original_bytes.map_err(|e| {
        tracing::error!("Storage failure for {}: {}", clean_filename, e);
        AppError::NotFound("File not found".into())
    })?;
    
    let mime_type = if clean_filename.ends_with(".m4s") { "video/iso.segment".to_string() } 
    else if clean_filename.ends_with(".mpd") { "application/dash+xml".to_string() } 
    else if clean_filename.ends_with(".m3u8") { "application/vnd.apple.mpegurl".to_string() } 
    else if clean_filename.ends_with(".ts") { "video/mp2t".to_string() } 
    else { mime_guess::from_path(clean_filename).first_or_octet_stream().to_string() };

    process_image(&state, data, &mime_type, clean_filename.to_string(), params.thumb, params.format, params.quality).await
}

// --- HANDLER: List Files ---
#[utoipa::path(get, path = "/api/v1/storage/files", params(FileListQuery), responses((status = 200, body = FileListResponse)))]
pub async fn list_files(DatabaseConnection(db): DatabaseConnection, Query(params): Query<FileListQuery>) -> Result<Json<FileListResponse>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * limit;
    let files = db.list_files(limit, offset).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let total = db.count_files().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(FileListResponse { items: files, total }))
}

// --- HANDLER: Delete File ---
#[utoipa::path(delete, path = "/api/v1/storage/files/{id}", params(FileIdPath), responses((status = 204, description = "File deleted")))]
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
    let user_id = claims.as_ref().map(|c| c.uid);
    if user_id.is_none() { return Err(AppError::Unauthorized("Login required".into())); }
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    
    trigger_void_hook(&state, "before_file_delete", serde_json::json!({ "id": path.id }), claims.as_ref(), Some(&event_scope.clone()), Some(base_url.clone())).await?;

    let file = db.get_file_metadata(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("File".into()))?;
    storage.delete(&file.filename).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    db.delete_file_metadata(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    let meta = extract_log_meta(&headers, Some(addr), serde_json::json!({ "id": path.id, "filename": file.filename }));
    let _ = db.log_audit_event("warning", "File Deleted", "storage", Some(meta)).await;

    Ok(StatusCode::NO_CONTENT)
}

// --- HELPER: Centralized Image Processing ---
// --- HELPER: Centralized Image Processing ---
async fn process_image(
    state: &AppState, 
    original_bytes: Vec<u8>, 
    original_mime: &str, 
    cache_key: String, 
    dim_str: Option<String>,
    req_format: Option<String>,
    req_quality: Option<u8>
) -> Result<Response, AppError> {
    let cache_header_val = "public, max-age=31536000, immutable";
    let etag = format!("\"{:x}\"", md5::compute(&original_bytes)); 

    if (dim_str.is_none() && req_format.is_none() && req_quality.is_none()) || original_mime.contains("svg") {
         return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, original_mime)
            .header(header::CACHE_CONTROL, cache_header_val)
            .header(header::ETAG, etag)
            .body(Body::from(original_bytes))
            .unwrap());
    }

    // Default to 80 quality for good web balance
    let quality = req_quality.unwrap_or(80).clamp(1, 100);

    let (target_format, target_mime) = match req_format.as_deref().unwrap_or("").to_lowercase().as_str() {
        "webp" => (image::ImageFormat::WebP, "image/webp"),
        "jpg" | "jpeg" => (image::ImageFormat::Jpeg, "image/jpeg"),
        "png" => (image::ImageFormat::Png, "image/png"),
        "avif" => (image::ImageFormat::Avif, "image/avif"),
        "gif" => (image::ImageFormat::Gif, "image/gif"),
        _ => (image::ImageFormat::from_mime_type(original_mime).unwrap_or(image::ImageFormat::Png), original_mime)
    };

    let dim_part = dim_str.as_deref().unwrap_or("orig");
    let fmt_part = target_mime.split('/').last().unwrap_or("bin");
    let full_cache_key = format!("{}_{}_{}_q{}", cache_key, dim_part, fmt_part, quality);

    if let Some(cached_bytes) = state.thumb_cache.get(&full_cache_key).await {
        let thumb_etag = format!("\"{:x}\"", md5::compute(cached_bytes.as_ref()));
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, target_mime)
            .header(header::CACHE_CONTROL, cache_header_val)
            .header(header::ETAG, thumb_etag)
            .body(Body::from(cached_bytes.as_ref().clone()))
            .unwrap());
    }

    let (w, h) = if let Some(d) = &dim_str { parse_dimensions(d).unwrap_or((0, 0)) } else { (0, 0) };
    let bytes_for_processing = original_bytes.clone();
    
    let img_result = tokio::task::spawn_blocking(move || { 
        image::load_from_memory(&bytes_for_processing) 
    }).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    match img_result {
        Ok(img) => {
            let processed_img = if w > 0 || h > 0 {
                let target_w = if w == 0 { u32::MAX } else { w };
                let target_h = if h == 0 { u32::MAX } else { h };
                // Triangle filter is much faster than Lanczos3 and perfectly fine for downscaling on the web
                img.resize(target_w, target_h, FilterType::Triangle)
            } else {
                img
            };

            let mut buffer = Cursor::new(Vec::new());
            let encoding_success = match target_format {
                image::ImageFormat::Jpeg => JpegEncoder::new_with_quality(&mut buffer, quality).encode_image(&processed_img).is_ok(),
                image::ImageFormat::Avif => AvifEncoder::new_with_speed_quality(&mut buffer, 8, quality).write_image(processed_img.as_bytes(), processed_img.width(), processed_img.height(), processed_img.color()).is_ok(),
                _ => processed_img.write_to(&mut buffer, target_format).is_ok(),
            };

            if !encoding_success {
                tracing::warn!("Image encoding to {} failed. Serving original.", target_mime);
                return Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, original_mime)
                    .header(header::CACHE_CONTROL, cache_header_val)
                    .header(header::ETAG, etag)
                    .body(Body::from(original_bytes))
                    .unwrap());
            }
            
            let thumb_bytes = buffer.into_inner();
            state.thumb_cache.insert(full_cache_key, Arc::new(thumb_bytes.clone())).await;
            
            let thumb_etag = format!("\"{:x}\"", md5::compute(&thumb_bytes));
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, target_mime)
                .header(header::CACHE_CONTROL, cache_header_val)
                .header(header::ETAG, thumb_etag)
                .body(Body::from(thumb_bytes))
                .unwrap())
        },
        Err(e) => { 
            tracing::warn!("Failed to load image for processing: {}. Serving original.", e);
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, original_mime)
                .header(header::CACHE_CONTROL, cache_header_val)
                .header(header::ETAG, etag)
                .body(Body::from(original_bytes))
                .unwrap()) 
        }
    }
}

// --- HANDLER: Serve App Logo ---
#[utoipa::path(get, path = "/logo", params(FileParams), responses((status = 200, description = "App Logo")))]
pub async fn serve_app_logo(DatabaseConnection(db): DatabaseConnection, StorageConnection(storage): StorageConnection, State(state): State<AppState>, Query(params): Query<FileParams>) -> Result<Response, AppError> {
    let settings = db.get_config("general").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let (bytes, mime, cache_key_base) = if let Some(val) = settings {
        if let Some(logo_filename) = val.get("app_logo").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            match storage.get(logo_filename).await {
                Ok(b) => { let m = mime_guess::from_path(logo_filename).first_or_octet_stream(); (b, m.to_string(), logo_filename.to_string()) },
                Err(_) => {
                    let root_local = LocalStorage::new("./storage/system/uploads", "/").await;
                    match root_local.get(logo_filename).await {
                         Ok(b) => { let m = mime_guess::from_path(logo_filename).first_or_octet_stream(); (b, m.to_string(), logo_filename.to_string()) },
                         Err(_) => get_default_logo()?
                    }
                }
            }
        } else { get_default_logo()? }
    } else { get_default_logo()? };
    
    process_image(&state, bytes, &mime, format!("logo_{}", cache_key_base), params.thumb, params.format, params.quality).await
}

fn get_default_logo() -> Result<(Vec<u8>, String, String), AppError> {
    let default_path = "images/apexkit-logo.svg"; 
    match Assets::get(default_path) {
        Some(content) => { let mime = mime_guess::from_path(default_path).first_or_octet_stream(); Ok((content.data.to_vec(), mime.to_string(), "default".to_string())) },
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