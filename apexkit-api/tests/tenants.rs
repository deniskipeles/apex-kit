use axum::{
    body::{to_bytes, Body},
    http::StatusCode,
};
use tower::ServiceExt;
use serde_json::json;

mod common;
use common::{setup_test_app, base_request};

#[tokio::test]
async fn test_tenant_creation_and_isolation() {
    let app = setup_test_app().await;
    let tenant_id = format!("tenant-{}", uuid::Uuid::new_v4());

    // 1. Create a tenant
    let response = app.clone()
        .oneshot(
            base_request()
                .method("POST")
                .uri("/api/v1/admin/tenants")
                .body(Body::from(json!({
                    "tenant_id": tenant_id
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    // 2. List tenants
    let response = app.clone()
        .oneshot(
            base_request()
                .method("GET")
                .uri("/api/v1/admin/tenants")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let tenants: Vec<String> = serde_json::from_slice(&body).unwrap();
    assert!(tenants.contains(&tenant_id));

    // 3. Create a collection in the tenant
    let response = app.clone()
        .oneshot(
            base_request()
                .method("POST")
                .uri(format!("/tenant/{}/api/v1/collections", tenant_id))
                .body(Body::from(json!({ "name": "TenantPosts" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    // 4. List collections in root - should be empty
    let response = app.clone()
        .oneshot(
            base_request()
                .method("GET")
                .uri("/api/v1/collections")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let collections: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(collections.as_array().unwrap().len(), 0);

    // 5. List collections in tenant - should have TenantPosts
    let response = app.clone()
        .oneshot(
            base_request()
                .method("GET")
                .uri(format!("/tenant/{}/api/v1/collections", tenant_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let collections: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(collections.as_array().unwrap().len(), 1);
    assert_eq!(collections[0]["name"], "TenantPosts");
}
