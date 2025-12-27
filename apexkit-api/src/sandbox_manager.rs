use std::path::Path;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use apexkit_core::{Db, ApexKit, VectorProvider};
use apexkit_core::search::SearchManager;
use apexkit_core::cache::CachedDb;
use libsql::Builder;
use tracing::{info, warn};
use apex_vector::{CandleEmbedder, VectorIndex};
use moka::future::Cache;

// --- 1. Context Container ---
// Holds both the DB connection and the specific Vector Provider for a sandbox.
// This allows jobs to access the in-memory HNSW index.
#[derive(Clone)]
pub struct SandboxContext {
    pub db: Arc<dyn Db>,
    pub vector_provider: Arc<dyn VectorProvider>,
}

// --- 2. Sandbox Vector Provider ---
// Implements the Vector trait using the shared Embedder (heavy) 
// but an isolated VectorIndex (light, per-sandbox).
pub struct SandboxVectorProvider {
    pub embedder: Option<Arc<CandleEmbedder>>,
    pub index: Arc<VectorIndex>,
}

#[async_trait::async_trait]
impl VectorProvider for SandboxVectorProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        if let Some(embedder) = &self.embedder {
            let embedder = embedder.clone();
            let t = text.to_string();
            // Offload heavy compute to blocking thread
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

// --- 3. Manager ---
#[derive(Clone)]
pub struct SandboxManager {
    // Shared heavy model (Read-only)
    pub shared_embedder: Option<Arc<CandleEmbedder>>,
    // Cache stores active contexts. Key: session_id
    cache: Cache<String, SandboxContext>,
}

impl SandboxManager {
    pub fn new(shared_embedder: Option<Arc<CandleEmbedder>>) -> Self {
        let _ = fs::create_dir_all("sandboxes");
        
        // STRICT CACHE POLICY:
        // Evict sandbox from memory 3 minutes after the last access.
        // This drops the HNSW index and closes DB connections.
        let cache = Cache::builder()
            .max_capacity(100) // Limit total active sandboxes
            .time_to_idle(Duration::from_secs(3 * 60)) 
            .eviction_listener(|key, _val, cause| {
                info!("Sandbox '{}' evicted from memory. Reason: {:?}", key, cause);
            })
            .build();

        Self { 
            shared_embedder,
            cache
        }
    }

    /// Creates a fresh sandbox by copying the main DB schema/structure.
    /// Immediately loads it into the cache.
    pub async fn create_sandbox(&self, session_id: &str) -> Result<Arc<dyn Db>, String> {
        let dbs = vec!["core", "data", "logs", "system", "vectors"];
        let sandbox_dir = format!("sandboxes/session_{}", session_id);
        
        // Ensure clean slate
        if Path::new(&sandbox_dir).exists() {
            let _ = fs::remove_dir_all(&sandbox_dir);
        }
        fs::create_dir_all(&sandbox_dir).map_err(|e| e.to_string())?;
        fs::create_dir_all(&format!("{}/indexes", sandbox_dir)).ok();
        fs::create_dir_all(&format!("{}/uploads", sandbox_dir)).ok();

        // Copy template DBs from root if they exist
        for db_name in &dbs {
            let prod_path = format!("{}.db", db_name);
            let target_path = format!("{}/{}.db", sandbox_dir, db_name);

            if Path::new(&prod_path).exists() {
                fs::copy(&prod_path, &target_path).map_err(|e| format!("Failed to clone {}: {}", db_name, e))?;
                // Try copying WAL files for consistency, ignore errors if missing
                let _ = fs::copy(format!("{}-wal", prod_path), format!("{}-wal", target_path));
                let _ = fs::copy(format!("{}-shm", prod_path), format!("{}-shm", target_path));
            }
        }

        info!("Sandbox created at {}", sandbox_dir);

        // Force load into cache and return DB
        self.get_sandbox(session_id).await
    }

    /// Connects to an existing sandbox.
    /// 1. Checks Cache (Fast)
    /// 2. Loads from Disk + Hydrates Vectors (Slow)
    pub async fn get_sandbox(&self, session_id: &str) -> Result<Arc<dyn Db>, String> {
        // 1. Fast Path: Return cached DB
        if let Some(ctx) = self.cache.get(session_id).await {
            return Ok(ctx.db);
        }

        // 2. Slow Path: Load Context
        let ctx = self.load_context_from_disk(session_id).await?;
        
        // 3. Cache it
        self.cache.insert(session_id.to_string(), ctx.clone()).await;

        Ok(ctx.db)
    }

