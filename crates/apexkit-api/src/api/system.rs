use crate::AppError;
use crate::AppState;
use crate::DatabaseConnection;
use apexkit_core::auth::Claims;
use apexkit_core::models::DashboardData;
use apexkit_core::realtime::EventScope;
use axum::{
    Extension, Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::{IntoParams, ToSchema};

pub mod api_keys;
pub mod backup;
pub mod config;
pub mod settings;
pub mod vscode_sync;

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
        vector: apexkit_vector::VERSION.to_string(),
    })
}

pub async fn metrics_handler(State(state): State<AppState>) -> Response {
    match &state.metrics {
        Some(handle) => handle.render().into_response(),
        None => (StatusCode::NOT_IMPLEMENTED, "Metrics not initialized").into_response(),
    }
}

// =========================================================
// SYSTEM & OTHER HANDLERS
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
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;

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
            return Err(AppError::Forbidden(
                "Only Root Admins can target specific scopes".into(),
            ));
        }

        if target == "root" {
            EventScope::Root
        } else if let Some(id) = target.strip_prefix("tenant:") {
            EventScope::Tenant(id.to_string())
        } else if let Some(id) = target.strip_prefix("sandbox:") {
            EventScope::Sandbox(id.to_string())
        } else {
            return Err(AppError::InputValidation(validator::ValidationErrors::new()));
        } // Invalid format
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
                tokio::spawn,
            );

            let new_schema =
                crate::graphql::build_schema(state.clone(), std::sync::Arc::new(relation_loader))
                    .await
                    .map_err(|e| AppError::UnknownError(e.to_string()))?;

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
        }
        EventScope::Tenant(id) => {
            // RELOAD TENANT (Invalidate Cache)
            // This forces next request to reload DB/Schema/Cache
            info!("[System] Reloading Tenant {}", id);
            state.tenant_manager.invalidate(&id).await;
            Ok(Json(serde_json::json!({
                "status": "ok",
                "message": format!("Tenant {} cache invalidated. Will reload on next request.", id)
            })))
        }
        EventScope::Sandbox(id) => {
            // RELOAD SANDBOX
            info!("[System] Reloading Sandbox {}", id);
            state.sandbox_manager.invalidate(&id).await;

            Ok(Json(serde_json::json!({
                "status": "ok",
                "message": format!("Sandbox {} cache invalidated.", id)
            })))
        }
        _ => Ok(Json(
            serde_json::json!({ "status": "ignored", "message": "Scope not reloadable" }),
        )),
    }
}

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct LogsQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub level: Option<String>,
    pub source: Option<String>,
    pub search: Option<String>,
    pub r#type: Option<String>, // [NEW] "system" or "audit"
}

#[derive(Serialize, ToSchema)]
pub struct LogsResponse {
    pub items: Vec<serde_json::Value>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[utoipa::path(
        get,
        path = "/api/v1/admin/logs",
        params(LogsQuery),
        responses((status = 200, body = LogsResponse))
    )]
pub async fn list_audit_logs(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    Query(q): Query<LogsQuery>,
) -> Result<Json<LogsResponse>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(10, 200);
    let log_type = q.r#type.unwrap_or_else(|| "system".to_string());

    let (items, total) = db
        .list_paginated_logs(&log_type, page, per_page, q.level, q.source, q.search)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(LogsResponse {
        items,
        total,
        page,
        per_page,
    }))
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
    if !matches!(auth.map(|e| e.0.role), Some(r) if r == "admin") {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    let data = db
        .get_dashboard_stats()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(data))
}
