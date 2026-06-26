use crate::AppState;
use crate::BaseUrl;
use crate::hooks::trigger_void_hook;
use apexkit_core::realtime::EventScope;
use apexkit_core::storage::StorageBackend;
use axum::body::HttpBody;
use axum::{
    Json,
    extract::{Path, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

pub async fn tenant_resolver_middleware(
    path_params: Option<Path<HashMap<String, String>>>,
    BaseUrl(base_url): BaseUrl,
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> std::result::Result<Response, StatusCode> {
    let method = req.method().to_string();
    let path_req = req.uri().path().to_string();

    let mut key_scope_override: Option<String> = None;

    // [FIXED] Use fast fail key parser for Smart Key Overrides
    if let Some(key_header) = req.headers().get("x-api-key")
        && let Ok(key) = key_header.to_str()
        && let Some(parsed) = apexkit_core::security::api_keys::parse_and_validate_key(key)
        && let Ok(Some(api_key)) = state
            .db
            .verify_api_key(&parsed.tenant_id, &parsed.key_id, &parsed.secret)
            .await
    {
        let scope = if api_key.env_type == "sys" {
            "root".to_string()
        } else {
            format!("tenant:{}", api_key.tenant_id)
        };
        key_scope_override = Some(scope);
    }

    let mut tenant_id = String::new();

    if let Some(scope) = key_scope_override
        && scope.starts_with("tenant:")
    {
        let target = scope.strip_prefix("tenant:").unwrap();
        if target != "*" {
            tenant_id = target.to_string();
        }
    }

    if tenant_id.is_empty() {
        let root_domain = std::env::var("APEX_ROOT_DOMAIN").unwrap_or_default();
        let host = req
            .headers()
            .get("host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("");

        if let Some(Path(params)) = &path_params
            && let Some(id) = params.get("tenant_id")
        {
            tenant_id = id.clone();
        }

        if tenant_id.is_empty() {
            let path = req.uri().path();
            if path.starts_with("/tenant/") {
                let parts: Vec<&str> = path.split('/').collect();
                if parts.len() >= 3 {
                    tenant_id = parts[2].to_string();
                }
            }
        }

        if tenant_id.is_empty() {
            if !root_domain.is_empty() && host == root_domain {
                tenant_id = String::new();
            } else {
                let parts: Vec<&str> = host.split('.').collect();
                if parts.len() >= 2 {
                    let sub = parts[0];
                    if !["localhost", "www", "api"].contains(&sub) {
                        tenant_id = sub.to_string();
                    }
                }
            }
        }
    }

    let ingress = req
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    if !tenant_id.is_empty() {
        let hook_payload = serde_json::json!({
            "tenant_id": tenant_id,
            "path": path_req,
            "method": method,
            "ip": req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()).unwrap_or("unknown"),
            "ingress": ingress,
            "egress": 0
        });

        if let Err(e) = trigger_void_hook(
            &state,
            "before_tenant_request",
            hook_payload.clone(),
            None,
            Some(&EventScope::Root),
            Some(base_url.clone()),
        )
        .await
        {
            tracing::warn!("Blocked request to tenant {}: {:?}", tenant_id, e);
            let msg = e.to_string();
            let body = Json(json!({ "error": "request_blocked", "message": msg, "status": 429 }));
            return Ok((StatusCode::TOO_MANY_REQUESTS, body).into_response());
        }
    }

    if tenant_id.is_empty() {
        req.extensions_mut().insert(EventScope::Root);
        let mut response = next.run(req).await;
        response
            .headers_mut()
            .insert("X-Apex-Scope", HeaderValue::from_static("root"));

        let status = response.status().as_u16();
        let db_clone = state.db.clone();

        tokio::spawn(async move {
            // LOG ROOT API REQUEST
            if !path_req.starts_with("/_dashboard/assets") && !path_req.starts_with("/styles.css") {
                let level = if status >= 400 { "error" } else { "info" };
                let meta =
                    serde_json::json!({ "method": method, "path": path_req, "status": status });
                let _ = db_clone
                    .log_audit_event(level, "API Request", "API-NON-CRUD-OPS", Some(meta))
                    .await;
            }
        });

        return Ok(response);
    }

    match state.tenant_manager.get_tenant(tenant_id.clone()).await {
        Ok(tenant_db) => {
            req.extensions_mut().insert(tenant_db.clone());

            let scope = EventScope::Tenant(tenant_id.clone());
            let storage: Arc<dyn StorageBackend> = Arc::new(
                crate::storage::ScopedDynamicStorage::new(state.clone(), scope.clone()),
            );

            req.extensions_mut().insert(storage);
            req.extensions_mut().insert(scope);

            let path_clone = req.uri().path().to_string();

            let mut response = next.run(req).await;

            if let Ok(val) = HeaderValue::from_str(&format!("tenant:{}", tenant_id)) {
                response.headers_mut().insert("X-Apex-Scope", val);
            }

            let egress = response
                .headers()
                .get(axum::http::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| response.body().size_hint().exact())
                .unwrap_or(0);

            let status = response.status().as_u16();
            let state_clone = state.clone();
            let base_url_clone = base_url.clone();
            let tid_clone = tenant_id.clone();
            let tenant_db_clone = tenant_db.clone();

            tokio::spawn(async move {
                // LOG TENANT API REQUEST
                if !path_clone.starts_with("/_dashboard/assets")
                    && !path_clone.starts_with("/styles.css")
                {
                    let level = if status >= 400 { "error" } else { "info" };
                    let meta = serde_json::json!({ "method": method, "path": path_clone, "status": status, "tenant_id": tid_clone });
                    let _ = tenant_db_clone
                        .log_audit_event(level, "API Request", "API-NON-CRUD-OPS", Some(meta))
                        .await;
                }

                let payload = serde_json::json!({
                    "tenant_id": tid_clone,
                    "path": path_clone,
                    "status": status,
                    "ingress": ingress,
                    "egress": egress
                });
                let _ = trigger_void_hook(
                    &state_clone,
                    "after_tenant_request",
                    payload,
                    None,
                    Some(&EventScope::Root),
                    Some(base_url_clone),
                )
                .await;
            });

            Ok(response)
        }
        Err(e) => {
            tracing::error!(
                "❌ [TenantResolver] Failed to load tenant '{}': {}",
                tenant_id,
                e
            );
            let body = Json(serde_json::json!({
                "error": "not_found",
                "message": "Tenant not found or inactive",
                "status": 404
            }));
            Ok((StatusCode::NOT_FOUND, body).into_response())
        }
    }
}
