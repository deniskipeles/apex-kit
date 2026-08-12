use super::middleware::{
    auth::auth_middleware, sandbox_resolver::sandbox_lifecycle_middleware,
    tenant_resolver::tenant_resolver_middleware,
};
use crate::AppState;
use crate::graphql::handlers::{
    graphql_handler, graphql_playground, sandbox_graphql_handler, sandbox_graphql_playground,
    tenant_graphql_handler, tenant_graphql_playground,
};
use crate::replication;
use crate::utils::{
    sandbox_openapi_json, sandbox_scalar_html, tenant_openapi_json, tenant_scalar_html,
};
use axum::extract::DefaultBodyLimit;
use axum::{
    Router,
    middleware::{self},
    routing::{get, post},
};
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::api::{
    ai::{actions as ai_routes, architect as ai_architect},
    data::{
        collections, records,
        search::{ose, sql_query, vector as vector_routes},
    },
    realtime::{sse, websocket},
    scripts as script_routes,
    site::{assets, serve_css, spa, ssr},
    storage,
    system::{api_keys, backup as backup_routes, config as config_routes, settings},
    templates as template_routes,
    workspace::{sandbox as sandbox_routes, tenant as tenant_routes},
};
use crate::dto::{
    AuthRequest, AuthResponse, CollectionResponse, CreateCollectionReq, ProblemDetail,
    RecordListResponse, RecordResponse, RelationRequest, SearchQuery, UpdateCollection, UserDto,
};

