use axum::{
    extract::{Path, State, Query, Request, FromRef},
    http::{StatusCode, HeaderMap, HeaderValue, request::Parts}, 
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router, Extension,
};
use axum::body::HttpBody;
use axum_extra::headers::{Authorization, authorization::Bearer, HeaderMapExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::fmt; 
use std::net::SocketAddr;
use apexkit_core::{
    schema::CollectionSchema,
    validation::ValidationError,
    auth::{self, Claims},
    jobs::JobQueue,
    realtime::DbEvent,
    storage::StorageBackend, 
    security::Vault,
    Db,
    VectorProvider,
};
use tokio::sync::{broadcast, RwLock};
use utoipa::{OpenApi, ToSchema, IntoParams};
use utoipa::openapi::Server; 
use utoipa_scalar::{Scalar, Servable}; 
use validator::Validate;
use metrics_exporter_prometheus::PrometheusHandle;
use std::time::Instant;
use moka::future::Cache;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use async_graphql::dynamic::Schema;
use async_graphql::dataloader::DataLoader;
use apexkit_core::scripting::ScriptEngine;
use crate::sandbox_manager::SandboxManager;
use crate::graphql::RelationLoader;
use std::collections::HashMap;
use crate::tenant_manager::TenantManager;
use apexkit_core::realtime::EventScope;
use axum::response::sse::{Event, Sse};
use futures::stream::{Stream};
use std::convert::Infallible;
use apexkit_core::jobs::JobContext;
use serde_json::{json, Value};
use apexkit_core::models::DashboardData;
use axum::extract::DefaultBodyLimit;
use tracing::{info, debug};
use tower_http::compression::CompressionLayer;

// --- Module Registrations ---
pub mod websocket;
pub mod storage; 
pub mod auth_advanced;
pub mod config_routes;
pub mod graphql; 
pub mod settings;
pub mod dynamic_cors;
pub mod scheduler;
pub mod logging;
pub mod assets;
pub mod ai_routes;
pub mod script_routes;
pub mod renderer;
pub mod template_routes;
pub mod ai_architect;
pub mod css_compiler;
pub mod vector_routes;
pub mod import_data_routes;
pub mod export_data_routes;
pub mod cli;
pub mod sandbox_manager; 
pub mod tenant_manager;
pub mod tenant_routes;
pub mod backup;
pub mod backup_routes;
pub mod key_routes;
pub mod site_routes;
pub mod collections_and_records_routes;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<dyn Db>,
    pub tenant_manager: Arc<TenantManager>,
    pub sandbox_manager: Arc<SandboxManager>,
    pub queue: JobQueue,
    pub metrics: Option<PrometheusHandle>,
    pub tx: broadcast::Sender<DbEvent>, 
    pub storage: Arc<dyn StorageBackend>,
    pub vault: Arc<Vault>,
    pub schema: Arc<RwLock<Schema>>, 
    pub scheduler: Arc<RwLock<scheduler::SchedulerService>>,
    pub script_engine: Arc<ScriptEngine>,
    pub css_cache: Arc<RwLock<String>>,
    pub thumb_cache: Cache<String, Arc<Vec<u8>>>, 
    pub embedder: Arc<apexkit_core::embeddings::EmbedderService>,
    pub vector_provider: Arc<dyn VectorProvider>,
    pub port: u16,
    // Script Cache (Key -> Value)
    // We use String values to store JSON or Numbers (parsed on retrieval)
    // [RENAMED] Only for Root scope
    pub root_script_cache: Cache<String, String>,
}

// --- DTOs ---

#[derive(Serialize, ToSchema, Deserialize)] pub struct CollectionResponse { id: i64, name: String, schema: Option<CollectionSchema>, index: Option<String> }
#[derive(Deserialize, ToSchema, Validate, Serialize)] pub struct UpdateCollection { #[validate(length(min = 1, max = 50))] name: Option<String>, schema: Option<CollectionSchema> }
#[derive(Deserialize, ToSchema, Validate)] pub struct CreateCollectionReq { #[validate(length(min = 1, max = 50))] name: String, schema: Option<CollectionSchema>, index: Option<String> }
#[derive(Serialize, ToSchema, Deserialize)] 
pub struct RecordResponse { 
    id: i64, 
    data: serde_json::Value, 
    #[serde(skip_serializing_if = "Option::is_none")]
    expand: Option<serde_json::Value>,
    pub created: String,
    pub updated: String,
}
#[derive(Deserialize, ToSchema, Validate)] 
pub struct AuthRequest { 
    #[validate(email)] 
    pub email: String, 
    #[validate(length(min = 6))] 
    pub password: String,
    // Optional Role (defaults to "user" if not provided or restricted)
    pub role: Option<String>,
    // Optional Metadata
    #[schema(value_type = Option<Object>)]
    pub metadata: Option<serde_json::Value>,
}
#[derive(Serialize, ToSchema)] pub struct AuthResponse { token: String, user: UserDto }
#[derive(Serialize, ToSchema, Deserialize)] 
pub struct UserDto { 
    id: i64, 
    email: String, 
    role: String, 
    pub metadata: Option<serde_json::Value>, 
    // Authoritative scope from the current session token
    pub scope: Option<String>,
}
#[derive(Serialize, ToSchema)] pub struct ProblemDetail { error: String, message: String, details: Option<serde_json::Value>, status: u16 }
#[derive(Deserialize, ToSchema)] pub struct RelationRequest { target_collection_id: i64, target_record_id: i64, relation_name: String }
#[derive(Deserialize, ToSchema, IntoParams)] pub struct SearchQuery { pub q: String, pub limit: Option<usize> }
#[derive(Serialize, ToSchema, Deserialize)] pub struct RecordListResponse { items: Vec<RecordResponse>, total: i64 }

#[derive(Debug)]
pub enum AppError {
    LibsqlError(libsql::Error),
    JsonError(String),
    UnknownError(String),
    NotFound(String),
    Validation(Vec<ValidationError>),
    InputValidation(validator::ValidationErrors),
    Unauthorized(String),
    Forbidden(String),
}
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{:?}", self) }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg, details) = match self {
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m, None),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, m, None),
            AppError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m, None),
            AppError::Validation(v) => (StatusCode::UNPROCESSABLE_ENTITY, "Schema Validation Failed".into(), Some(serde_json::json!(v))),
            AppError::InputValidation(v) => (StatusCode::BAD_REQUEST, "Input Validation Failed".into(), Some(serde_json::json!(v))),
            AppError::JsonError(m) => (StatusCode::BAD_REQUEST, m, None),
            AppError::LibsqlError(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Database Error: {}", e), None),
            AppError::UnknownError(m) => (StatusCode::INTERNAL_SERVER_ERROR, m, None),
        };
        
        let body = Json(ProblemDetail {
            error: status.canonical_reason().unwrap_or("error").to_string(),
            message: msg,
            details,
            status: status.as_u16()
        });
        
        (status, body).into_response()
    }
}

// --- EXTRACTORS ---

pub struct DatabaseConnection(pub Arc<dyn Db>);
impl<S> axum::extract::FromRequestParts<S> for DatabaseConnection
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(db) = parts.extensions.get::<Arc<dyn Db>>() {
            return Ok(DatabaseConnection(db.clone()));
        }
        let app_state = AppState::from_ref(state);
        Ok(DatabaseConnection(app_state.db))
    }
}

pub struct StorageConnection(pub Arc<dyn StorageBackend>);
impl<S> axum::extract::FromRequestParts<S> for StorageConnection
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(s) = parts.extensions.get::<Arc<dyn StorageBackend>>() {
            return Ok(StorageConnection(s.clone()));
        }
        let app_state = AppState::from_ref(state);
        Ok(StorageConnection(app_state.storage))
    }
}