    /// Helper for Job Worker to get the VectorProvider.
    /// This ensures background jobs use the SAME in-memory index as the API.
    pub async fn get_vector_provider(&self, session_id: &str) -> Option<Arc<dyn VectorProvider>> {
        // Try cache first
        if let Some(ctx) = self.cache.get(session_id).await {
            return Some(ctx.vector_provider);
        }

        // If not in cache, load it (this restarts the 3m timer)
        match self.load_context_from_disk(session_id).await {
            Ok(ctx) => {
                self.cache.insert(session_id.to_string(), ctx.clone()).await;
                Some(ctx.vector_provider)
            },
            Err(e) => {
                warn!("Failed to load sandbox provider for {}: {}", session_id, e);
                None
            }
        }
    }

    /// Internal logic to initialize DB connections and Vector Index
    // Internal logic to initialize DB connections and Vector Index
    async fn load_context_from_disk(&self, session_id: &str) -> Result<SandboxContext, String> {
        let sandbox_dir = format!("sandboxes/session_{}", session_id);
        if !Path::new(&sandbox_dir).exists() {
            return Err("Sandbox expired or not found".into());
        }

        info!("Loading Sandbox '{}' into memory...", session_id);

        // A. Setup Isolated Vector Components
        let vector_index = Arc::new(VectorIndex::new());
        let vec_provider = Arc::new(SandboxVectorProvider {
            embedder: self.shared_embedder.clone(),
            index: vector_index.clone(),
        });

        // B. Connect to SQLite Files
        let core = Builder::new_local(&format!("{}/core.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let data = Builder::new_local(&format!("{}/data.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let log = Builder::new_local(&format!("{}/logs.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let sys = Builder::new_local(&format!("{}/system.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;
        let vec = Builder::new_local(&format!("{}/vectors.db", sandbox_dir)).build().await.map_err(|e| e.to_string())?;

        let mut apexkit = ApexKit::new(
            Arc::new(core), 
            Arc::new(data), 
            Arc::new(log), 
            Arc::new(sys), 
            Arc::new(vec),
            vec_provider.clone() 
        );

        // C. Setup Tantivy Search
        let search_path = format!("{}/indexes", sandbox_dir);
        let search_manager = Arc::new(SearchManager::new(&search_path));
        apexkit.set_search_manager(search_manager);

        // D. Wrap in CachedDb
        let db_arc: Arc<dyn Db> = Arc::new(CachedDb::new(Arc::new(apexkit)));

        // E. Hydrate Vector Index from Disk
        let db_clone = db_arc.clone();
        let vec_prov_clone = vec_provider.clone();
        let active_model = std::env::var("APEX_VECTOR_MODEL").unwrap_or("all-minilm-l6-v2".to_string());
        
        // FIX: Create an owned String to move into the 'static background task
        let session_id_str = session_id.to_string();

        tokio::spawn(async move {
            if let Ok(cols) = db_clone.list_collections().await {
                for col in cols {
                     if let Ok(vecs) = db_clone.get_vectors_for_collection(col.id, &active_model).await {
                         let count = vecs.len();
                         for (rid, field, v) in vecs {
                             let _ = vec_prov_clone.index(col.id, rid, &field, &v).await;
                         }
                         if count > 0 {
                             info!("Sandbox {}: Hydrated {} vectors for col {}", session_id_str, count, col.id);
                         }
                     }
                }
            }
        });

        Ok(SandboxContext {
            db: db_arc,
            vector_provider: vec_provider,
        })
    }

    /// Deletes the sandbox files AND invalidates cache
    pub fn cleanup_sandbox(&self, session_id: &str) {
        // Remove from cache immediately
        // Note: moka's invalidate is sync or async depending on usage, explicit async here
        let session_key = session_id.to_string();
        let cache = self.cache.clone();
        
        // Fire and forget cleanup
        tokio::spawn(async move {
            cache.invalidate(&session_key).await;
            
            let sandbox_dir = format!("sandboxes/session_{}", session_key);
            if Path::new(&sandbox_dir).exists() {
                if let Err(e) = fs::remove_dir_all(&sandbox_dir) {
                    warn!("Failed to delete sandbox dir {}: {}", session_key, e);
                } else {
                    info!("Sandbox {} deleted from disk", session_key);
                }
            }
        });
    }
}