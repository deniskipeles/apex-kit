use crate::DatabaseConnection;
use crate::{AppError, AppState, BaseUrl};
use crate::{extract_log_meta, trigger_void_hook};
use apexkit_core::{auth::Claims, models::Tenant};
use axum::{
    Extension,
    extract::{ConnectInfo, Json, Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use utoipa::ToSchema;

// --- DTOs ---

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateTenantReq {
    #[schema(example = "customer-1")]
    pub tenant_id: String,
    pub name: Option<String>,
    pub tier: Option<String>,
    pub owner_id: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct TenantResponse {
    pub tenant_id: String,
    pub status: String,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateTenantStatusReq {
    #[schema(example = "suspended")]
    pub status: String, // "active", "suspended", "archived"
}

// --- HANDLERS ---

#[utoipa::path(
    get,
    path = "/api/v1/admin/tenants",
    responses((status = 200, body = Vec<Tenant>)) // [UPDATED] Return Type
)]
pub async fn list_tenants_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Vec<Tenant>>, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Only admins".into()));
    }

    trigger_void_hook(
        &state,
        "before_list_tenants",
        json!({}),
        Some(&claims),
        None,
        Some(base_url.clone()),
    )
    .await?;

    // [UPDATED] Returns objects now
    let tenants = db
        .list_tenants()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let meta = extract_log_meta(&headers, Some(addr), json!({ "count": tenants.len() }));
    let _ = state
        .db
        .log_audit_event("info", "Tenants Listed", "admin", Some(meta))
        .await;

    Ok(Json(tenants))
}

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
    BaseUrl(base_url): BaseUrl,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<CreateTenantReq>,
) -> Result<(StatusCode, Json<TenantResponse>), AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Only admins can create tenants".into()));
    }

    // [TRIGGER] Before Create
    trigger_void_hook(
        &state,
        "before_tenant_create",
        json!({ "tenant_id": payload.tenant_id, "meta": payload }),
        Some(&claims),
        None,
        Some(base_url.clone()),
    )
    .await?;

    // 1. [CRITICAL] Register in Management Table (Root DB) with injected data
    // We pass the new optional fields to the register_tenant method
    state
        .db
        .register_tenant(
            &payload.tenant_id,
            payload.owner_id,
            payload.name.clone(),
            payload.tier.clone(),
        )
        .await
        .map_err(|e| {
            AppError::UnknownError(format!("Failed to register tenant metadata: {}", e))
        })?;

    // 2. Create Resources on Disk (Provisioning)
    state
        .tenant_manager
        .create_tenant(payload.tenant_id.clone())
        .await
        .map_err(|e| AppError::UnknownError(e))?;

    // [LOG]
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "tenant_id": payload.tenant_id, "admin": claims.sub, "tier": payload.tier }),
    );
    let _ = state
        .db
        .log_audit_event("info", "Tenant Created", "admin", Some(meta))
        .await;

    // [TRIGGER] After Create
    let _ = trigger_void_hook(
        &state,
        "after_tenant_create",
        json!({ "tenant_id": payload.tenant_id }),
        Some(&claims),
        None,
        Some(base_url.clone()),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(TenantResponse {
            tenant_id: payload.tenant_id,
            status: "created".to_string(),
        }),
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/tenants/{id}/status",
    request_body = UpdateTenantStatusReq,
    responses((status = 200, description = "Status updated"))
)]
pub async fn update_tenant_status(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>, // Need State to access Manager & Root DB
    // We don't use DatabaseConnection here because we specifically need the ROOT DB
    // regardless of what the URL context implies (though this route should be called on root).
    Path(id): Path<String>,
    Json(payload): Json<UpdateTenantStatusReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // 1. Update Root DB
    state
        .db
        .update_tenant_status(&id, &payload.status)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 2. [CRITICAL] Invalidate Cache to apply suspension immediately
    state.tenant_manager.invalidate(&id).await;

    // [LOG]
    let _ = state
        .db
        .log_system_event(
            "warning",
            "Tenant Status Change",
            &format!("Tenant {} set to {}", id, payload.status),
        )
        .await;

    Ok(Json(
        json!({ "success": true, "new_status": payload.status }),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateTenantReq {
    pub name: Option<String>,
    pub tier: Option<String>,
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/tenants/{id}",
    responses((status = 204, description = "Tenant deleted"))
)]
pub async fn delete_tenant_handler(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // 1. Check Root DB Metadata
    state
        .db
        .delete_tenant_metadata(&id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 2. Delete Physical Resources via Manager
    state
        .tenant_manager
        .delete_tenant(&id)
        .await
        .map_err(|e| AppError::UnknownError(e))?;

    // Log
    let _ = state
        .db
        .log_system_event(
            "warning",
            "Tenant Deleted",
            &format!("Deleted tenant {}", id),
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/tenants/{id}",
    request_body = UpdateTenantReq,
    responses((status = 200, body = Value))
)]
pub async fn update_tenant_details(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateTenantReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !matches!(auth.map(|c| c.0.role), Some(r) if r == "admin") {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    state
        .db
        .update_tenant_full(&id, payload.name, None, payload.tier)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(json!({ "success": true })))
}