// --- HELPER: Audit Metadata Extraction ---
pub fn extract_log_meta(headers: &HeaderMap, addr: Option<SocketAddr>, details: serde_json::Value) -> serde_json::Value {
    let mut meta = details;
    
    // 1. IP Resolution
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .or_else(|| addr.map(|a| a.ip().to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    // 2. Client Info
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or("-");
    let referer = headers.get("referer").and_then(|v| v.to_str().ok()).unwrap_or("-");

    if let Some(obj) = meta.as_object_mut() {
        obj.insert("ip".into(), json!(ip));
        obj.insert("user_agent".into(), json!(ua));
        obj.insert("referer".into(), json!(referer));
    }
    meta
}

// --- HELPER: Void Hooks (Notify/Block) ---
pub async fn trigger_void_hook(
    state: &AppState,
    trigger: &str,
    data: Value,
    auth: Option<&Claims>,
    scope: Option<&EventScope>,
    base_url: Option<String>
) -> Result<(), AppError> {
    let scripts = state.db.get_scripts_by_trigger(trigger).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    if scripts.is_empty() { return Ok(()); }

    let ctx = json!({
        "trigger": trigger,
        "data": data,
        "auth": auth.map(|c| json!({ "id": c.uid, "email": c.sub, "role": c.role })),
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    let context = Arc::new(crate::ScopedScriptContext {
        state: state.clone(),
        scope: scope.cloned().unwrap_or(EventScope::Root),
    });

    for script in scripts {
        state.script_engine.run_hook(
            &script.code, 
            ctx.clone(), 
            context.clone(),
            base_url.clone(),  
            scope.cloned()
        ).await.map_err(|e| AppError::Validation(vec![ValidationError::ConstraintViolation(trigger.into(), e)]))?;
    }
    Ok(())
}

// --- HELPER: Filter Hooks (Modify Data) ---
pub async fn trigger_filter_hook(
    state: &AppState,
    trigger: &str,
    data: Value,
    auth: Option<&Claims>,
    scope: Option<&EventScope>,
    base_url: Option<String>
) -> Result<Value, AppError> {
    let scripts = state.db.get_scripts_by_trigger(trigger).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    if scripts.is_empty() { return Ok(data); }

    let mut current_data = data;

    let context = Arc::new(crate::ScopedScriptContext {
        state: state.clone(),
        scope: scope.cloned().unwrap_or(EventScope::Root),
    });

    for script in scripts {
        let ctx = json!({
            "trigger": trigger,
            "data": current_data,
            "auth": auth.map(|c| json!({ "id": c.uid, "email": c.sub, "role": c.role }))
        });

        if let Some(res) = state.script_engine.run_hook(
            &script.code, 
            ctx, 
            context.clone(), 
            base_url.clone(), 
            scope.cloned()
        ).await.map_err(|e| AppError::Validation(vec![ValidationError::ConstraintViolation(trigger.into(), e)]))? 
        {
            current_data = res;
        }
    }
    Ok(current_data)
}

// Helper function to resolve DB from scope (async)
async fn resolve_db_from_scope(state: &AppState, scope: &EventScope) -> Result<Arc<dyn Db>, AppError> {
    match scope {
        EventScope::Root => Ok(state.db.clone()),
        EventScope::Tenant(id) => state.tenant_manager.get_tenant(id.clone()).await
            .map_err(|e| AppError::UnknownError(e)),
        EventScope::Sandbox(id) => state.sandbox_manager.get_sandbox(id).await
            .map_err(|e| AppError::UnknownError(e)),
        _ => Ok(state.db.clone()),
    }
}
// --- HELPER: Record Hooks (Existing) ---
pub async fn trigger_hooks(
    state: &AppState,
    // [REVERTED] No 'db' parameter here. We resolve it from scope.
    trigger: &str,
    collection: &apexkit_core::Collection, 
    record_id: Option<i64>,
    data: &serde_json::Value,
    auth: Option<&Claims>,
    base_url: Option<String>,
    scope: Option<&EventScope>
) -> Result<Option<serde_json::Value>, AppError> {
    
    let actual_scope = scope.cloned().unwrap_or(EventScope::Root);
    
    // 1. Resolve DB locally just to fetch the scripts configuration
    let db = resolve_db_from_scope(state, &actual_scope).await?;
    
    let scripts = db.get_scripts_by_trigger(trigger).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    if scripts.is_empty() { return Ok(None); }

    let mut current_data = data.clone();
    let mut modified = false;

    // 2. Create Context (Lightweight, no DB instance attached)
    // The ScriptEngine will use `resolve_tenant_db` via the trait to get the DB when needed.
    let context = Arc::new(crate::ScopedScriptContext {
        state: state.clone(),
        scope: actual_scope.clone(),
    });

    for script in scripts {
        // Target Collection Filtering
        if let Some(target) = &script.target_collection {
            if target != &collection.name { continue; }
        }

        let event_context = serde_json::json!({
            "record": { "id": record_id, "data": current_data },
            "collection": { "id": collection.id, "name": collection.name, "schema": collection.schema },
            "auth": auth.map(|c| serde_json::json!({ "id": c.uid, "email": c.sub, "role": c.role })),
            "trigger": trigger
        });
        
        // Run Hook
        match state.script_engine.run_hook(&script.code, event_context, context.clone(), base_url.clone(), Some(actual_scope.clone())).await {
            Ok(Some(new_data)) => { current_data = new_data; modified = true; },
            Ok(None) => {},
            Err(err_msg) => {
                // If a hook fails, we block the operation
                return Err(AppError::Validation(vec![ValidationError::ConstraintViolation("_hook".to_string(), err_msg)]));
            }
        }
    }

    if modified { Ok(Some(current_data)) } else { Ok(None) }
}

// --- MIDDLEWARE ---

async fn auth_middleware(
    State(state): State<AppState>, 
    mut req: Request, 
    next: Next
) -> Result<Response, StatusCode> {
    
    // 1. Determine Scope
    let current_request_scope = req.extensions().get::<EventScope>().cloned().unwrap_or(EventScope::Root);

    // 2. [GATEKEEPER] Check Suspension
    // We use the Manager to get the context. This uses the Cache.
    let mut tenant_is_suspended = false;

    if let EventScope::Tenant(ref tenant_id) = current_request_scope {
        // Try to get context (Fast path via cache)
        if let Ok(ctx) = state.tenant_manager.get_tenant_context(tenant_id).await {
            // Check the status stored in memory
            if ctx.status == "suspended" || ctx.status == "archived" {
                tenant_is_suspended = true;
            }
        } else {
            // If we can't load the tenant (e.g. disk error or deleted), deny access
            return Err(StatusCode::NOT_FOUND);
        }
    }

    // 3. Resolve DB to Check for Keys
    // If we are in a tenant, we check the tenant's DB first
    let db_to_check: Arc<dyn Db> = if let Some(db) = req.extensions().get::<Arc<dyn Db>>() {
        db.clone()
    } else {
        state.db.clone()
    };

    // 4. Validate JWT (Bearer)
    if let Some(auth_header) = req.headers().typed_get::<Authorization<Bearer>>() {
        if let Ok(claims) = auth::decode_jwt(auth_header.token()) {
            
            // Check Scope Matches URL
            let is_authorized = match claims.scope.as_str() {
                "root" => true, // Root Admin can access everything
                scope_str => {
                    let expected = match &current_request_scope {
                        EventScope::Root => "root".to_string(),
                        EventScope::Tenant(id) => format!("tenant:{}", id),
                        EventScope::Sandbox(id) => format!("sandbox:{}", id),
                        _ => "root".to_string(), 
                    };
                    scope_str == expected
                }
            };

            // [ENFORCEMENT] 
            // If User is NOT Root (i.e. is_authorized via scope match, but not "root" scope explicitly)
            // AND Tenant is suspended -> BLOCK
            // However, the logic above sets is_authorized=true for tenants matching their own scope.
            // We need to differentiate Root Admin vs Tenant User.
            
            let is_root_user = claims.scope == "root";

            if !is_authorized { return Err(StatusCode::FORBIDDEN); }
            
            if tenant_is_suspended && !is_root_user {
                 return Err(StatusCode::FORBIDDEN); // "Account Suspended"
            }

            req.extensions_mut().insert(claims);
            return Ok(next.run(req).await);
        }
    }

    // 5. Validate API Key (x-api-key)
    if let Some(key_header) = req.headers().get("x-api-key") {
         if let Ok(key) = key_header.to_str() {
             
             // A. Check Local DB
             let local_verification = db_to_check.verify_api_key(key).await;
             
             // B. Resolve Key Origin
             let api_key_opt = if let Ok(Some(k)) = local_verification {
                 Some((k, true)) // Found in Local DB (Tenant Created)
             } else if !matches!(current_request_scope, EventScope::Root) {
                 // Fallback: Check Root DB
                 state.db.verify_api_key(key).await.ok().flatten().map(|k| (k, false))
             } else {
                 None
             };

             if let Some((api_key, is_local_key)) = api_key_opt {
                 
                 // [ENFORCEMENT]
                 if tenant_is_suspended {
                     if is_local_key {
                         // Local key usage blocked on suspended tenant
                         tracing::warn!("Blocked suspended tenant key");
                         return Err(StatusCode::FORBIDDEN);
                     }
                     // Root keys allowed (to manage/fix tenant)
                 }

                 // [SCOPE CHECK]
                 let is_allowed = if is_local_key {
                     true // Local key implies access to current DB
                 } else {
                     // Root key must have correct scope
                     if api_key.scope == "root" || api_key.scope == "*" {
                         true
                     } else if api_key.scope.starts_with("tenant:") {
                         if let EventScope::Tenant(tid) = &current_request_scope {
                             api_key.scope == format!("tenant:{}", tid)
                         } else { false }
                     } else if api_key.scope.starts_with("sandbox:") {
                         if let EventScope::Sandbox(sid) = &current_request_scope {
                             api_key.scope == format!("sandbox:{}", sid)
                         } else { false }
                     } else { false }
                 };

                 if !is_allowed { return Err(StatusCode::FORBIDDEN); }

                 let claims = Claims {
                     sub: format!("apikey:{}", api_key.id),
                     uid: 0,
                     role: api_key.role,
                     exp: 9999999999,
                     scope: if is_local_key { 
                         match current_request_scope {
                             EventScope::Tenant(ref id) => format!("tenant:{}", id),
                             EventScope::Sandbox(ref id) => format!("sandbox:{}", id),
                             _ => "root".to_string()
                         }
                     } else { api_key.scope },
                 };
                 req.extensions_mut().insert(claims);
             }
        }
    }

    Ok(next.run(req).await)
}

async fn metrics_middleware(req: Request, next: Next) -> Response {
    let _start = Instant::now();
    next.run(req).await
}

// --- JOB CONTEXT ---
pub struct GlobalJobContext {
    pub root_db: Arc<dyn Db>,
    pub root_vector_provider: Arc<dyn VectorProvider>,
    pub tenant_manager: Arc<TenantManager>,
    pub sandbox_manager: Arc<SandboxManager>,
}

#[async_trait::async_trait]
impl JobContext for GlobalJobContext {
    async fn resolve(&self, scope_id: Option<&str>) -> Option<(Arc<dyn Db>, Arc<dyn VectorProvider>)> {
        match scope_id {
            Some(id) => {
                if let Ok(ctx) = self.tenant_manager.get_tenant_context(id).await { return Some((ctx.db, ctx.vector_provider)); }
                if let Ok(db) = self.sandbox_manager.get_sandbox(id).await {
                     if let Some(prov) = self.sandbox_manager.get_vector_provider(id).await { return Some((db, prov)); }
                }
                None
            },
            None => Some((self.root_db.clone(), self.root_vector_provider.clone()))
        }
    }
}

// [NEW] Wrapper to enforce scoping
pub struct ScopedScriptContext {
    pub state: AppState,
    pub scope: EventScope,
}

impl ScopedScriptContext {
    fn _prefix_key(&self, key: &str) -> String {
        match &self.scope {
            EventScope::Root => format!("root:{}", key), // Root gets its own namespace
            EventScope::Tenant(id) => format!("tenant:{}:{}", id, key),
            EventScope::Sandbox(id) => format!("sandbox:{}:{}", id, key),
            _ => format!("global:{}", key),
        }
    }
    
    fn _get_default_ttl(&self) -> u64 {
        std::env::var("CACHE_TTL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300) // Default 5 minutes
    }
}
use crate::storage::ScopedDynamicStorage;
// Implement the trait for AppState
impl apexkit_core::ScriptContext for ScopedScriptContext {
    fn get_db(&self) -> Arc<dyn Db> {
        self.state.db.clone()
    }

    fn get_vault(&self) -> Arc<Vault> {
        self.state.vault.clone()
    }

    fn get_embedder(&self) -> Arc<apexkit_core::embeddings::EmbedderService> {
        self.state.embedder.clone()
    }

    fn get_vector_provider(&self) -> Arc<dyn VectorProvider> {
        self.state.vector_provider.clone()
    }

    fn get_realtime_tx(&self) -> tokio::sync::broadcast::Sender<apexkit_core::realtime::DbEvent> {
        self.state.tx.clone()
    }

    fn get_storage(&self) -> Arc<dyn StorageBackend> {
        Arc::new(ScopedDynamicStorage::new(self.state.clone(), self.scope.clone()))
    }

    fn get_scope(&self) -> EventScope {
        self.scope.clone()
    }

    fn get_shared_script(&self, name: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<apexkit_core::script_models::Script>> + Send>> {
        let db = self.state.db.clone(); // Root DB
        let n = name.to_string();
        Box::pin(async move {
            db.get_script_by_name(&n).await.ok().flatten()
        })
    }

    fn execute_shared_script(&self, code: String, payload: serde_json::Value, scope: EventScope) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<serde_json::Value, String>> + Send>> {
        let engine = self.state.script_engine.clone();
        let state = self.state.clone();
        
        // When executing a shared script, the context MUST have the correct scope.
        let new_ctx = Arc::new(ScopedScriptContext {
            state: state.clone(),
            scope: scope.clone(),
        });
        
        Box::pin(async move {
            engine.run_script(&code, payload, new_ctx, None, None).await
        })
    }

    // Dynamic Resolution for Tenant Switching
    fn resolve_tenant_db(&self, tenant_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>> {
        let tm = self.state.tenant_manager.clone();
        let tid = tenant_id.to_string();
        Box::pin(async move {
            tm.get_tenant(tid).await.ok()
        })
    }

    // Dynamic Resolution for Sandbox Switching
    fn resolve_sandbox_db(&self, session_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>> {
        let sm = self.state.sandbox_manager.clone();
        let sid = session_id.to_string();
        Box::pin(async move {
            sm.get_sandbox(&sid).await.ok()
        })
    }

    fn admin_create_tenant(&self, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let tm = self.state.tenant_manager.clone();
        Box::pin(async move {
            tm.create_tenant(id).await.map(|_| ())
        })
    }

    fn admin_update_tenant(&self, id: String, updates: serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let db = self.state.db.clone();
        let tm = self.state.tenant_manager.clone();
        Box::pin(async move {
            let name = updates.get("name").and_then(|v| v.as_str()).map(String::from);
            let status = updates.get("status").and_then(|v| v.as_str()).map(String::from);
            let tier = updates.get("tier").and_then(|v| v.as_str()).map(String::from);
            
            // 1. Update Metadata
            db.update_tenant_full(&id, name, status, tier).await.map_err(|e| e.to_string())?;
            
            // 2. Invalidate Cache so new status/settings take effect immediately
            tm.invalidate(&id).await;
            
            Ok(())
        })
    }

    fn admin_delete_tenant(&self, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let db = self.state.db.clone();
        let tm = self.state.tenant_manager.clone();
        Box::pin(async move {
            // 1. Delete Metadata
            db.delete_tenant_metadata(&id).await.map_err(|e| e.to_string())?;
            // 2. Delete Files & Cache
            tm.delete_tenant(&id).await?;
            Ok(())
        })
    }

    fn admin_get_tenant_usage(&self, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, String>> + Send>> {
        let db = self.state.db.clone(); // Use Root DB (which has the logic in ApexKit impl)
        Box::pin(async move {
            db.get_tenant_disk_usage(&id).await.map_err(|e| e.to_string())
        })
    }

    fn admin_create_sandbox(&self, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let sm = self.state.sandbox_manager.clone();
        let db = self.state.db.clone();
        Box::pin(async move {
            // Default strategy for script creation
            sm.create_sandbox(&id, sandbox_manager::CloneStrategy::None, db).await.map(|_| ()).map_err(|e| e.to_string())
        })
    }

    fn admin_update_sandbox(&self, id: String, updates: serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let db = self.state.db.clone();
        let sm = self.state.sandbox_manager.clone();
        Box::pin(async move {
            let name = updates.get("name").and_then(|v| v.as_str()).map(String::from);
            let status = updates.get("status").and_then(|v| v.as_str()).map(String::from);
            let expires_at = updates.get("expires_at").and_then(|v| v.as_str()).map(String::from);
            
            // 1. Update Metadata
            db.update_sandbox_full(&id, name, status, expires_at).await.map_err(|e| e.to_string())?;
            
            // 2. Invalidate Cache
            // (Sandbox manager doesn't strictly check DB status on load like Tenant manager does, but good practice)
            sm.cleanup_sandbox(&id); // Warning: cleanup deletes files. We just want to invalidate cache. 
            // Since sandbox manager is ephemeral, standard eviction handles updates mostly. 
            // But if we want to force status check, we might need an `invalidate_cache` method on SandboxManager too.
            // For now, metadata update is sufficient for listing visibility.
            Ok(())
        })
    }

    fn admin_delete_sandbox(&self, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let db = self.state.db.clone();
        let sm = self.state.sandbox_manager.clone();
        Box::pin(async move {
            db.delete_sandbox_metadata(&id).await.map_err(|e| e.to_string())?;
            sm.cleanup_sandbox(&id); // Deletes files & cache
            Ok(())
        })
    }

    fn admin_get_sandbox_usage(&self, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, String>> + Send>> {
        let db = self.state.db.clone(); 
        Box::pin(async move {
            db.get_sandbox_disk_usage(&id).await.map_err(|e| e.to_string())
        })
    }

    // [UPDATED] Cache Methods
    fn cache_get(&self, key: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>> {
        let key = key.to_string();
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_cache = self.state.root_script_cache.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            match scope {
                EventScope::Tenant(id) => {
                    if let Ok(ctx) = tm.get_tenant_context(&id).await {
                        return ctx.script_cache.get(&key).await;
                    }
                    None
                },
                EventScope::Sandbox(id) => {
                    if let Ok(ctx) = sm.get_sandbox_context(&id).await { // Need to expose get_sandbox_context
                        return ctx.script_cache.get(&key).await;
                    }
                    None
                },
                _ => root_cache.get(&key).await
            }
        })
    }

    fn cache_set(&self, key: &str, val: &str, _ttl: Option<u64>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let key = key.to_string();
        let val = val.to_string();
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_cache = self.state.root_script_cache.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            match scope {
                EventScope::Tenant(id) => {
                    if let Ok(ctx) = tm.get_tenant_context(&id).await {
                        ctx.script_cache.insert(key, val).await;
                    }
                },
                EventScope::Sandbox(id) => {
                    if let Ok(ctx) = sm.get_sandbox_context(&id).await {
                        ctx.script_cache.insert(key, val).await;
                    }
                },
                _ => { root_cache.insert(key, val).await; }
            }
        })
    }
    
    // For incr, you need read-modify-write on the specific cache instance.
    fn cache_incr(&self, key: &str, delta: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = i64> + Send>> {
        let key = key.to_string();
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_cache = self.state.root_script_cache.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            let cache = match scope {
                EventScope::Tenant(id) => tm.get_tenant_context(&id).await.ok().map(|c| c.script_cache),
                EventScope::Sandbox(id) => sm.get_sandbox_context(&id).await.ok().map(|c| c.script_cache),
                _ => Some(root_cache),
            };

            if let Some(c) = cache {
                let current_str = c.get(&key).await;
                let current_val = current_str.and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                let new_val = current_val + delta;
                c.insert(key, new_val.to_string()).await;
                new_val
            } else {
                0
            }
        })
    }
    
    fn cache_del(&self, key: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let key = key.to_string();
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_cache = self.state.root_script_cache.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            match scope {
                EventScope::Tenant(id) => { if let Ok(ctx) = tm.get_tenant_context(&id).await { ctx.script_cache.invalidate(&key).await; } },
                EventScope::Sandbox(id) => { if let Ok(ctx) = sm.get_sandbox_context(&id).await { ctx.script_cache.invalidate(&key).await; } },
                _ => { root_cache.invalidate(&key).await; }
            }
        })
    }

    // Implementation for listing keys
    fn cache_list_keys(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<String>> + Send>> {
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_cache = self.state.root_script_cache.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            let cache = match scope {
                EventScope::Tenant(id) => tm.get_tenant_context(&id).await.ok().map(|c| c.script_cache),
                EventScope::Sandbox(id) => sm.get_sandbox_context(&id).await.ok().map(|c| c.script_cache),
                _ => Some(root_cache),
            };

            if let Some(c) = cache {
                // moka::future::Cache::iter() is synchronous and returns an iterator over the keys
                c.iter().map(|(k, _)| k.as_ref().clone()).collect()
            } else {
                vec![]
            }
        })
    }
}

// --- TENANT/SANDBOX MIDDLEWARES ---

async fn sandbox_lifecycle_middleware(
    Path(params): Path<HashMap<String, String>>,
    BaseUrl(base_url): BaseUrl, // Needed for hooks
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let session_id = params.get("session_id").ok_or(StatusCode::NOT_FOUND)?.clone();

    // 1. Capture Ingress
    let ingress = req.headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    // Before Request Hook
    let hook_payload = serde_json::json!({
        "sandbox_id": session_id,
        "path": req.uri().path(),
        "method": req.method().to_string(),
        "ip": req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()).unwrap_or("unknown"),
        "ingress": ingress,
        "egress": 0
    });

    if let Err(e) = trigger_void_hook(
        &state, 
        "before_sandbox_request", 
        hook_payload, 
        None, 
        Some(&EventScope::Root), 
        Some(base_url.clone())
    ).await {
        tracing::warn!("Blocked request to sandbox {}: {:?}", session_id, e);
        return Err(StatusCode::FORBIDDEN);
    }

    match state.sandbox_manager.get_sandbox(&session_id).await {
        Ok(sandbox_db) => {
            req.extensions_mut().insert(sandbox_db);
            
            // [FIX] Use ScopedDynamicStorage to support S3 Reselling
            let scope = EventScope::Sandbox(session_id.clone());
            let storage: Arc<dyn StorageBackend> = Arc::new(
                crate::storage::ScopedDynamicStorage::new(state.clone(), scope.clone())
            );
            
            req.extensions_mut().insert(storage);
            req.extensions_mut().insert(scope);
            
            // Capture path before consuming req
            let path_clone = req.uri().path().to_string();

            let mut response = next.run(req).await;
            
            if let Ok(val) = HeaderValue::from_str(&format!("sandbox:{}", session_id)) {
                response.headers_mut().insert("X-Apex-Scope", val);
            }

            // 3. CAPTURE EGRESS
            let egress = response.headers()
                .get(axum::http::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| response.body().size_hint().exact())
                .unwrap_or(0);

            // After Request Hook (Async)
            let status = response.status().as_u16();
            let state_clone = state.clone();
            let base_url_clone = base_url.clone();
            let sid_clone = session_id.to_string();

            tokio::spawn(async move {
                let payload = serde_json::json!({
                    "sandbox_id": sid_clone,
                    "path": path_clone,
                    "status": status,
                    "ingress": ingress,
                    "egress": egress
                });
                let _ = trigger_void_hook(
                    &state_clone, 
                    "after_sandbox_request", 
                    payload, 
                    None, 
                    Some(&EventScope::Root), 
                    Some(base_url_clone)
                ).await;
            });

            Ok(response)
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

async fn tenant_resolver_middleware(
    path_params: Option<Path<HashMap<String, String>>>,
    BaseUrl(base_url): BaseUrl,
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> std::result::Result<Response, StatusCode> {
    
    // 1. Check for API Key override (Smart Key)
    let mut key_scope_override: Option<String> = None;
    
    if let Some(key_header) = req.headers().get("x-api-key") {
         if let Ok(key) = key_header.to_str() {
             // Verify against Root DB first for global keys
             if let Ok(Some(api_key)) = state.db.verify_api_key(key).await {
                 key_scope_override = Some(api_key.scope.clone());
                 req.extensions_mut().insert(api_key);
             }
         }
    }

    let mut tenant_id = String::new();

    // 2. Logic: If Key Scope is specific (e.g. "tenant:xyz"), force that tenant context.
    if let Some(scope) = key_scope_override {
        if scope.starts_with("tenant:") {
            let target = scope.strip_prefix("tenant:").unwrap();
            if target != "*" {
                tenant_id = target.to_string();
            }
        }
    }

    // 3. Fallback: URL routing with Root Domain Protection
    if tenant_id.is_empty() {
        let root_domain = std::env::var("APEX_ROOT_DOMAIN").unwrap_or_default();
        let host = req.headers()
            .get("host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(':').next().unwrap_or("");

        // PRIORITY 1: Explicit Path (/tenant/app-1/...)
        if let Some(Path(params)) = path_params {
            if let Some(id) = params.get("tenant_id") { 
                tenant_id = id.clone(); 
            }
        }

        // PRIORITY 2: Check against ROOT_DOMAIN
        if tenant_id.is_empty() {
            if !root_domain.is_empty() && host == root_domain {
                // Host is exactly the Root Domain (e.g. my-app.koyeb.app)
                tenant_id = String::new(); 
            } else {
                // PRIORITY 3: Subdomain Extraction
                let parts: Vec<&str> = host.split('.').collect();
                if parts.len() >= 2 {
                    let sub = parts[0];
                    if !["localhost", "www", "api"].contains(&sub) {
                        tenant_id = sub.to_string();
                    }
                }
            }
        }
    }

    // Capture Ingress (Request Size)
    let ingress = req.headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    // Before Request Hook
    if !tenant_id.is_empty() {
        let hook_payload = serde_json::json!({
            "tenant_id": tenant_id,
            "path": req.uri().path(),
            "method": req.method().to_string(),
            "ip": req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()).unwrap_or("unknown"),
            "ingress": ingress,
            "egress": 0
        });

        if let Err(e) = trigger_void_hook(
            &state, 
            "before_tenant_request", 
            hook_payload.clone(), 
            None, 
            Some(&EventScope::Root), 
            Some(base_url.clone())
        ).await {
            tracing::warn!("Blocked request to tenant {}: {:?}", tenant_id, e);
            let msg = e.to_string(); 
            let body = Json(json!({ "error": "request_blocked", "message": msg, "status": 429 }));
            return Ok((StatusCode::TOO_MANY_REQUESTS, body).into_response());
        }
    }

    // 4. Resolve Context
    if tenant_id.is_empty() {
        // Root Context
        req.extensions_mut().insert(EventScope::Root);
        // Ensure Root DynamicStorage is used if not overridden (usually defaults to AppState storage)
        let mut response = next.run(req).await;
        response.headers_mut().insert("X-Apex-Scope", HeaderValue::from_static("root"));
        return Ok(response);
    }

    // Tenant Context
    match state.tenant_manager.get_tenant(tenant_id.clone()).await {
        Ok(tenant_db) => {
            req.extensions_mut().insert(tenant_db.clone());
            
            // [FIX] Use ScopedDynamicStorage to support S3 Reselling
            let scope = EventScope::Tenant(tenant_id.clone());
            let storage: Arc<dyn StorageBackend> = Arc::new(
                crate::storage::ScopedDynamicStorage::new(state.clone(), scope.clone())
            );
            
            req.extensions_mut().insert(storage);
            req.extensions_mut().insert(scope);

            let path_clone = req.uri().path().to_string();
            
            // Execute Handler
            let mut response = next.run(req).await;
            
            if let Ok(val) = HeaderValue::from_str(&format!("tenant:{}", tenant_id)) {
                response.headers_mut().insert("X-Apex-Scope", val);
            }

            // Capture Egress
            let egress = response.headers()
                .get(axum::http::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| response.body().size_hint().exact()) 
                .unwrap_or(0);

            // After Request Hook (Async)
            let status = response.status().as_u16();
            let state_clone = state.clone();
            let base_url_clone = base_url.clone();
            let tid_clone = tenant_id.clone();

            tokio::spawn(async move {
                let payload = serde_json::json!({
                    "tenant_id": tid_clone,
                    "path": path_clone,
                    "status": status,
                    "ingress": ingress, 
                    "egress": egress
                });
                let _ = trigger_void_hook(
                    &state_clone, 
                    "after_tenant_request", 
                    payload, 
                    None, 
                    Some(&EventScope::Root), 
                    Some(base_url_clone)
                ).await;
            });

            Ok(response)
        }
        Err(_) => {
            req.extensions_mut().insert(EventScope::Root);
            let mut response = next.run(req).await;
            response.headers_mut().insert("X-Apex-Scope", HeaderValue::from_static("root"));
            Ok(response)
        }
    }
}

// --- DYNAMIC DOCS HELPERS ---
async fn tenant_openapi_json(Path(params): Path<HashMap<String, String>>) -> Json<utoipa::openapi::OpenApi> {
    let tenant_id = params.get("tenant_id").map(|s| s.as_str()).unwrap_or("");
    let mut doc = ApiDoc::openapi();
    doc.servers = Some(vec![Server::new(format!("/tenant/{}", tenant_id))]);
    Json(doc)
}
async fn tenant_scalar_html(Path(params): Path<HashMap<String, String>>) -> impl IntoResponse {
    let tenant_id = params.get("tenant_id").map(|s| s.as_str()).unwrap_or("");
    let html = format!(r#"<!DOCTYPE html><html><head><title>ApexKit API (Tenant)</title><meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" /><style>body {{ margin: 0; }}</style></head><body><script id="api-reference" data-url="/tenant/{}/scalar/openapi.json"></script><script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script></body></html>"#, tenant_id);
    axum::response::Html(html)
}
async fn sandbox_openapi_json(Path(params): Path<HashMap<String, String>>) -> Json<utoipa::openapi::OpenApi> {
    let session_id = params.get("session_id").map(|s| s.as_str()).unwrap_or("");
    let mut doc = ApiDoc::openapi();
    doc.servers = Some(vec![Server::new(format!("/sandbox/{}", session_id))]);
    Json(doc)
}
async fn sandbox_scalar_html(Path(params): Path<HashMap<String, String>>) -> impl IntoResponse {
    let session_id = params.get("session_id").map(|s| s.as_str()).unwrap_or("");
    let html = format!(r#"<!DOCTYPE html><html><head><title>ApexKit API (Sandbox)</title><meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" /><style>body {{ margin: 0; }}</style></head><body><script id="api-reference" data-url="/sandbox/{}/scalar/openapi.json"></script><script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script></body></html>"#, session_id);
    axum::response::Html(html)
}

// --- Path Structs for Nested Routes ---
#[derive(Deserialize, IntoParams)]
pub struct IdPath {
    pub id: String, // Can be "1" (ID) or "posts" (Name)
}

#[derive(Deserialize, IntoParams)]
pub struct RecordPath {
    pub id: String, // Collection ID or Name
    pub record_id: i64, // Maps to {record_id}
}

// Helper to resolve Collection from String (ID or Name)
pub async fn resolve_collection_by_id_or_name(
    db: &Arc<dyn Db>, 
    identifier: &str
) -> Result<apexkit_core::Collection, AppError> {
    // 1. Try to parse as numeric ID first
    if let Ok(id_num) = identifier.parse::<i64>() {
        if let Ok(Some(col)) = db.get_collection(id_num).await {
            return Ok(col);
        }
    }

    // 2. Fallback: Look up by Name via list (Cached in CachedDb)
    let cols = db.list_collections().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    cols.into_iter()
        .find(|c| c.name == identifier)
        .ok_or_else(|| AppError::NotFound(format!("Collection '{}' not found", identifier)))
}

// =========================================================
// 6. SYSTEM & OTHER HANDLERS
// =========================================================

#[derive(Deserialize, ToSchema)]
pub struct ReloadRequest {
    pub target: Option<String>, // "root", "tenant:xyz", "sandbox:abc" or null (auto)
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/system/reload",
    request_body = ReloadRequest,
    responses((status = 200, description = "Reload triggered"))
)]
pub async fn reload_system(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Json(payload): Json<ReloadRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    
    // Only admins can reload anything
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let caller_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    // Determine target scope to reload
    let target_scope = if let Some(target) = payload.target {
        // If target specified, verify permissions
        if claims.scope != "root" {
            // Non-root admins cannot target arbitrary scopes
            return Err(AppError::Forbidden("Only Root Admins can target specific scopes".into()));
        }
        
        if target == "root" { EventScope::Root }
        else if let Some(id) = target.strip_prefix("tenant:") { EventScope::Tenant(id.to_string()) }
        else if let Some(id) = target.strip_prefix("sandbox:") { EventScope::Sandbox(id.to_string()) }
        else { return Err(AppError::InputValidation(validator::ValidationErrors::new())); } // Invalid format
    } else {
        // Default to caller's current scope
        caller_scope.clone()
    };

    match target_scope {
        EventScope::Root => {
            // RELOAD ROOT SYSTEM (Global Schema + Global Jobs)
            info!("[System] Reloading Root System...");
            
            let relation_loader = async_graphql::dataloader::DataLoader::new(
                crate::graphql::RelationLoader::new(state.db.clone()), 
                tokio::spawn
            );
        
            let new_schema = crate::graphql::build_schema(
                state.clone(), 
                std::sync::Arc::new(relation_loader)
            ).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        
            {
                let mut lock = state.schema.write().await;
                *lock = new_schema;
            }
        
            // Reload background jobs
            state.scheduler.read().await.load_jobs(state.clone()).await;
            
            Ok(Json(serde_json::json!({ 
                "status": "ok", 
                "message": "Root System reloaded (Schema & Jobs refreshed)" 
            })))
        },
        EventScope::Tenant(id) => {
            // RELOAD TENANT (Invalidate Cache)
            // This forces next request to reload DB/Schema/Cache
            info!("[System] Reloading Tenant {}", id);
            state.tenant_manager.invalidate(&id).await;
            Ok(Json(serde_json::json!({ 
                "status": "ok", 
                "message": format!("Tenant {} cache invalidated. Will reload on next request.", id) 
            })))
        },
        EventScope::Sandbox(id) => {
            // RELOAD SANDBOX
            info!("[System] Reloading Sandbox {}", id);
            state.sandbox_manager.invalidate(&id).await; 
            
            Ok(Json(serde_json::json!({ 
                "status": "ok", 
                "message": format!("Sandbox {} cache invalidated.", id) 
            })))
        },
        _ => Ok(Json(serde_json::json!({ "status": "ignored", "message": "Scope not reloadable" })))
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/logs",
    responses((status = 200, body = Vec<serde_json::Value>))
)]
pub async fn list_audit_logs(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let logs = db.list_audit_logs().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(logs))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/dashboard",
    responses((status = 200, body = DashboardData))
)]
pub async fn get_dashboard_stats_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<apexkit_core::models::DashboardData>, AppError> {
    if !matches!(auth.map(|e| e.0.role), Some(r) if r == "admin") { return Err(AppError::Forbidden("Admins only".into())); }
    let data = db.get_dashboard_stats().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(data))
}

#[utoipa::path(post, path = "/api/v1/admin/collections/{id}/reindex", params(IdPath), responses((status = 200, description = "Reindexing started")))]
pub async fn reindex_collection_handler(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path(path): Path<IdPath>) -> Result<Json<serde_json::Value>, AppError> {
    if !matches!(auth.map(|e| e.0.role), Some(r) if r == "admin") { return Err(AppError::Forbidden("Admins only".into())); }
    
    // [FIX] Resolve ID
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;
    
    db.reindex_collection(col.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(
    get,
    path = "/styles.css",
    responses((status = 200, description = "Purged Tailwind CSS", content_type = "text/css"))
)]
pub async fn serve_styles(
    State(state): State<AppState>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    {
        let cache = state.css_cache.read().await;
        if !cache.is_empty() {
            return Ok(Response::builder()
                .header("Content-Type", "text/css")
                .header("Cache-Control", "public, max-age=60") 
                .body(axum::body::Body::from(cache.clone()))
                .unwrap());
        }
    }

    let css = css_compiler::compile_styles(db.clone()).await.map_err(|e| AppError::UnknownError(e))?;

    {
        let mut cache = state.css_cache.write().await;
        *cache = css.clone();
    }

    Ok(Response::builder()
        .header("Content-Type", "text/css")
        .body(axum::body::Body::from(css))
        .unwrap())
}

// [NEW] DTO for SSE Query Params
#[derive(Deserialize)]
pub struct SseQuery {
    pub channel: Option<String>,
    pub event: Option<String>,
}

// Helper to namespace channels (Same logic as websocket.rs to ensure security)
fn namespaced_channel_sse(scope: &EventScope, channel: &str) -> String {
    match scope {
        EventScope::Root => format!("root::{}", channel),
        EventScope::Tenant(id) => format!("tenant_{}::{}", id, channel),
        EventScope::Sandbox(id) => format!("sandbox_{}::{}", id, channel),
        _ => channel.to_string(), // Should not happen for channels
    }
}

#[utoipa::path(
    get,
    path = "/sse",
    params(
        ("channel" = Option<String>, Query, description = "Specific channel to listen to"),
        ("event" = Option<String>, Query, description = "Specific event name to filter")
    ),
    responses((status = 200, description = "SSE Stream"))
)]
pub async fn sse_handler(
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>, 
    Query(params): Query<SseQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    
    let client_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let mut rx = state.tx.subscribe();

    let target_channel = params.channel.clone().map(|c| namespaced_channel_sse(&client_scope, &c));
    let target_event = params.event.clone();

    // Changed to DEBUG to reduce noise in production logs
    debug!("[SSE] Connected. Scope: {:?}, Channel: {:?}", client_scope, params.channel);

    let stream = async_stream::stream! {
        while let Ok(msg) = rx.recv().await {
            let should_yield = match &msg {
                DbEvent::Custom { event, scope, data: _ } => {
                    if let Some(req_evt) = &target_event {
                        if req_evt != event { continue; }
                    }

                    if let EventScope::Channel(msg_channel) = scope {
                        if let Some(req_channel) = &target_channel {
                            msg_channel == req_channel
                        } else {
                            false 
                        }
                    } else {
                        scope == &client_scope
                    }
                },
                DbEvent::Insert { scope, .. } | 
                DbEvent::Update { scope, .. } | 
                DbEvent::Delete { scope, .. } => {
                    scope == &client_scope
                }
            };

            if should_yield {
                if let Ok(json_data) = serde_json::to_string(&msg) {
                    // Removed per-message info! log
                    yield Ok(Event::default().data(json_data));
                }
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

#[utoipa::path(
    get,
    path = "/healthz",
    responses((status = 200, description = "OK"))
)]
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

#[derive(Serialize, ToSchema)]
pub struct AppVersions {
    pub root: String,   // [NEW] The overall application binary version
    pub api: String,    // The API layer version
    pub core: String,   // The Core logic version
    pub vector: String, // The Vector engine version
}

#[utoipa::path(
    get,
    path = "/version",
    responses((status = 200, body = AppVersions))
)]
pub async fn get_versions_handler() -> Json<AppVersions> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    
    Json(AppVersions {
        root: current_version.clone(), // The binary version IS the root version
        api: current_version,
        core: apexkit_core::VERSION.to_string(),
        vector: apex_vector::VERSION.to_string(),
    })
}

// Helpers for helpers
pub fn get_current_model() -> String {
    std::env::var("APEX_VECTOR_MODEL").unwrap_or("all-minilm-l6-v2".to_string())
}
pub fn get_tenant_id_from_scope(scope: Option<&EventScope>) -> Option<String> {
    match scope {
        Some(EventScope::Tenant(id)) => Some(id.clone()),
        Some(EventScope::Sandbox(id)) => Some(id.clone()),
        _ => None,
    }
}

// Custom Extractor for Dynamic Base URL
pub struct BaseUrl(pub String);

impl<S> axum::extract::FromRequestParts<S> for BaseUrl
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> std::result::Result<Self, Self::Rejection> {
        // 1. Determine Protocol (Trust Proxy headers or default to http)
        let scheme = parts.headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("http");

        // 2. Get Host
        let host = parts.headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .ok_or((StatusCode::BAD_REQUEST, "Missing Host header".to_string()))?;

        // 3. Construct canonical URL
        Ok(BaseUrl(format!("{}://{}", scheme, host)))
    }
}

// =========================================================
// GRAPHQL HANDLERS
// =========================================================
async fn graphql_handler(
    auth: Option<Extension<Claims>>, // <--- Extract Claims
    State(state): State<AppState>, 
    req: GraphQLRequest
) -> GraphQLResponse {
    let schema = state.schema.read().await;
    
    // Inject claims into the execution context
    let mut request = req.into_inner();
    if let Some(Extension(claims)) = auth {
        request = request.data(claims);
    }
    
    schema.execute(request).await.into()
}

async fn tenant_graphql_handler(
    auth: Option<Extension<Claims>>, // <--- Extract Claims
    DatabaseConnection(db): DatabaseConnection, 
    State(state): State<AppState>, 
    scope: Option<Extension<EventScope>>, 
    req: GraphQLRequest
) -> GraphQLResponse {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let relation_loader = Arc::new(DataLoader::new(RelationLoader::new(db.clone()), tokio::spawn));
    let mut tenant_state = state.clone();
    tenant_state.db = db;
    
    match crate::graphql::build_schema(tenant_state, relation_loader).await {
        Ok(schema) => {
            let mut request = req.into_inner().data(event_scope);
            // Inject claims
            if let Some(Extension(claims)) = auth {
                request = request.data(claims);
            }
            schema.execute(request).await.into()
        },
        Err(e) => async_graphql::Response::from_errors(vec![async_graphql::ServerError::new(e.to_string(), None)]).into()
    }
}

async fn sandbox_graphql_handler(DatabaseConnection(db): DatabaseConnection, State(state): State<AppState>, scope: Option<Extension<EventScope>>, req: GraphQLRequest) -> GraphQLResponse {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let relation_loader = Arc::new(DataLoader::new(RelationLoader::new(db.clone()), tokio::spawn));
    let mut sandbox_state = state.clone();
    sandbox_state.db = db;
    match crate::graphql::build_schema(sandbox_state, relation_loader).await {
        Ok(schema) => schema.execute(req.into_inner().data(event_scope)).await.into(),
        Err(e) => async_graphql::Response::from_errors(vec![async_graphql::ServerError::new(e.to_string(), None)]).into()
    }
}



async fn graphql_playground() -> impl IntoResponse { axum::response::Html(async_graphql::http::playground_source(async_graphql::http::GraphQLPlaygroundConfig::new("/graphql"))) }
async fn sandbox_graphql_playground(Path(params): Path<HashMap<String, String>>) -> impl IntoResponse { let id = params.get("session_id").map(|s| s.as_str()).unwrap_or(""); axum::response::Html(async_graphql::http::playground_source(async_graphql::http::GraphQLPlaygroundConfig::new(&format!("/sandbox/{}/graphql", id)))) }
async fn tenant_graphql_playground(Path(params): Path<HashMap<String, String>>) -> impl IntoResponse { let id = params.get("tenant_id").map(|s| s.as_str()).unwrap_or(""); axum::response::Html(async_graphql::http::playground_source(async_graphql::http::GraphQLPlaygroundConfig::new(&format!("/tenant/{}/graphql", id)))) }

async fn metrics_handler(State(state): State<AppState>) -> Response {
    match &state.metrics {
        Some(handle) => handle.render().into_response(),
        None => (StatusCode::NOT_IMPLEMENTED, "Metrics not initialized").into_response(),
    }
}

// =========================================================
// ROUTER
// =========================================================
fn make_api_router() -> Router<AppState> {
    let upload_limit_mb = std::env::var("FILE_UPLOAD_LIMIT")
        .ok().and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10);

    Router::new()
        .route("/auth/login", post(auth_advanced::login))
        .route("/auth/register", post(auth_advanced::register))
        .route("/auth/roles", get(auth_advanced::list_roles_handler))
        .route("/auth/me", get(auth_advanced::get_me)) 
        // Github Auth Routes
        .route("/auth/github", get(auth_advanced::github_login))
        .route("/auth/github/callback", get(auth_advanced::github_callback))
        // Google Auth Routes
        .route("/auth/google", get(auth_advanced::google_login))
        .route("/auth/google/callback", get(auth_advanced::google_callback))
        .route("/auth/verify", get(auth_advanced::verify_email))
        .route("/auth/verify/resend", post(auth_advanced::resend_verification))
        .route("/collections", post(collections_and_records_routes::create_collection).get(collections_and_records_routes::list_collections))
        .route("/collections/{id}", get(collections_and_records_routes::get_collection).patch(collections_and_records_routes::update_collection).put(collections_and_records_routes::update_collection).delete(collections_and_records_routes::delete_collection))
        .route("/collections/{id}/records", post(collections_and_records_routes::create_record).get(collections_and_records_routes::list_records))
        // Advanced Query Endpoint
        .route("/collections/{id}/query", post(collections_and_records_routes::query_records_handler))
        .route("/collections/{id}/records/{record_id}", get(collections_and_records_routes::get_record).patch(collections_and_records_routes::update_record).put(collections_and_records_routes::update_record).delete(collections_and_records_routes::delete_record))
        .route("/collections/{id}/search", get(collections_and_records_routes::search_records))
        .route("/collections/{id}/instant-search", get(collections_and_records_routes::instant_search_handler))
        .route("/collections/{id}/search-vector", post(vector_routes::search_vector))
        .route("/collections/{id}/search-text-vector", post(vector_routes::query_vector_search))
        .route("/collections/{id}/get-vector/{record_id}", get(vector_routes::get_record_vector)) 
        .route("/collections/{id}/records/{record_id}/relations", post(collections_and_records_routes::create_relation).delete(collections_and_records_routes::delete_relation))
        .route("/storage/upload", post(storage::upload_file))
        .route("/storage/file/{*filename}", get(storage::serve_file)) 
        .route("/storage/files", get(storage::list_files))
        .route("/storage/files/{id}", axum::routing::delete(storage::delete_file))
        .route("/admin/storage/test", post(storage::test_s3_connection))
        .route("/admin/storage/migrate", post(storage::migrate_storage))
        .route("/admin/settings", get(settings::get_settings).patch(settings::update_settings).put(settings::update_settings))
        .route("/admin/smtp/test", post(auth_advanced::test_email_handler))
        .route("/admin/config", post(config_routes::set_config).get(config_routes::list_configs))
        .route("/admin/config/{key}", axum::routing::delete(config_routes::delete_config))
        .route("/admin/keys", get(key_routes::list_keys).post(key_routes::create_key))
        .route("/admin/keys/{id}", axum::routing::delete(key_routes::delete_key).patch(key_routes::update_key).put(key_routes::update_key))
        .route("/admin/system/reload", post(reload_system))
        .route("/admin/backup", post(backup_routes::trigger_backup_handler))
        .route("/admin/backups", get(backup_routes::list_backups_handler))
        .route("/admin/backups/{filename}", get(backup_routes::download_backup_handler))
        .route("/admin/restore-file", post(backup_routes::restore_from_file_handler))
        .route("/admin/restore", post(backup_routes::restore_handler))
        .route("/admin/users", get(auth_advanced::list_users_handler))
        .route("/admin/users/{id}", axum::routing::delete(auth_advanced::delete_user_handler).patch(auth_advanced::update_user_handler).put(auth_advanced::update_user_handler))
        .route("/admin/logs", get(list_audit_logs))
        .route("/admin/dashboard", get(get_dashboard_stats_handler))
        .route("/admin/import-data", post(import_data_routes::import_data_handler))
        .route("/admin/export-data/{id}", get(export_data_routes::export_data_handler))
        .route("/admin/import-schema", post(import_data_routes::import_schema_handler))
        .route("/admin/export-schema", get(export_data_routes::export_schema_handler))
        .route("/admin/export-scripts", get(export_data_routes::export_scripts_handler))
        .route("/admin/export-templates", get(export_data_routes::export_templates_handler))
        .route("/admin/export-ai-actions", get(export_data_routes::export_ai_actions_handler))
        .route("/admin/import-scripts", post(import_data_routes::import_scripts_handler))
        .route("/admin/import-templates", post(import_data_routes::import_templates_handler))
        .route("/admin/import-ai-actions", post(import_data_routes::import_ai_actions_handler))
        .route("/admin/collections/{id}/reindex", post(reindex_collection_handler))
        .route("/admin/collections/{id}/revectorize", post(vector_routes::revectorize_collection_handler))
        .route("/admin/ai/actions", get(ai_routes::list_actions).post(ai_routes::create_action))
        .route("/admin/ai/actions/{id}", axum::routing::delete(ai_routes::delete_action))
        .route("/ai/run/{slug}", post(ai_routes::run_action))
        .route("/admin/ai/edit-code", post(ai_routes::edit_code))
        .route("/admin/ai/sessions/{id}/chat", post(ai_architect::continue_chat))
        .route("/admin/ai/sessions/{id}/apply", post(ai_architect::apply_changes))
        .route("/admin/ai/sessions/{id}/publish", post(ai_architect::publish_plugin))
        .route("/admin/ai/plugins", get(ai_architect::list_plugins))
        // .route("/admin/ai/sessions/{id}", axum::routing::delete(ai_architect::delete_session))
        .route("/admin/scripts", get(script_routes::list_scripts).post(script_routes::create_script))
        .route("/admin/scripts/{id}", axum::routing::delete(script_routes::delete_script))
        .route("/run/{script_name}", post(script_routes::run_script))
        .route("/admin/templates", get(template_routes::list_templates).post(template_routes::create_template))
        .route("/admin/templates/{id}", axum::routing::patch(template_routes::update_template).put(template_routes::update_template).delete(template_routes::delete_template))
        .route("/admin/site/deploy", post(site_routes::deploy_site_handler))
        .route("/admin/site/files", get(site_routes::list_site_files_handler).delete(site_routes::delete_site_file_handler))
        .route("/sse", get(sse_handler))
        .layer(DefaultBodyLimit::max(upload_limit_mb * 1024 * 1024)) 
}

