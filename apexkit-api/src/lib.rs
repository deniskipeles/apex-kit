use axum::{
    extract::{Path, State, Query, Request, FromRef},
    http::{StatusCode, request::Parts}, 
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router, Extension,
};
use axum_extra::headers::{Authorization, authorization::Bearer, HeaderMapExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::fmt; 
use apexkit_core::{
    schema::CollectionSchema,
    validation::{validate_record, ValidationError},
    auth::{self, Claims},
    query::QueryOptions,
    jobs::{Job, JobQueue},
    realtime::DbEvent,
    policies,
    storage::StorageBackend, 
    security::Vault,
    Db,
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
use axum_extra::extract::Host;
use crate::tenant_manager::TenantManager;
use apexkit_core::realtime::EventScope;
use axum::response::sse::{Event, Sse};
use futures::stream::{Stream};
use std::convert::Infallible;
use apexkit_core::models::DashboardData;

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
pub mod sandbox_manager; 
pub mod vector_routes;
pub mod import_data_routes;
pub mod export_data_routes;
pub mod cli;
pub mod tenant_manager;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<dyn Db>,
    pub tenant_manager: Arc<TenantManager>,
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
    pub vector_provider: Arc<dyn apexkit_core::VectorProvider>,
}

// --- DTOs ---

#[derive(Serialize, ToSchema)] pub struct CollectionResponse { id: i64, name: String, schema: Option<CollectionSchema> }
#[derive(Deserialize, ToSchema, Validate)] pub struct UpdateCollection { #[validate(length(min = 1, max = 50))] name: Option<String>, schema: Option<CollectionSchema> }
#[derive(Deserialize, ToSchema, Validate)] pub struct CreateCollectionReq { #[validate(length(min = 1, max = 50))] name: String, schema: Option<CollectionSchema> }
#[derive(Serialize, ToSchema)] pub struct RecordResponse { id: i64, data: serde_json::Value }
#[derive(Deserialize, ToSchema, Validate)] pub struct AuthRequest { #[validate(email)] email: String, #[validate(length(min = 6))] password: String }
#[derive(Serialize, ToSchema)] pub struct AuthResponse { token: String, user: UserDto }
#[derive(Serialize, ToSchema)] pub struct UserDto { id: i64, email: String, role: String }
#[derive(Serialize, ToSchema)] struct ProblemDetail { error: String, message: String, details: Option<serde_json::Value>, status: u16 }
#[derive(Deserialize, ToSchema)] pub struct RelationRequest { target_collection_id: i64, target_record_id: i64, relation_name: String }
#[derive(Deserialize, ToSchema, IntoParams)] pub struct SearchQuery { pub q: String, pub limit: Option<usize> }
#[derive(Serialize, ToSchema)] pub struct RecordListResponse { items: Vec<RecordResponse>, total: i64 }

// --- Path Structs for Nested Routes ---
#[derive(Deserialize)]
pub struct IdPath {
    pub id: i64,
}

#[derive(Deserialize)]
pub struct RecordPath {
    pub id: i64, // Maps to collection {id}
    pub record_id: i64, // Maps to {record_id}
}

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

// --- DB EXTRACTOR ---
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

// --- MIDDLEWARE ---

async fn auth_middleware(State(_state): State<AppState>, mut req: Request, next: Next) -> Result<Response, StatusCode> {
    if let Some(auth_header) = req.headers().typed_get::<Authorization<Bearer>>() {
        if let Ok(claims) = auth::decode_jwt(auth_header.token()) {
            req.extensions_mut().insert(claims);
        }
    }
    Ok(next.run(req).await)
}

async fn metrics_middleware(req: Request, next: Next) -> Response {
    let _start = Instant::now();
    let response = next.run(req).await;
    response
}

// Use HashMap<String, String> to safely handle routes with different numbers of parameters
// (e.g. /graphql has 1 param {session_id}, /render/{slug} has 2 params {session_id, slug})
// Update the sandbox middleware to inject storage
async fn sandbox_lifecycle_middleware(
    Path(params): Path<HashMap<String, String>>,
    State(_state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let session_id = params.get("session_id").ok_or(StatusCode::NOT_FOUND)?;

    match SandboxManager::get_sandbox(session_id).await {
        Ok(sandbox_db) => {
            req.extensions_mut().insert(sandbox_db);

            // Inject Storage
            let storage_path = format!("sandboxes/session_{}/uploads", session_id);
            let public_url = "/api/v1/storage/file/".to_string(); 
            let storage: Arc<dyn StorageBackend> = Arc::new(
                apexkit_core::storage::LocalStorage::new(&storage_path, &public_url).await
            );
            req.extensions_mut().insert(storage);

            // FIX: Inject Scope
            req.extensions_mut().insert(EventScope::Sandbox(session_id.to_string()));

            Ok(next.run(req).await)
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

// --- OPENAPI DOCS ---
#[derive(OpenApi)]
#[openapi(
    paths(
        login, register, 
        list_collections, create_collection, get_collection, update_collection, delete_collection, 
        list_records, create_record, get_record, update_record, delete_record,
        search_records, instant_search_handler,
        storage::upload_file, storage::serve_file, storage::list_files, storage::delete_file,
        create_relation, delete_relation,
        config_routes::set_config, 
        settings::get_settings, settings::update_settings,
        list_users_handler, delete_user_handler,
        list_audit_logs,
        reload_system,
        ai_routes::list_actions, ai_routes::create_action, ai_routes::delete_action, ai_routes::run_action, ai_routes::edit_code,
        script_routes::list_scripts, script_routes::create_script, script_routes::delete_script, script_routes::run_script,
        template_routes::list_templates, template_routes::create_template, template_routes::update_template, template_routes::delete_template,
        ai_architect::start_session, ai_architect::continue_chat, ai_architect::publish_plugin, ai_architect::list_sessions,
        import_data_routes::import_data_handler,
        export_data_routes::export_data_handler,
        reindex_collection_handler,
        vector_routes::revectorize_collection_handler,
        vector_routes::search_vector,
        vector_routes::query_vector_search,
        serve_styles,
        create_tenant_handler,
        sse_handler
    ),
    components(schemas(
        CollectionResponse, AuthRequest, AuthResponse, RecordResponse, ProblemDetail, UserDto,
        CreateCollectionReq, UpdateCollection, RelationRequest, SearchQuery, RecordListResponse,
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
        apexkit_core::models::DashboardData,
        apexkit_core::models::DashboardStats,
        apexkit_core::models::ChartPoint,
        import_data_routes::ImportRequestDto,
        import_data_routes::ImportResponseDto,
        export_data_routes::ExportQuery,
        vector_routes::VectorSearchReq, 
        vector_routes::TextVectorSearchReq,
        TenantResponse, CreateTenantReq,
    )),
    tags((name = "ApexKit", description = "ApexKit API"))
)]
struct ApiDoc;

// --- UPDATED TENANT MIDDLEWARE ---
// Handles BOTH:
// 1. Path-based: /tenant/{id}/...
// 2. Subdomain-based: {id}.domain.com/...
// Handles Subdomains & Paths. Defaults to Root App if tenant not found.
async fn tenant_resolver_middleware(
    path_params: Option<Path<HashMap<String, String>>>,
    Host(host): Host,
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    
    let mut tenant_id = String::new();
    let host_parts: Vec<&str> = host.split('.').collect();
    if host_parts.len() >= 2 && host_parts[0] != "localhost" && host_parts[0] != "www" && host_parts[0] != "api" {
        tenant_id = host_parts[0].to_string();
    }
    if let Some(Path(params)) = path_params {
        if let Some(id) = params.get("tenant_id") {
            tenant_id = id.clone();
        }
    }

    if tenant_id.is_empty() {
        // Root App -> Inject Root Scope explicitly for consistency
        req.extensions_mut().insert(EventScope::Root);
        return Ok(next.run(req).await);
    }

    match state.tenant_manager.get_tenant(tenant_id.clone()).await {
        Ok(tenant_db) => {
            req.extensions_mut().insert(tenant_db.clone());
            
            let storage_path = format!("tenants/{}/uploads", tenant_id);
            let storage: Arc<dyn StorageBackend> = Arc::new(
                apexkit_core::storage::LocalStorage::new(&storage_path, "/api/v1/storage/file/").await
            );
            req.extensions_mut().insert(storage);

            // FIX: Inject Tenant Scope
            req.extensions_mut().insert(EventScope::Tenant(tenant_id));

            Ok(next.run(req).await)
        }
        Err(_) => {
            tracing::warn!("Tenant '{}' not found. Defaulting to Root.", tenant_id);
            req.extensions_mut().insert(EventScope::Root);
            Ok(next.run(req).await)
        }
    }
}


// Dynamic OpenAPI JSON for Tenants
async fn tenant_openapi_json(
    Path(params): Path<HashMap<String, String>>,
) -> Json<utoipa::openapi::OpenApi> {
    let tenant_id = params.get("tenant_id").map(|s| s.as_str()).unwrap_or("");
    let mut doc = ApiDoc::openapi();
    doc.servers = Some(vec![
        Server::new(format!("/tenant/{}", tenant_id)),
    ]);
    Json(doc)
}

// Dynamic Scalar HTML for Tenants
async fn tenant_scalar_html(
    Path(params): Path<HashMap<String, String>>
) -> impl IntoResponse {
    let tenant_id = params.get("tenant_id").map(|s| s.as_str()).unwrap_or("");
    let spec_url = format!("/tenant/{}/scalar/openapi.json", tenant_id);
    
    let html = format!(
r#"<!DOCTYPE html>
<html>
  <head>
    <title>ApexKit API Reference (Tenant)</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style> body {{ margin: 0; }} </style>
  </head>
  <body>
    <script id="api-reference" data-url="{}"></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  </body>
</html>"#,
        spec_url
    );
    axum::response::Html(html)
}

// --- DTO ---
#[derive(Deserialize, ToSchema)]
pub struct CreateTenantReq {
    #[schema(example = "customer-1")]
    pub tenant_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct TenantResponse {
    pub tenant_id: String,
    pub status: String,
}

// --- HANDLER: Create Tenant ---
#[utoipa::path(
    post,
    path = "/api/v1/admin/tenants",
    request_body = CreateTenantReq,
    responses(
        (status = 201, description = "Tenant created", body = TenantResponse),
        (status = 409, description = "Tenant already exists"),
        (status = 403, description = "Admin only")
    )
)]
pub async fn create_tenant_handler(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Json(payload): Json<CreateTenantReq>,
) -> Result<(StatusCode, Json<TenantResponse>), AppError> {
    // 1. Admin Check
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { 
        return Err(AppError::Forbidden("Only admins can create tenants".into())); 
    }

    // 2. Validate ID (Simple alphanumeric check)
    if !payload.tenant_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::InputValidation(validator::ValidationErrors::new())); // Or custom message
    }

    // 3. Create via Manager
    state.tenant_manager.create_tenant(payload.tenant_id.clone()).await
        .map_err(|e| AppError::UnknownError(e))?;

    Ok((StatusCode::CREATED, Json(TenantResponse {
        tenant_id: payload.tenant_id,
        status: "created".to_string()
    })))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/tenants",
    responses((status = 200, body = Vec<String>))
)]
pub async fn list_tenants_handler(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, AppError> {
    // 1. Admin Check
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { 
        return Err(AppError::Forbidden("Only admins can list tenants".into())); 
    }

    // 2. Fetch from Manager
    let tenants = state.tenant_manager.list_tenants().await
        .map_err(|e| AppError::UnknownError(e))?;

    Ok(Json(tenants))
}

// 2. Dynamic OpenAPI JSON
// Injects the "/sandbox/{id}" server URL into the spec so Scalar works
async fn sandbox_openapi_json(
    Path(params): Path<HashMap<String, String>>,
) -> Json<utoipa::openapi::OpenApi> {
    let session_id = params.get("session_id").map(|s| s.as_str()).unwrap_or("");
    
    let mut doc = ApiDoc::openapi();
    
    // Explicitly set the Server URL to the sandbox root
    doc.servers = Some(vec![
        Server::new(format!("/sandbox/{}", session_id)),
    ]);
    
    Json(doc)
}

// 3. Dynamic Scalar HTML (FIXED)
// Manually serves the HTML to avoid Axum version conflicts and trait issues
async fn sandbox_scalar_html(
    Path(params): Path<HashMap<String, String>>
) -> impl IntoResponse {
    let session_id = params.get("session_id").map(|s| s.as_str()).unwrap_or("");
    let spec_url = format!("/sandbox/{}/scalar/openapi.json", session_id);
    
    // We manually construct the HTML that Scalar uses.
    // This avoids the type errors with Scalar::new/with_url and Axum versions.
    let html = format!(
r#"<!DOCTYPE html>
<html>
  <head>
    <title>ApexKit API Reference (Sandbox)</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
      body {{ margin: 0; }}
    </style>
  </head>
  <body>
    <script
      id="api-reference"
      data-url="{}"
    ></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  </body>
</html>"#,
        spec_url
    );

    axum::response::Html(html)
}

// --- STORAGE EXTRACTOR ---
// Add this to lib.rs to allow handlers to receive dynamic storage
pub struct StorageConnection(pub Arc<dyn StorageBackend>);

impl<S> axum::extract::FromRequestParts<S> for StorageConnection
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // 1. Check for Tenant Storage override
        if let Some(s) = parts.extensions.get::<Arc<dyn StorageBackend>>() {
            return Ok(StorageConnection(s.clone()));
        }
        // 2. Fallback to Global
        let app_state = AppState::from_ref(state);
        Ok(StorageConnection(app_state.storage))
    }
}

