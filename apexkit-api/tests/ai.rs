use axum::{
    body::{to_bytes, Body},
    http::StatusCode,
};
use tower::ServiceExt;
use serde_json::json;

mod common;
use common::{setup_test_app, base_request};

#[tokio::test]
async fn test_ai_action_crud() {
    let app = setup_test_app().await;

    // 1. Create AI Action
    let response = app.clone()
        .oneshot(
            base_request()
                .method("POST")
                .uri("/api/v1/admin/ai/actions")
                .body(Body::from(json!({
                    "name": "Summary",
                    "slug": "summary",
                    "model": "gemini-1.5-flash",
                    "system_prompt": "Summarize",
                    "template": "{{text}}"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let action_id = res["id"].as_i64().unwrap();

    // 2. List AI Actions
    let response = app.clone()
        .oneshot(
            base_request()
                .method("GET")
                .uri("/api/v1/admin/ai/actions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(res.as_array().unwrap().iter().any(|a| a["slug"] == "summary"));

    // 3. Delete AI Action
    let response = app.clone()
        .oneshot(
            base_request()
                .method("DELETE")
                .uri(format!("/api/v1/admin/ai/actions/{}", action_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ai_run_not_configured() {
    let app = setup_test_app().await;

    // Create an action first
    app.clone()
        .oneshot(
            base_request()
                .method("POST")
                .uri("/api/v1/admin/ai/actions")
                .body(Body::from(json!({
                    "name": "Summary",
                    "slug": "summary",
                    "model": "gemini-1.5-flash",
                    "system_prompt": "Summarize",
                    "template": "{{text}}"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Run action - should fail because AI is not configured (no API key in config)
    let response = app
        .oneshot(
            base_request()
                .method("POST")
                .uri("/api/v1/ai/run/summary")
                .body(Body::from(json!({
                    "variables": { "text": "Hello" }
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(res["message"], "AI not configured");
}
