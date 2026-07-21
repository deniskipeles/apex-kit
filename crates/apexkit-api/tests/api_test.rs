use apexkit_api::{AppState, server::router::app_router};
use apexkit_core::database::cache::CachedDb;
use apexkit_core::database::sqlite::connections::ApexKit;
use apexkit_core::database::traits::{Db, VectorProvider};
use apexkit_core::security::vault::{MasterKey, Vault};
use apexkit_core::workers::{Job, JobContext, JobQueue};
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use moka::future::Cache;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tower::ServiceExt;

struct MockVectorProvider;

#[async_trait::async_trait]
impl VectorProvider for MockVectorProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> {
        Ok(vec![0.1, 0.2, 0.3])
    }
    async fn embed_image(&self, _base64_image: &str) -> Result<Vec<f32>, String> {
        Ok(vec![0.4, 0.5, 0.6])
    }
    async fn embed_text_for_image_search(&self, _text: &str) -> Result<Vec<f32>, String> {
        Ok(vec![0.7, 0.8, 0.9])
    }
    async fn search(
        &self,
        _col_id: i64,
        _field: &str,
        _vec: &[f32],
        _limit: usize,
    ) -> Result<Vec<(i64, f32)>, String> {
        Ok(vec![(1, 0.99)])
    }
    async fn index(
        &self,
        _col_id: i64,
        _rec_id: i64,
        _field: &str,
        _vec: &[f32],
    ) -> Result<(), String> {
        Ok(())
    }
}

struct TestJobContext {
    db: Arc<dyn Db>,
    vp: Arc<dyn VectorProvider>,
}

#[async_trait::async_trait]
impl JobContext for TestJobContext {
    async fn resolve(
        &self,
        _tenant_id: Option<&str>,
    ) -> Option<(Arc<dyn Db>, Arc<dyn VectorProvider>)> {
        Some((self.db.clone(), self.vp.clone()))
    }

    async fn get_file_bytes(
        &self,
        _tenant_id: Option<&str>,
        _filename: &str,
    ) -> Result<Vec<u8>, String> {
        Ok(vec![])
    }
}

fn generate_temp_dir() -> std::path::PathBuf {
    let rand_id = uuid::Uuid::new_v4().to_string();
    let path = std::env::temp_dir().join(format!("apexkit_api_test_{}", rand_id));
    std::fs::create_dir_all(&path).unwrap();
    path
}