// FIX: Added missing sandbox playground handler
async fn sandbox_graphql_playground(
    Path(params): Path<HashMap<String, String>>
) -> impl IntoResponse {
    let session_id = params.get("session_id").map(|s| s.as_str()).unwrap_or("");
    let endpoint = format!("/sandbox/{}/graphql", session_id);
    axum::response::Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new(&endpoint)
    ))
}

// FIX: Added missing tenant handlers
async fn tenant_graphql_playground(
    Path(params): Path<HashMap<String, String>>
) -> impl IntoResponse {
    let tenant_id = params.get("tenant_id").map(|s| s.as_str()).unwrap_or("");
    let endpoint = format!("/tenant/{}/graphql", tenant_id);
    axum::response::Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new(&endpoint)
    ))
}

async fn tenant_graphql_handler(
    DatabaseConnection(db): DatabaseConnection, 
    State(state): State<AppState>,              
    req: GraphQLRequest
) -> GraphQLResponse {
    let relation_loader = Arc::new(DataLoader::new(
        RelationLoader::new(db.clone()), 
        tokio::spawn
    ));
    let mut tenant_state = state.clone();
    tenant_state.db = db;
    match crate::graphql::build_schema(tenant_state, relation_loader).await {
        Ok(schema) => schema.execute(req.into_inner()).await.into(),
        Err(e) => {
            let err = async_graphql::ServerError::new(e.to_string(), None);
            async_graphql::Response::from_errors(vec![err]).into()
        }
    }
}

