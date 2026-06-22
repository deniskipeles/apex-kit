use crate::utils::resolve_collection_by_id_or_name;
use crate::{AppError, AppState, BaseUrl, DatabaseConnection, IdPath};
use apexkit_core::{auth::Claims, auth::policies, query::ApexQuery, realtime::EventScope};
use axum::{
    Extension,
    extract::{Json, Path, State},
};
use serde::Deserialize;
use utoipa::ToSchema;

// DTO for Advanced Query
// This now matches the full power of the new Query Engine
#[derive(Deserialize, ToSchema)]
pub struct AdvancedQueryRequest {
    pub from: Option<String>,                   // Optional if ID in path is used
    pub select: Option<Vec<serde_json::Value>>, // Complex SelectField JSON
    pub filter: Option<serde_json::Value>,
    pub group_by: Option<Vec<String>>,
    pub sort: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub system: Option<bool>,
    pub pipeline: Option<Vec<serde_json::Value>>, // Pipeline Steps
}

// --- HANDLER ---

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/query",
    request_body = AdvancedQueryRequest,
    params(IdPath),
    // [UPDATED] Response is generic JSON Array because structure depends on SELECT
    responses((status = 200,body = Vec<serde_json::Value>))
)]
pub async fn query_records_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(_state): State<AppState>, // Hooks not yet supported for raw engine in this iteration
    BaseUrl(_base_url): BaseUrl,
    _scope: Option<Extension<EventScope>>,
    Path(path): Path<IdPath>,
    Json(payload): Json<AdvancedQueryRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let claims = auth.map(|Extension(c)| c);
    let col = resolve_collection_by_id_or_name(&db, &path.id).await?;

    let policy = col
        .schema
        .as_ref()
        .map(|s| s.policies.read.as_str())
        .unwrap_or("public");

    // Compile Policy to SQL
    let rls_sql = policies::compile_to_sql(policy, claims.as_ref())
        .map_err(|e| AppError::UnknownError(format!("Policy Compilation Failed: {}", e)))?;

    if rls_sql == "1=0" {
        return Err(AppError::Forbidden("Read denied".into()));
    }

    let query = ApexQuery {
        from: col.name.clone(),
        select: payload
            .select
            .map(|v| serde_json::from_value(serde_json::Value::Array(v)).unwrap_or_default())
            .unwrap_or_default(),
        r#where: payload.filter,
        group_by: payload.group_by.unwrap_or_default(),
        sort: payload.sort,
        limit: payload.limit,
        offset: payload.offset,
        system: payload.system.unwrap_or(false),
        pipeline: payload
            .pipeline
            .map(|v| serde_json::from_value(serde_json::Value::Array(v)).unwrap_or_default())
            .unwrap_or_default(),
        rls_sql: Some(rls_sql), // INJECT RLS
    };

    let result = db
        .query_engine(query)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(result))
}
