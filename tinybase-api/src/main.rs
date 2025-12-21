// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/main.rs ===========================
use axum::{serve, middleware};
use std::sync::Arc;
use tinybase_api::{app_router, AppState, cli::{self, Cli}};
use clap::Parser;
use tinybase_core::{
    a_new_database_connection, jobs, cache::CachedDb, realtime, 
    storage::{StorageBackend}, 
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

use apex_vector::{VectorEngine, CandleEmbedder};
use tinybase_core::VectorProvider;

use tinybase_api::tenant_manager::TenantManager;

// --- 1. Define Bridge (Real AI) ---
// Bridges the ApexVector engine to the TinyBase Core trait
struct ApexBridge {
    engine: VectorEngine,
}

#[async_trait::async_trait]
impl VectorProvider for ApexBridge {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let engine = self.engine.clone();
        let text = text.to_string();
        // Offload blocking Candle task to a blocking thread to avoid freezing the Async runtime
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
// Used if Candle/Models fail to load (e.g. missing files)
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
    // 1. Load .env file
    dotenv().ok();

    // 2. CLI Parsing (Arguments)
    let cli = Cli::parse();

    // --- LOGGING SETUP (FIXED) ---
    // 1. Rotate logs if needed
    tinybase_api::logging::rotate_logs_on_startup("logs.db", "logs");
    
    // 2. Initialize Tracing System (Called EXACTLY ONCE)
    let log_rx = tinybase_api::logging::init_logging_system();
    
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
        println!(" 3. TO BE USED ON SERVER RESTART.");
        println!("=======================================================\n");
        new_key
    });

    let master_key = MasterKey::from_string(master_key_str)
        .expect("Invalid Master Key format (must be 32 bytes base64)");
    
    let vault = Arc::new(Vault::new(&master_key));
    // --------------------------

    let builder = PrometheusBuilder::new();
    let handle = builder.install_recorder().expect("failed to install Prometheus recorder");

    // --- 4. Initialize AI Engine (Split Result) ---
    tracing::info!("Initializing Apex Vector Engine...");
    
    // We split this so we can pass the specific `CandleEmbedder` struct to TenantManager
    // but pass the generic `dyn VectorProvider` trait to the AppState/DB.
    let (vector_provider, shared_embedder): (Arc<dyn VectorProvider>, Option<Arc<CandleEmbedder>>) = match VectorEngine::new().await {
        Ok(engine) => {
            tracing::info!("✅ Apex Vector Engine (Candle + HNSW) ready.");
            let embedder_ref = engine.embedder.clone(); // Keep a ref to the concrete struct
            (Arc::new(ApexBridge { engine }), Some(embedder_ref))
        },
        Err(e) => {
            tracing::error!("⚠️  Failed to init Vector Engine: {}. AI features will be disabled.", e);
            (Arc::new(FallbackVectorProvider), None)
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

    // 3. START LOG WORKER
    // Now that DB is ready, start draining the log buffer into SQLite
    let db_for_logs = cached_db.clone();
    tokio::spawn(async move {
        tinybase_api::logging::start_log_worker(log_rx, db_for_logs).await;
    });

    // --- 5. HNSW RELOAD ---
    let mut total_vectors_loaded = 0;
    tracing::info!("Reloading HNSW index from vectors database...");
    if let Ok(all_collections) = cached_db.list_collections().await {
        for col in &all_collections {
            if let Ok(vectors_to_load) = cached_db.get_vectors_for_collection(col.id).await {
                for (rec_id, field_name, vector) in vectors_to_load {
                    vector_provider.index(col.id, rec_id, &field_name, &vector).await.ok();
                    total_vectors_loaded += 1;
                }
            }
        }
    }
    tracing::info!("HNSW index reload complete. {} vectors loaded.", total_vectors_loaded); 

    // --- 6. SEEDING DEFAULTS ---
    if let Err(e) = seed_admin(cached_db.as_ref()).await {
            tracing::error!("Failed to seed admin: {}", e);
    }
    
    // Seed AI Actions
    if let Err(e) = seed_ai_actions(cached_db.as_ref()).await {
        tracing::error!("Failed to seed AI actions: {}", e);
    }

    let job_queue = jobs::start_background_worker(cached_db.clone(), vector_provider.clone(), vault.clone());
    let (tx, _rx) = broadcast::channel::<realtime::DbEvent>(100);

    // --- 7. STORAGE BACKEND (FIXED ARGUMENTS) ---
    tracing::info!("Initializing Dynamic Storage Backend...");
    let storage: Arc<dyn StorageBackend> = Arc::new(
        tinybase_api::storage::DynamicStorage::new(
            cached_db.clone(), 
            vault.clone(), 
            None, // No FS override for root
            "/api/v1/storage/file/".to_string() // Default public URL prefix
        )
    );

    // --- INIT TENANT MANAGER ---
    // Capacity 500 active databases. Unused ones dropped after 1hr.
    let tenant_manager = Arc::new(TenantManager::new(
        shared_embedder, // This is Option<Arc<CandleEmbedder>>
        vault.clone(), 
        500
    ));

    // --- 8. SCHEDULER & STATE ---
    // Init Scheduler
    let scheduler = tinybase_api::scheduler::SchedulerService::new().await;
    let scheduler_arc = Arc::new(RwLock::new(scheduler));

    // Init Loader (Needed for Schema Build)
    let relation_loader = Arc::new(DataLoader::new(
        tinybase_api::graphql::RelationLoader::new(cached_db.clone()), 
        tokio::spawn
    ));

    // Create Initial Empty Schema (To satisfy AppState requirements before full build)
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
    
    // Initialize EmbedderService (Wrapper for external APIs like OpenAI/Gemini if used in scripts)
    let embedder = Arc::new(tinybase_core::embeddings::EmbedderService::new());

    // 9. Construct AppState
    let state = AppState {
        db: cached_db.clone(),
        tenant_manager,
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

    // 10. Build Real Schema
    let gql_schema = tinybase_api::graphql::build_schema(
        state.clone(),
        relation_loader
    ).await.expect("Failed to build GraphQL schema");

    // Update State with Real Schema
    {
        let mut lock = state.schema.write().await;
        *lock = gql_schema;
    }

    // Load Cron Jobs from DB
    scheduler_arc.read().await.load_jobs(state.clone()).await;

    // --- 11. EXECUTE CLI COMMANDS ---
    // We check for CLI commands *after* State is built so the CLI has access
    // to the DB, Vault, and Script Engine.
    if let Some(command) = cli.command {
        match cli::execute_cli_command(state.clone(), command).await {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                tracing::error!("CLI Error: {}", e);
                std::process::exit(1);
            }
        }
    }
    // --- END CLI EXECUTION ---

    // --- 12. SERVER START ---
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(600) // Increased for dashboard usage
            .burst_size(1000)
            .finish()
            .unwrap(),
    );

    let app = app_router(state.clone())
        // Middleware Stack
        .layer(TraceLayer::new_for_http())
        // Dynamic CORS from DB
        .layer(middleware::from_fn_with_state(state.clone(), tinybase_api::dynamic_cors::cors_middleware)) 
        // Rate Limiting
        .layer(GovernorLayer::new(governor_conf));

    let addr = format!("0.0.0.0:{}", cli.port); 

    let listener = match TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("Failed to bind to port {}: {}", cli.port, e);
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