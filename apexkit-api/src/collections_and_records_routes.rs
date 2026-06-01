use crate::{
    AppError, AppState, BaseUrl, CollectionResponse, CreateCollectionReq, DatabaseConnection,
    IdPath, RecordListResponse, RecordPath, RecordResponse, RelationRequest, SearchQuery,
    UpdateCollection, extract_log_meta, get_current_model, get_tenant_id_from_scope,
    resolve_collection_by_id_or_name, trigger_filter_hook, trigger_hooks, trigger_void_hook,
};
use apexkit_core::{
    auth::Claims,
    jobs::Job,
    policies,
    query::QueryOptions,
    query_engine::ApexQuery,
    realtime::DbEvent,
    realtime::EventScope,
    schema::{CollectionSchema, FieldType},
    validation::validate_record,
};
use axum::extract::ConnectInfo;
use axum::{
    Extension,
    extract::{Json, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use utoipa::ToSchema;

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

// =========================================================
// 2. RECORDS HANDLERS
// =========================================================

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/records",
    params(IdPath, QueryOptions),
    responses((status = 200, body = RecordListResponse))
)]
pub async fn list_records(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<IdPath>,
    Query(q): Query<QueryOptions>,
) -> Result<Json<RecordListResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    let policy = col
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");

    // Compile Policy to SQL
    let rls_sql = policies::compile_to_sql(policy, claims.as_ref())
        .map_err(|e| AppError::UnknownError(format!("Policy Compilation Failed: {}", e)))?;

    // Block table-level rejections early
    if rls_sql == "1=0" {
        return Err(AppError::Forbidden("Read denied".into()));
    }

    let query_json = json!(q);
    let mut modified_q: QueryOptions = match trigger_hooks(
        &state,
        "before_list_records",
        &col,
        None,
        &query_json,
        claims.as_ref(),
        Some(base_url.clone()),
        Some(&event_scope.clone()),
    )
    .await?
    {
        Some(modified_json) => serde_json::from_value(modified_json).unwrap_or(q.clone()),
        None => q.clone(),
    };

    // Inject RLS SQL into the final query
    modified_q.rls_sql = Some(rls_sql);

    let res = db
        .list_records(col.id, modified_q.clone())
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let mut response_data = RecordListResponse {
        items: res
            .items
            .into_iter()
            .map(|r| RecordResponse {
                id: r.id,
                data: r.data,
                expand: r.expand,
                created: r.created,
                updated: r.updated,
            })
            .collect(),
        total: res.total,
    };

    // [FIX 2] Use trigger_hooks for AFTER hook
    let response_json = json!(response_data);
    if let Some(modified_json) = trigger_hooks(
        &state,
        "after_list_records",
        &col,
        None,
        &response_json,
        claims.as_ref(),
        Some(base_url.clone()),
        Some(&event_scope.clone()),
    )
    .await?
    {
        response_data = serde_json::from_value(modified_json).unwrap_or(response_data);
    }

    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "collection": col.name, "count": response_data.items.len(), "filter": modified_q.filter }),
    );
    let _ = db
        .log_audit_event("info", "Records Listed", "api", Some(meta))
        .await;

    Ok(Json(response_data))
}

#[utoipa::path(get, path = "/api/v1/collections/{id}/records/{record_id}", params(RecordPath, QueryOptions), responses((status = 200, body = RecordResponse)))]
pub async fn get_record(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<RecordPath>,
    Query(q): Query<QueryOptions>,
) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    // [UPDATED] Resolve Collection by ID or Name
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let query_json = json!(q);
    // [TRIGGER] Before Get
    trigger_hooks(
        &state,
        "before_get_record",
        &col,
        Some(path.record_id),
        &query_json,
        claims.as_ref(),
        Some(base_url.clone()),
        Some(&event_scope.clone()),
    )
    .await?;

    let r = db
        .get_record(col.id, path.record_id, q.expand)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Record not found".into()))?;

    let policy = col
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), Some(&r.data)) {
        return Err(AppError::Forbidden("Read denied".into()));
    }

    let response = RecordResponse {
        id: r.id,
        data: r.data,
        expand: r.expand,
        created: r.created,
        updated: r.updated,
    };

    // [TRIGGER] After Get
    let response_json = json!(response);
    let final_resp = match trigger_hooks(
        &state,
        "after_get_record",
        &col,
        Some(path.record_id),
        &response_json,
        claims.as_ref(),
        Some(base_url.clone()),
        Some(&event_scope.clone()),
    )
    .await?
    {
        Some(modified_json) => serde_json::from_value(modified_json)
            .unwrap_or(serde_json::from_value(response_json).unwrap()),
        None => serde_json::from_value(response_json).unwrap(),
    };

    // [LOG]
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "collection": col.name, "id": path.record_id }),
    );
    let _ = db
        .log_audit_event("info", "Record Accessed", "api", Some(meta))
        .await;

    Ok(Json(final_resp))
}

