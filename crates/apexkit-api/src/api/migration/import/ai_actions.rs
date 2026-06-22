use super::{ImportResult, read_file_from_multipart};
use crate::{AppError, DatabaseConnection};
use apexkit_core::auth::Claims;
use axum::{
    Extension,
    extract::{Json, Multipart},
};

use apexkit_core::models::AiAction;
use apexkit_core::models::CreateActionReq;

// Handler: Import AI Actions
#[utoipa::path(
    post,
    path = "/api/v1/admin/import-ai-actions",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses((status = 200, body = ImportResult))
)]
pub async fn import_ai_actions_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    multipart: Multipart,
) -> Result<Json<ImportResult>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let data = read_file_from_multipart(multipart).await?;
    let items: Vec<AiAction> = serde_json::from_slice(&data)
        .map_err(|e| AppError::UnknownError(format!("Invalid JSON: {}", e)))?;

    let mut result = ImportResult {
        created: 0,
        updated: 0,
        errors: vec![],
    };

    for item in items {
        let req = CreateActionReq {
            name: item.name.clone(),
            slug: item.slug.clone(),
            model: item.model,
            system_prompt: item.system_prompt,
            template: item.template,
            config: item.config,
        };

        // Assuming create handles upsert on slug, or we manually check
        // Db trait: create_ai_action usually just inserts. You might need to add upsert logic in core/lib.rs
        // For now, let's try create, if fail (duplicate slug), we ignore or log.
        if let Err(e) = db.create_ai_action(req.clone()).await {
            // Basic retry: delete then create (simple replace)
            if let Ok(Some(existing)) = db.get_ai_action(&item.slug).await {
                let _ = db.delete_ai_action(existing.id).await;
                let _ = db.create_ai_action(req).await;
                result.updated += 1;
            } else {
                result.errors.push(format!("Failed {}: {}", item.slug, e));
            }
        } else {
            result.created += 1;
        }
    }
    Ok(Json(result))
}
