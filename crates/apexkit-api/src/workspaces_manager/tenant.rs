use apexkit_core::batching::WriteForwarder;
use apexkit_core::cache::CachedDb;
use apexkit_core::models::ChangesetEvent;
use apexkit_core::search::SearchManager;
use apexkit_core::{ApexKit, Db, VectorProvider};
use apexkit_vector::{CandleEmbedder, VectorIndex};
use moka::future::Cache;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::info;

// --- 1. Isolated Vector Provider ---
struct TenantVectorProvider {
    embedder: Option<Arc<CandleEmbedder>>,
    index: Arc<VectorIndex>,
    ai_request_counter: Arc<AtomicU64>,
}

#[async_trait::async_trait]
impl VectorProvider for TenantVectorProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        self.ai_request_counter.fetch_add(1, Ordering::Relaxed);
        if let Some(embedder) = &self.embedder {
            let embedder = embedder.clone();
            let t = text.to_string();
            tokio::task::spawn_blocking(move || embedder.embed(&t).map_err(|e| e.to_string()))
                .await
                .map_err(|e| e.to_string())?
        } else {
            Err("Vector AI is disabled (Embedder not initialized)".to_string())
        }
    }

    async fn embed_image(&self, base64_image: &str) -> Result<Vec<f32>, String> {
        self.ai_request_counter.fetch_add(1, Ordering::Relaxed);
        if let Some(embedder) = &self.embedder {
            let embedder = embedder.clone();
            let img = base64_image.to_string();
            tokio::task::spawn_blocking(move || {
                embedder.embed_image(&img).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())?
        } else {
            Err("Vector AI is disabled (Embedder not initialized)".to_string())
        }
    }

    async fn embed_text_for_image_search(&self, text: &str) -> Result<Vec<f32>, String> {
        self.ai_request_counter.fetch_add(1, Ordering::Relaxed);
        if let Some(embedder) = &self.embedder {
            let embedder = embedder.clone();
            let txt = text.to_string();
            tokio::task::spawn_blocking(move || {
                embedder
                    .embed_text_for_image_search(&txt)
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())?
        } else {
            Err("Vector AI is disabled (Embedder not initialized)".to_string())
        }
    }

    async fn search(
        &self,
        col: i64,
        f: &str,
        v: &[f32],
        l: usize,
    ) -> Result<Vec<(i64, f32)>, String> {
        Ok(self.index.search(col, f, v, l))
    }

    async fn index(&self, c: i64, r: i64, f: &str, v: &[f32]) -> Result<(), String> {
        self.index.insert(c, r, f, v);
        Ok(())
    }

    fn get_and_reset_metrics(&self) -> u64 {
        self.ai_request_counter.swap(0, Ordering::Relaxed)
    }
}

// --- 2. Context Container ---
#[derive(Clone)]
pub struct TenantContext {
    pub db: Arc<dyn Db>,
    pub vector_provider: Arc<dyn VectorProvider>,
    pub status: String,
    // Isolated Cache
    pub script_cache: Cache<String, String>,
}

// --- 3. Tenant Manager ---
#[derive(Clone)]
pub struct TenantManager {
    // [OPTIMIZATION] Cache key is tenant_id
    pub cache: Cache<String, TenantContext>,
    shared_embedder: Option<Arc<CandleEmbedder>>,
    // We need access to Root DB to fetch status during load
    root_db: Arc<dyn Db>,
    forwarder: Option<Arc<dyn WriteForwarder>>,
    // Add the event transmitter
    event_tx: Option<tokio::sync::broadcast::Sender<ChangesetEvent>>,
}

impl TenantManager {
    pub fn new(
        shared_embedder: Option<Arc<CandleEmbedder>>,
        root_db: Arc<dyn Db>,
        capacity: u64,
        forwarder: Option<Arc<dyn WriteForwarder>>,
        event_tx: Option<tokio::sync::broadcast::Sender<ChangesetEvent>>,
    ) -> Self {
        let _ = std::fs::create_dir_all("storage/tenants");
        Self {
            cache: Cache::builder()
                .max_capacity(capacity)
                .time_to_idle(Duration::from_secs(3600))
                .build(),
            shared_embedder,
            root_db,
            forwarder,
            event_tx,
        }
    }

    /// RETRIEVES the specific Vector Provider for a tenant.
    pub async fn get_vector_provider(&self, tenant_id: &str) -> Option<Arc<dyn VectorProvider>> {
        self.get_tenant_context(tenant_id)
            .await
            .ok()
            .map(|c| c.vector_provider)
    }

    // [UPDATED] Returns Context (DB + Status)
    pub async fn get_tenant_context(&self, tenant_id: &str) -> Result<TenantContext, String> {
        if let Some(ctx) = self.cache.get(tenant_id).await {
            return Ok(ctx);
        }

        // We removed the `!Path::new(&base_path).exists()` check here.
        // `load_tenant` handles checking the Root DB and triggers the replica snapshot sync
        // if the files are missing locally.

        let ctx = self.load_tenant(tenant_id).await?;
        self.cache.insert(tenant_id.to_string(), ctx.clone()).await;
        Ok(ctx)
    }

    // Convenience method for just DB
    pub async fn get_tenant(&self, tenant_id: String) -> Result<Arc<dyn Db>, String> {
        let ctx = self.get_tenant_context(&tenant_id).await?;
        Ok(ctx.db)
    }

    /// EXPLICITLY CREATES a new tenant.
    pub async fn create_tenant(&self, tenant_id: String) -> Result<Arc<dyn Db>, String> {
        let base_path = format!("storage/tenants/{}", tenant_id);

        if Path::new(&base_path).exists() {
            return Err(format!("Tenant '{}' already exists", tenant_id));
        }

        info!("Provisioning new tenant: {}", tenant_id);

        // Create directory structure
        std::fs::create_dir_all(&base_path).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(format!("{}/uploads", base_path)).ok();
        std::fs::create_dir_all(format!("{}/indexes", base_path)).ok();

        // Initialize Filesystem
        let ctx = self.load_tenant(&tenant_id).await?;

        // Cache it immediately
        self.cache.insert(tenant_id, ctx.clone()).await;

        Ok(ctx.db)
    }

    /// Deletes a tenant from Disk and Cache
    pub async fn delete_tenant(&self, tenant_id: &str) -> Result<(), String> {
        // 1. Remove from cache
        self.cache.invalidate(tenant_id).await;

        // 2. Delete files
        let base_path = format!("storage/tenants/{}", tenant_id);
        if std::path::Path::new(&base_path).exists() {
            std::fs::remove_dir_all(&base_path).map_err(|e| e.to_string())?;
            info!("Tenant '{}' deleted from disk", tenant_id);
        }

        Ok(())
    }

    // [NEW] Invalidate cache (Called when Root updates status)
    pub async fn invalidate(&self, tenant_id: &str) {
        self.cache.invalidate(tenant_id).await;
    }

    async fn load_tenant(&self, tenant_id: &str) -> Result<TenantContext, String> {
        let base_path = format!("storage/tenants/{}", tenant_id);

        tracing::info!("[TenantManager] Attempting to load tenant: {}", tenant_id);

        // 1. Sync Files from Master FIRST (Wait for it to complete)
        crate::replication::ensure_replica_env(&base_path).await;

        // 2. TELL REPLICA EVENT STREAMER TO LISTEN TO THIS TENANT
        crate::replication::add_replica_subscription(&format!("tenant:{}", tenant_id));

        // 3. Fetch Status from Root DB (Safely fallback if replica lags)
        let status = self
            .root_db
            .get_tenant_status(tenant_id)
            .await
            .unwrap_or_else(|_| "not_found".to_string());

        // 4. Verify Files Exist
        let expected_db = format!("{}/data.db", base_path);
        let files_exist = std::path::Path::new(&expected_db).exists();
        let is_replica = std::env::var("APEX_MASTER_URL")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);

        // If the status says the tenant doesn't exist but the files are physically here,
        // it means the replica's Root DB is slightly lagging behind the Master. We bypass this to allow initiation.
        if status == "not_found" {
            if files_exist {
                tracing::warn!(
                    "[TenantManager] Tenant '{}' not found in Root DB registry, BUT files exist (likely replica lag). Proceeding...",
                    tenant_id
                );
            } else {
                tracing::warn!(
                    "[TenantManager] Tenant '{}' not found in Root DB registry.",
                    tenant_id
                );
                return Err("Tenant not found in registry".into());
            }
        } else if !files_exist {
            if is_replica {
                tracing::error!(
                    "[TenantManager] Snapshot fetch failed for {}. Database file {} does not exist.",
                    tenant_id,
                    expected_db
                );
                return Err("Failed to sync tenant database from master".into());
            } else {
                tracing::info!(
                    "[TenantManager] Master node creating new database files for tenant: {}",
                    tenant_id
                );
                // We don't return an error here, we let `ApexKit::init_filesystem` proceed and create the DBs.
            }
        }

        tracing::info!(
            "[TenantManager] Successfully verified files and status for tenant: {}",
            tenant_id
        );

        // 5. Initialize DB (Standard Logic)
        let vector_index = Arc::new(VectorIndex::new());
        let tenant_vector_provider = Arc::new(TenantVectorProvider {
            embedder: self.shared_embedder.clone(),
            index: vector_index.clone(),
            ai_request_counter: Arc::new(AtomicU64::new(0)),
        });

        // Pass None for forwarder and event_tx
        let mut apexkit = ApexKit::init_filesystem(
            &base_path,
            tenant_vector_provider.clone(),
            self.forwarder.clone(), // <--- Forward writes to Master
            self.event_tx.clone(),  // <--- Forward changesets to gRPC
            format!("tenant:{}", tenant_id),
        )
        .await
        .map_err(|e| {
            tracing::error!(
                "[TenantManager] Failed to init filesystem for {}: {}",
                tenant_id,
                e
            );
            e.to_string()
        })?;

        let search_path = format!("{}/indexes", base_path);
        let search_manager = Arc::new(SearchManager::new(&search_path));
        apexkit.set_search_manager(search_manager);

        let db_arc: Arc<dyn Db> = Arc::new(CachedDb::new(Arc::new(apexkit.clone())));

        // 3. Hydrate Vectors (Background)
        let db_clone = db_arc.clone();
        let vec_provider_clone = tenant_vector_provider.clone();

        // Fetch both models so we load text AND image vectors into the index
        let active_text_model = apexkit_vector::get_current_text_model();
        let active_vision_model = apexkit_vector::get_current_vision_model();

        tokio::spawn(async move {
            if let Ok(cols) = db_clone.list_collections().await {
                for col in cols {
                    for active_model in [&active_text_model, &active_vision_model] {
                        if let Ok(vecs) = db_clone
                            .get_vectors_for_collection(col.id, active_model)
                            .await
                        {
                            for (rid, field, vec) in vecs {
                                let _ = vec_provider_clone.index(col.id, rid, &field, &vec).await;
                            }
                        }
                    }
                }
            }
        });

        let cache_size = std::env::var("SCRIPT_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(100);

        let script_cache = Cache::builder()
            .max_capacity(cache_size)
            .time_to_live(Duration::from_secs(300)) // 5 mins default
            .build();

        Ok(TenantContext {
            db: db_arc,
            vector_provider: tenant_vector_provider,
            status: if status == "not_found" {
                "active".to_string()
            } else {
                status
            },
            script_cache,
        })
    }

    pub async fn list_tenants(&self) -> Result<Vec<String>, String> {
        let mut tenants = Vec::new();
        let path = Path::new("storage/tenants");

        if path.exists() {
            let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;
            for entry in entries {
                if let Ok(entry) = entry
                    && let Ok(file_type) = entry.file_type()
                    && file_type.is_dir()
                    && let Ok(name) = entry.file_name().into_string()
                {
                    tenants.push(name);
                }
            }
        }
        Ok(tenants)
    }
}
