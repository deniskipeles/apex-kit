use crate::AppState;
use apexkit_core::realtime::EventScope;
use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{
    Quota, RateLimiter,
    clock::{Clock, DefaultClock},
};
use serde_json::json;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

// --- [NEW] DYNAMIC GCRA RATE LIMITER MIDDLEWARE ---
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let api_key = req.headers().get("x-api-key").and_then(|v| v.to_str().ok());
    let client_id = api_key.unwrap_or(&ip).to_string();

    let scope = req
        .extensions()
        .get::<EventScope>()
        .cloned()
        .unwrap_or(EventScope::Root);
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