// --- ROUTER FACTORY ---
fn make_api_router() -> Router<AppState> {
    Router::new()
        // Auth Routes (Now nested under /api/v1 automatically)
        .route("/auth/login", post(login))
        .route("/auth/register", post(register))
        .route("/auth/github", get(auth_advanced::github_login))
        .route("/auth/github/callback", get(auth_advanced::github_callback))
        .route("/auth/verify", get(auth_advanced::verify_email))
        .route("/auth/verify/resend", post(auth_advanced::resend_verification))

        // Collections & Records
        .route("/collections", post(create_collection).get(list_collections))
        .route("/collections/{id}", get(get_collection).patch(update_collection).delete(delete_collection))
        .route("/collections/{id}/records", post(create_record).get(list_records))
        .route("/collections/{id}/records/{record_id}", get(get_record).patch(update_record).delete(delete_record))
        .route("/collections/{id}/search", get(search_records))
        .route("/collections/{id}/instant-search", get(instant_search_handler))
        .route("/collections/{id}/search-vector", post(vector_routes::search_vector))
        .route("/collections/{id}/search-text-vector", post(vector_routes::query_vector_search))
        .route("/collections/{id}/records/{record_id}/relations", post(create_relation).delete(delete_relation))
        
        // Storage
        .route("/storage/upload", post(storage::upload_file))
        .route("/storage/file/{filename}", get(storage::serve_file))
        .route("/storage/files", get(storage::list_files))
        .route("/storage/files/{id}", axum::routing::delete(storage::delete_file))
        
        // Admin / Config
        .route("/admin/config", post(config_routes::set_config))
        .route("/admin/settings", get(settings::get_settings).patch(settings::update_settings))
        .route("/admin/tenants", get(list_tenants_handler))
        .route("/admin/system/reload", post(reload_system))
        .route("/admin/users", get(list_users_handler))
        .route("/admin/users/{id}", axum::routing::delete(delete_user_handler))
        .route("/admin/logs", get(list_audit_logs))
        .route("/admin/dashboard", get(get_dashboard_stats_handler))
        .route("/admin/import-data", post(import_data_routes::import_data_handler))
        .route("/admin/export-data/{id}", get(export_data_routes::export_data_handler))
        .route("/admin/collections/{id}/reindex", post(reindex_collection_handler))
        .route("/admin/collections/{id}/revectorize", post(vector_routes::revectorize_collection_handler))
        
        // AI
        .route("/admin/ai/actions", get(ai_routes::list_actions).post(ai_routes::create_action))
        .route("/admin/ai/actions/{id}", axum::routing::delete(ai_routes::delete_action))
        .route("/ai/run/{slug}", post(ai_routes::run_action))
        
        // AI Architect
        .route("/admin/ai/sessions", post(ai_architect::start_session).get(ai_architect::list_sessions))
        .route("/admin/ai/sessions/{id}/chat", post(ai_architect::continue_chat))
        .route("/admin/ai/sessions/{id}/apply", post(ai_architect::apply_changes))
        .route("/admin/ai/sessions/{id}/publish", post(ai_architect::publish_plugin))
        .route("/admin/ai/plugins", get(ai_architect::list_plugins))
        .route("/admin/ai/edit-code", post(ai_routes::edit_code))

        // Scripting
        .route("/admin/scripts", get(script_routes::list_scripts).post(script_routes::create_script))
        .route("/admin/scripts/{id}", axum::routing::delete(script_routes::delete_script))
        .route("/run/{script_name}", post(script_routes::run_script))

        // Templates
        .route("/admin/templates", get(template_routes::list_templates).post(template_routes::create_template))
        .route("/admin/templates/{id}", axum::routing::patch(template_routes::update_template).delete(template_routes::delete_template))

        // Tenants
        .route("/admin/tenants", post(create_tenant_handler)) 

        // SSE
        .route("/sse", get(sse_handler))
}

