//! Functional tests for the GraphQL layer: schema building over live
//! collections and execution of generated queries.

mod common;

use apexkit_api::graphql::{RelationLoader, build_schema};
use async_graphql::dataloader::DataLoader;
use async_graphql::{Request, Variables};
use common::{admin_token, create_posts_collection, fresh_app};
use serde_json::json;
use std::sync::Arc;

async fn full_schema(state: &apexkit_api::AppState) -> async_graphql::dynamic::Schema {
    let loader = Arc::new(DataLoader::new(
        RelationLoader::new(state.db.clone()),
        tokio::spawn,
    ));
    build_schema(state.clone(), loader).await.unwrap()
}

#[tokio::test]
async fn schema_builds_with_status_field() {
    let (dir, state, _app) = fresh_app("gql_build").await;
    let schema = full_schema(&state).await;
    let res = schema.execute("{ _status }").await;
    assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
    let data = res.data.into_json().unwrap();
    assert!(data["_status"].is_string());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn schema_exposes_collection_type_and_query() {
    let (dir, state, _app) = fresh_app("gql_collection").await;
    create_posts_collection(&state).await;

    // Seed a record directly so the generated resolver has data.
    let col = state.db.list_collections().await.unwrap()[0].clone();
    state
        .db
        .create_record(col.id, &json!({"title": "GraphQL Post", "views": 7}))
        .await
        .unwrap();

    let schema = full_schema(&state).await;

    // Introspect the generated type name for the collection.
    let introspect = schema
        .execute("{ __schema { types { name } } }")
        .await;
    let data = introspect.data.into_json().unwrap();
    let names: Vec<String> = data["__schema"]["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.iter().any(|n| n == "Posts"), "types: {names:?}");

    // Query the generated list field.
    let query = "{ posts { items { id title views } total } }";
    let res = schema.execute(query).await;
    assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
    let data = res.data.into_json().unwrap();
    assert_eq!(data["posts"]["total"], 1);
    assert_eq!(data["posts"]["items"][0]["title"], "GraphQL Post");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn graphql_users_query_requires_policy() {
    let (dir, state, _app) = fresh_app("gql_users").await;
    // Default user read policy is admin; without claims it must be denied.
    let schema = full_schema(&state).await;

    let res = schema
        .execute(Request::new("{ _users { total items { id email } } }"))
        .await;
    assert!(!res.errors.is_empty(), "unauthenticated _users must be denied");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn graphql_users_query_with_admin_claims_succeeds() {
    let (dir, state, _app) = fresh_app("gql_users_admin").await;
    let _token = admin_token(&state).await;
    let schema = full_schema(&state).await;

    let claims = apexkit_core::auth::decode_jwt(&_token).unwrap();
    let res = schema
        .execute(
            Request::new("{ _users { total items { id email role } } }").data(claims),
        )
        .await;
    assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
    let data = res.data.into_json().unwrap();
    assert_eq!(data["_users"]["total"], 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn graphql_unknown_field_returns_error() {
    let (dir, state, _app) = fresh_app("gql_unknown").await;
    let schema = full_schema(&state).await;
    let res = schema.execute("{ thisFieldDoesNotExist }").await;
    assert!(!res.errors.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn graphql_variables_are_accepted() {
    let (dir, state, _app) = fresh_app("gql_vars").await;
    let schema = full_schema(&state).await;
    let claims = apexkit_core::auth::decode_jwt(&admin_token(&state).await).unwrap();

    let req = Request::new("query($n: Int) { _users(limit: $n) { total } }")
        .variables(Variables::from_json(json!({"n": 5})))
        .data(claims);
    let res = schema.execute(req).await;
    assert!(res.errors.is_empty(), "errors: {:?}", res.errors);

    let _ = std::fs::remove_dir_all(&dir);
}
