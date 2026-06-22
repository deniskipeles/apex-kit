use crate::AppState;
use crate::BaseUrl;
use crate::hooks::trigger_void_hook;
use apexkit_core::realtime::EventScope;
use apexkit_core::storage::StorageBackend;
use axum::body::HttpBody;
use axum::{
    extract::{Path, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::sync::Arc;

pub async fn sandbox_lifecycle_middleware(
    Path(params): Path<HashMap<String, String>>,
    BaseUrl(base_url): BaseUrl, // Needed for hooks
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let session_id = params
        .get("session_id")
        .ok_or(StatusCode::NOT_FOUND)?
        .clone();

    let method = req.method().to_string();
    let path_req = req.uri().path().to_string();

    // 1. Capture Ingress
    let ingress = req
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    // Before Request Hook
    let hook_payload = serde_json::json!({
        "sandbox_id": session_id,
        "path": path_req,
        "method": method,
        "ip": req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()).unwrap_or("unknown"),
        "ingress": ingress,
        "egress": 0
    });

    if let Err(e) = trigger_void_hook(
        &state,
        "before_sandbox_request",
        hook_payload,
        None,
        Some(&EventScope::Root),
        Some(base_url.clone()),
    )
    .await
    {
        tracing::warn!("Blocked request to sandbox {}: {:?}", session_id, e);
        return Err(StatusCode::FORBIDDEN);
    }

    match state.sandbox_manager.get_sandbox(&session_id).await {
        Ok(sandbox_db) => {
            req.extensions_mut().insert(sandbox_db.clone());

            // [FIX] Use ScopedDynamicStorage to support S3 Reselling
            let scope = EventScope::Sandbox(session_id.clone());
            let storage: Arc<dyn StorageBackend> = Arc::new(
                crate::storage::ScopedDynamicStorage::new(state.clone(), scope.clone()),
            );

            req.extensions_mut().insert(storage);
            req.extensions_mut().insert(scope);

            let path_clone = req.uri().path().to_string();

            let mut response = next.run(req).await;

            if let Ok(val) = HeaderValue::from_str(&format!("sandbox:{}", session_id)) {
                response.headers_mut().insert("X-Apex-Scope", val);
            }

            // 3. CAPTURE EGRESS
            let egress = response
                .headers()
                .get(axum::http::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| response.body().size_hint().exact())
                .unwrap_or(0);

            // After Request Hook & Logging
            let status = response.status().as_u16();
            let state_clone = state.clone();
            let base_url_clone = base_url.clone();
            let sid_clone = session_id.to_string();
            let sandbox_db_clone = sandbox_db.clone();

            tokio::spawn(async move {
                // LOG API REQUEST
                if !path_clone.starts_with("/_dashboard/assets")
                    && !path_clone.starts_with("/styles.css")
                {
                    let level = if status >= 400 { "error" } else { "info" };
                    let _ = sandbox_db_clone
                        .log_system_event(
                            level,
                            "API",
                            &format!("{} {} - {}", method, path_clone, status),
                        )
                        .await;
                }

                let payload = serde_json::json!({
                    "sandbox_id": sid_clone,
                    "path": path_clone,
                    "status": status,
                    "ingress": ingress,
                    "egress": egress
                });
                let _ = trigger_void_hook(
                    &state_clone,
                    "after_sandbox_request",
                    payload,
                    None,
                    Some(&EventScope::Root),
                    Some(base_url_clone),
                )
                .await;
            });

            Ok(response)
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}