// =========================================================
// ROUTER
// =========================================================
fn make_api_router() -> Router<AppState> {
    let upload_limit_mb = std::env::var("FILE_UPLOAD_LIMIT")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10);

    Router::new()
        .route("/auth/login", post(crate::api::auth::password_auth::login))
        .route(
            "/auth/register",
            post(crate::api::auth::password_auth::register),
        )
        .route(
            "/auth/roles",
            get(crate::api::auth::user_rud_ops::list_roles_handler),
        )
        .route(
            "/auth/me",
            get(crate::api::auth::user_rud_ops::get_me)
                .patch(crate::api::auth::user_rud_ops::update_me)
                .put(crate::api::auth::user_rud_ops::update_me),
        )
        // Github Auth Routes
        .route("/auth/github", get(crate::api::auth::oauth2::github_login))
        .route(
            "/auth/github/callback",
            get(crate::api::auth::oauth2::github_callback),
        )
        // Google Auth Routes
        .route("/auth/google", get(crate::api::auth::oauth2::google_login))
        .route(
            "/auth/google/callback",
            get(crate::api::auth::oauth2::google_callback),
        )
        .route(
            "/auth/verify",
            get(crate::api::auth::verification::verify_email),
        )
        .route(
            "/auth/verify/resend",
            post(crate::api::auth::verification::resend_verification),
        )
        .route(
            "/auth/request-password-reset",
            post(crate::api::auth::verification::request_password_reset),
        )
        .route(
            "/auth/confirm-password-reset",
            post(crate::api::auth::verification::confirm_password_reset),
        )
        .route(
            "/collections",
            post(collections::create_collection).get(collections::list_collections),
        )
        .route(
            "/collections/{id}",
            get(collections::get_collection)
                .patch(collections::update_collection)
                .put(collections::update_collection)
                .delete(collections::delete_collection),
        )
        .route(
            "/collections/{id}/records",
            post(records::create_record).get(records::list_records),
        )
        // Advanced Query Endpoint
        .route(
            "/collections/{id}/query",
            post(sql_query::query_records_handler),
        )
        .route(
            "/collections/{id}/records/{record_id}",
            get(records::get_record)
                .patch(records::update_record)
                .put(records::update_record)
                .delete(records::delete_record),
        )
        .route("/collections/{id}/search", get(ose::search_records))
        .route(
            "/collections/{id}/instant-search",
            get(ose::instant_search_handler),
        )
        .route(
            "/collections/{id}/search-vector-with-vector",
            post(vector_routes::query_vector_with_vector),
        )
        .route(
            "/collections/{id}/search-vector-with-text",
            post(vector_routes::query_vector_with_text),
        )
        .route(
            "/collections/{id}/search-image-vector-with-image",
            post(vector_routes::query_image_vector_search),
        )
        .route(
            "/collections/{id}/search-image-vector-with-text",
            post(vector_routes::query_text_image_vector_search),
        )
        .route(
            "/collections/{id}/get-vector/{record_id}",
            get(vector_routes::get_record_vector),
        )
        .route(
            "/collections/{id}/records/{record_id}/relations",
            post(records::create_relation).delete(records::delete_relation),
        )
        // --- [NEW] OpenGraph GET Endpoint ---
        .route("/storage/files/opengraph", get(storage::generate_opengraph))
        // ------------------------------------
        .route("/storage/upload", post(storage::upload_file))
        .route("/storage/file/{*filename}", get(storage::serve_file))
        .route("/storage/files", get(storage::list_files))
        .route(
            "/storage/files/{id}",
            get(storage::get_file).delete(storage::delete_file),
        )
        .route("/admin/storage/test", post(storage::test_s3_connection))
        .route("/admin/storage/migrate", post(storage::migrate_storage))
        .route(
            "/admin/settings",
            get(settings::get_settings)
                .patch(settings::update_settings)
                .put(settings::update_settings),
        )
        .route(
            "/admin/smtp/test",
            post(crate::api::auth::user_rud_ops::test_email_handler),
        )
        .route(
            "/admin/config",
            post(config_routes::set_config).get(config_routes::list_configs),
        )
        .route(
            "/admin/config/{key}",
            axum::routing::delete(config_routes::delete_config),
        )
        .route(
            "/admin/keys",
            get(api_keys::list_keys).post(api_keys::create_key),
        )
        .route(
            "/admin/keys/{id}",
            axum::routing::delete(api_keys::delete_key)
                .patch(api_keys::update_key)
                .put(api_keys::update_key),
        )
        .route(
            "/admin/system/reload",
            post(crate::api::system::reload_system),
        )
        .route("/admin/backup", post(backup_routes::trigger_backup_handler))
        .route("/admin/backups", get(backup_routes::list_backups_handler))
        .route(
            "/admin/backups/{filename}",
            get(backup_routes::download_backup_handler),
        )
        .route(
            "/admin/restore-file",
            post(backup_routes::restore_from_file_handler),
        )
        .route("/admin/restore", post(backup_routes::restore_handler))
        .route(
            "/admin/users",
            get(crate::api::auth::user_rud_ops::list_users_handler),
        )
        .route(
            "/admin/users/{id}",
            axum::routing::delete(crate::api::auth::user_rud_ops::delete_user_handler)
                .patch(crate::api::auth::user_rud_ops::update_user_handler)
                .put(crate::api::auth::user_rud_ops::update_user_handler),
        )
        .route("/admin/logs", get(crate::api::system::list_audit_logs))
        .route(
            "/admin/dashboard",
            get(crate::api::system::get_dashboard_stats_handler),
        )
        .route(
            "/admin/import-data",
            post(crate::api::migration::import::data::import_data_handler),
        )
        .route(
            "/admin/export-data/{id}",
            get(crate::api::migration::export::data::export_data_handler),
        )
        .route(
            "/admin/import-schema",
            post(crate::api::migration::import::schema::import_schema_handler),
        )
        .route(
            "/admin/export-schema",
            get(crate::api::migration::export::schema::export_schema_handler),
        )
        .route(
            "/admin/export-scripts",
            get(crate::api::migration::export::scripts::export_scripts_handler),
        )
        .route(
            "/admin/export-templates",
            get(crate::api::migration::export::templates::export_templates_handler),
        )
        .route(
            "/admin/export-ai-actions",
            get(crate::api::migration::export::ai_actions::export_ai_actions_handler),
        )
        .route(
            "/admin/import-scripts",
            post(crate::api::migration::import::scripts::import_scripts_handler),
        )
        .route(
            "/admin/import-templates",
            post(crate::api::migration::import::templates::import_templates_handler),
        )
        .route(
            "/admin/import-ai-actions",
            post(crate::api::migration::import::ai_actions::import_ai_actions_handler),
        )
        .route(
            "/admin/collections/{id}/reindex",
            post(collections::reindex_collection_handler),
        )
        .route(
            "/admin/collections/{id}/revectorize",
            post(vector_routes::revectorize_collection_handler),
        )
        .route(
            "/admin/ai/actions",
            get(ai_routes::list_actions).post(ai_routes::create_action),
        )
        .route(
            "/admin/ai/actions/{id}",
            axum::routing::delete(ai_routes::delete_action),
        )
        .route("/ai/run/{slug}", post(ai_routes::run_action))
        .route("/admin/ai/edit-code", post(ai_routes::edit_code))
        // Admin Sandbox Management (Parent Context)
        .route(
            "/admin/sandboxes",
            get(sandbox_routes::list_sandboxes_handler)
                .post(sandbox_routes::create_sandbox_handler),
        )
        .route(
            "/admin/sandboxes/{id}",
            axum::routing::delete(sandbox_routes::delete_sandbox_handler),
        )
        .route(
            "/admin/sandboxes/{id}/publish",
            post(sandbox_routes::publish_sandbox_handler),
        )
        // Scoped AI Chat Actions (Child Context)
        .route("/admin/ai/session", get(ai_architect::get_session))
        .route("/admin/ai/chat", post(ai_architect::chat_handler_api))
        .route("/admin/ai/apply", post(ai_architect::apply_changes))
        .route("/admin/ai/plugins", get(ai_architect::list_plugins))
        .route(
            "/admin/scripts",
            get(script_routes::list_scripts).post(script_routes::create_script),
        )
        .route(
            "/admin/scripts/{id}",
            axum::routing::delete(script_routes::delete_script),
        )
        // LEGACY COMPATIBILITY
        .route(
            "/run/{script_name}",
            axum::routing::any(script_routes::run_script),
        )
        // NEW script endpoint
        .route(
            "/webhook/{script_name}",
            axum::routing::any(script_routes::run_script),
        )
        .route(
            "/admin/templates",
            get(template_routes::list_templates).post(template_routes::create_template),
        )
        .route(
            "/admin/templates/{id}",
            axum::routing::patch(template_routes::update_template)
                .put(template_routes::update_template)
                .delete(template_routes::delete_template),
        )
        .route("/admin/site/deploy", post(spa::deploy_site_handler))
        .route(
            "/admin/site/files",
            get(spa::list_site_files_handler).delete(spa::delete_site_file_handler),
        )
        .route("/sse", get(sse::sse_handler))
        .layer(DefaultBodyLimit::max(upload_limit_mb * 1024 * 1024))
}

