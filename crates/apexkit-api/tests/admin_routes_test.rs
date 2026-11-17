//! Functional tests for admin routes: users, tenants, sandboxes, API keys,
//! config, settings, dashboard and audit logs.

mod common;

use axum::http::StatusCode;
use common::{admin_token, fresh_app, get_request, json_request, send};
use serde_json::json;

#[tokio::test]
async fn admin_users_list() {
    let (dir, state, app) = fresh_app("admin_users").await;
    let token = admin_token(&state).await;

    let (status, body) = send(&app, get_request("/api/v1/admin/users", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["email"], "admin@apexkit.io");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn admin_users_requires_admin() {
    let (dir, _state, app) = fresh_app("admin_users_403").await;
    let (status, _body) = send(&app, get_request("/api/v1/admin/users", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn admin_tenant_lifecycle() {
    let (dir, state, app) = fresh_app("admin_tenants").await;
    let token = admin_token(&state).await;

    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/admin/tenants",
            &json!({"tenant_id": "t1", "name": "Tenant One", "tier": "free"}),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "create tenant failed: {body}");

    let (status, body) = send(&app, get_request("/api/v1/admin/tenants", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    let tenants = body.as_array().unwrap();
    assert_eq!(tenants.len(), 1);
    assert_eq!(tenants[0]["id"], "t1");

    let (status, _) = send(
        &app,
        json_request(
            "DELETE",
            "/api/v1/admin/tenants/t1",
            &json!(null),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "delete tenant returned {status}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn admin_sandbox_lifecycle() {
    let (dir, state, app) = fresh_app("admin_sandbox").await;
    let token = admin_token(&state).await;

    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/admin/sandboxes",
            &json!({"name": "sb1", "clone_strategy": "none"}),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "create sandbox failed: {body}");
    let id = body["id"].as_str().unwrap().to_string();

    let (status, body) = send(&app, get_request("/api/v1/admin/sandboxes", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    let (status, _) = send(
        &app,
        json_request(
            "DELETE",
            &format!("/api/v1/admin/sandboxes/{id}"),
            &json!(null),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "delete sandbox returned {status}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn admin_api_key_lifecycle() {
    let (dir, state, app) = fresh_app("admin_keys").await;
    let token = admin_token(&state).await;

    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/admin/keys",
            &json!({"name": "k1", "target_tenant": "root", "env_type": "sys", "roles": ["admin"]}),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "create key failed: {body}");
    let raw = body["key"].as_str().unwrap().to_string();
    let id = body["info"]["id"].as_i64().unwrap();
    assert!(raw.starts_with("root_sys_prod_"));

    let (status, body) = send(&app, get_request("/api/v1/admin/keys", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    let (status, _) = send(
        &app,
        json_request(
            "DELETE",
            &format!("/api/v1/admin/keys/{id}"),
            &json!(null),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn admin_config_set_list_delete() {
    let (dir, state, app) = fresh_app("admin_config").await;
    let token = admin_token(&state).await;

    let (status, _) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/admin/config",
            &json!({"key": "greeting", "value": "hi", "encrypted": false}),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "set config returned {status}");

    let (status, body) = send(&app, get_request("/api/v1/admin/config", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    let items = body.as_array().unwrap();
    assert!(items.iter().any(|c| c["key"] == "greeting"));

    let (status, _) = send(
        &app,
        json_request(
            "DELETE",
            "/api/v1/admin/config/greeting",
            &json!(null),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn admin_settings_get_and_patch() {
    let (dir, state, app) = fresh_app("admin_settings").await;
    let token = admin_token(&state).await;

    let (status, body) = send(&app, get_request("/api/v1/admin/settings", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("security").is_some(), "settings shape: {body}");

    let (status, _) = send(
        &app,
        json_request(
            "PATCH",
            "/api/v1/admin/settings",
            &json!({"allow_public_registration": true}),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "patch settings returned {status}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn admin_dashboard_stats() {
    let (dir, state, app) = fresh_app("admin_dash").await;
    let token = admin_token(&state).await;

    let (status, body) = send(&app, get_request("/api/v1/admin/dashboard", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["stats"].is_object(), "dashboard shape: {body}");
    assert!(body["chart"].is_array());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn admin_logs_listing() {
    let (dir, state, app) = fresh_app("admin_logs").await;
    let token = admin_token(&state).await;

    state
        .db
        .log_audit_event("info", "test event", "test", None)
        .await
        .unwrap();

    let (status, body) = send(&app, get_request("/api/v1/admin/logs", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"].is_array());
    assert!(body["total"].as_i64().unwrap() >= 1);

    let _ = std::fs::remove_dir_all(&dir);
}
