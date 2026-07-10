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

/// Helper trait to simplify mapping common Rust types into rusqlite database values.
pub trait IntoSqlVal {
    /// Converts the implementing type into a native SQLite value representation.
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

impl IntoSqlVal for f64 {
    fn into_val(self) -> rusqlite::types::Value {
        rusqlite::types::Value::Real(self)
    }
}

impl IntoSqlVal for Option<f64> {
    fn into_val(self) -> rusqlite::types::Value {
        match self {
            Some(v) => rusqlite::types::Value::Real(v),
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

/// Abstract provider handling local or remote vector generation and calculations.
#[async_trait]
pub trait VectorProvider: Send + Sync {
    /// Generates high-dimensional numeric embeddings for a clean text sequence.
    async fn embed(&self, text: &str) -> std::result::Result<Vec<f32>, String>;

    /// Generates vector embeddings for a given base64 encoded image resource.
    async fn embed_image(&self, base64_image: &str) -> std::result::Result<Vec<f32>, String>;

    async fn embed_text_for_image_search(
        &self,
        text: &str,
    ) -> std::result::Result<Vec<f32>, String>;

    /// Searches for closest vectors using approximate or exact nearest neighbor matches.
    async fn search(
        &self,
        col_id: i64,
        field: &str,
        vec: &[f32],
        limit: usize,
    ) -> std::result::Result<Vec<(i64, f32)>, String>;

    /// Inserts or updates an individual vector record directly into the index layout.
    async fn index(
        &self,
        col_id: i64,
        rec_id: i64,
        field: &str,
        vec: &[f32],
    ) -> std::result::Result<(), String>;

    /// Returns the number of embedding requests since last check, and resets the counter.
    fn get_and_reset_metrics(&self) -> u64 {
        0
    }
}

/// Operations for registering, retrieving, and organizing data schemas and tables.
#[async_trait]
pub trait CollectionStore: Send + Sync {
    /// Registers a new collection matching the given name and optional validation schema.
    async fn create_collection(
        &self,
        name: &str,
        schema: &Option<CollectionSchema>,
        index: Option<String>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    /// Looks up a specific collection's structural properties using its database identifier.
    async fn get_collection(
        &self,
        id: i64,
    ) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>>;

    /// Returns all registered collections in the current database context.
    async fn list_collections(
        &self,
    ) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>>;

    /// Updates existing collection parameters and migrates schemas or renamed constraints safely.
    async fn update_collection(
        &self,
        id: i64,
        name: Option<String>,
        schema: Option<CollectionSchema>,
    ) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>>;

    /// Removes a collection and cleans up its entries, relationships, and index allocations.
    async fn delete_collection(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for inserting, modifying, and paginating individual record objects.
#[async_trait]
pub trait RecordStore: Send + Sync {
    /// Inserts a newly validated record object into the specified target collection.
    async fn create_record(
        &self,
        collection_id: i64,
        data: &Value,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    /// Direct bulk insertion bypassing trigger validations (commonly for restores/migrations).
    async fn import_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Lists and paginates records matching the query parameters and dynamic filters.
    async fn list_records(
        &self,
        collection_id: i64,
        options: QueryOptions,
    ) -> std::result::Result<ListResult, Box<dyn std::error::Error + Send + Sync>>;

    /// Retrieves an individual record with support for recursively fetching linked relations.
    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
        expand: Option<String>,
    ) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>>;

    /// Merges update values, verifies constraint rules, and commits modified attributes.
    async fn update_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>>;

    /// Deletes a record while safely removing orphaned unique or relationship references.
    async fn delete_record(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for full-text search indexing and synchronization.
#[async_trait]
pub trait SearchStore: Send + Sync {
    /// Purges and regenerates the entire search index for a specified target collection.
    async fn reindex_collection(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Performs low-latency matching, generating prefix snippets and document statistics.
    async fn instant_search(
        &self,
        collection_id: i64,
        query: &str,
        limit: usize,
    ) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>>;

    /// Audits document totals and repairs index-to-database desynchronizations.
    async fn recover_indexes(
        &self,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Queues or immediately indexes the text representation of a record.
    async fn index_record_search(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
        schema: &CollectionSchema,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Removes an object's searchable keys from index allocation files.
    async fn delete_record_search(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
}

/// Store operations for user profile metadata and local administrative credentials.
#[async_trait]
pub trait UserStore: Send + Sync {
    /// Generates a user registry profile.
    async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        role: &str,
        metadata: Option<Value>,
    ) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>>;

    /// Directly registers user profiles specifying structural primary key overrides.
    async fn import_user(
        &self,
        id: i64,
        email: &str,
        password_hash: &str,
        role: &str,
        metadata: Option<Value>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Resolves individual accounts matching registered email credentials.
    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;

    /// Queries user listings utilizing pagination parameters and custom queries.
    async fn list_users(
        &self,
        query: Option<String>,
        limit: i64,
        offset: i64,
    ) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>>;

    /// Aggregates database totals of user accounts matching an optional query filter.
    async fn count_users(
        &self,
        query: Option<String>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    /// Resolves multiple user records matching a slice of user ID keys.
    async fn get_users_by_ids(
        &self,
        ids: &[i64],
    ) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>>;

    /// Deletes a user profile and related authorization data.
    async fn delete_user(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Updates field attributes or credentials on an existing user record.
    async fn update_user(
        &self,
        id: i64,
        email: Option<String>,
        role: Option<String>,
        metadata: Option<serde_json::Value>,
        password: Option<String>,
    ) -> std::result::Result<User, Box<dyn StdError + Send + Sync>>;
}

/// Store operations for managing external OAuth registration and mapping.
#[async_trait]
pub trait OAuthStore: Send + Sync {
    /// Looks up linked user records matching external OAuth provider variables.
    async fn get_user_by_oauth(
        &self,
        provider: &str,
        provider_id: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;

    /// Links an OAuth provider identity to an existing system user ID.
    async fn link_oauth(
        &self,
        user_id: i64,
        provider: &str,
        provider_id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for transient authentication tokens and verification lifecycles.
#[async_trait]
pub trait AuthTokenStore: Send + Sync {
    /// Creates a transient authentication token with explicit expiration attributes.
    async fn create_auth_token(
        &self,
        user_id: i64,
        token_type: &str,
        token: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Evaluates, purges, and resolves active verification tokens.
    async fn consume_auth_token(
        &self,
        token: &str,
        token_type: &str,
    ) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>>;

    /// Updates verification state flag to true for a user.
    async fn set_user_verified(
        &self,
        user_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for multitenancy settings and storage usage tracking.
#[async_trait]
pub trait TenantStore: Send + Sync {
    /// Registers a new isolated database tenant mapping.
    async fn register_tenant(
        &self,
        id: &str,
        owner_id: Option<i64>,
        name: Option<String>,
        tier: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Retrieves status state flag for a specific tenant.
    async fn get_tenant_status(
        &self,
        tenant_id: &str,
    ) -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>>;

    /// Updates tenant status variables.
    async fn update_tenant_status(
        &self,
        tenant_id: &str,
        status: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Updates tenant attributes, including structural tier levels and limits.
    async fn update_tenant_full(
        &self,
        id: &str,
        name: Option<String>,
        status: Option<String>,
        tier: Option<String>,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Removes tenant metadata reference objects.
    async fn delete_tenant_metadata(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Returns a list of all tenants registered in core databases.
    async fn list_tenants(
        &self,
    ) -> std::result::Result<Vec<models::Tenant>, Box<dyn StdError + Send + Sync>>;

    /// Estimates file sizes allocated on disk for an isolated tenant space.
    async fn get_tenant_disk_usage(
        &self,
        tenant_id: &str,
    ) -> std::result::Result<u64, Box<dyn StdError + Send + Sync>>;

    /// Updates the aggregated statistics for a tenant.
    async fn update_tenant_stats(
        &self,
        tenant_id: &str,
        storage_mb: f64,
        vectors: i64,
        ai_requests: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
}

/// Store operations for managing ephemeral sandbox lifecycles.
#[async_trait]
pub trait SandboxStore: Send + Sync {
    /// Registers transient sandbox execution configurations.
    async fn register_sandbox(
        &self,
        id: &str,
        owner_id: Option<i64>,
        name: Option<String>,
        expires_at: Option<String>,
        scope: &str,
        tenant_id: Option<String>,
        max_storage_mb: i64,
        max_vectors: i64,
        max_ai_requests: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Lists active workspaces matching context filters.
    async fn list_sandboxes(
        &self,
        tenant_id: Option<String>,
    ) -> std::result::Result<
        Vec<crate::models::SandboxMetadata>,
        Box<dyn std::error::Error + Send + Sync>,
    >;

    /// Updates structural parameters, timelines, and bounds of a sandbox.
    async fn update_sandbox_full(
        &self,
        id: &str,
        name: Option<String>,
        status: Option<String>,
        expires_at: Option<String>,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Removes sandbox tracking profiles and credentials.
    async fn delete_sandbox_metadata(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Evaluates bytes utilized on disk inside ephemeral directories.
    async fn get_sandbox_disk_usage(
        &self,
        sandbox_id: &str,
    ) -> std::result::Result<u64, Box<dyn StdError + Send + Sync>>;

    /// Updates the aggregated statistics for a sandbox.
    async fn update_sandbox_stats(
        &self,
        sandbox_id: &str,
        storage_mb: f64,
        vectors: i64,
        ai_requests: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
}

/// Store operations for physical storage assets metadata.
#[async_trait]
pub trait FileStore: Send + Sync {
    /// Registers metadata information for an uploaded file entry.
    async fn create_file_metadata(
        &self,
        filename: &str,
        original_name: &str,
        mime_type: &str,
        size: i64,
        user_id: Option<i64>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    /// Queries paginated listings of files.
    async fn list_files(
        &self,
        limit: i64,
        offset: i64,
    ) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;

    /// Counts files entries registered in system storage tables.
    async fn count_files(
        &self,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    /// Looks up asset metadata using its database identifier.
    async fn get_file_metadata(
        &self,
        id: i64,
    ) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;

    /// Looks up asset metadata using its unique generated file key.
    async fn get_file_by_filename(
        &self,
        filename: &str,
    ) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;

    /// Removes storage asset registry references.
    async fn delete_file_metadata(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
}

/// Store operations for runtime configuration parameters and system keys.
#[async_trait]
pub trait ConfigStore: Send + Sync {
    /// Resolves an individual configuration value matching the designated key.
    async fn get_config(
        &self,
        key: &str,
    ) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;

    /// Sets or modifies configuration properties with support for data encryption.
    async fn set_config(
        &self,
        key: &str,
        value: &serde_json::Value,
        encrypted: bool,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Returns all registered configuration items.
    async fn list_configs(
        &self,
    ) -> std::result::Result<Vec<models::ConfigItem>, Box<dyn std::error::Error + Send + Sync>>;

    /// Clears a system configuration parameter.
    async fn delete_config(
        &self,
        key: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for tracking event changes and auditing actions.
#[async_trait]
pub trait AuditStore: Send + Sync {
    /// Submits a persistent security audit event log with structural metadata.
    async fn log_audit_event(
        &self,
        level: &str,
        message: &str,
        source: &str,
        meta: Option<serde_json::Value>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Gathers structural logs tracking administrative activities.
    async fn list_audit_logs(
        &self,
    ) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;

    /// Logs system execution signals.
    async fn log_system_event(
        &self,
        level: &str,
        target: &str,
        message: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Retrieves paginated log lists with structural searching, levels, and source filtering.
    async fn list_paginated_logs(
        &self,
        log_type: &str,
        page: i64,
        per_page: i64,
        level: Option<String>,
        source: Option<String>,
        search: Option<String>,
    ) -> std::result::Result<(Vec<serde_json::Value>, i64), Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for API authorization credentials.
#[async_trait]
pub trait ApiKeyStore: Send + Sync {
    /// Generates API key credentials and stores verified cryptohash properties.
    async fn create_api_key(
        &self,
        name: &str,
        tenant_id: &str,
        issuer: &str,
        env_type: &str,
        roles: Vec<String>,
        bypass_cors: bool,
    ) -> std::result::Result<(String, ApiKey), Box<dyn std::error::Error + Send + Sync>>;

    /// Edits an API key's details, active state flags, or authorized roles.
    async fn update_api_key(
        &self,
        id: i64,
        name: Option<String>,
        status: Option<String>,
        roles: Option<Vec<String>>,
        bypass_cors: Option<bool>,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Lists all registered API Key structures.
    async fn list_api_keys(
        &self,
    ) -> std::result::Result<Vec<ApiKey>, Box<dyn std::error::Error + Send + Sync>>;

    /// Permanently revokes an API key.
    async fn delete_api_key(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Cryptographically validates an API key and evaluates status variables.
    async fn verify_api_key(
        &self,
        tenant_id: &str,
        key_id: &str,
        secret: &str,
    ) -> std::result::Result<Option<ApiKey>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for template prompt endpoints and model configurations.
#[async_trait]
pub trait AiActionStore: Send + Sync {
    /// Returns a list of structured AI actions.
    async fn list_ai_actions(
        &self,
    ) -> std::result::Result<
        Vec<crate::models::ai::AiAction>,
        Box<dyn std::error::Error + Send + Sync>,
    >;

    /// Looks up a prompt profile configuration using its unique slug string.
    async fn get_ai_action(
        &self,
        slug: &str,
    ) -> std::result::Result<
        Option<crate::models::ai::AiAction>,
        Box<dyn std::error::Error + Send + Sync>,
    >;

    /// Registers a new structured AI action execution mapping.
    async fn create_ai_action(
        &self,
        action: crate::models::ai::CreateActionReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    /// Removes an AI action execution definition mapping.
    async fn delete_ai_action(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
}

/// Store operations for tracking interactive chat history states.
#[async_trait]
pub trait AiSessionStore: Send + Sync {
    /// Creates a conversation thread and maps initial variables.
    async fn create_ai_session(
        &self,
        session: &AiSession,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Loads active conversational states by thread identifier.
    async fn get_ai_session(
        &self,
        id: &str,
    ) -> std::result::Result<Option<AiSession>, Box<dyn std::error::Error + Send + Sync>>;

    /// Updates persistent structural parameters or chat messages in a session.
    async fn update_ai_session(
        &self,
        session: &AiSession,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Gathers list profiles of registered AI chat sessions.
    async fn list_ai_sessions(
        &self,
    ) -> std::result::Result<Vec<AiSession>, Box<dyn std::error::Error + Send + Sync>>;

    /// Deletes conversation thread history.
    async fn delete_ai_session(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
}

/// Store operations for managing extension manifests.
#[async_trait]
pub trait PluginStore: Send + Sync {
    /// Commits extension schemas and operational code variables.
    async fn save_plugin(
        &self,
        plugin: &Plugin,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Lists active extension modules registered.
    async fn list_plugins(
        &self,
    ) -> std::result::Result<Vec<Plugin>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for relationship graphs linking independent record entries.
#[async_trait]
pub trait RelationStore: Send + Sync {
    /// Registers a directional relationship between two records.
    async fn create_relation(
        &self,
        origin_col: i64,
        origin_id: i64,
        target_col: i64,
        target_id: i64,
        rel_name: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Clears direction links mapped on target database entities.
    async fn delete_relation(
        &self,
        origin_col: i64,
        origin_id: i64,
        target_col: i64,
        target_id: i64,
        rel_name: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Fetches linked target identifiers related to a record.
    async fn get_related_ids(
        &self,
        origin_col: i64,
        origin_id: i64,
        rel_name: &str,
    ) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>>;

    /// Returns structural entities from distinct collections matching a list of record IDs.
    async fn get_records_by_ids(
        &self,
        collection_id: i64,
        ids: &[i64],
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for Javascript event scripts and triggers.
#[async_trait]
pub trait ScriptStore: Send + Sync {
    /// Gathers structural information for event hook triggers.
    async fn list_scripts(
        &self,
    ) -> std::result::Result<
        Vec<crate::models::script::Script>,
        Box<dyn std::error::Error + Send + Sync>,
    >;

    /// Binds execution script assets to custom database transaction trigger events.
    async fn create_script(
        &self,
        req: crate::models::script::CreateScriptReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    /// Removes trigger scripting assets.
    async fn delete_script(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Locates execution modules matching their registered names.
    async fn get_script_by_name(
        &self,
        name: &str,
    ) -> std::result::Result<
        Option<crate::models::script::Script>,
        Box<dyn std::error::Error + Send + Sync>,
    >;

    /// Resolves target scripts matching a system execution trigger signal.
    async fn get_scripts_by_trigger(
        &self,
        trigger: &str,
    ) -> std::result::Result<
        Vec<crate::models::script::Script>,
        Box<dyn std::error::Error + Send + Sync>,
    >;
}

/// Store operations for managing user interface layouts and pages.
#[async_trait]
pub trait TemplateStore: Send + Sync {
    /// Returns registered page styling structures.
    async fn list_templates(
        &self,
    ) -> std::result::Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>>;

    /// Loads individual styling properties matching page slug configurations.
    async fn get_template_by_slug(
        &self,
        slug: &str,
    ) -> std::result::Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>>;

    /// Generates layout structures.
    async fn create_template(
        &self,
        req: models::CreateTemplateReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    /// Merges update parameters on existing layout configurations.
    async fn update_template(
        &self,
        id: i64,
        content: String,
        script_id: Option<i64>,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Deletes structural page templates.
    async fn delete_template(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;
}

/// Store operations for dashboard analytical logs and metrics.
#[async_trait]
pub trait DashboardStore: Send + Sync {
    /// Summarizes system activities, sizes on disk, index counts, and errors over timeline windows.
    async fn get_dashboard_stats(
        &self,
    ) -> std::result::Result<DashboardData, Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for managing and searching high-dimensional vector embeddings.
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Resolves raw vector data stored for a distinct algorithm model key.
    async fn get_vectors_for_collection(
        &self,
        collection_id: i64,
        model: &str,
    ) -> std::result::Result<Vec<(i64, String, Vec<f32>)>, Box<dyn StdError + Send + Sync>>;

    /// Searches numeric dimensions utilizing L2 distance equations.
    async fn search_vector(
        &self,
        collection_id: i64,
        field: &str,
        vector: Vec<f32>,
        limit: usize,
    ) -> std::result::Result<Vec<(Record, f32)>, Box<dyn std::error::Error + Send + Sync>>;

    /// Records structural coordinates corresponding to document fields.
    async fn save_vector(
        &self,
        collection_id: i64,
        record_id: i64,
        field_name: &str,
        vector: Vec<f32>,
        model: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>>;

    /// Confirms indexing states matching target coordinates.
    async fn has_vector(
        &self,
        collection_id: i64,
        record_id: i64,
        field_name: &str,
        model: &str,
    ) -> std::result::Result<bool, Box<dyn StdError + Send + Sync>>;

    /// Returns multi-algorithm embeddings registered for an individual record.
    async fn get_record_vectors(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<Vec<models::VectorRecord>, Box<dyn StdError + Send + Sync>>;
}

/// Store operations for structured data analytical evaluations.
#[async_trait]
pub trait QueryEngineStore: Send + Sync {
    /// Parses and evaluates complex filter operations over database tables.
    async fn query_engine(
        &self,
        query: ApexQuery,
    ) -> std::result::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>>;
}

/// Store operations for database connection reloading.
#[async_trait]
pub trait ConnectionStore: Send + Sync {
    /// Instructs SQLite to reload file descriptors and discard cached transaction contexts.
    async fn reload_connections(
        &self,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// The unified master database trait representing composite persistence operations.
#[async_trait]
pub trait Db:
    CollectionStore
    + RecordStore
    + SearchStore
    + UserStore
    + OAuthStore
    + AuthTokenStore
    + TenantStore
    + SandboxStore
    + FileStore
    + ConfigStore
    + AuditStore
    + ApiKeyStore
    + AiActionStore
    + AiSessionStore
    + PluginStore
    + RelationStore
    + ScriptStore
    + TemplateStore
    + DashboardStore
    + VectorStore
    + QueryEngineStore
    + ConnectionStore
    + Send
    + Sync
{
}