pub fn app_router(state: AppState) -> Router {
    let core_api = make_api_router().layer(middleware::from_fn_with_state(
        state.clone(),
        auth_middleware,
    ));
    let renderer_routes = Router::new().route(
        "/render/{*slug}",
        get(ssr::render_view).post(ssr::render_view),
    );
    let scalar_router: Router<AppState> =
        Scalar::with_url("/openapi.json", ApiDoc::openapi()).into();

    // [NEW] Isolated Replication Router protected STRICTLY by Master Key Middleware
    let replication_api = Router::new()
        .route("/write", post(replication::fallback_write_handler))
        .route("/snapshot", get(replication::fallback_snapshot_handler))
        .route("/sync-file", post(replication::fallback_sync_file_handler))
        .route(
            "/ws",
            axum::routing::get(replication::ws_replication_handler),
        )
        .layer(middleware::from_fn(replication::master_auth_middleware))
        .with_state(state.clone());

    let sandbox_router = Router::new()
        .nest("/api/v1", core_api.clone())
        .route("/styles.css", get(serve_css::serve_styles))
        // Explicit route for sandbox renderer (2 params)
        .route(
            "/render/{*slug}",
            get(ssr::render_sandbox_view).post(ssr::render_sandbox_view),
        )
        .route(
            "/graphql",
            post(sandbox_graphql_handler).get(sandbox_graphql_playground),
        )
        .route("/openapi.json", get(sandbox_scalar_html))
        .route("/scalar-openapi.json", get(sandbox_openapi_json))
        .route("/ws", get(websocket::websocket_handler))
        .route("/logo", get(storage::serve_app_logo))
        .route("/app-name", get(settings::get_public_app_name))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            sandbox_lifecycle_middleware,
        ));

    let tenant_path_router = Router::new()
        .nest("/api/v1", core_api.clone())
        .route("/styles.css", get(serve_css::serve_styles))
        // Explicit route for tenant renderer (2 params)
        .route(
            "/render/{*slug}",
            get(ssr::render_tenant_view).post(ssr::render_tenant_view),
        )
        .route(
            "/graphql",
            post(tenant_graphql_handler).get(tenant_graphql_playground),
        )
        .route("/openapi.json", get(tenant_scalar_html))
        .route("/scalar-openapi.json", get(tenant_openapi_json))
        .route("/ws", get(websocket::websocket_handler))
        .route("/logo", get(storage::serve_app_logo))
        .route("/app-name", get(settings::get_public_app_name))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            tenant_resolver_middleware,
        ));

    let root_and_subdomain_router = Router::new()
        .nest("/api/v1", core_api.clone())
        .merge(renderer_routes)
        .merge(scalar_router)
        .route("/graphql", post(graphql_handler).get(graphql_playground))
        .route("/ws", get(websocket::websocket_handler))
        // Mount VS Code Live Sync Route
        .route(
            "/dev/sync",
            get(crate::api::system::vscode_sync::vscode_sync_handler),
        )
        .route("/logo", get(storage::serve_app_logo))
        .route("/app-name", get(settings::get_public_app_name))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            tenant_resolver_middleware,
        ));

    // Root handler (for /)
    let root_index_route = Router::new().route("/", get(assets::index_handler));
    Router::new()
        .merge(root_and_subdomain_router)
        .nest("/sandbox/{session_id}", sandbox_router)
        .nest("/tenant/{tenant_id}", tenant_path_router)
        .nest("/replication", replication_api) // <--- MOUNTED ISOLATED AT THE ROOT
        .route(
            "/api/v1/admin/tenants",
            get(tenant_routes::list_tenants_handler)
                .post(tenant_routes::create_tenant_handler)
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
        )
        .route(
            "/api/v1/admin/tenants/{id}",
            axum::routing::delete(tenant_routes::delete_tenant_handler)
                .patch(tenant_routes::update_tenant_details)
                .put(tenant_routes::update_tenant_details)
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
        )
        .route(
            "/api/v1/admin/tenants/{id}/status",
            axum::routing::patch(tenant_routes::update_tenant_status).layer(
                middleware::from_fn_with_state(state.clone(), auth_middleware),
            ),
        )
        .route("/metrics", get(crate::api::system::metrics_handler))
        .route("/healthz", get(crate::api::system::health_check))
        .route("/version", get(crate::api::system::get_versions_handler))
        .route("/styles.css", get(serve_css::serve_styles))
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
        .with_state(state)
}

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::auth::password_auth::login,crate::api::auth::password_auth::register,
        crate::api::auth::verification::request_password_reset,crate::api::auth::verification::confirm_password_reset,
        collections::list_collections,collections::create_collection,collections::get_collection,collections::update_collection,collections::delete_collection,
        records::list_records,records::create_record,records::get_record,records::update_record,records::delete_record,
        ose::search_records,ose::instant_search_handler,
        sql_query::query_records_handler,
        storage::generate_opengraph,
        storage::upload_file, storage::serve_file, storage::list_files, storage::get_file, storage::delete_file,
        records::create_relation,records::delete_relation,
        config_routes::set_config,
        settings::get_settings,settings::update_settings,
        crate::api::auth::user_rud_ops::list_users_handler,crate::api::auth::user_rud_ops::delete_user_handler,
        crate::api::auth::user_rud_ops::update_me, crate::api::auth::user_rud_ops::get_me,
        crate::api::system::list_audit_logs,
        crate::api::system::reload_system,
        ai_routes::list_actions,ai_routes::create_action,ai_routes::delete_action,ai_routes::run_action,ai_routes::edit_code,
        script_routes::list_scripts,script_routes::create_script,script_routes::delete_script,script_routes::run_script,
        template_routes::list_templates,template_routes::create_template,template_routes::update_template,template_routes::delete_template,
        crate::api::migration::import::data::import_data_handler,
        crate::api::migration::export::data::export_data_handler,
        crate::api::migration::import::schema::import_schema_handler,
        crate::api::migration::export::schema::export_schema_handler,
        collections::reindex_collection_handler,
        vector_routes::revectorize_collection_handler,
        vector_routes::query_vector_with_vector,
        vector_routes::query_vector_with_text,
        vector_routes::get_record_vector,
        vector_routes::query_image_vector_search,
        vector_routes::query_text_image_vector_search,
        serve_css::serve_styles,
        tenant_routes::create_tenant_handler,
        sse::sse_handler,
        sandbox_routes::list_sandboxes_handler,
        sandbox_routes::create_sandbox_handler,
        sandbox_routes::delete_sandbox_handler,
        sandbox_routes::publish_sandbox_handler,
        ai_architect::get_session,
        ai_architect::chat_handler_api,
        ai_architect::apply_changes,
        ai_architect::list_plugins
    ),
    components(schemas(
        CollectionResponse,AuthRequest,AuthResponse,RecordResponse,ProblemDetail,UserDto,
        CreateCollectionReq,UpdateCollection,RelationRequest,SearchQuery,RecordListResponse,
        sql_query::AdvancedQueryRequest,
        config_routes::SetConfigRequest,
        storage::OpenGraphQuery,
        storage::FileResponse,storage::FileUploadRequest,storage::FileListResponse,storage::FileListQuery,
        crate::system::dto::AppSettingsDto,crate::system::dto::SmtpConfigDto,crate::system::dto::StorageConfigDto,crate::system::dto::S3ConfigDto,crate::system::dto::SecurityConfigDto,crate::system::dto::AiConfigDto,
        apexkit_core::models::Record,
        apexkit_core::models::StoredFile,
        apexkit_core::models::CronJob,
        apexkit_core::models::InstantResult,
        apexkit_core::models::ai::AiAction,apexkit_core::models::ai::CreateActionReq,
        apexkit_core::models::schema::CollectionSchema,
        apexkit_core::models::schema::CollectionPolicies,
        apexkit_core::models::schema::FieldDefinition,
        apexkit_core::models::schema::FieldType,
        ai_routes::ExecutePromptReq,ai_routes::CodeEditReq,
        apexkit_core::models::script::Script,
        apexkit_core::models::script::CreateScriptReq,
        apexkit_core::models::Template,
        apexkit_core::models::CreateTemplateReq,
        template_routes::UpdateTemplateReq,
        apexkit_core::models::ai::AiSession,
        apexkit_core::models::ai::ChatMessage,
        apexkit_core::models::ai::Plugin,
        apexkit_core::models::ai::CreateSessionReq,
        apexkit_core::models::ai::ChatReq,
        apexkit_core::models::DashboardStats,
        apexkit_core::models::ChartPoint,
        crate::api::migration::import::ImportDataRequestDto,
        crate::api::migration::import::ImportDataResponseDto,
        crate::api::migration::import::ImportSchemaRequestDto,
        crate::api::migration::import::ImportSchemaResponseDto,
        crate::api::migration::export::ExportQuery,
        crate::api::auth::verification::RequestPasswordResetReq,crate::api::auth::verification::ConfirmPasswordResetReq,
        crate::api::auth::user_rud_ops::UpdateMeReq,
        vector_routes::VectorSearchReq,
        vector_routes::ImageVectorSearchReq,
        vector_routes::TextImageVectorSearchReq,
        vector_routes::RecordVectorPath,
        vector_routes::TextVectorSearchReq,
        tenant_routes::TenantResponse,tenant_routes::CreateTenantReq,
    )),
    tags((name = "ApexKit",description = "ApexKit API"))
)]
pub struct ApiDoc;
