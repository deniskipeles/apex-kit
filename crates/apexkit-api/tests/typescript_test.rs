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
    let path = std::env::temp_dir().join(format!("apexkit_ts_test_{}", rand_id));
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

async fn run_ts(
    state: &AppState,
    ts_code: &str,
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
            ts_code,
            payload,
            context,
            Some("http://localhost:5000".into()),
            None,
            method,
            url,
        )
        .await
}

// =========================================================================
// TYPESCRIPT TEST CASES
// =========================================================================

#[tokio::test]
async fn test_ts_interfaces_and_type_annotations() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        interface UserProfile {
            id: number;
            username: string;
            isActive: boolean;
            tags: string[];
            score?: number;
        }

        type ApiResponse<T> = {
            success: boolean;
            data: T;
            timestamp: number;
        };

        export default async function(req: Request): Promise<Response> {
            const body: { username: string; score: number } = await req.json();

            const profile: UserProfile = {
                id: 101,
                username: body.username,
                isActive: true,
                tags: ["developer", "rust", "typescript"],
                score: body.score
            };

            const response: ApiResponse<UserProfile> = {
                success: true,
                data: profile,
                timestamp: Date.now()
            };

            return new Response(response, { status: 200 });
        }
    "#;

    let payload = json!({ "username": "alex", "score": 98.5 });
    let res = run_ts(&state, code, payload, None, None).await.unwrap();

    assert_eq!(res["status"], 200);
    assert_eq!(res["body"]["success"], true);
    assert_eq!(res["body"]["data"]["id"], 101);
    assert_eq!(res["body"]["data"]["username"], "alex");
    assert_eq!(res["body"]["data"]["isActive"], true);
    assert_eq!(res["body"]["data"]["score"], 98.5);
    assert_eq!(res["body"]["data"]["tags"][0], "developer");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_ts_enums_and_generics() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        enum Role {
            Admin = "ADMIN",
            Editor = "EDITOR",
            Viewer = "VIEWER"
        }

        enum HttpCode {
            Ok = 200,
            Created = 201,
            BadRequest = 400
        }

        function wrapInList<T>(item: T, count: number): T[] {
            const result: T[] = [];
            for (let i: number = 0; i < count; i++) {
                result.push(item);
            }
            return result;
        }

        export default async function(req: Request): Promise<Response> {
            const role: Role = Role.Admin;
            const items: string[] = wrapInList<string>("apex", 3);

            return new Response({
                role: role,
                statusCode: HttpCode.Created,
                items: items
            }, { status: HttpCode.Created });
        }
    "#;

    let res = run_ts(&state, code, json!({}), None, None).await.unwrap();

    assert_eq!(res["status"], 201);
    assert_eq!(res["body"]["role"], "ADMIN");
    assert_eq!(res["body"]["statusCode"], 201);
    assert_eq!(res["body"]["items"].as_array().unwrap().len(), 3);
    assert_eq!(res["body"]["items"][0], "apex");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_ts_classes_and_parameter_properties() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        class Calculator {
            private multiplier: number;

            constructor(multiplier: number = 1) {
                this.multiplier = multiplier;
            }

            public multiply(value: number): number {
                return value * this.multiplier;
            }

            public calculateSum(numbers: number[]): number {
                return numbers.reduce((acc: number, cur: number) => acc + cur, 0) * this.multiplier;
            }
        }

        export default async function(req: Request): Promise<Response> {
            const body: { factor: number; values: number[] } = await req.json();
            const calc = new Calculator(body.factor);

            const multiplied = calc.multiply(10);
            const total = calc.calculateSum(body.values);

            return new Response({
                singleResult: multiplied,
                sumResult: total
            });
        }
    "#;

    let payload = json!({ "factor": 3, "values": [1, 2, 3, 4] });
    let res = run_ts(&state, code, payload, None, None).await.unwrap();

    assert_eq!(res["status"], 200);
    assert_eq!(res["body"]["singleResult"], 30);
    assert_eq!(res["body"]["sumResult"], 30); // (1+2+3+4) * 3 = 30

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_ts_unions_assertions_and_nullish_coalescing() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        type ResultState = 
            | { status: "success"; payload: string }
            | { status: "error"; message: string; code: number };

        function formatState(res: ResultState): string {
            if (res.status === "success") {
                return `OK: ${res.payload}`;
            } else {
                return `ERR [${res.code}]: ${res.message}`;
            }
        }

        export default async function(req: Request): Promise<Response> {
            const rawBody: any = await req.json();
            
            // Type assertion & optional chaining / nullish coalescing
            const maybeName = (rawBody?.profile?.name as string) ?? "Anonymous";
            const score: number = (rawBody?.score as number) ?? 0;

            const successState: ResultState = { status: "success", payload: maybeName };
            const errorState: ResultState = { status: "error", message: "Failed", code: 404 };

            return new Response({
                name: maybeName,
                score: score,
                formattedSuccess: formatState(successState),
                formattedError: formatState(errorState)
            });
        }
    "#;

    let payload = json!({ "profile": { "name": "Sarah" }, "score": null });
    let res = run_ts(&state, code, payload, None, None).await.unwrap();

    assert_eq!(res["status"], 200);
    assert_eq!(res["body"]["name"], "Sarah");
    assert_eq!(res["body"]["score"], 0); // nullish fallback to 0
    assert_eq!(res["body"]["formattedSuccess"], "OK: Sarah");
    assert_eq!(res["body"]["formattedError"], "ERR [404]: Failed");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_ts_utility_types() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        interface Article {
            id: string;
            title: string;
            body: string;
            published: boolean;
            views: number;
        }

        type ArticleDraft = Partial<Article>;
        type ArticleSummary = Pick<Article, "id" | "title">;
        type ArticleUpdate = Omit<Article, "id">;
        type StatsMap = Record<string, number>;

        export default async function(req: Request): Promise<Response> {
            const draft: ArticleDraft = { title: "Draft Title" };
            const summary: ArticleSummary = { id: "art_1", title: "Summary Title" };
            const stats: StatsMap = { views: 1500, likes: 320 };

            return new Response({
                draft,
                summary,
                stats
            });
        }
    "#;

    let res = run_ts(&state, code, json!({}), None, None).await.unwrap();

    assert_eq!(res["status"], 200);
    assert_eq!(res["body"]["draft"]["title"], "Draft Title");
    assert_eq!(res["body"]["summary"]["id"], "art_1");
    assert_eq!(res["body"]["stats"]["views"], 1500);
    assert_eq!(res["body"]["stats"]["likes"], 320);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_ts_with_apex_sdk_and_globals() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        interface CalculationResult {
            uuid: string;
            slug: string;
            hash: string;
            urlInfo: {
                origin: string;
                pathname: string;
                queryParam: string | null;
            };
        }

        export default async function(req: Request): Promise<Response> {
            // Test standard Request & URL properties in TypeScript
            const url = new URL(req.url);
            const queryParam: string | null = url.searchParams.get("query");

            // Test $util built-in API in TypeScript
            const generatedId: string = $util.uuid();
            const slugified: string = $util.slugify("TypeScript on ApexKit BaaS");
            const hashVal: string = $util.hash("secure_payload", "sha256");

            const payload: CalculationResult = {
                uuid: generatedId,
                slug: slugified,
                hash: hashVal,
                urlInfo: {
                    origin: url.origin,
                    pathname: url.pathname,
                    queryParam: queryParam
                }
            };

            const headers = new Headers();
            headers.set("X-Apex-Engine", "QuickJS-TypeScript");

            return new Response(payload, {
                status: 200,
                headers: headers
            });
        }
    "#;

    let res = run_ts(
        &state,
        code,
        json!({}),
        Some("GET".into()),
        Some("https://api.apexkit.io/run/test-ts?query=awesome".into()),
    )
    .await
    .unwrap();

    assert_eq!(res["status"], 200);
    assert_eq!(res["headers"]["x-apex-engine"], "QuickJS-TypeScript");
    assert_eq!(res["body"]["slug"], "typescript-on-apexkit-baas");
    assert_eq!(res["body"]["urlInfo"]["queryParam"], "awesome");
    assert!(!res["body"]["uuid"].as_str().unwrap().is_empty());
    assert!(!res["body"]["hash"].as_str().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_ts_async_pipeline_and_higher_order_functions() {
    let dir = generate_temp_dir();
    let state = setup_test_state(&dir).await;

    let code = r#"
        interface Transaction {
            id: string;
            amount: number;
            type: "credit" | "debit";
        }

        async function processTransaction(tx: Transaction): Promise<number> {
            await $util.sleep(5); // Non-blocking async sleep
            return tx.type === "credit" ? tx.amount : -tx.amount;
        }

        export default async function(req: Request): Promise<Response> {
            const transactions: Transaction[] = [
                { id: "tx_1", amount: 100, type: "credit" },
                { id: "tx_2", amount: 40, type: "debit" },
                { id: "tx_3", amount: 250, type: "credit" }
            ];

            const settledAmounts: number[] = await Promise.all(
                transactions.map((tx: Transaction): Promise<number> => processTransaction(tx))
            );

            const netBalance: number = settledAmounts.reduce(
                (sum: number, amt: number): number => sum + amt, 
                0
            );

            return new Response({
                settledAmounts,
                netBalance
            });
        }
    "#;

    let res = run_ts(&state, code, json!({}), None, None).await.unwrap();

    assert_eq!(res["status"], 200);
    assert_eq!(res["body"]["netBalance"], 310); // 100 - 40 + 250 = 310
    let arr = res["body"]["settledAmounts"].as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], 100);
    assert_eq!(arr[1], -40);
    assert_eq!(arr[2], 250);

    let _ = std::fs::remove_dir_all(&dir);
}
