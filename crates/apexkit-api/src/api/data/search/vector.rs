use crate::BaseUrl;
use crate::{AppError, AppState, DatabaseConnection, IdPath, RecordResponse};
use crate::{
    hooks::trigger_void_hook,
    utils::{extract_log_meta, resolve_collection_by_id_or_name},
};
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
}

#[derive(Deserialize, ToSchema)]
pub struct TextVectorSearchReq {
    pub query_text: String,
    pub limit: Option<usize>,
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
    path = "/api/v1/collections/{id}/search-vector",
    request_body = VectorSearchReq,
    params(IdPath),
    responses((status = 200,body = Vec<RecordResponse>))
)]
pub async fn search_vector(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(_state): State<AppState>,
    Path(path): Path<IdPath>,
    Json(payload): Json<VectorSearchReq>,
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

    let limit = payload.limit.unwrap_or(10).min(100);
    let mut records_with_scores = db
        .search_vector(collection.id, &payload.field, payload.vector, limit)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [FIX]: `score` here is raw L2 distance from the HNSW index - LOWER means MORE similar
    // (0.0 = identical). Sort ASCENDING so the closest match comes first.
    records_with_scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Map tuple (Record,f32) -> RecordResponse and inject _score
    Ok(Json(
        records_with_scores
            .into_iter()
            .map(|(mut r, score)| {
                if let Some(obj) = r.data.as_object_mut() {
                    obj.insert("_score".to_string(), serde_json::json!(score));
                }
                RecordResponse {
                    id: r.id,
                    data: r.data,
                    expand: r.expand,
                    created: r.created,
                    updated: r.updated,
                }
            })
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/search-text-vector",
    request_body = TextVectorSearchReq,
    params(IdPath),
    responses((status = 200,body = Vec<RecordResponse>))
)]
pub async fn query_vector_search(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Path(path): Path<IdPath>,
    Json(payload): Json<TextVectorSearchReq>,
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

    // NOTE: use `embed_query` (the query-side prompt prefix), not `embed`, if your
    // vector_provider distinguishes the two - mixing them up hurts ranking even though
    // nothing here will error.
    let query_vector = state
        .vector_provider
        .embed(&payload.query_text)
        .await
        .map_err(|e| AppError::UnknownError(format!("Embedding generation failed: {}", e)))?;

    let limit = payload.limit.unwrap_or(10).min(100);

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

    // [FIX]: this map now tracks the BEST (lowest) distance seen for each record across
    // all vectorizable fields, not the highest. Start from f32::MAX, not f32::MIN, since
    // "no value yet" must lose to any real distance, never win against it.
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

            // [FIX]: Track the LOWEST distance (= most similar) per record.
            let entry = best_scores.entry(rec.id).or_insert(f32::MAX);
            if distance < *entry {
                *entry = distance;
            }
        }
    }

    let mut sorted_ids: Vec<(i64, f32)> = best_scores.into_iter().collect();
    // [FIX]: Sort ASCENDING - lowest distance (best match) first.
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
    path = "/api/v1/collections/{id}/search-image-vector",
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

    // [FIX]: same min-distance tracking as the text-search handler above.
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

            // [FIX]: Track the LOWEST distance (= most similar) per record.
            let entry = best_scores.entry(rec.id).or_insert(f32::MAX);
            if distance < *entry {
                *entry = distance;
            }
        }
    }

    let mut sorted_ids: Vec<(i64, f32)> = best_scores.into_iter().collect();
    // [FIX]: Sort ASCENDING - lowest distance (best match) first.
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
    path = "/api/v1/admin/collections/{id}/revectorize",
    request_body = RevectorizeOptions,
    params(IdPath),
    responses((status = 202,description = "Revectorization jobs queued"))
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

    let current_tenant = crate::utils::get_tenant_id_from_scope(scope.as_ref().map(|e| &e.0));

    let schema = collection.schema.clone().unwrap_or_default();

    let vectorizable_fields: Vec<String> = schema
        .fields
        .iter()
        .filter(|(_, def)| def.vectorize)
        .map(|(name, _)| name.clone())
        .collect();

    if vectorizable_fields.is_empty() {
        return Ok(Json(serde_json::json!({
            "success": true,
            "message": "No vector fields found."
        })));
    }

    let meta = extract_log_meta(
        &headers,
        Some(addr),
        serde_json::json!({ "collection_id": collection.id,"force": options.force }),
    );
    let _ = db
        .log_audit_event("info", "Revectorization Started", "ai", Some(meta))
        .await;

    let mut query_opts = apexkit_core::query::QueryOptions::default();
    query_opts.limit = Some(100_000);
    query_opts.per_page = None;

    let all_records = db
        .list_records(collection.id, query_opts)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .items;

    let mut total_queued = 0;
    let mut total_skipped = 0;

    // Maps to track counts per model
    let mut model_queued_counts: HashMap<String, usize> = HashMap::new();
    let mut model_skipped_counts: HashMap<String, usize> = HashMap::new();

    for record in all_records {
        let record_id: i64 = record.id;

        for field_name in &vectorizable_fields {
            if let Some(text_content) = record.data.get(field_name).and_then(|v| v.as_str()) {
                // Add field detection logic
                let def = schema.fields.get(field_name).unwrap();
                let c_type = if def.r#type == apexkit_core::models::schema::FieldType::File {
                    "file"
                } else {
                    "text"
                };

                let current_model = crate::utils::get_current_model(c_type);

                if !options.force {
                    let exists = db
                        .has_vector(collection.id, record_id, field_name, &current_model)
                        .await
                        .unwrap_or(false);

                    if exists {
                        total_skipped += 1;
                        *model_skipped_counts
                            .entry(current_model.clone())
                            .or_insert(0) += 1;
                        continue;
                    }
                }

                let job = Job::GenerateEmbedding {
                    tenant_id: current_tenant.clone(),
                    collection_id: collection.id,
                    record_id,
                    field_name: field_name.clone(),
                    content: text_content.to_string(),
                    content_type: c_type.to_string(),
                    model: current_model.clone(), // Pass field-specific model
                };

                state.queue.enqueue(job).await;

                total_queued += 1;
                *model_queued_counts.entry(current_model).or_insert(0) += 1;
            }
        }
    }

    // Construct a detailed human-readable message
    let mut queued_details = Vec::new();
    for (model, count) in &model_queued_counts {
        queued_details.push(format!("{}: {}", model, count));
    }
    let queued_breakdown = if queued_details.is_empty() {
        "".to_string()
    } else {
        format!(" ({})", queued_details.join(", "))
    };

    let message = format!(
        "Queued {} jobs{}. Skipped {} existing.",
        total_queued, queued_breakdown, total_skipped
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": message,
        "jobs_queued": total_queued,
        "skipped": total_skipped,
        "models_queued": model_queued_counts,    // Added explicit breakdown to JSON payload
        "models_skipped": model_skipped_counts,  // Added explicit breakdown to JSON payload
        "mode": if options.force { "hard (overwrite)" } else { "soft (skip existing)" }
    })))
}
