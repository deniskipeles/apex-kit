use axum::{
    extract::{Path, State, Query, Request, FromRef},
    http::{StatusCode, request::Parts}, // <--- Import Parts here
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
use tinybase_core::scripting::ScriptEngine;

use tinybase_core::models::DashboardData;
use crate::sandbox_manager::SandboxManager; 

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
    // Cache for thumbnails (Key: "filename_100x100", Value: Bytes)
    pub thumb_cache: Cache<String, Arc<Vec<u8>>>, 
    pub embedder: Arc<tinybase_core::embeddings::EmbedderService>,
    pub vector_provider: Arc<dyn tinybase_core::VectorProvider>,
}

// --- REMOVED CONFLICTING IMPL OF FromRef ---
// axum provides this automatically because AppState derives Clone.

// --- DTOs (Same as before) ---

#[derive(Serialize, ToSchema)]
pub struct CollectionResponse {
    id: i64,
    name: String,
    schema: Option<CollectionSchema>,
}

#[derive(Deserialize, ToSchema, Validate)]
pub struct UpdateCollection {
    #[validate(length(min = 1, max = 50))]
    name: Option<String>,
    schema: Option<CollectionSchema>,
}

#[derive(Deserialize, ToSchema, Validate)]
pub struct CreateCollectionReq {
    #[validate(length(min = 1, max = 50))]
    name: String,
    schema: Option<CollectionSchema>,
}

#[derive(Serialize, ToSchema)]
pub struct RecordResponse {
    id: i64,
    data: serde_json::Value,
}

#[derive(Deserialize, ToSchema, Validate)]
pub struct AuthRequest {
    #[validate(email)]
    email: String,
    #[validate(length(min = 6))]
    password: String,
}

#[derive(Serialize, ToSchema)]
pub struct AuthResponse {
    token: String,
    user: UserDto,
}

#[derive(Serialize, ToSchema)]
pub struct UserDto {
    id: i64,
    email: String,
    role: String,
}

#[derive(Serialize, ToSchema)]
struct ProblemDetail {
    error: String,
    message: String,
    details: Option<serde_json::Value>,
    status: u16,
}

#[derive(Deserialize, ToSchema)]
pub struct RelationRequest {
    target_collection_id: i64,
    target_record_id: i64,
    relation_name: String, 
}

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct SearchQuery {
    pub q: String,
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::LibsqlError(e) => write!(f, "Database Error: {}", e),
            AppError::JsonError(e) => write!(f, "JSON Error: {}", e),
            AppError::UnknownError(e) => write!(f, "Unknown Error: {}", e),
            AppError::NotFound(e) => write!(f, "Not Found: {}", e),
            AppError::Validation(e) => write!(f, "Schema Validation Error: {:?}", e),
            AppError::InputValidation(e) => write!(f, "Input Validation Error: {}", e),
            AppError::Unauthorized(e) => write!(f, "Unauthorized: {}", e),
            AppError::Forbidden(e) => write!(f, "Forbidden: {}", e),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            AppError::LibsqlError(e) => tracing::error!("Database error: {}", e),
            AppError::UnknownError(e) => tracing::error!("Unknown error: {}", e),
            _ => (),
        }

        let (status, problem) = match self {
            AppError::LibsqlError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemDetail {
                    error: "database_error".to_string(),
                    message: "A database error occurred.".to_string(),
                    details: Some(serde_json::json!({ "db_error": e.to_string() })),
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                },
            ),
            AppError::InputValidation(e) => (
                StatusCode::BAD_REQUEST,
                ProblemDetail {
                    error: "invalid_input".to_string(),
                    message: "Input validation failed".to_string(),
                    details: Some(serde_json::json!(e)),
                    status: StatusCode::BAD_REQUEST.as_u16(),
                }
            ),
            AppError::Unauthorized(e) => (
                StatusCode::UNAUTHORIZED,
                ProblemDetail {
                    error: "unauthorized".to_string(),
                    message: e,
                    details: None,
                    status: StatusCode::UNAUTHORIZED.as_u16(),
                }
            ),
            AppError::Forbidden(e) => (
                StatusCode::FORBIDDEN,
                ProblemDetail {
                    error: "forbidden".to_string(),
                    message: e,
                    details: None,
                    status: StatusCode::FORBIDDEN.as_u16(),
                }
            ),
            AppError::Validation(e) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ProblemDetail {
                    error: "schema_validation_error".to_string(),
                    message: "Record data does not match collection schema.".to_string(),
                    details: Some(serde_json::json!(e)),
                    status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
                },
            ),
            AppError::NotFound(e) => (
                StatusCode::NOT_FOUND, 
                ProblemDetail { error: "not_found".into(), message: e, details: None, status: 404 }
            ),
            AppError::JsonError(e) | AppError::UnknownError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR, 
                ProblemDetail { error: "server_error".into(), message: e, details: None, status: 500 }
            ),
        };

        (status, Json(problem)).into_response()
    }
}

