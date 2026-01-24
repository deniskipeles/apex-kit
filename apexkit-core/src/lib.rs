use crate::auth::User;
use crate::query::QueryOptions;
use crate::schema::CollectionSchema;
use async_trait::async_trait;
use libsql::{params, Builder, Connection, Database, Result, Row};
use search::SearchManager;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::ai_models::{AiSession, Plugin};
use crate::models::{DashboardData, DashboardStats, ChartPoint, ApiKey};
use chrono::Utc;
use std::collections::{HashMap, BTreeMap};
use std::error::Error as StdError;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering}; 
use crate::schema::FieldType;
use crate::embeddings::EmbedderService;
use crate::security::Vault;

const COMPOSITE_SEPARATOR: &str = "__::__";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// --- Modules ---
pub mod auth;
pub mod cache;
pub mod events;
pub mod jobs;
pub mod models;
pub mod policies;
pub mod query;
pub mod realtime;
pub mod schema;
pub mod search;
pub mod security;
pub mod storage;
pub mod validation;
pub mod ai_models; 
pub mod script_models;
pub mod scripting;
pub mod filter;
pub mod batching;
pub mod embeddings;
pub mod utils;
pub mod query_engine;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)] 
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub schema: Option<CollectionSchema>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)] 
pub struct Record {
    pub id: i64,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand: Option<Value>,
    // [NEW] Timestamps
    pub created: String,
    pub updated: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ListResult {
    pub items: Vec<Record>,
    pub total: i64,
}

// Interface for Vector Operations
#[async_trait]
pub trait VectorProvider: Send + Sync {
    async fn embed(&self, text: &str) -> std::result::Result<Vec<f32>, String>;
    async fn search(&self, col_id: i64, field: &str, vec: &[f32], limit: usize) -> std::result::Result<Vec<(i64, f32)>, String>;
    async fn index(&self, col_id: i64, rec_id: i64, field: &str, vec: &[f32]) -> std::result::Result<(), String>;
}

/// Interface to expose Application State to the Script Engine
/// without creating circular dependencies.
pub trait ScriptContext: Send + Sync {
    fn get_db(&self) -> Arc<dyn Db>;
    fn get_vault(&self) -> Arc<Vault>;
    fn get_embedder(&self) -> Arc<EmbedderService>;
    fn get_vector_provider(&self) -> Arc<dyn VectorProvider>;
    fn get_realtime_tx(&self) -> tokio::sync::broadcast::Sender<crate::realtime::DbEvent>;
    
