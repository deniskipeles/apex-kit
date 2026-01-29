use std::sync::Arc;
use std::net::SocketAddr;
use tokio::sync::{broadcast, RwLock};
use axum::{Router, extract::ConnectInfo};
use apexkit_api::{app_router, AppState, GlobalJobContext};
use apexkit_api::tenant_manager::TenantManager;
use apexkit_api::sandbox_manager::SandboxManager;
use apexkit_core::{
    jobs::{self}, cache::CachedDb,
    storage::{StorageBackend, LocalStorage},
    security::{MasterKey, Vault},
    scripting::ScriptEngine,
    embeddings::EmbedderService,
    VectorProvider,
};
use moka::future::Cache;
use async_graphql::dynamic::{Schema, Object, Field, TypeRef, FieldFuture};
use async_graphql::Value as GqlValue;

pub struct TestContext {
    pub state: AppState,
    pub base_path: String,
}

pub async fn setup_test_context() -> TestContext {
    let base_path = format!("test_data_{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&base_path).unwrap();

    // 1. Vault
    let master_key = MasterKey::from_string("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()).unwrap();
    let vault = Arc::new(Vault::new(&master_key));

    // 2. Vector Provider (Mock)
    struct MockVectorProvider;
    #[async_trait::async_trait]
    impl VectorProvider for MockVectorProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> { Ok(vec![0.0; 384]) }
        async fn search(&self, _c: i64, _f: &str, _v: &[f32], _l: usize) -> Result<Vec<(i64, f32)>, String> { Ok(vec![]) }
        async fn index(&self, _c: i64, _r: i64, _f: &str, _v: &[f32]) -> Result<(), String> { Ok(()) }
    }
    let vector_provider: Arc<dyn VectorProvider> = Arc::new(MockVectorProvider);

    // 3. DB
    let raw_db = apexkit_core::ApexKit::init_filesystem(&base_path, vector_provider.clone()).await.unwrap();
    let cached_db = Arc::new(CachedDb::new(Arc::new(raw_db)));

    // 4. Managers
    let tenant_manager = Arc::new(TenantManager::new(None, 10));
    let sandbox_manager = Arc::new(SandboxManager::new(None));

    // 5. Job System
    let job_context = Arc::new(GlobalJobContext {
        root_db: cached_db.clone(),
        root_vector_provider: vector_provider.clone(),
        tenant_manager: tenant_manager.clone(),
        sandbox_manager: sandbox_manager.clone(),
    });
    let queue = jobs::start_background_worker(job_context, vault.clone());

    // 6. Realtime
    let (tx, _) = broadcast::channel(100);

    // 7. Storage
    let storage_path = format!("{}/storage", base_path);
    let storage: Arc<dyn StorageBackend> = Arc::new(LocalStorage::new(&storage_path, "/api/v1/storage/file/").await);

    // 8. Script Engine & Other
    let script_engine = Arc::new(ScriptEngine::new().await);
    let scheduler = Arc::new(RwLock::new(apexkit_api::scheduler::SchedulerService::new().await));
    let thumb_cache = Cache::builder().max_capacity(10).build();
    let embedder = Arc::new(EmbedderService::new());

    // 9. Schema
    let dummy_schema = Schema::build("Query", None, None)
        .register(Object::new("Query").field(Field::new("version", TypeRef::named_nn(TypeRef::STRING), |_| FieldFuture::new(async {Ok(Some(GqlValue::from("1.0")))}))))
        .finish()
        .unwrap();

    let state = AppState {
        db: cached_db.clone(),
        tenant_manager,
        sandbox_manager,
        queue,
        metrics: None,
        tx,
        storage,
        vault,
        schema: Arc::new(RwLock::new(dummy_schema)),
        scheduler,
        script_engine,
        css_cache: Arc::new(RwLock::new(String::new())),
        thumb_cache,
        embedder,
        vector_provider,
        port: 0,
    };

    TestContext {
        state,
        base_path,
    }
}

pub async fn setup_test_app() -> Router {
    let ctx = setup_test_context().await;
    app_router(ctx.state)
}

pub fn admin_token() -> String {
    apexkit_core::auth::create_jwt(1, "admin@apexkit.io", "admin", "root").unwrap()
}

pub fn test_request() -> axum::http::request::Builder {
    axum::http::Request::builder()
        .header("host", "localhost")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 1234))))
}

pub fn base_request() -> axum::http::request::Builder {
    test_request()
        .header("authorization", format!("Bearer {}", admin_token()))
        .header("content-type", "application/json")
}