// --- MAIN ROUTER ---

pub fn app_router(state: AppState) -> Router {
    SandboxManager::init();

    // 1. Core API (Reusable)
    let core_api = make_api_router()
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    // 3. Renderer Routes (Public)
    let renderer_routes = Router::new()
        .route("/render/{*slug}", get(renderer::render_view).post(renderer::render_view));

    // 4. Scalar Documentation (Production)
    let scalar_router: Router<AppState> = Scalar::with_url("/scalar", ApiDoc::openapi()).into();

    // =========================================================
    // 5. THE SANDBOX FACTORY
    // =========================================================
    let sandbox_router = Router::new()
        // A. Nest the full API (Includes /auth, /collections, etc)
        .nest("/api/v1", core_api.clone())
        // B. Merge Renderer
        .merge(renderer_routes.clone())
        // C. Sandbox GraphQL
        .route("/graphql", post(sandbox_graphql_handler).get(sandbox_graphql_playground))
        // D. Sandbox Scalar
        .route("/scalar", get(sandbox_scalar_html))
        .route("/scalar/openapi.json", get(sandbox_openapi_json))
        // WebSocket for Sandbox
        .route("/ws", get(websocket::websocket_handler))
        // E. Middleware injection
        .layer(middleware::from_fn_with_state(state.clone(), sandbox_lifecycle_middleware));

    // =========================================================
    // 6. THE TENANT FACTORY (Path-Based)
    // =========================================================
    // Handles /tenant/{tenant_id}/...
    let tenant_path_router = Router::new()
        .nest("/api/v1", core_api.clone())
        .merge(renderer_routes.clone())
        // Tenant graphql handler (it just needs a DB injection which tenant middleware provides)
        .route("/graphql", post(tenant_graphql_handler).get(tenant_graphql_playground)) 
        // Tenant Specific Scalar
        .route("/scalar", get(tenant_scalar_html))
        .route("/scalar/openapi.json", get(tenant_openapi_json))
        // WebSocket for Tenant (Path-based)
        .route("/ws", get(websocket::websocket_handler)) 
        // Middleware: Injects Tenant DB & Storage based on {tenant_id} path param
        .layer(middleware::from_fn_with_state(state.clone(), tenant_resolver_middleware));

    // =========================================================
    // 7. ROOT / SUBDOMAIN ROUTER
    // =========================================================
    // Handles root traffic OR subdomain traffic (tenant.app.com)
    let root_and_subdomain_router = Router::new()
        .nest("/api/v1", core_api.clone())
        .merge(renderer_routes)
        .merge(scalar_router)
        .route("/graphql", post(graphql_handler).get(graphql_playground))
        // WebSocket for Root & Subdomains
        // Placed HERE so it sits behind the tenant_resolver_middleware
        .route("/ws", get(websocket::websocket_handler)) 
        // Middleware: Checks Host header. If subdomain found, injects Tenant DB.
        // If no subdomain, passes through to Global DB.
        .layer(middleware::from_fn_with_state(state.clone(), tenant_resolver_middleware));

    // =========================================================
    // 8. FINAL ASSEMBLY
    // =========================================================
    Router::new()
        // Mount Root/Subdomain logic
        .merge(root_and_subdomain_router)

        // Mount Explicit Path Logic (Higher specificity)
        .nest("/sandbox/{session_id}", sandbox_router)
        .nest("/tenant/{tenant_id}", tenant_path_router)

        // Global Static Assets & Utils (Shared)
        .route("/styles.css", get(serve_styles)) 
        .route("/metrics", get(metrics_handler))
        .route("/_dashboard", get(assets::dashboard_handler))
        .route("/_dashboard/{*path}", get(assets::dashboard_handler))
        .route("/static/{*path}", get(assets::serve_static_asset))
        .route("/logo", get(storage::serve_app_logo)) 
        .route("/healthz", get(health_check)) 
        .route("/", get(assets::index_handler))

        .layer(middleware::from_fn(metrics_middleware))
        .with_state(state)
}

