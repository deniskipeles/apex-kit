use crate::auth::User;
use crate::models;
use crate::models::schema::CollectionSchema;
use crate::models::{AiSession, Plugin};
use crate::models::{ApiKey, DashboardData};
use crate::models::{Collection, ListResult, Record};
use crate::query::ApexQuery;
use crate::query::QueryOptions;
use async_trait::async_trait;
use serde_json::Value;
use std::error::Error as StdError;

// Helpers to quickly map rust types to rusqlite values
pub trait IntoSqlVal {
    fn into_val(self) -> rusqlite::types::Value;
}
impl IntoSqlVal for String {
    fn into_val(self) -> rusqlite::types::Value {
        rusqlite::types::Value::Text(self)
    }
}
impl IntoSqlVal for &str {
    fn into_val(self) -> rusqlite::types::Value {
        rusqlite::types::Value::Text(self.to_string())
    }
}
impl IntoSqlVal for i64 {
    fn into_val(self) -> rusqlite::types::Value {
        rusqlite::types::Value::Integer(self)
    }
}
impl IntoSqlVal for Option<i64> {
    fn into_val(self) -> rusqlite::types::Value {
        match self {
            Some(v) => rusqlite::types::Value::Integer(v),
            None => rusqlite::types::Value::Null,
        }
    }
}
impl IntoSqlVal for bool {
    fn into_val(self) -> rusqlite::types::Value {
        rusqlite::types::Value::Integer(if self { 1 } else { 0 })
    }
}
impl IntoSqlVal for Option<bool> {
    fn into_val(self) -> rusqlite::types::Value {
        match self {
            Some(b) => rusqlite::types::Value::Integer(if b { 1 } else { 0 }),
            None => rusqlite::types::Value::Null,
        }
    }
}
impl IntoSqlVal for Option<String> {
    fn into_val(self) -> rusqlite::types::Value {
        match self {
            Some(v) => rusqlite::types::Value::Text(v),
            None => rusqlite::types::Value::Null,
        }
    }
}

#[async_trait]
pub trait VectorProvider: Send + Sync {
    async fn embed(&self, text: &str) -> std::result::Result<Vec<f32>, String>;
    async fn embed_image(&self, base64_image: &str) -> std::result::Result<Vec<f32>, String>;
    async fn search(
        &self,
        col_id: i64,
        field: &str,
        vec: &[f32],
        limit: usize,
    ) -> std::result::Result<Vec<(i64, f32)>, String>;
    async fn index(
        &self,
        col_id: i64,
        rec_id: i64,
        field: &str,
        vec: &[f32],
    ) -> std::result::Result<(), String>;
}

