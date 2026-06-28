use apexkit_core::{
    auth::{Claims, policies},
    query::QueryOptions,
};
use axum::{
    Extension,
    extract::{Json, Path, Query},
};

use crate::utils::resolve_collection_by_id_or_name;
use crate::{
    AppError, DatabaseConnection, IdPath, RecordListResponse, RecordResponse, SearchQuery,
};

#[utoipa::path(get,path = "/api/v1/collections/{id}/search",params(IdPath,SearchQuery),responses((status = 200,body = RecordListResponse)))]
pub async fn search_records(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    Path(path): Path<IdPath>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<RecordListResponse>, AppError> {
    let claims = auth.map(|Extension(c)| c);

    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let policy = col
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");
    if !policies::check_access(policy, claims.as_ref(), None) {
        return Err(AppError::Forbidden("Search denied".into()));
    }

    // 1. Fetch matching document IDs directly from the Tantivy Index (Max 1000 matches)
    let search_res = db
        .instant_search(col.id, &q.q, 10000)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let total = search_res.len() as i64;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    // 2. Paginate the matched IDs in-memory
    let paginated_ids: Vec<i64> = search_res
        .into_iter()
        .skip(offset as usize)
        .take(per_page as usize)
        .map(|item| item.id)
        .collect();

    let mut final_records = Vec::new();

    // 3. Query the SQLite database ONCE for the paginated subset of records
    if !paginated_ids.is_empty() {
        let options = QueryOptions {
            limit: Some(paginated_ids.len() as u64),
            filter: Some(serde_json::json!({ "id": { "$in": paginated_ids } }).to_string()),
            expand: q.expand.clone(), // Seamlessly handles None or Some()
            ..Default::default()
        };
        let list_res = db
            .list_records(col.id, options)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;

        let mut item_map: std::collections::HashMap<i64, apexkit_core::models::Record> =
            list_res.items.into_iter().map(|r| (r.id, r)).collect();

        // 4. Align the loaded records with the search relevance order
        final_records = paginated_ids
            .into_iter()
            .filter_map(|id| item_map.remove(&id))
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