// --- GRAPHQL HANDLERS ---

async fn graphql_handler(State(state): State<AppState>, req: GraphQLRequest) -> GraphQLResponse {
    let schema = state.schema.read().await;
    schema.execute(req.into_inner()).await.into()
}

async fn sandbox_graphql_handler(
    DatabaseConnection(db): DatabaseConnection, 
    State(state): State<AppState>,              
    req: GraphQLRequest
) -> GraphQLResponse {
    let relation_loader = Arc::new(DataLoader::new(
        RelationLoader::new(db.clone()), 
        tokio::spawn
    ));
    let mut sandbox_state = state.clone();
    sandbox_state.db = db;
    match crate::graphql::build_schema(sandbox_state, relation_loader).await {
        Ok(schema) => schema.execute(req.into_inner()).await.into(),
        Err(e) => {
            let err = async_graphql::ServerError::new(e.to_string(), None);
            async_graphql::Response::from_errors(vec![err]).into()
        }
    }
}

async fn graphql_playground() -> impl IntoResponse {
    axum::response::Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/graphql")
    ))
}

async fn metrics_handler(State(state): State<AppState>) -> Response {
    match &state.metrics {
        Some(handle) => handle.render().into_response(),
        None => (StatusCode::NOT_IMPLEMENTED, "Metrics not initialized").into_response(),
    }
}

// --- CRUD HANDLERS (With Macros) ---

#[utoipa::path(
    get,
    path = "/api/v1/collections",
    responses((status = 200, body = Vec<CollectionResponse>))
)]
async fn list_collections(DatabaseConnection(db): DatabaseConnection) -> Result<Json<Vec<CollectionResponse>>, AppError> {
    let cols = db.list_collections().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(cols.into_iter().map(|c| CollectionResponse { id: c.id, name: c.name, schema: c.schema }).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections",
    request_body = CreateCollectionReq,
    responses((status = 201, body = CollectionResponse))
)]
async fn create_collection(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Json(payload): Json<CreateCollectionReq>) -> Result<(StatusCode, Json<CollectionResponse>), AppError> {
    let claims = auth.map(|Extension(c)| c);
    if !matches!(claims, Some(c) if c.role == "admin") { return Err(AppError::Forbidden("Admins only".into())); }
    
    let id = db.create_collection(&payload.name, &payload.schema).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(CollectionResponse{id, name: payload.name, schema: payload.schema})))
}

#[utoipa::path(get, path = "/api/v1/collections/{id}")]
async fn get_collection(DatabaseConnection(db): DatabaseConnection, Path(path): Path<IdPath>) -> Result<Json<CollectionResponse>, AppError> {
    let c = db.get_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Not found".into()))?;
    Ok(Json(CollectionResponse{id: c.id, name: c.name, schema: c.schema}))
}

#[utoipa::path(patch, path = "/api/v1/collections/{id}")]
async fn update_collection(DatabaseConnection(db): DatabaseConnection, Path(path): Path<IdPath>, Json(payload): Json<UpdateCollection>) -> Result<Json<CollectionResponse>, AppError> {
    let c = db.update_collection(path.id, payload.name, payload.schema).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(CollectionResponse{id: c.id, name: c.name, schema: c.schema}))
}

#[utoipa::path(delete, path = "/api/v1/collections/{id}")]
async fn delete_collection(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path(path): Path<IdPath>) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);
    if !matches!(claims, Some(c) if c.role == "admin") { return Err(AppError::Forbidden("Admins only".into())); }
    db.delete_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/records",
    responses((status = 200, body = RecordListResponse))
)]
async fn list_records(
    auth: Option<Extension<Claims>>, 
    DatabaseConnection(db): DatabaseConnection, 
    Path(path): Path<IdPath>, // <--- CHANGED: Uses struct to allow extra params
    Query(q): Query<QueryOptions>
) -> Result<Json<RecordListResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) { return Err(AppError::Forbidden("Read denied".into())); }

    let res = db.list_records(path.id, q).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(RecordListResponse{ items: res.items.into_iter().map(|r| RecordResponse{id: r.id, data: r.data}).collect(), total: res.total }))
}

