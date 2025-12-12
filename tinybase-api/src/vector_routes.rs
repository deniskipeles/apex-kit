// tinybase-api/src/vector_routes.rs
use axum::{
    extract::{Path, State, Json},
    Extension,
};
use serde::{Deserialize, Serialize};
use tinybase_core::{auth::Claims, models::Record};
use crate::{AppState, AppError, RecordResponse};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct VectorSearchReq {
    pub vector: Vec<f32>,
    pub limit: Option<usize>,
    pub field: String, // Which field to search against (e.g. "description_vec")
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