// --- MIDDLEWARE ---

async fn auth_middleware(
    State(_state): State<AppState>, 
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(auth_header) = req.headers().typed_get::<Authorization<Bearer>>() {
        let token = auth_header.token();
        if let Ok(claims) = auth::decode_jwt(token) {
            req.extensions_mut().insert(claims);
        }
    }
    Ok(next.run(req).await)
}

async fn metrics_middleware(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let path = req.uri().path().to_owned();
    let method = req.method().to_string();

    let response = next.run(req).await;

    let latency = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    metrics::increment_counter!(
        "http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status.clone()
    );

    metrics::histogram!(
        "http_request_duration_seconds",
        latency,
        "method" => method,
        "path" => path,
        "status" => status
    );
      
    response
}

// --- SANDBOX MIDDLEWARE ---
// --- SANDBOX MIDDLEWARE (FIXED) ---

async fn sandbox_middleware(
    // FIX: Accept 2 path arguments because routes like /render/{slug} inside /sandbox/{id} have 2 params.
    // The first one is session_id, the second is dropped (_) because we only need session_id here.
    Path((session_id, _)): Path<(String, String)>,
    State(_state): State<AppState>, 
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    tracing::info!("[Sandbox Middleware] Intercepting request for session: {}", session_id);

    match SandboxManager::get_sandbox(&session_id).await {
        Ok(sandbox_db) => {
            req.extensions_mut().insert(sandbox_db);
            Ok(next.run(req).await)
        }
        Err(_) => {
            tracing::error!("Sandbox {} not found", session_id);
            Err(StatusCode::NOT_FOUND)
        }
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

// --- ROUTER ---

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
        ai_routes::list_actions, ai_routes::create_action, ai_routes::delete_action, ai_routes::run_action,
        script_routes::list_scripts, script_routes::create_script, script_routes::delete_script, script_routes::run_script,
        template_routes::list_templates,template_routes::create_template,template_routes::update_template,template_routes::delete_template,
        // NEW AI ARCHITECT ROUTES
        ai_architect::start_session, ai_architect::continue_chat, ai_architect::publish_plugin,ai_architect::list_sessions,
    ),
    components(schemas(
        CollectionResponse, AuthRequest, AuthResponse, RecordResponse, ProblemDetail, UserDto,
        CreateCollectionReq, UpdateCollection, RelationRequest, SearchQuery,
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
        ai_routes::ExecutePromptReq,
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
        tinybase_core::models::ChartPoint
    )),
    tags((name = "Tinybase", description = "Tinybase API"))
)]
struct ApiDoc;

