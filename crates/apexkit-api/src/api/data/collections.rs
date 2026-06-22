use crate::hooks::{trigger_filter_hook, trigger_void_hook};
use crate::utils::{extract_log_meta, resolve_collection_by_id_or_name};
use crate::{
    AppError, AppState, BaseUrl, CollectionResponse, CreateCollectionReq, DatabaseConnection,
    IdPath, UpdateCollection,
};
use apexkit_core::{auth::Claims, realtime::EventScope};
use axum::extract::ConnectInfo;
use axum::{
    Extension,
    extract::{Json, Path, State},
    http::{HeaderMap, StatusCode},
};
use serde_json::json;
use std::net::SocketAddr;

// =========================================================
// 1. COLLECTIONS HANDLERS
// =========================================================

#[utoipa::path(
    get,
    path = "/api/v1/collections",
    responses((status = 200, body = Vec<CollectionResponse>))
)]
pub async fn list_collections(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Vec<CollectionResponse>>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    // [TRIGGER] Before List
    trigger_void_hook(
        &state,
        "before_list_collections",
        json!({}),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    let cols = db
        .list_collections()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let resp = cols
        .into_iter()
        .map(|c| CollectionResponse {
            id: c.id,
            name: c.name,
            schema: c.schema,
            index: c.index,
        })
        .collect::<Vec<_>>();

    // [TRIGGER] After List
    let filtered_json = trigger_filter_hook(
        &state,
        "after_list_collections",
        json!(resp),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;
    let final_resp: Vec<CollectionResponse> = serde_json::from_value(filtered_json)
        .map_err(|_| AppError::UnknownError("Script returned invalid collection format".into()))?;

    // [LOG]
    let meta = extract_log_meta(&headers, Some(addr), json!({ "count": final_resp.len() }));
    let _ = db
        .log_audit_event("info", "Collections Listed", "api", Some(meta))
        .await;

    Ok(Json(final_resp))
}

#[utoipa::path(get, path = "/api/v1/collections/{id}", params(IdPath))]
pub async fn get_collection(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<IdPath>,
) -> Result<Json<CollectionResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    // [TRIGGER] Before Get
    trigger_void_hook(
        &state,
        "before_get_collection",
        json!({ "id": path.id }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    // [FIX] Use Resolver
    let c = resolve_collection_by_id_or_name(&db, &path.id).await?;
    let resp = CollectionResponse {
        id: c.id,
        name: c.name,
        schema: c.schema,
        index: c.index,
    };

    // [TRIGGER] After Get
    let filtered_json = trigger_filter_hook(
        &state,
        "after_get_collection",
        json!(resp),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;
    let final_resp: CollectionResponse = serde_json::from_value(filtered_json).unwrap_or(resp);

    // [LOG]
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "collection_id": final_resp.id, "name": final_resp.name }),
    );
    let _ = db
        .log_audit_event("info", "Collection Accessed", "api", Some(meta))
        .await;

    Ok(Json(final_resp))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections",
    request_body = CreateCollectionReq,
    responses((status = 201, body = CollectionResponse))
)]
pub async fn create_collection(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<CreateCollectionReq>,
) -> Result<(StatusCode, Json<CollectionResponse>), AppError> {
    let claims = auth.map(|Extension(c)| c);
    if !matches!(claims, Some(ref c) if c.role == "admin") {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    // [TRIGGER]
    trigger_void_hook(
        &state,
        "before_collection_create",
        json!({ "name": payload.name, "schema": payload.schema }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    let id = db
        .create_collection(&payload.name, &payload.schema, None)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOG]
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "name": payload.name, "id": id }),
    );
    let _ = db
        .log_audit_event("info", "Collection Created", "api", Some(meta))
        .await;

    // [TRIGGER]
    let _ = trigger_void_hook(
        &state,
        "after_collection_create",
        json!({ "id": id, "name": payload.name }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await;

    // --- AUTO RELOAD SCOPE ---
    let state_clone = state.clone();
    let scope_clone = event_scope.clone();
    tokio::spawn(async move {
        trigger_scope_reload(state_clone, scope_clone).await;
    });

    Ok((
        StatusCode::CREATED,
        Json(CollectionResponse {
            id,
            name: payload.name,
            schema: payload.schema,
            index: payload.index,
        }),
    ))
}

#[utoipa::path(patch, path = "/api/v1/collections/{id}", params(IdPath))]
pub async fn update_collection(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<IdPath>,
    Json(payload): Json<UpdateCollection>,
) -> Result<Json<CollectionResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    if !matches!(claims, Some(ref c) if c.role == "admin") {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    // [TRIGGER]
    trigger_void_hook(
        &state,
        "before_collection_update",
        json!({ "id": path.id, "updates": payload }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    // [FIX] Resolve ID
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;
    let c = db
        .update_collection(col.id, payload.name, payload.schema)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOG]
    let meta = extract_log_meta(&headers, Some(addr), json!({ "id": c.id, "name": c.name }));
    let _ = db
        .log_audit_event("info", "Collection Updated", "api", Some(meta))
        .await;

    // [TRIGGER]
    let _ = trigger_void_hook(
        &state,
        "after_collection_update",
        json!({ "id": c.id }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await;

    // --- AUTO RELOAD SCOPE ---
    let state_clone = state.clone();
    let scope_clone = event_scope.clone();
    tokio::spawn(async move {
        trigger_scope_reload(state_clone, scope_clone).await;
    });

    Ok(Json(CollectionResponse {
        id: c.id,
        name: c.name,
        schema: c.schema,
        index: c.index,
    }))
}

#[utoipa::path(delete, path = "/api/v1/collections/{id}", params(IdPath))]
pub async fn delete_collection(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<IdPath>,
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);
    if !matches!(claims, Some(ref c) if c.role == "admin") {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    // [TRIGGER]
    trigger_void_hook(
        &state,
        "before_collection_delete",
        json!({ "id": path.id }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    // [FIX] Resolve ID
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;
    db.delete_collection(col.id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOG]
    let meta = extract_log_meta(&headers, Some(addr), json!({ "id": col.id }));
    let _ = db
        .log_audit_event("warning", "Collection Deleted", "api", Some(meta))
        .await;

    // --- AUTO RELOAD SCOPE ---
    let state_clone = state.clone();
    let scope_clone = event_scope.clone();
    tokio::spawn(async move {
        trigger_scope_reload(state_clone, scope_clone).await;
    });

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/v1/admin/collections/{id}/reindex", params(IdPath), responses((status = 200, description = "Reindexing started")))]
pub async fn reindex_collection_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    Path(path): Path<IdPath>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !matches!(auth.map(|e| e.0.role), Some(r) if r == "admin") {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // [FIX] Resolve ID
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    db.reindex_collection(col.id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn trigger_scope_reload(state: AppState, scope: EventScope) {
    match scope {
        EventScope::Root => {
            let relation_loader = async_graphql::dataloader::DataLoader::new(
                crate::graphql::RelationLoader::new(state.db.clone()),
                tokio::spawn,
            );
            if let Ok(new_schema) =
                crate::graphql::build_schema(state.clone(), std::sync::Arc::new(relation_loader))
                    .await
            {
                let mut lock = state.schema.write().await;
                *lock = new_schema;
                tracing::info!(
                    "[System] Root GraphQL schema automatically reloaded due to schema change."
                );
            }
        }
        EventScope::Tenant(id) => {
            state.tenant_manager.invalidate(&id).await;
            tracing::info!(
                "[System] Tenant '{}' cache invalidated due to schema change.",
                id
            );
        }
        EventScope::Sandbox(id) => {
            state.sandbox_manager.invalidate(&id).await;
            tracing::info!(
                "[System] Sandbox '{}' cache invalidated due to schema change.",
                id
            );
        }
        _ => {}
    }
}
