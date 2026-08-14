use apexkit_core::database::cache::CachedDb;
use apexkit_core::database::sqlite::connections::ApexKit;
use apexkit_core::database::traits::{Db, VectorProvider};
use apexkit_core::realtime::EventScope;
use apexkit_core::scripting::ScriptEngine;
use apexkit_core::scripting::module_loader::VfsState;
use apexkit_core::security::vault::{MasterKey, Vault};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::sync::Arc;

struct MockVectorProvider;

#[async_trait::async_trait]
impl VectorProvider for MockVectorProvider {
    async fn embed(&self, _t: &str) -> Result<Vec<f32>, String> {
        Ok(vec![])
    }
    async fn embed_image(&self, _i: &str) -> Result<Vec<f32>, String> {
        Ok(vec![])
    }
    async fn embed_text_for_image_search(&self, _t: &str) -> Result<Vec<f32>, String> {
        Ok(vec![])
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

fn generate_temp_dir() -> std::path::PathBuf {
    let rand_id = uuid::Uuid::new_v4().to_string();
    let path = std::env::temp_dir().join(format!("apexkit_wasm_test_{}", rand_id));
    std::fs::create_dir_all(&path).unwrap();
    path
}

async fn setup_test_context(base_path: &std::path::Path) -> Arc<dyn apexkit_core::ScriptContext> {
    let core = Connection::open(base_path.join("core.db")).unwrap();
    let data = Connection::open(base_path.join("data.db")).unwrap();
    let log = Connection::open(base_path.join("logs.db")).unwrap();
    let sys = Connection::open(base_path.join("system.db")).unwrap();
    let vec = Connection::open(base_path.join("vectors.db")).unwrap();

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

    let uploads_dir = base_path.join("uploads");
    std::fs::create_dir_all(&uploads_dir).unwrap();
    let storage: Arc<dyn apexkit_core::storage::StorageBackend> =
        Arc::new(apexkit_core::storage::local::LocalStorage {
            base_path: uploads_dir,
            base_url: "/api/v1/storage/file/".into(),
        });

    struct TestScriptContext {
        db: Arc<dyn Db>,
        vault: Arc<Vault>,
        vp: Arc<dyn VectorProvider>,
        storage: Arc<dyn apexkit_core::storage::StorageBackend>,
    }

    #[async_trait::async_trait]
    impl apexkit_core::ScriptContext for TestScriptContext {
        fn get_db(&self) -> Arc<dyn Db> {
            self.db.clone()
        }
        fn get_vault(&self) -> Arc<Vault> {
            self.vault.clone()
        }
        fn get_embedder(&self) -> Arc<apexkit_core::embeddings::EmbedderService> {
            Arc::new(apexkit_core::embeddings::EmbedderService::new())
        }
        fn get_vector_provider(&self) -> Arc<dyn VectorProvider> {
            self.vp.clone()
        }
        fn get_realtime_tx(
            &self,
        ) -> tokio::sync::broadcast::Sender<apexkit_core::realtime::DbEvent> {
            tokio::sync::broadcast::channel(1).0
        }
        fn get_storage(&self) -> Arc<dyn apexkit_core::storage::StorageBackend> {
            self.storage.clone()
        }
        fn get_scope(&self) -> EventScope {
            EventScope::Root
        }
        fn get_scoped_vector_provider(
            &self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Arc<dyn VectorProvider>> + Send>>
        {
            let vp = self.vp.clone();
            Box::pin(async move { vp })
        }
        fn check_quota(
            &self,
            _metric: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        {
            Box::pin(async move { Ok(()) })
        }
        fn get_shared_script(
            &self,
            _name: &str,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Option<apexkit_core::models::script::Script>>
                    + Send,
            >,
        > {
            Box::pin(async move { None })
        }
        fn execute_shared_script(
            &self,
            _code: String,
            _payload: Value,
            _scope: EventScope,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>>
        {
            Box::pin(async move { Ok(json!({})) })
        }
        fn resolve_tenant_db(
            &self,
            _tenant_id: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>>
        {
            Box::pin(async move { None })
        }
        fn resolve_sandbox_db(
            &self,
            _session_id: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>>
        {
            Box::pin(async move { None })
        }
        fn admin_create_tenant(
            &self,
            _id: String,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        {
            Box::pin(async move { Ok(()) })
        }
        fn admin_update_tenant(
            &self,
            _id: String,
            _updates: Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        {
            Box::pin(async move { Ok(()) })
        }
        fn admin_delete_tenant(
            &self,
            _id: String,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        {
            Box::pin(async move { Ok(()) })
        }
        fn admin_get_tenant_usage(
            &self,
            _id: String,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, String>> + Send>>
        {
            Box::pin(async move { Ok(0) })
        }
        fn admin_create_sandbox(
            &self,
            _id: String,
            _config: Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        {
            Box::pin(async move { Ok(()) })
        }
        fn admin_update_sandbox(
            &self,
            _id: String,
            _updates: Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        {
            Box::pin(async move { Ok(()) })
        }
        fn admin_delete_sandbox(
            &self,
            _id: String,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        {
            Box::pin(async move { Ok(()) })
        }
        fn admin_get_sandbox_usage(
            &self,
            _id: String,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, String>> + Send>>
        {
            Box::pin(async move { Ok(0) })
        }
        fn cache_get(
            &self,
            _k: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>> {
            Box::pin(async move { None })
        }
        fn cache_set(
            &self,
            _k: &str,
            _v: &str,
            _t: Option<u64>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
            Box::pin(async move { () })
        }
        fn cache_del(
            &self,
            _k: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
            Box::pin(async move { () })
        }
        fn cache_incr(
            &self,
            _k: &str,
            _d: i64,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = i64> + Send>> {
            Box::pin(async move { 0 })
        }
        fn cache_list_keys(
            &self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<String>> + Send>> {
            Box::pin(async move { vec![] })
        }
    }

    Arc::new(TestScriptContext {
        db: cached_db,
        vault,
        vp: Arc::new(MockVectorProvider),
        storage,
    })
}

#[tokio::test]
async fn test_wasm_execution_f64_add() {
    let dir = generate_temp_dir();
    let ctx = setup_test_context(&dir).await;

    // Purge stale test cache files if any exist
    if let Ok(cache_dir) =
        std::env::current_exe().map(|p| p.parent().unwrap().join(".cache").join("wasm"))
    {
        let _ = std::fs::remove_dir_all(&cache_dir);
    }

    let vfs = VfsState::new();
    let engine = ScriptEngine::with_vfs(vfs, Some(ctx.get_db())).await;

    // Guaranteed 100% valid WASM binary (41 bytes)
    // WAT Equivalent: (module (func (export "add") (param f64 f64) (result f64) local.get 0 local.get 1 f64.add))
    let wasm_bytes: Vec<u8> = vec![
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // Magic & Version
        0x01, 0x07, 0x01, 0x60, 0x02, 0x7c, 0x7c, 0x01,
        0x7c, // Type Section (f64, f64) -> f64
        0x03, 0x02, 0x01, 0x00, // Function Section
        0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00, // Export Section ("add")
        0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0xa0, 0x0b, // Code Section
    ];
    let wasm_base64 = STANDARD.encode(&wasm_bytes);

    let js_code = format!(
        r#"
        export default async function(req) {{
            console.log("Starting WASM test...");
            let b64 = "{}";
            
            // Expected to return 10.5 + 20.5 = 31.0
            let result = await $wasm.call(b64, "add", [10.5, 20.5], {{ name: "add_test.wasm" }});
            
            return new Response(JSON.stringify({{ wasm_result: result }}));
        }}
    "#,
        wasm_base64
    );

    let result = engine
        .run_script(&js_code, json!({}), ctx.clone(), None, None, None, None)
        .await
        .unwrap();

    let expected = 31.0;
    let actual = result["body"]["wasm_result"].as_f64().unwrap();

    assert_eq!(actual, expected, "WASM execution failed to add 10.5 + 20.5");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_wasm_photon_image_processor() {
    let dir = generate_temp_dir();
    let ctx = setup_test_context(&dir).await;

    // Minimal valid PNG image binary (1x1 red pixel)
    let image_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let image_bytes = STANDARD.decode(image_b64).unwrap();

    let test_filename = "a9edec4d-0854-40a7-ac6d-e9459ae04e7a.jpg";
    ctx.get_storage()
        .save(test_filename, &image_bytes, "image/jpeg")
        .await
        .unwrap();

    let vfs = VfsState::new();
    let engine = ScriptEngine::with_vfs(vfs, Some(ctx.get_db())).await;

    let js_code = r#"
import init, { resize, PhotonImage } from "https://esm.sh/@silvia-odwyer/photon@0.3.3";

export default async function (req) {
    await init("https://esm.sh/@silvia-odwyer/photon@0.3.3/es2022/photon_rs_bg.wasm");

    // 1. Read Base64 image
    const base64Data = await $files.read("a9edec4d-0854-40a7-ac6d-e9459ae04e7a.jpg");
    
    // 2. Decode into Uint8Array
    const arrayBuffer = $util.base64DecodeBuffer(base64Data);
    const inputBytes = new Uint8Array(arrayBuffer);

    // 3. Load into Photon
    const img = PhotonImage.new_from_byteslice(inputBytes);
    
    // 4. Resize (returns the new resized PhotonImage)
    const resizedImg = resize(img, 800, 600, 1); 
    
    // 5. Get JPEG output bytes (quality 85)
    const outputBytes = resizedImg.get_bytes_jpeg(85);
    
    // 6. Encode Uint8Array to Base64 using $util.base64Encode
    const outBase64 = $util.base64Encode(outputBytes);
    
    // 7. Save file
    const newFile = await $files.save("resized_output.jpg", outBase64, "image/jpeg");
    
    return new Response({
        success: true,
        message: "Image resized successfully!",
        file: newFile
    });
}
"#;

    let result = engine
        .run_script(js_code, json!({}), ctx.clone(), None, None, None, None)
        .await
        .unwrap();

    assert_eq!(result["__is_apex_response"], true);
    assert_eq!(result["status"], 200);
    assert_eq!(result["body"]["success"], true);
    assert_eq!(result["body"]["message"], "Image resized successfully!");
    assert!(result["body"]["file"]["url"].is_string());

    let _ = std::fs::remove_dir_all(&dir);
}