// Helpers to get model and tenant
fn get_current_model() -> String {
    std::env::var("APEX_VECTOR_MODEL").unwrap_or("all-minilm-l6-v2".to_string())
}
fn get_tenant_id_from_scope(scope: Option<&EventScope>) -> Option<String> {
    match scope {
        Some(EventScope::Tenant(id)) => Some(id.clone()),
        Some(EventScope::Sandbox(id)) => Some(id.clone()),
        _ => None,
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/records",
    request_body = apexkit_core::models::Record,
    responses((status = 201, body = RecordResponse))
)]
async fn create_record(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection, 
    Path(path): Path<IdPath>, 
    Json(p): Json<apexkit_core::models::Record>
) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    
    let policy = col.schema.as_ref().map(|s| s.policies.create.as_str()).unwrap_or("auth");
    if !policies::check_access(policy, claims.as_ref(), None) { return Err(AppError::Forbidden("Create denied".into())); }
    
    // Hook: Before Create
    let data_to_save = match trigger_hooks(&state, "before_create", &col, None, &p.data, claims.as_ref()).await? {
        Some(d) => d,
        None => p.data
    };

    if let Some(schema) = &col.schema { validate_record(schema, &data_to_save).map_err(AppError::Validation)?; }

    let rid = db.create_record(path.id, &data_to_save).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    // Broadcast
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let _ = state.tx.send(DbEvent::Insert { 
        collection_id: path.id, 
        record_id: rid, 
        data: data_to_save.clone(), 
        scope: event_scope.clone()
    });

    // --- Audit Log ---
    let log_meta = serde_json::json!({
        "collection": col.name,
        "record_id": rid,
        "user_id": claims.as_ref().map(|c| c.uid),
        "action": "create"
    });
    let _ = db.log_audit_event("info", "Record Created", "api", Some(log_meta)).await;
    // ------------------------

    // Hook: After Create
    let _ = trigger_hooks(&state, "after_create", &col, Some(rid), &data_to_save, claims.as_ref()).await;

    // --- ASYNC JOBS ---
    if let Some(schema) = col.schema {
        // A. Vector Embeddings
        let current_tenant = get_tenant_id_from_scope(Some(&event_scope)); // Get scope
        let model_name = get_current_model(); // Get config
        for (field_name, def) in &schema.fields {
            if def.vectorize {
                if let Some(text_val) = data_to_save.get(field_name).and_then(|v| v.as_str()) {
                    let job = Job::GenerateEmbedding {
                        collection_id: path.id,
                        record_id: rid,
                        tenant_id: current_tenant.clone(),
                        field_name: field_name.clone(),
                        text_content: text_val.to_string(),
                        model: model_name.clone()
                    };
                    state.queue.enqueue(job).await;
                }
            }
        }
        
        // B. Full Text Search Indexing
        if schema.fields.values().any(|f| f.indexed) {
            let job = Job::IndexRecord {
                collection_id: path.id,
                record_id: rid,
                data: data_to_save.clone(),
                schema: schema.clone(),
                tenant_id: current_tenant.clone()
            };
            state.queue.enqueue(job).await;
        }
    }

    Ok(Json(RecordResponse{id: rid, data: data_to_save}))
}

#[utoipa::path(patch, path = "/api/v1/collections/{id}/records/{record_id}")]
async fn update_record(
    auth: Option<Extension<Claims>>, 
    State(state): State<AppState>, 
    scope: Option<Extension<EventScope>>, 
    DatabaseConnection(db): DatabaseConnection, 
    Path(path): Path<RecordPath>, 
    Json(p): Json<apexkit_core::models::Record>
) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let existing = db.get_record(path.id, path.record_id, None).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Record not found".into()))?;
    
    let policy = col.schema.as_ref().map(|s| s.policies.update.as_str()).unwrap_or("admin");
    if !policies::check_access(policy, claims.as_ref(), Some(&existing.data)) { return Err(AppError::Forbidden("Update denied".into())); }
    
    // Hook: Before Update
    let data_updates = match trigger_hooks(&state, "before_update", &col, Some(path.record_id), &p.data, claims.as_ref()).await? { 
        Some(d) => d, 
        None => p.data 
    };
    
    let r = db.update_record(path.id, path.record_id, &data_updates).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let _ = state.tx.send(DbEvent::Update { collection_id: path.id, record_id: path.record_id, data: r.data.clone(), scope: event_scope.clone() });
    
    // --- Audit Log ---
    let log_meta = serde_json::json!({
        "collection": col.name,
        "record_id": path.record_id,
        "user_id": claims.as_ref().map(|c| c.uid),
        "action": "update"
    });
    let _ = db.log_audit_event("info", "Record Updated", "api", Some(log_meta)).await;
    // ------------------------

    // Hook: After Update
    let _ = trigger_hooks(&state, "after_update", &col, Some(path.record_id), &r.data, claims.as_ref()).await;

    // --- ASYNC JOBS ---
    if let Some(schema) = col.schema {
        // A. Vector Embeddings
        let current_tenant = get_tenant_id_from_scope(Some(&event_scope)); // Get scope
        let model_name = get_current_model();
        for (field_name, def) in &schema.fields {
            if def.vectorize {
                // Check if the field exists in the UPDATED data (r.data)
                // Optimization: We could check if it changed, but enqueuing is safe
                if let Some(text_val) = r.data.get(field_name).and_then(|v| v.as_str()) {
                    let job = Job::GenerateEmbedding {
                        collection_id: path.id,
                        record_id: path.record_id,
                        field_name: field_name.clone(),
                        text_content: text_val.to_string(),
                        tenant_id: current_tenant.clone(),
                        model: model_name.clone() 
                    };
                    state.queue.enqueue(job).await;
                }
            }
        }
        
        // B. Full Text Search Indexing
        if schema.fields.values().any(|f| f.indexed) {
            let job = Job::IndexRecord {
                collection_id: path.id,
                record_id: path.record_id,
                data: r.data.clone(),
                schema: schema.clone(),
                tenant_id: current_tenant.clone()
            };
            state.queue.enqueue(job).await;
        }
    }

    Ok(Json(RecordResponse{id: r.id, data: r.data}))
}

