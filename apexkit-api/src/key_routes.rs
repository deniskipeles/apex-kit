use crate::{AppError, DatabaseConnection};
use apexkit_core::{auth::Claims, models::ApiKey, realtime::EventScope};
use axum::{
    Extension,
    extract::{Json, Path},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct CreateKeyReq {
    pub name: String,

    // --- Backward Compatibility Fields ---
    pub role: Option<String>,  // Legacy field (e.g. "admin", "user")
    pub scope: Option<String>, // Legacy field (e.g. "root", "tenant:id")

    // --- Scoped, Composite Fields ---
    pub target_tenant: Option<String>,
    #[serde(default = "default_env_type")]
    pub env_type: String, // 'sys', 'tnnt', 'sk', 'pk'
    #[serde(default = "default_roles")]
    pub roles: Vec<String>,
    #[serde(default)]
    pub bypass_cors: bool,
}

fn default_env_type() -> String {
    "sys".to_string()
}
fn default_roles() -> Vec<String> {
    vec!["admin".to_string()]
}

#[derive(Serialize, ToSchema)]
pub struct CreateKeyResponse {
    pub key: String,
    pub info: ApiKey,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateKeyReq {
    pub name: Option<String>,
    pub status: Option<String>,
    pub roles: Option<Vec<String>>,
    pub bypass_cors: Option<bool>,
}

#[derive(Deserialize)]
pub struct KeyPath {
    pub id: i64,
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/keys",
    responses((status = 200, body = Vec<ApiKey>))
)]
pub async fn list_keys(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<Vec<ApiKey>>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    let keys = db
        .list_api_keys()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(keys))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/keys",
    request_body = CreateKeyReq,
    responses((status = 201, body = CreateKeyResponse))
)]
pub async fn create_key(
    Extension(claims): Extension<Claims>,
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection,
    Json(payload): Json<CreateKeyReq>,
) -> Result<Json<CreateKeyResponse>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|Extension(s)| s).unwrap_or(EventScope::Root);

    // 1. Resolve Roles (Map legacy 'role' if 'roles' is empty)
    let mut final_roles = payload.roles.clone();
    if final_roles.is_empty() {
        if let Some(r) = &payload.role {
            final_roles.push(r.clone());
        } else {
            final_roles.push("admin".to_string());
        }
    }

    // 2. Resolve Scope (Map legacy 'scope' if 'target_tenant' or 'env_type' is default/empty)
    let mut resolved_env = payload.env_type.clone();
    let mut resolved_tenant = payload
        .target_tenant
        .clone()
        .unwrap_or_else(|| "root".to_string());

    if let Some(legacy_scope) = &payload.scope {
        if legacy_scope == "root" {
            resolved_env = "sys".to_string();
            resolved_tenant = "root".to_string();
        } else if let Some(tid) = legacy_scope.strip_prefix("tenant:") {
            resolved_env = "sk".to_string();
            resolved_tenant = tid.to_string();
        } else if legacy_scope == "*" {
            resolved_env = "sys".to_string();
            resolved_tenant = "root".to_string();
        }
    }

    // 3. Bind to Active Context Boundaries
    let (issuer, final_tenant, final_env) = match event_scope {
        EventScope::Root => {
            if resolved_env == "tnnt" || resolved_env == "sk" || resolved_env == "pk" {
                ("root".to_string(), resolved_tenant, resolved_env)
            } else if resolved_tenant != "root" {
                ("root".to_string(), resolved_tenant, "tnnt".to_string())
            } else {
                ("root".to_string(), "root".to_string(), "sys".to_string())
            }
        }
        EventScope::Tenant(id) => {
            let env = if resolved_env == "pk" {
                "pk".to_string()
            } else {
                "sk".to_string()
            };
            ("tnt".to_string(), id, env)
        }
        _ => return Err(AppError::Forbidden("Scope not supported for keys".into())),
    };

    let (key, info) = db
        .create_api_key(
            &payload.name,
            &final_tenant,
            &issuer,
            &final_env,
            final_roles,
            payload.bypass_cors,
        )
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(CreateKeyResponse { key, info }))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/keys/{id}",
    request_body = UpdateKeyReq,
    responses((status = 200, body = Value))
)]
pub async fn update_key(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    Path(params): Path<KeyPath>,
    Json(payload): Json<UpdateKeyReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    db.update_api_key(
        params.id,
        payload.name,
        payload.status,
        payload.roles,
        payload.bypass_cors,
    )
    .await
    .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(json!({ "success": true })))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/keys/{id}",
    responses((status = 204, description = "Key deleted"))
)]
pub async fn delete_key(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    Path(params): Path<KeyPath>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }
    db.delete_api_key(params.id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}
