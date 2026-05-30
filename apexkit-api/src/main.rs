use axum::{middleware};
use std::sync::Arc;
use apexkit_api::{app_router, AppState, GlobalJobContext, cli::{self, Cli}};
use clap::Parser;
use apexkit_core::{
    a_new_database_connection, jobs::{self}, cache::CachedDb, realtime, 
    storage::{StorageBackend}, 
    security::{MasterKey, Vault},
    ai_models::CreateActionReq,
    script_models::CreateScriptReq,
    Db, VectorProvider, 
    models::ChangesetEvent,
};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use metrics_exporter_prometheus::PrometheusBuilder;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_governor::key_extractor::GlobalKeyExtractor;
use tokio::sync::{broadcast, RwLock};
use std::env;
use dotenvy::dotenv;

use async_graphql::dataloader::DataLoader;
use async_graphql::Value;
use async_graphql::dynamic::{Schema, Object, Field, TypeRef, FieldFuture}; 

use apexkit_core::scripting::ScriptEngine;
use moka::future::Cache;

use apex_vector::{VectorEngine, CandleEmbedder, EmbeddingModelConfig};

use apexkit_api::tenant_manager::TenantManager;
use apexkit_api::sandbox_manager::SandboxManager;
use apexkit_api::replication::{pb::replication_server::ReplicationServer, MasterReplicationService, GrpcWriteForwarder, ensure_replica_env, init_master_replica_tracker};

struct ApexBridge {
    engine: VectorEngine,
}

