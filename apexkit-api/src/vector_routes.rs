use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde::{ Deserialize };
use apexkit_core::{auth::Claims, jobs::Job}; 
use crate::{AppState, AppError, RecordResponse, DatabaseConnection, IdPath};
use std::collections::HashMap;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct VectorSearchReq {
    pub vector: Vec<f32>,
    pub limit: Option<usize>,
    pub field: String, 
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
    DatabaseConnection(db): DatabaseConnection, // FIXED: Use Extractor
    State(_state): State<AppState>, // Still need State for vector_provider
    Path(path): Path<IdPath>, // FIXED: Use IdPath
    Json(payload): Json<VectorSearchReq>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    // 1. Auth Check (Read Policy)
    let claims = auth.map(|Extension(c)| c);
    let collection = db.get_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;
    
    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !apexkit_core::policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    // 2. Perform Search using DB method which delegates to provider
    // Note: State.vector_provider is the global/root one.
    // If we are in a tenant, the TenantManager has already configured the DB to use the tenant provider.
    // However, `db.search_vector` implementation in `CachedDb` calls `inner.search_vector` -> `ApexKit::search_vector` -> `vector_provider.search`.
    // The `ApexKit` instance inside `CachedDb` was constructed with the correct provider (Global or Tenant).
    // So we just call `db.search_vector`.

    let limit = payload.limit.unwrap_or(10).min(100);
    let records = db.search_vector(path.id, &payload.field, payload.vector, limit)
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
    DatabaseConnection(db): DatabaseConnection, // FIXED
    State(state): State<AppState>,
    Path(path): Path<IdPath>, // FIXED
    Json(payload): Json<TextVectorSearchReq>,
) -> Result<Json<Vec<RecordResponse>>, AppError> {
    // 1. Auth Check (Read Policy)
    let claims = auth.map(|Extension(c)| c);
    let collection = db.get_collection(path.id).await.map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;
    
    let policy = collection.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
    if !apexkit_core::policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    // 2. Generate Vector from Query Text
    // This uses the GLOBAL state.embedder because embedding logic is stateless/heavy and shared.
    // TenantManager passes the SAME embedder instance to tenants anyway.
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

    // We can't access `vector_provider` directly from `db` because `db` is `Arc<dyn Db>`.
    // But `ApexKit` implements `search_vector`.
    // We will use `db.search_vector` for each field.

    for field_name in vectorizable_fields {
        // Search the HNSW index
        let records = db.search_vector(path.id, &field_name, query_vector.clone(), limit).await
            .map_err(|e| AppError::UnknownError(format!("Vector search failed for {}: {}", field_name, e)))?;

        // Aggregate scores (Dummy score for now since search_vector returns Records, not scores+ids)
        // If exact scoring is needed, `Db` trait needs update to return scores. 
        // For now, we just merge results.
        for rec in records {
            // Simple accumulation: If it appears in multiple fields, it's more relevant
             *record_scores.entry(rec.id).or_insert(0.0) += 1.0; 
        }
    }
    
    // 5. Get top N aggregated records
    let mut sorted_records: Vec<(i64, f32)> = record_scores.into_iter().collect();
    sorted_records.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    let top_ids: Vec<i64> = sorted_records.into_iter()
        .take(limit)
        .map(|(id, _)| id)
        .collect();

    // 6. Fetch Records from DB
    let records = db.get_records_by_ids(path.id, &top_ids).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(records.into_iter().map(|r| RecordResponse { id: r.id, data: r.data }).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/collections/{id}/revectorize",
    responses((status = 202, description = "Revectorization jobs queued"))
)]
pub async fn revectorize_collection_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection, // FIXED
    State(state): State<AppState>,
    Path(path): Path<IdPath>, // FIXED
) -> Result<Json<serde_json::Value>, AppError> {
    // 1. Auth Check (Admins Only)
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { 
        return Err(AppError::Forbidden("Admins only".into())); 
    }

    // 2. Get Collection Schema
    let collection = db.get_collection(path.id).await
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
    let mut options = apexkit_core::query::QueryOptions::default();
    options.limit = None; 
    options.per_page = None;
    
    let all_records = db.list_records(path.id, options).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?.items;

    let mut jobs_queued = 0;
    
    // 5. Queue Jobs
    for record in all_records {
        let record_id: i64 = record.id; 

        for field_name in &vectorizable_fields {
            if let Some(text_content) = record.data.get(field_name).and_then(|v| v.as_str()) {
                let job = Job::GenerateEmbedding {
                    collection_id: path.id,
                    record_id,
                    field_name: field_name.clone(),
                    text_content: text_content.to_string()
                };
                // NOTE: Queue is global, but the Job Handler will need to know WHICH TENANT context.
                // Currently `Job::GenerateEmbedding` doesn't carry tenant_id.
                // This means background jobs will fail for Tenants unless updated.
                // For now, this works for Root App. 
                // To fix for tenants, Job struct and Worker need updates (out of scope for this snippet request).
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