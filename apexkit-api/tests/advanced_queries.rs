use axum::{
    body::{Body, to_bytes},
    http::StatusCode,
};
use serde_json::json;
use tower::ServiceExt;

mod common;
use common::{base_request, setup_test_app};

async fn create_collection_with_data(app: &axum::Router) -> i64 {
    let response = app
        .clone()
        .oneshot(
            base_request()
                .method("POST")
                .uri("/api/v1/collections")
                .body(Body::from(
                    json!({
                        "name": "Products",
                        "schema": {
                            "fields": {
                                "name": { "type": "string", "required": true },
                                "price": { "type": "number", "required": true },
                                "category": { "type": "string", "required": true }
                            }
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    if status != StatusCode::CREATED {
        panic!(
            "Failed to create collection: status={}, body={:?}",
            status,
            String::from_utf8_lossy(&body_bytes)
        );
    }
    let collection: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let collection_id = collection["id"].as_i64().unwrap();

    let products = vec![
        json!({ "name": "Laptop", "price": 1000, "category": "Electronics" }),
        json!({ "name": "Phone", "price": 500, "category": "Electronics" }),
        json!({ "name": "Shirt", "price": 20, "category": "Clothing" }),
        json!({ "name": "Pants", "price": 40, "category": "Clothing" }),
    ];

    for p in products {
        let response = app
            .clone()
            .oneshot(
                base_request()
                    .method("POST")
                    .uri(format!("/api/v1/collections/{}/records", collection_id))
                    .body(Body::from(json!({ "data": p }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        if status != StatusCode::CREATED {
            let body_bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
            panic!(
                "Failed to create record: status={}, body={:?}",
                status,
                String::from_utf8_lossy(&body_bytes)
            );
        }
    }

    collection_id
}

#[tokio::test]
async fn test_advanced_query_filtering() {
    let app = setup_test_app().await;
    let collection_id = create_collection_with_data(&app).await;

    // 1. Filter by category
    let response = app
        .clone()
        .oneshot(
            base_request()
                .method("POST")
                .uri(format!("/api/v1/collections/{}/query", collection_id))
                .body(Body::from(
                    json!({
                        "filter": { "category": "Electronics" }
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
    assert_eq!(res["total"], 2);
    assert_eq!(res["items"].as_array().unwrap().len(), 2);

    // 2. Filter by price > 100
    let response = app
        .clone()
        .oneshot(
            base_request()
                .method("POST")
                .uri(format!("/api/v1/collections/{}/query", collection_id))
                .body(Body::from(
                    json!({
                        "filter": { "price": { "$gt": 100 } }
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
    assert_eq!(res["total"], 2);

    // 3. Sorting and Limit
    let response = app
        .clone()
        .oneshot(
            base_request()
                .method("POST")
                .uri(format!("/api/v1/collections/{}/query", collection_id))
                .body(Body::from(
                    json!({
                        "filter": {},
                        "sort": "-price",
                        "limit": 1
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
    assert_eq!(res["items"].as_array().unwrap().len(), 1);
    assert_eq!(res["items"][0]["data"]["name"], "Laptop");
}