#[async_trait::async_trait]
impl VectorProvider for ApexBridge {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let engine = self.engine.clone();
        let text = text.to_string();
        tokio::task::spawn_blocking(move || {
            engine.embedder.embed(&text).map_err(|e| e.to_string())
        }).await.map_err(|e| e.to_string())?
    }

    async fn embed_image(&self, base64_image: &str) -> Result<Vec<f32>, String> {
        let engine = self.engine.clone();
        let image = base64_image.to_string();
        tokio::task::spawn_blocking(move || {
            engine.embedder.embed_image(&image).map_err(|e| e.to_string())
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

struct FallbackVectorProvider;

#[async_trait::async_trait]
impl VectorProvider for FallbackVectorProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> {
        Err("Vector Engine failed to initialize. Check server logs.".to_string())
    }
    async fn embed_image(&self, _image: &str) -> Result<Vec<f32>, String> {
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
    dotenv().ok();
    let cli = Cli::parse();

    apexkit_api::logging::rotate_logs_on_startup("logs.db", "logs");
    let log_rx = apexkit_api::logging::init_logging_system();
    tracing::info!("System starting up...");

    let master_key_str = env::var("APEXKIT_MASTER_KEY").unwrap_or_else(|_| {
        let new_key = MasterKey::generate();
        println!("\n=======================================================");
        println!(" [SECURITY WARNING] NO MASTER KEY FOUND");
        println!(" Generated new APEXKIT_MASTER_KEY:");
        println!(" {}\n", new_key);
        println!(" 1. COPY this key immediately.");
        println!(" 2. ADD it to your .env file: APEXKIT_MASTER_KEY={}", new_key);
        println!("");
        println!(" 3. TO BE USED ON SERVER RESTART.");
        println!("=======================================================\n");
        new_key
    });

    let master_key = MasterKey::from_string(master_key_str)
        .expect("Invalid Master Key format (must be 32 bytes base64)");
    
    let vault = Arc::new(Vault::new(&master_key));

    let builder = PrometheusBuilder::new();
    let handle = builder.install_recorder().expect("failed to install Prometheus recorder");

    let master_url = env::var("APEX_MASTER_URL").ok().filter(|s| !s.is_empty());
    let is_replica = master_url.is_some();

    let forwarder: Option<Arc<dyn apexkit_core::batching::WriteForwarder>> = if let Some(url) = &master_url {
        Some(Arc::new(GrpcWriteForwarder::new(url.clone())))
    } else {
        None
    };

    tracing::info!("Initializing Apex Vector Engine...");
    let active_model_name = apexkit_api::get_current_model();
    
    let model_config = match active_model_name.as_str() {
        "bge-small" => EmbeddingModelConfig::bge_small_en_v1_5(),
        "bge-base" => EmbeddingModelConfig::bge_base_en_v1_5(),
        "gte-small" => EmbeddingModelConfig::gte_small(),
        "gemma-300m" => EmbeddingModelConfig::gemma_300m(),
        
        "custom" => EmbeddingModelConfig::custom(
            std::env::var("APEX_VECTOR_CUSTOM_REPO").unwrap_or("sentence-transformers/all-MiniLM-L6-v2".to_string()),
            std::env::var("APEX_VECTOR_CUSTOM_REV").unwrap_or("main".to_string()),
            std::env::var("APEX_VECTOR_CUSTOM_CONFIG").unwrap_or("config.json".to_string()),
            std::env::var("APEX_VECTOR_CUSTOM_TOKENIZER").unwrap_or("tokenizer.json".to_string()),
            std::env::var("APEX_VECTOR_CUSTOM_WEIGHTS").unwrap_or("model.safetensors".to_string()),
            std::env::var("APEX_VECTOR_CUSTOM_WINDOW").ok().and_then(|v| v.parse().ok()).unwrap_or(512),
            std::env::var("APEX_VECTOR_CUSTOM_OVERLAP").ok().and_then(|v| v.parse().ok()).unwrap_or(128),
        ),
        _ => EmbeddingModelConfig::default(),
    };

    tracing::info!("Active Vector Model: {} (Context: {}, Overlap: {})", active_model_name, model_config.window_size, model_config.overlap);

    let (vector_provider, shared_embedder): (Arc<dyn VectorProvider>, Option<Arc<CandleEmbedder>>) = match VectorEngine::new(Some(model_config)).await {
        Ok(engine) => {
            tracing::info!("✅ Apex Vector Engine (Candle + HNSW) ready.");
            let embedder_ref = engine.embedder.clone(); 
            (Arc::new(ApexBridge { engine }), Some(embedder_ref))
        },
        Err(e) => {
            tracing::error!("⚠️  Failed to init Vector Engine: {}. AI features will be disabled.", e);
            (Arc::new(FallbackVectorProvider), None)
        }
    };

    let (sqlite_event_tx, _sqlite_event_rx) = tokio::sync::broadcast::channel::<ChangesetEvent>(1000);

    ensure_replica_env("storage/system").await;
    
    let raw_db = match a_new_database_connection(
        vector_provider.clone(), 
        forwarder.clone(),
        if !is_replica { Some(sqlite_event_tx.clone()) } else { None } 
    ).await {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("Failed to connect to database: {}", e);
            return;
        }
    };

    let cached_db = Arc::new(CachedDb::new(Arc::new(raw_db)));

    let db_for_logs = cached_db.clone();
    tokio::spawn(async move {
        apexkit_api::logging::start_log_worker(log_rx, db_for_logs).await;
    });

    let mut total_vectors_loaded = 0;
    tracing::info!("Reloading HNSW index from vectors database for model '{}'...", active_model_name);
    if let Ok(all_collections) = cached_db.list_collections().await {
        for col in &all_collections {
            if let Ok(vectors_to_load) = cached_db.get_vectors_for_collection(col.id, &active_model_name).await {
                for (rec_id, field_name, vector) in vectors_to_load {
                    vector_provider.index(col.id, rec_id, &field_name, &vector).await.ok();
                    total_vectors_loaded += 1;
                }
            }
        }
    }
    tracing::info!("HNSW index reload complete. {} vectors loaded.", total_vectors_loaded); 

    if let Err(e) = cached_db.recover_indexes().await { 
        tracing::error!("Index Recovery Failed: {}", e);
    }

    if let Err(e) = seed_admin(cached_db.as_ref()).await { tracing::error!("Failed to seed admin: {}", e); }
    if let Err(e) = seed_ai_actions(cached_db.as_ref()).await { tracing::error!("Failed to seed AI actions: {}", e); }
    if let Err(e) = seed_default_scripts(cached_db.as_ref()).await { tracing::error!("Failed to seed default scripts: {}", e); }

    let tenant_manager = Arc::new(TenantManager::new(
        shared_embedder.clone(), 
        cached_db.clone(), 
        500, 
        forwarder.clone(),
        if !is_replica { Some(sqlite_event_tx.clone()) } else { None }
    ));

    let sandbox_manager = Arc::new(SandboxManager::new(
        shared_embedder.clone(), 
        forwarder.clone(),
        if !is_replica { Some(sqlite_event_tx.clone()) } else { None }
    ));

    let job_context = Arc::new(GlobalJobContext {
        root_db: cached_db.clone(),
        root_vector_provider: vector_provider.clone(),
        tenant_manager: tenant_manager.clone(),
        sandbox_manager: sandbox_manager.clone(),
    });

    let job_queue = jobs::start_background_worker(job_context, vault.clone());
    let (tx, _rx) = broadcast::channel::<realtime::DbEvent>(100);

    tracing::info!("Initializing Dynamic Storage Backend...");
    let storage: Arc<dyn StorageBackend> = Arc::new(
        apexkit_api::storage::DynamicStorage::new(
            cached_db.clone(), vault.clone(), None, "/api/v1/storage/file/".to_string() 
        )
    );

    let scheduler = apexkit_api::scheduler::SchedulerService::new().await;
    let scheduler_arc = Arc::new(RwLock::new(scheduler));

    let relation_loader = Arc::new(DataLoader::new(
        apexkit_api::graphql::RelationLoader::new(cached_db.clone()), tokio::spawn
    ));

    let empty_schema = {
        let builder = Schema::build("Query", None, None);
        let query = Object::new("Query").field(Field::new("status", TypeRef::named(TypeRef::STRING), |_| { FieldFuture::new(async { Ok(Some(Value::from("Initializing..."))) }) }));
        builder.register(query).finish().expect("Failed to build empty schema")
    };

    let script_engine = Arc::new(ScriptEngine::new().await);

    let thumb_cache = Cache::builder()
        .max_capacity(1000)
        .time_to_live(std::time::Duration::from_secs(3600)) 
        .build();

    let embedder = Arc::new(apexkit_core::embeddings::EmbedderService::new());

    let env_ttl = std::env::var("ROOT_CACHE_TTL").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let env_root_cache_size = std::env::var("ROOT_CACHE_SIZE").ok().and_then(|s| s.parse().ok()).unwrap_or(1000);

    let root_script_cache = Cache::builder()
        .max_capacity(env_root_cache_size)
        .time_to_live(std::time::Duration::from_secs(env_ttl))
        .build();
    // [NEW] Cache for OSE milestones (5 minutes TTL as requested)
    let record_count_cache = Cache::builder()
        .max_capacity(10_000)
        .time_to_live(std::time::Duration::from_secs(300)) 
        .build();

    let state = AppState {
        db: cached_db.clone(),
        tenant_manager,
        sandbox_manager,
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
        port: cli.port,
        root_script_cache,
        record_count_cache,
    };

    // --- SNAPSHOT REPLICATION APPLIER ---
    if let Ok(master_url) = std::env::var("APEX_MASTER_URL") {
        if !master_url.is_empty() {
            let state_clone = state.clone();
            let mut rx = apexkit_api::replication::get_db_sync_tx().subscribe();
            
            tokio::spawn(async move {
                apexkit_api::replication::start_event_streamer(master_url, Some(state_clone.clone())).await;
            });

            let state_for_apply = state.clone();
            tokio::spawn(async move {
                loop {
                    if let Ok(event) = rx.recv().await {
                        let db_path = apexkit_api::replication::get_db_path_from_scope(&event.scope, &event.db_name);
                        if db_path.is_empty() || db_path.ends_with("logs.db") { continue; }

                        let state_clone = state_for_apply.clone();
                        
                        tokio::task::spawn_blocking(move || {
                            if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                                // We use event.changeset.as_slice() which implements the Read trait
                                let mut input = event.changeset.as_slice();
                                
                                if let Err(e) = conn.apply_strm(
                                    &mut input,
                                    None::<fn(&str) -> bool>, 
                                    |_conflict_type, _item| rusqlite::session::ConflictAction::SQLITE_CHANGESET_REPLACE,
                                ) {
                                    tracing::error!("❌ [Replica] Failed to apply changeset to {}: {}", db_path, e);
                                }
                            }
                        }).await.unwrap();

                        // Instant Cache Invalidation 
                        if event.scope == "root" {
                            let _ = state_clone.db.reload_connections().await;
                        } else if event.scope.starts_with("tenant:") {
                            let tid = event.scope.strip_prefix("tenant:").unwrap();
                            state_clone.tenant_manager.invalidate(tid).await;
                        } else if event.scope.starts_with("sandbox:") {
                            let sid = event.scope.strip_prefix("sandbox:").unwrap();
                            state_clone.sandbox_manager.invalidate(sid).await;
                        }
                    }
                }
            });
        }
    }

    let gql_schema = apexkit_api::graphql::build_schema(
        state.clone(),
        relation_loader
    ).await.expect("Failed to build GraphQL schema");

    {
        let mut lock = state.schema.write().await;
        *lock = gql_schema;
    }

    scheduler_arc.read().await.load_jobs(state.clone()).await;

    if let Some(command) = cli.command {
        match cli::execute_cli_command(state.clone(), command).await {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                tracing::error!("CLI Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(600) 
            .burst_size(1000)
            .key_extractor(GlobalKeyExtractor) 
            .finish()
            .unwrap(),
    );

    let axum_app = app_router(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn_with_state(state.clone(), apexkit_api::dynamic_cors::cors_middleware)) 
        .layer(GovernorLayer::new(governor_conf));

    let addr = format!("0.0.0.0:{}", cli.port); 
    let listener = TcpListener::bind(&addr).await.unwrap();

    tracing::info!("ApexKit listening on {}", listener.local_addr().unwrap());

    if is_replica {
        tracing::info!("Running in REPLICA mode. Forwarding writes to Master via gRPC.");
        if let Err(e) = axum::serve(listener, axum_app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await {
            tracing::error!("Server error: {}", e);
        }
    } else {
        tracing::info!("Running in MASTER mode. Multiplexing HTTP and gRPC.");
        
        init_master_replica_tracker(sqlite_event_tx.clone()).await;

        // 1. Initialize the server and set the limits FIRST
        let master_service = ReplicationServer::new(MasterReplicationService {
            event_tx: sqlite_event_tx.clone(),
            state: state.clone(),
        })
        .max_decoding_message_size(100 * 1024 * 1024)
        .max_encoding_message_size(100 * 1024 * 1024);

        // 2. THEN wrap it in the interceptor
        let grpc_service = tonic::service::interceptor::InterceptedService::new(
            master_service,
            apexkit_api::replication::server_auth_interceptor
        );
        
        let grpc_router = axum::Router::new()
            .route_service("/replication.Replication/ExecuteWrite", grpc_service.clone())
            .route_service("/replication.Replication/FetchDbSnapshot", grpc_service.clone())
            .route_service("/replication.Replication/StreamEvents", grpc_service.clone())
            .route_service("/replication.Replication/SyncFile", grpc_service); // [NEW] Bind SyncFile Route

        let multiplexed_app = grpc_router.merge(axum_app);

        if let Err(e) = axum::serve(listener, multiplexed_app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await {
            tracing::error!("Server error: {}", e);
        }
    }
}

    async fn seed_admin(db: &impl apexkit_core::Db) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let email = "admin@apexkit.io";
    if db.get_user_by_email(email).await?.is_none() {
        let hash = apexkit_core::auth::hash_password("password")?;
        db.create_user(email, &hash, "admin", None).await?;
        tracing::info!("Seeded admin user: {}", email);
    }
    if db.get_config("policy_users").await?.is_none() {
        let default_policy = apexkit_core::schema::CollectionPolicies {
            read: "admin || owner:id".to_string(),
            create: "public".to_string(),
            update: "admin || owner:id".to_string(),
            delete: "admin".to_string(),
        };
        db.set_config("policy_users", &serde_json::to_value(default_policy)?, false).await?;
        tracing::info!("Seeded default user policies");
    }
    Ok(())
    }

    async fn seed_ai_actions(db: &impl apexkit_core::Db) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let actions = vec![
        CreateActionReq {
            name: "Generate Image".to_string(),
            slug: "generate-image".to_string(),
            model: "gemini-2.5-flash-image".to_string(),
            system_prompt: Some("You are a creative image generation assistant.".to_string()),
            template: "{{prompt}}".to_string(),
        },
        CreateActionReq {
            name: "Content Editor".to_string(),
            slug: "content-editor".to_string(),
            model: "gemini-2.5-flash-lite".to_string(),
            system_prompt: Some("You are an expert content editor. Use Google Search to ensure information is accurate. Respond in Markdown.".to_string()),
            template: "User Request: {{prompt}}\n\nOriginal Text:\n{{originalText}}".to_string(),
        },
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

    async fn seed_default_scripts(db: &impl apexkit_core::Db) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let script_name = "apex-auth-roles";

    if db.get_script_by_name(script_name).await?.is_none() {
        tracing::info!("Seeding default script: {}", script_name);
        
        let code = r#"
    export default async function(req) {
    return new Response({ 
        roles: ["user", "admin", "editor"] 
    });
    }
    "#.trim().to_string();

        db.create_script(CreateScriptReq {
            name: script_name.to_string(),
            trigger_type: "manual".to_string(),
            target_collection: None,
            active: true,
            visibility: "private".to_string(),
            code,
        }).await?;
    }
    Ok(())
    }