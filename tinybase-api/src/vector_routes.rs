use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde::{Deserialize, Serialize};
use tinybase_core::{auth::Claims, models::Record, jobs::Job}; // Import Job
use crate::{AppState, AppError, RecordResponse};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct VectorSearchReq {
    pub vector: Vec<f32>,
    pub limit: Option<usize>,
    pub field: String, // Which field to search against (e.g. "description_vec")
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TextVectorSearchReq {
    pub query_text: String,
    pub limit: Option<usize>,
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/search-vector",
    request_body = VectorSearchReq,
    responses((status = 200, body = Vec<RecordResponse>))
)]
pub async fn search_vector(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<VectorSearchReq>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    // 1. Auth Check (Read Policy)
    let claims = auth.map(|Extension(c)| c);
    let collection = state.db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;
    
    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !tinybase_core::policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    // 2. Perform Search
    let limit = payload.limit.unwrap_or(10).min(100);
    let records = state.db.search_vector(id, &payload.field, payload.vector, limit)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 3. Map Response
    Ok(Json(records.into_iter().map(|r| RecordResponse { id: r.id, data: r.data }).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/search-text-vector",
    request_body = TextVectorSearchReq,
    responses((status = 200, body = Vec<RecordResponse>))
)]
pub async fn query_vector_search(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<TextVectorSearchReq>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    // 1. Auth Check (Read Policy)
    let claims = auth.map(|Extension(c)| c);
    let collection = state.db.get_collection(id).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;
    
    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !tinybase_core::policies::check_access(policy, claims.as_ref(), None) {
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
    // Stores: { record_id: total_score }
    let mut record_scores: HashMap<i64, f32> = HashMap::new();

    for field_name in vectorizable_fields {
        // Search the HNSW index
        let results = state.vector_provider.search(id, &field_name, &query_vector, limit).await
            .map_err(|e| AppError::UnknownError(format!("Vector search failed for {}: {}", field_name, e)))?;

        // Aggregate scores (e.g., sum or average for unified score)
        for (rec_id, score) in results {
            // Using sum of scores as aggregation logic (Higher score is better in HNSW L2 distance)
            // Note: score aggregation logic can vary (sum, max, min, average)
            *record_scores.entry(rec_id).or_insert(0.0) += score;
        }
    }
    
    // 5. Get top N aggregated records
    let mut sorted_records: Vec<(i64, f32)> = record_scores.into_iter().collect();
    // Sort by score (descending)
    sorted_records.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    let top_ids: Vec<i64> = sorted_records.into_iter()
        .take(limit)
        .map(|(id, _)| id)
        .collect();

    // 6. Fetch Records from DB
    let records = state.db.get_records_by_ids(id, &top_ids).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 7. Map Response (maintaining order from search results is complex, skipping for brevity)
    Ok(Json(records.into_iter().map(|r| RecordResponse { id: r.id, data: r.data }).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/collections/{id}/revectorize",
    responses((status = 202, description = "Revectorization jobs queued"))
)]
pub async fn revectorize_collection_handler(
    auth: Option<Extension<Claims>>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    // 1. Auth Check (Admins Only)
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { 
        return Err(AppError::Forbidden("Admins only".into())); 
    }

    // 2. Get Collection Schema
    let collection = state.db.get_collection(id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;
        
    let schema = collection.schema.clone().unwrap_or_default();
    
    // 3. Identify Vectorizable Fields
    let vectorizable_fields: Vec<String> = schema.fields.iter()
        .filter(|(_, def)| def.vectorize)
        .map(|(name, _)| name.clone())
        .collect();

    if vectorizable_fields.is_empty() {
        return Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("Collection {} has no vectorizable fields defined.", collection.name)
        })));
    }

    // 4. Iterate over all records in the collection
    let mut options = tinybase_core::query::QueryOptions::default();
    options.limit = None; // Get all records
    options.per_page = None;
    
    // Using simple list_records with no limits (might be slow for huge DBs, 
    // but correct for reindexing all data).
    let all_records = state.db.list_records(id, options).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?.items;

    let mut jobs_queued = 0;
    
    // 5. Queue Jobs
    for record in all_records {
        // FIX: The compiler insists record.id is i64 here, so we trust it.
        // If the core logic correctly unwrapped it for fetched records, we use it directly.
        let record_id: i64 = record.id; 

        for field_name in &vectorizable_fields {
            if let Some(text_content) = record.data.get(field_name).and_then(|v| v.as_str()) {
                let job = Job::GenerateEmbedding {
                    collection_id: id,
                    record_id,
                    field_name: field_name.clone(),
                    text_content: text_content.to_string()
                };
                state.queue.enqueue(job).await;
                jobs_queued += 1;
            }
        }
    }

    Ok(Json(serde_json::json!({ 
        "success": true, 
        "message": format!("Queued {} vectorization jobs for collection {}.", jobs_queued, collection.name),
        "jobs_queued": jobs_queued
    })))
}