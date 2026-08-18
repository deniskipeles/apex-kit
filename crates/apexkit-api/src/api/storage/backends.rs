use crate::AppState;
use crate::system::dto::StorageConfigDto;
use apexkit_core::realtime::EventScope;
use apexkit_core::{
    Db,
    security::vault::{EncryptedValue, Vault},
    storage::{LocalStorage, S3Storage, StorageBackend},
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

pub fn get_storage_path(subpath: &str) -> String {
    if let Ok(base) = std::env::var("APEXKIT_MOUNTED_FILE_STORAGE") {
        let clean_base = base.trim_end_matches('/');
        let clean_sub = subpath.trim_start_matches('/');
        format!("{}/{}", clean_base, clean_sub)
    } else {
        subpath.to_string()
    }
}

// --- DYNAMIC STORAGE PROXY (Root Only) ---
pub struct DynamicStorage {
    db: Arc<dyn Db>,
    vault: Arc<Vault>,
    backend_cache: RwLock<Option<(Arc<dyn StorageBackend>, bool)>>,
    last_update: RwLock<std::time::Instant>,
    fs_root_override: Option<String>,
    public_url_prefix: Option<String>,
}

impl DynamicStorage {
    pub fn new(
        db: Arc<dyn Db>,
        vault: Arc<Vault>,
        fs_root_override: Option<String>,
        public_url_prefix: String,
    ) -> Self {
        Self {
            db,
            vault,
            backend_cache: RwLock::new(None),
            last_update: RwLock::new(std::time::Instant::now()),
            fs_root_override,
            public_url_prefix: Some(public_url_prefix),
        }
    }

    async fn resolve_backend(
        &self,
    ) -> Result<(Arc<dyn StorageBackend>, bool), Box<dyn std::error::Error + Send + Sync>> {
        {
            let cache = self.backend_cache.read().await;
            let time = self.last_update.read().await;
            if let Some(cached) = cache.as_ref()
                && time.elapsed() < std::time::Duration::from_secs(60)
            {
                return Ok(cached.clone());
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

        let (backend, is_local): (Arc<dyn StorageBackend>, bool) =
            if config.active_driver == "s3" && config.s3.enabled {
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
                    "__root_app__/",
                )
                .await;
                (Arc::new(s3), false)
            } else {
                let path = self
                    .fs_root_override
                    .clone()
                    .unwrap_or_else(|| get_storage_path("storage/system/uploads"));
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
    async fn save(
        &self,
        name: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let (backend, is_local) = self.resolve_backend().await?;
        let res = backend.save(name, data, content_type).await?;

        if is_local
            && let Ok(master_url) = std::env::var("APEXKIT_MASTER_URL")
            && !master_url.is_empty()
        {
            let data_clone = data.to_vec();
            let name_clone = name.to_string();
            let mime_clone = content_type.to_string();

            tokio::spawn(async move {
                if let Err(e) = crate::replication::forward_file_to_master(
                    &master_url,
                    "root",
                    &name_clone,
                    &mime_clone,
                    &data_clone,
                )
                .await
                {
                    tracing::error!("Failed to sync file to master via gRPC: {}", e);
                }
            });
        }
        Ok(res)
    }

    async fn get(&self, name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let (backend, _is_local) = self.resolve_backend().await?;
        match backend.get(name).await {
            Ok(data) => Ok(data),
            Err(e) => {
                if let Ok(master_url) = std::env::var("APEXKIT_MASTER_URL")
                    && !master_url.is_empty()
                {
                    tracing::info!(
                        "☁️ File '{}' missing locally on Replica. Fetching from Master...",
                        name
                    );
                    let url_path = self.get_public_url_base();
                    let full_url =
                        format!("{}{}{}", master_url.trim_end_matches('/'), url_path, name);

                    let res = reqwest::Client::new()
                        .get(&full_url)
                        .send()
                        .await
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                    if res.status().is_success() {
                        let bytes = res
                            .bytes()
                            .await
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                            .to_vec();
                        let mime = mime_guess::from_path(name)
                            .first_or_octet_stream()
                            .to_string();
                        let _ = backend.save(name, &bytes, &mime).await;
                        return Ok(bytes);
                    }
                }
                Err(e)
            }
        }
    }

    async fn delete(&self, name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.resolve_backend().await?.0.delete(name).await
    }

    async fn list_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, u64, String)>, Box<dyn std::error::Error + Send + Sync>> {
        self.resolve_backend().await?.0.list_prefix(prefix).await
    }

    async fn get_signed_url(
        &self,
        name: &str,
        ttl: u64,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.resolve_backend()
            .await?
            .0
            .get_signed_url(name, ttl)
            .await
    }

    fn get_public_url_base(&self) -> String {
        self.public_url_prefix
            .clone()
            .unwrap_or_else(|| "/api/v1/storage/file/".to_string())
    }
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
            let current = self
                .state
                .root_script_cache
                .get(&key)
                .await
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);

            self.state
                .root_script_cache
                .insert(key, (current + 1).to_string())
                .await;
        }
    }

    async fn resolve(
        &self,
    ) -> Result<(Arc<dyn StorageBackend>, bool, bool), Box<dyn std::error::Error + Send + Sync>>
    {
        let db = match &self.scope {
            EventScope::Root => {
                return Ok((
                    Arc::new(DynamicStorage::new(
                        self.state.db.clone(),
                        self.state.vault.clone(),
                        None,
                        "/api/v1/storage/file/".to_string(),
                    )),
                    false,
                    true,
                ));
            }
            EventScope::Tenant(id) => self
                .state
                .tenant_manager
                .get_tenant(id.clone())
                .await
                .map_err(|e| e.to_string())?,
            EventScope::Sandbox(id) => self
                .state
                .sandbox_manager
                .get_sandbox(id)
                .await
                .map_err(|e| e.to_string())?,
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
                } else {
                    String::new()
                };

                let s3 = S3Storage::new_with_creds(
                    &tenant_config.s3.bucket,
                    &tenant_config.s3.region,
                    &tenant_config.s3.endpoint,
                    &url_prefix,
                    &tenant_config.s3.access_key,
                    &secret_key,
                    "",
                )
                .await;
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
                } else {
                    String::new()
                };

                let isolation_prefix = match &self.scope {
                    EventScope::Tenant(id) => format!("tenants/{}/uploads/", id),
                    EventScope::Sandbox(id) => format!("sandboxes/session_{}/uploads/", id),
                    _ => "__root_app__/".to_string(),
                };

                let s3 = S3Storage::new_with_creds(
                    &root_config.s3.bucket,
                    &root_config.s3.region,
                    &root_config.s3.endpoint,
                    &url_prefix,
                    &root_config.s3.access_key,
                    &secret_key,
                    &isolation_prefix,
                )
                .await;
                return Ok((Arc::new(s3), true, false));
            }
        }

        let fs_root = match &self.scope {
            EventScope::Tenant(id) => get_storage_path(&format!("storage/tenants/{}/uploads", id)),
            EventScope::Sandbox(id) => {
                get_storage_path(&format!("storage/sandboxes/session_{}/uploads", id))
            }
            _ => get_storage_path("storage/tmp"),
        };
        Ok((
            Arc::new(LocalStorage::new(&fs_root, &url_prefix).await),
            false,
            true,
        ))
    }
}

