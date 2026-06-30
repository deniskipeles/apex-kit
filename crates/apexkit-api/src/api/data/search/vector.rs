use crate::{AppError, AppState, DatabaseConnection, IdPath, RecordResponse};
use crate::{BaseUrl, RecordListResponse};
use crate::{
    hooks::trigger_void_hook,
    utils::{extract_log_meta, resolve_collection_by_id_or_name},
};
use apexkit_core::query::QueryOptions;
use apexkit_core::realtime::EventScope;
use apexkit_core::{auth::Claims, models::VectorRecord, workers::Job};
use axum::extract::ConnectInfo;
use axum::{
    Extension,
    extract::{Json, Path, State},
};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use utoipa::{IntoParams, ToSchema};

#[derive(Deserialize, ToSchema)]
pub struct VectorSearchReq {
    pub vector: Vec<f32>,
    pub limit: Option<usize>,
    pub field: String,
    pub expand: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
pub struct TextVectorSearchReq {
    pub query_text: String,
    pub limit: Option<usize>,
    pub expand: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
pub struct RevectorizeOptions {
    #[serde(default)]
    pub force: bool,
}

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct RecordVectorPath {
    pub id: String,     // Collection ID/Name
    pub record_id: i64, // Record ID
}

#[derive(Deserialize, ToSchema)]
pub struct ImageVectorSearchReq {
    /// Base64 encoded image (data:image/png;base64,...)
    pub image_data: String,
    pub limit: Option<usize>,
}

#[derive(Deserialize, ToSchema)]
pub struct TextImageVectorSearchReq {
    pub query_text: String,
    pub limit: Option<usize>,
}

//  Handler: Get Vector
#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/get-vector/{record_id}",
    params(RecordVectorPath),
    responses((status = 200,body = Vec<VectorRecord>))
)]
pub async fn get_record_vector(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    Path(path): Path<RecordVectorPath>,
) -> Result<Json<Vec<VectorRecord>>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;
    let policy = collection
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");

    let access_granted = if policy == "public" {
        true
    } else if policy == "admin" {
        claims.as_ref().map(|c| c.role == "admin").unwrap_or(false)
    } else if policy == "auth" {
        claims.is_some()
    } else {
        let rec = db
            .get_record(collection.id, path.record_id, None)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?
            .ok_or(AppError::NotFound("Record not found".into()))?;

        apexkit_core::auth::policies::check_access(policy, claims.as_ref(), Some(&rec.data))
    };

    if !access_granted {
        return Err(AppError::Forbidden("Read denied".into()));
    }

    let vectors = db
        .get_record_vectors(collection.id, path.record_id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(vectors))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/search-vector-with-vector",
    request_body = VectorSearchReq,
    params(IdPath),
    responses((status = 200,body = RecordListResponse))
)]
pub async fn query_vector_with_vector(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(_state): State<AppState>,
    Path(path): Path<IdPath>,
    Json(payload): Json<VectorSearchReq>,
) -> Result<Json<RecordListResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let policy = collection
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");
    if !apexkit_core::auth::policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    let search_limit = payload.limit.unwrap_or(1000).min(1000);
    let mut records_with_scores = db
        .search_vector(collection.id, &payload.field, payload.vector, search_limit)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    records_with_scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let total = records_with_scores.len() as i64;
    let page = payload.page.unwrap_or(1).max(1);
    let per_page = payload.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let paginated_results: Vec<(apexkit_core::models::Record, f32)> = records_with_scores
        .into_iter()
        .skip(offset as usize)
        .take(per_page as usize)
        .collect();

    let mut final_records = Vec::new();
    let ids: Vec<i64> = paginated_results.iter().map(|(r, _)| r.id).collect();
    let mut scores_map: std::collections::HashMap<i64, f32> = paginated_results
        .into_iter()
        .map(|(r, score)| (r.id, score))
        .collect();

    if !ids.is_empty() {
        let options = QueryOptions {
            limit: Some(ids.len() as u64),
            filter: Some(serde_json::json!({ "id": { "$in": ids } }).to_string()),
            expand: payload.expand.clone(),
            ..Default::default()
        };
        let list_res = db
            .list_records(collection.id, options)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let mut item_map: std::collections::HashMap<i64, apexkit_core::models::Record> =
            list_res.items.into_iter().map(|r| (r.id, r)).collect();

        final_records = ids
            .into_iter()
            .filter_map(|id| {
                if let Some(mut r) = item_map.remove(&id) {
                    let score = scores_map.remove(&id).unwrap_or(0.0);
                    if let Some(obj) = r.data.as_object_mut() {
                        obj.insert("_score".to_string(), serde_json::json!(score));
                    }
                    Some(r)
                } else {
                    None
                }
            })
            .collect();
    }

    Ok(Json(RecordListResponse {
        items: final_records
            .into_iter()
            .map(|r| RecordResponse {
                id: r.id,
                data: r.data,
                expand: r.expand,
                created: r.created,
                updated: r.updated,
            })
            .collect(),
        total,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/search-vector-with-text",
    request_body = TextVectorSearchReq,
    params(IdPath),
    responses((status = 200,body = RecordListResponse))
)]
pub async fn query_vector_with_text(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Path(path): Path<IdPath>,
    Json(payload): Json<TextVectorSearchReq>,
) -> Result<Json<RecordListResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let policy = collection
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");
    if !apexkit_core::auth::policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    let query_vector = state
        .vector_provider
        .embed(&payload.query_text)
        .await
        .map_err(|e| AppError::UnknownError(format!("Embedding generation failed: {}", e)))?;

    let search_limit = payload.limit.unwrap_or(1000).min(1000);

    let vectorizable_fields: Vec<String> = collection
        .schema
        .as_ref()
        .unwrap_or(&Default::default())
        .fields
        .iter()
        .filter(|(_, def)| def.vectorize)
        .map(|(name, _)| name.clone())
        .collect();

    if vectorizable_fields.is_empty() {
        return Err(AppError::NotFound(
            "No vectorizable fields found for this collection.".into(),
        ));
    }

    let mut best_scores: HashMap<i64, f32> = HashMap::new();

    for field_name in vectorizable_fields {
        let records_with_scores = db
            .search_vector(
                collection.id,
                &field_name,
                query_vector.clone(),
                search_limit,
            )
            .await
            .map_err(|e| {
                AppError::UnknownError(format!("Vector search failed for {}: {}", field_name, e))
            })?;

        for (rec, distance) in records_with_scores {
            let entry = best_scores.entry(rec.id).or_insert(f32::MAX);
            if distance < *entry {
                *entry = distance;
            }
        }
    }

    let mut sorted_ids: Vec<(i64, f32)> = best_scores.into_iter().collect();
    sorted_ids.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let total = sorted_ids.len() as i64;
    let page = payload.page.unwrap_or(1).max(1);
    let per_page = payload.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let paginated_ids: Vec<(i64, f32)> = sorted_ids
        .into_iter()
        .skip(offset as usize)
        .take(per_page as usize)
        .collect();

    let mut final_records = Vec::new();
    let ids: Vec<i64> = paginated_ids.iter().map(|(id, _)| *id).collect();

    if !ids.is_empty() {
        let mut scores_map: std::collections::HashMap<i64, f32> =
            paginated_ids.into_iter().collect();
        let options = QueryOptions {
            limit: Some(ids.len() as u64),
            filter: Some(serde_json::json!({ "id": { "$in": ids } }).to_string()),
            expand: payload.expand.clone(),
            ..Default::default()
        };
        let list_res = db
            .list_records(collection.id, options)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let mut item_map: std::collections::HashMap<i64, apexkit_core::models::Record> =
            list_res.items.into_iter().map(|r| (r.id, r)).collect();

        final_records = ids
            .into_iter()
            .filter_map(|id| {
                if let Some(mut r) = item_map.remove(&id) {
                    let score = scores_map.remove(&id).unwrap_or(0.0);
                    if let Some(obj) = r.data.as_object_mut() {
                        obj.insert("_score".to_string(), serde_json::json!(score));
                    }
                    Some(r)
                } else {
                    None
                }
            })
            .collect();
    }

    Ok(Json(RecordListResponse {
        items: final_records
            .into_iter()
            .map(|r| RecordResponse {
                id: r.id,
                data: r.data,
                expand: r.expand,
                created: r.created,
                updated: r.updated,
            })
            .collect(),
        total,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/search-image-vector-with-image",
    request_body = ImageVectorSearchReq,
    params(IdPath),
    responses((status = 200,body = Vec<RecordResponse>))
)]
pub async fn query_image_vector_search(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Path(path): Path<IdPath>,
    Json(payload): Json<ImageVectorSearchReq>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let policy = collection
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");
    if !apexkit_core::auth::policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    let query_vector = state
        .vector_provider
        .embed_image(&payload.image_data)
        .await
        .map_err(|e| AppError::UnknownError(format!("Image Embedding failed: {}", e)))?;

    let limit = payload.limit.unwrap_or(10).min(100);

    let vectorizable_fields: Vec<String> = collection
        .schema
        .as_ref()
        .unwrap_or(&Default::default())
        .fields
        .iter()
        .filter(|(_, def)| {
            def.vectorize && def.r#type == apexkit_core::models::schema::FieldType::File
        })
        .map(|(name, _)| name.clone())
        .collect();

    if vectorizable_fields.is_empty() {
        return Err(AppError::NotFound(
            "No vectorizable File fields found for this collection.".into(),
        ));
    }

    let mut best_scores: HashMap<i64, f32> = HashMap::new();
    let mut id_to_record: HashMap<i64, apexkit_core::models::Record> = HashMap::new();

    for field_name in vectorizable_fields {
        let records_with_scores = db
            .search_vector(collection.id, &field_name, query_vector.clone(), limit)
            .await
            .map_err(|e| {
                AppError::UnknownError(format!("Vector search failed for {}: {}", field_name, e))
            })?;

        for (rec, distance) in records_with_scores {
            id_to_record.insert(rec.id, rec.clone());

            let entry = best_scores.entry(rec.id).or_insert(f32::MAX);
            if distance < *entry {
                *entry = distance;
            }
        }
    }

    let mut sorted_ids: Vec<(i64, f32)> = best_scores.into_iter().collect();
    sorted_ids.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let final_records: Vec<RecordResponse> = sorted_ids
        .into_iter()
        .take(limit)
        .filter_map(|(id, score)| {
            if let Some(mut r) = id_to_record.remove(&id) {
                // Inject the score so it's visible in the API/UI
                if let Some(obj) = r.data.as_object_mut() {
                    obj.insert("_score".to_string(), serde_json::json!(score));
                }
                Some(RecordResponse {
                    id: r.id,
                    data: r.data,
                    expand: r.expand,
                    created: r.created,
                    updated: r.updated,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(Json(final_records))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/search-image-vector-with-text",
    request_body = TextImageVectorSearchReq,
    params(IdPath),
    responses((status = 200,body = Vec<RecordResponse>))
)]
pub async fn query_text_image_vector_search(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Path(path): Path<IdPath>,
    Json(payload): Json<TextImageVectorSearchReq>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let policy = collection
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");
    if !apexkit_core::auth::policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    let mut clean_query = payload.query_text.to_lowercase();
    if !clean_query.ends_with('.') {
        clean_query.push('.');
    }

    let query_vector = state
        .vector_provider
        .embed_text_for_image_search(&clean_query)
        .await
        .map_err(|e| AppError::UnknownError(format!("Text-image embedding failed: {}", e)))?;

    let limit = payload.limit.unwrap_or(10).min(100);

    let vectorizable_fields: Vec<String> = collection
        .schema
        .as_ref()
        .unwrap_or(&Default::default())
        .fields
        .iter()
        .filter(|(_, def)| {
            def.vectorize && def.r#type == apexkit_core::models::schema::FieldType::File
        })
        .map(|(name, _)| name.clone())
        .collect();

    if vectorizable_fields.is_empty() {
        return Err(AppError::NotFound(
            "No vectorizable File fields found for this collection.".into(),
        ));
    }

    let mut best_scores: HashMap<i64, f32> = HashMap::new();
    let mut id_to_record: HashMap<i64, apexkit_core::models::Record> = HashMap::new();

    for field_name in vectorizable_fields {
        let records_with_scores = db
            .search_vector(collection.id, &field_name, query_vector.clone(), limit)
            .await
            .map_err(|e| {
                AppError::UnknownError(format!("Vector search failed for {}: {}", field_name, e))
            })?;

        for (rec, distance) in records_with_scores {
            id_to_record.insert(rec.id, rec.clone());

            let entry = best_scores.entry(rec.id).or_insert(f32::MAX);
            if distance < *entry {
                *entry = distance;
            }
        }
    }

    let mut sorted_ids: Vec<(i64, f32)> = best_scores.into_iter().collect();
    sorted_ids.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let final_records: Vec<RecordResponse> = sorted_ids
        .into_iter()
        .take(limit)
        .filter_map(|(id, score)| {
            if let Some(mut r) = id_to_record.remove(&id) {
                // Inject the score so it's visible in the API/UI
                if let Some(obj) = r.data.as_object_mut() {
                    obj.insert("_score".to_string(), serde_json::json!(score));
                }
                Some(RecordResponse {
                    id: r.id,
                    data: r.data,
                    expand: r.expand,
                    created: r.created,
                    updated: r.updated,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(Json(final_records))
}

// [FIX] Extracts bulk operation to worker queue
#[utoipa::path(
    post,
    path = "/api/v1/admin/collections/{id}/revectorize",
    request_body = RevectorizeOptions,
    params(IdPath),
    responses((status = 202,description = "Revectorization job queued"))
)]
pub async fn revectorize_collection_handler(
    auth: Option<Extension<Claims>>,
    scope: Option<Extension<EventScope>>,
    BaseUrl(base_url): BaseUrl,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(path): Path<IdPath>,
    Json(options): Json<RevectorizeOptions>,
) -> Result<Json<serde_json::Value>, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.clone().map(|s| s.0).unwrap_or(EventScope::Root);
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;

    trigger_void_hook(
        &state,
        "on_vectorization_start",
        serde_json::json!({ "collection_id": collection.id }),
        Some(&claims),
        Some(&event_scope.clone()),
        Some(base_url.clone()),
    )
    .await?;

    let meta = extract_log_meta(
        &headers,
        Some(addr),
        serde_json::json!({ "collection_id": collection.id,"force": options.force }),
    );
    let _ = db
        .log_audit_event("info", "Revectorization Job Queued", "ai", Some(meta))
        .await;

    let current_tenant = crate::utils::get_tenant_id_from_scope(scope.as_ref().map(|e| &e.0));

    // Queue the bulk job. The worker will handle pagination and execution.
    state
        .queue
        .enqueue(Job::RevectorizeCollection {
            tenant_id: current_tenant,
            collection_id: collection.id,
            force: options.force,
        })
        .await;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Revectorization job successfully queued in background.",
    })))
}
