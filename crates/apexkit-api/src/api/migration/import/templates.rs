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
    let text = String::from_utf8_lossy(&data);
    let mut items: Vec<apexkit_core::models::Template> = Vec::new();

    if text.trim_start().starts_with('[') {
        items = serde_json::from_str(&text)
            .map_err(|e| AppError::UnknownError(format!("Invalid JSON: {}", e)))?;
    } else {
        let blocks: Vec<&str> = text
            .split("<!-- ====================start-metadata==================== -->")
            .collect();
        for block in blocks {
            if block.trim().is_empty() {
                continue;
            }

            let meta_end = block
                .find("<!-- ====================end-metadata==================== -->")
                .ok_or_else(|| AppError::UnknownError("Missing end-metadata marker".to_string()))?;
            let meta_str = &block[..meta_end];

            let clean_meta: String = meta_str
                .lines()
                .filter_map(|l| {
                    let mut s = l.trim();
                    if s.starts_with("<!--") {
                        s = s.strip_prefix("<!--").unwrap().trim();
                    }
                    if s.ends_with("-->") {
                        s = s.strip_suffix("-->").unwrap().trim();
                    }
                    if s.starts_with("===") {
                        return None;
                    }
                    if !s.is_empty() { Some(s) } else { None }
                })
                .collect::<Vec<_>>()
                .join("");

            let mut tmpl: apexkit_core::models::Template = serde_json::from_str(&clean_meta)
                .map_err(|e| AppError::UnknownError(format!("Invalid metadata JSON: {}", e)))?;

            let code_start_marker = "<!-- ====================start-code==================== -->";
            let code_end_marker = "<!-- ====================end-code==================== -->";
            let fallback_end_marker = "<!-- ====================start-code==================== -->"; // Handling potential user typo

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

            tmpl.content = code_str
                .trim_matches('\n')
                .trim_end_matches('\r')
                .to_string();
            items.push(tmpl);
        }
    }

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
