use std::path::Path;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use apexkit_core::{Db, ApexKit, VectorProvider, query::QueryOptions, schema::FieldType};
use apexkit_core::search::SearchManager;
use apexkit_core::cache::CachedDb;
use apexkit_core::batching::WriteForwarder;
use apexkit_core::models::ChangesetEvent;
use tracing::{info, warn};
use apex_vector::{CandleEmbedder, VectorIndex};
use moka::future::Cache;
use std::collections::HashSet;
use tokio::sync::Mutex;
use serde_json::Value;


// Enum to represent the cloning strategy
#[derive(Debug, Clone)]
pub enum CloneStrategy {
    None,            // Spin up empty
    SchemaOnly,      // Clone collections and schema, no data
    Partial(usize),  // Clone schema and N records per collection
    Full,            // Clone schema and all records
}

// --- 1. Context Container ---
// Holds both the DB connection and the specific Vector Provider for a sandbox.
// This allows jobs to access the in-memory HNSW index.
#[derive(Clone)]
pub struct SandboxContext {
    pub db: Arc<dyn Db>,
    pub vector_provider: Arc<dyn VectorProvider>,
    pub script_cache: Cache<String, String>,
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
            tokio::task::spawn_blocking(move || {
                embedder.embed(&t).map_err(|e| e.to_string())
            }).await.map_err(|e| e.to_string())?
        } else {
            Err("Vector AI is disabled (Embedder not initialized)".to_string())
        }
    }

    async fn embed_image(&self, base64_image: &str) -> Result<Vec<f32>, String> {
        if let Some(embedder) = &self.embedder {
            let embedder = embedder.clone();
            let img = base64_image.to_string();
            tokio::task::spawn_blocking(move || {
                embedder.embed_image(&img).map_err(|e| e.to_string())
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
    forwarder: Option<Arc<dyn WriteForwarder>>, 
    // Add the event transmitter
    event_tx: Option<tokio::sync::broadcast::Sender<ChangesetEvent>>,
}

impl SandboxManager {
    pub fn new(
        shared_embedder: Option<Arc<CandleEmbedder>>, 
        forwarder: Option<Arc<dyn WriteForwarder>>,
        event_tx: Option<tokio::sync::broadcast::Sender<ChangesetEvent>>
    ) -> Self {
        let _ = fs::create_dir_all("storage/sandboxes");
        
        let cache = Cache::builder()
            .max_capacity(100) 
            .time_to_idle(Duration::from_secs(3 * 60)) 
            .eviction_listener(|key, _val, cause| {
                info!("Sandbox '{}' evicted from memory. Reason: {:?}", key, cause);
            })
            .build();

        Self { 
            shared_embedder,
            cache,
            forwarder, 
            event_tx, 
        }
    }

    /// Creates a fresh sandbox by copying the main DB schema/structure.
    /// Immediately loads it into the cache.
    // [UPDATED] create_sandbox now takes a strategy and a source DB
    pub async fn create_sandbox(&self, session_id: &str, strategy: CloneStrategy, source_db: Arc<dyn Db>) -> Result<Arc<dyn Db>, String> {
        let sandbox_dir = format!("storage/sandboxes/session_{}", session_id);
        
        if Path::new(&sandbox_dir).exists() {
            let _ = fs::remove_dir_all(&sandbox_dir);
        }
        fs::create_dir_all(&sandbox_dir).map_err(|e| e.to_string())?;

        // 1. Always start with a fresh, empty set of DB files for the sandbox.
        // We will programmatically copy data instead of filesystem copy.
        let sandbox_db = self.init_empty_sandbox_db(session_id).await?;

        // 2. Based on strategy, spawn a background task to clone data.
        // This makes the UI responsive immediately.
        let session_id_clone = session_id.to_string();
        tokio::spawn(async move {
            info!("Spawning background clone task for sandbox '{}' with strategy: {:?}", session_id_clone, strategy);
            if let Err(e) = Self::clone_data_to_sandbox(source_db, sandbox_db, strategy).await {
                warn!("Sandbox clone task for '{}' failed: {}", session_id_clone, e);
            } else {
                info!("Sandbox clone task for '{}' completed successfully.", session_id_clone);
            }
        });

        // 3. Return the (initially empty) sandbox DB handle immediately.
        self.get_sandbox(session_id).await
    }
    
    // [NEW] Helper to initialize an empty sandbox filesystem and DB connection
    async fn init_empty_sandbox_db(&self, session_id: &str) -> Result<Arc<dyn Db>, String> {
        let base_path = format!("storage/sandboxes/session_{}", session_id);
        fs::create_dir_all(&format!("{}/indexes", base_path)).ok();
        fs::create_dir_all(&format!("{}/uploads", base_path)).ok();
        
        let vector_provider = Arc::new(SandboxVectorProvider {
            embedder: self.shared_embedder.clone(),
            index: Arc::new(VectorIndex::new()),
        });
        
        // Pass the forwarder here
        let apexkit = ApexKit::init_filesystem(
            &base_path, 
            vector_provider, 
            self.forwarder.clone(), // <--- Forward writes to master
            self.event_tx.clone(), // <--- Forward changesets to gRPC 
            format!("sandbox:{}", session_id)
        ).await.map_err(|e| format!("Failed to init sandbox filesystem: {}", e))?;

        Ok(Arc::new(CachedDb::new(Arc::new(apexkit))))
    }

    // [NEW] The core data cloning logic
    // [UPDATED] Core data cloning logic with Dependency Resolution
    async fn clone_data_to_sandbox(
        source_db: Arc<dyn Db>, 
        sandbox_db: Arc<dyn Db>, 
        strategy: CloneStrategy
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        
        match strategy {
            CloneStrategy::None => Ok(()),
            CloneStrategy::SchemaOnly | CloneStrategy::Partial(_) | CloneStrategy::Full => {
                // 1. Clone Schema
                let collections = source_db.list_collections().await?;
                // Map Name -> ID for quick lookup during dependency resolution
                let mut col_name_map = std::collections::HashMap::new();

                for col in &collections {
                    let new_id = sandbox_db.create_collection(&col.name, &col.schema, col.index.clone()).await?;
                    col_name_map.insert(col.name.clone(), new_id);
                }

                if let CloneStrategy::SchemaOnly = strategy {
                    return Ok(());
                }

                // 2. Clone Records
                let record_limit = if let CloneStrategy::Partial(limit) = strategy { Some(limit as u64) } else { None };
                
                // Track copied IDs to prevent duplicates and circular loops
                // Key: "collection_id:record_id" or "user:id"
                let copied_tracker = Arc::new(Mutex::new(HashSet::new()));

                for col in &collections {
                    let opts = QueryOptions {
                        limit: record_limit,
                        per_page: record_limit,
                        ..Default::default()
                    };
                    
                    let result = source_db.list_records(col.id, opts).await?;
                    
                    for record in result.items {
                        // A. Insert the main record
                        let target_col_id = *col_name_map.get(&col.name).unwrap_or(&col.id);
                        
                        let track_key = format!("{}:{}", target_col_id, record.id);
                        let tracker = copied_tracker.lock().await;
                        
                        if !tracker.contains(&track_key) {
                            drop(tracker); 

                            // 1. Resolve Dependencies recursively
                            Self::resolve_dependencies(
                                source_db.clone(), 
                                sandbox_db.clone(), 
                                &record.data, 
                                &col.schema.as_ref().unwrap(),
                                &col_name_map,
                                copied_tracker.clone()
                            ).await?;

                            // 2. Insert record WITH ID PRESERVED
                            // [UPDATED] Use import_record
                            let _ = sandbox_db.import_record(
                                target_col_id, 
                                record.id, // Explicit ID
                                &record.data
                            ).await?;
                            
                            let mut t = copied_tracker.lock().await;
                            t.insert(track_key);
                        }
                    }
                }
                Ok(())
            }
        }
    }

    // [NEW] Helper to scan a record and pull in its dependencies
    async fn resolve_dependencies(
        source_db: Arc<dyn Db>,
        sandbox_db: Arc<dyn Db>,
        data: &Value,
        schema: &apexkit_core::schema::CollectionSchema,
        col_map: &std::collections::HashMap<String, i64>,
        tracker: Arc<Mutex<HashSet<String>>>
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        
        if let Some(obj) = data.as_object() {
            for (key, val) in obj {
                // 1. Check for Relation Fields
                if let Some(field_def) = schema.fields.get(key) {
                    
                    // --- Handle USER (Owner) Dependencies ---
                    if field_def.r#type == FieldType::Owner {
                        if let Some(user_id) = val.as_i64().or(val.as_str().and_then(|s| s.parse().ok())) {
                            let track_key = format!("user:{}", user_id);
                            let t = tracker.lock().await;
                            
                            if !t.contains(&track_key) {
                                drop(t); // Unlock
                                // Fetch User from Source
                                // Note: get_users_by_ids returns Vec
                                let users = source_db.get_users_by_ids(&[user_id]).await?;
                                if let Some(u) = users.first() {
                                    // [UPDATED] Use import_user to preserve ID
                                    let _ = sandbox_db.import_user(
                                        u.id, // Explicit ID
                                        &u.email, 
                                        &u.password_hash, 
                                        &u.role, 
                                        u.metadata.clone()
                                    ).await?;
                                }
                                let mut t = tracker.lock().await;
                                t.insert(track_key);
                            }
                        }
                    }

                    // --- Handle RELATION Dependencies ---
                    if field_def.r#type == FieldType::Relation {
                         if let Some(rel_id) = val.as_i64().or(val.as_str().and_then(|s| s.parse().ok())) {
                             if let Some(target_col_name) = &field_def.relation_to {
                                 // Look up target collection ID in the SANDBOX (using our map)
                                 if let Some(target_col_id) = col_map.get(target_col_name) {
                                     
                                     let track_key = format!("{}:{}", target_col_id, rel_id);
                                     let t = tracker.lock().await;

                                     if !t.contains(&track_key) {
                                         drop(t);
                                         
                                         // 1. Fetch from Source (we need the Source Collection ID)
                                         // We have target_col_name, let's find source ID
                                         let all_source_cols = source_db.list_collections().await?;
                                         if let Some(source_c) = all_source_cols.iter().find(|c| c.name == *target_col_name) {
                                             if let Ok(Some(rel_record)) = source_db.get_record(source_c.id, rel_id, None).await {
                                                 
                                                 // RECURSION: Resolve dependencies of this dependency
                                                 // (Limit depth implicitly by graph structure, or add depth param)
                                                 Box::pin(Self::resolve_dependencies(
                                                     source_db.clone(), 
                                                     sandbox_db.clone(), 
                                                     &rel_record.data, 
                                                     source_c.schema.as_ref().unwrap(), 
                                                     col_map, 
                                                     tracker.clone()
                                                 )).await?;

                                                 // 2. Insert into Sandbox
                                                 let _ = sandbox_db.create_record(*target_col_id, &rel_record.data).await?;
                                                 
                                                 let mut t = tracker.lock().await;
                                                 t.insert(track_key);
                                             }
                                         }
                                     }
                                 }
                             }
                         }
                    }
                }
            }
        }
        Ok(())
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

    // Internal logic to initialize DB connections and Vector Index
    async fn load_context_from_disk(&self, session_id: &str) -> Result<SandboxContext, String> {
        let sandbox_dir = format!("storage/sandboxes/session_{}", session_id);
        
        // [NEW] Check and sync with Master if running as Replica
        crate::replication::ensure_replica_env(&sandbox_dir).await;
        
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

        // B. Connect to SQLite Files (using rusqlite directly)
        let core = rusqlite::Connection::open(&format!("{}/core.db", sandbox_dir)).map_err(|e| e.to_string())?;
        let data = rusqlite::Connection::open(&format!("{}/data.db", sandbox_dir)).map_err(|e| e.to_string())?;
        let log = rusqlite::Connection::open(&format!("{}/logs.db", sandbox_dir)).map_err(|e| e.to_string())?;
        let sys = rusqlite::Connection::open(&format!("{}/system.db", sandbox_dir)).map_err(|e| e.to_string())?;
        let vec = rusqlite::Connection::open(&format!("{}/vectors.db", sandbox_dir)).map_err(|e| e.to_string())?;

        // Pass raw connections to ApexKit::new
        // 1: path, 2: core, 3: data, 4: log, 5: sys, 6: vec, 
        // 7: vector_provider, 8: forwarder, 9: event_tx, 10: scope
        let mut apexkit = ApexKit::new(
            &sandbox_dir,
            core, 
            data, 
            log, 
            sys, 
            vec, 
            vec_provider.clone(),
            self.forwarder.clone(), // <--- Forward writes to master
            self.event_tx.clone(), // <--- Forward changesets to gRPC
            format!("sandbox:{}", session_id) 
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
        let active_model = crate::get_current_model();
        
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

        let cache_size = std::env::var("SCRIPT_CACHE_SIZE")
            .ok().and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(100);

        let script_cache = Cache::builder()
            .max_capacity(cache_size)
            .time_to_live(Duration::from_secs(300))
            .build();

        Ok(SandboxContext {
            db: db_arc,
            vector_provider: vec_provider,
            script_cache,
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
            
            let sandbox_dir = format!("storage/sandboxes/session_{}", session_key);
            if Path::new(&sandbox_dir).exists() {
                if let Err(e) = fs::remove_dir_all(&sandbox_dir) {
                    warn!("Failed to delete sandbox dir {}: {}", session_key, e);
                } else {
                    info!("Sandbox {} deleted from disk", session_key);
                }
            }
        });
    }
    // Check if sandbox is currently loaded in memory
    pub fn is_active(&self, session_id: &str) -> bool {
        self.cache.contains_key(session_id)
    }
    pub async fn get_sandbox_context(&self, session_id: &str) -> Result<SandboxContext, String> {
        if let Some(ctx) = self.cache.get(session_id).await {
            return Ok(ctx);
        }
        let ctx = self.load_context_from_disk(session_id).await?;
        self.cache.insert(session_id.to_string(), ctx.clone()).await;
        Ok(ctx)
    }
    pub async fn invalidate(&self, session_id: &str) {
        self.cache.invalidate(session_id).await;
    }
}