use crate::AppState;
use apexkit_core::realtime::EventScope;
use apexkit_core::{
    Db,
    auth::{self, Claims},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use axum_extra::headers::{Authorization, HeaderMapExt, authorization::Bearer};
use std::sync::Arc;

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let current_request_scope = req
        .extensions()
        .get::<EventScope>()
        .cloned()
        .unwrap_or(EventScope::Root);

    let mut tenant_is_suspended = false;

    if let EventScope::Tenant(ref tenant_id) = current_request_scope {
        if let Ok(ctx) = state.tenant_manager.get_tenant_context(tenant_id).await {
            if ctx.status == "suspended" || ctx.status == "archived" {
                tenant_is_suspended = true;
            }
        } else {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    let db_to_check: Arc<dyn Db> = if let Some(db) = req.extensions().get::<Arc<dyn Db>>() {
        db.clone()
    } else {
        state.db.clone()
    };

    let mut is_root_admin = false;

    // 1. Check Standard JWT Bearer
    if let Some(auth_header) = req.headers().typed_get::<Authorization<Bearer>>()
        && let Ok(claims) = auth::decode_jwt(auth_header.token())
    {
        let is_authorized = match claims.scope.as_str() {
            "root" => true,
            scope_str => match &current_request_scope {
                EventScope::Root => scope_str == "root",
                EventScope::Tenant(id) => scope_str == format!("tenant:{}", id),
                EventScope::Sandbox(sandbox_id) => {
                    if scope_str == format!("sandbox:{}", sandbox_id) {
                        true
                    } else if scope_str.starts_with("tenant:") {
                        let user_tenant_id = scope_str.strip_prefix("tenant:").unwrap();
                        if let Ok(sandboxes) = state
                            .db
                            .list_sandboxes(Some(user_tenant_id.to_string()))
                            .await
                        {
                            sandboxes.iter().any(|s| &s.id == sandbox_id)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                _ => false,
            },
        };

        let is_root_user = claims.scope == "root";
        if is_root_user && claims.role == "admin" {
            is_root_admin = true;
        }

        if !is_authorized {
            return Err(StatusCode::FORBIDDEN);
        }

        if tenant_is_suspended && !is_root_admin {
            return Err(StatusCode::FORBIDDEN);
        }

        req.extensions_mut().insert(claims);
        return Ok(next.run(req).await);
    }

    // 2. [FIXED] API Key Integration
    if let Some(key_header) = req.headers().get("x-api-key")
        && let Ok(key) = key_header.to_str()
    {
        // Fast-Fail parsing
        if let Some(parsed) = apexkit_core::security::api_keys::parse_and_validate_key(key) {
            // Verify against Local DB OR fallback to Root DB
            let local_verification = db_to_check
                .verify_api_key(&parsed.tenant_id, &parsed.key_id, &parsed.secret)
                .await;

            let api_key_opt = if let Ok(Some(k)) = local_verification {
                Some((k, true))
            } else if !matches!(current_request_scope, EventScope::Root) {
                state
                    .db
                    .verify_api_key(&parsed.tenant_id, &parsed.key_id, &parsed.secret)
                    .await
                    .ok()
                    .flatten()
                    .map(|k| (k, false))
            } else {
                None
            };

            if let Some((api_key, is_local_key)) = api_key_opt {
                let is_root_key = api_key.env_type == "sys";
                if is_root_key && api_key.roles.contains(&"admin".to_string()) {
                    is_root_admin = true;
                }

                if tenant_is_suspended && !is_root_admin {
                    return Err(StatusCode::FORBIDDEN);
                }

                let scope = if api_key.env_type == "sys" {
                    "root".to_string()
                } else {
                    format!("tenant:{}", api_key.tenant_id)
                };

                let is_allowed = if is_local_key {
                    true
                } else {
                    match &current_request_scope {
                        EventScope::Root => scope == "root",
                        EventScope::Tenant(tid) => scope == format!("tenant:{}", tid),
                        EventScope::Sandbox(sid) => scope == format!("sandbox:{}", sid),
                        _ => false,
                    }
                };

                if !is_allowed {
                    return Err(StatusCode::FORBIDDEN);
                }

                let role = api_key.roles.first().cloned().unwrap_or("user".to_string());

                let claims = Claims {
                    sub: format!("apikey:{}", api_key.id),
                    uid: 0,
                    role,
                    exp: 9999999999,
                    scope,
                };
                req.extensions_mut().insert(claims);
                return Ok(next.run(req).await);
            }
        }
    }

    if tenant_is_suspended && !is_root_admin {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(req).await)
}
