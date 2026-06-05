use apexkit_api::sandbox_manager::SandboxManager;
use apexkit_api::tenant_manager::TenantManager;
use apexkit_api::{AppState, GlobalJobContext, app_router};
use apexkit_core::{
    VectorProvider,
    cache::CachedDb,
    embeddings::EmbedderService,
    jobs,
    models::ChangesetEvent,
    scripting::ScriptEngine,
    security::{MasterKey, Vault},
    storage::{LocalStorage, StorageBackend},
};
use async_graphql::Value as GqlValue;
use async_graphql::dynamic::{Field, FieldFuture, Object, Schema, TypeRef};
use axum::{Router, extract::ConnectInfo};
use moka::future::Cache;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

pub struct TestContext {
    pub state: AppState,
    pub base_path: String,
    pub app: Router,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Cleanup test directories after the test finishes
        let _ = std::fs::remove_dir_all(&self.base_path);
    }
}

pub async fn setup_test_context_with_forwarder(
    forwarder: Option<Arc<dyn apexkit_core::batching::WriteForwarder>>,
    event_tx: Option<tokio::sync::broadcast::Sender<ChangesetEvent>>,
) -> TestContext {
    let base_path = format!("storage/test_data_{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&base_path).unwrap();

    let master_key =
        MasterKey::from_string("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()).unwrap();
    let vault = Arc::new(Vault::new(&master_key));

    struct MockVectorProvider;
    #[async_trait::async_trait]
    impl VectorProvider for MockVectorProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> {
            Ok(vec![0.0; 384])
        }
        async fn embed_image(&self, _b64: &str) -> Result<Vec<f32>, String> {
            Ok(vec![0.0; 384])
        }
        async fn search(
            &self,
            _c: i64,
            _f: &str,
            _v: &[f32],
            _l: usize,
        ) -> Result<Vec<(i64, f32)>, String> {
            Ok(vec![])
        }
        async fn index(&self, _c: i64, _r: i64, _f: &str, _v: &[f32]) -> Result<(), String> {
            Ok(())
        }
    }
    let vector_provider: Arc<dyn VectorProvider> = Arc::new(MockVectorProvider);

    let raw_db = apexkit_core::ApexKit::init_filesystem(
        &format!("{}/system", base_path),
        vector_provider.clone(),
        forwarder.clone(),
        event_tx.clone(),
        "root".to_string(),
    )
    .await
    .unwrap();

    let cached_db = Arc::new(CachedDb::new(Arc::new(raw_db)));

    let tenant_manager = Arc::new(TenantManager::new(
        None,
        cached_db.clone(),
        10,
        forwarder.clone(),
        event_tx.clone(),
    ));
    let sandbox_manager = Arc::new(SandboxManager::new(
        None,
        forwarder.clone(),
        event_tx.clone(),
    ));

    let job_context = Arc::new(GlobalJobContext {
        root_db: cached_db.clone(),
        root_vector_provider: vector_provider.clone(),
        tenant_manager: tenant_manager.clone(),
        sandbox_manager: sandbox_manager.clone(),
    });
    let queue = jobs::start_background_worker(job_context, vault.clone());

    let (tx, _) = broadcast::channel(100);

    let storage_path = format!("{}/system/uploads", base_path);
    let storage: Arc<dyn StorageBackend> =
        Arc::new(LocalStorage::new(&storage_path, "/api/v1/storage/file/").await);

    let script_engine = Arc::new(ScriptEngine::new().await);
    let scheduler = Arc::new(RwLock::new(
        apexkit_api::scheduler::SchedulerService::new().await,
    ));
    let thumb_cache = Cache::builder().max_capacity(10).build();
    let root_script_cache = Cache::builder().max_capacity(10).build();
    let record_count_cache = Cache::builder().max_capacity(10).build();
    let rate_limiters = Cache::builder().max_capacity(10).build();
    let embedder = Arc::new(EmbedderService::new());

    let dummy_schema = Schema::build("Query", None, None)
        .register(Object::new("Query").field(Field::new(
            "version",
            TypeRef::named_nn(TypeRef::STRING),
            |_| FieldFuture::new(async { Ok(Some(GqlValue::from("1.0"))) }),
        )))
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
        root_script_cache,
        record_count_cache,
        rate_limiters,
        embedder,
        vector_provider,
        port: 0,
    };

    let app = app_router(state.clone());

    TestContext {
        state,
        base_path,
        app,
    }
}

pub async fn setup_test_app() -> Router {
    let ctx = setup_test_context_with_forwarder(None, None).await;
    ctx.app.clone()
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