pub fn app_router(state: AppState) -> Router {
    SandboxManager::init();

    // 1. API Routes
    let api_routes = Router::new()
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
        .route("/admin/collections/{id}/reindex", post(reindex_collection_handler))
        .route("/admin/collections/{id}/revectorize", post(vector_routes::revectorize_collection_handler))

        // AI
        .route("/admin/ai/actions", get(ai_routes::list_actions).post(ai_routes::create_action))
        .route("/admin/ai/actions/{id}", axum::routing::delete(ai_routes::delete_action))
        .route("/ai/run/{slug}", post(ai_routes::run_action))

        // NEW AI ARCHITECT ROUTES
        .route("/admin/ai/sessions", post(ai_architect::start_session).get(ai_architect::list_sessions))
        .route("/admin/ai/sessions/{id}/chat", post(ai_architect::continue_chat))
        .route("/admin/ai/sessions/{id}/publish", post(ai_architect::publish_plugin))
        .route("/admin/ai/plugins", get(ai_architect::list_plugins))

        // SCRIPTING ENGINE
        .route("/admin/scripts", get(script_routes::list_scripts).post(script_routes::create_script))
        .route("/admin/scripts/{id}", axum::routing::delete(script_routes::delete_script))
        .route("/run/{script_name}", post(script_routes::run_script))

        // Templates CRUD Operations
        .route("/admin/templates", get(template_routes::list_templates).post(template_routes::create_template))
        .route("/admin/templates/{id}", axum::routing::patch(template_routes::update_template).delete(template_routes::delete_template))

        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware)); 

    // 2. Auth Routes
    let auth_routes = Router::new()
        .route("/auth/login", post(login))
        .route("/auth/register", post(register))
        .route("/auth/github", get(auth_advanced::github_login))
        .route("/auth/github/callback", get(auth_advanced::github_callback))
        .route("/auth/verify", get(auth_advanced::verify_email))
        .route("/auth/verify/resend", post(auth_advanced::resend_verification));

    // Public Routes
    let renderer_routes = Router::new()
        .route("/render/{*slug}", get(renderer::render_view).post(renderer::render_view));

     // --- SANDBOX ROUTES (Updated) ---
     let sandbox_routes = Router::new()
        // Use the Sandbox-specific handlers (they accept 2 path parameters)
        .route("/render/{*slug}", get(renderer::render_sandbox_view).post(renderer::render_sandbox_view))
        .route("/run/{script_name}", post(script_routes::run_sandbox_script))
        .layer(middleware::from_fn_with_state(state.clone(), sandbox_middleware));

    // 3. Construct Main Router
    let app_router = Router::new()
        .nest("/api/v1", auth_routes.merge(api_routes))
        .nest("/sandbox/{session_id}", sandbox_routes) 
        .merge(renderer_routes)
        
        .route("/styles.css", get(serve_styles)) 
        .route("/metrics", get(metrics_handler))
        .route("/ws", get(websocket::websocket_handler))
        .route("/graphql", post(graphql_handler).get(graphql_playground))

        .route("/_dashboard", get(assets::dashboard_handler))
        .route("/_dashboard/{*path}", get(assets::dashboard_handler))
        
        .route("/static/{*path}", get(assets::serve_static_asset))
        // --- LOGO ENDPOINT ---
        // Serves /logo or /logo?thumb=50x50
        .route("/logo", get(storage::serve_app_logo)) 

        
        .route("/", get(assets::index_handler))

        .layer(middleware::from_fn(metrics_middleware));
    
    // 4. Inject State
    let app_router_with_state = app_router.with_state(state);

    let scalar_router: Router = Scalar::with_url("/scalar", ApiDoc::openapi()).into();

    app_router_with_state.merge(scalar_router)
}


