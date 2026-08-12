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
    let text = String::from_utf8_lossy(&data);
    let mut items: Vec<apexkit_core::models::Script> = Vec::new();

    if text.trim_start().starts_with('[') {
        items = serde_json::from_str(&text)
            .map_err(|e| AppError::UnknownError(format!("Invalid JSON: {}", e)))?;
    } else {
        let blocks: Vec<&str> = text
            .split("//====================start-metadata====================")
            .collect();
        for block in blocks {
            if block.trim().is_empty() {
                continue;
            }

            let meta_end = block
                .find("//====================end-metadata====================")
                .ok_or_else(|| AppError::UnknownError("Missing end-metadata marker".to_string()))?;
            let meta_str = &block[..meta_end];

            let clean_meta: String = meta_str
                .lines()
                .filter_map(|l| {
                    let trimmed = l.trim();
                    if trimmed.starts_with("//") {
                        let inner = trimmed.strip_prefix("//").unwrap_or(trimmed).trim();
                        if inner.starts_with("===") {
                            None
                        } else if !inner.is_empty() {
                            Some(inner)
                        } else {
                            None
                        }
                    } else if !trimmed.is_empty() {
                        Some(trimmed)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");

            let mut script: apexkit_core::models::Script = serde_json::from_str(&clean_meta)
                .map_err(|e| AppError::UnknownError(format!("Invalid metadata JSON: {}", e)))?;

            let code_start_marker = "//====================start-code====================";
            let code_end_marker = "//====================end-code====================";
            let fallback_end_marker = "//====================start-code===================="; // Handling potential user typo

            let code_start_idx = block
                .find(code_start_marker)
                .map(|i| i + code_start_marker.len())
                .ok_or_else(|| AppError::UnknownError("Missing start-code marker".to_string()))?;

            let code_str = if let Some(code_end_idx) = block.find(code_end_marker) {
                &block[code_start_idx..code_end_idx]
            } else {
                let last_start = block.rfind(fallback_end_marker).unwrap_or(code_start_idx);
                if last_start > code_start_idx {
                    &block[code_start_idx..last_start]
                } else {
                    &block[code_start_idx..]
                }
            };

            script.code = code_str
                .trim_matches('\n')
                .trim_end_matches('\r')
                .to_string();
            items.push(script);
        }
    }

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
            metadata: item.metadata,
        };

        if let Err(e) = db.create_script(req).await {
            result.errors.push(format!("Failed {}: {}", item.name, e));
        } else {
            result.created += 1; // Actually could be updated too
        }
    }
    Ok(Json(result))
}
