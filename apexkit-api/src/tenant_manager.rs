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
}

// --- 3. Tenant Manager ---
#[derive(Clone)]
pub struct TenantManager {
    cache: Cache<String, TenantContext>,
    shared_embedder: Option<Arc<CandleEmbedder>>,
}

impl TenantManager {
    pub fn new(shared_embedder: Option<Arc<CandleEmbedder>>, capacity: u64) -> Self {
        let _ = std::fs::create_dir_all("tenants");
        Self {
            cache: Cache::builder()
                .max_capacity(capacity)
                .time_to_idle(Duration::from_secs(3600)) // Evict inactive tenants after 1 hour
                .build(),
            shared_embedder,
        }
    }

    /// RETRIEVES an existing tenant's DB. Fails if not found on disk.
    pub async fn get_tenant(&self, tenant_id: String) -> Result<Arc<dyn Db>, String> {
        let ctx = self.get_tenant_context(&tenant_id).await?;
        Ok(ctx.db)
    }

    /// RETRIEVES the specific Vector Provider for a tenant.
    pub async fn get_vector_provider(&self, tenant_id: &str) -> Option<Arc<dyn VectorProvider>> {
        self.get_tenant_context(tenant_id).await.ok().map(|c| c.vector_provider)
    }

    /// Internal helper to fetch or load context
    pub async fn get_tenant_context(&self, tenant_id: &str) -> Result<TenantContext, String> {
        // 1. Try Memory Cache
        if let Some(ctx) = self.cache.get(tenant_id).await {
            return Ok(ctx);
        }

        // 2. Check Disk Existence (Security Check)
        let base_path = format!("tenants/{}", tenant_id);
        if !Path::new(&base_path).exists() {
            return Err(format!("Tenant '{}' does not exist", tenant_id));
        }

        // 3. Load from Disk
        let ctx = self.load_tenant(tenant_id).await?;
        self.cache.insert(tenant_id.to_string(), ctx.clone()).await;
        Ok(ctx)
    }

    /// EXPLICITLY CREATES a new tenant.
    pub async fn create_tenant(&self, tenant_id: String) -> Result<Arc<dyn Db>, String> {
        let base_path = format!("tenants/{}", tenant_id);
        
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

    // Internal helper to hydrate the DB connection logic
    async fn load_tenant(&self, tenant_id: &str) -> Result<TenantContext, String> {
        let base_path = format!("tenants/{}", tenant_id);

        // 1. Prepare Tenant Specific Vector Provider (Isolated HNSW Index)
        let vector_index = Arc::new(VectorIndex::new());
        let tenant_vector_provider = Arc::new(TenantVectorProvider {
            embedder: self.shared_embedder.clone(),
            index: vector_index.clone(),
        });

        // 2. Initialize Database (Uses ApexKit Core Factory)
        let mut apexkit = ApexKit::init_filesystem(&base_path, tenant_vector_provider.clone())
            .await
            .map_err(|e| format!("Failed to init tenant DB: {}", e))?;

        // 3. Initialize Search (Tantivy)
        let search_path = format!("{}/indexes", base_path);
        let search_manager = Arc::new(SearchManager::new(&search_path));
        apexkit.set_search_manager(search_manager);

        // 4. Wrap in CachedDB
        let db_arc: Arc<dyn Db> = Arc::new(CachedDb::new(Arc::new(apexkit.clone())));
        
        // 5. Hydrate Indexes (Background Task)
        let db_clone = db_arc.clone();
        let vec_provider_clone = tenant_vector_provider.clone();
        let active_model = std::env::var("APEX_VECTOR_MODEL").unwrap_or("all-minilm-l6-v2".to_string());
        
        // Use owned string for static async block
        let tid = tenant_id.to_string();

        tokio::spawn(async move {
            // A. Hydrate Vector Index (HNSW - Memory Only) from SQLite
            if let Ok(cols) = db_clone.list_collections().await {
                for col in cols {
                    if let Ok(vecs) = db_clone.get_vectors_for_collection(col.id, &active_model).await {
                         for (rid, field, vec) in vecs {
                             let _ = vec_provider_clone.index(col.id, rid, &field, &vec).await;
                         }
                    }
                }
            }
            
            // B. Recover Tantivy Index (Consistency Check)
            // This handles cases where the server crashed and the search index (on disk) 
            // became out of sync with the SQLite DB.
            info!("Tenant '{}': Checking search index consistency...", tid);
            if let Err(e) = db_clone.recover_indexes().await {
                 error!("Tenant '{}' index recovery failed: {}", tid, e);
            }
        });

        Ok(TenantContext {
            db: db_arc,
            vector_provider: tenant_vector_provider
        })
    }

    pub async fn list_tenants(&self) -> Result<Vec<String>, String> {
        let mut tenants = Vec::new();
        let path = Path::new("tenants");
        
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