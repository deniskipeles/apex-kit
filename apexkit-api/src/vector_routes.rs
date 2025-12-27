use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde::{ Deserialize };
use apexkit_core::{auth::Claims, jobs::Job}; 
use crate::{AppState, AppError, RecordResponse, DatabaseConnection, IdPath};
use std::collections::HashMap;
use apexkit_core::realtime::EventScope;

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

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RevectorizeOptions {
    /// If true, overwrites existing vectors for this model (Hard). 
    /// If false, skips records that already have a vector for this model (Soft).
    #[serde(default)]
    pub force: bool,
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
    Ok(Json(records.into_iter().map(|r| RecordResponse { id: r.id, data: r.data, expand: r.expand }).collect()))
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
        // Search the HNSW index
        let records = db.search_vector(path.id, &field_name, query_vector.clone(), limit).await
            .map_err(|e| AppError::UnknownError(format!("Vector search failed for {}: {}", field_name, e)))?;

        // Aggregate scores 
        // Note: Currently simple accumulation. Ideally search_vector should return (id, score) tuples.
        for rec in records {
             *record_scores.entry(rec.id).or_insert(0.0) += 1.0; 
        }
    }
    
    // 5. Get top N aggregated records
    let mut sorted_records_tuples: Vec<(i64, f32)> = record_scores.iter().map(|(&k, &v)| (k, v)).collect();
    // Sort Descending (Highest score first)
    sorted_records_tuples.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    let top_ids: Vec<i64> = sorted_records_tuples.iter()
        .take(limit)
        .map(|(id, _)| *id)
        .collect();

    // 6. Fetch Records from DB
    // Note: SQL `WHERE id IN (...)` does NOT guarantee order, so we must sort again manually.
    let mut records = db.get_records_by_ids(path.id, &top_ids).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 7. Re-Sort Records by Score (Descending)
    // We use the `record_scores` map to look up the score for each record and sort.
    records.sort_by(|a, b| {
        let score_a = record_scores.get(&a.id).unwrap_or(&0.0);
        let score_b = record_scores.get(&b.id).unwrap_or(&0.0);
        // Compare B to A for descending order
        score_b.partial_cmp(score_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(Json(records.into_iter().map(|r| RecordResponse { id: r.id, data: r.data, expand: r.expand }).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/collections/{id}/revectorize",
    request_body = RevectorizeOptions, // Update docs
    responses((status = 202, description = "Revectorization jobs queued"))
)]
pub async fn revectorize_collection_handler(
    auth: Option<Extension<Claims>>,
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection, 
    State(state): State<AppState>,
    Path(path): Path<IdPath>, 
    // Accept JSON body for options, use default if missing
    Json(options): Json<RevectorizeOptions>, 
) -> Result<Json<serde_json::Value>, AppError> {
    
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // Helper logic to extract tenant ID (Duplicated here to avoid visibility issues across modules)
    let current_tenant = scope.as_ref().map(|Extension(s)| match s {
        EventScope::Tenant(id) => Some(id.clone()),
        EventScope::Sandbox(id) => Some(id.clone()),
        EventScope::Root => None,
    }).flatten();

    let current_model = std::env::var("APEX_VECTOR_MODEL").unwrap_or("all-minilm-l6-v2".to_string());

    let collection = db.get_collection(path.id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;
        
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

    let mut query_opts = apexkit_core::query::QueryOptions::default();
    query_opts.limit = Some(100_000); 
    query_opts.per_page = None;
    
    let all_records = db.list_records(path.id, query_opts).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?.items;

    let mut jobs_queued = 0;
    let mut skipped = 0;
    
    for record in all_records {
        let record_id: i64 = record.id; 

        for field_name in &vectorizable_fields {
            if let Some(text_content) = record.data.get(field_name).and_then(|v| v.as_str()) {
                
                // --- HARD vs SOFT LOGIC ---
                if !options.force {
                    // Soft Mode: Check if exists
                    let exists = db.has_vector(path.id, record_id, field_name, &current_model).await
                        .unwrap_or(false);
                        
                    if exists {
                        skipped += 1;
                        continue;
                    }
                }

                let job = Job::GenerateEmbedding {
                    tenant_id: current_tenant.clone(),
                    collection_id: path.id,
                    record_id,
                    field_name: field_name.clone(),
                    text_content: text_content.to_string(),
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