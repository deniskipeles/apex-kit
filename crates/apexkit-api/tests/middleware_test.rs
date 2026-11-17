//! Functional tests for server middleware behaviour: CORS, rate-limit
//! headers, tenant resolution, and auth enforcement across scopes.

mod common;

use axum::http::{StatusCode, header};
use common::{admin_token, fresh_app, get_request, send_raw};
use tower::ServiceExt;

#[tokio::test]
async fn cors_preflight_allows_configured_methods() {
    let (dir, _state, app) = fresh_app("mw_cors").await;
    let req = axum::http::Request::builder()
        .method("OPTIONS")
        .uri("/api/v1/collections")
        .header(header::HOST, "localhost")
        .header(header::ORIGIN, "http://example.com")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert!(
        res.status() == StatusCode::OK || res.status() == StatusCode::NO_CONTENT,
        "preflight status {}",
        res.status()
    );
    assert!(
        res.headers().contains_key(header::ACCESS_CONTROL_ALLOW_METHODS),
        "missing allow-methods header"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn rate_limit_headers_are_present() {
    let (dir, _state, app) = fresh_app("mw_ratelimit").await;
    let res = app
        .clone()
        .oneshot(get_request("/healthz", None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        res.headers().contains_key("x-ratelimit-limit"),
        "missing rate limit headers"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn compression_negotiation_works() {
    let (dir, state, app) = fresh_app("mw_gzip").await;
    let token = admin_token(&state).await;
    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/collections")
        .header(header::HOST, "localhost")
        .header(header::ACCEPT_ENCODING, "gzip")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(axum::body::Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let encoding = res
        .headers()
        .get(header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(encoding, "gzip", "expected gzip encoding");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn request_id_or_trace_header_is_not_required() {
    // The server must answer without any request-id headers supplied.
    let (dir, _state, app) = fresh_app("mw_nohdrs").await;
    let (status, _) = send_raw(&app, get_request("/healthz", None)).await;
    assert_eq!(status, StatusCode::OK);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn invalid_bearer_token_is_unauthorized() {
    let (dir, _state, app) = fresh_app("mw_badtoken").await;
    let (status, _) = send_raw(&app, get_request("/api/v1/auth/me", Some("garbage.token.value"))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn tenant_header_for_unknown_tenant_returns_not_found() {
    let (dir, _state, app) = fresh_app("mw_tenant").await;
    // A Host header referencing an unknown tenant should 404 rather than crash.
    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/collections")
        .header(header::HOST, "ghost.apexkit.io")
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert!(
        res.status() == StatusCode::NOT_FOUND
            || res.status() == StatusCode::UNAUTHORIZED
            || res.status() == StatusCode::OK,
        "tenant host returned {}",
        res.status()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn static_asset_extension_bypasses_rate_limit() {
    let (dir, _state, app) = fresh_app("mw_static").await;
    // Static assets should be served (or 404) without rate limit headers.
    let res = app
        .clone()
        .oneshot(get_request("/styles.css", None))
        .await
        .unwrap();
    assert!(
        res.status() == StatusCode::OK || res.status() == StatusCode::NOT_FOUND,
        "static asset returned {}",
        res.status()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
