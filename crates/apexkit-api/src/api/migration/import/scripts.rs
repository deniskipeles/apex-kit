use super::{ImportResult, read_file_from_multipart};
use crate::{AppError, DatabaseConnection};
use apexkit_core::auth::Claims;
use axum::{
    Extension,
    extract::{Json, Multipart},
};

use apexkit_core::models::CreateScriptReq;

// Handler: Import Scripts
#[utoipa::path(
    post,
    path = "/api/v1/admin/import-scripts",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses((status = 200, body = ImportResult))
)]
pub async fn import_scripts_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    multipart: Multipart,
) -> Result<Json<ImportResult>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let data = read_file_from_multipart(multipart).await?;
    let items: Vec<apexkit_core::models::Script> = serde_json::from_slice(&data)
        .map_err(|e| AppError::UnknownError(format!("Invalid JSON: {}", e)))?;

    let mut result = ImportResult {
        created: 0,
        updated: 0,
        errors: vec![],
    };

    for item in items {
        // Upsert Logic (Try create, if fails due to unique constraint, try update logic if desired or skip)
        // Here we use create_script which has ON CONFLICT UPDATE built-in usually, or we check existence.
        // Assuming create_script handles upsert based on name.
        let req = CreateScriptReq {
            name: item.name.clone(),
            trigger_type: item.trigger_type,
            target_collection: item.target_collection,
            code: item.code,
            active: item.active,
            visibility: item.visibility,
        };

        if let Err(e) = db.create_script(req).await {
            result.errors.push(format!("Failed {}: {}", item.name, e));
        } else {
            result.created += 1; // Actually could be updated too
        }
    }
    Ok(Json(result))
}
