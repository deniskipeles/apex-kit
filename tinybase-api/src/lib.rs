// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/lib.rs ===========================
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
use tinybase_core::{
    models::Record,
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
use utoipa_scalar::{Scalar, Servable}; 
use validator::Validate;
use metrics_exporter_prometheus::PrometheusHandle;
use std::time::Instant;
use moka::future::Cache;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use async_graphql::dynamic::Schema;
use async_graphql::dataloader::DataLoader;
use async_graphql::Value;
use async_graphql::dynamic::{Object, Field, TypeRef, FieldFuture}; 

use tinybase_core::scripting::ScriptEngine;
use crate::sandbox_manager::SandboxManager;
use crate::graphql::RelationLoader;
use tinybase_core::models::DashboardData;

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

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<dyn Db>,
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
    pub embedder: Arc<tinybase_core::embeddings::EmbedderService>,
    pub vector_provider: Arc<dyn tinybase_core::VectorProvider>,
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
#[derive(Deserialize, ToSchema, IntoParams)] pub struct SearchQuery { pub q: String }
#[derive(Serialize, ToSchema)] pub struct RecordListResponse { items: Vec<RecordResponse>, total: i64 }

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

async fn sandbox_lifecycle_middleware(
    Path((session_id, _)): Path<(String, String)>,
    State(_state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    match SandboxManager::get_sandbox(&session_id).await {
        Ok(sandbox_db) => {
            req.extensions_mut().insert(sandbox_db);
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
        get_dashboard_stats_handler,
        reload_system,
        reindex_collection_handler,
        serve_styles,
        ai_routes::list_actions, ai_routes::create_action, ai_routes::delete_action, ai_routes::run_action, ai_routes::edit_code,
        script_routes::list_scripts, script_routes::create_script, script_routes::delete_script, script_routes::run_script,
        template_routes::list_templates, template_routes::create_template, template_routes::update_template, template_routes::delete_template,
        ai_architect::start_session, ai_architect::continue_chat, ai_architect::publish_plugin, ai_architect::list_sessions,
        import_data_routes::import_data_handler,
        export_data_routes::export_data_handler,
        vector_routes::revectorize_collection_handler,
        vector_routes::search_vector,
        vector_routes::query_vector_search
    ),
    components(schemas(
        CollectionResponse, AuthRequest, AuthResponse, RecordResponse, ProblemDetail, UserDto,
        CreateCollectionReq, UpdateCollection, RelationRequest, SearchQuery, RecordListResponse,
        config_routes::SetConfigRequest, 
        storage::FileResponse, storage::FileUploadRequest, storage::FileListResponse, storage::FileListQuery,
        settings::AppSettingsDto, settings::SmtpConfigDto, settings::StorageConfigDto, settings::S3ConfigDto, settings::SecurityConfigDto, settings::AiConfigDto,
        tinybase_core::models::Record,
        tinybase_core::models::StoredFile,
        tinybase_core::models::CronJob,
        tinybase_core::models::InstantResult,
        tinybase_core::ai_models::AiAction, tinybase_core::ai_models::CreateActionReq,
        tinybase_core::schema::CollectionSchema,
        tinybase_core::schema::CollectionPolicies,
        tinybase_core::schema::FieldDefinition,
        tinybase_core::schema::FieldType,
        ai_routes::ExecutePromptReq, ai_routes::CodeEditReq,
        tinybase_core::script_models::Script,
        tinybase_core::script_models::CreateScriptReq,
        tinybase_core::models::Template,
        tinybase_core::models::CreateTemplateReq,
        template_routes::UpdateTemplateReq,
        tinybase_core::ai_models::AiSession,
        tinybase_core::ai_models::ChatMessage,
        tinybase_core::ai_models::Plugin,
        tinybase_core::ai_models::CreateSessionReq,
        tinybase_core::ai_models::ChatReq,
        tinybase_core::models::DashboardData,
        tinybase_core::models::DashboardStats,
        tinybase_core::models::ChartPoint,
        import_data_routes::ImportRequestDto,
        import_data_routes::ImportResponseDto,
        export_data_routes::ExportQuery,
        vector_routes::VectorSearchReq, vector_routes::TextVectorSearchReq
    )),
    tags((name = "Tinybase", description = "Tinybase API"))
)]
struct ApiDoc;

// --- ROUTER FACTORY ---
fn make_api_router() -> Router<AppState> {
    Router::new()
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
}

// --- MAIN ROUTER ---

pub fn app_router(state: AppState) -> Router {
    SandboxManager::init();

    // 1. Core API (Reusable)
    let core_api = make_api_router()
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    // 2. Auth Routes (Shared)
    let auth_routes = Router::new()
        .route("/auth/login", post(login))
        .route("/auth/register", post(register))
        .route("/auth/github", get(auth_advanced::github_login))
        .route("/auth/github/callback", get(auth_advanced::github_callback))
        .route("/auth/verify", get(auth_advanced::verify_email))
        .route("/auth/verify/resend", post(auth_advanced::resend_verification));

    // 3. Renderer Routes (Public)
    let renderer_routes = Router::new()
        .route("/render/{*slug}", get(renderer::render_view).post(renderer::render_view));

    // 4. Scalar Documentation
    let scalar_router: Router<AppState> = Scalar::with_url("/scalar", ApiDoc::openapi()).into();

    // =========================================================
    // 5. THE SANDBOX FACTORY
    // =========================================================
    let sandbox_router = Router::new()
        // A. Nest the full API
        .nest("/api/v1", core_api.clone())
        // B. Merge Auth
        .merge(auth_routes.clone())
        // C. Specific Sandbox Renderer
        .route("/render/{*slug}", get(renderer::render_sandbox_view).post(renderer::render_sandbox_view))
        // D. Sandbox GraphQL
        .route("/graphql", post(sandbox_graphql_handler).get(graphql_playground))
        // E. Sandbox Scalar
        .merge(scalar_router.clone())

        // Middleware injects DB connection into Extensions
        .layer(middleware::from_fn_with_state(state.clone(), sandbox_lifecycle_middleware));

    // =========================================================
    // 6. ROOT ROUTER ASSEMBLY
    // =========================================================
    Router::new()
        // --- PRODUCTION ---
        .nest("/api/v1", core_api)
        .merge(auth_routes)
        .merge(renderer_routes)
        .merge(scalar_router)
        .route("/graphql", post(graphql_handler).get(graphql_playground))

        // --- SANDBOX ---
        .nest("/sandbox/{session_id}", sandbox_router)

        // --- GLOBAL STATIC & UTILS ---
        .route("/styles.css", get(serve_styles)) 
        .route("/metrics", get(metrics_handler))
        .route("/ws", get(websocket::websocket_handler)) 
        .route("/_dashboard", get(assets::dashboard_handler))
        .route("/_dashboard/{*path}", get(assets::dashboard_handler))
        .route("/static/{*path}", get(assets::serve_static_asset))
        .route("/logo", get(storage::serve_app_logo)) 
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
async fn get_collection(DatabaseConnection(db): DatabaseConnection, Path(id): Path<i64>) -> Result<Json<CollectionResponse>, AppError> {
    let c = db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Not found".into()))?;
    Ok(Json(CollectionResponse{id: c.id, name: c.name, schema: c.schema}))
}

#[utoipa::path(patch, path = "/api/v1/collections/{id}")]
async fn update_collection(DatabaseConnection(db): DatabaseConnection, Path(id): Path<i64>, Json(payload): Json<UpdateCollection>) -> Result<Json<CollectionResponse>, AppError> {
    let c = db.update_collection(id, payload.name, payload.schema).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(CollectionResponse{id: c.id, name: c.name, schema: c.schema}))
}

#[utoipa::path(delete, path = "/api/v1/collections/{id}")]
async fn delete_collection(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path(id): Path<i64>) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);
    if !matches!(claims, Some(c) if c.role == "admin") { return Err(AppError::Forbidden("Admins only".into())); }
    db.delete_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
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
    Path(id): Path<i64>, 
    Query(q): Query<QueryOptions>
) -> Result<Json<RecordListResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) { return Err(AppError::Forbidden("Read denied".into())); }

    let res = db.list_records(id, q).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(RecordListResponse{ items: res.items.into_iter().map(|r| RecordResponse{id: r.id, data: r.data}).collect(), total: res.total }))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/records",
    request_body = tinybase_core::models::Record,
    responses((status = 201, body = RecordResponse))
)]
async fn create_record(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path(id): Path<i64>, Json(p): Json<tinybase_core::models::Record>) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let policy = col.schema.as_ref().map(|s| s.policies.create.as_str()).unwrap_or("auth");
    if !policies::check_access(policy, claims.as_ref(), None) { return Err(AppError::Forbidden("Create denied".into())); }
    
    if let Some(schema) = &col.schema { validate_record(schema, &p.data).map_err(AppError::Validation)?; }

    let rid = db.create_record(id, &p.data).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(RecordResponse{id: rid, data: p.data}))
}

