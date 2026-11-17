//! Shared test harness for apexkit-api integration tests.
//!
//! Wires a full `AppState` (real SQLite DB + router + GraphQL) so tests can
//! drive the Axum router with `tower::ServiceExt::oneshot` exactly as the
//! production server would, minus the TCP bind.

#![allow(dead_code)]

use apexkit_api::{AppState, server::router::app_router};
use apexkit_core::database::cache::CachedDb;
use apexkit_core::database::sqlite::connections::ApexKit;
use apexkit_core::database::traits::{Db, VectorProvider};
use apexkit_core::security::vault::{MasterKey, Vault};
use apexkit_core::workers::{Job, JobContext, JobQueue};
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use axum::Router;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use moka::future::Cache;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tower::ServiceExt;

/// Mock vector provider: fixed vectors, no model loading.
pub struct MockVectorProvider;

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

pub fn temp_dir(tag: &str) -> PathBuf {
    let id = uuid::Uuid::new_v4();
    let path = std::env::temp_dir().join(format!("apexkit_api_{tag}_{id}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// Builds a fully-wired `AppState` rooted at `base_path`.
pub async fn setup_test_state(base_path: &std::path::Path) -> AppState {
    use apexkit_core::database::sqlite::setup::{
        apply_pragmas, setup_core, setup_data, setup_logs, setup_sys, setup_vectors,
    };

    let core = Connection::open(base_path.join("core.db")).unwrap();
    let data = Connection::open(base_path.join("data.db")).unwrap();
    let log = Connection::open(base_path.join("logs.db")).unwrap();
    let sys = Connection::open(base_path.join("system.db")).unwrap();
    let vec = Connection::open(base_path.join("vectors.db")).unwrap();

    for c in [&core, &data, &log, &sys, &vec] {
        apply_pragmas(c).unwrap();
    }
    setup_core(&core).unwrap();
    setup_data(&data).unwrap();
    setup_logs(&log).unwrap();
    setup_sys(&sys).unwrap();
    setup_vectors(&vec).unwrap();

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

    let master_key = MasterKey::from_string(STANDARD.encode([0u8; 32])).unwrap();
    let vault = Arc::new(Vault::new(&master_key));

    let vp: Arc<dyn VectorProvider> = Arc::new(MockVectorProvider);

    let _job_ctx = Arc::new(TestJobContext {
        db: cached_db.clone(),
        vp: vp.clone(),
    });
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Job>(100);
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
        .register(
            async_graphql::dynamic::Object::new("Query").field(
                async_graphql::dynamic::Field::new(
                    "status",
                    async_graphql::dynamic::TypeRef::named(
                        async_graphql::dynamic::TypeRef::STRING,
                    ),
                    |_| {
                        async_graphql::dynamic::FieldFuture::new(async {
                            Ok(Some(async_graphql::Value::from("Initializing...")))
                        })
                    },
                ),
            ),
        )
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
    let vfs = apexkit_core::scripting::module_loader::VfsState::new();

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
        script_engine: Arc::new(
            apexkit_core::scripting::ScriptEngine::with_vfs(vfs.clone(), Some(cached_db.clone()))
                .await,
        ),
        thumb_cache: Cache::builder().max_capacity(1000).build(),
        embedder: Arc::new(apexkit_core::embeddings::EmbedderService::new()),
        vector_provider: vp,
        port: 5000,
        root_script_cache: Cache::builder().max_capacity(1000).build(),
        record_count_cache: Cache::builder().max_capacity(1000).build(),
        rate_limiters: Cache::builder().max_capacity(100).build(),
        vfs,
    }
}

/// Returns `(dir, state, router)` with a ready-to-drive router.
pub async fn fresh_app(tag: &str) -> (PathBuf, AppState, Router) {
    let dir = temp_dir(tag);
    let state = setup_test_state(&dir).await;
    let app = app_router(state.clone());
    (dir, state, app)
}

/// Issues a request against the router, injecting a loopback `ConnectInfo`.
pub async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let mut req = req;
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

/// Issues a request and returns only the raw body bytes.
pub async fn send_raw(app: &Router, req: Request<Body>) -> (StatusCode, Vec<u8>) {
    let mut req = req;
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

pub fn json_request(method: &str, uri: &str, body: &Value, token: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "localhost")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::from(serde_json::to_vec(body).unwrap())).unwrap()
}

pub fn get_request(uri: &str, token: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::HOST, "localhost");
    if let Some(t) = token {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::empty()).unwrap()
}

/// Logs in as the seeded admin and returns a bearer token.
pub async fn admin_token(state: &AppState) -> String {
    let hash = apexkit_core::auth::hash_password("password").unwrap();
    if state.db.get_user_by_email("admin@apexkit.io").await.unwrap().is_none() {
        state
            .db
            .create_user("admin@apexkit.io", &hash, "admin", None)
            .await
            .unwrap();
    }
    apexkit_core::auth::create_jwt(1, "admin@apexkit.io", "admin", "root").unwrap()
}

/// Creates a collection with a simple schema; returns its id.
pub async fn create_posts_collection(state: &AppState) -> i64 {
    let schema = json!({
        "fields": {
            "title": { "type": "text", "required": true, "ose_indexed": true },
            "views": { "type": "number", "required": false }
        },
        "policies": { "read": "public", "create": "public", "update": "public", "delete": "public" }
    });
    let schema: apexkit_core::models::schema::CollectionSchema =
        serde_json::from_value(schema).unwrap();
    state
        .db
        .create_collection("posts", &Some(schema), None)
        .await
        .unwrap()
}