    // Optional: Only available if Managers are initialized
    fn resolve_tenant_db(&self, tenant_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>>;
    fn resolve_sandbox_db(&self, session_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>>;

    // Expose Admin Manager access if needed
    fn admin_create_tenant(&self, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send>>;
    fn admin_create_sandbox(&self, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send>>;

    // [NEW] Cache Primitives
    fn cache_get(&self, key: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>>;
    fn cache_set(&self, key: &str, val: &str, ttl_secs: Option<u64>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;
    fn cache_del(&self, key: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;
    
    // [NEW] Atomic Increment (Crucial for Quotas)
    // Returns the new value
    fn cache_incr(&self, key: &str, delta: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = i64> + Send>>;
}

#[async_trait]
pub trait Db: Send + Sync {
    // --- Collections ---
    async fn create_collection(&self, name: &str, schema: &Option<CollectionSchema>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_collection(&self, id: i64) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>>;
    async fn list_collections(&self) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_collection(&self, id: i64, name: Option<String>, schema: Option<CollectionSchema>) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_collection(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Records ---
    async fn create_record(&self, collection_id: i64, data: &Value) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    // Allows forcing an ID (for Sandbox cloning/Importing)
    async fn import_record(&self, collection_id: i64, record_id: i64, data: &Value) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_records(&self, collection_id: i64, options: QueryOptions) -> std::result::Result<ListResult, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_record(&self, collection_id: i64, record_id: i64, expand: Option<String>) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_record(&self, collection_id: i64, record_id: i64, data: &Value) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_record(&self, collection_id: i64, record_id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Search ---
    async fn search_records(&self, collection_id: i64, query: &str) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;
    async fn reindex_collection(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
    async fn instant_search(&self, collection_id: i64, query: &str, limit: usize) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>>;
    async fn recover_indexes(&self) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    
    // --- Search Helpers exposed for Job Worker ---
    async fn index_record_search(&self, collection_id: i64, record_id: i64, data: &Value, schema: &CollectionSchema) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
    async fn delete_record_search(&self, collection_id: i64, record_id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    // --- Users (Auth) ---
    async fn create_user(&self, email: &str, password_hash: &str, role: &str, metadata: Option<Value>) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>>;
    
    // Tenants
    async fn register_tenant(&self, id: &str, owner_id: Option<i64>, name: Option<String>, tier: Option<String>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_tenant_status(&self, tenant_id: &str) -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_tenant_status(&self, tenant_id: &str, status: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    // [NEW] Sandbox Management
    async fn register_sandbox(&self, id: &str, owner_id: Option<i64>, name: Option<String>, expires_at: Option<String>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    
    // Allows forcing an ID (for Sandbox cloning)
    async fn import_user(&self, id: i64, email: &str, password_hash: &str, role: &str, metadata: Option<Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_user_by_email(&self, email: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn list_users(&self, query: Option<String>, limit: i64, offset: i64) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn count_users(&self, query: Option<String>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_users_by_ids(&self, ids: &[i64]) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_user(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Storage Metadata ---
    async fn create_file_metadata(&self, filename: &str, original_name: &str, mime_type: &str, size: i64, user_id: Option<i64>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn list_files(&self, limit: i64, offset: i64) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;
    async fn count_files(&self) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>; 
    async fn get_file_metadata(&self, id: i64) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_file_metadata(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Advanced Auth ---
    async fn get_user_by_oauth(&self, provider: &str, provider_id: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn link_oauth(&self, user_id: i64, provider: &str, provider_id: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn create_auth_token(&self, user_id: i64, token_type: &str, token: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn consume_auth_token(&self, token: &str, token_type: &str) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>>;
    async fn set_user_verified(&self, user_id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Secure Config ---
    async fn get_config(&self, key: &str) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;
    async fn set_config(&self, key: &str, value: &serde_json::Value, encrypted: bool) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_configs(&self) -> std::result::Result<Vec<models::ConfigItem>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_config(&self, key: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    
    // --- Audit Logs ---
    async fn log_audit_event(&self, level: &str, message: &str, source: &str, meta: Option<serde_json::Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_audit_logs(&self) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;
    async fn log_system_event(&self, level: &str, target: &str, message: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- API keys ---
    async fn create_api_key(&self, name: &str, role: &str, scope: &str, bypass_cors: bool) -> std::result::Result<(String, ApiKey), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_api_keys(&self) -> std::result::Result<Vec<ApiKey>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_api_key(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn verify_api_key(&self, key: &str) -> std::result::Result<Option<ApiKey>, Box<dyn std::error::Error + Send + Sync>>;

    // --- AI Actions ---
    async fn list_ai_actions(&self) -> std::result::Result<Vec<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_ai_action(&self, slug: &str) -> std::result::Result<Option<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>>;
    async fn create_ai_action(&self, action: ai_models::CreateActionReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_ai_action(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

     // --- AI Sessions ---
     async fn create_ai_session(&self, session: &AiSession) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
     async fn get_ai_session(&self, id: &str) -> std::result::Result<Option<AiSession>, Box<dyn std::error::Error + Send + Sync>>;
     async fn update_ai_session(&self, session: &AiSession) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
     async fn list_ai_sessions(&self) -> std::result::Result<Vec<AiSession>, Box<dyn std::error::Error + Send + Sync>>;
 
     // --- Plugins ---
     async fn save_plugin(&self, plugin: &Plugin) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
     async fn list_plugins(&self) -> std::result::Result<Vec<Plugin>, Box<dyn std::error::Error + Send + Sync>>; 

    // --- Relations ---
    async fn create_relation(&self, origin_col: i64, origin_id: i64, target_col: i64, target_id: i64, rel_name: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_relation(&self, origin_col: i64, origin_id: i64, target_col: i64, target_id: i64, rel_name: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_related_ids(&self, origin_col: i64, origin_id: i64, rel_name: &str) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_records_by_ids(&self, collection_id: i64, ids: &[i64]) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;

    // Scripts
    async fn list_scripts(&self) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>>;
    async fn create_script(&self, req: script_models::CreateScriptReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_script(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_script_by_name(&self, name: &str) -> std::result::Result<Option<script_models::Script>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_scripts_by_trigger(&self, trigger: &str) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>>;

    // Templates
    async fn list_templates(&self) -> std::result::Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_template_by_slug(&self, slug: &str) -> std::result::Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>>;
    async fn create_template(&self, req: models::CreateTemplateReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_template(&self, id: i64, content: String, script_id: Option<i64>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_template(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- DASHBOARD METHODS ---
    async fn get_dashboard_stats(&self) -> std::result::Result<DashboardData, Box<dyn std::error::Error + Send + Sync>>;

    // VECTORS
    async fn get_vectors_for_collection(&self, collection_id: i64, model: &str) -> std::result::Result<Vec<(i64, String, Vec<f32>)>, Box<dyn StdError + Send + Sync>>;

    async fn search_vector(&self, collection_id: i64, field: &str, vector: Vec<f32>, limit: usize) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;
    async fn save_vector(&self, collection_id: i64, record_id: i64, field_name: &str, vector: Vec<f32>, model: &str) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
    async fn has_vector(&self, collection_id: i64, record_id: i64, field_name: &str, model: &str) -> std::result::Result<bool, Box<dyn StdError + Send + Sync>>;
    async fn get_record_vectors(&self, collection_id: i64, record_id: i64) -> std::result::Result<Vec<models::VectorRecord>, Box<dyn StdError + Send + Sync>>;
}

fn row_to_collection(row: &Row) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
    let schema_str: Option<String> = row.get(2)?;
    let schema = match schema_str { Some(s) => serde_json::from_str(&s)?, None => None, };
    Ok(Collection { id: row.get(0)?, name: row.get(1)?, schema })
}

fn row_to_record(
    row: &Row,
) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
    let id = row.get(0)?;
    
    let val_data = row.get_value(1).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    let data: serde_json::Value = match val_data {
        libsql::Value::Text(s) => serde_json::from_str(&s)?,
        libsql::Value::Blob(b) => serde_json::from_slice(&b).unwrap_or(serde_json::json!({})),
        _ => serde_json::json!({}),
    };

    let expand = if let Ok(val_expand) = row.get_value(2) {
        match val_expand {
            libsql::Value::Text(s) => Some(serde_json::from_str(&s)?),
            libsql::Value::Blob(b) => Some(serde_json::from_slice(&b).unwrap_or(serde_json::json!({}))),
            _ => None,
        }
    } else {
        None
    };

    let created: String = row.get(3).unwrap_or_else(|_| "".to_string());
    let updated: String = row.get(4).unwrap_or_else(|_| "".to_string());

    Ok(Record { id, data, expand, created, updated })
}

// --- The Orchestrator: ApexKit ---
#[derive(Clone)] 
pub struct ApexKit {
    // Sharded Databases (Wrapped in Arc for Clone)
    // We suppress dead code warning as these Arcs are kept for reference/maintenance
    #[allow(dead_code)]
    core_db: Arc<Database>,  
    #[allow(dead_code)]
    data_db: Arc<Database>,  
    #[allow(dead_code)]
    log_db: Arc<Database>,   
    #[allow(dead_code)]
    sys_db: Arc<Database>,  
    #[allow(dead_code)]
    vector_db: Arc<Database>,  

    // Persistent Connections (The Monster Fix)
    hot_conn_core: Arc<Mutex<Connection>>,
    hot_conn_data: Arc<Mutex<Connection>>,
    hot_conn_log: Arc<Mutex<Connection>>,
    hot_conn_sys: Arc<Mutex<Connection>>,
    hot_conn_vec: Arc<Mutex<Connection>>,
    
    // Batchers
    data_batcher: batching::WriteManager,
    log_batcher: batching::WriteManager,
    vector_batcher: batching::WriteManager,

    search: Arc<SearchManager>,
    pub embedder: Arc<embeddings::EmbedderService>,
    pub vector_provider: Arc<dyn VectorProvider>, 
}

struct ExpandableItem<'a> {
    data: &'a Value,
    expand: &'a mut Option<Value>,
}

// --- Constructor ---
pub async fn a_new_database_connection(
    vector_provider: Arc<dyn VectorProvider>
) -> std::result::Result<ApexKit, Box<dyn std::error::Error + Send + Sync>> {
    
    // Ensure the new centralized directory structure exists
    let base_dirs = vec![
        "storage/system",
        "storage/system/uploads",
        "storage/system/indexes",
        "storage/tenants",
        "storage/sandboxes",
        "storage/tmp"
    ];

    for dir in base_dirs {
        std::fs::create_dir_all(dir).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    }

    // Connect to DBs (Errors here are libsql::Error, need boxing)
    let core = Builder::new_local("storage/system/core.db").build().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    let data = Builder::new_local("storage/system/data.db").build().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    let log = Builder::new_local("storage/system/logs.db").build().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    let sys = Builder::new_local("storage/system/system.db").build().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    let vec = Builder::new_local("storage/system/vectors.db").build().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Helper to map errors for async blocks below
    let map_err = |e: libsql::Error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) };

    apply_pragmas(&core).await.map_err(map_err)?;
    apply_pragmas(&data).await.map_err(map_err)?;
    apply_pragmas(&log).await.map_err(map_err)?;
    apply_pragmas(&sys).await.map_err(map_err)?;
    apply_pragmas(&vec).await.map_err(map_err)?;

    setup_core(&core).await.map_err(map_err)?;
    setup_data(&data).await.map_err(map_err)?;
    setup_logs(&log).await.map_err(map_err)?;
    setup_sys(&sys).await.map_err(map_err)?;
    setup_vectors(&vec).await.map_err(map_err)?;

    let mut instance = ApexKit::new(
        Arc::new(core), 
        Arc::new(data), 
        Arc::new(log), 
        Arc::new(sys), 
        Arc::new(vec), 
        vector_provider
    );
    
    // Apply Busy Timeout to Hot Connections
    instance.get_core_read().await.execute_batch("PRAGMA busy_timeout = 5000;").await.map_err(map_err)?;
    instance.get_data_read().await.execute_batch("PRAGMA busy_timeout = 5000;").await.map_err(map_err)?;
    instance.get_log_read().await.execute_batch("PRAGMA busy_timeout = 5000;").await.map_err(map_err)?;
    instance.get_sys_read().await.execute_batch("PRAGMA busy_timeout = 5000;").await.map_err(map_err)?;
    instance.get_vector_read().await.execute_batch("PRAGMA busy_timeout = 5000;").await.map_err(map_err)?;

    instance.set_search_manager(Arc::new(SearchManager::new("storage/system/indexes")));

    Ok(instance)
}

impl ApexKit {
    pub fn new(core: Arc<Database>, data: Arc<Database>, log: Arc<Database>, sys: Arc<Database>, vec: Arc<Database>, vector_provider: Arc<dyn VectorProvider>) -> Self {
        let data_batcher = batching::WriteManager::new(data.clone());
        let log_batcher = batching::WriteManager::new(log.clone());
        let vector_batcher = batching::WriteManager::new(vec.clone());

        // Open Hot Connections immediately
        let open_hot = |db: &Arc<Database>| -> Arc<Mutex<Connection>> {
            Arc::new(Mutex::new(db.connect().expect("Failed to open hot connection")))
        };

        Self {
            core_db: core.clone(),
            data_db: data.clone(),
            log_db: log.clone(),
            sys_db: sys.clone(),
            vector_db: vec.clone(),
            
            hot_conn_core: open_hot(&core),
            hot_conn_data: open_hot(&data),
            hot_conn_log: open_hot(&log),
            hot_conn_sys: open_hot(&sys),
            hot_conn_vec: open_hot(&vec),

            vector_provider,
            data_batcher,
            log_batcher,
            vector_batcher,
            search: Arc::new(SearchManager::new("./storage/system/indexes")),
            embedder: Arc::new(embeddings::EmbedderService::new()), 
        }
    }

    pub async fn init_filesystem(
        base_path: &str, 
        vector_provider: Arc<dyn VectorProvider>
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        
        if !Path::new(base_path).exists() {
            std::fs::create_dir_all(base_path)?;
        }

        let core = Builder::new_local(&format!("{}/core.db", base_path)).build().await?;
        let data = Builder::new_local(&format!("{}/data.db", base_path)).build().await?;
        let log = Builder::new_local(&format!("{}/logs.db", base_path)).build().await?;
        let sys = Builder::new_local(&format!("{}/system.db", base_path)).build().await?;
        let vec = Builder::new_local(&format!("{}/vectors.db", base_path)).build().await?;

        apply_pragmas(&core).await?;
        apply_pragmas(&data).await?;
        apply_pragmas(&log).await?;
        apply_pragmas(&sys).await?;
        apply_pragmas(&vec).await?;

        setup_core(&core).await?;
        setup_data(&data).await?;
        setup_logs(&log).await?;
        setup_sys(&sys).await?;
        setup_vectors(&vec).await?;

        let instance = Self::new(
            Arc::new(core),
            Arc::new(data),
            Arc::new(log),
            Arc::new(sys),
            Arc::new(vec),
            vector_provider
        );

        // Apply Busy Timeout to Hot Connections
        instance.get_core_read().await.execute_batch("PRAGMA busy_timeout = 5000;").await?;
        instance.get_data_read().await.execute_batch("PRAGMA busy_timeout = 5000;").await?;
        instance.get_log_read().await.execute_batch("PRAGMA busy_timeout = 5000;").await?;
        instance.get_sys_read().await.execute_batch("PRAGMA busy_timeout = 5000;").await?;
        instance.get_vector_read().await.execute_batch("PRAGMA busy_timeout = 5000;").await?;

        Ok(instance)
    }

    pub fn set_search_manager(&mut self, manager: Arc<SearchManager>) {
        self.search = manager;
    }

    // Helper: Get persistent connection
    async fn get_core_read<'a>(&'a self) -> tokio::sync::MutexGuard<'a, Connection> { self.hot_conn_core.lock().await }
    async fn get_data_read<'a>(&'a self) -> tokio::sync::MutexGuard<'a, Connection> { self.hot_conn_data.lock().await }
    async fn get_log_read<'a>(&'a self) -> tokio::sync::MutexGuard<'a, Connection> { self.hot_conn_log.lock().await }
    async fn get_sys_read<'a>(&'a self) -> tokio::sync::MutexGuard<'a, Connection> { self.hot_conn_sys.lock().await }
    async fn get_vector_read<'a>(&'a self) -> tokio::sync::MutexGuard<'a, Connection> { self.hot_conn_vec.lock().await }
    
    
    async fn ensure_search_index(&self, collection_id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(col) = self.get_collection(collection_id).await? {
            if let Some(schema) = &col.schema {
                if schema.fields.values().any(|f| f.ose_indexed) {
                    self.search.load_index(collection_id, schema)?;
                }
            }
        }
        Ok(())
    }

    // [FIXED] Use the `conn` passed in argument instead of acquiring a new lock
    async fn sync_relations(&self, conn: &Connection, collection_id: i64, record_id: i64, data: &Value) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // REMOVED: let read_conn = self.get_data_read().await; <-- Caused Deadlock
        // USE: conn (passed from create_record/update_record)
        
        let mut rows = conn.query("SELECT schema FROM collections WHERE id = ?1", params![collection_id]).await?;
        
        if let Some(row) = rows.next().await? {
            let schema_str: Option<String> = row.get(0)?;
            if let Some(s) = schema_str {
                let schema: CollectionSchema = serde_json::from_str(&s).unwrap_or_default();

                for (rel_name, rel_def) in schema.relations {
                    if let Some(val) = data.get(&rel_name) {
                        self.data_batcher.execute(
                            "DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND rel_name=?3".into(),
                            vec![collection_id.into(), record_id.into(), rel_name.clone().into()]
                        ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;

                        let target_rec_id = match val {
                            Value::String(s) => s.parse::<i64>().unwrap_or(0),
                            Value::Number(n) => n.as_i64().unwrap_or(0),
                            _ => 0
                        };
                        if target_rec_id == 0 { continue; }

                        let identifier = &rel_def.target_collection;
                        let mut target_col_id: Option<i64> = None;
                        
                        // Reuse `conn` for lookups
                        let mut name_rows = conn.query("SELECT id FROM collections WHERE name = ?1", params![identifier.clone()]).await?;
                        if let Some(r) = name_rows.next().await? { target_col_id = Some(r.get(0)?); } 
                        else if let Ok(id_num) = identifier.parse::<i64>() {
                             let mut id_rows = conn.query("SELECT id FROM collections WHERE id = ?1", params![id_num]).await?;
                             if let Some(_) = id_rows.next().await? { target_col_id = Some(id_num); }
                        }

                        if let Some(tc_id) = target_col_id {
                            self.data_batcher.execute(
                                "INSERT INTO _relations (origin_col_id, origin_rec_id, target_col_id, target_rec_id, rel_name) VALUES (?1, ?2, ?3, ?4, ?5)".into(),
                                vec![collection_id.into(), record_id.into(), tc_id.into(), target_rec_id.into(), rel_name.into()]
                            ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn populate_owners_in_memory(
        &self,
        records: &mut [Record],
        collection_id: i64,
        expand_opt: Option<&String>
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        
        let expand_str = match expand_opt {
            Some(s) if !s.trim().is_empty() => s,
            _ => return Ok(()),
        };

        let tree = crate::query::build_expand_tree(expand_str);

        let mut root_items: Vec<ExpandableItem> = records.iter_mut()
            .map(|r| ExpandableItem { data: &r.data, expand: &mut r.expand })
            .collect();

        self.hydrate_owners_recursive(&mut root_items, collection_id, &tree).await?;

        Ok(())
    }

    fn hydrate_owners_recursive<'a>(
        &'a self,
        items: &'a mut Vec<ExpandableItem<'a>>, 
        collection_id: i64,
        tree: &'a HashMap<String, Vec<String>>
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'a>> {
        Box::pin(async move {
            if items.is_empty() || tree.is_empty() { return Ok(()); }

            let col = match self.get_collection(collection_id).await? {
                Some(c) => c,
                None => return Ok(()),
            };
            let schema = col.schema.unwrap_or_default();
            
            let mut owner_fields = Vec::new();
            let mut relation_fields = Vec::new(); 

            let all_collections = self.list_collections().await?;
            let col_map: HashMap<String, i64> = all_collections.iter().map(|c| (c.name.clone(), c.id)).collect();
            let id_map: HashMap<String, i64> = all_collections.iter().map(|c| (c.id.to_string(), c.id)).collect();

            for (field_name, sub_paths) in tree {
                if let Some(def) = schema.fields.get(field_name) {
                    if def.r#type == FieldType::Owner {
                        owner_fields.push(field_name);
                    }
                }
                
                let mut target_col_id = None;
                
                if let Some(rel_def) = schema.relations.get(field_name) {
                    let target = &rel_def.target_collection;
                    target_col_id = col_map.get(target).or_else(|| id_map.get(target)).cloned();
                } else if let Some(target_id) = col_map.get(field_name) {
                    target_col_id = Some(*target_id);
                }

                if let Some(tid) = target_col_id {
                    relation_fields.push((field_name, tid, sub_paths));
                }
            }

            if !owner_fields.is_empty() {
                for item in items.iter_mut() {
                    if item.expand.is_none() {
                        *item.expand = Some(serde_json::json!({}));
                    }
                }

                let mut user_ids = std::collections::HashSet::new();
                for item in items.iter() {
                    if let Some(obj) = item.data.as_object() {
                        for field in &owner_fields {
                            if let Some(val) = obj.get(*field) {
                                if let Some(uid) = val.as_i64() { user_ids.insert(uid); }
                                else if let Some(s) = val.as_str() { if let Ok(uid) = s.parse::<i64>() { user_ids.insert(uid); } }
                            }
                        }
                    }
                }

                if !user_ids.is_empty() {
                    let ids_vec: Vec<i64> = user_ids.into_iter().collect();
                    let users = self.get_users_by_ids(&ids_vec).await?;
                    let user_map: HashMap<i64, User> = users.into_iter().map(|u| (u.id, u)).collect();

                    for item in items.iter_mut() {
                        let mut updates = Vec::new();
                        if let Some(obj) = item.data.as_object() {
                            for field in &owner_fields {
                                if let Some(val) = obj.get(*field) {
                                    let uid_opt = val.as_i64().or_else(|| val.as_str().and_then(|s| s.parse::<i64>().ok()));
                                    if let Some(uid) = uid_opt {
                                        if let Some(user) = user_map.get(&uid) {
                                            updates.push(((*field).clone(), serde_json::json!({
                                                "id": user.id,
                                                "email": user.email,
                                                "role": user.role,
                                                "metadata": user.metadata
                                            })));
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(expand_obj) = item.expand.as_mut().and_then(|v| v.as_object_mut()) {
                            for (f, v) in updates { expand_obj.insert(f, v); }
                        }
                    }
                }
            }

            for (rel_name, target_id, sub_paths_list) in relation_fields {
                let sub_tree = crate::query::build_expand_tree_from_list(sub_paths_list);
                if sub_tree.is_empty() { continue; }

                for item in items.iter_mut() {
                    if let Some(expand_val) = item.expand {
                        if let Some(rel_val) = expand_val.get_mut(rel_name) {
                            if let Some(arr) = rel_val.as_array_mut() {
                                self.hydrate_json_values_recursive(arr, target_id, &sub_tree).await?;
                            } else if rel_val.is_object() {
                                let slice = std::slice::from_mut(rel_val);
                                self.hydrate_json_values_recursive(slice, target_id, &sub_tree).await?;
                            }
                        }
                    }
                }
            }

            Ok(())
        })
    }

    fn hydrate_json_values_recursive<'a>(
        &'a self,
        json_records: &'a mut [Value], 
        collection_id: i64,
        tree: &'a HashMap<String, Vec<String>>
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'a>> {
        Box::pin(async move {
            if json_records.is_empty() { return Ok(()); }
            
            let col = match self.get_collection(collection_id).await? { Some(c) => c, None => return Ok(()), };
            let schema = col.schema.unwrap_or_default();

            let mut owner_fields = Vec::new();
            let mut relation_fields = Vec::new();
            let all_collections = self.list_collections().await?;
            let col_map: HashMap<String, i64> = all_collections.iter().map(|c| (c.name.clone(), c.id)).collect();
            let id_map: HashMap<String, i64> = all_collections.iter().map(|c| (c.id.to_string(), c.id)).collect();

            for (field_name, sub_paths) in tree {
                if let Some(def) = schema.fields.get(field_name) {
                    if def.r#type == FieldType::Owner { owner_fields.push(field_name); }
                }
                let mut target_col_id = None;
                if let Some(rel_def) = schema.relations.get(field_name) {
                    target_col_id = col_map.get(&rel_def.target_collection).or_else(|| id_map.get(&rel_def.target_collection)).cloned();
                } else if let Some(target_id) = col_map.get(field_name) {
                    target_col_id = Some(*target_id);
                }
                if let Some(tid) = target_col_id { relation_fields.push((field_name, tid, sub_paths)); }
            }

            if !owner_fields.is_empty() {
                let mut user_ids = std::collections::HashSet::new();
                for rec in json_records.iter() {
                    if let Some(data) = rec.get("data") {
                        for field in &owner_fields {
                            if let Some(val) = data.get(*field) {
                                if let Some(uid) = val.as_i64() { user_ids.insert(uid); }
                                else if let Some(s) = val.as_str() { if let Ok(uid) = s.parse::<i64>() { user_ids.insert(uid); } }
                            }
                        }
                    }
                }
                
                if !user_ids.is_empty() {
                    let ids_vec: Vec<i64> = user_ids.into_iter().collect();
                    let users = self.get_users_by_ids(&ids_vec).await?;
                    let user_map: HashMap<i64, User> = users.into_iter().map(|u| (u.id, u)).collect();

                    for rec in json_records.iter_mut() {
                        let mut updates = Vec::new();
                         if let Some(data) = rec.get("data") {
                            for field in &owner_fields {
                                if let Some(val) = data.get(*field) {
                                     let uid_opt = val.as_i64().or_else(|| val.as_str().and_then(|s| s.parse::<i64>().ok()));
                                     if let Some(uid) = uid_opt {
                                        if let Some(user) = user_map.get(&uid) {
                                            updates.push(((*field).clone(), serde_json::json!({ 
                                                "id": user.id, 
                                                "email": user.email, 
                                                "role": user.role,
                                                "metadata": user.metadata
                                            })));
                                        }
                                     }
                                }
                            }
                         }
                         
                         if !updates.is_empty() {
                             if rec.get("expand").is_none() || rec.get("expand").unwrap().is_null() {
                                 if let Some(obj) = rec.as_object_mut() {
                                     obj.insert("expand".to_string(), serde_json::json!({}));
                                 }
                             }
                             if let Some(expand) = rec.get_mut("expand").and_then(|v| v.as_object_mut()) {
                                 for (f, v) in updates { expand.insert(f, v); }
                             }
                         }
                    }
                }
            }

            for (rel_name, target_id, sub_paths_list) in relation_fields {
                 let sub_tree = crate::query::build_expand_tree_from_list(sub_paths_list);
                 if sub_tree.is_empty() { continue; }
                 
                 for rec in json_records.iter_mut() {
                     if let Some(expand) = rec.get_mut("expand") {
                         if let Some(rel_val) = expand.get_mut(rel_name) {
                            if let Some(arr) = rel_val.as_array_mut() {
                                self.hydrate_json_values_recursive(arr, target_id, &sub_tree).await?;
                            } else if rel_val.is_object() {
                                let slice = std::slice::from_mut(rel_val);
                                self.hydrate_json_values_recursive(slice, target_id, &sub_tree).await?;
                            }
                         }
                     }
                 }
            }

            Ok(())
        })
    }
}


// --- UNIQUENESS LOGIC ---
fn serialize_unique_val(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => val.to_string()
    }
}

async fn check_conflict(conn: &Connection, key: &str, val: &str, current_rec_id: Option<i64>, err_context: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut rows = conn.query("SELECT record_id FROM _unique_values WHERE index_key = ?1 AND value = ?2", params![key.to_string(), val.to_string()]).await?;
    if let Some(row) = rows.next().await? {
        let existing_id: i64 = row.get(0)?;
        if Some(existing_id) != current_rec_id {
            let msg = format!("Unique constraint violation: {} with value '{}' already exists.", err_context, val);
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::AlreadyExists, msg)));
        }
    }
    Ok(())
}

async fn enforce_uniqueness(conn: &Connection, col_id: i64, record_id: Option<i64>, data: &Value, schema: &CollectionSchema) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Check Standard Fields
    for (name, def) in &schema.fields {
        if def.unique.unwrap_or(false) {
            if let Some(val) = data.get(name) {
                if val.is_null() { continue; }
                let val_str = serialize_unique_val(val);
                let index_key = format!("{}-{}", col_id, def.uid);
                check_conflict(conn, &index_key, &val_str, record_id, &format!("Field '{}'", name)).await?;
            }
        }
    }

    // 2. Check Relations (One-to-One)
    for (name, def) in &schema.relations {
        if def.relation_type == crate::schema::RelationType::One {
            if let Some(val) = data.get(name) {
                if val.is_null() { continue; }
                let val_str = serialize_unique_val(val);
                let index_key = format!("{}-{}", col_id, def.uid);
                check_conflict(conn, &index_key, &val_str, record_id, &format!("Relation '{}'", name)).await?;
            }
        }
    }

    // 3. Check Composite Keys
    for field_group in &schema.composite_unique {
        let mut composite_uids = Vec::new();
        let mut composite_values = Vec::new();
        let mut missing_data = false;

        for field_name in field_group {
            let uid_opt = if let Some(def) = schema.fields.get(field_name) {
                Some(def.uid.clone())
            } else if let Some(rel) = schema.relations.get(field_name) {
                Some(rel.uid.clone())
            } else {
                None
            };

            if let Some(uid) = uid_opt {
                composite_uids.push(uid);
                if let Some(val) = data.get(field_name) {
                    if val.is_null() { missing_data = true; break; }
                    composite_values.push(serialize_unique_val(val));
                } else { 
                    missing_data = true; 
                    break; 
                }
            } else {
                missing_data = true;
                break;
            }
        }
        
        if missing_data { continue; }
        
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        for (i, uid) in composite_uids.iter().enumerate() { map.insert(uid.clone(), composite_values[i].clone()); }
        let sorted_uids: Vec<&String> = map.keys().collect();
        let sorted_vals: Vec<&String> = map.values().collect();
        let index_key = format!("{}-{}", col_id, sorted_uids.iter().map(|s| s.as_str()).collect::<Vec<&str>>().join("-"));
        let value_str = sorted_vals.iter().map(|s| s.as_str()).collect::<Vec<&str>>().join(COMPOSITE_SEPARATOR);
        check_conflict(conn, &index_key, &value_str, record_id, &format!("Combination {:?}", field_group)).await?;
    }
    Ok(())
}

async fn commit_uniqueness(
    batcher: &batching::WriteManager, 
    col_id: i64, 
    record_id: i64, 
    data: &Value, 
    schema: &CollectionSchema
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    
    batcher.execute(
        "DELETE FROM _unique_values WHERE record_id = ?1".into(), 
        vec![record_id.into()]
    ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;

    for (name, def) in &schema.fields {
        if def.unique.unwrap_or(false) {
            if let Some(val) = data.get(name) {
                if !val.is_null() {
                    let index_key = format!("{}-{}", col_id, def.uid);
                    let val_str = serialize_unique_val(val);
                    batcher.execute(
                        "INSERT INTO _unique_values (index_key, value, record_id) VALUES (?1, ?2, ?3)".into(), 
                        vec![index_key.into(), val_str.into(), record_id.into()]
                    ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
                }
            }
        }
    }

    for (name, def) in &schema.relations {
        if def.relation_type == crate::schema::RelationType::One {
            if let Some(val) = data.get(name) {
                if !val.is_null() {
                    let index_key = format!("{}-{}", col_id, def.uid);
                    let val_str = serialize_unique_val(val);
                    batcher.execute(
                        "INSERT INTO _unique_values (index_key, value, record_id) VALUES (?1, ?2, ?3)".into(), 
                        vec![index_key.into(), val_str.into(), record_id.into()]
                    ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
                }
            }
        }
    }

    for field_group in &schema.composite_unique {
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        let mut missing = false;

        for field_name in field_group {
            let uid_opt = if let Some(def) = schema.fields.get(field_name) {
                Some(def.uid.clone())
            } else if let Some(rel) = schema.relations.get(field_name) {
                Some(rel.uid.clone())
            } else {
                None
            };

            if let Some(uid) = uid_opt {
                if let Some(val) = data.get(field_name) {
                    if val.is_null() { missing = true; break; }
                    map.insert(uid, serialize_unique_val(val));
                } else { 
                    missing = true; 
                    break; 
                }
            } else {
                missing = true;
                break;
            }
        }
        
        if !missing {
            let sorted_uids: Vec<&String> = map.keys().collect();
            let sorted_vals: Vec<&String> = map.values().collect();
            
            let index_key = format!("{}-{}", col_id, sorted_uids.iter().map(|s| s.as_str()).collect::<Vec<&str>>().join("-"));
            let value_str = sorted_vals.iter().map(|s| s.as_str()).collect::<Vec<&str>>().join(COMPOSITE_SEPARATOR);
            
            batcher.execute(
                "INSERT INTO _unique_values (index_key, value, record_id) VALUES (?1, ?2, ?3)".into(), 
                vec![index_key.into(), value_str.into(), record_id.into()]
            ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        }
    }
    Ok(())
}

fn sql_index_name(col_id: i64, field_uid: &str) -> String {
    format!("idx_col_{}_{}", col_id, field_uid)
}

async fn reconcile_sql_indexes(
    batcher: &batching::WriteManager,
    col_id: i64,
    new_schema: &CollectionSchema,
    old_schema: Option<&CollectionSchema>
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    
    for (name, def) in &new_schema.fields {
        if def.sql_indexed {
            let idx_name = sql_index_name(col_id, &def.uid);
            
            let exists = old_schema.map(|s| {
                s.fields.get(name).map(|f| f.sql_indexed).unwrap_or(false)
            }).unwrap_or(false);

            if !exists {
                let sql = format!(
                    "CREATE INDEX IF NOT EXISTS {} ON records (json_extract(data, '$.{}')) WHERE collection_id = {}", 
                    idx_name, name, col_id
                );
                batcher.execute(sql, vec![]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
            }
        }
    }

    if let Some(old) = old_schema {
        for (name, def) in &old.fields {
            let should_drop = if let Some(new_def) = new_schema.fields.get(name) {
                def.sql_indexed && !new_def.sql_indexed 
            } else {
                def.sql_indexed
            };

            if should_drop {
                let idx_name = sql_index_name(col_id, &def.uid);
                let sql = format!("DROP INDEX IF EXISTS {}", idx_name);
                batcher.execute(sql, vec![]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
            }
        }
    }

    Ok(())
}

#[async_trait]
impl Db for ApexKit {
    // --- API keys / Super tokens ---
    async fn create_api_key(&self, name: &str, role: &str, scope: &str, bypass_cors: bool) -> std::result::Result<(String, ApiKey), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let raw_key = format!("ak_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        let prefix = raw_key[0..8].to_string();
        let hash = crate::auth::hash_password(&raw_key)?;

        conn.execute(
            "INSERT INTO _api_keys (name, prefix, hash, role, scope, bypass_cors) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", 
            params![name, prefix.clone(), hash, role, scope, bypass_cors]
        ).await?;
        
        let id = conn.last_insert_rowid();
        
        let key_obj = ApiKey {
            id,
            name: name.to_string(),
            prefix,
            hash: String::new(),
            role: role.to_string(),
            scope: scope.to_string(),
            bypass_cors,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        Ok((raw_key, key_obj))
    }

    async fn list_api_keys(&self) -> std::result::Result<Vec<ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut rows = conn.query("SELECT id, name, prefix, hash, role, created_at, scope, bypass_cors FROM _api_keys ORDER BY created_at DESC", ()).await?;
        let mut keys = Vec::new();
        while let Some(row) = rows.next().await? {
            keys.push(ApiKey {
                id: row.get(0)?,
                name: row.get(1)?,
                prefix: row.get(2)?,
                hash: String::new(), 
                role: row.get(4)?,
                created_at: row.get(5)?,
                scope: row.get(6).unwrap_or("root".to_string()),
                bypass_cors: row.get(7).unwrap_or(false),
            });
        }
        Ok(keys)
    }

    async fn delete_api_key(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.get_core_read().await.execute("DELETE FROM _api_keys WHERE id = ?1", params![id]).await?;
        Ok(())
    }

    async fn verify_api_key(&self, key: &str) -> std::result::Result<Option<ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let prefix = if key.len() > 8 { &key[0..8] } else { key };
        
        let mut rows = conn.query("SELECT id, name, prefix, hash, role, created_at, scope, bypass_cors FROM _api_keys WHERE prefix = ?1", params![prefix]).await?;
        
        while let Some(row) = rows.next().await? {
            let hash: String = row.get(3)?;
            if crate::auth::verify_password(key, &hash) {
                return Ok(Some(ApiKey {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    prefix: row.get(2)?,
                    hash: String::new(),
                    role: row.get(4)?,
                    created_at: row.get(5)?,
                    scope: row.get(6).unwrap_or("root".to_string()),
                    bypass_cors: row.get(7).unwrap_or(false),
                }));
            }
        }
        Ok(None)
    }
    
    // --- Data DB (Collections/Records) ---

    async fn create_collection(&self, name: &str, schema: &Option<CollectionSchema>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let schema_str = serde_json::to_string(&schema)?;
        let id = self.data_batcher.insert("INSERT INTO collections (name, schema) VALUES (?1, ?2)".into(), vec![name.into(), schema_str.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        
        if let Some(s) = schema {
            if s.fields.values().any(|f| f.ose_indexed) { 
                self.search.load_index(id, s)?; 
            }
            reconcile_sql_indexes(&self.data_batcher, id, s, None).await?;
        }
        Ok(id)
    }

    async fn update_collection(&self, id: i64, name: Option<String>, schema: Option<CollectionSchema>) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
        let existing = self.get_collection(id).await?.ok_or("Not found")?;
        let old_schema = existing.schema;

        if let Some(n) = name { self.data_batcher.execute("UPDATE collections SET name = ?1 WHERE id = ?2".into(), vec![n.into(), id.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?; }
        
        if let Some(s) = &schema { 
            let s_str = serde_json::to_string(&s)?; 
            self.data_batcher.execute("UPDATE collections SET schema = ?1 WHERE id = ?2".into(), vec![s_str.into(), id.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?; 
            
            reconcile_sql_indexes(&self.data_batcher, id, s, old_schema.as_ref()).await?;
            
            if s.fields.values().any(|f| f.ose_indexed) {
                self.search.load_index(id, s)?;
            }
        }
        
        self.get_collection(id).await?.ok_or("Not found".into())
    }

    async fn get_collection(&self, id: i64) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut rows = conn.query("SELECT id, name, schema FROM collections WHERE id = ?1", params![id]).await?;
        match rows.next().await? { Some(row) => Ok(Some(row_to_collection(&row)?)), None => Ok(None) }
    }

    async fn list_collections(&self) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut rows = conn.query("SELECT id, name, schema FROM collections", ()).await?;
        let mut cols = Vec::new();
        while let Some(row) = rows.next().await? { cols.push(row_to_collection(&row)?); }
        Ok(cols)
    }

    async fn delete_collection(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
        };

        self.data_batcher.execute("DELETE FROM records WHERE collection_id = ?1".into(), vec![id.into()]).await.map_err(map_err)?;
        self.data_batcher.execute("DELETE FROM _relations WHERE origin_col_id = ?1".into(), vec![id.into()]).await.map_err(map_err)?;
        self.data_batcher.execute("DELETE FROM _relations WHERE target_col_id = ?1".into(), vec![id.into()]).await.map_err(map_err)?;
        self.data_batcher.execute("DELETE FROM collections WHERE id = ?1".into(), vec![id.into()]).await.map_err(map_err)?;
        let _ = self.search.delete_index(id);
        Ok(())
    }

    // --- Records ---

    // [OPTIMIZED] Bypass Mutex. Use fresh connection for READ checks. Writes go to batcher.
    async fn create_record(&self, collection_id: i64, data: &Value) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        // Transient connection for Reads (Schema check, Uniqueness check)
        let conn = self.data_db.connect()?; 
        
        let col = {
            let mut rows = conn.query("SELECT id, name, schema FROM collections WHERE id = ?1", params![collection_id]).await?;
            if let Some(row) = rows.next().await? {
                Some(row_to_collection(&row)?)
            } else { None }
        }.ok_or("Collection not found")?;

        let schema = col.schema.unwrap_or_default();
        let json_str = serde_json::to_string(data)?;

        // READ Check
        enforce_uniqueness(&conn, collection_id, None, data, &schema).await?;

        // WRITE (Async Batcher)
        let record_id = self.data_batcher.insert(
            "INSERT INTO records (collection_id, data) VALUES (?1, jsonb(?2))".into(),
            vec![collection_id.into(), json_str.into()]
        ).await?;
        
        // Writes
        let unique_future = commit_uniqueness(&self.data_batcher, collection_id, record_id, data, &schema);
        
        // READ + Write (using transient conn for reads)
        let relation_future = self.sync_relations(&conn, collection_id, record_id, data);

        tokio::try_join!(unique_future, relation_future)?;
        
        Ok(record_id)
    }

    // [OPTIMIZED] Update
    async fn update_record(&self, collection_id: i64, record_id: i64, data: &Value) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.data_db.connect()?;

        let col = {
            let mut rows = conn.query("SELECT id, name, schema FROM collections WHERE id = ?1", params![collection_id]).await?;
            if let Some(row) = rows.next().await? { Some(row_to_collection(&row)?) } else { None }
        }.ok_or("Col not found")?;

        let schema = col.schema.unwrap_or_default();

        let existing = {
            let mut rows = conn.query("SELECT id, json(data), NULL, created, updated FROM records WHERE collection_id = ?1 AND id = ?2", params![collection_id, record_id]).await?;
            if let Some(row) = rows.next().await? { Some(row_to_record(&row)?) } else { None }
        }.ok_or("Rec not found")?;
        
        let mut merged_data = existing.data.clone();
        if let Some(obj) = merged_data.as_object_mut() {
            if let Some(new_obj) = data.as_object() {
                for (k, v) in new_obj { obj.insert(k.clone(), v.clone()); }
            }
        }
        
        let json_str = serde_json::to_string(&merged_data)?;

        enforce_uniqueness(&conn, collection_id, Some(record_id), &merged_data, &schema).await?;

        self.data_batcher.execute(
            "UPDATE records SET data = jsonb(?1), updated = CURRENT_TIMESTAMP WHERE collection_id = ?2 AND id = ?3".into(), 
            vec![json_str.into(), collection_id.into(), record_id.into()]
        ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        
        let unique_future = commit_uniqueness(&self.data_batcher, collection_id, record_id, &merged_data, &schema);
        let relation_future = self.sync_relations(&conn, collection_id, record_id, &merged_data);
        
        tokio::try_join!(unique_future, relation_future)?;
        
        Ok(Record { 
            id: record_id, 
            data: merged_data, 
            expand: serde_json::json!({}).into(),
            created: existing.created,
            updated: chrono::Utc::now().to_rfc3339()
        })
    }

    // [OPTIMIZED] Import
    async fn import_record(&self, collection_id: i64, record_id: i64, data: &Value) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.data_db.connect()?; 
        
        let col = {
             let mut rows = conn.query("SELECT id, name, schema FROM collections WHERE id = ?1", params![collection_id]).await?;
             if let Some(row) = rows.next().await? { Some(row_to_collection(&row)?) } else { None }
        }.ok_or("Collection not found")?;

        let schema = col.schema.unwrap_or_default();
        let json_str = serde_json::to_string(data)?;

        enforce_uniqueness(&conn, collection_id, Some(record_id), data, &schema).await?;

        self.data_batcher.execute(
            "INSERT INTO records (id, collection_id, data) VALUES (?1, ?2, jsonb(?3))".into(),
            vec![record_id.into(), collection_id.into(), json_str.into()] 
        ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        
        let unique_future = commit_uniqueness(&self.data_batcher, collection_id, record_id, data, &schema);
        let relation_future = self.sync_relations(&conn, collection_id, record_id, data);
        tokio::try_join!(unique_future, relation_future)?;

        Ok(())
    }

    async fn delete_record(&self, collection_id: i64, record_id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
        };
        
        let f1 = self.data_batcher.execute(
            "DELETE FROM records WHERE collection_id = ?1 AND id = ?2".into(), 
            vec![collection_id.into(), record_id.into()]
        );
        let f2 = self.data_batcher.execute(
            "DELETE FROM _unique_values WHERE record_id = ?1".into(), 
            vec![record_id.into()]
        );
        let f3 = self.data_batcher.execute(
            "DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2".into(), 
            vec![collection_id.into(), record_id.into()]
        );
        let f4 = self.data_batcher.execute(
            "DELETE FROM _relations WHERE target_col_id=?1 AND target_rec_id=?2".into(), 
            vec![collection_id.into(), record_id.into()]
        );
        let f5 = self.vector_batcher.execute(
            "DELETE FROM vectors WHERE collection_id = ?1 AND record_id = ?2".into(),
            vec![collection_id.into(), record_id.into()]
        );

        let _ = tokio::try_join!(f1, f2, f3, f4).map_err(map_err)?;
        let _ = f5.await.map_err(map_err)?;

        Ok(())
    }

    // --- Implement New Trait Methods ---
    async fn index_record_search(&self, collection_id: i64, record_id: i64, data: &Value, schema: &CollectionSchema) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.search.index_record(collection_id, record_id, data, schema).map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn StdError + Send + Sync>)
    }

    async fn delete_record_search(&self, collection_id: i64, record_id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.search.delete_record(collection_id, record_id).map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn StdError + Send + Sync>)
    }

    // [FIXED] Scoped lock to prevent deadlock during populate_owners
    async fn list_records(&self, collection_id: i64, options: QueryOptions) -> std::result::Result<ListResult, Box<dyn std::error::Error + Send + Sync>> {
        let (mut records, total) = {
            let conn = self.get_data_read().await;
            
            let mut schema_map = HashMap::new();
            let mut id_map = HashMap::new();
            let mut current_col_name = String::new();

            // We need to fetch collection metadata. 
            // Since we hold the lock 'conn', we CANNOT call self.list_collections() or self.get_collection().
            // We must execute raw SQL on 'conn' here.
            
            // 1. Fetch current collection name
            {
                let mut rows = conn.query("SELECT name FROM collections WHERE id = ?1", params![collection_id]).await?;
                if let Some(row) = rows.next().await? {
                    current_col_name = row.get(0)?;
                } else {
                     return Ok(ListResult { items: vec![], total: 0 });
                }
            }

            // 2. Fetch schemas if expanding (Raw SQL)
            if let Some(ref ex) = options.expand {
                if !ex.trim().is_empty() {
                    let mut rows = conn.query("SELECT id, name, schema FROM collections", ()).await?;
                    while let Some(row) = rows.next().await? {
                        let id: i64 = row.get(0)?;
                        let name: String = row.get(1)?;
                        let schema_str: Option<String> = row.get(2)?;
                        let schema = match schema_str { Some(s) => serde_json::from_str(&s).unwrap_or_default(), None => CollectionSchema::default() };
                        
                        schema_map.insert(name.clone(), schema.clone());
                        schema_map.insert(id.to_string(), schema);
                        id_map.insert(name, id);
                        id_map.insert(id.to_string(), id);
                    }
                }
            }

            let builder = query::SqlBuilder::new(collection_id, &current_col_name, options.clone(), &schema_map, &id_map);
            
            let mut count_rows = conn.query(&builder.count_sql, builder.params.clone()).await?;
            let total = if let Some(row) = count_rows.next().await? { row.get::<i64>(0)? } else { 0 };

            let mut rows = conn.query(&builder.base_sql, builder.params).await?;
            let mut recs = Vec::new();
            while let Some(row) = rows.next().await? { 
                recs.push(row_to_record(&row)?); 
            }
            (recs, total)
        }; // Lock 'conn' is dropped here

        // Now safe to call populate_owners_in_memory which might acquire new locks internally
        self.populate_owners_in_memory(&mut records, collection_id, options.expand.as_ref()).await?;

        Ok(ListResult { items: records, total })
    }

    // [FIXED] Scoped lock to prevent deadlock
    async fn get_record(&self, collection_id: i64, record_id: i64, expand: Option<String>) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let mut record_opt = {
            let conn = self.get_data_read().await;
            
            if expand.is_none() || expand.as_ref().unwrap().trim().is_empty() {
                let mut rows = conn.query(
                    "SELECT id, json(data), NULL, created, updated FROM records WHERE collection_id = ?1 AND id = ?2", 
                    params![collection_id, record_id]
                ).await?;
                match rows.next().await? { Some(row) => Some(row_to_record(&row)?), None => None }
            } else {
                let expand_str = expand.clone().unwrap();
                
                // Raw SQL for metadata to avoid recursive lock
                let mut schema_map = HashMap::new(); 
                let mut id_map = HashMap::new(); 
                let mut current_col_name = String::new();

                // Get current name
                {
                    let mut rows = conn.query("SELECT name FROM collections WHERE id = ?1", params![collection_id]).await?;
                    if let Some(row) = rows.next().await? { current_col_name = row.get(0)?; }
                }

                // Get schemas
                {
                    let mut rows = conn.query("SELECT id, name, schema FROM collections", ()).await?;
                    while let Some(row) = rows.next().await? {
                        let id: i64 = row.get(0)?;
                        let name: String = row.get(1)?;
                        let schema_str: Option<String> = row.get(2)?;
                        let schema = match schema_str { Some(s) => serde_json::from_str(&s).unwrap_or_default(), None => CollectionSchema::default() };
                        schema_map.insert(name.clone(), schema.clone());
                        schema_map.insert(id.to_string(), schema);
                        id_map.insert(name, id);
                        id_map.insert(id.to_string(), id);
                    }
                }

                let paths = crate::query::smart_split(&expand_str);
                let expand_json_sql = crate::query::build_expand_json_object(
                    paths, "records", 0, &current_col_name, collection_id, &schema_map, &id_map
                );

                let sql = format!(
                    "SELECT id, json(data), {}, created, updated FROM records WHERE collection_id = ?1 AND id = ?2", 
                    expand_json_sql
                );
                
                let mut rows = conn.query(&sql, params![collection_id, record_id]).await?;
                match rows.next().await? { Some(row) => Some(row_to_record(&row)?), None => None }
            }
        }; // Lock 'conn' dropped

        if let Some(rec) = &mut record_opt {
             let mut single_vec = vec![rec.clone()];
             self.populate_owners_in_memory(&mut single_vec, collection_id, expand.as_ref()).await?;
             *rec = single_vec.pop().unwrap();
        }

        Ok(record_opt)
    }

    async fn search_records(&self, collection_id: i64, query: &str) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_search_index(collection_id).await?;
        
        // 1. Get IDs sorted by relevance score from Tantivy
        let ids = self.search.search(collection_id, query, 50)?;
        if ids.is_empty() { return Ok(vec![]); }
        
        // 2. Fetch raw records from DB (Returned in arbitrary order)
        let id_list = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, json(data), NULL, created, updated FROM records WHERE id IN ({})", id_list);
        let conn = self.get_data_read().await;
        let mut rows = conn.query(&sql, ()).await?;
        
        let mut records = Vec::new();
        while let Some(row) = rows.next().await? { 
            records.push(row_to_record(&row)?); 
        }

        // 3. Re-sort records to match the relevance order of 'ids'
        let id_pos: std::collections::HashMap<i64, usize> = ids.iter().enumerate().map(|(pos, id)| (*id, pos)).collect();
        
        records.sort_by(|a, b| {
            let pos_a = id_pos.get(&a.id).unwrap_or(&usize::MAX);
            let pos_b = id_pos.get(&b.id).unwrap_or(&usize::MAX);
            pos_a.cmp(pos_b)
        });

        Ok(records)
    }

    async fn recover_indexes(&self) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let collections = self.list_collections().await?;
        let available_cores = std::thread::available_parallelism()?.get();
        let max_concurrency = (available_cores / 2).max(1); 
        
        println!("[Recovery] Starting Index Recovery. Utilizing {}/{} cores.", max_concurrency, available_cores);

        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrency));
        let recovered_count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        for col in collections {
            let db = self.clone(); 
            let sem = semaphore.clone();
            let counter = recovered_count.clone();

            let handle = tokio::spawn(async move {
                // Wrap logic in an inner async block that returns Option<()>
                // This allows using '?' to exit early if DB queries fail
                let task_logic = async {
                    let _permit = sem.acquire().await.ok()?;
                    
                    // 1. Get DB Count
                    let db_count_res = {
                        let conn = db.get_data_read().await;
                        let stmt = conn.prepare("SELECT COUNT(*) FROM records WHERE collection_id = ?").await.ok()?;
                        let mut rows = stmt.query(params![col.id]).await.ok()?;
                        if let Some(row) = rows.next().await.ok()? {
                            Some(row.get::<i64>(0).unwrap_or(0) as u64)
                        } else { Some(0) }
                    };

                    let db_count = db_count_res.unwrap_or(0);
                    if db_count == 0 { return Some(()); } 

                    // 2. Get Index Count
                    if let Some(schema) = &col.schema {
                        if !schema.fields.values().any(|f| f.ose_indexed) { return Some(()); }
                        let _ = db.search.load_index(col.id, schema);
                    }
                    
                    let idx_count = db.search.get_doc_count(col.id).unwrap_or(0);

                    // 3. Compare & Re-index
                    if db_count != idx_count {
                        println!("[Recovery] Collection '{}' (ID: {}) mismatch. DB: {}, Index: {}. Re-indexing...", col.name, col.id, db_count, idx_count);
                        if let Err(e) = db.reindex_collection(col.id).await {
                            eprintln!("[Recovery] Failed to re-index collection {}: {}", col.id, e);
                        } else {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Some(())
                };

                // Run the inner logic
                task_logic.await;
            });
            handles.push(handle);
        }

        for h in handles {
            let _ = h.await;
        }

        let count = recovered_count.load(Ordering::Relaxed);
        if count > 0 {
            println!("[Recovery] Successfully recovered {} collections.", count);
        } else {
            println!("[Recovery] Indexes healthy.");
        }

        Ok(())
    }

    async fn reindex_collection(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let col = self.get_collection(id).await?.ok_or_else(|| format!("Collection {} not found", id))?; 
        let schema = col.schema.unwrap_or_default();
        
        if !schema.fields.values().any(|f| f.ose_indexed) { return Ok(()); }
        
        self.search.delete_index(id).map_err(|e| format!("Search Delete Error: {}", e))?; 
        self.search.load_index(id, &schema).map_err(|e| format!("Search Load Error: {}", e))?;
        
        let conn = self.get_data_read().await;
        let mut rows = conn.query("SELECT id, json(data), NULL, created, updated FROM records WHERE collection_id = ?1", params![id])
            .await
            .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;
            
        let mut buffer: Vec<(i64, serde_json::Value)> = Vec::with_capacity(1000);
        let batch_size = 1000;

        while let Some(row) = rows.next().await.map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)? {
            let record = row_to_record(&row)?;
            buffer.push((record.id, record.data));

            if buffer.len() >= batch_size {
                self.search.index_batch(id, &buffer, &schema).map_err(|e| format!("Indexing Error: {}", e))?;
                buffer.clear();
            }
        }

        if !buffer.is_empty() {
            self.search.index_batch(id, &buffer, &schema).map_err(|e| format!("Indexing Error: {}", e))?;
        }

        Ok(())
    }

    async fn instant_search(&self, collection_id: i64, query: &str, limit: usize) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(col) = self.get_collection(collection_id).await? {
            if let Some(schema) = &col.schema {
                if schema.fields.values().any(|f| f.ose_indexed) { self.search.load_index(collection_id, schema)?; } else { return Ok(vec![]); }
            }
        }
        let results = self.search.instant_search(collection_id, query, limit.try_into().unwrap())?;
        Ok(results)
    }

    // --- Core DB (Users, Auth, Settings) ---
    async fn create_user(&self, e: &str, p: &str, r: &str, metadata: Option<Value>) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let meta = metadata.unwrap_or(json!({}));
        let meta_str = serde_json::to_string(&meta)?;
        conn.execute("INSERT INTO users (email, password_hash, role, metadata) VALUES (?1, ?2, ?3, ?4)", params![e, p, r, meta_str]).await?;
        Ok(User { 
            id: conn.last_insert_rowid(), 
            email: e.into(), 
            password_hash: p.into(), 
            role: r.into(), 
            metadata: Some(meta) 
        })
    }

    // Import User (Forced ID)
    async fn import_user(&self, id: i64, e: &str, p: &str, r: &str, metadata: Option<Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let meta = metadata.unwrap_or(json!({}));
        let meta_str = serde_json::to_string(&meta)?;
        
        // Explicitly set ID
        conn.execute(
            "INSERT INTO users (id, email, password_hash, role, metadata) VALUES (?1, ?2, ?3, ?4, ?5)", 
            params![id, e, p, r, meta_str]
        ).await?;
        Ok(())
    }

    async fn get_user_by_email(&self, email: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut r = conn.query("SELECT id, email, password_hash, role, metadata FROM users WHERE email = ?1", params![email]).await?;
        if let Some(row) = r.next().await? { 
            let meta_str: String = row.get(4).unwrap_or("{}".to_string());
            Ok(Some(User { 
                id: row.get(0)?, 
                email: row.get(1)?, 
                password_hash: row.get(2)?, 
                role: row.get(3)?,
                metadata: serde_json::from_str(&meta_str).ok()
            })) 
        } else { Ok(None) }
    }

    async fn list_users(&self, query: Option<String>, limit: i64, offset: i64) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let sql = if let Some(q) = query {
            format!("SELECT id, email, password_hash, role, metadata FROM users WHERE email LIKE '%{}%' ORDER BY id DESC LIMIT {} OFFSET {}", q, limit, offset)
        } else {
            format!("SELECT id, email, password_hash, role, metadata FROM users ORDER BY id DESC LIMIT {} OFFSET {}", limit, offset)
        };
        
        let mut rows = conn.query(&sql, ()).await?;
        let mut users = Vec::new();
        while let Some(row) = rows.next().await? { 
            let meta_str: String = row.get(4).unwrap_or("{}".to_string());
            users.push(User { 
                id: row.get(0)?, 
                email: row.get(1)?, 
                password_hash: row.get(2)?, 
                role: row.get(3)?,
                metadata: serde_json::from_str(&meta_str).ok()
            }); 
        }
        Ok(users)
    }

    async fn count_users(&self, query: Option<String>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let sql = if let Some(q) = query {
            format!("SELECT COUNT(*) FROM users WHERE email LIKE '%{}%'", q)
        } else {
            "SELECT COUNT(*) FROM users".to_string()
        };
        let mut row = conn.query(&sql, ()).await?;
        if let Some(r) = row.next().await? { Ok(r.get(0)?) } else { Ok(0) }
    }

    async fn get_users_by_ids(&self, ids: &[i64]) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> {
        if ids.is_empty() { return Ok(vec![]); }
        let id_list = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, email, password_hash, role, metadata FROM users WHERE id IN ({})", id_list);
        
        let conn = self.get_core_read().await;
        let mut rows = conn.query(&sql, ()).await?;
        let mut users = Vec::new();
        while let Some(row) = rows.next().await? { 
            let meta_str: String = row.get(4).unwrap_or("{}".to_string());
            users.push(User { 
                id: row.get(0)?, 
                email: row.get(1)?, 
                password_hash: row.get(2)?, 
                role: row.get(3)?,
                metadata: serde_json::from_str(&meta_str).ok()
            }); 
        }
        Ok(users)
    }

    async fn delete_user(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_core_read().await.execute("DELETE FROM users WHERE id = ?1", params![id]).await?; Ok(()) }
    async fn get_user_by_oauth(&self, p: &str, pid: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut r = conn.query("SELECT u.id, u.email, u.password_hash, u.role, u.metadata FROM users u JOIN auth_identities ai ON u.id = ai.user_id WHERE ai.provider = ?1 AND ai.provider_id = ?2", params![p, pid]).await?;
        if let Some(row) = r.next().await? { 
            let meta_str: String = row.get(4).unwrap_or("{}".to_string());
            Ok(Some(User { 
                id: row.get(0)?, 
                email: row.get(1)?, 
                password_hash: row.get(2)?, 
                role: row.get(3)?,
                metadata: serde_json::from_str(&meta_str).ok()
            })) 
        } else { Ok(None) }
    }
    async fn link_oauth(&self, uid: i64, p: &str, pid: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_core_read().await.execute("INSERT INTO auth_identities (user_id, provider, provider_id) VALUES (?1, ?2, ?3)", params![uid, p, pid]).await?; Ok(()) }
    async fn create_auth_token(&self, uid: i64, t: &str, tk: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_core_read().await.execute("INSERT INTO auth_tokens (token, user_id, type, expires_at) VALUES (?1, ?2, ?3, datetime('now', '+1 hour'))", params![tk, uid, t]).await?; Ok(()) }
    async fn consume_auth_token(&self, tk: &str, t: &str) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut r = conn.query("SELECT user_id FROM auth_tokens WHERE token = ?1 AND type = ?2 AND expires_at > datetime('now')", params![tk, t]).await?;
        if let Some(row) = r.next().await? { let uid: i64 = row.get(0)?; conn.execute("DELETE FROM auth_tokens WHERE token = ?1", params![tk]).await?; Ok(Some(uid)) } else { Ok(None) }
    }
    async fn set_user_verified(&self, uid: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_core_read().await.execute("UPDATE users SET is_verified = 1 WHERE id = ?1", params![uid]).await?; Ok(()) }
    async fn get_config(&self, key: &str) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut rows = conn.query("SELECT value FROM _system_config_settings WHERE key = ?1", params![key]).await?;
        if let Some(row) = rows.next().await? { 
            let v_str: String = row.get(0)?; 
            Ok(Some(serde_json::from_str(&v_str)?)) 
        } else { 
            Ok(None) 
        }
    }
    async fn set_config(&self, key: &str, value: &serde_json::Value, encrypted: bool) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let v_str = serde_json::to_string(value)?;
        conn.execute(
            "INSERT INTO _system_config_settings (key, value, encrypted, updated_at) VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP) 
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, encrypted=excluded.encrypted, updated_at=CURRENT_TIMESTAMP", 
            params![key, v_str, encrypted]
        ).await?; 
        Ok(())
    }
    async fn list_configs(&self) -> std::result::Result<Vec<models::ConfigItem>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut rows = conn.query("SELECT key, value, encrypted, updated_at FROM _system_config_settings ORDER BY key ASC", ()).await?;
        let mut items = Vec::new();
        while let Some(row) = rows.next().await? {
            let key: String = row.get(0)?;
            let val_str: String = row.get(1)?;
            let encrypted: bool = row.get(2)?;
            let updated_at: String = row.get(3)?;
            
            // If encrypted, we return None for value (masked) or a placeholder
            // If not encrypted, we return the raw value
            let value = if encrypted {
                Some("******".to_string())
            } else {
                // Try to format JSON nicely if possible, else return raw
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&val_str) {
                    if v.is_string() {
                        Some(v.as_str().unwrap().to_string())
                    } else {
                        Some(val_str)
                    }
                } else {
                    Some(val_str)
                }
            };

            items.push(models::ConfigItem { key, value, encrypted, updated_at });
        }
        Ok(items)
    }
    async fn delete_config(&self, key: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        conn.execute("DELETE FROM _system_config_settings WHERE key = ?1", params![key]).await?;
        Ok(())
    }

    // --- Files ---
    async fn create_file_metadata(&self, f: &str, o: &str, m: &str, s: i64, u: Option<i64>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.data_batcher.insert("INSERT INTO _storage_files (filename, original_name, mime_type, size, user_id) VALUES (?1, ?2, ?3, ?4, ?5)".into(), vec![f.into(), o.into(), m.into(), s.into(), u.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)
    }
    async fn list_files(&self, limit: i64, offset: i64) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut rows = conn.query("SELECT id, filename, original_name, mime_type, size, created_at FROM _storage_files ORDER BY created_at DESC LIMIT ?1 OFFSET ?2", params![limit, offset]).await?;
        let mut files = Vec::new();
        while let Some(row) = rows.next().await? { files.push(models::StoredFile { id: row.get(0)?, filename: row.get(1)?, original_name: row.get(2)?, mime_type: row.get(3)?, size: row.get(4)?, created_at: row.get(5)? }); }
        Ok(files)
    }
    async fn count_files(&self) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut row = conn.query("SELECT COUNT(*) FROM _storage_files", ()).await?;
        if let Some(r) = row.next().await? { Ok(r.get(0)?) } else { Ok(0) }
    }
    async fn get_file_metadata(&self, id: i64) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut rows = conn.query("SELECT id, filename, original_name, mime_type, size, created_at FROM _storage_files WHERE id = ?1", params![id]).await?;
        if let Some(row) = rows.next().await? { Ok(Some(models::StoredFile { id: row.get(0)?, filename: row.get(1)?, original_name: row.get(2)?, mime_type: row.get(3)?, size: row.get(4)?, created_at: row.get(5)? })) } else { Ok(None) }
    }
    async fn delete_file_metadata(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
        };
        self.data_batcher.execute("DELETE FROM _storage_files WHERE id = ?1".into(), vec![id.into()]).await.map_err(map_err)?; 
        Ok(())
    }

    // --- Logs (Use Batcher) ---
    async fn log_audit_event(&self, level: &str, message: &str, source: &str, meta: Option<serde_json::Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let meta_str = serde_json::to_string(&meta).unwrap_or("{}".to_string());
        self.log_batcher.execute("INSERT INTO _audit_logs (level, message, source, meta) VALUES (?1, ?2, ?3, ?4)".into(), vec![level.into(), message.into(), source.into(), meta_str.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }
    async fn list_audit_logs(&self) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_log_read().await;
        let mut rows = conn.query("SELECT id, level, message, source, meta, timestamp FROM _audit_logs ORDER BY timestamp DESC LIMIT 100", ()).await?;
        let mut logs = Vec::new();
        while let Some(row) = rows.next().await? {
            let meta_str: Option<String> = row.get(4)?;
            let meta = meta_str.map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null));
            logs.push(serde_json::json!({ "id": row.get::<i64>(0)?, "level": row.get::<String>(1)?, "message": row.get::<String>(2)?, "source": row.get::<String>(3)?, "meta": meta, "timestamp": row.get::<String>(5)? }));
        }
        Ok(logs)
    }
    async fn log_system_event(&self, level: &str, target: &str, message: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.log_batcher.execute(
            "INSERT INTO _system_logs (level, target, message) VALUES (?1, ?2, ?3)".into(),
            vec![level.into(), target.into(), message.into()]
        ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }

    // --- System DB ---
    async fn list_ai_actions(&self) -> std::result::Result<Vec<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut rows = conn.query("SELECT id, slug, name, model, system_prompt, template, config FROM _ai_actions", ()).await?;
        let mut res = Vec::new();
        while let Some(row) = rows.next().await? {
            let conf_str: String = row.get(6).unwrap_or("{}".to_string());
            res.push(ai_models::AiAction { id: row.get(0)?, slug: row.get(1)?, name: row.get(2)?, model: row.get(3)?, system_prompt: row.get(4)?, template: row.get(5)?, config: serde_json::from_str(&conf_str).unwrap_or_default() });
        }
        Ok(res)
    }
    async fn get_ai_action(&self, slug: &str) -> std::result::Result<Option<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut rows = conn.query("SELECT id, slug, name, model, system_prompt, template, config FROM _ai_actions WHERE slug = ?1", params![slug]).await?;
        if let Some(row) = rows.next().await? {
            let conf_str: String = row.get(6).unwrap_or("{}".to_string());
            Ok(Some(ai_models::AiAction { id: row.get(0)?, slug: row.get(1)?, name: row.get(2)?, model: row.get(3)?, system_prompt: row.get(4)?, template: row.get(5)?, config: serde_json::from_str(&conf_str).unwrap_or_default() }))
        } else { Ok(None) }
    }
    async fn create_ai_action(&self, req: ai_models::CreateActionReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        conn.execute("INSERT INTO _ai_actions (slug, name, model, system_prompt, template, config) VALUES (?1, ?2, ?3, ?4, ?5, '{}')", params![req.slug, req.name, req.model, req.system_prompt, req.template]).await?;
        Ok(conn.last_insert_rowid())
    }
    async fn delete_ai_action(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_sys_read().await.execute("DELETE FROM _ai_actions WHERE id = ?1", params![id]).await?; Ok(()) }
    async fn create_ai_session(&self, s: &AiSession) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { 
        self.get_sys_read().await.execute(
            "INSERT INTO _ai_sessions (id, name, messages, current_manifest, pending_manifest, diff_summary, last_error, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", 
            params![
                s.id.clone(), 
                s.name.clone(), 
                serde_json::to_string(&s.messages)?, 
                serde_json::to_string(&s.current_manifest)?, 
                serde_json::to_string(&s.pending_manifest)?, 
                s.diff_summary.clone(), 
                s.last_error.clone(), 
                s.created_at.clone()
            ]
        ).await?; 
        Ok(()) 
    }
    async fn get_ai_session(&self, id: &str) -> std::result::Result<Option<AiSession>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut r = conn.query("SELECT id, name, messages, current_manifest, pending_manifest, diff_summary, last_error, created_at FROM _ai_sessions WHERE id = ?1", params![id]).await?;
        
        if let Some(row) = r.next().await? {
            let m_str: String = row.get(2)?; 
            let man_str: Option<String> = row.get(3)?;
            let pend_str: Option<String> = row.get(4).unwrap_or(None);

            Ok(Some(AiSession { 
                id: row.get(0)?, 
                name: row.get(1)?, 
                messages: serde_json::from_str(&m_str)?, 
                current_manifest: match man_str { Some(s) => serde_json::from_str(&s).ok(), None => None },
                pending_manifest: match pend_str { Some(s) => serde_json::from_str(&s).ok(), None => None },
                diff_summary: row.get(5).unwrap_or(None),
                last_error: row.get(6).unwrap_or(None),
                created_at: row.get(7)? 
            }))
        } else { Ok(None) }
    }

    async fn update_ai_session(&self, s: &AiSession) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { 
        self.get_sys_read().await.execute(
            "UPDATE _ai_sessions SET messages = ?1, current_manifest = ?2, pending_manifest = ?3, diff_summary = ?4, last_error = ?5 WHERE id = ?6", 
            params![
                serde_json::to_string(&s.messages)?, 
                serde_json::to_string(&s.current_manifest)?, 
                serde_json::to_string(&s.pending_manifest)?, 
                s.diff_summary.clone(), 
                s.last_error.clone(), 
                s.id.clone()
            ]
        ).await?; 
        Ok(()) 
    }

    async fn list_ai_sessions(&self) -> std::result::Result<Vec<AiSession>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut r = conn.query("SELECT id, name, messages, current_manifest, pending_manifest, diff_summary, last_error, created_at FROM _ai_sessions ORDER BY created_at DESC", ()).await?;
        let mut s = Vec::new();
        while let Some(row) = r.next().await? {
             let m_str: String = row.get(2)?; 
             let man_str: Option<String> = row.get(3)?;
             let pend_str: Option<String> = row.get(4).unwrap_or(None);

             s.push(AiSession { 
                 id: row.get(0)?, 
                 name: row.get(1)?, 
                 messages: serde_json::from_str(&m_str).unwrap_or_default(), 
                 current_manifest: match man_str { Some(str) => serde_json::from_str(&str).ok(), None => None }, 
                 pending_manifest: match pend_str { Some(str) => serde_json::from_str(&str).ok(), None => None },
                 diff_summary: row.get(5).unwrap_or(None),
                 last_error: row.get(6).unwrap_or(None),
                 created_at: row.get(7)? 
             });
        }
        Ok(s)
    }
    async fn save_plugin(&self, p: &Plugin) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_sys_read().await.execute("INSERT INTO _plugins (id, name, version, manifest, description) VALUES (?1, ?2, ?3, ?4, ?5)", params![p.id.clone(), p.name.clone(), p.version.clone(), serde_json::to_string(&p.manifest)?, p.description.clone()]).await?; Ok(()) }
    async fn list_plugins(&self) -> std::result::Result<Vec<Plugin>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut r = conn.query("SELECT id, name, version, manifest, description FROM _plugins ORDER BY created_at DESC", ()).await?;
        let mut p = Vec::new();
        while let Some(row) = r.next().await? { let m_str: String = row.get(3)?; p.push(Plugin { id: row.get(0)?, name: row.get(1)?, version: row.get(2)?, manifest: serde_json::from_str(&m_str)?, description: row.get(4)? }); }
        Ok(p)
    }
    async fn list_scripts(&self) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut r = conn.query("SELECT id, name, trigger_type, code, active, target_collection FROM _scripts", ()).await?;
        let mut v = Vec::new();
        while let Some(row) = r.next().await? { 
            v.push(script_models::Script { 
                id: row.get(0)?, 
                name: row.get(1)?, 
                trigger_type: row.get(2)?, 
                code: row.get(3)?, 
                active: row.get(4)?,
                target_collection: row.get(5)? 
            }); 
        }
        Ok(v)
    }

    async fn create_script(&self, req: script_models::CreateScriptReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut rows = conn.query(
            "INSERT INTO _scripts (name, trigger_type, code, target_collection) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(name) DO UPDATE SET trigger_type=excluded.trigger_type, code=excluded.code, target_collection=excluded.target_collection, created_at=CURRENT_TIMESTAMP RETURNING id", 
            params![req.name, req.trigger_type, req.code, req.target_collection]
        ).await?;
        if let Some(row) = rows.next().await? { Ok(row.get(0)?) } else { Err("Failed".into()) }
    }

    async fn delete_script(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_sys_read().await.execute("DELETE FROM _scripts WHERE id = ?1", params![id]).await?; Ok(()) }
    
    async fn get_script_by_name(&self, name: &str) -> std::result::Result<Option<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut r = conn.query("SELECT id, name, trigger_type, code, active, target_collection FROM _scripts WHERE name = ?1", params![name]).await?;
        if let Some(row) = r.next().await? { 
            Ok(Some(script_models::Script { 
                id: row.get(0)?, 
                name: row.get(1)?, 
                trigger_type: row.get(2)?, 
                code: row.get(3)?, 
                active: row.get(4)?,
                target_collection: row.get(5)? 
            })) 
        } else { Ok(None) }
    }

    async fn get_scripts_by_trigger(&self, t: &str) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut r = conn.query("SELECT id, name, trigger_type, code, active, target_collection FROM _scripts WHERE trigger_type = ?1 AND active = 1", params![t]).await?;
        let mut v = Vec::new();
        while let Some(row) = r.next().await? { 
            v.push(script_models::Script { 
                id: row.get(0)?, 
                name: row.get(1)?, 
                trigger_type: row.get(2)?, 
                code: row.get(3)?, 
                active: row.get(4)?,
                target_collection: row.get(5)? 
            }); 
        }
        Ok(v)
    }
    async fn list_templates(&self) -> std::result::Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut r = conn.query("SELECT id, slug, content, script_id, created_at FROM _templates", ()).await?;
        let mut v = Vec::new();
        while let Some(row) = r.next().await? { v.push(models::Template { id: row.get(0)?, slug: row.get(1)?, content: row.get(2)?, script_id: row.get(3)?, created_at: row.get(4)? }); }
        Ok(v)
    }
    async fn get_template_by_slug(&self, s: &str) -> std::result::Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut r = conn.query("SELECT id, slug, content, script_id, created_at FROM _templates WHERE slug = ?1", params![s]).await?;
        if let Some(row) = r.next().await? { Ok(Some(models::Template { id: row.get(0)?, slug: row.get(1)?, content: row.get(2)?, script_id: row.get(3)?, created_at: row.get(4)? })) } else { Ok(None) }
    }
    async fn create_template(&self, req: models::CreateTemplateReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut rows = conn.query("INSERT INTO _templates (slug, content, script_id) VALUES (?1, ?2, ?3) ON CONFLICT(slug) DO UPDATE SET content=excluded.content, script_id=excluded.script_id, created_at=CURRENT_TIMESTAMP RETURNING id", params![req.slug, req.content, req.script_id]).await?;
        if let Some(row) = rows.next().await? { Ok(row.get(0)?) } else { Err("Failed".into()) }
    }
    async fn update_template(&self, id: i64, content: String, script_id: Option<i64>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_sys_read().await.execute("UPDATE _templates SET content = ?1, script_id = ?2 WHERE id = ?3", params![content, script_id, id]).await?; Ok(()) }
    async fn delete_template(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_sys_read().await.execute("DELETE FROM _templates WHERE id = ?1", params![id]).await?; Ok(()) }

    async fn create_relation(&self, oc: i64, oi: i64, tc: i64, ti: i64, rn: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.data_batcher.execute("INSERT INTO _relations (origin_col_id, origin_rec_id, target_col_id, target_rec_id, rel_name) VALUES (?1, ?2, ?3, ?4, ?5)".into(), vec![oc.into(), oi.into(), tc.into(), ti.into(), rn.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?; Ok(())
    }
    async fn delete_relation(&self, oc: i64, oi: i64, tc: i64, ti: i64, rn: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
        };
        self.data_batcher.execute("DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND target_col_id=?3 AND target_rec_id=?4 AND rel_name=?5".into(), vec![oc.into(), oi.into(), tc.into(), ti.into(), rn.into()]).await.map_err(map_err)?; 
        Ok(())
    }
    async fn get_related_ids(&self, oc: i64, oi: i64, rn: &str) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut rows = conn.query("SELECT target_col_id, target_rec_id FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND rel_name=?3", params![oc, oi, rn]).await?;
        let mut results = Vec::new(); while let Some(row) = rows.next().await? { results.push((row.get(0)?, row.get(1)?)); } Ok(results)
    }
    async fn get_records_by_ids(&self, collection_id: i64, ids: &[i64]) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        if ids.is_empty() { return Ok(vec![]); }
        let id_list = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, json(data), NULL, created, updated FROM records WHERE collection_id = ? AND id IN ({})", id_list);
        let conn = self.get_data_read().await;
        let mut rows = conn.query(&sql, params![collection_id]).await?;
        let mut records = Vec::new();
        while let Some(row) = rows.next().await? { records.push(row_to_record(&row)?); }
        Ok(records)
    }

    // --- Tenants ---
    async fn register_tenant(&self, id: &str, owner_id: Option<i64>, name: Option<String>, tier: Option<String>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        
        // Uses UPSERT logic (ON CONFLICT DO UPDATE) to allow re-registering safely or updating details
        conn.execute(
            "INSERT INTO _tenants (id, owner_id, name, tier) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET 
                owner_id=excluded.owner_id, 
                name=excluded.name, 
                tier=excluded.tier,
                updated_at=CURRENT_TIMESTAMP",
            params![
                id, 
                owner_id, 
                name, 
                tier.unwrap_or("free".to_string()) // Default tier
            ]
        ).await?;
        Ok(())
    }

    async fn get_tenant_status(&self, tenant_id: &str) -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        // Use optional query to handle missing tenants gracefully
        let mut stmt = conn.prepare("SELECT status FROM _tenants WHERE id = ?1").await?;
        let mut rows = stmt.query(params![tenant_id]).await?;

        if let Some(row) = rows.next().await? {
            let status: String = row.get(0)?;
            Ok(status)
        } else {
            Ok("not_found".to_string())
        }
    }

    async fn update_tenant_status(&self, tenant_id: &str, status: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        conn.execute("UPDATE _tenants SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2", params![status, tenant_id]).await?;
        Ok(())
    }

    // --- Sandboxes ---
    async fn register_sandbox(&self, id: &str, owner_id: Option<i64>, name: Option<String>, expires_at: Option<String>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        
        conn.execute(
            "INSERT INTO _sandboxes (id, owner_id, name, expires_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET 
                owner_id=excluded.owner_id, 
                name=excluded.name, 
                expires_at=excluded.expires_at",
            params![
                id, 
                owner_id, 
                name,
                expires_at
            ]
        ).await?;
        Ok(())
    }

    // --- Dashboard ---
    async fn get_dashboard_stats(&self) -> std::result::Result<DashboardData, Box<dyn std::error::Error + Send + Sync>> {
        let data_conn = self.get_data_read().await;
        let log_conn = self.get_log_read().await;
        let sys_conn = self.get_sys_read().await;

        // 1. Collections Count
        let mut row = data_conn.query("SELECT COUNT(*) FROM collections", ()).await?;
        let collections_count: i64 = if let Some(r) = row.next().await? { r.get(0)? } else { 0 };

        // 2. Records Count
        let mut row = data_conn.query("SELECT COUNT(*) FROM records", ()).await?;
        let total_records: i64 = if let Some(r) = row.next().await? { r.get(0)? } else { 0 };

        // 3. DB Size (Calculate via SQL to support Tenants/Sandboxes dynamically)
        // We sum up Data + Logs + System DB sizes
        let mut total_bytes: i64 = 0;
        
        for conn in [&data_conn, &log_conn, &sys_conn] {
            let mut p_count = conn.query("PRAGMA page_count", ()).await?;
            let count: i64 = if let Some(r) = p_count.next().await? { r.get(0)? } else { 0 };
            
            let mut p_size = conn.query("PRAGMA page_size", ()).await?;
            let size: i64 = if let Some(r) = p_size.next().await? { r.get(0)? } else { 0 };
            
            total_bytes += count * size;
        }

        let db_size_mb = (total_bytes as f64 / 1024.0 / 1024.0 * 100.0).round() / 100.0;

        // 4. Chart Data (Using _system_logs for traffic analysis)
        let sql_chart = "
            SELECT 
                strftime('%Y-%m-%d', timestamp) as day_date, 
                COUNT(*) as req_count, 
                SUM(CASE WHEN level = 'ERROR' OR level = 'error' THEN 1 ELSE 0 END) as err_count 
            FROM _system_logs 
            WHERE timestamp >= date('now', '-7 days') 
            GROUP BY day_date
        ";
        
        let mut rows = log_conn.query(sql_chart, ()).await?;
        let mut daily_stats: HashMap<String, (i64, i64)> = HashMap::new();
        let mut total_requests = 0;

        while let Some(row) = rows.next().await? {
            let date_str: String = row.get(0)?;
            let reqs: i64 = row.get(1)?;
            let errs: i64 = row.get(2)?;
            total_requests += reqs;
            daily_stats.insert(date_str, (reqs, errs));
        }

        let mut chart_data: Vec<ChartPoint> = Vec::new();
        let now = Utc::now();
        // Generate last 7 days points (filling 0s if no logs)
        for i in (0..7).rev() {
            let date = now - chrono::Duration::days(i);
            let date_key = date.format("%Y-%m-%d").to_string();
            let day_name = date.format("%a").to_string(); // Mon, Tue...
            let (reqs, errs) = daily_stats.get(&date_key).unwrap_or(&(0, 0));
            chart_data.push(ChartPoint { name: day_name, requests: *reqs, errors: *errs });
        }

        // 5. Recent Logs (From _system_logs)
        // We select the most recent 10 logs
        let mut recent_rows = log_conn.query(
            "SELECT id, level, message, target, timestamp FROM _system_logs ORDER BY timestamp DESC LIMIT 10", 
            ()
        ).await?;
        
        let mut recent_logs = Vec::new();
        while let Some(row) = recent_rows.next().await? {
            recent_logs.push(serde_json::json!({ 
                "id": row.get::<i64>(0)?.to_string(), 
                "level": row.get::<String>(1)?, 
                "message": row.get::<String>(2)?, 
                "source": row.get::<String>(3)?, // target mapped to source for UI compatibility
                "timestamp": row.get::<String>(4)? 
            }));
        }

        Ok(DashboardData {
            stats: DashboardStats { total_requests, db_size_mb, collections_count, total_records },
            chart: chart_data,
            recent_logs,
        })
    }

    // VECTORS: Implement save_vector
    async fn save_vector(&self, collection_id: i64, record_id: i64, field_name: &str, vector: Vec<f32>, model: &str) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let vec_json = serde_json::to_string(&vector)?;
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
        };
        
        // OPTIMIZATION: Use UPSERT (Insert or Replace)
        // This relies on the UNIQUE constraint we added: UNIQUE(collection_id, record_id, field_name, model)
        self.vector_batcher.insert(
            "INSERT INTO vectors (collection_id, record_id, field_name, vector, model) 
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(collection_id, record_id, field_name, model) 
             DO UPDATE SET vector = excluded.vector, created_at = CURRENT_TIMESTAMP".into(),
            vec![
                collection_id.into(), 
                record_id.into(), 
                field_name.into(), 
                vec_json.into(), 
                model.into()
            ]
        ).await.map_err(map_err)?;

        Ok(())
    }
    // Check vector existence
    async fn has_vector(&self, collection_id: i64, record_id: i64, field_name: &str, model: &str) -> std::result::Result<bool, Box<dyn StdError + Send + Sync>> {
        let conn = self.get_vector_read().await;
        let mut rows = conn.query(
            "SELECT 1 FROM vectors WHERE collection_id = ?1 AND record_id = ?2 AND field_name = ?3 AND model = ?4", 
            params![collection_id, record_id, field_name, model]
        ).await?;
        
        Ok(rows.next().await?.is_some())
    }

    async fn get_record_vectors(&self, collection_id: i64, record_id: i64) -> std::result::Result<Vec<models::VectorRecord>, Box<dyn StdError + Send + Sync>> {
        let conn = self.get_vector_read().await;
        let mut rows = conn.query(
            "SELECT field_name, vector, model FROM vectors WHERE collection_id = ?1 AND record_id = ?2",
            params![collection_id, record_id]
        ).await?;

        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            let field_name: String = row.get(0)?;
            
            // [FIX] Safely extract value to avoid 'invalid value type' panic in libsql
            let val = row.get_value(1).map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;
            
            let vector_blob: Vec<u8> = match val {
                libsql::Value::Blob(b) => b,
                libsql::Value::Text(s) => s.into_bytes(), // Fallback if stored as text
                _ => vec![], // Empty if Null or other type
            };

            let model: String = row.get(2)?;

            // Parse the stored JSON vector
            // Only try to parse if we have data
            if !vector_blob.is_empty() {
                let vector: Vec<f32> = serde_json::from_slice(&vector_blob).unwrap_or_default();
                results.push(models::VectorRecord {
                    field_name,
                    vector,
                    model
                });
            }
        }
        Ok(results)
    }

    // VECTORS: Implement get_vectors_for_collection
    // Filter by model so we don't load incompatible vectors into HNSW
    async fn get_vectors_for_collection(&self, collection_id: i64, model: &str) -> std::result::Result<Vec<(i64, String, Vec<f32>)>, Box<dyn StdError + Send + Sync>> {
        let conn = self.get_vector_read().await; 
        let mut rows = conn.query(
            "SELECT record_id, field_name, vector FROM vectors WHERE collection_id = ?1 AND model = ?2", 
            params![collection_id, model.to_string()]
        ).await?;
        
        let mut vectors = Vec::new();
        while let Some(row) = rows.next().await? {
            let record_id: i64 = row.get(0)?;
            let field_name: String = row.get(1)?;
            let vector_json_str: String = row.get(2)?;
            
            if let Ok(vector) = serde_json::from_str::<Vec<f32>>(&vector_json_str) {
                vectors.push((record_id, field_name, vector));
            }
        }
        Ok(vectors)
    }

    // VECTORS Implement search_vector
    async fn search_vector(&self, collection_id: i64, field: &str, vector: Vec<f32>, limit: usize) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Search Index via Provider
        let results = self.vector_provider.search(collection_id, field, &vector, limit).await
            .map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        
        if results.is_empty() {
            return Ok(vec![]);
        }

        // 2. Extract IDs
        let ids: Vec<i64> = results.iter().map(|(id, _score)| *id).collect();

        // 3. Fetch Records
        self.get_records_by_ids(collection_id, &ids).await
    }
}

// --- OPTIMIZATION: PRAGMAS ---
async fn apply_pragmas(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    conn.execute_batch("
        PRAGMA journal_mode = WAL;          -- Write-Ahead Logging (Crucial for concurrency)
        PRAGMA synchronous = NORMAL;        -- Faster writes, safe enough for WAL
        PRAGMA busy_timeout = 5000;         -- Wait 5s before failing if locked
        PRAGMA temp_store = MEMORY;         -- Temp tables in RAM
        PRAGMA cache_size = -64000;         -- 64MB Cache
        PRAGMA wal_autocheckpoint = 1000;   -- Checkpoint every 1000 pages
        PRAGMA mmap_size = 30000000000;     -- Memory Map large DBs (Fast Reads)
    ").await.map(|_| ())
}

async fn setup_core(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    
    conn.execute("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, role TEXT NOT NULL, is_verified BOOLEAN DEFAULT 0, metadata JSON DEFAULT '{}')", ()).await?;
    let _ = conn.execute("ALTER TABLE users ADD COLUMN metadata JSON DEFAULT '{}'", ()).await;

    conn.execute("CREATE TABLE IF NOT EXISTS auth_identities (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL, provider TEXT NOT NULL, provider_id TEXT NOT NULL)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS auth_tokens (token TEXT PRIMARY KEY, user_id INTEGER NOT NULL, type TEXT NOT NULL, expires_at DATETIME NOT NULL)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _system_config_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, encrypted BOOLEAN DEFAULT 0, updated_at DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _api_keys (
        id INTEGER PRIMARY KEY AUTOINCREMENT, 
        name TEXT NOT NULL, 
        prefix TEXT NOT NULL, 
        hash TEXT NOT NULL, 
        role TEXT NOT NULL DEFAULT 'admin',
        scope TEXT DEFAULT 'root', 
        bypass_cors BOOLEAN DEFAULT 0,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
    )", ()).await?;
    
    let _ = conn.execute("ALTER TABLE _api_keys ADD COLUMN scope TEXT DEFAULT 'root'", ()).await;
    let _ = conn.execute("ALTER TABLE _api_keys ADD COLUMN bypass_cors BOOLEAN DEFAULT 0", ()).await;

    conn.execute("CREATE TABLE IF NOT EXISTS _tenants (
        id TEXT PRIMARY KEY,
        name TEXT,
        owner_id INTEGER,
        status TEXT DEFAULT 'active',
        tier TEXT DEFAULT 'free',
        max_storage_mb INTEGER DEFAULT 500,
        current_storage_mb REAL DEFAULT 0,
        max_vectors INTEGER DEFAULT 10000,
        current_vectors INTEGER DEFAULT 0,
        max_ai_requests INTEGER DEFAULT 100,
        current_ai_requests INTEGER DEFAULT 0,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
    )", ()).await?;

    conn.execute("CREATE TABLE IF NOT EXISTS _sandboxes (
        id TEXT PRIMARY KEY,
        name TEXT,
        owner_id INTEGER,
        status TEXT DEFAULT 'active',
        max_storage_mb INTEGER DEFAULT 100,
        current_storage_mb REAL DEFAULT 0,
        expires_at DATETIME,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
    )", ()).await?;
    
    Ok(())
}

async fn setup_data(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    conn.execute("CREATE TABLE IF NOT EXISTS collections (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, schema JSON)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS records (id INTEGER PRIMARY KEY AUTOINCREMENT, collection_id INTEGER NOT NULL, data JSONB NOT NULL, created DATETIME DEFAULT CURRENT_TIMESTAMP, updated DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    
    let _ = conn.execute("ALTER TABLE records ADD COLUMN created DATETIME DEFAULT CURRENT_TIMESTAMP", ()).await;
    let _ = conn.execute("ALTER TABLE records ADD COLUMN updated DATETIME DEFAULT CURRENT_TIMESTAMP", ()).await;

    conn.execute("CREATE TABLE IF NOT EXISTS _storage_files (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL, original_name TEXT NOT NULL, mime_type TEXT NOT NULL, size INTEGER NOT NULL, user_id INTEGER, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _relations (id INTEGER PRIMARY KEY AUTOINCREMENT, origin_col_id INTEGER NOT NULL, origin_rec_id INTEGER NOT NULL, target_col_id INTEGER NOT NULL, target_rec_id INTEGER NOT NULL, rel_name TEXT NOT NULL, properties JSON)", ()).await?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_relations_origin ON _relations(origin_col_id, origin_rec_id, rel_name)", ()).await?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_relations_target ON _relations(target_col_id, target_rec_id)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _unique_values (index_key TEXT NOT NULL, value TEXT NOT NULL, record_id INTEGER NOT NULL, PRIMARY KEY (index_key, value))", ()).await?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_unique_lookup ON _unique_values(index_key, value)", ()).await?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_unique_record ON _unique_values(record_id)", ()).await?;
    Ok(())
}

async fn setup_logs(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    conn.execute("CREATE TABLE IF NOT EXISTS _audit_logs (id INTEGER PRIMARY KEY AUTOINCREMENT, level TEXT NOT NULL, message TEXT NOT NULL, source TEXT NOT NULL, meta JSON, timestamp DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _system_logs (
        id INTEGER PRIMARY KEY AUTOINCREMENT, 
        level TEXT NOT NULL, 
        target TEXT NOT NULL, 
        message TEXT NOT NULL, 
        timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
    )", ()).await?;
    Ok(())
}

async fn setup_sys(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    conn.execute("CREATE TABLE IF NOT EXISTS _ai_actions (id INTEGER PRIMARY KEY AUTOINCREMENT, slug TEXT UNIQUE NOT NULL, name TEXT NOT NULL, model TEXT NOT NULL, system_prompt TEXT, template TEXT NOT NULL, config JSON, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _ai_sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL, messages JSON, current_manifest JSON, pending_manifest JSON, diff_summary TEXT, last_error TEXT, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _plugins (id TEXT PRIMARY KEY, name TEXT NOT NULL, version TEXT NOT NULL, manifest JSON NOT NULL, description TEXT, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _scripts (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE NOT NULL, trigger_type TEXT NOT NULL, code TEXT NOT NULL, active BOOLEAN DEFAULT 1, target_collection TEXT, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _templates (id INTEGER PRIMARY KEY AUTOINCREMENT, slug TEXT UNIQUE NOT NULL, content TEXT NOT NULL, script_id INTEGER, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    Ok(())
}

async fn setup_vectors(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS vectors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            collection_id INTEGER NOT NULL,
            record_id INTEGER NOT NULL,
            field_name TEXT NOT NULL,
            vector BLOB NOT NULL,
            model TEXT NOT NULL DEFAULT 'unknown',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(collection_id, record_id, field_name, model) 
        )", 
        ()
    ).await?;
    let _ = conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_vec_unique_model ON vectors(collection_id, record_id, field_name, model)", ()).await;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_vec_record ON vectors(record_id)", ()).await?;
    Ok(())
}