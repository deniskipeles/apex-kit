//! Functional tests for collection routes (CRUD) through the Axum router.

mod common;

use axum::http::StatusCode;
use common::{admin_token, fresh_app, get_request, json_request, send};
use serde_json::json;

fn posts_body() -> serde_json::Value {
    json!({
        "name": "posts",
        "schema": {
            "fields": {
                "title": { "type": "text", "required": true },
                "views": { "type": "number", "required": false }
            },
            "policies": { "read": "public", "create": "public", "update": "public", "delete": "public" }
        }
    })
}

#[tokio::test]
async fn create_and_list_collections() {
    let (dir, state, app) = fresh_app("coll_create").await;
    let token = admin_token(&state).await;

    let (status, body) = send(
        &app,
        json_request("POST", "/api/v1/collections", &posts_body(), Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create failed: {body}");
    assert_eq!(body["name"], "posts");
    assert!(body["id"].as_i64().unwrap() > 0);

    let (status, body) = send(&app, get_request("/api/v1/collections", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    let items = body.as_array().expect("list must be an array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "posts");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn get_collection_by_name() {
    let (dir, state, app) = fresh_app("coll_byname").await;
    let token = admin_token(&state).await;
    send(
        &app,
        json_request("POST", "/api/v1/collections", &posts_body(), Some(&token)),
    )
    .await;

    let (status, body) = send(&app, get_request("/api/v1/collections/posts", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "posts");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn get_missing_collection_returns_404() {
    let (dir, state, app) = fresh_app("coll_404").await;
    let token = admin_token(&state).await;
    let (status, _body) = send(&app, get_request("/api/v1/collections/nope", Some(&token))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn update_collection_name() {
    let (dir, state, app) = fresh_app("coll_update").await;
    let token = admin_token(&state).await;
    let (_, created) = send(
        &app,
        json_request("POST", "/api/v1/collections", &posts_body(), Some(&token)),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let (status, body) = send(
        &app,
        json_request(
            "PATCH",
            &format!("/api/v1/collections/{id}"),
            &json!({"name": "articles"}),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "articles");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn delete_collection_removes_it() {
    let (dir, state, app) = fresh_app("coll_delete").await;
    let token = admin_token(&state).await;
    let (_, created) = send(
        &app,
        json_request("POST", "/api/v1/collections", &posts_body(), Some(&token)),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let (status, _) = send(
        &app,
        json_request(
            "DELETE",
            &format!("/api/v1/collections/{id}"),
            &json!(null),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "delete returned {status}");

    let (status, _) = send(&app, get_request("/api/v1/collections/posts", Some(&token))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn create_collection_requires_auth() {
    let (dir, _state, app) = fresh_app("coll_auth").await;
    let (status, _body) = send(
        &app,
        json_request("POST", "/api/v1/collections", &posts_body(), None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn create_collection_with_invalid_name_is_rejected() {
    let (dir, state, app) = fresh_app("coll_badname").await;
    let token = admin_token(&state).await;

    // Identifier validation rejects names with spaces/punctuation.
    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/collections",
            &json!({"name": "bad name!!", "schema": null}),
            Some(&token),
        ),
    )
    .await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::INTERNAL_SERVER_ERROR,
        "invalid name should be rejected, got {status}: {body}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn collection_reindex_endpoint_succeeds() {
    let (dir, state, app) = fresh_app("coll_reindex").await;
    let token = admin_token(&state).await;
    let (_, created) = send(
        &app,
        json_request("POST", "/api/v1/collections", &posts_body(), Some(&token)),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    // Reindex by numeric id queues a background job.
    let (status, body) = send(
        &app,
        json_request(
            "POST",
            &format!("/api/v1/admin/collections/{id}/reindex"),
            &json!(null),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reindex failed: {body}");
    assert_eq!(body["success"], true);

    let _ = std::fs::remove_dir_all(&dir);
}