#[utoipa::path(delete, path = "/api/v1/collections/{id}/records/{record_id}")]
async fn delete_record(
    auth: Option<Extension<Claims>>, 
    State(state): State<AppState>, 
    scope: Option<Extension<EventScope>>, 
    DatabaseConnection(db): DatabaseConnection, 
    Path(path): Path<RecordPath>
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let existing = db.get_record(path.id, path.record_id, None).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Record not found".into()))?;
    
    let policy = col.schema.as_ref().map(|s| s.policies.delete.as_str()).unwrap_or("admin");
    if !policies::check_access(policy, claims.as_ref(), Some(&existing.data)) { return Err(AppError::Forbidden("Delete denied".into())); }
    
    // Hook: Before Delete
    let _ = trigger_hooks(&state, "before_delete", &col, Some(path.record_id), &existing.data, claims.as_ref()).await?;
    
    db.delete_record(path.id, path.record_id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let _ = state.tx.send(DbEvent::Delete { collection_id: path.id, record_id: path.record_id, scope: event_scope.clone() });
    
    // --- ADDED: Audit Log ---
    let log_meta = serde_json::json!({
        "collection": col.name,
        "record_id": path.record_id,
        "user_id": claims.as_ref().map(|c| c.uid),
        "action": "delete"
    });
    let _ = db.log_audit_event("warning", "Record Deleted", "api", Some(log_meta)).await;
    // ------------------------

    // Hook: After Delete
    let _ = trigger_hooks(&state, "after_delete", &col, Some(path.record_id), &existing.data, claims.as_ref()).await;

    // --- ASYNC JOBS ---
    // Remove from Search Index
    if let Some(schema) = col.schema {
        if schema.fields.values().any(|f| f.indexed) {
            let current_tenant = get_tenant_id_from_scope(Some(&event_scope));
            let job = Job::DeleteFromIndex {
                tenant_id: current_tenant,
                collection_id: path.id,
                record_id: path.record_id,
            };
            state.queue.enqueue(job).await;
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/api/v1/collections/{id}/records/{record_id}", responses((status = 200, body = RecordResponse)))]
async fn get_record(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path(path): Path<RecordPath>, Query(q): Query<QueryOptions>) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let r = db.get_record(path.id, path.record_id, q.expand).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Record not found".into()))?;
    let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), Some(&r.data)) { return Err(AppError::Forbidden("Read denied".into())); }
    Ok(Json(RecordResponse{id: r.id, data: r.data}))
}

#[utoipa::path(get, path = "/api/v1/collections/{id}/search", params(SearchQuery), responses((status = 200, body = Vec<RecordResponse>)))]
async fn search_records(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path(path): Path<IdPath>, Query(q): Query<SearchQuery>) -> Result<Json<Vec<RecordResponse>>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) { return Err(AppError::Forbidden("Search denied".into())); }
    let res = db.search_records(path.id, &q.q).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(res.into_iter().map(|r| RecordResponse{id: r.id, data: r.data}).collect()))
}
#[utoipa::path(get, path = "/api/v1/collections/{id}/instant-search", params(SearchQuery), responses((status = 200, body = Vec<apexkit_core::models::InstantResult>)))]
pub async fn instant_search_handler(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path(path): Path<IdPath>, Query(params): Query<SearchQuery>) -> Result<Json<Vec<apexkit_core::models::InstantResult>>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let collection = db.get_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound(format!("Collection {} not found", path.id)))?;
    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) { return Err(AppError::Forbidden("Search denied by policy".into())); }
    let results = db.instant_search(path.id, &params.q, params.limit.unwrap_or(10)).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(results))
}
#[utoipa::path(post, path = "/api/v1/collections/{id}/records/{record_id}/relations", request_body = RelationRequest, responses((status = 201, description = "Relation created")))]
async fn create_relation(DatabaseConnection(db): DatabaseConnection, Path(path): Path<RecordPath>, Json(p): Json<RelationRequest>) -> Result<StatusCode, AppError> {
    db.create_relation(path.id, path.record_id, p.target_collection_id, p.target_record_id, &p.relation_name).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::CREATED)
}
#[utoipa::path(delete, path = "/api/v1/collections/{id}/records/{record_id}/relations", request_body = RelationRequest, responses((status = 204, description = "Relation deleted")))]
async fn delete_relation(DatabaseConnection(db): DatabaseConnection, Path(path): Path<RecordPath>, Json(p): Json<RelationRequest>) -> Result<StatusCode, AppError> {
    db.delete_relation(path.id, path.record_id, p.target_collection_id, p.target_record_id, &p.relation_name).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(post, path = "/api/v1/collections/{id}/reindex", responses((status = 200, description = "Reindexing started")))]
pub async fn reindex_collection_handler(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path(path): Path<IdPath>) -> Result<Json<serde_json::Value>, AppError> {
    if !matches!(auth.map(|e| e.0.role), Some(r) if r == "admin") { return Err(AppError::Forbidden("Admins only".into())); }
    db.reindex_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/system/reload",
    responses((status = 200, description = "System reloaded"))
)]
pub async fn reload_system(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
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
    state.scheduler.read().await.load_jobs(state.clone()).await;
    Ok(Json(serde_json::json!({ "status": "ok", "message": "System reloaded successfully" })))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    responses((status = 200, body = Vec<UserDto>))
)]
pub async fn list_users_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<Vec<UserDto>>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    let users = db.list_users().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(users.into_iter().map(|u| UserDto { id: u.id, email: u.email, role: u.role }).collect()))
}

