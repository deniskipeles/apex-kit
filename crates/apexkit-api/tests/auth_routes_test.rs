//! Functional tests for authentication routes: register, login, /me, roles.

mod common;

use axum::http::StatusCode;
use common::{admin_token, fresh_app, get_request, json_request, send};
use serde_json::json;

#[tokio::test]
async fn register_disabled_returns_forbidden() {
    let (dir, _state, app) = fresh_app("auth_reg_disabled").await;
    let reg = json_request(
        "POST",
        "/api/v1/auth/register",
        &json!({"email": "new@apexkit.io", "password": "secret123", "role": "user"}),
        None,
    );
    let (status, body) = send(&app, reg).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "got {body}");
    assert_eq!(body["status"], 403);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn register_then_login_round_trip() {
    let (dir, state, app) = fresh_app("auth_flow").await;

    // Enable public registration via the config store.
    state
        .db
        .set_config("general", &json!({"allow_public_registration": true}), false)
        .await
        .unwrap();

    let reg = json_request(
        "POST",
        "/api/v1/auth/register",
        &json!({"email": "new@apexkit.io", "password": "secret123", "role": "user"}),
        None,
    );
    let (status, body) = send(&app, reg).await;
    assert_eq!(status, StatusCode::OK, "register failed: {body}");
    assert!(body["token"].is_string());
    assert_eq!(body["user"]["email"], "new@apexkit.io");

    let login = json_request(
        "POST",
        "/api/v1/auth/login",
        &json!({"email": "new@apexkit.io", "password": "secret123"}),
        None,
    );
    let (status, body) = send(&app, login).await;
    assert_eq!(status, StatusCode::OK, "login failed: {body}");
    assert!(body["token"].is_string());
    assert_eq!(body["user"]["role"], "user");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn register_duplicate_user_is_rejected() {
    let (dir, state, app) = fresh_app("auth_reg_dup").await;
    state
        .db
        .set_config("general", &json!({"allow_public_registration": true}), false)
        .await
        .unwrap();

    let body = json!({"email": "dup@apexkit.io", "password": "secret123", "role": "user"});
    let (first, _) = send(
        &app,
        json_request("POST", "/api/v1/auth/register", &body, None),
    )
    .await;
    assert_eq!(first, StatusCode::OK);

    let (second, resp) = send(
        &app,
        json_request("POST", "/api/v1/auth/register", &body, None),
    )
    .await;
    assert!(!second.is_success(), "duplicate register must fail: {resp}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn login_with_wrong_password_is_rejected() {
    let (dir, state, app) = fresh_app("auth_badpw").await;
    let hash = apexkit_core::auth::hash_password("correct").unwrap();
    state
        .db
        .create_user("u@apexkit.io", &hash, "user", None)
        .await
        .unwrap();

    let login = json_request(
        "POST",
        "/api/v1/auth/login",
        &json!({"email": "u@apexkit.io", "password": "wrong"}),
        None,
    );
    let (status, _body) = send(&app, login).await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST,
        "wrong password should not authenticate, got {status}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn login_with_unknown_user_is_rejected() {
    let (dir, _state, app) = fresh_app("auth_nouser").await;
    let login = json_request(
        "POST",
        "/api/v1/auth/login",
        &json!({"email": "ghost@apexkit.io", "password": "whatever"}),
        None,
    );
    let (status, _body) = send(&app, login).await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_FOUND,
        "unknown user should not authenticate, got {status}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn public_registration_cannot_self_assign_admin() {
    let (dir, state, app) = fresh_app("auth_reg_admin").await;
    state
        .db
        .set_config("general", &json!({"allow_public_registration": true}), false)
        .await
        .unwrap();

    let reg = json_request(
        "POST",
        "/api/v1/auth/register",
        &json!({"email": "wannabe@apexkit.io", "password": "secret123", "role": "admin"}),
        None,
    );
    let (status, body) = send(&app, reg).await;
    assert_eq!(status, StatusCode::OK, "got {body}");
    assert_eq!(
        body["user"]["role"], "user",
        "public registration must not grant admin"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn me_returns_current_user() {
    let (dir, state, app) = fresh_app("auth_me").await;
    let token = admin_token(&state).await;

    let (status, body) = send(&app, get_request("/api/v1/auth/me", Some(&token))).await;
    assert_eq!(status, StatusCode::OK, "me failed: {body}");
    assert_eq!(body["email"], "admin@apexkit.io");
    assert_eq!(body["role"], "admin");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn me_without_token_is_unauthorized() {
    let (dir, _state, app) = fresh_app("auth_me_401").await;
    let (status, _body) = send(&app, get_request("/api/v1/auth/me", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn roles_endpoint_lists_known_roles() {
    let (dir, state, app) = fresh_app("auth_roles").await;
    let token = admin_token(&state).await;
    let (status, body) = send(&app, get_request("/api/v1/auth/roles", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    let roles = body["roles"].as_array().expect("roles array");
    assert!(roles.iter().any(|r| r == "admin"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn me_update_changes_metadata() {
    let (dir, state, app) = fresh_app("auth_me_update").await;
    let token = admin_token(&state).await;

    let req = json_request(
        "PATCH",
        "/api/v1/auth/me",
        &json!({"metadata": {"theme": "dark"}}),
        Some(&token),
    );
    let (status, _body) = send(&app, req).await;
    assert!(
        status == StatusCode::OK || status == StatusCode::NO_CONTENT,
        "update me returned {status}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
