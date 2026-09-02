use apexkit_api::AppState;
use apexkit_core::database::cache::CachedDb;
use apexkit_core::database::sqlite::connections::ApexKit;
use apexkit_core::database::traits::{Db, VectorProvider};
use apexkit_core::security::vault::{MasterKey, Vault};
use apexkit_core::workers::{Job, JobContext, JobQueue};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use moka::future::Cache;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

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

#[allow(dead_code)]
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
    let path = std::env::temp_dir().join(format!("apexkit_scripting_test_{}", rand_id));
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

// Helper to execute a JS script within our Test AppState
async fn run_js(
    state: &AppState,
    code: &str,
    payload: Value,
    method: Option<String>,
    url: Option<String>,
) -> Result<Value, String> {
    let context = Arc::new(apexkit_api::context::ScopedScriptContext {
        state: state.clone(),
        scope: apexkit_core::realtime::EventScope::Root,
    });

    state
        .script_engine
        .run_script(
            code,
            payload,
            context,
            Some("http://localhost".into()),
            None,
            method,
            url,
        )
        .await
}

#[tokio::test]
async fn test_script_basic_response() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        export default async function(req) {
            const body = await req.json();
            return new Response({ ok: true, input: body.input_val }, { status: 201 });
        }
    "#;

    let res = run_js(&state, code, json!({ "input_val": 42 }), None, None)
        .await
        .unwrap();

    // Verify __is_apex_response wrapper
    assert_eq!(res["__is_apex_response"], true);
    assert_eq!(res["status"], 201);
    assert_eq!(res["body"]["ok"], true);
    assert_eq!(res["body"]["input"], 42);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_script_request_parsing() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        export default async function(req) {
            return new Response({
                method: req.method,
                url: req.url,
            });
        }
    "#;

    let res = run_js(
        &state,
        code,
        json!({}),
        Some("PATCH".to_string()),
        Some("https://example.com/api/test".to_string()),
    )
    .await
    .unwrap();

    assert_eq!(res["body"]["method"], "PATCH");
    assert_eq!(res["body"]["url"], "https://example.com/api/test");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_script_util_builtins() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        export default async function(req) {
            return new Response({
                b64: $util.base64Encode("hello"),
                slug: $util.slugify("Hello World!"),
                hash: $util.hash("test", "sha256")
            });
        }
    "#;

    let res = run_js(&state, code, json!({}), None, None).await.unwrap();

    assert_eq!(res["body"]["b64"], "aGVsbG8=");
    assert_eq!(res["body"]["slug"], "hello-world");
    assert_eq!(
        res["body"]["hash"],
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_script_db_access() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        export default async function(req) {
            // Test DB collections list mapping
            const cols = await $db.collections.list();
            return new Response({ cols: cols });
        }
    "#;

    let res = run_js(&state, code, json!({}), None, None).await.unwrap();

    // By default, a fresh DB has no collections
    assert!(res["body"]["cols"].is_array());
    assert_eq!(res["body"]["cols"].as_array().unwrap().len(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}
