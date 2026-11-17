//! Unit tests for the API data-transfer objects and the `AppError` -> HTTP
//! status mapping.

use apexkit_api::dto::{
    AuthRequest, CreateCollectionReq, ProblemDetail, RecordListResponse, RecordResponse, SearchQuery,
    UpdateCollection,
};
use apexkit_api::AppError;
use axum::response::IntoResponse;
use serde_json::json;

#[test]
fn auth_request_deserializes() {
    let r: AuthRequest =
        serde_json::from_value(json!({"email": "a@b.com", "password": "secret"})).unwrap();
    assert_eq!(r.email, "a@b.com");
    assert_eq!(r.password, "secret");
}

#[test]
fn auth_request_requires_both_fields() {
    assert!(serde_json::from_value::<AuthRequest>(json!({"email": "a@b.com"})).is_err());
}

#[test]
fn create_collection_req_parses_name_and_schema() {
    let r: CreateCollectionReq = serde_json::from_value(json!({
        "name": "posts",
        "schema": {"fields": {"title": {"type": "text", "required": true}}}
    }))
    .unwrap();
    assert_eq!(r.name, "posts");
    assert!(r.schema.is_some());
    assert!(r.schema.unwrap().fields.contains_key("title"));
}

#[test]
fn update_collection_req_all_fields_optional() {
    let r: UpdateCollection = serde_json::from_value(json!({})).unwrap();
    assert!(r.name.is_none());
    assert!(r.schema.is_none());
}

#[test]
fn search_query_deserializes_q_and_limit() {
    let r: SearchQuery = serde_json::from_value(json!({"q": "hello", "limit": 5})).unwrap();
    assert_eq!(r.q, "hello");
    assert_eq!(r.limit, Some(5));
}

#[test]
fn search_query_requires_q() {
    assert!(serde_json::from_value::<SearchQuery>(json!({"limit": 5})).is_err());
}

#[test]
fn record_response_serializes() {
    let r = RecordResponse {
        id: 1,
        data: json!({"title": "x"}),
        expand: None,
        created: "2024-01-01 00:00:00".into(),
        updated: "2024-01-01 00:00:00".into(),
    };
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["id"], 1);
    assert_eq!(v["data"]["title"], "x");
}

#[test]
fn record_list_response_serializes_total() {
    let r = RecordListResponse {
        items: vec![],
        total: 42,
    };
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["total"], 42);
    assert!(v["items"].as_array().unwrap().is_empty());
}

#[test]
fn problem_detail_has_expected_shape() {
    let p = ProblemDetail {
        error: "Not Found".into(),
        message: "missing".into(),
        details: None,
        status: 404,
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["status"], 404);
    assert_eq!(v["error"], "Not Found");
}

#[tokio::test]
async fn app_error_status_mapping() {
    use axum::http::StatusCode;

    let cases: Vec<(AppError, StatusCode)> = vec![
        (AppError::NotFound("x".into()), StatusCode::NOT_FOUND),
        (AppError::JsonError("x".into()), StatusCode::BAD_REQUEST),
        (AppError::Unauthorized("x".into()), StatusCode::UNAUTHORIZED),
        (AppError::Forbidden("x".into()), StatusCode::FORBIDDEN),
        (
            AppError::Validation(vec![apexkit_core::validation::ValidationError::InvalidType(
                "f".into(),
                "obj".into(),
            )]),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            AppError::UnknownError("x".into()),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ];

    for (err, expected) in cases {
        let resp = err.into_response();
        assert_eq!(resp.status(), expected, "wrong status for {expected}");
    }
}

#[tokio::test]
async fn app_error_body_is_problem_detail() {
    let resp = AppError::NotFound("thing".into()).into_response();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["status"], 404);
    assert!(body["error"].is_string());
    assert!(body["message"].is_string());
}

#[test]
fn app_error_display_is_non_empty() {
    assert!(!AppError::NotFound("x".into()).to_string().is_empty());
    assert!(!AppError::Forbidden("x".into()).to_string().is_empty());
}
