use axum::{
    extract::{State, Request},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{Response, IntoResponse},
};
use crate::{AppState, settings::SecurityConfigDto};

pub async fn cors_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    // 1. Get Origin Header from Request
    let origin_header = req.headers().get(header::ORIGIN).cloned();
    let method = req.method().clone();

    // 2. Fetch Settings (Cached DB call)
    let security_setting = state.db.get_setting("security").await.unwrap_or(None);
    
    let config: SecurityConfigDto = if let Some(val) = security_setting {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        // Default to Allow All if not configured
        SecurityConfigDto { cors_allow_all: true, cors_origins: "".to_string() }
    };

    // 3. Determine if Origin is Allowed
    let mut allow_origin_val: Option<HeaderValue> = None;

    if config.cors_allow_all {
        allow_origin_val = Some(HeaderValue::from_static("*"));
    } else if let Some(origin) = origin_header {
        if let Ok(origin_str) = origin.to_str() {
            // Check comma-separated list
            let allowed_list: Vec<&str> = config.cors_origins.split(',').map(|s| s.trim()).collect();
            if allowed_list.contains(&origin_str) {
                allow_origin_val = Some(origin); // Echo back the specific origin
            }
        }
    }

    // 4. Handle Response
    // If it's an OPTIONS request (Preflight), we return immediately
    if method == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        let headers = response.headers_mut();

        if let Some(val) = allow_origin_val {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
            headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"));
            headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("Authorization, Content-Type"));
            headers.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("86400")); // Cache preflight for 24h
        }
        return response;
    }

    // Standard Request
    let mut response = next.run(req).await;
    
    if let Some(val) = allow_origin_val {
        response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
        // Required for non-* origins to tell caches to vary by Origin
        response.headers_mut().insert(header::VARY, HeaderValue::from_static("Origin")); 
    }

    response
}