pub fn app_router(state: AppState) -> Router {
    let core_api = make_api_router().layer(middleware::from_fn_with_state(state.clone(), auth_middleware));
    let renderer_routes = Router::new().route("/render/{*slug}", get(renderer::render_view).post(renderer::render_view));
    let scalar_router: Router<AppState> = Scalar::with_url("/scalar", ApiDoc::openapi()).into();

    let sandbox_router = Router::new()
        .nest("/api/v1", core_api.clone())
        // Explicit route for sandbox renderer (2 params)
        .route("/render/{*slug}", get(renderer::render_sandbox_view).post(renderer::render_sandbox_view))
        .route("/graphql", post(sandbox_graphql_handler).get(sandbox_graphql_playground))
        .route("/scalar", get(sandbox_scalar_html))
        .route("/scalar/openapi.json", get(sandbox_openapi_json))
        .route("/ws", get(websocket::websocket_handler))
        .route("/logo", get(storage::serve_app_logo))
        .route("/app-name", get(settings::get_public_app_name))
        .layer(middleware::from_fn_with_state(state.clone(), sandbox_lifecycle_middleware));

    let tenant_path_router = Router::new()
        .nest("/api/v1", core_api.clone())
        // Explicit route for tenant renderer (2 params)
        .route("/render/{*slug}", get(renderer::render_tenant_view).post(renderer::render_tenant_view))
        .route("/graphql", post(tenant_graphql_handler).get(tenant_graphql_playground)) 
        .route("/scalar", get(tenant_scalar_html))
        .route("/scalar/openapi.json", get(tenant_openapi_json))
        .route("/ws", get(websocket::websocket_handler)) 
        .route("/logo", get(storage::serve_app_logo))
        .route("/app-name", get(settings::get_public_app_name))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(middleware::from_fn_with_state(state.clone(), tenant_resolver_middleware));

    let root_and_subdomain_router = Router::new()
        .nest("/api/v1", core_api.clone())
        .merge(renderer_routes)
        .merge(scalar_router)
        .route("/graphql", post(graphql_handler).get(graphql_playground))
        .route("/ws", get(websocket::websocket_handler)) 
        .route("/logo", get(storage::serve_app_logo))
        .route("/app-name", get(settings::get_public_app_name))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(middleware::from_fn_with_state(state.clone(), tenant_resolver_middleware));

    // Root handler (for /)
    let root_index_route = Router::new().route("/", get(assets::index_handler));
    Router::new()
        .merge(root_and_subdomain_router)
        .nest("/sandbox/{session_id}", sandbox_router)
        .nest("/tenant/{tenant_id}", tenant_path_router)
        .route(
            "/api/v1/admin/tenants", 
            get(tenant_routes::list_tenants_handler)
                .post(tenant_routes::create_tenant_handler)
                .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        )
        .route("/api/v1/admin/tenants/{id}", axum::routing::delete(tenant_routes::delete_tenant_handler).patch(tenant_routes::update_tenant_details).put(tenant_routes::update_tenant_details).layer(middleware::from_fn_with_state(state.clone(), auth_middleware)))
        .route("/api/v1/admin/tenants/{id}/status", axum::routing::patch(tenant_routes::update_tenant_status).layer(middleware::from_fn_with_state(state.clone(), auth_middleware)))
        .route("/api/v1/admin/ai/sessions", get(ai_architect::list_sessions).post(ai_architect::start_session).layer(middleware::from_fn_with_state(state.clone(), auth_middleware)))
        .route("/api/v1/admin/ai/sessions/{id}", axum::routing::delete(ai_routes::delete_session).layer(middleware::from_fn_with_state(state.clone(), auth_middleware)))
        
        .route("/metrics", get(metrics_handler))
        .route("/healthz", get(health_check)) 
        .route("/version", get(get_versions_handler))
        .route("/styles.css", get(serve_styles)) 
        
        // [NEW] Explicit Scoped Roots
        // These handle GET /tenant/{id} and GET /sandbox/{id} directly
        .route("/tenant/{tenant_id}", get(assets::scoped_index_handler))
        .route("/sandbox/{session_id}", get(assets::scoped_index_handler))
        
        .route("/_dashboard", get(assets::dashboard_handler))
        .route("/_dashboard/", get(assets::dashboard_handler))
        .route("/_dashboard/{*path}", get(assets::dashboard_handler))
        
        // Root Index
        .merge(root_index_route)
        
        // Fallback for everything else (Static Assets & SPA Routing)
        // Must be last
        .route("/{*path}", get(assets::serve_static_asset))
        .layer(middleware::from_fn(metrics_middleware))
        .layer(CompressionLayer::new())
        .with_state(state)
}

