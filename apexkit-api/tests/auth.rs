use axum::{
    body::{Body, to_bytes},
    http::StatusCode,
};
use serde_json::json;
use tower::ServiceExt;

mod common;
use common::{setup_test_app, test_request};

#[tokio::test]
async fn test_auth_register_and_login() {
    let app = setup_test_app().await;

    // 1. Register
    let response = app
        .clone()
        .oneshot(
            test_request()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": "test@example.com",
                        "password": "password123",
                        "role": "user"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(res["token"].is_string());
    assert_eq!(res["user"]["email"], "test@example.com");

    // 2. Login
    let response = app
        .clone()
        .oneshot(
            test_request()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": "test@example.com",
                        "password": "password123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let token = res["token"].as_str().unwrap();

    // 3. Get Me
    let response = app
        .oneshot(
            test_request()
                .method("GET")
                .uri("/api/v1/auth/me")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(res["email"], "test@example.com");
}

#[tokio::test]
async fn test_auth_login_fail() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            test_request()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": "nonexistent@example.com",
                        "password": "wrongpassword"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_roles() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            test_request()
                .method("GET")
                .uri("/api/v1/auth/roles")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(res["roles"].is_array());
}
