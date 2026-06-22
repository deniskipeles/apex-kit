use super::dataloaders::RelationLoader;
use crate::{state::AppState, utils::DatabaseConnection};
use apexkit_core::auth::Claims;
use apexkit_core::realtime::EventScope;
use async_graphql::dataloader::DataLoader;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    Extension,
    extract::{Path, State},
    response::IntoResponse,
};
use std::collections::HashMap;
use std::sync::Arc;

// =========================================================
// GRAPHQL HANDLERS
// =========================================================
pub async fn graphql_handler(
    auth: Option<Extension<Claims>>, // <--- Extract Claims
    State(state): State<AppState>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let schema = state.schema.read().await;

    // Inject claims into the execution context
    let mut request = req.into_inner();
    if let Some(Extension(claims)) = auth {
        request = request.data(claims);
    }

    schema.execute(request).await.into()
}

pub async fn tenant_graphql_handler(
    auth: Option<Extension<Claims>>, // <--- Extract Claims
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let relation_loader = Arc::new(DataLoader::new(
        RelationLoader::new(db.clone()),
        tokio::spawn,
    ));
    let mut tenant_state = state.clone();
    tenant_state.db = db;

    match crate::graphql::build_schema(tenant_state, relation_loader).await {
        Ok(schema) => {
            let mut request = req.into_inner().data(event_scope);
            // Inject claims
            if let Some(Extension(claims)) = auth {
                request = request.data(claims);
            }
            schema.execute(request).await.into()
        }
        Err(e) => async_graphql::Response::from_errors(vec![async_graphql::ServerError::new(
            e.to_string(),
            None,
        )])
        .into(),
    }
}

pub async fn sandbox_graphql_handler(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let relation_loader = Arc::new(DataLoader::new(
        RelationLoader::new(db.clone()),
        tokio::spawn,
    ));
    let mut sandbox_state = state.clone();
    sandbox_state.db = db;
    match crate::graphql::build_schema(sandbox_state, relation_loader).await {
        Ok(schema) => schema
            .execute(req.into_inner().data(event_scope))
            .await
            .into(),
        Err(e) => async_graphql::Response::from_errors(vec![async_graphql::ServerError::new(
            e.to_string(),
            None,
        )])
        .into(),
    }
}

pub async fn graphql_playground() -> impl IntoResponse {
    axum::response::Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/graphql"),
    ))
}
pub async fn sandbox_graphql_playground(
    Path(params): Path<HashMap<String, String>>,
) -> impl IntoResponse {
    let id = params.get("session_id").map(|s| s.as_str()).unwrap_or("");
    axum::response::Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new(&format!("/sandbox/{}/graphql", id)),
    ))
}
pub async fn tenant_graphql_playground(
    Path(params): Path<HashMap<String, String>>,
) -> impl IntoResponse {
    let id = params.get("tenant_id").map(|s| s.as_str()).unwrap_or("");
    axum::response::Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new(&format!("/tenant/{}/graphql", id)),
    ))
}
