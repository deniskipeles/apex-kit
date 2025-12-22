use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

mod common;
use common::setup_test_app;

async fn create_test_collection(app: &axum::Router) -> i64 {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{ "name": "Posts", "schema": { "fields": { "title": { "type": "string", "required": true } } } }"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let collection: serde_json::Value = serde_json::from_slice(&body).unwrap();
    collection["id"].as_i64().unwrap()
}

#[tokio::test]
async fn test_create_record() {
    let app = setup_test_app().await;
    let collection_id = create_test_collection(&app).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/collections/{}/records", collection_id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "data": { "title": "Hello!" } }"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_create_record_validation_error() {
    let app = setup_test_app().await;
    let collection_id = create_test_collection(&app).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/collections/{}/records", collection_id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "data": { "wrong_field": "Hello!" } }"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_list_records() {
    let app = setup_test_app().await;
    let collection_id = create_test_collection(&app).await;

    // Create a record
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/collections/{}/records", collection_id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "data": { "title": "Hello!" } }"#))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/collections/{}/records", collection_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let records: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(records.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_get_record() {
    let app = setup_test_app().await;
    let collection_id = create_test_collection(&app).await;

    // Create a record
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/collections/{}/records", collection_id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "data": { "title": "Hello!" } }"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let record_id = record["id"].as_i64().unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/collections/{}/records/{}",
                    collection_id, record_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(record["data"]["title"], "Hello!");
}

#[tokio::test]
async fn test_update_record() {
    let app = setup_test_app().await;
    let collection_id = create_test_collection(&app).await;

    // Create a record
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/collections/{}/records", collection_id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "data": { "title": "Hello!" } }"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let record_id = record["id"].as_i64().unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!(
                    "/api/v1/collections/{}/records/{}",
                    collection_id, record_id
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "data": { "title": "New Hello!" } }"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(record["data"]["title"], "New Hello!");
}

#[tokio::test]
async fn test_delete_record() {
    let app = setup_test_app().await;
    let collection_id = create_test_collection(&app).await;

    // Create a record
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/collections/{}/records", collection_id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{ "data": { "title": "Hello!" } }"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let record_id = record["id"].as_i64().unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/api/v1/collections/{}/records/{}",
                    collection_id, record_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify that the record is gone
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/collections/{}/records/{}",
                    collection_id, record_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
