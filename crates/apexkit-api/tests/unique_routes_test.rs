//! Functional route tests for unique constraints and index-backed filtering.
//!
//! Uniqueness is enforced by expression indexes created from the collection
//! schema, so these drive the real HTTP routes and assert the observable
//! contract: duplicate writes are refused, distinct writes are accepted, and
//! indexed fields remain filterable and sortable.

mod common;

use apexkit_core::database::traits::Db;
use axum::http::StatusCode;
use common::{admin_token, fresh_app, get_request, json_request, send};
use serde_json::{Value, json};

async fn make_collection(state: &apexkit_api::AppState, name: &str, schema: Value) {
    let schema: apexkit_core::models::schema::CollectionSchema =
        serde_json::from_value(schema).unwrap();
    state
        .db
        .create_collection(name, &Some(schema), None)
        .await
        .unwrap();
}

fn unique_schema() -> Value {
    json!({
        "fields": { "email": { "type": "email", "required": true, "unique": true } },
        "policies": { "read": "public", "create": "public", "update": "public", "delete": "public" }
    })
}

async fn create_record(
    app: &axum::Router,
    token: &str,
    collection: &str,
    data: Value,
) -> (StatusCode, Value) {
    send(
        app,
        json_request(
            "POST",
            &format!("/api/v1/collections/{collection}/records"),
            &json!({ "data": data }),
            Some(token),
        ),
    )
    .await
}

#[tokio::test]
async fn duplicate_unique_value_is_refused() {
    let (dir, state, app) = fresh_app("uniq_route_dup").await;
    let token = admin_token(&state).await;
    make_collection(&state, "contacts", unique_schema()).await;

    let (status, _) = create_record(&app, &token, "contacts", json!({"email": "a@b.com"})).await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) =
        create_record(&app, &token, "contacts", json!({"email": "a@b.com"})).await;
    assert!(
        !status.is_success(),
        "duplicate must be refused, got {status}: {body}"
    );

    // Only the first record persists.
    let (status, listed) = send(
        &app,
        get_request("/api/v1/collections/contacts/records", Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["total"], 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn distinct_unique_values_are_accepted() {
    let (dir, state, app) = fresh_app("uniq_route_ok").await;
    let token = admin_token(&state).await;
    make_collection(&state, "contacts", unique_schema()).await;

    for i in 0..4 {
        let (status, body) = create_record(
            &app,
            &token,
            "contacts",
            json!({ "email": format!("u{i}@b.com") }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "insert {i} failed: {body}");
    }

    let (_, listed) = send(
        &app,
        get_request("/api/v1/collections/contacts/records", Some(&token)),
    )
    .await;
    assert_eq!(listed["total"], 4);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn duplicate_on_update_is_refused() {
    let (dir, state, app) = fresh_app("uniq_route_update").await;
    let token = admin_token(&state).await;
    make_collection(&state, "contacts", unique_schema()).await;

    let (_, first) = create_record(&app, &token, "contacts", json!({"email": "one@b.com"})).await;
    let (_, second) = create_record(&app, &token, "contacts", json!({"email": "two@b.com"})).await;
    let second_id = second["id"].as_i64().unwrap();

    let (status, body) = send(
        &app,
        json_request(
            "PATCH",
            &format!("/api/v1/collections/contacts/records/{second_id}"),
            &json!({ "data": { "email": "one@b.com" } }),
            Some(&token),
        ),
    )
    .await;
    assert!(!status.is_success(), "colliding update must fail: {status} {body}");

    // Original values survive the rejected update.
    let (_, kept) = send(
        &app,
        get_request(
            &format!("/api/v1/collections/contacts/records/{second_id}"),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(kept["data"]["email"], "two@b.com");
    assert_eq!(first["data"]["email"], "one@b.com");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn composite_unique_pair_is_enforced_over_routes() {
    let (dir, state, app) = fresh_app("uniq_route_composite").await;
    let token = admin_token(&state).await;
    make_collection(
        &state,
        "items",
        json!({
            "fields": {
                "tenant": { "type": "string", "required": true },
                "sku": { "type": "string", "required": true }
            },
            "composite_unique": [["tenant", "sku"]],
            "policies": { "read": "public", "create": "public", "update": "public", "delete": "public" }
        }),
    )
    .await;

    let (s1, _) =
        create_record(&app, &token, "items", json!({"tenant": "a", "sku": "1"})).await;
    assert_eq!(s1, StatusCode::OK);

    let (dup, _) =
        create_record(&app, &token, "items", json!({"tenant": "a", "sku": "1"})).await;
    assert!(!dup.is_success(), "duplicate pair must fail");

    let (s2, _) =
        create_record(&app, &token, "items", json!({"tenant": "a", "sku": "2"})).await;
    assert_eq!(s2, StatusCode::OK);
    let (s3, _) =
        create_record(&app, &token, "items", json!({"tenant": "b", "sku": "1"})).await;
    assert_eq!(s3, StatusCode::OK);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn indexed_field_is_filterable_and_sortable() {
    let (dir, state, app) = fresh_app("uniq_route_index").await;
    let token = admin_token(&state).await;
    make_collection(
        &state,
        "posts",
        json!({
            "fields": {
                "status": { "type": "text", "required": false, "sql_indexed": true },
                "qty": { "type": "number", "required": false, "sql_indexed": true }
            },
            "policies": { "read": "public", "create": "public", "update": "public", "delete": "public" }
        }),
    )
    .await;

    for i in 0..6 {
        let (status, body) = create_record(
            &app,
            &token,
            "posts",
            json!({ "status": if i % 2 == 0 { "even" } else { "odd" }, "qty": i }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "seed {i} failed: {body}");
    }

    let (status, filtered) = send(
        &app,
        get_request(
            "/api/v1/collections/posts/records?filter=%7B%22status%22%3A%22even%22%7D",
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(filtered["total"], 3);

    let (status, sorted) = send(
        &app,
        get_request("/api/v1/collections/posts/records?sort=-qty", Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let qtys: Vec<i64> = sorted["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["data"]["qty"].as_i64().unwrap())
        .collect();
    assert_eq!(qtys, vec![5, 4, 3, 2, 1, 0]);

    let _ = std::fs::remove_dir_all(&dir);
}