// Helper to inject auto fileds AND SANITIZE THE STRING NUMBERS
fn inject_auto_fields(
    data: &mut serde_json::Value,
    schema: &CollectionSchema,
    user_id: Option<i64>,
) {
    if let Some(obj) = data.as_object_mut() {
        for (name, def) in &schema.fields {
            // Check the relation and owner field and turn to number
            if def.r#type == FieldType::Relation || def.r#type == FieldType::Owner {
                if let Some(val) = obj.get(name) {
                    if let Some(num) = val.as_str().and_then(|s| s.parse::<i64>().ok()) {
                        obj.insert(name.clone(), serde_json::json!(num));
                    }
                }
            }
            // Check if field is effectively "missing" (not present, null, or empty string)
            // Empty string check fixes frontend forms sending "" for empty dates
            let is_missing = match obj.get(name) {
                None => true,
                Some(val) => val.is_null() || (val.as_str().map(|s| s.is_empty()).unwrap_or(false)),
            };

            if is_missing {
                // Clean up empty strings to ensure clean state for injection or validation
                if obj.contains_key(name) {
                    obj.remove(name);
                }

                match def.r#type {
                    // 1. Owner: Inject User ID
                    FieldType::Owner => {
                        if def.auto {
                            if let Some(uid) = user_id {
                                obj.insert(name.clone(), serde_json::json!(uid));
                            }
                        } else if let Some(default_val) = &def.default {
                            obj.insert(name.clone(), default_val.clone());
                        }
                    }
                    // 2. Date: Inject Current Timestamp
                    FieldType::Date => {
                        if def.auto {
                            // Inject current UTC time in ISO 8601 format
                            obj.insert(
                                name.clone(),
                                serde_json::json!(chrono::Utc::now().to_rfc3339()),
                            );
                        } else if let Some(default_val) = &def.default {
                            obj.insert(name.clone(), default_val.clone());
                        }
                    }
                    // 3. Others: Inject configured default
                    _ => {
                        if let Some(default_val) = &def.default {
                            obj.insert(name.clone(), default_val.clone());
                        }
                    }
                }
            }
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/records",
    request_body = apexkit_core::models::Record,
    params(IdPath),
    responses((status = 201, body = RecordResponse))
)]
pub async fn create_record(
    BaseUrl(base_url): BaseUrl,
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<IdPath>,
    Json(p): Json<apexkit_core::models::Record>,
) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    // [UPDATED] Resolve Collection by ID or Name
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let policy = col
        .schema
        .as_ref()
        .map(|s| s.policies.create.as_str())
        .unwrap_or("auth");
    if !policies::check_access(policy, claims.as_ref(), None) {
        // [LOG] Failed
        let meta = extract_log_meta(
            &headers,
            Some(addr),
            json!({ "error": "forbidden", "collection": col.name }),
        );
        let _ = db
            .log_audit_event("warning", "Create Record Denied", "api", Some(meta))
            .await;
        return Err(AppError::Forbidden("Create denied".into()));
    }

    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    // [TRIGGER] Record-level Hook (legacy) AND System Hook (new)
    let mut data_to_save = match trigger_hooks(
        &state,
        "before_create_record",
        &col,
        None,
        &p.data,
        claims.as_ref(),
        Some(base_url.clone()),
        Some(&event_scope.clone()),
    )
    .await?
    {
        Some(d) => d,
        None => p.data,
    };

    // [NEW] Auto-Inject Owner ID
    if let Some(schema) = &col.schema {
        let uid = claims.as_ref().map(|c| c.uid);
        inject_auto_fields(&mut data_to_save, schema, uid);

        // THEN Validate
        validate_record(schema, &data_to_save).map_err(AppError::Validation)?;
    }

    let rid = db
        .create_record(col.id, &data_to_save)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // --- [NEW] AUTO RE-INDEX OSE ON MILESTONES ---
    if let Some(schema) = &col.schema {
        if schema.fields.values().any(|f| f.ose_indexed) {
            let cache_key = format!("{:?}_{}", event_scope, col.id);

            // 1. Get count (from Cache, or fallback to DB if Cache expired)
            let current_count =
                if let Some(cached_count) = state.record_count_cache.get(&cache_key).await {
                    cached_count + 1
                } else {
                    let mut opts = QueryOptions::default();
                    opts.limit = Some(1); // We only need the 'total' metadata, limit 1 speeds it up
                    db.list_records(col.id, opts)
                        .await
                        .map(|r| r.total)
                        .unwrap_or(1)
                };

            // 2. Update Cache
            state
                .record_count_cache
                .insert(cache_key, current_count)
                .await;

            // 3. Calculate Milestone
            // Power logic:
            // count 1..9    -> power 10^0 = 1    (all trigger)
            // count 10..99  -> power 10^1 = 10   (triggers on 10, 20, 30...)
            // count 100..   -> power 10^2 = 100  (triggers on 100, 200, 300...)
            let power = 10_i64.pow((current_count.to_string().len() as u32).saturating_sub(1));
            let is_milestone = current_count > 0 && current_count % power == 0;

            if is_milestone {
                tracing::info!(
                    "Milestone {} reached for collection {}. Triggering OSE auto-reindex.",
                    current_count,
                    col.name
                );
                let db_clone = db.clone();
                let col_id = col.id;

                // Spawn in background so it doesn't block the API response
                tokio::spawn(async move {
                    if let Err(e) = db_clone.reindex_collection(col_id).await {
                        tracing::error!("Auto re-index failed for col {}: {}", col_id, e);
                    }
                });
            }
        }
    }
    // ---------------------------------------------

    // [LOG] Success
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "collection": col.name, "record_id": rid, "user_id": claims.as_ref().map(|c| c.uid) }),
    );
    let _ = db
        .log_audit_event("info", "Record Created", "api", Some(meta))
        .await;

    // ... (broadcast, hooks, jobs) ...
    let _ = state.tx.send(DbEvent::Insert {
        collection_id: col.id,
        record_id: rid,
        data: data_to_save.clone(),
        scope: event_scope.clone(),
    });

    let _ = trigger_hooks(
        &state,
        "after_create_record",
        &col,
        Some(rid),
        &data_to_save,
        claims.as_ref(),
        Some(base_url),
        Some(&event_scope.clone()),
    )
    .await;

    // Jobs (Vector/Index)
    if let Some(schema) = col.schema {
        let current_tenant = get_tenant_id_from_scope(Some(&event_scope));
        let model_name = get_current_model();

        for (field_name, def) in &schema.fields {
            if def.vectorize {
                if let Some(content_val) = data_to_save.get(field_name).and_then(|v| v.as_str()) {
                    // Determine if this field is a File reference or raw Text
                    let c_type = if def.r#type == FieldType::File {
                        "file"
                    } else {
                        "text"
                    };

                    let job = Job::GenerateEmbedding {
                        tenant_id: current_tenant.clone(),
                        collection_id: col.id,
                        record_id: rid,
                        field_name: field_name.clone(),
                        content: content_val.to_string(),
                        content_type: c_type.to_string(),
                        model: model_name.clone(),
                    };
                    state.queue.enqueue(job).await;
                }
            }
        }
        if schema.fields.values().any(|f| f.ose_indexed) {
            let job = Job::IndexRecord {
                collection_id: col.id,
                record_id: rid,
                data: data_to_save.clone(),
                schema: schema.clone(),
                tenant_id: current_tenant.clone(),
            };
            state.queue.enqueue(job).await;
        }
    }

    Ok(Json(RecordResponse {
        id: rid,
        data: data_to_save,
        expand: None,
        created: chrono::Utc::now().to_rfc3339(),
        updated: chrono::Utc::now().to_rfc3339(),
    }))
}