async fn graphql_handler(
    State(state): State<AppState>, 
    req: GraphQLRequest
) -> GraphQLResponse {
    let schema = state.schema.read().await;
    schema.execute(req.into_inner()).await.into()
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

    tracing::info!("System Reload Triggered by Admin...");

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

    tracing::info!("System Reload Complete.");
    Ok(Json(serde_json::json!({ "status": "ok", "message": "System reloaded successfully" })))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/instant-search",
    params(SearchQuery),
    responses((status = 200, body = Vec<tinybase_core::models::InstantResult>))
)]
pub async fn instant_search_handler(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<tinybase_core::models::InstantResult>>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    
    let collection = state.db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Collection {} not found", id)))?;

    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied by policy".into()));
    }

    let results = state.db.instant_search(id, &params.q).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    Ok(Json(results))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    responses((status = 200, body = Vec<UserDto>))
)]
pub async fn list_users_handler(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
) -> Result<Json<Vec<UserDto>>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    let users = state.db.list_users().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(users.into_iter().map(|u| UserDto { id: u.id, email: u.email, role: u.role }).collect()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/users/{id}",
    responses((status = 204, description = "User deleted"))
)]
pub async fn delete_user_handler(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    
    state.db.delete_user(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/logs",
    responses((status = 200, body = Vec<serde_json::Value>))
)]
pub async fn list_audit_logs(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let logs = state.db.list_audit_logs().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    Ok(Json(logs))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = AuthRequest,
    responses((status = 200, body = AuthResponse), (status = 401, body = ProblemDetail))
)]
async fn login(
    State(state): State<AppState>,
    Json(payload): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    payload.validate().map_err(AppError::InputValidation)?;

    let user = state.db.get_user_by_email(&payload.email).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::Unauthorized("Invalid credentials".into()))?;

    if !auth::verify_password(&payload.password, &user.password_hash) {
        tracing::warn!("Failed login attempt for {}", payload.email);
        return Err(AppError::Unauthorized("Invalid credentials".into()));
    }

    let token = auth::create_jwt(user.id, &user.email, &user.role)
        .map_err(|_| AppError::UnknownError("Token generation failed".into()))?;

    let _ = state.db.log_audit_event(
        "info",
        "User Login",
        "auth",
        Some(serde_json::json!({ "email": user.email }))
    ).await;

    Ok(Json(AuthResponse {
        token,
        user: UserDto { id: user.id, email: user.email, role: user.role },
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    request_body = AuthRequest,
    responses((status = 200, body = AuthResponse), (status = 400, body = ProblemDetail))
)]
async fn register(
    State(state): State<AppState>,
    Json(payload): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    payload.validate().map_err(AppError::InputValidation)?;

    let hash = auth::hash_password(&payload.password)
        .map_err(|_| AppError::UnknownError("Hashing failed".into()))?;

    let user = state.db.create_user(&payload.email, &hash, "user").await
        .map_err(|_| AppError::UnknownError("Failed to create user (email might exist)".into()))?;

    let token = auth::create_jwt(user.id, &user.email, &user.role)
        .map_err(|_| AppError::UnknownError("Token generation failed".into()))?;

    state.queue.enqueue(Job::SendWelcomeEmail { 
        email: user.email.clone(), 
        user_id: user.id 
    }).await;

    let _ = state.db.log_audit_event("info", "User Registered", "auth", Some(serde_json::json!({"email": user.email}))).await;

    Ok(Json(AuthResponse {
        token,
        user: UserDto { id: user.id, email: user.email, role: user.role },
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections",
    request_body = CreateCollectionReq,
    responses((status = 201, body = CollectionResponse))
)]
async fn create_collection(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Json(payload): Json<CreateCollectionReq>,
) -> Result<(StatusCode, Json<CollectionResponse>), AppError> {
    let claims = auth.map(|Extension(c)| c);

    if let Some(claims) = claims {
        if claims.role != "admin" {
             return Err(AppError::Forbidden("Only admins can create collections".into()));
        }
    } else {
        return Err(AppError::Unauthorized("Admin login required".into()));
    }

    payload.validate().map_err(AppError::InputValidation)?;

    let id = state.db.create_collection(&payload.name, &payload.schema).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let _ = state.db.log_audit_event("info", "Collection Created", "system", Some(serde_json::json!({"name": payload.name}))).await;

    Ok((StatusCode::CREATED, Json(CollectionResponse { id, name: payload.name, schema: payload.schema })))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections",
    responses((status = 200, body = Vec<CollectionResponse>))
)]
async fn list_collections(State(state): State<AppState>) -> Result<Json<Vec<CollectionResponse>>, AppError> {
    let cols = state.db.list_collections().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(cols.into_iter().map(|c| CollectionResponse { id: c.id, name: c.name, schema: c.schema }).collect()))
}

#[utoipa::path(get, path = "/api/v1/collections/{id}")]
async fn get_collection(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<CollectionResponse>, AppError> {
    let c = state.db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Collection not found".into()))?;
    Ok(Json(CollectionResponse { id: c.id, name: c.name, schema: c.schema }))
}

#[utoipa::path(patch, path = "/api/v1/collections/{id}")]
async fn update_collection(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>, 
    Path(id): Path<i64>, 
    Json(payload): Json<UpdateCollection>
) -> Result<Json<CollectionResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    if let Some(claims) = claims {
        if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    } else { return Err(AppError::Unauthorized("Admin login required".into())); }

    payload.validate().map_err(AppError::InputValidation)?;
    let c = state.db.update_collection(id, payload.name, payload.schema).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(CollectionResponse { id: c.id, name: c.name, schema: c.schema }))
}

