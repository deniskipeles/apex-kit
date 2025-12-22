use std::sync::Arc;
use moka::future::Cache;
use apexkit_core::{Db, ApexKit, VectorProvider};
use apexkit_core::search::SearchManager;
use apex_vector::{CandleEmbedder, VectorIndex}; 
use std::time::Duration;
use crate::storage::DynamicStorage;
use apexkit_core::cache::CachedDb;
use std::path::Path;
use tracing::info;

// A custom provider that mixes the SHARED embedder (heavy) 
// with an ISOLATED index (light, per tenant)
struct TenantVectorProvider {
    embedder: Option<Arc<CandleEmbedder>>, // Changed to Option
    index: Arc<VectorIndex>,
}

#[async_trait::async_trait]
impl VectorProvider for TenantVectorProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        if let Some(embedder) = &self.embedder {
            let embedder = embedder.clone();
            let t = text.to_string();
            // Use blocking task for heavy compute
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

#[derive(Clone)]
pub struct TenantManager {
    cache: Cache<String, Arc<dyn Db>>,
    shared_embedder: Option<Arc<CandleEmbedder>>,
    vault: Arc<apexkit_core::security::Vault>,
}

impl TenantManager {
    pub fn new(shared_embedder: Option<Arc<CandleEmbedder>>, vault: Arc<apexkit_core::security::Vault>, capacity: u64) -> Self {
        let _ = std::fs::create_dir_all("tenants");
        Self {
            cache: Cache::builder()
                .max_capacity(capacity)
                .time_to_idle(Duration::from_secs(3600))
                .build(),
            shared_embedder,
            vault,
        }
    }

    /// RETRIEVES an existing tenant. Fails if not found on disk.
    pub async fn get_tenant(&self, tenant_id: String) -> Result<Arc<dyn Db>, String> {
        // 1. Try Memory Cache
        if let Some(db) = self.cache.get(&tenant_id).await {
            return Ok(db);
        }

        // 2. Check Disk Existence (Security Check)
        let base_path = format!("tenants/{}", tenant_id);
        if !Path::new(&base_path).exists() {
            return Err(format!("Tenant '{}' does not exist", tenant_id));
        }

        // 3. Load from Disk
        let db = self.load_tenant(&tenant_id).await?;
        self.cache.insert(tenant_id, db.clone()).await;
        Ok(db)
    }

    /// EXPLICITLY CREATES a new tenant.
    pub async fn create_tenant(&self, tenant_id: String) -> Result<Arc<dyn Db>, String> {
        let base_path = format!("tenants/{}", tenant_id);
        
        if Path::new(&base_path).exists() {
            return Err(format!("Tenant '{}' already exists", tenant_id));
        }

        info!("Provisioning new tenant: {}", tenant_id);
        std::fs::create_dir_all(&base_path).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&format!("{}/uploads", base_path)).ok();
        std::fs::create_dir_all(&format!("{}/indexes", base_path)).ok();

        // Initialize Filesystem (This creates .db files and runs migrations via Core)
        // We use a temporary provider just to init the files
        let _ = self.load_tenant(&tenant_id).await?;

        // Return the loaded instance
        self.get_tenant(tenant_id).await
    }

    // Internal helper to hydrate the DB connection logic
    async fn load_tenant(&self, tenant_id: &str) -> Result<Arc<dyn Db>, String> {
        let base_path = format!("tenants/{}", tenant_id);

        // 1. Prepare Tenant Specific Vector Provider
        let vector_index = Arc::new(VectorIndex::new());
        let tenant_vector_provider = Arc::new(TenantVectorProvider {
            embedder: self.shared_embedder.clone(),
            index: vector_index.clone(),
        });

        // 2. Initialize Database (Uses ApexKit Core Factory)
        // This connects to existing files or creates empty ones if create_tenant called it
        let mut apexkit = ApexKit::init_filesystem(&base_path, tenant_vector_provider.clone())
            .await
            .map_err(|e| format!("Failed to init tenant DB: {}", e))?;

        // 3. Initialize Search (Tantivy)
        let search_path = format!("{}/indexes", base_path);
        let search_manager = Arc::new(SearchManager::new(&search_path));
        apexkit.set_search_manager(search_manager);

        // 4. Initialize Storage
        let upload_path = format!("{}/uploads", base_path);
        let public_url = "/api/v1/storage/file/".to_string(); 
        
        let db_arc: Arc<dyn Db> = Arc::new(CachedDb::new(Arc::new(apexkit.clone())));

        // Initialize dynamic storage (doesn't need return, just ensuring it inits if needed internally)
        let _storage = DynamicStorage::new(
             db_arc.clone(),
             self.vault.clone(),
             Some(upload_path),
             public_url
        );
        
        // 5. Hydrate Vector Index (Background)
        let db_clone_for_hydration = db_arc.clone();
        let vec_provider_for_hydration = tenant_vector_provider.clone(); // Use concrete type
        
        tokio::spawn(async move {
            if let Ok(cols) = db_clone_for_hydration.list_collections().await {
                for col in cols {
                    if let Ok(vecs) = db_clone_for_hydration.get_vectors_for_collection(col.id).await {
                         for (rid, field, vec) in vecs {
                             let _ = vec_provider_for_hydration.index(col.id, rid, &field, &vec).await;
                         }
                    }
                }
            }
        });

        Ok(db_arc)
    }

    /// Returns a list of all active Tenant IDs (folder names)
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