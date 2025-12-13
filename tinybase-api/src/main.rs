// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/main.rs ===========================
use axum::{serve, middleware, routing::get};
use std::sync::Arc;
use tinybase_api::{app_router, AppState};
use tinybase_core::{
    a_new_database_connection, jobs, cache::CachedDb, realtime, 
    storage::{LocalStorage, S3Storage, StorageBackend}, 
    security::{MasterKey, Vault},
    ai_models::CreateActionReq,
    Db,
};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use metrics_exporter_prometheus::PrometheusBuilder;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tokio::sync::{broadcast, RwLock};
use std::env;
use dotenvy::dotenv;

use async_graphql::dataloader::DataLoader;
use async_graphql::Value;
use async_graphql::dynamic::{Schema, Object, Field, TypeRef, FieldFuture}; 

use tinybase_core::scripting::ScriptEngine;
use moka::future::Cache;

// Import ApexVector types
use apex_vector::VectorEngine;
use tinybase_core::VectorProvider;

// --- 1. Define Bridge (Real AI) ---
struct ApexBridge {
    engine: VectorEngine,
}

#[async_trait::async_trait]
impl VectorProvider for ApexBridge {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let engine = self.engine.clone();
        let text = text.to_string();
        // Offload blocking Candle task
        tokio::task::spawn_blocking(move || {
            engine.embedder.embed(&text).map_err(|e| e.to_string())
        }).await.map_err(|e| e.to_string())?
    }

    async fn search(&self, col_id: i64, field: &str, vec: &[f32], limit: usize) -> Result<Vec<(i64, f32)>, String> {
        Ok(self.engine.index.search(col_id, field, vec, limit))
    }

    async fn index(&self, col_id: i64, rec_id: i64, field: &str, vec: &[f32]) -> Result<(), String> {
        self.engine.index.insert(col_id, rec_id, field, vec);
        Ok(())
    }
}

// --- 2. Define Fallback (No AI) ---
struct FallbackVectorProvider;

