use crate::AppError;
use crate::DatabaseConnection;
use apexkit_core::auth::Claims;
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
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // 1. Fetch all collections
    let mut collections = db
        .list_collections()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 2. [FIX] Normalize Relations (Replace DB IDs with Names/Indexes)

    // Build lookup maps
    // ID -> (Name, Index)
    let mut id_lookup: HashMap<String, (String, Option<String>)> = HashMap::new();
    // Name -> (Name, Index) - needed if target is already a name
    let mut name_lookup: HashMap<String, (String, Option<String>)> = HashMap::new();

    for col in &collections {
        let val = (col.name.clone(), col.index.clone());
        id_lookup.insert(col.id.to_string(), val.clone());
        name_lookup.insert(col.name.clone(), val);
    }

    // Iterate through schema relations and normalize
    for col in &mut collections {
        if let Some(schema) = &mut col.schema {
            for rel in schema.relations.values_mut() {
                let target_raw = &rel.target_collection;

                // Case A: Target is an ID (e.g. "17")
                if let Some((name, idx)) = id_lookup.get(target_raw) {
                    rel.target_collection = name.clone(); // Replace ID with Name
                    if rel.target_index.is_none() {
                        rel.target_index = idx.clone(); // Inject UUID Index
                    }
                }
                // Case B: Target is already a Name (e.g. "issues")
                else if let Some((_, idx)) = name_lookup.get(target_raw)
                    && rel.target_index.is_none()
                {
                    rel.target_index = idx.clone(); // Inject UUID Index
                }
            }
        }
    }

    // 3. Serialize & Return
    let export_obj = serde_json::json!({
        "collections": collections,
        "strategy": "skip", // Default strategy hint
        "exported_at": chrono::Utc::now().to_rfc3339()
    });

    let json_bytes = serde_json::to_vec_pretty(&export_obj)
        .map_err(|e| AppError::UnknownError(format!("Serialization Error: {}", e)))?;

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"apex_schema.json\"",
        )
        .body(json_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}
