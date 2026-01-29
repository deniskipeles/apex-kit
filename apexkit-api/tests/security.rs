use axum::{
    body::{to_bytes, Body},
    http::StatusCode,
};
use tower::ServiceExt;
use serde_json::json;

mod common;
use common::{setup_test_app, test_request};

fn user_token() -> String {
    // A standard user token (role: user)
    apexkit_core::auth::create_jwt(2, "user@example.com", "user", "root").unwrap()
}

#[tokio::test]
async fn test_admin_route_access_denied_for_user() {
    let app = setup_test_app().await;
    let token = user_token();

    // Use GET routes where possible to avoid 415/400 errors from Json extractor
    let admin_routes = vec![
        ("GET", "/api/v1/admin/users"),
        ("GET", "/api/v1/admin/tenants"),
        ("GET", "/api/v1/admin/logs"),
        ("GET", "/api/v1/admin/dashboard"),
        ("GET", "/api/v1/admin/scripts"),
        ("GET", "/api/v1/admin/ai/actions"),
    ];

    for (method, uri) in admin_routes {
        let response = app.clone()
            .oneshot(
                test_request()
                    .method(method)
                    .uri(uri)
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "Route {} should be forbidden for user", uri
        );
    }
}

#[tokio::test]
async fn test_tenant_data_isolation_cross_access() {
    let app = setup_test_app().await;
    let admin_token = apexkit_core::auth::create_jwt(1, "admin@apexkit.io", "admin", "root").unwrap();

    // 1. Create Tenant A and a collection in it
    app.clone().oneshot(
        test_request()
            .method("POST")
            .uri("/api/v1/admin/tenants")
            .header("authorization", format!("Bearer {}", admin_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"tenant_id": "tenant-a"}).to_string()))
            .unwrap()
    ).await.unwrap();

    app.clone().oneshot(
        test_request()
            .method("POST")
            .uri("/tenant/tenant-a/api/v1/collections")
            .header("authorization", format!("Bearer {}", admin_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "SecretA"}).to_string()))
            .unwrap()
    ).await.unwrap();

    // 2. Create Tenant B
    app.clone().oneshot(
        test_request()
            .method("POST")
            .uri("/api/v1/admin/tenants")
            .header("authorization", format!("Bearer {}", admin_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"tenant_id": "tenant-b"}).to_string()))
            .unwrap()
    ).await.unwrap();

    // 3. Try to access Tenant A's collection from Tenant B's endpoint
    // We expect it NOT to find SecretA in Tenant B's context.
    let response = app.clone().oneshot(
        test_request()
            .method("GET")
            .uri("/tenant/tenant-b/api/v1/collections")
            .header("authorization", format!("Bearer {}", admin_token))
            .body(Body::empty())
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let collections: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let names: Vec<&str> = collections.as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert!(!names.contains(&"SecretA"));
}

#[tokio::test]
async fn test_malformed_json_payload() {
    let app = setup_test_app().await;
    let token = apexkit_core::auth::create_jwt(1, "admin@apexkit.io", "admin", "root").unwrap();

    let response = app
        .oneshot(
            test_request()
                .method("POST")
                .uri("/api/v1/collections")
                .header("authorization", format!("Bearer {}", token))
                .header("content-type", "application/json")
                .body(Body::from("{ \"name\": \"Unterminated"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