#[async_trait::async_trait]
impl VectorProvider for FallbackVectorProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> {
        Err("Vector Engine failed to initialize. Check server logs.".to_string())
    }
    async fn search(&self, _c: i64, _f: &str, _v: &[f32], _l: usize) -> Result<Vec<(i64, f32)>, String> {
        Ok(vec![])
    }
    async fn index(&self, _c: i64, _r: i64, _f: &str, _v: &[f32]) -> Result<(), String> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    // Load .env file
    dotenv().ok();

    // --- LOGGING INIT ---
    // Initialize file-based logging (logs/ folder) + console output
    // Returns a guard that must be kept alive for the duration of the process
    let _guard = tinybase_api::logging::init_logging("logs", 7);

    tracing::info!("System starting up...");

    // --- SECURITY BOOTSTRAP ---
    let master_key_str = env::var("TINYBASE_MASTER_KEY").unwrap_or_else(|_| {
        let new_key = MasterKey::generate();
        println!("\n=======================================================");
        println!(" [SECURITY WARNING] NO MASTER KEY FOUND");
        println!(" Generated new TINYBASE_MASTER_KEY:");
        println!(" {}\n", new_key);
        println!(" 1. COPY this key immediately.");
        println!(" 2. ADD it to your .env file: TINYBASE_MASTER_KEY={}", new_key);
        println!("");
        println!(" 3. RESTART the server.");
        println!("=======================================================\n");
        new_key
    });

    let master_key = MasterKey::from_string(master_key_str)
        .expect("Invalid Master Key format (must be 32 bytes base64)");
    
    let vault = Arc::new(Vault::new(&master_key));
    // --------------------------

    let builder = PrometheusBuilder::new();
    let handle = builder.install_recorder().expect("failed to install Prometheus recorder");

    // --- 3. Initialize AI Engine (With Graceful Fallback) ---
    tracing::info!("Initializing Apex Vector Engine...");
    
    let vector_provider: Arc<dyn VectorProvider> = match VectorEngine::new().await {
        Ok(engine) => {
            tracing::info!("✅ Apex Vector Engine (Candle + HNSW) ready.");
            Arc::new(ApexBridge { engine })
        },
        Err(e) => {
            // Log the error but DO NOT CRASH
            tracing::error!("⚠️  Failed to init Vector Engine: {}. AI features will be disabled.", e);
            Arc::new(FallbackVectorProvider)
        }
    };

    // Pass the provider (Real or Fallback) to the DB
    let raw_db = match a_new_database_connection(vector_provider.clone()).await {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("Failed to connect to database: {}", e);
            return;
        }
    };

    let cached_db = Arc::new(CachedDb::new(Arc::new(raw_db)));

    // --- STARTUP ROUTINE TO RELOAD HNSW INDEX ---
    // This logic should be placed after the vector_provider and cached_db are initialized.
    let mut total_vectors_loaded = 0;

    tracing::info!("Reloading HNSW index from vectors database...");

    // FIX: Db trait is now imported, so these methods are visible.
    let all_collections = cached_db.list_collections().await
        .expect("Failed to list collections for HNSW reload");

    for col in &all_collections {
        let col_id = col.id;
        
        // Get all stored vectors for this collection
        let vectors_to_load = cached_db.get_vectors_for_collection(col_id).await
            .expect(&format!("Failed to get vectors for collection {}", col_id));
            
        // FIX: The types in the tuple are String and Vec<f32>, which are Sized.
        // The compiler just needed the trait in scope to correctly bind this method.
        for (rec_id, field_name, vector) in vectors_to_load {
            // Index each one into the in-memory HNSW index
            let field_name_ref: &str = &field_name;
            let vector_ref: &[f32] = &vector;

            vector_provider.index(col_id, rec_id, field_name_ref, vector_ref).await
                .unwrap_or_else(|e| tracing::error!("HNSW Reload Error: {}", e));
            total_vectors_loaded += 1;
        }
    }

    tracing::info!("HNSW index reload complete. {} vectors loaded.", total_vectors_loaded); 

    // --- SEEDING DEFAULTS ---
    if let Err(e) = seed_admin(cached_db.as_ref()).await {
            tracing::error!("Failed to seed admin: {}", e);
    }
    
    // Seed AI Actions
    if let Err(e) = seed_ai_actions(cached_db.as_ref()).await {
        tracing::error!("Failed to seed AI actions: {}", e);
    }

    let job_queue = jobs::start_background_worker(cached_db.clone(), vector_provider.clone());
    let (tx, _rx) = broadcast::channel::<realtime::DbEvent>(100);

    // --- STORAGE BACKEND (UPDATED) ---
    // Instead of reading ENV, we wrap the DB and Vault in the DynamicStorage proxy.
    // This allows the backend to switch between Local and S3 based on the 'settings' table.
    
    tracing::info!("Initializing Dynamic Storage Backend (DB-backed configuration)");
    let storage: Arc<dyn StorageBackend> = Arc::new(
        tinybase_api::storage::DynamicStorage::new(cached_db.clone(), vault.clone())
    );

    // --- SCHEDULER & STATE ---
    
    // 1. Init Scheduler
    let scheduler = tinybase_api::scheduler::SchedulerService::new().await;
    let scheduler_arc = Arc::new(RwLock::new(scheduler));

    // 2. Init Loader (Needed for Schema Build)
    let relation_loader = Arc::new(DataLoader::new(
        tinybase_api::graphql::RelationLoader::new(cached_db.clone()), 
        tokio::spawn
    ));

    // 3. Create Initial Empty Schema (To satisfy AppState requirements before full build)
    let empty_schema = {
        let builder = Schema::build("Query", None, None);
        let query = Object::new("Query")
            .field(Field::new("status", TypeRef::named(TypeRef::STRING), |_| {
                FieldFuture::new(async { Ok(Some(Value::from("Initializing..."))) })
            }));
        builder.register(query).finish().expect("Failed to build empty schema")
    };

    // Init Script Engine
    let script_engine = Arc::new(ScriptEngine::new().await);
    // Configure Cache: Max 1000 images or 500MB (approx), TTL 1 hour (matches HTTP header)
    let thumb_cache = Cache::builder()
        .max_capacity(1000)
        .time_to_live(std::time::Duration::from_secs(3600)) 
        .build();
    // Initialize EmbedderService
    let embedder = Arc::new(tinybase_core::embeddings::EmbedderService::new());
    // 4. Construct AppState
    let state = AppState {
        db: cached_db.clone(),
        queue: job_queue,
        metrics: Some(handle),
        tx,
        storage,
        vault,
        schema: Arc::new(RwLock::new(empty_schema)),
        scheduler: scheduler_arc.clone(),
        script_engine,
        css_cache: Arc::new(RwLock::new(String::new())),
        thumb_cache,
        embedder,
        vector_provider: vector_provider.clone(),
    };

    // 5. Build Real Schema
    let gql_schema = tinybase_api::graphql::build_schema(
        state.clone(),
        relation_loader
    ).await.expect("Failed to build GraphQL schema");

    // 6. Update State with Real Schema
    {
        let mut lock = state.schema.write().await;
        *lock = gql_schema;
    }

    // 7. Load Cron Jobs from DB
    scheduler_arc.read().await.load_jobs(state.clone()).await;

    // --- MIDDLEWARE CONFIG ---
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(600) // Increased for dashboard usage
            .burst_size(1000)
            .finish()
            .unwrap(),
    );

    // --- ROUTER SETUP ---
    let app = app_router(state.clone())
        // Middleware Stack
        .layer(TraceLayer::new_for_http())
        // Dynamic CORS from DB
        .layer(middleware::from_fn_with_state(state.clone(), tinybase_api::dynamic_cors::cors_middleware)) 
        // Rate Limiting
        .layer(GovernorLayer::new(governor_conf));

    // --- SERVER START ---
    const PORT: u16 = 5000;
    let addr = format!("0.0.0.0:{}", PORT);

    let listener = match TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("Failed to bind to port {}: {}", PORT, e);
            return;
        }
    };
    
    tracing::info!("Tinybase listening on {}", listener.local_addr().unwrap());
    
    if let Err(e) = serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await {
        tracing::error!("Server error: {}", e);
    }
}

