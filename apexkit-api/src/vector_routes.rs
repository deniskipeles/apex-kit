use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde::{ Deserialize };
use apexkit_core::{auth::Claims, jobs::Job, models::VectorRecord}; 
use crate::{AppState, AppError, RecordResponse, DatabaseConnection, IdPath, resolve_collection_by_id_or_name};
use std::collections::HashMap;
use apexkit_core::realtime::EventScope;
use axum::extract::ConnectInfo;
use std::net::SocketAddr;
use crate::{trigger_void_hook, extract_log_meta};
use crate::BaseUrl;
use utoipa::{ToSchema, IntoParams};

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
    pub id: String,        // Collection ID/Name
    pub record_id: i64,    // Record ID
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
    responses((status = 200, body = Vec<VectorRecord>))
)]
pub async fn get_record_vector(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection, 
    Path(path): Path<RecordVectorPath>,
) -> Result<Json<Vec<VectorRecord>>, AppError> {
    // 1. Auth Check
    let claims = auth.map(|Extension(c)| c);
    
    // 2. Resolve Collection
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;
    
    // 3. Check Policy (Read access is required to see vectors)
    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    
    // We need to fetch the record data first to verify 'owner' policy if needed
    // However, for efficiency, if policy is public/admin/auth we can skip fetching data.
    // If it's complex (owner-based), we fetch.
    let access_granted = if policy == "public" {
        true
    } else if policy == "admin" {
        claims.as_ref().map(|c| c.role == "admin").unwrap_or(false)
    } else if policy == "auth" {
        claims.is_some()
    } else {
        // Complex policy: fetch record to verify
        let rec = db.get_record(collection.id, path.record_id, None).await
            .map_err(|e| AppError::UnknownError(e.to_string()))?
            .ok_or(AppError::NotFound("Record not found".into()))?;
            
        apexkit_core::policies::check_access(policy, claims.as_ref(), Some(&rec.data))
    };

    if !access_granted {
        return Err(AppError::Forbidden("Read denied".into()));
    }

    // 4. Fetch Vectors
    let vectors = db.get_record_vectors(collection.id, path.record_id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(vectors))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/search-vector",
    request_body = VectorSearchReq,
    params(IdPath),
    responses((status = 200, body = Vec<RecordResponse>))
)]
pub async fn search_vector(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection, 
    State(_state): State<AppState>, 
    Path(path): Path<IdPath>, 
    Json(payload): Json<VectorSearchReq>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    // 1. Auth Check (Read Policy)
    let claims = auth.map(|Extension(c)| c);
    
    // [FIX] Resolve ID
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;
    
    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !apexkit_core::policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    // 2. Perform Search 
    let limit = payload.limit.unwrap_or(10).min(100);
    // [FIX] Use collection.id
    let records = db.search_vector(collection.id, &payload.field, payload.vector, limit)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(records.into_iter().map(|r| RecordResponse { id: r.id, data: r.data, expand: r.expand, created: r.created, updated: r.updated }).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/search-text-vector",
    request_body = TextVectorSearchReq,
    params(IdPath),
    responses((status = 200, body = Vec<RecordResponse>))
)]
pub async fn query_vector_search(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection, 
    State(state): State<AppState>,
    Path(path): Path<IdPath>, 
    Json(payload): Json<TextVectorSearchReq>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    // 1. Auth Check (Read Policy)
    let claims = auth.map(|Extension(c)| c);
    
    // [FIX] Resolve ID
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;
    
    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !apexkit_core::policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    // 2. Generate Vector from Query Text
    let query_vector = state.vector_provider.embed(&payload.query_text).await
        .map_err(|e| AppError::UnknownError(format!("Embedding generation failed: {}", e)))?;
        
    let limit = payload.limit.unwrap_or(10).min(100);

    // 3. Identify all vectorizable fields to search against
    let vectorizable_fields: Vec<String> = collection.schema.as_ref().unwrap_or(&Default::default()).fields.iter()
        .filter(|(_, def)| def.vectorize)
        .map(|(name, _)| name.clone())
        .collect();
        
    if vectorizable_fields.is_empty() {
         return Err(AppError::NotFound("No vectorizable fields found for this collection.".into()));
    }
    
    // 4. Perform Search for *each* vectorizable field and aggregate scores
    let mut record_scores: HashMap<i64, f32> = HashMap::new();

    for field_name in vectorizable_fields {
        // [FIX] Use collection.id
        let records = db.search_vector(collection.id, &field_name, query_vector.clone(), limit).await
            .map_err(|e| AppError::UnknownError(format!("Vector search failed for {}: {}", field_name, e)))?;

        for rec in records {
             *record_scores.entry(rec.id).or_insert(0.0) += 1.0; 
        }
    }
    
    // 5. Get top N aggregated records
    let mut sorted_records_tuples: Vec<(i64, f32)> = record_scores.iter().map(|(&k, &v)| (k, v)).collect();
    sorted_records_tuples.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    let top_ids: Vec<i64> = sorted_records_tuples.iter()
        .take(limit)
        .map(|(id, _)| *id)
        .collect();

    // 6. Fetch Records from DB
    // [FIX] Use collection.id
    let mut records = db.get_records_by_ids(collection.id, &top_ids).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 7. Re-Sort Records
    records.sort_by(|a, b| {
        let score_a = record_scores.get(&a.id).unwrap_or(&0.0);
        let score_b = record_scores.get(&b.id).unwrap_or(&0.0);
        score_b.partial_cmp(score_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(Json(records.into_iter().map(|r| RecordResponse { id: r.id, data: r.data, expand: r.expand, created: r.created, updated: r.updated }).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/collections/{id}/revectorize",
    request_body = RevectorizeOptions, 
    params(IdPath),
    responses((status = 202, description = "Revectorization jobs queued"))
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
    
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let event_scope = scope.clone().map(|s| s.0).unwrap_or(EventScope::Root);
    
    // [FIX] Resolve ID
    let collection = resolve_collection_by_id_or_name(&db, &path.id).await?;

    // [TRIGGER]
    trigger_void_hook(&state, "on_vectorization_start", serde_json::json!({ "collection_id": collection.id }), Some(&claims),  Some(&event_scope.clone()), Some(base_url.clone())).await?;

    let current_tenant = crate::get_tenant_id_from_scope(scope.as_ref().map(|e| &e.0));
    let current_model = crate::get_current_model();

    let schema = collection.schema.clone().unwrap_or_default();
    
    let vectorizable_fields: Vec<String> = schema.fields.iter()
        .filter(|(_, def)| def.vectorize)
        .map(|(name, _)| name.clone())
        .collect();

    if vectorizable_fields.is_empty() {
        return Ok(Json(serde_json::json!({
            "success": true,
            "message": "No vector fields found."
        })));
    }

    // [LOG]
    let meta = extract_log_meta(&headers, Some(addr), serde_json::json!({ "collection_id": collection.id, "force": options.force }));
    let _ = db.log_audit_event("info", "Revectorization Started", "ai", Some(meta)).await;

    let mut query_opts = apexkit_core::query::QueryOptions::default();
    query_opts.limit = Some(100_000); 
    query_opts.per_page = None;
    
    // [FIX] Use collection.id
    let all_records = db.list_records(collection.id, query_opts).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?.items;

    let mut jobs_queued = 0;
    let mut skipped = 0;
    
    for record in all_records {
        let record_id: i64 = record.id; 

        for field_name in &vectorizable_fields {
            if let Some(text_content) = record.data.get(field_name).and_then(|v| v.as_str()) {
                if !options.force {
                    // [FIX] Use collection.id
                    let exists = db.has_vector(collection.id, record_id, field_name, &current_model).await
                        .unwrap_or(false);
                        
                    if exists {
                        skipped += 1;
                        continue;
                    }
                }

                let job = Job::GenerateEmbedding {
                    tenant_id: current_tenant.clone(),
                    collection_id: collection.id, 
                    record_id,
                    field_name: field_name.clone(),
                    content: text_content.to_string(), 
                    content_type: "text".to_string(),
                    model: current_model.clone()
                };
                state.queue.enqueue(job).await;
                jobs_queued += 1;
            }
        }
    }
    Ok(Json(serde_json::json!({ 
        "success": true, 
        "message": format!("Queued {} jobs for model '{}'. Skipped {} existing.", jobs_queued, current_model, skipped),
        "jobs_queued": jobs_queued,
        "skipped": skipped,
        "mode": if options.force { "hard (overwrite)" } else { "soft (skip existing)" }
    })))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/search-image-vector",
    request_body = ImageVectorSearchReq,
    params(IdPath),
    responses((status = 200, body = Vec<RecordResponse>))
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
    
    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !apexkit_core::policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    // 1. Generate Vector from Uploaded Image
    let query_vector = state.vector_provider.embed_image(&payload.image_data).await
        .map_err(|e| AppError::UnknownError(format!("Image Embedding failed: {}", e)))?;
        
    let limit = payload.limit.unwrap_or(10).min(100);

    // 2. Identify all vectorizable FILE fields
    // We only want to search against fields that are actually images/files to compare apples to apples
    let vectorizable_fields: Vec<String> = collection.schema.as_ref().unwrap_or(&Default::default()).fields.iter()
        .filter(|(_, def)| def.vectorize && def.r#type == apexkit_core::schema::FieldType::File)
        .map(|(name, _)| name.clone())
        .collect();
        
    if vectorizable_fields.is_empty() {
         return Err(AppError::NotFound("No vectorizable File fields found for this collection.".into()));
    }
    
    // 3. Perform Search for each field and aggregate scores
    let mut record_scores: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();

    for field_name in vectorizable_fields {
        let records = db.search_vector(collection.id, &field_name, query_vector.clone(), limit).await
            .map_err(|e| AppError::UnknownError(format!("Vector search failed for {}: {}", field_name, e)))?;

        for rec in records {
             *record_scores.entry(rec.id).or_insert(0.0) += 1.0; 
        }
    }
    
    // 4. Sort and Fetch (Same logic as text search)
    let mut sorted_records_tuples: Vec<(i64, f32)> = record_scores.iter().map(|(&k, &v)| (k, v)).collect();
    sorted_records_tuples.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    let top_ids: Vec<i64> = sorted_records_tuples.iter().take(limit).map(|(id, _)| *id).collect();

    let mut records = db.get_records_by_ids(collection.id, &top_ids).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    records.sort_by(|a, b| {
        let score_a = record_scores.get(&a.id).unwrap_or(&0.0);
        let score_b = record_scores.get(&b.id).unwrap_or(&0.0);
        score_b.partial_cmp(score_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(Json(records.into_iter().map(|r| RecordResponse { id: r.id, data: r.data, expand: r.expand, created: r.created, updated: r.updated }).collect()))
}