use crate::{AppError, AppState, AuthRequest, AuthResponse, ProblemDetail, UserDto};
use crate::{BaseUrl, DatabaseConnection};
use crate::{hooks::trigger_void_hook, utils::extract_log_meta};
use apexkit_core::auth::Claims;
use apexkit_core::realtime::EventScope;
use apexkit_core::{auth, workers::Job};
use axum::Extension;
use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
};
use serde_json::json;
use std::net::SocketAddr;

// =========================================================
// 4. USERS / AUTH
// =========================================================

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = AuthRequest,
    responses((status = 200, body = AuthResponse), (status = 401, body = ProblemDetail))
)]
pub async fn login(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>, // [ADDED] Extract global AppState for scripting engine
    BaseUrl(base_url): BaseUrl,    // [ADDED] Extract dynamic base URL for script context
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    scope: Option<Extension<EventScope>>,
    headers: HeaderMap,
    Json(p): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let event_scope = scope.clone().map(|e| e.0).unwrap_or(EventScope::Root);
    let scope_str = match &event_scope {
        EventScope::Tenant(id) => format!("tenant:{}", id),
        EventScope::Sandbox(id) => format!("sandbox:{}", id),
        _ => "root".to_string(),
    };

    // [TRIGGER] before_user_login (Allows custom rate-limiting or domain/IP blacklisting)
    let input_data = json!({ "email": p.email, "ip": addr.ip().to_string() });
    trigger_void_hook(
        &state,
        "before_user_login",
        input_data,
        None,
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    let base_meta = json!({ "email": p.email });

    let user_opt = db
        .get_user_by_email(&p.email)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let u = match user_opt {
        Some(u) => u,
        None => {
            let meta = extract_log_meta(&headers, Some(addr), base_meta);
            let _ = db
                .log_audit_event(
                    "warning",
                    "Login Failed (User Not Found)",
                    "auth",
                    Some(meta),
                )
                .await;
            return Err(AppError::Unauthorized("Bad creds".into()));
        }
    };

    if !auth::verify_password(&p.password, &u.password_hash) {
        let meta = extract_log_meta(&headers, Some(addr), base_meta);
        let _ = db
            .log_audit_event("warning", "Login Failed (Bad Password)", "auth", Some(meta))
            .await;
        return Err(AppError::Unauthorized("Bad creds".into()));
    }

    // [FIX] Pass scope to JWT
    let token = auth::create_jwt(u.id, &u.email, &u.role, &scope_str)
        .map_err(|_| AppError::UnknownError("JWT fail".into()))?;

    // [LOG] Success
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "email": u.email, "user_id": u.id }),
    );
    let _ = db
        .log_audit_event("info", "Login Success", "auth", Some(meta))
        .await;

    // [TRIGGER] after_user_login (Asynchronous logging, activity tracking, or metric updates)
    let user_json = json!({
        "id": u.id,
        "email": u.email,
        "role": u.role,
        "scope": scope_str
    });
    let _ = trigger_void_hook(
        &state,
        "after_user_login",
        user_json,
        None,
        Some(&event_scope.clone()),
        Some(base_url),
    )
    .await;

    Ok(Json(AuthResponse {
        token,
        user: UserDto {
            id: u.id,
            email: u.email,
            role: u.role,
            metadata: u.metadata,
            scope: Some(scope_str),
        },
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    request_body = AuthRequest,
    responses((status = 200, body = AuthResponse))
)]
pub async fn register(
    BaseUrl(_base_url): BaseUrl,
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(p): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // 1. Check if requester is Admin
    let is_admin = matches!(auth, Some(Extension(ref c)) if c.role == "admin");

    // 2. Check Public Registration Setting (Only if NOT admin)
    if !is_admin {
        let general_settings = db
            .get_config("general")
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let allow_registration = general_settings
            .and_then(|v| v.get("allow_public_registration").and_then(|b| b.as_bool()))
            .unwrap_or(true); // Default true

        if !allow_registration {
            return Err(AppError::Forbidden(
                "Public registration is disabled".into(),
            ));
        }
    }

    let event_scope = scope.clone().map(|e| e.0).unwrap_or(EventScope::Root);
    let scope_str = match &event_scope {
        EventScope::Tenant(id) => format!("tenant:{}", id),
        EventScope::Sandbox(id) => format!("sandbox:{}", id),
        _ => "root".to_string(),
    };

    // --- NEW: STRICT ROLE VALIDATION ---
    let mut final_role = "user".to_string();

    if let Some(requested_role) = p.role {
        let req_role = requested_role.trim().to_lowercase();

        if is_admin {
            // Admins can assign any role they want, no restrictions
            final_role = req_role;
        } else {
            // Public users cannot request system/hidden roles prefixed with _
            if !req_role.starts_with('_') {
                // Check if the requested role exists in the allowed APEX_AUTH_ROLES
                let allowed_roles: Vec<String> =
                    if let Ok(Some(val)) = db.get_config("APEX_AUTH_ROLES").await {
                        if let Some(s) = val.as_str() {
                            serde_json::from_str(s)
                                .unwrap_or_else(|_| vec!["admin".to_string(), "user".to_string()])
                        } else if val.is_array() {
                            serde_json::from_value(val)
                                .unwrap_or_else(|_| vec!["admin".to_string(), "user".to_string()])
                        } else {
                            vec!["admin".to_string(), "user".to_string()]
                        }
                    } else {
                        vec!["admin".to_string(), "user".to_string()]
                    };

                // Prevent public users from registering as admin
                if allowed_roles.contains(&req_role) && req_role != "admin" {
                    final_role = req_role;
                }
            }
        }
    }
    // -----------------------------------

    // [TRIGGER] before_user_create
    let input_data = json!({ "email": p.email, "role": final_role, "metadata": p.metadata });
    trigger_void_hook(
        &state,
        "before_user_create",
        input_data.clone(),
        None,
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    let hash =
        auth::hash_password(&p.password).map_err(|_| AppError::UnknownError("Hash fail".into()))?;

    // Pass final_role and metadata
    let u = db
        .create_user(&p.email, &hash, &final_role, p.metadata)
        .await
        .map_err(|_| AppError::UnknownError("User exists".into()))?;

    // [LOG]
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "email": u.email, "user_id": u.id, "role": u.role }),
    );
    let _ = db
        .log_audit_event("info", "Register", "auth", Some(meta))
        .await;

    // [TRIGGER] after_user_create
    let user_json = json!({ "id": u.id, "email": u.email, "role": u.role });
    let _ = trigger_void_hook(
        &state,
        "after_user_create",
        user_json,
        None,
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await;

    // Pass scope to JWT
    let token = auth::create_jwt(u.id, &u.email, &u.role, &scope_str)
        .map_err(|_| AppError::UnknownError("JWT fail".into()))?;

    let tenant_id = crate::utils::get_tenant_id_from_scope(scope.as_ref().map(|e| &e.0));
    state
        .queue
        .enqueue(Job::SendWelcomeEmail {
            tenant_id,
            email: u.email.clone(),
            user_id: u.id,
        })
        .await;

    Ok(Json(AuthResponse {
        token,
        user: UserDto {
            id: u.id,
            email: u.email,
            role: u.role,
            metadata: u.metadata,
            scope: Some(scope_str),
        },
    }))
}