async fn seed_admin(db: &impl tinybase_core::Db) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let email = "admin@tinybase.io";
    if db.get_user_by_email(email).await?.is_none() {
        let hash = tinybase_core::auth::hash_password("password")?;
        db.create_user(email, &hash, "admin").await?;
        tracing::info!("Seeded admin user: {}", email);
    }
    Ok(())
}

// --- NEW: SEED AI ACTIONS ---
async fn seed_ai_actions(db: &impl tinybase_core::Db) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let actions = vec![
        // 1. Generate Image (gemini-2.5-flash-image)
        CreateActionReq {
            name: "Generate Image".to_string(),
            slug: "generate-image".to_string(),
            model: "gemini-2.5-flash-image".to_string(),
            system_prompt: Some("You are a creative image generation assistant.".to_string()),
            template: "{{prompt}}".to_string(),
        },
        // 2. Content Editor (gemini-2.5-flash-lite)
        CreateActionReq {
            name: "Content Editor".to_string(),
            slug: "content-editor".to_string(),
            model: "gemini-2.5-flash-lite".to_string(),
            system_prompt: Some("You are an expert content editor. Use Google Search to ensure information is accurate. Respond in Markdown.".to_string()),
            template: "User Request: {{prompt}}\n\nOriginal Text:\n{{originalText}}".to_string(),
        },
        // 3. Edit Image (gemini-2.5-flash-image) - Required for Magic Wand in Editor
        CreateActionReq {
            name: "Edit Image".to_string(),
            slug: "edit-image".to_string(),
            model: "gemini-2.5-flash-image".to_string(),
            system_prompt: Some("You are an expert image editor.".to_string()),
            template: "Edit the attached image based on this instruction: {{prompt}}".to_string(),
        }
    ];

    for action in actions {
        if db.get_ai_action(&action.slug).await?.is_none() {
            tracing::info!("Seeding AI Action: {}", action.name);
            db.create_ai_action(action).await?;
        }
    }
    Ok(())
}