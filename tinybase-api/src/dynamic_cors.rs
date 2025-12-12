// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/dynamic_cors.rs ===========================
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
    let origin_header = req.headers().get(header::ORIGIN).cloned();
    let method = req.method().clone();
    
    // Capture requested headers to reflect them back (Simplifies header whitelisting)
    let request_headers = req.headers()
        .get("access-control-request-headers")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Load Settings
    let security_setting = state.db.get_setting("security").await.unwrap_or(None);
    
    let config: SecurityConfigDto = if let Some(val) = security_setting {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        SecurityConfigDto { cors_allow_all: true, cors_origins: "".to_string() }
    };

    let mut allow_origin_val: Option<HeaderValue> = None;
    let mut allow_credentials = false;

    if let Some(origin) = origin_header {
        if let Ok(origin_str) = origin.to_str() {
            if config.cors_allow_all {
                // Public API mode
                allow_origin_val = Some(HeaderValue::from_static("*"));
            } else {
                // Restricted mode: Check allowed list
                let allowed_list: Vec<&str> = config.cors_origins.split(',').map(|s| s.trim()).collect();
                
                if allowed_list.contains(&origin_str) {
                    allow_origin_val = Some(origin); // Echo exact origin
                    allow_credentials = true; // Allow creds for specific matches
                } else {
                    // DEBUG LOG: Helps identify why it blocked
                    tracing::warn!("CORS Blocked: Origin '{}' not in allowed list: {:?}", origin_str, allowed_list);
                }
            }
        }
    }

    // --- PREFLIGHT (OPTIONS) HANDLER ---
    if method == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        let headers = response.headers_mut();

        if let Some(val) = allow_origin_val {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
            // Explicitly allow PATCH and other methods
            headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"));
            
            // Allow standard headers + whatever the client requested
            let default_headers = "Authorization, Content-Type, Accept, Origin, X-Requested-With";
            let final_headers = if !request_headers.is_empty() {
                format!("{}, {}", default_headers, request_headers)
            } else {
                default_headers.to_string()
            };
            
            if let Ok(h_val) = HeaderValue::from_str(&final_headers) {
                headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, h_val);
            }

            headers.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("86400"));
            
            if allow_credentials {
                headers.insert(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, HeaderValue::from_static("true"));
            }
        }
        return response;
    }

    // --- ACTUAL REQUEST HANDLER ---
    let mut response = next.run(req).await;
    
    if let Some(val) = allow_origin_val {
        response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
        if allow_credentials {
            response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, HeaderValue::from_static("true"));
        }
        // Important for caching proxies to know response varies by Origin
        response.headers_mut().insert(header::VARY, HeaderValue::from_static("Origin"));
    }

    response
}

