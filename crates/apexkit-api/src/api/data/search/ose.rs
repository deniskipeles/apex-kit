use crate::utils::resolve_collection_by_id_or_name;
use crate::{AppError, DatabaseConnection, IdPath, RecordResponse, SearchQuery};
use apexkit_core::{auth::Claims, auth::policies};
use axum::{
    Extension,
    extract::{Json, Path, Query},
};

#[utoipa::path(get,path = "/api/v1/collections/{id}/search",params(IdPath,SearchQuery),responses((status = 200,body = Vec<RecordResponse>)))]
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
    params(IdPath,SearchQuery),
    responses((status = 200,body = Vec<apexkit_core::models::InstantResult>))
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
