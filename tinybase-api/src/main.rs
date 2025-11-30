// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/main.rs ===========================
use axum::{serve, middleware, routing::get};
use std::sync::Arc;
use tinybase_api::{app_router, AppState};
use tinybase_core::{
    a_new_database_connection, jobs, cache::CachedDb, realtime, 
    storage::{LocalStorage, S3Storage, StorageBackend}, 
    security::{MasterKey, Vault}
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

    // --- DATABASE INIT ---
    let raw_db = match a_new_database_connection().await {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("Failed to connect to database: {}", e);
            return;
        }
    };

    let cached_db = Arc::new(CachedDb::new(Arc::new(raw_db)));

    if let Err(e) = seed_admin(cached_db.as_ref()).await {
         tracing::error!("Failed to seed admin: {}", e);
    }

    let job_queue = jobs::start_background_worker();
    let (tx, _rx) = broadcast::channel::<realtime::DbEvent>(100);

    // --- STORAGE BACKEND ---
    let storage_type = env::var("STORAGE_TYPE").unwrap_or_else(|_| "local".to_string());
    let storage: Arc<dyn StorageBackend> = if storage_type == "s3" {
        let bucket = env::var("S3_BUCKET").expect("S3_BUCKET must be set");
        let region = env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let public_url = env::var("S3_PUBLIC_URL").expect("S3_PUBLIC_URL must be set");
        
        tracing::info!("Using S3 Storage (Bucket: {})", bucket);
        Arc::new(S3Storage::new(&bucket, &region, &public_url).await)
    } else {
        let storage_path = "./uploads";
        tracing::info!("Using Local Storage (Path: {})", storage_path);
        Arc::new(LocalStorage::new(storage_path, "/api/v1/storage/file/").await)
    };

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
        // 1. Serve React Dashboard (Embed) - Axum 0.8 Wildcard Syntax
        .route("/_dashboard", get(tinybase_api::assets::dashboard_handler))
        .route("/_dashboard/{*path}", get(tinybase_api::assets::dashboard_handler))
        
        // 2. Serve Landing Page (Embed)
        .route("/", get(tinybase_api::assets::index_handler))
        
        // 3. Middleware Stack
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
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/main.rs ends here ===========================