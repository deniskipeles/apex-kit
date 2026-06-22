use crate::{AppError, DatabaseConnection};
use apexkit_core::auth::Claims;
use axum::{
    Extension,
    extract::{Json, Multipart},
};
use std::collections::HashMap;

use super::{ImportSchemaRequestDto, ImportSchemaResponseDto};

//  Handler: Import Collections (Schema Only)
#[utoipa::path(
    post,
    path = "/api/v1/admin/import-schema",
    request_body(content = ImportSchemaRequestDto, content_type = "multipart/form-data"),
    responses((status = 200, body = ImportSchemaResponseDto))
)]
pub async fn import_schema_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    mut multipart: Multipart,
) -> Result<Json<ImportSchemaResponseDto>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let mut file_data = Vec::new();
    let mut strategy = "skip".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::UnknownError("Multipart error".into()))?
    {
        let name = field.name().unwrap_or_default().to_string();

        if name == "file" {
            file_data = field
                .bytes()
                .await
                .map_err(|_| AppError::UnknownError("Failed to read file".into()))?
                .to_vec();
        } else if name == "strategy"
            && let Ok(s) = field.text().await
        {
            strategy = s;
        }
    }

    if file_data.is_empty() {
        return Err(AppError::UnknownError("No file uploaded".into()));
    }

    // Parse JSON from file bytes
    let payload: ImportSchemaRequestDto = serde_json::from_slice(&file_data)
        .map_err(|e| AppError::UnknownError(format!("Invalid JSON Schema File: {}", e)))?;

    let existing_cols = db
        .list_collections()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let mut stats = ImportSchemaResponseDto {
        created: 0,
        updated: 0,
        skipped: 0,
        errors: vec![],
    };

    // 1. First Pass: Resolve stable Indexes for all incoming collections
    // Create a lookup map of Index -> Name from the payload itself
    // This allows us to resolve intra-payload references even if they don't exist in DB yet.
    let mut payload_index_map = HashMap::new();
    for col in &payload.collections {
        if let Some(idx) = &col.index {
            payload_index_map.insert(idx.clone(), col.name.clone());
        }
    }

    // 2. Second Pass: Process & Import
    for mut col in payload.collections {
        // A. Match against DB by Index (Strong match) or Name (Weak match)
        let exists = existing_cols.iter().find(|c| {
            if let (Some(a), Some(b)) = (&c.index, &col.index) {
                a == b
            } else {
                c.name == col.name
            }
        });

        // B. Fix Relations using Stable Index
        if let Some(schema) = &mut col.schema {
            for rel in schema.relations.values_mut() {
                if let Some(target_idx) = &rel.target_index {
                    // Try to resolve name from DB first (if it exists and was renamed there)
                    if let Some(db_target) = existing_cols
                        .iter()
                        .find(|c| c.index.as_ref() == Some(target_idx))
                    {
                        rel.target_collection = db_target.name.clone();
                    }
                    // Fallback to payload map (if it's a new collection in this import)
                    else if let Some(payload_name) = payload_index_map.get(target_idx) {
                        rel.target_collection = payload_name.clone();
                    }
                }
            }
        }

        let effective_strategy = if !strategy.is_empty() {
            strategy.as_str()
        } else {
            payload.strategy.as_str()
        };

        match (exists, effective_strategy) {
            (Some(existing), "overwrite") => {
                if let Err(e) = db.update_collection(existing.id, None, col.schema).await {
                    stats
                        .errors
                        .push(format!("Failed to update {}: {}", col.name, e));
                } else {
                    stats.updated += 1;
                }
            }
            (Some(_), "error") => {
                return Err(AppError::UnknownError(format!(
                    "Collection {} exists",
                    col.name
                )));
            }
            (Some(_), _) => {
                stats.skipped += 1;
            }
            (None, _) => {
                // [UPDATED] Pass the index explicitly
                if let Err(e) = db
                    .create_collection(&col.name, &col.schema, col.index)
                    .await
                {
                    stats
                        .errors
                        .push(format!("Failed to create {}: {}", col.name, e));
                } else {
                    stats.created += 1;
                }
            }
        }
    }

    Ok(Json(stats))
}