#[utoipa::path(
    patch,
    path = "/api/v1/collections/{id}/records/{record_id}",
    params(RecordPath)
)]
pub async fn update_record(
    BaseUrl(base_url): BaseUrl,
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<RecordPath>,
    Json(p): Json<apexkit_core::models::Record>,
) -> Result<Json<RecordResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    // [UPDATED] Resolve Collection
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let existing = db
        .get_record(col.id, path.record_id, None)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Record not found".into()))?;

    let policy = col
        .schema
        .as_ref()
        .map(|s| s.policies.update.as_str())
        .unwrap_or("admin");
    if !policies::check_access(policy, claims.as_ref(), Some(&existing.data)) {
        return Err(AppError::Forbidden("Update denied".into()));
    }

    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);

    // Create a clone for the hook so we don't move the original p.data
    let hook_data = p.data.clone();

    let data_updates = match trigger_hooks(
        &state,
        "before_update_record",
        &col,
        Some(path.record_id),
        &hook_data,
        claims.as_ref(),
        Some(base_url.clone()),
        Some(&event_scope.clone()),
    )
    .await?
    {
        Some(d) => d,
        None => p.data,
    };

    let r = db
        .update_record(col.id, path.record_id, &data_updates)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOG]
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "collection": col.name, "record_id": path.record_id, "user_id": claims.as_ref().map(|c| c.uid) }),
    );
    let _ = db
        .log_audit_event("info", "Record Updated", "api", Some(meta))
        .await;

    let _ = state.tx.send(DbEvent::Update {
        collection_id: col.id,
        record_id: path.record_id,
        data: r.data.clone(),
        scope: event_scope.clone(),
    });
    let _ = trigger_hooks(
        &state,
        "after_update_record",
        &col,
        Some(path.record_id),
        &r.data,
        claims.as_ref(),
        Some(base_url),
        Some(&event_scope.clone()),
    )
    .await;

    if let Some(schema) = col.schema {
        let current_tenant = get_tenant_id_from_scope(Some(&event_scope));
        let model_name = get_current_model();

        for (field_name, def) in &schema.fields {
            if def.vectorize {
                // Use data_updates which is the final payload being sent to DB
                if let Some(content_val) = data_updates.get(field_name).and_then(|v| v.as_str()) {
                    let c_type = if def.r#type == FieldType::File {
                        "file"
                    } else {
                        "text"
                    };

                    let job = Job::GenerateEmbedding {
                        tenant_id: current_tenant.clone(),
                        collection_id: col.id,
                        record_id: path.record_id,
                        field_name: field_name.clone(),
                        content: content_val.to_string(),
                        content_type: c_type.to_string(),
                        model: model_name.clone(),
                    };
                    state.queue.enqueue(job).await;
                }
            }
        }
    }

    Ok(Json(RecordResponse {
        id: r.id,
        data: r.data,
        expand: r.expand,
        created: r.created,
        updated: r.updated,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/collections/{id}/records/{record_id}",
    params(RecordPath)
)]
pub async fn delete_record(
    BaseUrl(base_url): BaseUrl,
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<RecordPath>,
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);

    // [UPDATED] Resolve Collection
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let existing = db
        .get_record(col.id, path.record_id, None)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Record not found".into()))?;

    let policy = col
        .schema
        .as_ref()
        .map(|s| s.policies.delete.as_str())
        .unwrap_or("admin");
    if !policies::check_access(policy, claims.as_ref(), Some(&existing.data)) {
        return Err(AppError::Forbidden("Delete denied".into()));
    }

    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);

    let _ = trigger_hooks(
        &state,
        "before_delete_record",
        &col,
        Some(path.record_id),
        &existing.data,
        claims.as_ref(),
        Some(base_url.clone()),
        Some(&event_scope.clone()),
    )
    .await?;

    db.delete_record(col.id, path.record_id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOG]
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "collection": col.name, "record_id": path.record_id, "user_id": claims.as_ref().map(|c| c.uid) }),
    );
    let _ = db
        .log_audit_event("warning", "Record Deleted", "api", Some(meta))
        .await;

    let _ = state.tx.send(DbEvent::Delete {
        collection_id: col.id,
        record_id: path.record_id,
        scope: event_scope.clone(),
    });
    let _ = trigger_hooks(
        &state,
        "after_delete_record",
        &col,
        Some(path.record_id),
        &existing.data,
        claims.as_ref(),
        Some(base_url),
        Some(&event_scope.clone()),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// DTO for Advanced Query
// This now matches the full power of the new Query Engine
#[derive(Deserialize, ToSchema)]
pub struct AdvancedQueryRequest {
    pub from: Option<String>,                   // Optional if ID in path is used
    pub select: Option<Vec<serde_json::Value>>, // Complex SelectField JSON
    pub filter: Option<serde_json::Value>,
    pub group_by: Option<Vec<String>>,
    pub sort: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub system: Option<bool>,
    pub pipeline: Option<Vec<serde_json::Value>>, // Pipeline Steps
}

// --- HANDLER ---

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/query",
    request_body = AdvancedQueryRequest,
    params(IdPath),
    // [UPDATED] Response is generic JSON Array because structure depends on SELECT
    responses((status = 200, body = Vec<serde_json::Value>))
)]
pub async fn query_records_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(_state): State<AppState>, // Hooks not yet supported for raw engine in this iteration
    BaseUrl(_base_url): BaseUrl,
    _scope: Option<Extension<EventScope>>,
    Path(path): Path<IdPath>,
    Json(payload): Json<AdvancedQueryRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let policy = col
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");

    // Compile Policy to SQL
    let rls_sql = policies::compile_to_sql(policy, claims.as_ref())
        .map_err(|e| AppError::UnknownError(format!("Policy Compilation Failed: {}", e)))?;

    if rls_sql == "1=0" {
        return Err(AppError::Forbidden("Read denied".into()));
    }

    let query = ApexQuery {
        from: col.name.clone(),
        select: payload
            .select
            .map(|v| serde_json::from_value(serde_json::Value::Array(v)).unwrap_or_default())
            .unwrap_or_default(),
        r#where: payload.filter,
        group_by: payload.group_by.unwrap_or_default(),
        sort: payload.sort,
        limit: payload.limit,
        offset: payload.offset,
        system: payload.system.unwrap_or(false),
        pipeline: payload
            .pipeline
            .map(|v| serde_json::from_value(serde_json::Value::Array(v)).unwrap_or_default())
            .unwrap_or_default(),
        rls_sql: Some(rls_sql), // INJECT RLS
    };

    let result = db
        .query_engine(query)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(result))
}

