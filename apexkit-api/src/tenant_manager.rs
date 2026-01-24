use std::sync::Arc;
use moka::future::Cache;
use apexkit_core::{Db, ApexKit, VectorProvider};
use apexkit_core::search::SearchManager;
use apex_vector::{CandleEmbedder, VectorIndex}; 
use std::time::Duration;
use apexkit_core::cache::CachedDb;
use std::path::Path;
use tracing::{info, error};

// --- 1. Isolated Vector Provider ---
struct TenantVectorProvider {
    embedder: Option<Arc<CandleEmbedder>>, 
    index: Arc<VectorIndex>,
}

#[async_trait::async_trait]
impl VectorProvider for TenantVectorProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        if let Some(embedder) = &self.embedder {
            let embedder = embedder.clone();
            let t = text.to_string();
            tokio::task::spawn_blocking(move || {
                embedder.embed(&t).map_err(|e| e.to_string())
            }).await.map_err(|e| e.to_string())?
        } else {
            Err("Vector AI is disabled (Embedder not initialized)".to_string())
        }
    }

    async fn search(&self, col: i64, f: &str, v: &[f32], l: usize) -> Result<Vec<(i64, f32)>, String> {
        Ok(self.index.search(col, f, v, l))
    }

    async fn index(&self, c: i64, r: i64, f: &str, v: &[f32]) -> Result<(), String> {
        self.index.insert(c, r, f, v);
        Ok(())
    }
}

// --- 2. Context Container ---
#[derive(Clone)]
pub struct TenantContext {
    pub db: Arc<dyn Db>,
    pub vector_provider: Arc<dyn VectorProvider>,
    pub status: String,
}

// --- 3. Tenant Manager ---
#[derive(Clone)]
pub struct TenantManager {
    // [OPTIMIZATION] Cache key is tenant_id
    pub cache: Cache<String, TenantContext>,
    shared_embedder: Option<Arc<CandleEmbedder>>,
    // We need access to Root DB to fetch status during load
    root_db: Arc<dyn Db>, 
}

impl TenantManager {
    pub fn new(shared_embedder: Option<Arc<CandleEmbedder>>, root_db: Arc<dyn Db>, capacity: u64) -> Self {
        let _ = std::fs::create_dir_all("storage/tenants");
        Self {
            cache: Cache::builder()
                .max_capacity(capacity)
                .time_to_idle(Duration::from_secs(3600))
                .build(),
            shared_embedder,
            root_db,
        }
    }

    /// RETRIEVES the specific Vector Provider for a tenant.
    pub async fn get_vector_provider(&self, tenant_id: &str) -> Option<Arc<dyn VectorProvider>> {
        self.get_tenant_context(tenant_id).await.ok().map(|c| c.vector_provider)
    }

    // [UPDATED] Returns Context (DB + Status)
    pub async fn get_tenant_context(&self, tenant_id: &str) -> Result<TenantContext, String> {
        if let Some(ctx) = self.cache.get(tenant_id).await {
            return Ok(ctx);
        }

        let base_path = format!("storage/tenants/{}", tenant_id);
        if !Path::new(&base_path).exists() {
            return Err(format!("Tenant '{}' does not exist", tenant_id));
        }

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
        std::fs::create_dir_all(&format!("{}/uploads", base_path)).ok();
        std::fs::create_dir_all(&format!("{}/indexes", base_path)).ok();

        // Initialize Filesystem
        let ctx = self.load_tenant(&tenant_id).await?;
        
        // Cache it immediately
        self.cache.insert(tenant_id, ctx.clone()).await;

        Ok(ctx.db)
    }

    // [NEW] Invalidate cache (Called when Root updates status)
    pub async fn invalidate(&self, tenant_id: &str) {
        self.cache.invalidate(tenant_id).await;
    }

    async fn load_tenant(&self, tenant_id: &str) -> Result<TenantContext, String> {
        // 1. Fetch Status from Root DB (The "One Hit")
        let status = self.root_db.get_tenant_status(tenant_id).await
            .map_err(|e| format!("Failed to fetch status: {}", e))?;

        // If explicitly deleted/not_found in DB, deny load even if files exist
        if status == "not_found" {
             return Err("Tenant not found in registry".into());
        }

        // 2. Initialize DB (Standard Logic)
        let base_path = format!("storage/tenants/{}", tenant_id);
        let vector_index = Arc::new(VectorIndex::new());
        let tenant_vector_provider = Arc::new(TenantVectorProvider {
            embedder: self.shared_embedder.clone(),
            index: vector_index.clone(),
        });

        let mut apexkit = ApexKit::init_filesystem(&base_path, tenant_vector_provider.clone())
            .await.map_err(|e| e.to_string())?;

        let search_path = format!("{}/indexes", base_path);
        let search_manager = Arc::new(SearchManager::new(&search_path));
        apexkit.set_search_manager(search_manager);

        let db_arc: Arc<dyn Db> = Arc::new(CachedDb::new(Arc::new(apexkit.clone())));
        
        // 3. Hydrate Vectors (Background)
        let db_clone = db_arc.clone();
        let vec_provider_clone = tenant_vector_provider.clone();
        let active_model = std::env::var("APEX_VECTOR_MODEL").unwrap_or("all-minilm-l6-v2".to_string());
        
        tokio::spawn(async move {
            if let Ok(cols) = db_clone.list_collections().await {
                for col in cols {
                    if let Ok(vecs) = db_clone.get_vectors_for_collection(col.id, &active_model).await {
                         for (rid, field, vec) in vecs {
                             let _ = vec_provider_clone.index(col.id, rid, &field, &vec).await;
                         }
                    }
                }
            }
        });

        Ok(TenantContext {
            db: db_arc,
            vector_provider: tenant_vector_provider,
            status, // [OPTIMIZATION] Stored in memory
        })
    }

    pub async fn list_tenants(&self) -> Result<Vec<String>, String> {
        let mut tenants = Vec::new();
        let path = Path::new("storage/tenants");
        
        if path.exists() {
            let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;
            for entry in entries {
                if let Ok(entry) = entry {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_dir() {
                            if let Ok(name) = entry.file_name().into_string() {
                                tenants.push(name);
                            }
                        }
                    }
                }
            }
        }
        Ok(tenants)
    }
}