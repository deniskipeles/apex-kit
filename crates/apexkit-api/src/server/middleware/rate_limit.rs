use crate::AppState;
use apexkit_core::realtime::EventScope;
use axum::{
    Json,
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{
    Quota, RateLimiter,
    clock::{Clock, DefaultClock},
};
use serde_json::json;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

/// Helper to detect if the request should bypass rate limiting.
/// Covers static assets, direct Admin UI routes, and API calls originating from the Admin UI.
fn should_bypass_rate_limiting(path: &str, headers: &axum::http::HeaderMap) -> bool {
    // [NEW] CRITICAL: Stored files must undergo rate limiting.
    if path.contains("/storage/file/") || path.contains("/storage/files/") {
        return false;
    }

    // 1. Bypass all Admin UI static content paths
    if path.starts_with("/_dashboard") {
        return true;
    }

    // 2. Bypass any API requests originating from the Admin UI (checks Referer header)
    if let Some(referer) = headers.get("referer").and_then(|v| v.to_str().ok()) {
        if referer.contains("/_dashboard") {
            return true;
        }
    }

    // 3. Direct standard routes
    if path == "/styles.css" || path == "/logo" || path == "/favicon.ico" {
        return true;
    }

    // 4. Subdirectories dedicated to static assets
    if path.starts_with("/static/") || path.contains("/assets/") {
        return true;
    }

    // 5. Match against typical static file extensions
    let extensions = [
        ".js", ".css", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".woff", ".woff2", ".ttf",
        ".eot", ".map", ".mp4", ".mp3", ".m4s", ".mpd", ".m3u8", ".ts",
    ];

    // Strip query parameters before matching extensions
    let clean_path = path.split('?').next().unwrap_or(path);

    extensions.iter().any(|ext| clean_path.ends_with(ext))
}

// --- DYNAMIC GCRA RATE LIMITER MIDDLEWARE ---
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();

    // Bypass rate limiting entirely for assets and Admin UI requests
    if should_bypass_rate_limiting(path, req.headers()) {
        return Ok(next.run(req).await);
    }

    // 1. Get the Real IP address (Fallback to actual socket IP, NOT hardcoded 127.0.0.1)
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string());

    // 2. Identify the Client securely
    // Preference: Auth Token > API Key > IP Address
    let client_id = if let Some(auth) = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
    {
        if auth.starts_with("Bearer ") {
            // Hash the token so we don't store raw JWTs in memory keys
            apexkit_core::utils::sha256(auth)
        } else {
            ip.clone()
        }
    } else if let Some(api_key) = req.headers().get("x-api-key").and_then(|v| v.to_str().ok()) {
        api_key.to_string()
    } else {
        ip.clone()
    };

    // 3. Resolve the EventScope dynamically from the path (Outer layer fix)
    let mut scope = EventScope::Root;
    if path.starts_with("/tenant/") {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 3 {
            scope = EventScope::Tenant(parts[2].to_string());
        }
    } else if path.starts_with("/sandbox/") {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 3 {
            scope = EventScope::Sandbox(parts[2].to_string());
        }
    }

    let scope_str = match &scope {
        EventScope::Root => "root",
        EventScope::Tenant(id) => id.as_str(),
        EventScope::Sandbox(id) => id.as_str(),
        _ => "unknown",
    };

    let compound_key = format!("{}:{}", scope_str, client_id);
    let config_cache_key = "system:security_config";

    let config: crate::system::dto::SecurityConfigDto =
        if let Some(cached) = state.root_script_cache.get(config_cache_key).await {
            serde_json::from_str(&cached).unwrap_or_default()
        } else {
            let sec_conf = state.db.get_config("security").await.unwrap_or_default();
            let parsed: crate::system::dto::SecurityConfigDto = if let Some(val) = sec_conf {
                serde_json::from_value(val).unwrap_or_default()
            } else {
                Default::default()
            };
            state
                .root_script_cache
                .insert(
                    config_cache_key.to_string(),
                    serde_json::to_string(&parsed).unwrap_or_default(),
                )
                .await;
            parsed
        };

    let mut limit = config.global_rate_limit.unwrap_or(600);

    match &scope {
        EventScope::Root => limit = config.global_rate_limit.unwrap_or(600),
        EventScope::Sandbox(_) => limit = 60,
        EventScope::Tenant(tid) => {
            let tier_cache_key = format!("tenant_tier:{}", tid);
            let tier = if let Some(t) = state.root_script_cache.get(&tier_cache_key).await {
                t
            } else {
                let mut fetched_tier = "free".to_string();
                if let Ok(tenants) = state.db.list_tenants().await
                    && let Some(t) = tenants.iter().find(|t| &t.id == tid)
                {
                    fetched_tier = t.tier.clone();
                }
                state
                    .root_script_cache
                    .insert(tier_cache_key, fetched_tier.clone())
                    .await;
                fetched_tier
            };

            if tier == "pro" {
                limit = config.tenant_pro_rate_limit.unwrap_or(3000);
            } else {
                limit = config.tenant_free_rate_limit.unwrap_or(120);
            }
        }
        _ => {}
    }

    let limit_u32 = limit.max(1) as u32;

    let limiter = if let Some(l) = state.rate_limiters.get(&limit).await {
        l
    } else {
        let quota = Quota::per_minute(NonZeroU32::new(limit_u32).unwrap());
        let new_limiter = Arc::new(RateLimiter::keyed(quota));
        state.rate_limiters.insert(limit, new_limiter.clone()).await;
        new_limiter
    };

    match limiter.check_key(&compound_key) {
        Ok(_) => {
            let mut response = next.run(req).await;
            response.headers_mut().insert(
                "X-RateLimit-Limit",
                HeaderValue::from_str(&limit.to_string()).unwrap(),
            );
            Ok(response)
        }
        Err(negative) => {
            let wait_time = negative.wait_time_from(DefaultClock::default().now());
            let retry_after_secs = wait_time.as_secs().max(1);
            tracing::warn!(
                "Rate limit exceeded for {} in scope {}. Retry after {}s",
                client_id,
                scope_str,
                retry_after_secs
            );

            let body = Json(json!({
                "error": "too_many_requests",
                "message": format!("Rate limit exceeded. Try again in {} seconds.", retry_after_secs),
                "retry_after": retry_after_secs
            }));

            let mut response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                HeaderValue::from_str(&retry_after_secs.to_string()).unwrap(),
            );
            response.headers_mut().insert(
                "X-RateLimit-Limit",
                HeaderValue::from_str(&limit.to_string()).unwrap(),
            );
            Ok(response)
        }
    }
}

pub async fn metrics_middleware(req: Request, next: Next) -> Response {
    let _start = Instant::now();
    next.run(req).await
}
