use crate::auth::User;
use crate::query::QueryOptions;
use crate::schema::CollectionSchema;
use async_trait::async_trait;
use libsql::{params, Builder, Connection, Database, Result, Row};
use search::SearchManager;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::ai_models::{AiSession, Plugin};
use crate::models::{DashboardData, DashboardStats, ChartPoint};
use chrono::Utc;
use std::collections::{HashMap, BTreeMap};
use std::error::Error as StdError;
use std::path::Path;

const COMPOSITE_SEPARATOR: &str = "__::__";

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
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ListResult {
    pub items: Vec<Record>,
    pub total: i64,
}

// Interface for Vector Operations
#[async_trait]
pub trait VectorProvider: Send + Sync {
    // FIX: Fully qualified Result to avoid clash with libsql::Result
    async fn embed(&self, text: &str) -> std::result::Result<Vec<f32>, String>;
    async fn search(&self, col_id: i64, field: &str, vec: &[f32], limit: usize) -> std::result::Result<Vec<(i64, f32)>, String>;
    async fn index(&self, col_id: i64, rec_id: i64, field: &str, vec: &[f32]) -> std::result::Result<(), String>;
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
    async fn list_records(&self, collection_id: i64, options: QueryOptions) -> std::result::Result<ListResult, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_record(&self, collection_id: i64, record_id: i64, expand: Option<String>) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_record(&self, collection_id: i64, record_id: i64, data: &Value) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_record(&self, collection_id: i64, record_id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Search ---
    async fn search_records(&self, collection_id: i64, query: &str) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;
    async fn reindex_collection(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
    async fn instant_search(&self, collection_id: i64, query: &str, limit: usize) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>>;
    // --- NEW: Search Helpers exposed for Job Worker ---
    async fn index_record_search(&self, collection_id: i64, record_id: i64, data: &Value, schema: &CollectionSchema) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
    async fn delete_record_search(&self, collection_id: i64, record_id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    // --- Users (Auth) ---
    async fn create_user(&self, email: &str, password_hash: &str, role: &str) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_user_by_email(&self, email: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn list_users(&self) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_user(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Storage Metadata ---
    async fn create_file_metadata(&self, filename: &str, original_name: &str, mime_type: &str, size: i64, user_id: Option<i64>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn list_files(&self, limit: i64, offset: i64) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_file_metadata(&self, id: i64) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_file_metadata(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Advanced Auth ---
    async fn get_user_by_oauth(&self, provider: &str, provider_id: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn link_oauth(&self, user_id: i64, provider: &str, provider_id: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn create_auth_token(&self, user_id: i64, token_type: &str, token: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn consume_auth_token(&self, token: &str, token_type: &str) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>>;
    async fn set_user_verified(&self, user_id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Secure Config ---
    async fn set_system_config(&self, key: &str, value: &security::EncryptedValue) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_system_config(&self, key: &str) -> std::result::Result<Option<security::EncryptedValue>, Box<dyn std::error::Error + Send + Sync>>;

    // --- Settings (Robust JSON) ---
    async fn get_setting(&self, key: &str) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;
    async fn save_setting(&self, key: &str, value: serde_json::Value, encrypt: bool) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Audit Logs ---
    async fn log_audit_event(&self, level: &str, message: &str, source: &str, meta: Option<serde_json::Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_audit_logs(&self) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;
    async fn log_system_event(&self, level: &str, target: &str, message: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

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
    // Retrieve all vectors for a collection (for HNSW reload on startup)
    async fn get_vectors_for_collection(&self, collection_id: i64) -> std::result::Result<Vec<(i64, String, Vec<f32>)>, Box<dyn std::error::Error + Send + Sync>>;

    async fn search_vector(&self, collection_id: i64, field: &str, vector: Vec<f32>, limit: usize) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;
    // Save vector to persistence layer (used by job worker)
    async fn save_vector(&self, collection_id: i64, record_id: i64, field_name: &str, vector: Vec<f32>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

fn row_to_collection(row: &Row) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
    let schema_str: Option<String> = row.get(2)?;
    let schema = match schema_str { Some(s) => serde_json::from_str(&s)?, None => None, };
    Ok(Collection { id: row.get(0)?, name: row.get(1)?, schema })
}

// OPTIMIZED row_to_record
fn row_to_record(
    row: &Row,
) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
    // We check the type of the value returned by LibSQL.
    // Index 1 is the 'data' column.
    let val = row.get_value(1).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    let data: serde_json::Value = match val {
        // Case 1: Standard (Text) - Returned when using json() or if column is TEXT
        libsql::Value::Text(s) => serde_json::from_str(&s)?,
        
        // Case 2: Binary (Blob) - Returned if column is JSONB and we forgot json()
        // We try to parse it as UTF-8 JSON. If it's valid JSON text stored as blob, this works.
        // If it's internal SQLite binary JSONB, this will fail gracefully instead of panic.
        libsql::Value::Blob(b) => serde_json::from_slice(&b).map_err(|_| "Failed to parse JSONB blob directly (driver requires json() wrapper in SQL)".to_string())?,
        
        // Case 3: Null/Other
        _ => serde_json::json!({}),
    };

    Ok(Record {
        id: row.get(0)?,
        data,
    })
}

// --- The Orchestrator: ApexKit ---
#[derive(Clone)] 
pub struct ApexKit {
    // Sharded Databases (Wrapped in Arc for Clone)
    core_db: Arc<Database>,  
    data_db: Arc<Database>,  
    log_db: Arc<Database>,   
    sys_db: Arc<Database>,  
    vector_db: Arc<Database>,  
    
    // Batchers
    data_batcher: batching::WriteManager,
    log_batcher: batching::WriteManager,
    vector_batcher: batching::WriteManager,

    search: Arc<SearchManager>,
    // Embedding Service
    pub embedder: Arc<embeddings::EmbedderService>,
    // Abstract interface for vectors
    // Core doesn't know about Candle, it just knows "VectorProvider"
    pub vector_provider: Arc<dyn VectorProvider>, 
}

impl ApexKit {
    pub fn new(core: Arc<Database>, data: Arc<Database>, log: Arc<Database>, sys: Arc<Database>, vec: Arc<Database>, vector_provider: Arc<dyn VectorProvider>) -> Self {
        // Init Batchers
        let data_batcher = batching::WriteManager::new(data.clone());
        let log_batcher = batching::WriteManager::new(log.clone());
        let vector_batcher = batching::WriteManager::new(vec.clone());

        Self {
            core_db: core,
            data_db: data,
            log_db: log,
            sys_db: sys,
            vector_db: vec,
            vector_provider,
            data_batcher,
            log_batcher,
            vector_batcher,
            search: Arc::new(SearchManager::new("./tantivy_indexes")),
            embedder: Arc::new(embeddings::EmbedderService::new()), // Init service
        }
    }

    /// Factory method to initialize a ApexKit instance from a specific folder path.
    /// This handles connection opening and Schema Migration automatically.
    pub async fn init_filesystem(
        base_path: &str, 
        vector_provider: Arc<dyn VectorProvider>
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        
        // 1. Ensure directory exists
        if !Path::new(base_path).exists() {
            std::fs::create_dir_all(base_path)?;
        }

        // 2. Connect to DBs
        let core = Builder::new_local(&format!("{}/core.db", base_path)).build().await?;
        let data = Builder::new_local(&format!("{}/data.db", base_path)).build().await?;
        let log = Builder::new_local(&format!("{}/logs.db", base_path)).build().await?;
        let sys = Builder::new_local(&format!("{}/system.db", base_path)).build().await?;
        let vec = Builder::new_local(&format!("{}/vectors.db", base_path)).build().await?;

        // 3. Apply Pragmas (Performance settings)
        apply_pragmas(&core).await?;
        apply_pragmas(&data).await?;
        apply_pragmas(&log).await?;
        apply_pragmas(&sys).await?;
        apply_pragmas(&vec).await?;

        // 4. Run Migrations (Create Tables if not exist)
        // These use the internal private setup_* functions in this file
        setup_core(&core).await?;
        setup_data(&data).await?;
        setup_logs(&log).await?;
        setup_sys(&sys).await?;
        setup_vectors(&vec).await?;

        // 5. Construct Instance
        Ok(Self::new(
            Arc::new(core),
            Arc::new(data),
            Arc::new(log),
            Arc::new(sys),
            Arc::new(vec),
            vector_provider
        ))
    }

    // FIX 2: Add setter for SearchManager (used by TenantManager)
    pub fn set_search_manager(&mut self, manager: Arc<SearchManager>) {
        self.search = manager;
    }

    // Helper: Which connection to use?
    fn get_core(&self) -> Result<Connection> { self.core_db.connect() }
    fn get_data(&self) -> Result<Connection> { self.data_db.connect() }
    fn get_log(&self) -> Result<Connection> { self.log_db.connect() }
    fn get_sys(&self) -> Result<Connection> { self.sys_db.connect() }
    fn get_vector(&self) -> Result<Connection> { self.vector_db.connect() }
    
    
    async fn ensure_search_index(&self, collection_id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(col) = self.get_collection(collection_id).await? {
            if let Some(schema) = &col.schema {
                if schema.fields.values().any(|f| f.indexed) {
                    self.search.load_index(collection_id, schema)?;
                }
            }
        }
        Ok(())
    }

    async fn sync_relations(&self, _conn: &Connection, collection_id: i64, record_id: i64, data: &Value) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let read_conn = self.get_data()?;
        let mut rows = read_conn.query("SELECT schema FROM collections WHERE id = ?1", params![collection_id]).await?;
        
        if let Some(row) = rows.next().await? {
            let schema_str: Option<String> = row.get(0)?;
            if let Some(s) = schema_str {
                let schema: CollectionSchema = serde_json::from_str(&s).unwrap_or_default();

                for (rel_name, rel_def) in schema.relations {
                    if let Some(val) = data.get(&rel_name) {
                        // DELETE old
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
                        let mut name_rows = read_conn.query("SELECT id FROM collections WHERE name = ?1", params![identifier.clone()]).await?;
                        if let Some(r) = name_rows.next().await? { target_col_id = Some(r.get(0)?); } 
                        else if let Ok(id_num) = identifier.parse::<i64>() {
                             let mut id_rows = read_conn.query("SELECT id FROM collections WHERE id = ?1", params![id_num]).await?;
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
    for field_group in &schema.composite_unique {
        let mut composite_uids = Vec::new();
        let mut composite_values = Vec::new();
        let mut missing_data = false;
        for field_name in field_group {
            if let Some(def) = schema.fields.get(field_name) {
                composite_uids.push(def.uid.clone());
                if let Some(val) = data.get(field_name) {
                    if val.is_null() { missing_data = true; break; }
                    composite_values.push(serialize_unique_val(val));
                } else { missing_data = true; break; }
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

async fn commit_uniqueness(batcher: &batching::WriteManager, col_id: i64, record_id: i64, data: &Value, schema: &CollectionSchema) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    batcher.execute("DELETE FROM _unique_values WHERE record_id = ?1".into(), vec![record_id.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
    for (name, def) in &schema.fields {
        if def.unique.unwrap_or(false) {
            if let Some(val) = data.get(name) {
                if !val.is_null() {
                    let index_key = format!("{}-{}", col_id, def.uid);
                    let val_str = serialize_unique_val(val);
                    batcher.execute("INSERT INTO _unique_values (index_key, value, record_id) VALUES (?1, ?2, ?3)".into(), vec![index_key.into(), val_str.into(), record_id.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
                }
            }
        }
    }
    for field_group in &schema.composite_unique {
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        let mut missing = false;
        for field_name in field_group {
            if let Some(def) = schema.fields.get(field_name) {
                if let Some(val) = data.get(field_name) {
                    if val.is_null() { missing = true; break; }
                    map.insert(def.uid.clone(), serialize_unique_val(val));
                } else { missing = true; break; }
            }
        }
        if !missing {
            let index_key = format!("{}-{}", col_id, map.keys().cloned().collect::<Vec<_>>().join("-"));
            let value_str = map.values().cloned().collect::<Vec<_>>().join(COMPOSITE_SEPARATOR);
            batcher.execute("INSERT INTO _unique_values (index_key, value, record_id) VALUES (?1, ?2, ?3)".into(), vec![index_key.into(), value_str.into(), record_id.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        }
    }
    Ok(())
}

#[async_trait]
impl Db for ApexKit {
    // --- Data DB (Collections/Records) ---

    async fn create_collection(&self, name: &str, schema: &Option<CollectionSchema>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let schema_str = serde_json::to_string(&schema)?;
        let id = self.data_batcher.insert("INSERT INTO collections (name, schema) VALUES (?1, ?2)".into(), vec![name.into(), schema_str.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        if let Some(s) = schema { if s.fields.values().any(|f| f.indexed) { self.search.load_index(id, s)?; } }
        Ok(id)
    }

    async fn get_collection(&self, id: i64) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data()?;
        let mut rows = conn.query("SELECT id, name, schema FROM collections WHERE id = ?1", params![id]).await?;
        match rows.next().await? { Some(row) => Ok(Some(row_to_collection(&row)?)), None => Ok(None) }
    }

    async fn list_collections(&self) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data()?;
        let mut rows = conn.query("SELECT id, name, schema FROM collections", ()).await?;
        let mut cols = Vec::new();
        while let Some(row) = rows.next().await? { cols.push(row_to_collection(&row)?); }
        Ok(cols)
    }

    async fn update_collection(&self, id: i64, name: Option<String>, schema: Option<CollectionSchema>) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(n) = name { self.data_batcher.execute("UPDATE collections SET name = ?1 WHERE id = ?2".into(), vec![n.into(), id.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?; }
        if let Some(s) = schema { let s_str = serde_json::to_string(&s)?; self.data_batcher.execute("UPDATE collections SET schema = ?1 WHERE id = ?2".into(), vec![s_str.into(), id.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?; }
        self.get_collection(id).await?.ok_or("Not found".into())
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

    async fn create_record(&self, collection_id: i64, data: &Value) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data()?; 
        let col = self.get_collection(collection_id).await?.ok_or("Collection not found")?;
        let schema = col.schema.unwrap_or_default();
        
        // 1. Serialization (Do once)
        let json_str = serde_json::to_string(data)?;

        // 2. Logic Checks (Must block)
        enforce_uniqueness(&conn, collection_id, None, data, &schema).await?;

        // 3. Batcher INSERT (Fast)
        let record_id = self.data_batcher.insert(
            "INSERT INTO records (collection_id, data) VALUES (?1, jsonb(?2))".into(),
            vec![collection_id.into(), json_str.into()] // Pass pre-serialized string
        ).await?;
        
        // 4. Post-Process (Parallelize)
        // We can run uniqueness commitment and relation syncing concurrently
        let unique_future = commit_uniqueness(&self.data_batcher, collection_id, record_id, data, &schema);
        let relation_future = self.sync_relations(&conn, collection_id, record_id, data);

        // Wait for both, but let them run in parallel
        tokio::try_join!(unique_future, relation_future)?;

        // NOTE: Search Indexing is now handled by the API Layer via JobQueue (Async)
        
        Ok(record_id)
    }

    async fn update_record(&self, collection_id: i64, record_id: i64, data: &Value) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data()?;
        let col = self.get_collection(collection_id).await?.ok_or("Col not found")?;
        let schema = col.schema.unwrap_or_default();
        let existing = self.get_record(collection_id, record_id, None).await?.ok_or("Rec not found")?;
        
        // 1. Data Merging
        let mut merged_data = existing.data.clone();
        if let Some(obj) = merged_data.as_object_mut() {
            if let Some(new_obj) = data.as_object() {
                for (k, v) in new_obj { obj.insert(k.clone(), v.clone()); }
            }
        }
        
        // 2. Serialization
        let json_str = serde_json::to_string(&merged_data)?;

        // 3. Constraints
        enforce_uniqueness(&conn, collection_id, Some(record_id), &merged_data, &schema).await?;

        // 4. Update (Fast)
        self.data_batcher.execute(
            "UPDATE records SET data = jsonb(?1) WHERE collection_id = ?2 AND id = ?3".into(), 
            vec![json_str.into(), collection_id.into(), record_id.into()]
        ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        
        // 5. Post-Process (Parallelize)
        let unique_future = commit_uniqueness(&self.data_batcher, collection_id, record_id, &merged_data, &schema);
        let relation_future = self.sync_relations(&conn, collection_id, record_id, &merged_data);
        
        tokio::try_join!(unique_future, relation_future)?;
        
        // NOTE: Search Indexing is now handled by the API Layer via JobQueue (Async)
        
        Ok(Record { id: record_id, data: merged_data })
    }

    async fn delete_record(&self, collection_id: i64, record_id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
        };

        // 1. Parallelize Deletions
        // Instead of waiting for one SQL execute to finish before sending the next,
        // we fire all of them at the batcher simultaneously. 
        // The batcher queue will handle serialization, but we don't pay the round-trip latency cost 5 times.
        
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
        
        // Vectors are in a different DB file (different batcher), so it runs perfectly in parallel
        let f5 = self.vector_batcher.execute(
            "DELETE FROM vectors WHERE collection_id = ?1 AND record_id = ?2".into(),
            vec![collection_id.into(), record_id.into()]
        );

        // 2. Await all
        // We define Search deletion as non-critical here (handled by API job), 
        // but if we did it here, we'd add it to the join.
        let _ = tokio::try_join!(f1, f2, f3, f4).map_err(map_err)?;
        let _ = f5.await.map_err(map_err)?;

        Ok(())
    }

    // --- Implement New Trait Methods ---
    async fn index_record_search(&self, collection_id: i64, record_id: i64, data: &Value, schema: &CollectionSchema) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        // This is now called by the Background Worker
        self.search.index_record(collection_id, record_id, data, schema).map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn StdError + Send + Sync>)
    }

    async fn delete_record_search(&self, collection_id: i64, record_id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.search.delete_record(collection_id, record_id).map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn StdError + Send + Sync>)
    }

    async fn list_records(&self, collection_id: i64, options: QueryOptions) -> std::result::Result<ListResult, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data()?;
        
        let mut schema_map = HashMap::new();
        let mut id_map = HashMap::new();
        let mut current_col_name = String::new();

        if let Some(ref ex) = options.expand {
            if !ex.trim().is_empty() {
                let all_cols = self.list_collections().await?;
                for c in all_cols {
                    if c.id == collection_id { current_col_name = c.name.clone(); }
                    if let Some(s) = c.schema { 
                        schema_map.insert(c.name.clone(), s.clone()); 
                        schema_map.insert(c.id.to_string(), s); 
                    }
                    id_map.insert(c.name.clone(), c.id); 
                    id_map.insert(c.id.to_string(), c.id);
                }
            }
        }

        let builder = query::SqlBuilder::new(collection_id, &current_col_name, options, &schema_map, &id_map);
        
        // 1. Run Count Query (Clone params because we use them twice)
        let mut count_rows = conn.query(&builder.count_sql, builder.params.clone()).await?;
        let total = if let Some(row) = count_rows.next().await? {
            row.get::<i64>(0)?
        } else { 0 };

        // 2. Run Main Query
        let mut rows = conn.query(&builder.base_sql, builder.params).await?;
        let mut records = Vec::new();
        while let Some(row) = rows.next().await? { records.push(row_to_record(&row)?); }
        
        Ok(ListResult { items: records, total })
    }

    async fn get_record(&self, collection_id: i64, record_id: i64, expand: Option<String>) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data()?;
        
        if expand.is_none() || expand.as_ref().unwrap().trim().is_empty() {
            let mut rows = conn.query("SELECT id, json(data) FROM records WHERE collection_id = ?1 AND id = ?2", params![collection_id, record_id]).await?;
            return match rows.next().await? { Some(row) => Ok(Some(row_to_record(&row)?)), None => Ok(None) };
        }
        let expand_str = expand.unwrap();
        let all_cols = self.list_collections().await?;
        
        // FIX: Ensure variable names match usages below
        let mut schema_map = HashMap::new(); 
        let mut id_map = HashMap::new(); 
        let mut current_col_name = String::new();

        for c in all_cols {
            if c.id == collection_id { current_col_name = c.name.clone(); }
            if let Some(s) = c.schema { 
                // FIX: Use schema_map, not map
                schema_map.insert(c.name.clone(), s.clone()); 
                schema_map.insert(c.id.to_string(), s); 
            }
            // FIX: Use id_map, not ids
            id_map.insert(c.name.clone(), c.id); 
            id_map.insert(c.id.to_string(), c.id);
        }

        let paths = crate::query::smart_split(&expand_str);
        let expanded_json_sql = crate::query::build_recursive_select(paths, "records", 0, &current_col_name, collection_id, &schema_map, &id_map);
        let sql = format!("SELECT id, {} as data FROM records WHERE collection_id = ?1 AND id = ?2", expanded_json_sql);
        let mut rows = conn.query(&sql, params![collection_id, record_id]).await?;
        match rows.next().await? { Some(row) => Ok(Some(row_to_record(&row)?)), None => Ok(None) }
    }

    async fn search_records(&self, collection_id: i64, query: &str) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_search_index(collection_id).await?;
        let ids = self.search.search(collection_id, query, 50)?;
        if ids.is_empty() { return Ok(vec![]); }
        let id_list = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, json(data) FROM records WHERE id IN ({})", id_list);
        let conn = self.get_data()?;
        let mut rows = conn.query(&sql, ()).await?;
        let mut records = Vec::new();
        while let Some(row) = rows.next().await? { records.push(row_to_record(&row)?); }
        Ok(records)
    }

    async fn reindex_collection(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let col = self.get_collection(id).await?.ok_or_else(|| format!("Collection {} not found", id))?; 
        let schema = col.schema.unwrap_or_default();
        
        if !schema.fields.values().any(|f| f.indexed) { return Ok(()); }
        
        // 1. Reset Index
        self.search.delete_index(id).map_err(|e| format!("Search Delete Error: {}", e))?; 
        self.search.load_index(id, &schema).map_err(|e| format!("Search Load Error: {}", e))?;
        
        let conn = self.get_data().map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;
        
        // 2. Stream records
        let mut rows = conn.query("SELECT id, json(data) FROM records WHERE collection_id = ?1", params![id])
            .await
            .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;
            
        // 3. Batching Strategy
        let mut buffer: Vec<(i64, serde_json::Value)> = Vec::with_capacity(1000);
        let batch_size = 1000;

        while let Some(row) = rows.next().await.map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)? {
            let record = row_to_record(&row)?;
            buffer.push((record.id, record.data));

            if buffer.len() >= batch_size {
                // Flush buffer to Tantivy
                self.search.index_batch(id, &buffer, &schema).map_err(|e| format!("Indexing Error: {}", e))?;
                buffer.clear();
            }
        }

        // 4. Flush remaining items
        if !buffer.is_empty() {
            self.search.index_batch(id, &buffer, &schema).map_err(|e| format!("Indexing Error: {}", e))?;
        }

        Ok(())
    }

    async fn instant_search(&self, collection_id: i64, query: &str, limit: usize) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(col) = self.get_collection(collection_id).await? {
            if let Some(schema) = &col.schema {
                if schema.fields.values().any(|f| f.indexed) { self.search.load_index(collection_id, schema)?; } else { return Ok(vec![]); }
            }
        }
        let results = self.search.instant_search(collection_id, query, limit.try_into().unwrap())?;
        Ok(results)
    }

    // --- Core DB (Users, Auth, Settings) ---
    async fn create_user(&self, e: &str, p: &str, r: &str) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core()?;
        conn.execute("INSERT INTO users (email, password_hash, role) VALUES (?1, ?2, ?3)", params![e, p, r]).await?;
        Ok(User { id: conn.last_insert_rowid(), email: e.into(), password_hash: p.into(), role: r.into() })
    }
    async fn get_user_by_email(&self, email: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core()?;
        let mut r = conn.query("SELECT id, email, password_hash, role FROM users WHERE email = ?1", params![email]).await?;
        if let Some(row) = r.next().await? { Ok(Some(User { id: row.get(0)?, email: row.get(1)?, password_hash: row.get(2)?, role: row.get(3)? })) } else { Ok(None) }
    }
    async fn list_users(&self) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core()?;
        let mut rows = conn.query("SELECT id, email, password_hash, role FROM users", ()).await?;
        let mut users = Vec::new();
        while let Some(row) = rows.next().await? { users.push(User { id: row.get(0)?, email: row.get(1)?, password_hash: row.get(2)?, role: row.get(3)? }); }
        Ok(users)
    }
    async fn delete_user(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_core()?.execute("DELETE FROM users WHERE id = ?1", params![id]).await?; Ok(()) }
    async fn get_user_by_oauth(&self, p: &str, pid: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core()?;
        let mut r = conn.query("SELECT u.id, u.email, u.password_hash, u.role FROM users u JOIN auth_identities ai ON u.id = ai.user_id WHERE ai.provider = ?1 AND ai.provider_id = ?2", params![p, pid]).await?;
        if let Some(row) = r.next().await? { Ok(Some(User { id: row.get(0)?, email: row.get(1)?, password_hash: row.get(2)?, role: row.get(3)? })) } else { Ok(None) }
    }
    async fn link_oauth(&self, uid: i64, p: &str, pid: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_core()?.execute("INSERT INTO auth_identities (user_id, provider, provider_id) VALUES (?1, ?2, ?3)", params![uid, p, pid]).await?; Ok(()) }
    async fn create_auth_token(&self, uid: i64, t: &str, tk: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_core()?.execute("INSERT INTO auth_tokens (token, user_id, type, expires_at) VALUES (?1, ?2, ?3, datetime('now', '+1 hour'))", params![tk, uid, t]).await?; Ok(()) }
    async fn consume_auth_token(&self, tk: &str, t: &str) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core()?;
        let mut r = conn.query("SELECT user_id FROM auth_tokens WHERE token = ?1 AND type = ?2 AND expires_at > datetime('now')", params![tk, t]).await?;
        if let Some(row) = r.next().await? { let uid: i64 = row.get(0)?; conn.execute("DELETE FROM auth_tokens WHERE token = ?1", params![tk]).await?; Ok(Some(uid)) } else { Ok(None) }
    }
    async fn set_user_verified(&self, uid: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_core()?.execute("UPDATE users SET is_verified = 1 WHERE id = ?1", params![uid]).await?; Ok(()) }
    async fn set_system_config(&self, k: &str, v: &security::EncryptedValue) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core()?;
        let j = serde_json::to_string(v)?;
        conn.execute("INSERT INTO _system_config (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![k, j]).await?; Ok(())
    }
    async fn get_system_config(&self, k: &str) -> std::result::Result<Option<security::EncryptedValue>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core()?;
        let mut r = conn.query("SELECT value FROM _system_config WHERE key = ?1", params![k]).await?;
        if let Some(row) = r.next().await? { let j: String = row.get(0)?; Ok(Some(serde_json::from_str(&j)?)) } else { Ok(None) }
    }
    async fn get_setting(&self, key: &str) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core()?;
        let mut rows = conn.query("SELECT value FROM _settings WHERE key = ?1", params![key]).await?;
        if let Some(row) = rows.next().await? { let v: String = row.get(0)?; Ok(Some(serde_json::from_str(&v)?)) } else { Ok(None) }
    }
    async fn save_setting(&self, key: &str, value: serde_json::Value, encrypted: bool) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core()?;
        let v_str = serde_json::to_string(&value)?;
        conn.execute("INSERT INTO _settings (key, value, encrypted) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, encrypted=excluded.encrypted, updated_at=CURRENT_TIMESTAMP", params![key, v_str, encrypted]).await?; Ok(())
    }

    // --- Files ---
    async fn create_file_metadata(&self, f: &str, o: &str, m: &str, s: i64, u: Option<i64>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.data_batcher.insert("INSERT INTO _storage_files (filename, original_name, mime_type, size, user_id) VALUES (?1, ?2, ?3, ?4, ?5)".into(), vec![f.into(), o.into(), m.into(), s.into(), u.into()]).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)
    }
    async fn list_files(&self, limit: i64, offset: i64) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data()?;
        let mut rows = conn.query("SELECT id, filename, original_name, mime_type, size, created_at FROM _storage_files ORDER BY created_at DESC LIMIT ?1 OFFSET ?2", params![limit, offset]).await?;
        let mut files = Vec::new();
        while let Some(row) = rows.next().await? { files.push(models::StoredFile { id: row.get(0)?, filename: row.get(1)?, original_name: row.get(2)?, mime_type: row.get(3)?, size: row.get(4)?, created_at: row.get(5)? }); }
        Ok(files)
    }
    async fn get_file_metadata(&self, id: i64) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data()?;
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
        let conn = self.get_log()?;
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
        // Use log_batcher to avoid locking issues
        self.log_batcher.execute(
            "INSERT INTO _system_logs (level, target, message) VALUES (?1, ?2, ?3)".into(),
            vec![level.into(), target.into(), message.into()]
        ).await.map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }

    // --- System DB ---
    async fn list_ai_actions(&self) -> std::result::Result<Vec<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        let mut rows = conn.query("SELECT id, slug, name, model, system_prompt, template, config FROM _ai_actions", ()).await?;
        let mut res = Vec::new();
        while let Some(row) = rows.next().await? {
            let conf_str: String = row.get(6).unwrap_or("{}".to_string());
            res.push(ai_models::AiAction { id: row.get(0)?, slug: row.get(1)?, name: row.get(2)?, model: row.get(3)?, system_prompt: row.get(4)?, template: row.get(5)?, config: serde_json::from_str(&conf_str).unwrap_or_default() });
        }
        Ok(res)
    }
    async fn get_ai_action(&self, slug: &str) -> std::result::Result<Option<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        let mut rows = conn.query("SELECT id, slug, name, model, system_prompt, template, config FROM _ai_actions WHERE slug = ?1", params![slug]).await?;
        if let Some(row) = rows.next().await? {
            let conf_str: String = row.get(6).unwrap_or("{}".to_string());
            Ok(Some(ai_models::AiAction { id: row.get(0)?, slug: row.get(1)?, name: row.get(2)?, model: row.get(3)?, system_prompt: row.get(4)?, template: row.get(5)?, config: serde_json::from_str(&conf_str).unwrap_or_default() }))
        } else { Ok(None) }
    }
    async fn create_ai_action(&self, req: ai_models::CreateActionReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        conn.execute("INSERT INTO _ai_actions (slug, name, model, system_prompt, template, config) VALUES (?1, ?2, ?3, ?4, ?5, '{}')", params![req.slug, req.name, req.model, req.system_prompt, req.template]).await?;
        Ok(conn.last_insert_rowid())
    }
    async fn delete_ai_action(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_sys()?.execute("DELETE FROM _ai_actions WHERE id = ?1", params![id]).await?; Ok(()) }
    async fn create_ai_session(&self, s: &AiSession) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { 
        self.get_sys()?.execute(
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
        let conn = self.get_sys()?;
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
                // NEW FIELDS
                pending_manifest: match pend_str { Some(s) => serde_json::from_str(&s).ok(), None => None },
                diff_summary: row.get(5).unwrap_or(None),
                last_error: row.get(6).unwrap_or(None),
                created_at: row.get(7)? 
            }))
        } else { Ok(None) }
    }

    // 4. UPDATE UPDATE SESSION
    async fn update_ai_session(&self, s: &AiSession) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { 
        self.get_sys()?.execute(
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

    // 5. UPDATE LIST SESSIONS
    async fn list_ai_sessions(&self) -> std::result::Result<Vec<AiSession>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
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
                 // NEW FIELDS
                 pending_manifest: match pend_str { Some(str) => serde_json::from_str(&str).ok(), None => None },
                 diff_summary: row.get(5).unwrap_or(None),
                 last_error: row.get(6).unwrap_or(None),
                 created_at: row.get(7)? 
             });
        }
        Ok(s)
    }
    async fn save_plugin(&self, p: &Plugin) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_sys()?.execute("INSERT INTO _plugins (id, name, version, manifest, description) VALUES (?1, ?2, ?3, ?4, ?5)", params![p.id.clone(), p.name.clone(), p.version.clone(), serde_json::to_string(&p.manifest)?, p.description.clone()]).await?; Ok(()) }
    async fn list_plugins(&self) -> std::result::Result<Vec<Plugin>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        let mut r = conn.query("SELECT id, name, version, manifest, description FROM _plugins ORDER BY created_at DESC", ()).await?;
        let mut p = Vec::new();
        while let Some(row) = r.next().await? { let m_str: String = row.get(3)?; p.push(Plugin { id: row.get(0)?, name: row.get(1)?, version: row.get(2)?, manifest: serde_json::from_str(&m_str)?, description: row.get(4)? }); }
        Ok(p)
    }
    async fn list_scripts(&self) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        // Added target_collection to SELECT
        let mut r = conn.query("SELECT id, name, trigger_type, code, active, target_collection FROM _scripts", ()).await?;
        let mut v = Vec::new();
        while let Some(row) = r.next().await? { 
            v.push(script_models::Script { 
                id: row.get(0)?, 
                name: row.get(1)?, 
                trigger_type: row.get(2)?, 
                code: row.get(3)?, 
                active: row.get(4)?,
                target_collection: row.get(5)? // Added field mapping
            }); 
        }
        Ok(v)
    }

    // 3. Update create_script
    async fn create_script(&self, req: script_models::CreateScriptReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        // Added target_collection to INSERT and UPDATE
        let mut rows = conn.query(
            "INSERT INTO _scripts (name, trigger_type, code, target_collection) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(name) DO UPDATE SET trigger_type=excluded.trigger_type, code=excluded.code, target_collection=excluded.target_collection, created_at=CURRENT_TIMESTAMP RETURNING id", 
            params![req.name, req.trigger_type, req.code, req.target_collection]
        ).await?;
        if let Some(row) = rows.next().await? { Ok(row.get(0)?) } else { Err("Failed".into()) }
    }

    async fn delete_script(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_sys()?.execute("DELETE FROM _scripts WHERE id = ?1", params![id]).await?; Ok(()) }

    // 4. Update get_script_by_name
    async fn get_script_by_name(&self, name: &str) -> std::result::Result<Option<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        // Added target_collection to SELECT
        let mut r = conn.query("SELECT id, name, trigger_type, code, active, target_collection FROM _scripts WHERE name = ?1", params![name]).await?;
        if let Some(row) = r.next().await? { 
            Ok(Some(script_models::Script { 
                id: row.get(0)?, 
                name: row.get(1)?, 
                trigger_type: row.get(2)?, 
                code: row.get(3)?, 
                active: row.get(4)?,
                target_collection: row.get(5)? // Added field mapping
            })) 
        } else { Ok(None) }
    }

    // 5. Update get_scripts_by_trigger
    async fn get_scripts_by_trigger(&self, t: &str) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        // Added target_collection to SELECT
        let mut r = conn.query("SELECT id, name, trigger_type, code, active, target_collection FROM _scripts WHERE trigger_type = ?1 AND active = 1", params![t]).await?;
        let mut v = Vec::new();
        while let Some(row) = r.next().await? { 
            v.push(script_models::Script { 
                id: row.get(0)?, 
                name: row.get(1)?, 
                trigger_type: row.get(2)?, 
                code: row.get(3)?, 
                active: row.get(4)?,
                target_collection: row.get(5)? // Added field mapping
            }); 
        }
        Ok(v)
    }
    async fn list_templates(&self) -> std::result::Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        let mut r = conn.query("SELECT id, slug, content, script_id, created_at FROM _templates", ()).await?;
        let mut v = Vec::new();
        while let Some(row) = r.next().await? { v.push(models::Template { id: row.get(0)?, slug: row.get(1)?, content: row.get(2)?, script_id: row.get(3)?, created_at: row.get(4)? }); }
        Ok(v)
    }
    async fn get_template_by_slug(&self, s: &str) -> std::result::Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        let mut r = conn.query("SELECT id, slug, content, script_id, created_at FROM _templates WHERE slug = ?1", params![s]).await?;
        if let Some(row) = r.next().await? { Ok(Some(models::Template { id: row.get(0)?, slug: row.get(1)?, content: row.get(2)?, script_id: row.get(3)?, created_at: row.get(4)? })) } else { Ok(None) }
    }
    async fn create_template(&self, req: models::CreateTemplateReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys()?;
        let mut rows = conn.query("INSERT INTO _templates (slug, content, script_id) VALUES (?1, ?2, ?3) ON CONFLICT(slug) DO UPDATE SET content=excluded.content, script_id=excluded.script_id, created_at=CURRENT_TIMESTAMP RETURNING id", params![req.slug, req.content, req.script_id]).await?;
        if let Some(row) = rows.next().await? { Ok(row.get(0)?) } else { Err("Failed".into()) }
    }
    async fn update_template(&self, id: i64, content: String, script_id: Option<i64>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_sys()?.execute("UPDATE _templates SET content = ?1, script_id = ?2 WHERE id = ?3", params![content, script_id, id]).await?; Ok(()) }
    async fn delete_template(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { self.get_sys()?.execute("DELETE FROM _templates WHERE id = ?1", params![id]).await?; Ok(()) }

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
        let conn = self.get_data()?;
        let mut rows = conn.query("SELECT target_col_id, target_rec_id FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND rel_name=?3", params![oc, oi, rn]).await?;
        let mut results = Vec::new(); while let Some(row) = rows.next().await? { results.push((row.get(0)?, row.get(1)?)); } Ok(results)
    }
    async fn get_records_by_ids(&self, collection_id: i64, ids: &[i64]) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        if ids.is_empty() { return Ok(vec![]); }
        let id_list = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, json(data) FROM records WHERE collection_id = ? AND id IN ({})", id_list);
        let conn = self.get_data()?;
        let mut rows = conn.query(&sql, params![collection_id]).await?;
        let mut records = Vec::new();
        while let Some(row) = rows.next().await? { records.push(row_to_record(&row)?); }
        Ok(records)
    }

    // --- Dashboard ---
    async fn get_dashboard_stats(&self) -> std::result::Result<DashboardData, Box<dyn std::error::Error + Send + Sync>> {
        let data_conn = self.get_data()?;
        let log_conn = self.get_log()?;
        let sys_conn = self.get_sys()?;

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
    async fn save_vector(&self, collection_id: i64, record_id: i64, field_name: &str, vector: Vec<f32>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let vec_json = serde_json::to_string(&vector)?;
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
        };
        
        // 1. Delete existing doc for this field
        self.vector_batcher.execute(
            "DELETE FROM vectors WHERE collection_id=?1 AND record_id=?2 AND field_name=?3".into(),
            vec![collection_id.into(), record_id.into(), field_name.into()]
        ).await.map_err(map_err)?;
        
        // 2. Insert new vector
        self.vector_batcher.insert(
            "INSERT INTO vectors (collection_id, record_id, field_name, vector) VALUES (?1, ?2, ?3, ?4)".into(),
            vec![collection_id.into(), record_id.into(), field_name.into(), vec_json.into()]
        ).await.map_err(map_err)?;

        Ok(())
    }

    // VECTORS: Implement get_vectors_for_collection
    async fn get_vectors_for_collection(&self, collection_id: i64) -> std::result::Result<Vec<(i64, String, Vec<f32>)>, Box<dyn std::error::Error + Send + Sync>> {
        // FIX: Corrected method call to self.get_vector()?
        let conn = self.get_vector()?; 
        let mut rows = conn.query(
            "SELECT record_id, field_name, vector FROM vectors WHERE collection_id = ?1", 
            params![collection_id]
        ).await?;
        
        let mut vectors = Vec::new();
        while let Some(row) = rows.next().await? {
            let record_id: i64 = row.get(0)?;
            let field_name: String = row.get(1)?;
            let vector_json_str: String = row.get(2)?;
            
            // Deserialize the vector JSON string
            let vector: Vec<f32> = serde_json::from_str(&vector_json_str)?;
            
            vectors.push((record_id, field_name, vector));
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

// DUMMY Provider for Core initialization (since Core can't depend on ApexVector)
#[allow(dead_code)] // Allow dead code on this struct since it's a fallback
struct NoOpVectorProvider;
#[async_trait]
impl VectorProvider for NoOpVectorProvider {
    async fn embed(&self, _text: &str) -> std::result::Result<Vec<f32>, String> { Err("Vector search not enabled in core".into()) }
    async fn search(&self, _c: i64, _f: &str, _v: &[f32], _l: usize) -> std::result::Result<Vec<(i64, f32)>, String> { Ok(vec![]) }
    async fn index(&self, _c: i64, _r: i64, _f: &str, _v: &[f32]) -> std::result::Result<(), String> { Ok(()) }
}

// --- Constructor ---
pub async fn a_new_database_connection(vector_provider: Arc<dyn VectorProvider>) -> Result<ApexKit> {
    let core = Builder::new_local("core.db").build().await?;
    let data = Builder::new_local("data.db").build().await?;
    let log = Builder::new_local("logs.db").build().await?;
    let sys = Builder::new_local("system.db").build().await?;
    let vec = Builder::new_local("vectors.db").build().await?;

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

    // Pass the provider to ApexKit::new
    Ok(ApexKit::new(
        Arc::new(core), 
        Arc::new(data), 
        Arc::new(log), 
        Arc::new(sys), 
        Arc::new(vec), 
        vector_provider
    ))
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
    conn.execute("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, role TEXT NOT NULL, is_verified BOOLEAN DEFAULT 0)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS auth_identities (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL, provider TEXT NOT NULL, provider_id TEXT NOT NULL)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS auth_tokens (token TEXT PRIMARY KEY, user_id INTEGER NOT NULL, type TEXT NOT NULL, expires_at DATETIME NOT NULL)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _system_config (key TEXT PRIMARY KEY, value TEXT NOT NULL)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, encrypted BOOLEAN DEFAULT 0, updated_at DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    Ok(())
}

async fn setup_data(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    conn.execute("CREATE TABLE IF NOT EXISTS collections (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, schema JSON)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS records (id INTEGER PRIMARY KEY AUTOINCREMENT, collection_id INTEGER NOT NULL, data JSONB NOT NULL)", ()).await?;
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
    // Audit Logs (Business Logic)
    conn.execute("CREATE TABLE IF NOT EXISTS _audit_logs (id INTEGER PRIMARY KEY AUTOINCREMENT, level TEXT NOT NULL, message TEXT NOT NULL, source TEXT NOT NULL, meta JSON, timestamp DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;
    // System Logs (Technical/Debug Logs from Tracing)
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

// --- TABLE SETUP ---
async fn setup_vectors(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    // UPDATED SCHEMA: Links vector to specific data point
    conn.execute(
        "CREATE TABLE IF NOT EXISTS vectors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            collection_id INTEGER NOT NULL,
            record_id INTEGER NOT NULL,
            field_name TEXT NOT NULL,
            vector BLOB NOT NULL, -- JSON String of float array
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(collection_id, record_id, field_name) -- One vector per field per record
        )", 
        ()
    ).await?;
    
    // Index for fast lookups/deletion
    conn.execute("CREATE INDEX IF NOT EXISTS idx_vec_record ON vectors(record_id)", ()).await?;
    Ok(())
}

// --- Mock Implementation ---
#[async_trait]
impl Db for Mutex<Connection> {
    async fn create_collection(&self, _n: &str, _s: &Option<CollectionSchema>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn get_collection(&self, _id: i64) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn list_collections(&self) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn update_collection(&self, _id: i64, _n: Option<String>, _s: Option<CollectionSchema>) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> { Err("Mock".into()) }
    async fn delete_collection(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn create_record(&self, _c: i64, _d: &Value) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn list_records(&self, _c: i64, _o: QueryOptions) -> std::result::Result<ListResult, Box<dyn std::error::Error + Send + Sync>> { Ok(ListResult{ items: vec![], total: 0 }) }
    async fn get_record(&self, _c: i64, _r: i64, _e: Option<String>) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn update_record(&self, _c: i64, _r: i64, _d: &Value) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> { Err("Mock".into()) }
    async fn delete_record(&self, _c: i64, _r: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn search_records(&self, _c: i64, _q: &str) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn instant_search(&self, _c: i64, _q: &str, _l: usize) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn index_record_search(&self, _c: i64, _r: i64, _d: &serde_json::Value, _s: &crate::schema::CollectionSchema
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { 
        Ok(()) 
    }
    async fn delete_record_search(&self, _c: i64, _r: i64
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { 
        Ok(()) 
    }
    async fn reindex_collection(&self, _id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> { Ok(()) }
    async fn create_user(&self, _e: &str, _p: &str, _r: &str) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>> { Err("Mock".into()) }
    async fn get_user_by_email(&self, _e: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn list_users(&self) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn delete_user(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn create_file_metadata(&self, _f: &str, _o: &str, _m: &str, _s: i64, _u: Option<i64>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn list_files(&self, _l: i64, _o: i64) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn get_file_metadata(&self, _id: i64) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn delete_file_metadata(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn get_user_by_oauth(&self, _p: &str, _pid: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn link_oauth(&self, _u: i64, _p: &str, _pid: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn create_auth_token(&self, _u: i64, _t: &str, _tk: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn consume_auth_token(&self, _t: &str, _tt: &str) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn set_user_verified(&self, _u: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn set_system_config(&self, _k: &str, _v: &security::EncryptedValue) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn get_system_config(&self, _k: &str) -> std::result::Result<Option<security::EncryptedValue>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn get_setting(&self, _k: &str) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn save_setting(&self, _k: &str, _v: serde_json::Value, _e: bool) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn log_audit_event(&self, _l: &str, _m: &str, _s: &str, _meta: Option<serde_json::Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn list_audit_logs(&self) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn log_system_event(&self, _level: &str, _target: &str, _message: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn list_ai_actions(&self) -> std::result::Result<Vec<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn get_ai_action(&self, _slug: &str) -> std::result::Result<Option<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn create_ai_action(&self, _a: ai_models::CreateActionReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn delete_ai_action(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn create_ai_session(&self, _s: &AiSession) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn get_ai_session(&self, _id: &str) -> std::result::Result<Option<AiSession>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn update_ai_session(&self, _s: &AiSession) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn list_ai_sessions(&self) -> std::result::Result<Vec<AiSession>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn save_plugin(&self, _p: &Plugin) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn list_plugins(&self) -> std::result::Result<Vec<Plugin>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn create_relation(&self, _oc: i64, _oi: i64, _tc: i64, _ti: i64, _rn: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn delete_relation(&self, _oc: i64, _oi: i64, _tc: i64, _ti: i64, _rn: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn get_related_ids(&self, _oc: i64, _oi: i64, _rn: &str) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn get_records_by_ids(&self, _c: i64, _i: &[i64]) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn list_scripts(&self) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn create_script(&self, _r: script_models::CreateScriptReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn delete_script(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn get_script_by_name(&self, _n: &str) -> std::result::Result<Option<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn get_scripts_by_trigger(&self, _t: &str) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn list_templates(&self) -> std::result::Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn get_template_by_slug(&self, _slug: &str) -> std::result::Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn create_template(&self, _req: models::CreateTemplateReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn update_template(&self, _id: i64, _c: String, _s: Option<i64>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn delete_template(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn get_dashboard_stats(&self) -> std::result::Result<DashboardData, Box<dyn std::error::Error + Send + Sync>> {
        Ok(DashboardData { stats: DashboardStats { total_requests: 0, db_size_mb: 0.0, collections_count: 0, total_records: 0 }, chart: vec![], recent_logs: vec![] })
    }
    async fn search_vector(&self, _c: i64, _f: &str, _v: Vec<f32>, _l: usize) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn save_vector(&self, _collection_id: i64, _record_id: i64, _field_name: &str, _vector: Vec<f32>) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> { Ok(()) }
    async fn get_vectors_for_collection(&self, _c: i64) -> std::result::Result<Vec<(i64, String, Vec<f32>)>, Box<dyn StdError + Send + Sync>> { Ok(vec![]) }
}