#[async_trait]
impl StorageBackend for ScopedDynamicStorage {
    async fn save(
        &self,
        name: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let (backend, is_reseller, is_local) = self.resolve().await?;
        if is_reseller {
            self.track_op("s3_put").await;
        }
        let res = backend.save(name, data, content_type).await?;

        if is_local
            && let Ok(master_url) = std::env::var("APEXKIT_MASTER_URL")
            && !master_url.is_empty()
        {
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
                if let Err(e) = crate::replication::forward_file_to_master(
                    &master_url,
                    &scope_str,
                    &name_clone,
                    &mime_clone,
                    &data_clone,
                )
                .await
                {
                    tracing::error!("Failed to sync file to master via gRPC: {}", e);
                }
            });
        }

        Ok(res)
    }

    async fn get(&self, name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let (backend, is_reseller, _is_local) = self.resolve().await?;

        let mut result = backend.get(name).await;

        if result.is_err() && is_reseller {
            let fs_root = match &self.scope {
                EventScope::Tenant(id) => {
                    get_storage_path(&format!("storage/tenants/{}/uploads", id))
                }
                EventScope::Sandbox(id) => {
                    get_storage_path(&format!("storage/sandboxes/session_{}/uploads", id))
                }
                _ => get_storage_path("storage/system/uploads"),
            };
            let local = LocalStorage::new(&fs_root, "/").await;
            result = local.get(name).await;
        }

        if result.is_err()
            && let Ok(master_url) = std::env::var("APEXKIT_MASTER_URL")
            && !master_url.is_empty()
        {
            tracing::info!(
                "☁️ File '{}' missing locally on Replica (Scope: {:?}). Fetching from Master...",
                name,
                self.scope
            );
            let url_path = self.get_public_url_base();
            let full_url = format!("{}{}{}", master_url.trim_end_matches('/'), url_path, name);

            let res = reqwest::Client::new()
                .get(&full_url)
                .send()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            if res.status().is_success() {
                let bytes = res
                    .bytes()
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                    .to_vec();
                let mime = mime_guess::from_path(name)
                    .first_or_octet_stream()
                    .to_string();

                let _ = backend.save(name, &bytes, &mime).await;

                if is_reseller {
                    self.track_op("s3_get").await;
                }
                return Ok(bytes);
            } else {
                tracing::warn!("Failed to fetch file from Master: HTTP {}", res.status());
            }
        }

        match result {
            Ok(data) => {
                if is_reseller {
                    self.track_op("s3_get").await;
                }
                Ok(data)
            }
            Err(e) => Err(e),
        }
    }

    async fn delete(&self, name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (active, is_reseller, _is_local) = self.resolve().await?;
        if is_reseller {
            self.track_op("s3_del").await;
        }

        let _ = active.delete(name).await;

        let fs_root = match &self.scope {
            EventScope::Tenant(id) => get_storage_path(&format!("storage/tenants/{}/uploads", id)),
            EventScope::Sandbox(id) => {
                get_storage_path(&format!("storage/sandboxes/session_{}/uploads", id))
            }
            _ => get_storage_path("storage/system/uploads"),
        };
        let local = LocalStorage::new(&fs_root, "/").await;
        let _ = local.delete(name).await;
        Ok(())
    }

    async fn list_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, u64, String)>, Box<dyn std::error::Error + Send + Sync>> {
        self.resolve().await?.0.list_prefix(prefix).await
    }

    async fn get_signed_url(
        &self,
        name: &str,
        ttl: u64,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let (backend, is_reseller, _is_local) = self.resolve().await?;
        if is_reseller {
            self.track_op("s3_get").await;
        }
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
