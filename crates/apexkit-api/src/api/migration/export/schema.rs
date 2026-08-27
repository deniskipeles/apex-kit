use crate::AppError;
use crate::DatabaseConnection;
use apexkit_core::auth::Claims;
use apexkit_core::realtime::EventScope;
use axum::{Extension, http::header, response::Response};
use std::collections::HashMap;

// Handler: Export All Collections (Schema Only)
#[utoipa::path(
    get,
    path = "/api/v1/admin/export-schema",
    responses((status = 200, description = "Downloadable JSON"))
)]
pub async fn export_schema_handler(
    Extension(claims): Extension<Claims>,
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let (scope_type, scope_id) = match event_scope {
        EventScope::Root => ("root", "root".to_string()),
        EventScope::Tenant(id) => ("tenant", id),
        EventScope::Sandbox(id) => ("sandbox", id),
        _ => ("unknown", "unknown".to_string()),
    };

    let date_str = chrono::Utc::now().format("%Y%m%d").to_string();
    let filename = format!(
        "apexkit-schema-{}-{}-{}.json",
        scope_type, scope_id, date_str
    );

    // 1. Fetch all collections
    let mut collections = db
        .list_collections()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 2. Normalize Relations (Replace DB IDs with Names/Indexes)
    let mut id_lookup: HashMap<String, (String, Option<String>)> = HashMap::new();
    let mut name_lookup: HashMap<String, (String, Option<String>)> = HashMap::new();

    for col in &collections {
        let val = (col.name.clone(), col.index.clone());
        id_lookup.insert(col.id.to_string(), val.clone());
        name_lookup.insert(col.name.clone(), val);
    }

    for col in &mut collections {
        if let Some(schema) = &mut col.schema {
            for rel in schema.relations.values_mut() {
                let target_raw = &rel.target_collection;

                if let Some((name, idx)) = id_lookup.get(target_raw) {
                    rel.target_collection = name.clone();
                    if rel.target_index.is_none() {
                        rel.target_index = idx.clone();
                    }
                } else if let Some((_, idx)) = name_lookup.get(target_raw)
                    && rel.target_index.is_none()
                {
                    rel.target_index = idx.clone();
                }
            }
        }
    }

    // 3. Serialize & Return with Version and Dynamic Filename
    let export_obj = serde_json::json!({
        "apexkit_version": env!("CARGO_PKG_VERSION"),
        "version": env!("CARGO_PKG_VERSION"),
        "collections": collections,
        "strategy": "skip",
        "exported_at": chrono::Utc::now().to_rfc3339()
    });

    let json_bytes = serde_json::to_vec_pretty(&export_obj)
        .map_err(|e| AppError::UnknownError(format!("Serialization Error: {}", e)))?;

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(json_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}