async fn setup_test_state(base_path: &std::path::Path) -> AppState {
    let core = Connection::open(base_path.join("core.db")).unwrap();
    let data = Connection::open(base_path.join("data.db")).unwrap();
    let log = Connection::open(base_path.join("logs.db")).unwrap();
    let sys = Connection::open(base_path.join("system.db")).unwrap();
    let vec = Connection::open(base_path.join("vectors.db")).unwrap();

    apexkit_core::database::sqlite::setup::apply_pragmas(&core).unwrap();
    apexkit_core::database::sqlite::setup::apply_pragmas(&data).unwrap();
    apexkit_core::database::sqlite::setup::apply_pragmas(&log).unwrap();
    apexkit_core::database::sqlite::setup::apply_pragmas(&sys).unwrap();
    apexkit_core::database::sqlite::setup::apply_pragmas(&vec).unwrap();

    apexkit_core::database::sqlite::setup::setup_core(&core).unwrap();
    apexkit_core::database::sqlite::setup::setup_data(&data).unwrap();
    apexkit_core::database::sqlite::setup::setup_logs(&log).unwrap();
    apexkit_core::database::sqlite::setup::setup_sys(&sys).unwrap();
    apexkit_core::database::sqlite::setup::setup_vectors(&vec).unwrap();

    let path_str = base_path.to_str().unwrap().to_string();
    let raw_db = Arc::new(ApexKit::new(
        &path_str,
        core,
        data,
        log,
        sys,
        vec,
        Arc::new(MockVectorProvider),
        None,
        None,
        "root".to_string(),
    ));

    let cached_db = Arc::new(CachedDb::new(raw_db));

    let master_key_bytes = [0u8; 32];
    let master_key_str = STANDARD.encode(&master_key_bytes);
    let master_key = MasterKey::from_string(master_key_str).unwrap();
    let vault = Arc::new(Vault::new(&master_key));

    let vp = Arc::new(MockVectorProvider);

    let _job_ctx = Arc::new(TestJobContext {
        db: cached_db.clone(),
        vp: vp.clone(),
    });
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Job>(100);
    // Drain background jobs
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let queue = JobQueue::new(tx);

    let tenant_manager = Arc::new(apexkit_api::workspaces_manager::tenant::TenantManager::new(
        None,
        cached_db.clone(),
        100,
        None,
        None,
    ));

    let sandbox_manager =
        Arc::new(apexkit_api::workspaces_manager::sandbox::SandboxManager::new(None, None, None));

    let (db_event_tx, _db_event_rx) = broadcast::channel(100);

    let empty_schema = async_graphql::dynamic::Schema::build("Query", None, None)
        .register(async_graphql::dynamic::Object::new("Query").field(
            async_graphql::dynamic::Field::new(
                "status",
                async_graphql::dynamic::TypeRef::named(async_graphql::dynamic::TypeRef::STRING),
                |_| {
                    async_graphql::dynamic::FieldFuture::new(async {
                        Ok(Some(async_graphql::Value::from("Initializing...")))
                    })
                },
            ),
        ))
        .finish()
        .unwrap();

    let scheduler = apexkit_api::system::scheduler::SchedulerService::new().await;

    let storage: Arc<dyn apexkit_core::storage::StorageBackend> =
        Arc::new(apexkit_api::api::storage::DynamicStorage::new(
            cached_db.clone(),
            vault.clone(),
            None,
            "/api/v1/storage/file/".to_string(),
        ));

    AppState {
        db: cached_db.clone(),
        tenant_manager,
        sandbox_manager,
        queue,
        metrics: None,
        tx: db_event_tx,
        storage,
        vault,
        schema: Arc::new(RwLock::new(empty_schema)),
        scheduler: Arc::new(RwLock::new(scheduler)),
        script_engine: Arc::new(apexkit_core::scripting::ScriptEngine::new().await),
        css_cache: Arc::new(RwLock::new(HashMap::new())),
        thumb_cache: Cache::builder().max_capacity(1000).build(),
        embedder: Arc::new(apexkit_core::embeddings::EmbedderService::new()),
        vector_provider: vp,
        port: 5000,
        root_script_cache: Cache::builder().max_capacity(1000).build(),
        record_count_cache: Cache::builder().max_capacity(1000).build(),
        rate_limiters: Cache::builder().max_capacity(100).build(),
    }
}

#[tokio::test]
async fn test_api_health_check() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;
    let app = app_router(state);

    let request = Request::builder()
        .uri("/healthz")
        .header("Host", "localhost")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "OK");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_api_version() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;
    let app = app_router(state);

    let request = Request::builder()
        .uri("/version")
        .header("Host", "localhost")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_json: Value = serde_json::from_slice(&body).unwrap();
    assert!(body_json["core"].is_string());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_api_auth_register_and_login() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;
    let app = app_router(state.clone());

    // 1. Set general settings to allow public registration
    let general_settings = json!({
        "allow_public_registration": true
    });
    // Write directly to DB config store
    state
        .db
        .set_config("general", &general_settings, false)
        .await
        .unwrap();

    // 2. Register a new user
    let register_payload = json!({
        "email": "test@apexkit.io",
        "password": "strongpassword123",
        "role": "user"
    });

    let mut register_request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header("Host", "localhost")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_payload).unwrap()))
        .unwrap();

    register_request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

    let register_response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(register_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(register_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let register_res: Value = serde_json::from_slice(&body).unwrap();
    assert!(register_res["token"].is_string());
    assert_eq!(register_res["user"]["email"], "test@apexkit.io");

    // 3. Login with the registered user
    let login_payload = json!({
        "email": "test@apexkit.io",
        "password": "strongpassword123"
    });

    let mut login_request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header("Host", "localhost")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&login_payload).unwrap()))
        .unwrap();

    login_request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

    let login_response = app.oneshot(login_request).await.unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);

    let login_body = axum::body::to_bytes(login_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let login_res: Value = serde_json::from_slice(&login_body).unwrap();
    assert!(login_res["token"].is_string());
    assert_eq!(login_res["user"]["email"], "test@apexkit.io");

    let _ = std::fs::remove_dir_all(&dir);
}
