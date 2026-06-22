use serde_json::Value;
use std::sync::Arc;

use super::super::queue::JobContext;
use crate::models::schema::CollectionSchema;

pub async fn handle_generate_embedding(
    resolver: Arc<dyn JobContext>,
    tenant_id: Option<String>,
    collection_id: i64,
    record_id: i64,
    field_name: String,
    content: String,
    content_type: String,
    model: String,
) -> Result<(), String> {
    if let Some((db, vector_provider)) = resolver.resolve(tenant_id.as_deref()).await {
        let vec_res = if content_type == "file" {
            let fs_root = match tenant_id.as_deref() {
                Some(id) if id.starts_with("session_") => {
                    format!("storage/sandboxes/{}/uploads", id)
                }
                Some(id) => format!("storage/tenants/{}/uploads", id),
                None => "./storage/system/uploads".to_string(),
            };

            let file_path = std::path::Path::new(&fs_root).join(&content);
            if let Ok(bytes) = tokio::fs::read(&file_path).await {
                let ext = file_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if ["jpg", "jpeg", "png", "webp", "gif"].contains(&ext.as_str()) {
                    use base64::{Engine as _, engine::general_purpose::STANDARD};
                    let b64 = STANDARD.encode(&bytes);
                    vector_provider.embed_image(&b64).await
                } else {
                    Err("Only image files are currently supported for vectorization.".into())
                }
            } else {
                Err(format!(
                    "File {} not found on disk for vectorization.",
                    content
                ))
            }
        } else if content.starts_with("data:image/") {
            vector_provider.embed_image(&content).await
        } else {
            vector_provider.embed(&content).await
        };

        match vec_res {
            Ok(vec) => {
                if let Err(e) = vector_provider
                    .index(collection_id, record_id, &field_name, &vec)
                    .await
                {
                    eprintln!("[Job] Failed to index vector: {}", e);
                }
                db.save_vector(collection_id, record_id, &field_name, vec, &model)
                    .await
                    .map_err(|e| e.to_string())?;
                println!(
                    "[Job] Successfully vectorized {} for record {}",
                    field_name, record_id
                );
            }
            Err(e) => {
                return Err(format!("Failed to generate embedding: {}", e));
            }
        }
    } else {
        return Err("Failed to resolve context for vectorization".to_string());
    }
    Ok(())
}

pub async fn handle_index_record(
    resolver: Arc<dyn JobContext>,
    tenant_id: Option<String>,
    collection_id: i64,
    record_id: i64,
    data: Value,
    schema: CollectionSchema,
) -> Result<(), String> {
    if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await {
        db.index_record_search(collection_id, record_id, &data, &schema)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub async fn handle_delete_record(
    resolver: Arc<dyn JobContext>,
    tenant_id: Option<String>,
    collection_id: i64,
    record_id: i64,
) -> Result<(), String> {
    if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await {
        db.delete_record_search(collection_id, record_id)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
