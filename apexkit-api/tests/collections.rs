use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

mod common;
use common::setup_test_app;

#[tokio::test]
async fn test_create_collection() {
    let app = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{ "name": "Users", "schema": { "fields": { "name": { "type": "string", "required": true } } } }"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_list_collections() {
    let app = setup_test_app().await;

    // Create a collection
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "name": "Users" }"#))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
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
    assert_eq!(collections.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_get_collection() {
    let app = setup_test_app().await;

    // Create a collection
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "name": "Users" }"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let collection: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let collection_id = collection["id"].as_i64().unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/collections/{}", collection_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let collection: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(collection["name"], "Users");
}

#[tokio::test]
async fn test_update_collection() {
    let app = setup_test_app().await;

    // Create a collection
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "name": "Users" }"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let collection: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let collection_id = collection["id"].as_i64().unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/collections/{}", collection_id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "name": "New Users" }"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let collection: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(collection["name"], "New Users");
}

#[tokio::test]
async fn test_delete_collection() {
    let app = setup_test_app().await;

    // Create a collection
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "name": "Users" }"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let collection: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let collection_id = collection["id"].as_i64().unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/collections/{}", collection_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify that the collection is gone
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/collections/{}", collection_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
