use axum::{
    body::{to_bytes, Body},
    http::StatusCode,
};
use tower::ServiceExt;
use serde_json::json;

mod common;
use common::{setup_test_app, base_request, test_request};

#[tokio::test]
async fn test_script_lifecycle_and_execution() {
    let app = setup_test_app().await;

    // 1. Create a script
    let script_code = r#"
export default async function(req) {
    const data = await req.json();
    return new Response({
        message: "Hello " + data.name,
        timestamp: new Date().toISOString()
    });
}
"#;

    let response = app.clone()
        .oneshot(
            base_request()
                .method("POST")
                .uri("/api/v1/admin/scripts")
                .body(Body::from(json!({
                    "name": "hello-script",
                    "trigger_type": "manual",
                    "code": script_code
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 2. List scripts
    let response = app.clone()
        .oneshot(
            base_request()
                .method("GET")
                .uri("/api/v1/admin/scripts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(res.as_array().unwrap().iter().any(|s| s["name"] == "hello-script"));

    // 3. Execute script
    let response = app.clone()
        .oneshot(
            test_request()
                .method("POST")
                .uri("/api/v1/run/hello-script")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "name": "World" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(res["message"], "Hello World");
}