#[utoipa::path(
    get, 
    path = "/api/v1/collections/{id}/records/{record_id}",
    responses((status = 200, body = RecordResponse))
)]
async fn get_record(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path((cid, rid)): Path<(i64, i64)>, Query(q): Query<QueryOptions>) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(cid).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let r = db.get_record(cid, rid, q.expand).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Record not found".into()))?;
    
    let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), Some(&r.data)) { return Err(AppError::Forbidden("Read denied".into())); }

    Ok(Json(RecordResponse{id: r.id, data: r.data}))
}

#[utoipa::path(patch, path = "/api/v1/collections/{id}/records/{record_id}")]
async fn update_record(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path((cid, rid)): Path<(i64, i64)>, Json(p): Json<tinybase_core::models::Record>) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(cid).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let existing = db.get_record(cid, rid, None).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Record not found".into()))?;

    let policy = col.schema.as_ref().map(|s| s.policies.update.as_str()).unwrap_or("admin");
    if !policies::check_access(policy, claims.as_ref(), Some(&existing.data)) { return Err(AppError::Forbidden("Update denied".into())); }

    let r = db.update_record(cid, rid, &p.data).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(RecordResponse{id: r.id, data: r.data}))
}

#[utoipa::path(delete, path = "/api/v1/collections/{id}/records/{record_id}")]
async fn delete_record(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path((cid, rid)): Path<(i64, i64)>) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(cid).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let existing = db.get_record(cid, rid, None).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Record not found".into()))?;

    let policy = col.schema.as_ref().map(|s| s.policies.delete.as_str()).unwrap_or("admin");
    if !policies::check_access(policy, claims.as_ref(), Some(&existing.data)) { return Err(AppError::Forbidden("Delete denied".into())); }

    db.delete_record(cid, rid).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/search",
    params(SearchQuery),
    responses((status = 200, body = Vec<RecordResponse>))
)]
async fn search_records(auth: Option<Extension<Claims>>, DatabaseConnection(db): DatabaseConnection, Path(id): Path<i64>, Query(q): Query<SearchQuery>) -> Result<Json<Vec<RecordResponse>>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) { return Err(AppError::Forbidden("Search denied".into())); }

    let res = db.search_records(id, &q.q).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(res.into_iter().map(|r| RecordResponse{id: r.id, data: r.data}).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/instant-search",
    params(SearchQuery),
    responses((status = 200, body = Vec<tinybase_core::models::InstantResult>))
)]
pub async fn instant_search_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection, // Use DB connection extractor
    Path(id): Path<i64>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<tinybase_core::models::InstantResult>>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    
    let collection = db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Collection {} not found", id)))?;

    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied by policy".into()));
    }

    let results = db.instant_search(id, &params.q).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    Ok(Json(results))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/records/{record_id}/relations",
    request_body = RelationRequest,
    responses((status = 201, description = "Relation created"))
)]
async fn create_relation(DatabaseConnection(db): DatabaseConnection, Path((oc, oi)): Path<(i64, i64)>, Json(p): Json<RelationRequest>) -> Result<StatusCode, AppError> {
    db.create_relation(oc, oi, p.target_collection_id, p.target_record_id, &p.relation_name).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    delete,
    path = "/api/v1/collections/{id}/records/{record_id}/relations",
    request_body = RelationRequest,
    responses((status = 204, description = "Relation deleted"))
)]
async fn delete_relation(DatabaseConnection(db): DatabaseConnection, Path((oc, oi)): Path<(i64, i64)>, Json(p): Json<RelationRequest>) -> Result<StatusCode, AppError> {
    db.delete_relation(oc, oi, p.target_collection_id, p.target_record_id, &p.relation_name).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
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
    post,
    path = "/api/v1/collections/{id}/reindex",
    responses((status = 200, description = "Reindexing started"))
)]
pub async fn reindex_collection_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { 
        return Err(AppError::Forbidden("Admins only".into())); 
    }

    db.reindex_collection(id).await
        .map_err(|e| AppError::UnknownError(format!("Reindex failed: {}", e)))?;

    Ok(Json(serde_json::json!({ "success": true, "message": "Collection re-indexed successfully" })))
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

#[utoipa::path(
    delete,
    path = "/api/v1/admin/users/{id}",
    responses((status = 204, description = "User deleted"))
)]
pub async fn delete_user_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    db.delete_user(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
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
) -> Result<Json<tinybase_core::models::DashboardData>, AppError> {
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