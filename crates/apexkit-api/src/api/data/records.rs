use crate::hooks::{trigger_hooks, trigger_void_hook};
use crate::utils::{extract_log_meta, get_tenant_id_from_scope, resolve_collection_by_id_or_name};
use crate::{
    AppError, AppState, BaseUrl, DatabaseConnection, IdPath, RecordListResponse, RecordPath,
    RecordResponse, RelationRequest,
};
use apexkit_core::{
    auth::Claims,
    auth::policies,
    models::schema::{CollectionSchema, FieldType},
    query::QueryOptions,
    realtime::DbEvent,
    realtime::EventScope,
    validation::validate_record,
    workers::Job,
};
use axum::extract::ConnectInfo;
use axum::{
    Extension,
    extract::{Json, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

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
    let rls_sql = policies::compile_to_sql(policy, claims.as_ref(), None, Some(db.clone()))
        .await
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

    // [SECURITY FIX]: Deeply sanitize expanded owner profiles based on the 'expand' query parameter
    if let Some(schema) = &col.schema {
        sanitize_expanded_records(
            &db,
            &mut response_data.items,
            claims.as_ref(),
            schema,
            modified_q.expand.as_ref(),
        )
        .await;
    }

    // [TRIGGER]
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

    // Resolve Collection by ID or Name
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
        .get_record(col.id, path.record_id, q.expand.clone())
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Record not found".into()))?;

    let policy = col
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");
    if !policies::check_access(
        policy,
        claims.as_ref(),
        Some(&r.data),
        None,
        Some(db.clone()),
    )
    .await
    {
        return Err(AppError::Forbidden("Read denied".into()));
    }

    let mut response = RecordResponse {
        id: r.id,
        data: r.data,
        expand: r.expand,
        created: r.created,
        updated: r.updated,
    };

    // [SECURITY FIX]: Deeply sanitize expanded owner profiles
    if let Some(schema) = &col.schema {
        let mut single_item = vec![response];
        sanitize_expanded_records(
            &db,
            &mut single_item,
            claims.as_ref(),
            schema,
            q.expand.as_ref(),
        )
        .await;
        response = single_item.pop().unwrap();
    }

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
            if (def.r#type == FieldType::Relation || def.r#type == FieldType::Owner)
                && let Some(val) = obj.get(name)
                && let Some(num) = val.as_str().and_then(|s| s.parse::<i64>().ok())
            {
                obj.insert(name.clone(), serde_json::json!(num));
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
    let request_data_json = serde_json::to_value(&p.data).ok();
    if !policies::check_access(
        policy,
        claims.as_ref(),
        None,
        request_data_json.as_ref(),
        Some(db.clone()),
    )
    .await
    {
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
    if let Some(schema) = &col.schema
        && schema.fields.values().any(|f| f.ose_indexed)
    {
        let cache_key = format!("{:?}_{}", event_scope, col.id);

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

        state
            .record_count_cache
            .insert(cache_key, current_count)
            .await;

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

            tokio::spawn(async move {
                if let Err(e) = db_clone.reindex_collection(col_id).await {
                    tracing::error!("Auto re-index failed for col {}: {}", col_id, e);
                }
            });
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

        for (field_name, def) in &schema.fields {
            if def.vectorize
                && let Some(content_val) = data_to_save.get(field_name).and_then(|v| v.as_str())
            {
                let c_type = if def.r#type == FieldType::File {
                    "file"
                } else {
                    "text"
                };

                let model_name = crate::utils::get_current_model(c_type);

                let job = Job::GenerateEmbedding {
                    tenant_id: current_tenant.clone(),
                    collection_id: col.id,
                    record_id: rid,
                    field_name: field_name.clone(),
                    content: content_val.to_string(),
                    content_type: c_type.to_string(),
                    model: model_name,
                };
                state.queue.enqueue(job).await;
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
    let request_data_json = serde_json::to_value(&p.data).ok();
    if !policies::check_access(
        policy,
        claims.as_ref(),
        Some(&existing.data),
        request_data_json.as_ref(),
        Some(db.clone()),
    )
    .await
    {
        return Err(AppError::Forbidden("Update denied".into()));
    }

    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);

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

        for (field_name, def) in &schema.fields {
            if def.vectorize {
                if let Some(content_val) = data_updates.get(field_name).and_then(|v| v.as_str()) {
                    let c_type = if def.r#type == FieldType::File {
                        "file"
                    } else {
                        "text"
                    };

                    let model_name = crate::utils::get_current_model(c_type);

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
    if !policies::check_access(
        policy,
        claims.as_ref(),
        Some(&existing.data),
        None,
        Some(db.clone()),
    )
    .await
    {
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

// --- [SECURITY REDESIGN] RECURSIVE SANITIZER FOR EXPANDED RELATION OWNERS ---
use std::future::Future;
use std::pin::Pin;

async fn sanitize_expanded_records(
    db: &Arc<dyn apexkit_core::Db>,
    records: &mut [RecordResponse],
    claims: Option<&Claims>,
    root_schema: &CollectionSchema,
    expand_str: Option<&String>,
) {
    let expand_str = match expand_str {
        Some(s) if !s.trim().is_empty() => s,
        _ => return, // Fast exit if no expand parameter provided
    };

    let expand_tree = apexkit_core::query::builder::build_expand_tree(expand_str);

    let policy_json = db.get_config("policy_users").await.unwrap_or(None);
    let user_read_policy = if let Some(val) = policy_json {
        let parsed = match val {
            serde_json::Value::String(s) => {
                serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
            }
            _ => val,
        };
        serde_json::from_value::<apexkit_core::models::schema::CollectionPolicies>(parsed)
            .map(|p| p.read)
            .unwrap_or_else(|_| "admin || owner:id".to_string())
    } else {
        "admin || owner:id".to_string()
    };

    // Preload collection schemas to avoid sequential DB bottlenecks during tree traversal
    let mut schema_cache = HashMap::new();
    let mut col_map = HashMap::new();
    if let Ok(all_cols) = db.list_collections().await {
        for c in all_cols {
            if let Some(s) = c.schema {
                schema_cache.insert(c.id, s);
            }
            col_map.insert(c.name.clone(), c.id);
        }
    }

    for rec in records {
        if let Some(expand_obj) = &mut rec.expand {
            sanitize_expand_node(
                expand_obj,
                root_schema,
                &expand_tree,
                claims,
                &user_read_policy,
                &schema_cache,
                &col_map,
                db,
            )
            .await;
        }
    }
}

// Recursively walks down the JSON object mirroring the user's `expand` request
fn sanitize_expand_node<'a>(
    expand_obj: &'a mut Value,
    current_schema: &'a CollectionSchema,
    current_tree: &'a HashMap<String, Vec<String>>,
    claims: Option<&'a Claims>,
    user_read_policy: &'a str,
    schema_cache: &'a HashMap<i64, CollectionSchema>,
    col_map: &'a HashMap<String, i64>,
    db: &'a Arc<dyn apexkit_core::Db>,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        let owner_fields: Vec<String> = current_schema
            .fields
            .iter()
            .filter(|(_, def)| def.r#type == FieldType::Owner)
            .map(|(name, _)| name.clone())
            .collect();

        if let Value::Object(obj) = expand_obj {
            // 1. Sanitize direct owner fields at this depth
            for field in &owner_fields {
                if let Some(user_val) = obj.get_mut(field) {
                    if !user_val.is_null()
                        && !policies::check_access(
                            user_read_policy,
                            claims,
                            Some(&*user_val),
                            None,
                            Some(db.clone()),
                        )
                        .await
                    {
                        *user_val = Value::Null; // Denied by policy
                    }
                }
            }

            // 2. Recurse down requested relations
            for (rel_name, sub_paths) in current_tree {
                if let Some(rel_val) = obj.get_mut(rel_name) {
                    let mut target_schema = None;

                    // Match against schema (forward relation) or reverse collection name
                    if let Some(rel_def) = current_schema.relations.get(rel_name) {
                        let target_name = &rel_def.target_collection;
                        let target_id_opt = col_map
                            .get(target_name)
                            .copied()
                            .or_else(|| target_name.parse::<i64>().ok());

                        if let Some(id) = target_id_opt {
                            target_schema = schema_cache.get(&id);
                        }
                    } else if let Some(id) = col_map.get(rel_name) {
                        target_schema = schema_cache.get(id);
                    }

                    if let Some(t_schema) = target_schema {
                        let sub_tree =
                            apexkit_core::query::builder::build_expand_tree(&sub_paths.join(","));

                        if let Value::Array(arr) = rel_val {
                            for item in arr.iter_mut() {
                                if let Some(nested_expand) = item.get_mut("expand") {
                                    sanitize_expand_node(
                                        nested_expand,
                                        t_schema,
                                        &sub_tree,
                                        claims,
                                        user_read_policy,
                                        schema_cache,
                                        col_map,
                                        db,
                                    )
                                    .await;
                                }
                            }
                        } else if let Value::Object(_) = rel_val {
                            if let Some(nested_expand) = rel_val.get_mut("expand") {
                                sanitize_expand_node(
                                    nested_expand,
                                    t_schema,
                                    &sub_tree,
                                    claims,
                                    user_read_policy,
                                    schema_cache,
                                    col_map,
                                    db,
                                )
                                .await;
                            }
                        }
                    }
                }
            }
        }
    })
}