#[async_trait]
pub trait Db: Send + Sync {
    // --- Collections ---
    async fn create_collection(
        &self,
        name: &str,
        schema: &Option<CollectionSchema>,
        index: Option<String>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_collection(
        &self,
        id: i64,
    ) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>>;
    async fn list_collections(
        &self,
    ) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_collection(
        &self,
        id: i64,
        name: Option<String>,
        schema: Option<CollectionSchema>,
    ) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_collection(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Records ---
    async fn create_record(
        &self,
        collection_id: i64,
        data: &Value,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn import_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_records(
        &self,
        collection_id: i64,
        options: QueryOptions,
    ) -> std::result::Result<ListResult, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
        expand: Option<String>,
    ) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_record(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Search ---
    async fn search_records(
        &self,
        collection_id: i64,
        query: &str,
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;
    async fn reindex_collection(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
    async fn instant_search(
        &self,
        collection_id: i64,
        query: &str,
        limit: usize,
    ) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>>;
    async fn recover_indexes(
        &self,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn index_record_search(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
        schema: &CollectionSchema,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
    async fn delete_record_search(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    // --- Users (Auth) ---
    async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        role: &str,
        metadata: Option<Value>,
    ) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>>;

    // Tenants
    async fn register_tenant(
        &self,
        id: &str,
        owner_id: Option<i64>,
        name: Option<String>,
        tier: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_tenant_status(
        &self,
        tenant_id: &str,
    ) -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_tenant_status(
        &self,
        tenant_id: &str,
        status: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn update_tenant_full(
        &self,
        id: &str,
        name: Option<String>,
        status: Option<String>,
        tier: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_tenant_metadata(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_tenants(
        &self,
    ) -> std::result::Result<Vec<models::Tenant>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_tenant_disk_usage(
        &self,
        tenant_id: &str,
    ) -> std::result::Result<u64, Box<dyn std::error::Error + Send + Sync>>;

    // Sandbox Management
    async fn register_sandbox(
        &self,
        id: &str,
        owner_id: Option<i64>,
        name: Option<String>,
        expires_at: Option<String>,
        scope: &str,
        tenant_id: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_sandboxes(
        &self,
        tenant_id: Option<String>,
    ) -> std::result::Result<
        Vec<crate::models::SandboxMetadata>,
        Box<dyn std::error::Error + Send + Sync>,
    >;
    async fn update_sandbox_full(
        &self,
        id: &str,
        name: Option<String>,
        status: Option<String>,
        expires_at: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_sandbox_metadata(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_sandbox_disk_usage(
        &self,
        sandbox_id: &str,
    ) -> std::result::Result<u64, Box<dyn std::error::Error + Send + Sync>>;

    async fn import_user(
        &self,
        id: i64,
        email: &str,
        password_hash: &str,
        role: &str,
        metadata: Option<Value>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn list_users(
        &self,
        query: Option<String>,
        limit: i64,
        offset: i64,
    ) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn count_users(
        &self,
        query: Option<String>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_users_by_ids(
        &self,
        ids: &[i64],
    ) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_user(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn update_user(
        &self,
        id: i64,
        email: Option<String>,
        role: Option<String>,
        metadata: Option<serde_json::Value>,
        password: Option<String>,
    ) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>>;

    // --- Storage Metadata ---
    async fn create_file_metadata(
        &self,
        filename: &str,
        original_name: &str,
        mime_type: &str,
        size: i64,
        user_id: Option<i64>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn list_files(
        &self,
        limit: i64,
        offset: i64,
    ) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;
    async fn count_files(
        &self,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_file_metadata(
        &self,
        id: i64,
    ) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_file_by_filename(
        &self,
        filename: &str,
    ) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_file_metadata(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Advanced Auth ---
    async fn get_user_by_oauth(
        &self,
        provider: &str,
        provider_id: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn link_oauth(
        &self,
        user_id: i64,
        provider: &str,
        provider_id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn create_auth_token(
        &self,
        user_id: i64,
        token_type: &str,
        token: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn consume_auth_token(
        &self,
        token: &str,
        token_type: &str,
    ) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>>;
    async fn set_user_verified(
        &self,
        user_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Secure Config ---
    async fn get_config(
        &self,
        key: &str,
    ) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;
    async fn set_config(
        &self,
        key: &str,
        value: &serde_json::Value,
        encrypted: bool,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_configs(
        &self,
    ) -> std::result::Result<Vec<models::ConfigItem>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_config(
        &self,
        key: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Audit Logs ---
    async fn log_audit_event(
        &self,
        level: &str,
        message: &str,
        source: &str,
        meta: Option<serde_json::Value>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_audit_logs(
        &self,
    ) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;
    async fn log_system_event(
        &self,
        level: &str,
        target: &str,
        message: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // [NEW] Dynamically query system or audit databases
    async fn list_paginated_logs(
        &self,
        log_type: &str, // "system" or "audit"
        page: i64,
        per_page: i64,
        level: Option<String>,
        source: Option<String>,
        search: Option<String>,
    ) -> std::result::Result<(Vec<serde_json::Value>, i64), Box<dyn std::error::Error + Send + Sync>>;

    // --- API keys ---
    async fn create_api_key(
        &self,
        name: &str,
        tenant_id: &str,
        issuer: &str,
        env_type: &str,
        roles: Vec<String>,
        bypass_cors: bool,
    ) -> std::result::Result<(String, ApiKey), Box<dyn std::error::Error + Send + Sync>>;
    async fn update_api_key(
        &self,
        id: i64,
        name: Option<String>,
        status: Option<String>,
        roles: Option<Vec<String>>,
        bypass_cors: Option<bool>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_api_keys(
        &self,
    ) -> std::result::Result<Vec<ApiKey>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_api_key(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn verify_api_key(
        &self,
        tenant_id: &str,
        key_id: &str,
        secret: &str,
    ) -> std::result::Result<Option<ApiKey>, Box<dyn std::error::Error + Send + Sync>>;

    // --- AI Actions ---
    async fn list_ai_actions(
        &self,
    ) -> std::result::Result<
        Vec<crate::models::ai::AiAction>,
        Box<dyn std::error::Error + Send + Sync>,
    >;
    async fn get_ai_action(
        &self,
        slug: &str,
    ) -> std::result::Result<
        Option<crate::models::ai::AiAction>,
        Box<dyn std::error::Error + Send + Sync>,
    >;
    async fn create_ai_action(
        &self,
        action: crate::models::ai::CreateActionReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_ai_action(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- AI Sessions ---
    async fn create_ai_session(
        &self,
        session: &AiSession,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_ai_session(
        &self,
        id: &str,
    ) -> std::result::Result<Option<AiSession>, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_ai_session(
        &self,
        session: &AiSession,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_ai_sessions(
        &self,
    ) -> std::result::Result<Vec<AiSession>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_ai_session(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    // --- Plugins ---
    async fn save_plugin(
        &self,
        plugin: &Plugin,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_plugins(
        &self,
    ) -> std::result::Result<Vec<Plugin>, Box<dyn std::error::Error + Send + Sync>>;

    // --- Relations ---
    async fn create_relation(
        &self,
        origin_col: i64,
        origin_id: i64,
        target_col: i64,
        target_id: i64,
        rel_name: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_relation(
        &self,
        origin_col: i64,
        origin_id: i64,
        target_col: i64,
        target_id: i64,
        rel_name: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_related_ids(
        &self,
        origin_col: i64,
        origin_id: i64,
        rel_name: &str,
    ) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_records_by_ids(
        &self,
        collection_id: i64,
        ids: &[i64],
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;

    // Scripts
    async fn list_scripts(
        &self,
    ) -> std::result::Result<
        Vec<crate::models::script::Script>,
        Box<dyn std::error::Error + Send + Sync>,
    >;
    async fn create_script(
        &self,
        req: crate::models::script::CreateScriptReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_script(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_script_by_name(
        &self,
        name: &str,
    ) -> std::result::Result<
        Option<crate::models::script::Script>,
        Box<dyn std::error::Error + Send + Sync>,
    >;
    async fn get_scripts_by_trigger(
        &self,
        trigger: &str,
    ) -> std::result::Result<
        Vec<crate::models::script::Script>,
        Box<dyn std::error::Error + Send + Sync>,
    >;

    // Templates
    async fn list_templates(
        &self,
    ) -> std::result::Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_template_by_slug(
        &self,
        slug: &str,
    ) -> std::result::Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>>;
    async fn create_template(
        &self,
        req: models::CreateTemplateReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_template(
        &self,
        id: i64,
        content: String,
        script_id: Option<i64>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_template(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- DASHBOARD METHODS ---
    async fn get_dashboard_stats(
        &self,
    ) -> std::result::Result<DashboardData, Box<dyn std::error::Error + Send + Sync>>;

    // VECTORS
    async fn get_vectors_for_collection(
        &self,
        collection_id: i64,
        model: &str,
    ) -> std::result::Result<Vec<(i64, String, Vec<f32>)>, Box<dyn StdError + Send + Sync>>;
    async fn search_vector(
        &self,
        collection_id: i64,
        field: &str,
        vector: Vec<f32>,
        limit: usize,
    ) -> std::result::Result<Vec<(Record, f32)>, Box<dyn std::error::Error + Send + Sync>>;
    async fn save_vector(
        &self,
        collection_id: i64,
        record_id: i64,
        field_name: &str,
        vector: Vec<f32>,
        model: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
    async fn has_vector(
        &self,
        collection_id: i64,
        record_id: i64,
        field_name: &str,
        model: &str,
    ) -> std::result::Result<bool, Box<dyn StdError + Send + Sync>>;
    async fn get_record_vectors(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<Vec<models::VectorRecord>, Box<dyn StdError + Send + Sync>>;

    // QUERY ENGINE
    async fn query_engine(
        &self,
        query: ApexQuery,
    ) -> std::result::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>>;

    // [NEW] Forces SQLite to drop open connections and re-read the physical DB files
    async fn reload_connections(
        &self,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
