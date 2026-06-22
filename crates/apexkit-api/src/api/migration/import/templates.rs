use super::{ImportResult, read_file_from_multipart};
use crate::{AppError, DatabaseConnection};
use apexkit_core::auth::Claims;
use axum::{
    Extension,
    extract::{Json, Multipart},
};

use apexkit_core::models::CreateTemplateReq;

// Handler: Import Templates
#[utoipa::path(
    post,
    path = "/api/v1/admin/import-templates",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses((status = 200, body = ImportResult))
)]
pub async fn import_templates_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    multipart: Multipart,
) -> Result<Json<ImportResult>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let data = read_file_from_multipart(multipart).await?;
    let items: Vec<apexkit_core::models::Template> = serde_json::from_slice(&data)
        .map_err(|e| AppError::UnknownError(format!("Invalid JSON: {}", e)))?;

    let mut result = ImportResult {
        created: 0,
        updated: 0,
        errors: vec![],
    };

    for item in items {
        let req = CreateTemplateReq {
            slug: item.slug.clone(),
            content: item.content,
            script_id: item.script_id, // Note: Script IDs might mismatch if scripts weren't imported first or IDs changed
        };

        if let Err(e) = db.create_template(req).await {
            result.errors.push(format!("Failed {}: {}", item.slug, e));
        } else {
            result.created += 1;
        }
    }
    Ok(Json(result))
}
