use crate::ApiDoc;
use crate::AppError;
use crate::AppState;
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

// Helper function to resolve DB from scope (async)
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

// Helpers for helpers
pub fn get_current_model() -> String {
    let model = std::env::var("APEX_VECTOR_MODEL").unwrap_or("all-minilm-l6-v2".to_string());
    if model == "custom" {
        std::env::var("APEX_VECTOR_CUSTOM_REPO").unwrap_or("custom".to_string())
    } else {
        model
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
        r#"<!DOCTYPE html><html><head><title>ApexKit API (Tenant)</title><meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" /><style>body {{ margin: 0; }}</style></head><body><script id="api-reference" data-url="/tenant/{}/scalar/openapi.json"></script><script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script></body></html>"#,
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
        r#"<!DOCTYPE html><html><head><title>ApexKit API (Sandbox)</title><meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" /><style>body {{ margin: 0; }}</style></head><body><script id="api-reference" data-url="/sandbox/{}/scalar/openapi.json"></script><script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script></body></html>"#,
        session_id
    );
    axum::response::Html(html)
}