#[utoipa::path(delete, path = "/api/v1/collections/{id}")]
async fn delete_collection(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>, 
    Path(id): Path<i64>
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);

    if let Some(claims) = claims {
        if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }
    } else { return Err(AppError::Unauthorized("Admin login required".into())); }

    state.db.delete_collection(id).await.map_err(|_| AppError::UnknownError("DB Error".into()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/records",
    request_body = Record,
    responses((status = 201, body = RecordResponse))
)]
async fn create_record(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<Record>, 
) -> Result<(StatusCode, Json<RecordResponse>), AppError> {
    let claims = auth.map(|Extension(c)| c);

    let collection = state.db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Collection {} not found", id)))?;

    let policy = collection.schema.as_ref().map(|s| s.policies.create.as_str()).unwrap_or("auth");
    if !policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Create denied by policy".into()));
    }

    if let Some(schema) = &collection.schema {
        validate_record(schema, &payload.data).map_err(AppError::Validation)?;
    }

    let record_id = state.db.create_record(id, &payload.data).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let _ = state.tx.send(DbEvent::Insert { collection_id: id, record_id, data: payload.data.clone() });

    // --- Trigger Vectorization ---
    // Fetch schema to check which fields need embedding
    if let Some(col) = state.db.get_collection(id).await.unwrap_or(None) {
        if let Some(schema) = col.schema {
            for (field_name, def) in schema.fields {
                // If field is marked for vectorization AND data exists
                if def.vectorize {
                    if let Some(text_val) = payload.data.get(&field_name).and_then(|v| v.as_str()) {
                        let job = tinybase_core::jobs::Job::GenerateEmbedding {
                            collection_id: id,
                            record_id: record_id,
                            field_name: field_name,
                            text_content: text_val.to_string()
                        };
                        state.queue.enqueue(job).await;
                    }
                }
            }
        }
    }

    Ok((StatusCode::CREATED, Json(RecordResponse { id: record_id, data: payload.data })))
}

#[derive(Serialize, ToSchema)]
pub struct RecordListResponse {
    items: Vec<RecordResponse>,
    total: i64,
}
#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/records",
    responses((status = 200, body = RecordListResponse))
)]
async fn list_records(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<QueryOptions>,
) -> Result<Json<RecordListResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    let collection = state.db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Collection {} not found", id)))?;

    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Read denied by policy".into()));
    }
    
    let result = state.db.list_records(id, params).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    Ok(Json(RecordListResponse {
        items: result.items.into_iter().map(|r| RecordResponse { id: r.id, data: r.data }).collect(),
        total: result.total,
    }))
}

#[utoipa::path(
    get, 
    path = "/api/v1/collections/{id}/records/{record_id}",
    params(
        ("id" = i64, Path, description = "Collection ID"),
        ("record_id" = i64, Path, description = "Record ID"),
        ("expand" = Option<String>, Query, description = "Expand relations (e.g. 'author, comments(5)')")
    ),
    responses((status = 200, body = RecordResponse))
)]
async fn get_record(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>, 
    Path((cid, rid)): Path<(i64, i64)>,
    Query(opts): Query<QueryOptions>, // Accept query params
) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    let collection = state.db.get_collection(cid).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;

    // Pass the expand option
    let r = state.db.get_record(cid, rid, opts.expand).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Record not found".into()))?;

    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), Some(&r.data)) {
        return Err(AppError::Forbidden("Read denied by policy".into()));
    }

    Ok(Json(RecordResponse { id: r.id, data: r.data }))
}

#[utoipa::path(patch, path = "/api/v1/collections/{id}/records/{record_id}")]
async fn update_record(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>, 
    Path((cid, rid)): Path<(i64, i64)>, 
    Json(payload): Json<Record>
) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    let collection = state.db.get_collection(cid).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;

    let existing = state.db.get_record(cid, rid, None).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Record not found".into()))?;

    let policy = collection.schema.as_ref().map(|s| s.policies.update.as_str()).unwrap_or("admin");
    if !policies::check_access(policy, claims.as_ref(), Some(&existing.data)) {
        return Err(AppError::Forbidden("Update denied by policy".into()));
    }

    let r = state.db.update_record(cid, rid, &payload.data).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let _ = state.tx.send(DbEvent::Update { collection_id: cid, record_id: rid, data: r.data.clone() });

    Ok(Json(RecordResponse { id: r.id, data: r.data }))
}

