use crate::ApiDoc;
use crate::AppError;
use crate::AppState;
use apexkit_core::VectorProvider;
use apexkit_core::models::Collection;
use apexkit_core::realtime::EventScope;
use apexkit_core::{Db, storage::StorageBackend};
use axum::{
    Json,
    extract::{FromRef, Path},
    http::{HeaderMap, StatusCode, request::Parts},
    response::IntoResponse,
};
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa::openapi::Server;

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
pub fn extract_log_meta(
    headers: &HeaderMap,
    addr: Option<SocketAddr>,
    details: serde_json::Value,
) -> serde_json::Value {
    let mut meta = details;

    // 1. IP Resolution
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .or_else(|| addr.map(|a| a.ip().to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    // 2. Client Info
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    let referer = headers
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");

    if let Some(obj) = meta.as_object_mut() {
        obj.insert("ip".into(), json!(ip));
        obj.insert("user_agent".into(), json!(ua));
        obj.insert("referer".into(), json!(referer));
    }
    meta
}

// Helper to resolve DB from scope (async)
pub async fn resolve_db_from_scope(
    state: &AppState,
    scope: &EventScope,
) -> Result<Arc<dyn Db>, AppError> {
    match scope {
        EventScope::Root => Ok(state.db.clone()),
        EventScope::Tenant(id) => state
            .tenant_manager
            .get_tenant(id.clone())
            .await
            .map_err(AppError::UnknownError),
        EventScope::Sandbox(id) => state
            .sandbox_manager
            .get_sandbox(id)
            .await
            .map_err(AppError::UnknownError),
        _ => Ok(state.db.clone()),
    }
}

// Helper to resolve the scoped Vector Provider (async)
pub async fn resolve_vector_provider_from_scope(
    state: &AppState,
    scope: &EventScope,
) -> Result<Arc<dyn VectorProvider>, AppError> {
    match scope {
        EventScope::Root => Ok(state.vector_provider.clone()),
        EventScope::Tenant(id) => state
            .tenant_manager
            .get_tenant_context(id)
            .await
            .map(|ctx| ctx.vector_provider)
            .map_err(AppError::UnknownError),
        EventScope::Sandbox(id) => state
            .sandbox_manager
            .get_sandbox_context(id)
            .await
            .map(|ctx| ctx.vector_provider)
            .map_err(AppError::UnknownError),
        _ => Ok(state.vector_provider.clone()),
    }
}

// Helpers for helpers
pub fn get_current_model(content_type: &str) -> String {
    if content_type == "file" || content_type == "image" {
        apexkit_vector::get_current_vision_model()
    } else {
        apexkit_vector::get_current_text_model()
    }
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

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        // 1. Determine Protocol (Trust Proxy headers or default to http)
        let scheme = parts
            .headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("http");

        // 2. Get Host
        let host = parts
            .headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .ok_or((StatusCode::BAD_REQUEST, "Missing Host header".to_string()))?;

        // 3. Construct canonical URL
        Ok(BaseUrl(format!("{}://{}", scheme, host)))
    }
}

// Helper to resolve Collection from String (ID or Name)
pub async fn resolve_collection_by_id_or_name(
    db: &Arc<dyn Db>,
    identifier: &str,
) -> Result<Collection, AppError> {
    // 1. Try to parse as numeric ID first
    if let Ok(id_num) = identifier.parse::<i64>()
        && let Ok(Some(col)) = db.get_collection(id_num).await
    {
        return Ok(col);
    }

    // 2. Fallback: Look up by Name via list (Cached in CachedDb)
    let cols = db
        .list_collections()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    cols.into_iter()
        .find(|c| c.name == identifier)
        .ok_or_else(|| AppError::NotFound(format!("Collection '{}' not found", identifier)))
}

// --- SYSTEM SCOPE RELOADER ---
// Invalidate cache and regenerate GraphQL schemas globally across environments
pub async fn trigger_scope_reload(state: AppState, scope: EventScope) {
    match scope {
        EventScope::Root => {
            let relation_loader = async_graphql::dataloader::DataLoader::new(
                crate::graphql::RelationLoader::new(state.db.clone()),
                tokio::spawn,
            );
            if let Ok(new_schema) =
                crate::graphql::build_schema(state.clone(), std::sync::Arc::new(relation_loader))
                    .await
            {
                let mut lock = state.schema.write().await;
                *lock = new_schema;
                tracing::info!(
                    "[System] Root GraphQL schema automatically reloaded due to schema/config change."
                );
            }
        }
        EventScope::Tenant(id) => {
            state.tenant_manager.invalidate(&id).await;
            tracing::info!(
                "[System] Tenant '{}' cache invalidated due to schema/config change.",
                id
            );
        }
        EventScope::Sandbox(id) => {
            state.sandbox_manager.invalidate(&id).await;
            tracing::info!(
                "[System] Sandbox '{}' cache invalidated due to schema/config change.",
                id
            );
        }
        _ => {}
    }
}

// --- DYNAMIC DOCS HELPERS ---
pub async fn tenant_openapi_json(
    Path(params): Path<HashMap<String, String>>,
) -> Json<utoipa::openapi::OpenApi> {
    let tenant_id = params.get("tenant_id").map(|s| s.as_str()).unwrap_or("");
    let mut doc = ApiDoc::openapi();
    doc.servers = Some(vec![Server::new(format!("/tenant/{}", tenant_id))]);
    Json(doc)
}
pub async fn tenant_scalar_html(Path(params): Path<HashMap<String, String>>) -> impl IntoResponse {
    let tenant_id = params.get("tenant_id").map(|s| s.as_str()).unwrap_or("");
    let html = format!(
        r#"<!DOCTYPE html><html><head><title>ApexKit API (Tenant)</title><meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" /><style>body {{ margin: 0; }}</style></head><body><script id="api-reference" data-url="/tenant/{}/scalar-openapi.json"></script><script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script></body></html>"#,
        tenant_id
    );
    axum::response::Html(html)
}
pub async fn sandbox_openapi_json(
    Path(params): Path<HashMap<String, String>>,
) -> Json<utoipa::openapi::OpenApi> {
    let session_id = params.get("session_id").map(|s| s.as_str()).unwrap_or("");
    let mut doc = ApiDoc::openapi();
    doc.servers = Some(vec![Server::new(format!("/sandbox/{}", session_id))]);
    Json(doc)
}
pub async fn sandbox_scalar_html(Path(params): Path<HashMap<String, String>>) -> impl IntoResponse {
    let session_id = params.get("session_id").map(|s| s.as_str()).unwrap_or("");
    let html = format!(
        r#"<!DOCTYPE html><html><head><title>ApexKit API (Sandbox)</title><meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" /><style>body {{ margin: 0; }}</style></head><body><script id="api-reference" data-url="/sandbox/{}/scalar-openapi.json"></script><script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script></body></html>"#,
        session_id
    );
    axum::response::Html(html)
}

// Verify real-time storage bounds before file write commits
pub async fn check_storage_quota(state: &AppState, scope: &EventScope) -> Result<(), AppError> {
    match scope {
        EventScope::Root => Ok(()),
        EventScope::Tenant(id) => {
            let tenants = state
                .db
                .list_tenants()
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;
            if let Some(t) = tenants.iter().find(|t| &t.id == id) {
                if t.stats.storage_mb >= t.stats.max_storage_mb as f64 {
                    return Err(AppError::Forbidden(format!(
                        "Tenant storage quota exceeded ({} MB max)",
                        t.stats.max_storage_mb
                    )));
                }
            }
            Ok(())
        }
        EventScope::Sandbox(id) => {
            let sandboxes = state
                .db
                .list_sandboxes(None)
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;
            if let Some(sb) = sandboxes.iter().find(|s| &s.id == id) {
                if sb.current_storage_mb >= sb.max_storage_mb as f64 {
                    return Err(AppError::Forbidden(format!(
                        "Sandbox storage quota exceeded ({} MB max)",
                        sb.max_storage_mb
                    )));
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// Verify real-time AI quota (Aggregated persistent database + live active memory)
pub async fn check_ai_quota(state: &AppState, scope: &EventScope) -> Result<(), AppError> {
    match scope {
        EventScope::Root => Ok(()),
        EventScope::Tenant(id) => {
            let tenants = state
                .db
                .list_tenants()
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;
            if let Some(t) = tenants.iter().find(|t| &t.id == id) {
                let mut live_requests = 0;
                if let Ok(ctx) = state.tenant_manager.get_tenant_context(id).await {
                    live_requests = ctx.vector_provider.get_metrics() as i64;
                }
                if t.stats.ai_requests + live_requests >= t.stats.max_ai_requests {
                    return Err(AppError::Forbidden(format!(
                        "Tenant AI request limit exceeded ({} max per 30m)",
                        t.stats.max_ai_requests
                    )));
                }
            }
            Ok(())
        }
        EventScope::Sandbox(id) => {
            let sandboxes = state
                .db
                .list_sandboxes(None)
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;
            if let Some(sb) = sandboxes.iter().find(|s| &s.id == id) {
                let mut live_requests = 0;
                if let Ok(ctx) = state.sandbox_manager.get_sandbox_context(id).await {
                    live_requests = ctx.vector_provider.get_metrics() as i64;
                }
                if sb.current_ai_requests + live_requests >= sb.max_ai_requests {
                    return Err(AppError::Forbidden(format!(
                        "Sandbox AI request limit exceeded ({} max per 30m)",
                        sb.max_ai_requests
                    )));
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub fn get_temp_limit_multiplier() -> f64 {
    std::env::var("TMP_DIR_LIMIT_OF_SCOPE_LIMIT")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(1.0)
}

pub fn get_temp_path(subpath: &str) -> std::path::PathBuf {
    let base = std::env::var("APEXKIT_TMP_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("apexkit_tmp"));

    let clean_sub = subpath.trim_start_matches('/').trim_start_matches("./");
    base.join(clean_sub)
}

pub fn calculate_dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total_size = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                total_size += calculate_dir_size(&entry.path())?;
            } else {
                total_size += metadata.len();
            }
        }
    } else if path.exists() {
        total_size = path.metadata()?.len();
    }
    Ok(total_size)
}

// Verify real-time temp directory quota relative to the scope storage limit
pub async fn check_temp_quota(
    state: &AppState,
    scope: &EventScope,
    additional_bytes: u64,
) -> Result<(), AppError> {
    let multiplier = get_temp_limit_multiplier();

    let (max_storage_mb, temp_dir) = match scope {
        EventScope::Root => {
            // [UPDATED] Read ROOT_TMP_DIR_LIMIT_IN_MB from env, defaulting to 1GB (1024 MB)
            let max_mb = std::env::var("ROOT_TMP_DIR_LIMIT_IN_MB")
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(1024);
            (max_mb, get_temp_path("system/tmp"))
        }
        EventScope::Tenant(id) => {
            let tenants = state
                .db
                .list_tenants()
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;
            let max_mb = tenants
                .iter()
                .find(|t| &t.id == id)
                .map(|t| t.stats.max_storage_mb)
                .unwrap_or(500); // 500 MB default
            (max_mb, get_temp_path(&format!("tenants/{}/tmp", id)))
        }
        EventScope::Sandbox(id) => {
            let sandboxes = state
                .db
                .list_sandboxes(None)
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;
            let max_mb = sandboxes
                .iter()
                .find(|s| &s.id == id)
                .map(|s| s.max_storage_mb)
                .unwrap_or(100); // 100 MB default
            (
                max_mb,
                get_temp_path(&format!("sandboxes/session_{}/tmp", id)),
            )
        }
        _ => return Ok(()),
    };

    let max_allowed_bytes = ((max_storage_mb as f64) * multiplier * 1024.0 * 1024.0) as u64;

    let current_temp_bytes = tokio::task::spawn_blocking(move || {
        if temp_dir.exists() {
            calculate_dir_size(&temp_dir).unwrap_or(0)
        } else {
            0
        }
    })
    .await
    .unwrap_or(0);

    if current_temp_bytes + additional_bytes > max_allowed_bytes {
        return Err(AppError::Forbidden(format!(
            "Temp quota exceeded: current {:.2} MB + new {:.2} MB > allowed {:.2} MB (Scope Limit: {} MB × Multiplier: {})",
            (current_temp_bytes as f64) / (1024.0 * 1024.0),
            (additional_bytes as f64) / (1024.0 * 1024.0),
            (max_allowed_bytes as f64) / (1024.0 * 1024.0),
            max_storage_mb,
            multiplier
        )));
    }

    Ok(())
}
