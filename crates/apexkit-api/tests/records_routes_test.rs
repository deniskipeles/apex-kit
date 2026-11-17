//! Functional tests for record routes: CRUD, pagination, filtering, sorting,
//! projection, validation errors, and the SQL query engine endpoint.

mod common;

use axum::http::StatusCode;
use common::{admin_token, create_posts_collection, fresh_app, json_request, send};
use serde_json::json;

async fn seed(app: &axum::Router, token: &str, n: usize) {
    for i in 0..n {
        send(
            app,
            json_request(
                "POST",
                "/api/v1/collections/posts/records",
                &json!({"data": {"title": format!("Post {i}"), "views": i}}),
                Some(token),
            ),
        )
        .await;
    }
}

#[tokio::test]
async fn create_get_update_delete_record() {
    let (dir, state, app) = fresh_app("rec_crud").await;
    let token = admin_token(&state).await;
    create_posts_collection(&state).await;

    let (status, created) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/collections/posts/records",
            &json!({"data": {"title": "Hello", "views": 1}}),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create failed: {created}");
    let id = created["id"].as_i64().unwrap();
    assert_eq!(created["data"]["title"], "Hello");

    let (status, got) = send(
        &app,
        common::get_request(&format!("/api/v1/collections/posts/records/{id}"), Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["data"]["title"], "Hello");

    let (status, updated) = send(
        &app,
        json_request(
            "PUT",
            &format!("/api/v1/collections/posts/records/{id}"),
            &json!({"data": {"title": "Updated", "views": 2}}),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["title"], "Updated");

    let (status, _) = send(
        &app,
        json_request(
            "DELETE",
            &format!("/api/v1/collections/posts/records/{id}"),
            &json!(null),
            Some(&token),
        ),
    )
    .await;
    assert!(status.is_success(), "delete returned {status}");

    let (status, _) = send(
        &app,
        common::get_request(&format!("/api/v1/collections/posts/records/{id}"), Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn list_records_reports_total() {
    let (dir, state, app) = fresh_app("rec_list").await;
    let token = admin_token(&state).await;
    create_posts_collection(&state).await;
    seed(&app, &token, 3).await;

    let (status, body) = send(
        &app,
        common::get_request("/api/v1/collections/posts/records", Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3);
    assert_eq!(body["items"].as_array().unwrap().len(), 3);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn list_records_pagination_limits_items() {
    let (dir, state, app) = fresh_app("rec_page").await;
    let token = admin_token(&state).await;
    create_posts_collection(&state).await;
    seed(&app, &token, 5).await;

    let (status, body) = send(
        &app,
        common::get_request(
            "/api/v1/collections/posts/records?per_page=2&page=1",
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 5);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn list_records_filter_by_field() {
    let (dir, state, app) = fresh_app("rec_filter").await;
    let token = admin_token(&state).await;
    create_posts_collection(&state).await;
    seed(&app, &token, 4).await;

    let uri = "/api/v1/collections/posts/records?filter=%7B%22views%22%3A%7B%22%24gt%22%3A1%7D%7D";
    let (status, body) = send(&app, common::get_request(uri, Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 2, "views>1 should match 2 of 4");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn missing_required_field_returns_422() {
    let (dir, state, app) = fresh_app("rec_422").await;
    let token = admin_token(&state).await;
    create_posts_collection(&state).await;

    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/collections/posts/records",
            &json!({"data": {"views": 1}}),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "got {body}");
    assert_eq!(body["status"], 422);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn create_record_on_missing_collection_returns_404() {
    let (dir, state, app) = fresh_app("rec_missing_coll").await;
    let token = admin_token(&state).await;

    let (status, _body) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/collections/ghost/records",
            &json!({"data": {"title": "x"}}),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn sql_query_engine_endpoint_returns_rows() {
    let (dir, state, app) = fresh_app("rec_sqlq").await;
    let token = admin_token(&state).await;
    create_posts_collection(&state).await;
    seed(&app, &token, 3).await;

    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/collections/posts/query",
            &json!({"from": "posts", "select": ["title"], "sort": "-views"}),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "query failed: {body}");
    let rows = body.as_array().expect("query must return rows");
    assert_eq!(rows.len(), 3);
    assert!(rows[0].get("title").is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn sql_query_engine_aggregation() {
    let (dir, state, app) = fresh_app("rec_sqlagg").await;
    let token = admin_token(&state).await;
    create_posts_collection(&state).await;
    seed(&app, &token, 3).await;

    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/v1/collections/posts/query",
            &json!({
                "from": "posts",
                "select": [{"fn": "sum", "field": "views", "as": "total_views"}]
            }),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "agg failed: {body}");
    let rows = body.as_array().unwrap();
    assert_eq!(rows[0]["total_views"], json!(3.0));

    let _ = std::fs::remove_dir_all(&dir);
}
