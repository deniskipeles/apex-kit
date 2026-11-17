//! Functional tests for the system/utility routes: health, version, metrics,
//! OpenAPI spec, GraphQL, and 404 handling.

mod common;

use axum::http::StatusCode;
use common::{fresh_app, get_request, json_request, send, send_raw};
use serde_json::json;

#[tokio::test]
async fn health_check_returns_ok() {
    let (dir, _state, app) = fresh_app("health").await;
    let (status, body) = send_raw(&app, get_request("/healthz", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"OK");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn version_reports_all_crate_versions() {
    let (dir, _state, app) = fresh_app("version").await;
    let (status, body) = send(&app, get_request("/version", None)).await;
    assert_eq!(status, StatusCode::OK);
    for key in ["root", "api", "core", "vector"] {
        assert!(body[key].is_string(), "missing version key {key}: {body}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn openapi_spec_is_served() {
    let (dir, _state, app) = fresh_app("openapi").await;
    let (status, body) = send_raw(&app, get_request("/openapi.json", None)).await;
    assert_eq!(status, StatusCode::OK);
    let spec: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(spec.get("paths").is_some(), "OpenAPI must expose paths");
    assert!(spec.get("openapi").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn graphql_status_query_works() {
    let (dir, _state, app) = fresh_app("graphql").await;
    let req = json_request("POST", "/graphql", &json!({"query": "{ _status }"}), None);
    let (status, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["data"]["_status"].is_string(),
        "expected a _status string, got {body}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn graphql_introspection_lists_types() {
    let (dir, _state, app) = fresh_app("graphql_intro").await;
    let req = json_request(
        "POST",
        "/graphql",
        &json!({"query": "{ __schema { queryType { name } } }"}),
        None,
    );
    let (status, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["__schema"]["queryType"]["name"], "Query");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn unknown_api_route_returns_404() {
    let (dir, _state, app) = fresh_app("404").await;
    let (status, _body) = send(&app, get_request("/api/v1/definitely-not-a-route", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn dashboard_route_is_served() {
    let (dir, _state, app) = fresh_app("dashboard").await;
    // The dashboard is a rust-embed asset; without the built UI it may be a
    // placeholder, but it must not 5xx.
    let (status, _body) = send_raw(&app, get_request("/_dashboard", None)).await;
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_FOUND,
        "dashboard returned unexpected status {status}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn metrics_endpoint_responds() {
    let (dir, _state, app) = fresh_app("metrics").await;
    let (status, _body) = send_raw(&app, get_request("/metrics", None)).await;
    assert_eq!(status, StatusCode::OK);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn app_details_route_responds() {
    let (dir, _state, app) = fresh_app("app_details").await;
    let (status, _body) = send(&app, get_request("/app-details", None)).await;
    assert!(
        status == StatusCode::OK || status == StatusCode::UNAUTHORIZED,
        "app-details returned {status}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