#[utoipa::path(delete, path = "/api/v1/collections/{id}/records/{record_id}")]
async fn delete_record(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>, 
    Path((cid, rid)): Path<(i64, i64)>
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);

    let collection = state.db.get_collection(cid).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;

    let existing = state.db.get_record(cid, rid, None).await.map_err(|e| AppError::UnknownError(e.to_string()))?.ok_or(AppError::NotFound("Record not found".into()))?;

    let policy = collection.schema.as_ref().map(|s| s.policies.delete.as_str()).unwrap_or("admin");
    if !policies::check_access(policy, claims.as_ref(), Some(&existing.data)) {
        return Err(AppError::Forbidden("Delete denied by policy".into()));
    }

    state.db.delete_record(cid, rid).await.map_err(|_| AppError::UnknownError("DB Error".into()))?;
    
    let _ = state.tx.send(DbEvent::Delete { collection_id: cid, record_id: rid });

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/search",
    params(SearchQuery),
    responses((status = 200, body = Vec<RecordResponse>))
)]
async fn search_records(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    
    let collection = state.db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Collection {} not found", id)))?;

    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied by policy".into()));
    }

    let records = state.db.search_records(id, &params.q).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    Ok(Json(records.into_iter().map(|r| RecordResponse { id: r.id, data: r.data }).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/records/{record_id}/relations",
    request_body = RelationRequest,
    responses((status = 201, description = "Relation created"))
)]
async fn create_relation(
    State(state): State<AppState>,
    Path((origin_col_id, origin_rec_id)): Path<(i64, i64)>,
    Json(payload): Json<RelationRequest>,
) -> Result<StatusCode, AppError> {
    state.db.create_relation(
        origin_col_id, 
        origin_rec_id, 
        payload.target_collection_id, 
        payload.target_record_id, 
        &payload.relation_name
    ).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    delete,
    path = "/api/v1/collections/{id}/records/{record_id}/relations",
    request_body = RelationRequest,
    responses((status = 204, description = "Relation deleted"))
)]
async fn delete_relation(
    State(state): State<AppState>,
    Path((origin_col_id, origin_rec_id)): Path<(i64, i64)>,
    Json(payload): Json<RelationRequest>,
) -> Result<StatusCode, AppError> {
    state.db.delete_relation(
        origin_col_id, 
        origin_rec_id, 
        payload.target_collection_id, 
        payload.target_record_id, 
        &payload.relation_name
    ).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/styles.css",
    responses((status = 200, description = "Purged Tailwind CSS", content_type = "text/css"))
)]
pub async fn serve_styles(
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    // 1. Return Cache if exists
    {
        let cache = state.css_cache.read().await;
        if !cache.is_empty() {
            return Ok(Response::builder()
                .header("Content-Type", "text/css")
                .header("Cache-Control", "public, max-age=60") // 1 minute cache (invalidated by logic)
                .body(axum::body::Body::from(cache.clone()))
                .unwrap());
        }
    }

    // 2. Compile (Purge) if empty
    // This runs the regex logic over the 3MB file
    let css = css_compiler::compile_styles(state.db.clone()).await
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
    path = "/api/v1/admin/dashboard",
    responses((status = 200, body = DashboardData))
)]
pub async fn get_dashboard_stats_handler(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
) -> Result<Json<DashboardData>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let data = state.db.get_dashboard_stats().await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    Ok(Json(data))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/reindex",
    responses((status = 200, description = "Reindexing started"))
)]
pub async fn reindex_collection_handler(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    // 1. Security Check (Admins Only)
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { 
        return Err(AppError::Forbidden("Admins only".into())); 
    }

    // 2. Run Reindex
    state.db.reindex_collection(id).await
        .map_err(|e| AppError::UnknownError(format!("Reindex failed: {}", e)))?;

    Ok(Json(serde_json::json!({ "success": true, "message": "Collection re-indexed successfully" })))
}