#[utoipa::path(delete, path = "/api/v1/admin/users/{id}", responses((status = 204, description = "User deleted")))]
pub async fn delete_user_handler(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path(path): Path<IdPath>) -> Result<StatusCode, AppError> {
    if !matches!(auth.map(|e| e.0.role), Some(r) if r == "admin") { return Err(AppError::Forbidden("Admins only".into())); }
    db.delete_user(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
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

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = AuthRequest,
    responses((status = 200, body = AuthResponse), (status = 401, body = ProblemDetail))
)]
async fn login(DatabaseConnection(db): DatabaseConnection, Json(p): Json<AuthRequest>) -> Result<Json<AuthResponse>, AppError> {
    let u = db.get_user_by_email(&p.email).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::Unauthorized("Bad creds".into()))?;
    if !auth::verify_password(&p.password, &u.password_hash) { return Err(AppError::Unauthorized("Bad creds".into())); }
    let token = auth::create_jwt(u.id, &u.email, &u.role).map_err(|_| AppError::UnknownError("JWT fail".into()))?;
    let _ = db.log_audit_event("info", "Login", "auth", Some(serde_json::json!({"email": u.email}))).await;
    Ok(Json(AuthResponse{token, user: UserDto{id: u.id, email: u.email, role: u.role}}))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    request_body = AuthRequest,
    responses((status = 200, body = AuthResponse), (status = 400, body = ProblemDetail))
)]
async fn register(DatabaseConnection(db): DatabaseConnection, State(state): State<AppState>, Json(p): Json<AuthRequest>) -> Result<Json<AuthResponse>, AppError> {
    let hash = auth::hash_password(&p.password).map_err(|_| AppError::UnknownError("Hash fail".into()))?;
    let u = db.create_user(&p.email, &hash, "user").await.map_err(|_| AppError::UnknownError("User exists".into()))?;
    let token = auth::create_jwt(u.id, &u.email, &u.role).map_err(|_| AppError::UnknownError("JWT fail".into()))?;
    state.queue.enqueue(Job::SendWelcomeEmail { email: u.email.clone(), user_id: u.id }).await;
    let _ = db.log_audit_event("info", "Register", "auth", Some(serde_json::json!({"email": u.email}))).await;
    Ok(Json(AuthResponse{token, user: UserDto{id: u.id, email: u.email, role: u.role}}))
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
    // 1. Return Cache if exists
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

    // 2. Compile (Purge) using the correct DB (Prod or Sandbox)
    let css = css_compiler::compile_styles(db.clone()).await
        .map_err(|e| AppError::UnknownError(e))?;

    // 3. Update Cache
    {
        let mut cache = state.css_cache.write().await;
        *cache = css.clone();
    }

    Ok(Response::builder()
        .header("Content-Type", "text/css")
        .body(axum::body::Body::from(css))
        .unwrap())
}

#[utoipa::path(
    get,
    path = "/sse",
    responses((status = 200, description = "SSE Stream"))
)]
pub async fn sse_handler(
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>, // Injected by Tenant/Sandbox Middleware
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    
    // 1. Determine Scope
    let client_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    
    // 2. Subscribe to the Broadcast Channel
    let mut rx = state.tx.subscribe();

    // 3. Create a Stream
    let stream = async_stream::stream! {
        while let Ok(msg) = rx.recv().await {
            // 4. FILTER: Only yield events matching the client's scope
            if is_event_allowed(&msg, &client_scope) {
                if let Ok(json_data) = serde_json::to_string(&msg) {
                    yield Ok(Event::default().data(json_data));
                }
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

// Reuse logic to check scope
fn is_event_allowed(event: &DbEvent, client_scope: &EventScope) -> bool {
    let event_scope = match event {
        DbEvent::Insert { scope, .. } => scope,
        DbEvent::Update { scope, .. } => scope,
        DbEvent::Delete { scope, .. } => scope,
    };
    event_scope == client_scope
}


// --- HELPER: HOOK EXECUTOR ---
async fn trigger_hooks(
    state: &AppState,
    trigger: &str,           // e.g. "before_create"
    // FIX: Changed type from models::Collection to Collection (The one defined in core/lib.rs)
    collection: &apexkit_core::Collection, 
    record_id: Option<i64>,  // Null for before_create
    data: &serde_json::Value,// The payload or existing data
    auth: Option<&Claims>    // The current user
) -> Result<Option<serde_json::Value>, AppError> {
    
    let scripts = state.db.get_scripts_by_trigger(trigger).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let mut current_data = data.clone();
    let mut modified = false;

    for script in scripts {
        // Filter by collection if target is set
        if let Some(target) = &script.target_collection {
            if target != &collection.name { continue; }
        }

        // Construct 'e' Context
        let event_context = serde_json::json!({
            "record": {
                "id": record_id,
                "data": current_data
            },
            "collection": {
                "id": collection.id,
                "name": collection.name,
                // schema might be heavy, include if necessary or just name/id
                "schema": collection.schema 
            },
            "auth": auth.map(|c| serde_json::json!({
                "id": c.uid,
                "email": c.sub,
                "role": c.role
            })),
            "trigger": trigger
        });

        match state.script_engine.run_hook(
            &script.code, 
            event_context, 
            state.db.clone(), 
            state.embedder.clone(), 
            state.vector_provider.clone(),
            state.vault.clone()
        ).await {
            Ok(Some(new_data)) => {
                current_data = new_data;
                modified = true;
            },
            Ok(None) => { /* Continue */ },
            Err(err_msg) => {
                // FIX: Use Enum variant ConstraintViolation instead of struct syntax
                return Err(AppError::Validation(vec![
                    ValidationError::ConstraintViolation(
                        "_hook".to_string(), 
                        err_msg
                    )
                ]));
            }
        }
    }

    if modified { Ok(Some(current_data)) } else { Ok(None) }
}

#[utoipa::path(
    get,
    path = "/healthz",
    responses((status = 200, description = "OK"))
)]
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}