#[utoipa::path(get, path = "/api/v1/collections/{id}/search", params(IdPath, SearchQuery), responses((status = 200, body = Vec<RecordResponse>)))]
pub async fn search_records(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    Path(path): Path<IdPath>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    // [UPDATED] Resolve Collection
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let policy = col
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }
    let res = db
        .search_records(col.id, &q.q)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(
        res.into_iter()
            .map(|r| RecordResponse {
                id: r.id,
                data: r.data,
                expand: r.expand,
                created: r.created,
                updated: r.updated,
            })
            .collect(),
    ))
}

#[utoipa::path(
    get, 
    path = "/api/v1/collections/{id}/instant-search", 
    params(IdPath, SearchQuery), 
    responses((status = 200, body = Vec<apexkit_core::models::InstantResult>))
)]
pub async fn instant_search_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    Path(path): Path<IdPath>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<apexkit_core::models::InstantResult>>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    // [UPDATED] Resolve Collection
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let policy = collection
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied by policy".into()));
    }
    let results = db
        .instant_search(collection.id, &params.q, params.limit.unwrap_or(10))
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    Ok(Json(results))
}

// =========================================================
// 3. RELATIONS
// =========================================================

#[utoipa::path(post, path = "/api/v1/collections/{id}/records/{record_id}/relations", request_body = RelationRequest, params(RecordPath), responses((status = 201, description = "Relation created")))]
pub async fn create_relation(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<RecordPath>,
    auth: Option<Extension<Claims>>,
    Json(p): Json<RelationRequest>,
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);

    // [TRIGGER]
    trigger_void_hook(&state, "before_relation_create", json!({ "origin": path.id, "target_col": p.target_collection_id, "relation": p.relation_name }), claims.as_ref(), Some(&event_scope.clone()), Some(base_url.clone())).await?;

    // [FIX] Resolve Collection ID
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    db.create_relation(
        col.id,
        path.record_id,
        p.target_collection_id,
        p.target_record_id,
        &p.relation_name,
    )
    .await
    .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOG]
    let meta = extract_log_meta(
        &headers,
        Some(addr),
        json!({ "relation": p.relation_name, "origin_id": path.record_id, "target_id": p.target_record_id }),
    );
    let _ = db
        .log_audit_event("info", "Relation Created", "api", Some(meta))
        .await;

    // [TRIGGER]
    let _ = trigger_void_hook(
        &state,
        "after_relation_create",
        json!({ "relation": p.relation_name }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await;

    Ok(StatusCode::CREATED)
}

#[utoipa::path(delete, path = "/api/v1/collections/{id}/records/{record_id}/relations", request_body = RelationRequest, params(RecordPath), responses((status = 204, description = "Relation deleted")))]
pub async fn delete_relation(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(path): Path<RecordPath>,
    auth: Option<Extension<Claims>>,
    Json(p): Json<RelationRequest>,
) -> Result<StatusCode, AppError> {
    let claims = auth.map(|Extension(c)| c);

    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);

    trigger_void_hook(
        &state,
        "before_relation_delete",
        json!({ "relation": p.relation_name }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    // [FIX] Resolve Collection ID
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    db.delete_relation(
        col.id,
        path.record_id,
        p.target_collection_id,
        p.target_record_id,
        &p.relation_name,
    )
    .await
    .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOG]
    let meta = extract_log_meta(&headers, Some(addr), json!({ "relation": p.relation_name }));
    let _ = db
        .log_audit_event("info", "Relation Deleted", "api", Some(meta))
        .await;

    let _ = trigger_void_hook(
        &state,
        "after_relation_delete",
        json!({ "relation": p.relation_name }),
        claims.as_ref(),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
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
