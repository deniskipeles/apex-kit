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
    
    // [UPDATED] Capture Host AND Forwarded Host (for Cloud Environments)
    let host_header = req.headers().get(header::HOST)
        .and_then(|v| v.to_str().ok()).map(|s| s.to_string());
        
    let forwarded_host = req.headers().get("x-forwarded-host")
        .and_then(|v| v.to_str().ok()).map(|s| s.to_string());

    // Capture requested headers
    let request_headers = req.headers()
        .get("access-control-request-headers")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let security_setting = state.db.get_config("security").await.unwrap_or(None);
    let config: SecurityConfigDto = if let Some(val) = security_setting {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        SecurityConfigDto { cors_allow_all: true, cors_origins: "".to_string(), tenant_transparency: false }
    };

    let mut allow_origin_val: Option<HeaderValue> = None;
    let mut allow_credentials = false;
    let mut blocked = false; // Track if we should strictly block execution

    if let Some(origin) = origin_header {
        if let Ok(origin_str) = origin.to_str() {
            // if config.cors_allow_all {
            //     allow_origin_val = Some(HeaderValue::from_static("*"));
            if config.cors_allow_all {
                allow_origin_val = Some(origin.clone());
                allow_credentials = true;
            } else {
                let allowed_list: Vec<&str> = config.cors_origins.split(',').map(|s| s.trim()).collect();
                
                // [UPDATED] Robust Same-Origin Check
                // Strips http:// or https:// to compare just the domain/port
                let clean_origin = origin_str.replace("https://", "").replace("http://", "");
                
                let matches_host = host_header.as_ref().map(|h| clean_origin.contains(h)).unwrap_or(false);
                let matches_forwarded = forwarded_host.as_ref().map(|h| clean_origin.contains(h)).unwrap_or(false);

                // [NEW] Smart Key Check for CORS Bypass
                // We check headers for x-api-key. If valid AND bypass_cors=true, we skip origin checks.
                let mut bypass_cors = false;
                
                if let Some(key_header) = req.headers().get("x-api-key") {
                    if let Ok(key) = key_header.to_str() {
                        // Verification is async. 
                        // Note: In middleware, we clone db ref.
                        if let Ok(Some(api_key)) = state.db.verify_api_key(key).await {
                            if api_key.bypass_cors {
                                bypass_cors = true;
                            }
                        }
                    }
                }

                if allowed_list.contains(&origin_str) || matches_host || matches_forwarded || bypass_cors {
                    allow_origin_val = Some(origin); 
                    allow_credentials = true; 
                } else {
                    tracing::warn!("CORS Blocked: Origin '{}' not allowed. Hosts checked: {:?}/{:?}", origin_str, host_header, forwarded_host);
                    blocked = true; // Mark as blocked
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
            headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"));
            
            let default_headers = "Authorization, Content-Type, Accept, Origin, X-Requested-With, HX-Request, HX-Current-URL, HX-Target, HX-Trigger";
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

    // --- [CRITICAL FIX] STRICT BLOCKING ---
    // If we determined this is a blocked Origin, STOP execution here. 
    // Do not call next.run(req), or the DB operation will happen anyway.
    if blocked {
        return StatusCode::FORBIDDEN.into_response();
    }

    // --- ACTUAL REQUEST HANDLER ---
    let mut response = next.run(req).await;
    
    if let Some(val) = allow_origin_val {
        response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
        if allow_credentials {
            response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, HeaderValue::from_static("true"));
        }
        response.headers_mut().insert(header::VARY, HeaderValue::from_static("Origin"));
    }

    response
}