#[derive(OpenApi)]
#[openapi(
    paths(
        auth_advanced::login, auth_advanced::register, 
        collections_and_records_routes::list_collections, collections_and_records_routes::create_collection, collections_and_records_routes::get_collection, collections_and_records_routes::update_collection, collections_and_records_routes::delete_collection, 
        collections_and_records_routes::list_records, collections_and_records_routes::create_record, collections_and_records_routes::get_record, collections_and_records_routes::update_record, collections_and_records_routes::delete_record,
        collections_and_records_routes::search_records, collections_and_records_routes::instant_search_handler,
        collections_and_records_routes::query_records_handler, 
        storage::upload_file, storage::serve_file, storage::list_files, storage::delete_file,
        collections_and_records_routes::create_relation, collections_and_records_routes::delete_relation,
        config_routes::set_config, 
        settings::get_settings, settings::update_settings,
        auth_advanced::list_users_handler, auth_advanced::delete_user_handler,
        list_audit_logs,
        reload_system,
        ai_routes::list_actions, ai_routes::create_action, ai_routes::delete_action, ai_routes::run_action, ai_routes::edit_code,
        script_routes::list_scripts, script_routes::create_script, script_routes::delete_script, script_routes::run_script,
        template_routes::list_templates, template_routes::create_template, template_routes::update_template, template_routes::delete_template,
        ai_architect::start_session, ai_architect::continue_chat, ai_architect::publish_plugin, ai_architect::list_sessions,
        import_data_routes::import_data_handler,
        export_data_routes::export_data_handler,
        import_data_routes::import_schema_handler,
        export_data_routes::export_schema_handler,
        reindex_collection_handler,
        vector_routes::revectorize_collection_handler,
        vector_routes::search_vector,
        vector_routes::query_vector_search,
        vector_routes::get_record_vector,
        serve_styles,
        tenant_routes::create_tenant_handler,
        sse_handler
    ),
    components(schemas(
        CollectionResponse, AuthRequest, AuthResponse, RecordResponse, ProblemDetail, UserDto,
        CreateCollectionReq, UpdateCollection, RelationRequest, SearchQuery, RecordListResponse,
        collections_and_records_routes::AdvancedQueryRequest,
        config_routes::SetConfigRequest, 
        storage::FileResponse, storage::FileUploadRequest, storage::FileListResponse, storage::FileListQuery,
        settings::AppSettingsDto, settings::SmtpConfigDto, settings::StorageConfigDto, settings::S3ConfigDto, settings::SecurityConfigDto, settings::AiConfigDto,
        apexkit_core::models::Record,
        apexkit_core::models::StoredFile,
        apexkit_core::models::CronJob,
        apexkit_core::models::InstantResult,
        apexkit_core::ai_models::AiAction, apexkit_core::ai_models::CreateActionReq,
        apexkit_core::schema::CollectionSchema,
        apexkit_core::schema::CollectionPolicies,
        apexkit_core::schema::FieldDefinition,
        apexkit_core::schema::FieldType,
        ai_routes::ExecutePromptReq, ai_routes::CodeEditReq,
        apexkit_core::script_models::Script,
        apexkit_core::script_models::CreateScriptReq,
        apexkit_core::models::Template,
        apexkit_core::models::CreateTemplateReq,
        template_routes::UpdateTemplateReq,
        apexkit_core::ai_models::AiSession,
        apexkit_core::ai_models::ChatMessage,
        apexkit_core::ai_models::Plugin,
        apexkit_core::ai_models::CreateSessionReq,
        apexkit_core::ai_models::ChatReq,
        apexkit_core::models::DashboardStats,
        apexkit_core::models::ChartPoint,
        import_data_routes::ImportRequestDto,
        import_data_routes::ImportResponseDto,
        import_data_routes::ImportSchemaRequest,
        import_data_routes::ImportSchemaResponse,
        export_data_routes::ExportQuery,
        vector_routes::VectorSearchReq, 
        vector_routes::RecordVectorPath, 
        vector_routes::TextVectorSearchReq,
        tenant_routes::TenantResponse, tenant_routes::CreateTenantReq,
    )),
    tags((name = "ApexKit", description = "ApexKit API"))
)]
struct ApiDoc;