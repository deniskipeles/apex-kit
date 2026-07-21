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
    model: Option<String>,
) -> Result<(), String> {
    // --- VERIFY VECTOR QUOTA ---
    if let Some(tid) = &tenant_id {
        if let Some((root_db, _)) = resolver.resolve(None).await {
            if tid.starts_with("session_") {
                let sid = tid.replace("session_", "");
                let sandboxes = root_db.list_sandboxes(None).await.unwrap_or_default();
                if let Some(sb) = sandboxes.iter().find(|s| s.id == sid) {
                    if sb.current_vectors >= sb.max_vectors {
                        return Err(format!(
                            "Sandbox {} vector limit exceeded ({} max)",
                            sid, sb.max_vectors
                        ));
                    }
                }
            } else {
                let tenants = root_db.list_tenants().await.unwrap_or_default();
                if let Some(t) = tenants.iter().find(|t| &t.id == tid) {
                    if t.stats.vector_count >= t.stats.max_vectors {
                        return Err(format!(
                            "Tenant {} vector limit exceeded ({} max)",
                            tid, t.stats.max_vectors
                        ));
                    }
                }
            }
        }
    }
    // ----------------------------

    if let Some((db, vector_provider)) = resolver.resolve(tenant_id.as_deref()).await {
        let mut is_image_embedding = false;

        let vec_res = if content_type == "file" {
            if let Ok(bytes) = resolver
                .get_file_bytes(tenant_id.as_deref(), &content)
                .await
            {
                let ext = std::path::Path::new(&content)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if ["jpg", "jpeg", "png", "webp", "gif"].contains(&ext.as_str()) {
                    use base64::{Engine as _, engine::general_purpose::STANDARD};
                    let b64 = STANDARD.encode(&bytes);
                    is_image_embedding = true;
                    vector_provider.embed_image(&b64).await
                } else {
                    Err("Only image files are currently supported for vectorization.".into())
                }
            } else {
                Err(format!(
                    "File {} not found in storage for vectorization.",
                    content
                ))
            }
        } else if content.starts_with("data:image/") {
            is_image_embedding = true;
            vector_provider.embed_image(&content).await
        } else {
            vector_provider.embed(&content).await
        };

        let resolved_model = model.unwrap_or_else(|| {
            if is_image_embedding {
                apexkit_vector::get_current_vision_model()
            } else {
                apexkit_vector::get_current_text_model()
            }
        });

        match vec_res {
            Ok(vec) => {
                if let Err(e) = vector_provider
                    .index(collection_id, record_id, &field_name, &vec)
                    .await
                {
                    eprintln!("[Job] Failed to index vector: {}", e);
                }
                db.save_vector(collection_id, record_id, &field_name, vec, &resolved_model)
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

// [NEW] Handles Safe Sequential Bulk Vectorization with DB Pagination
pub async fn handle_revectorize_collection(
    resolver: Arc<dyn JobContext>,
    tenant_id: Option<String>,
    collection_id: i64,
    force: bool,
) -> Result<(), String> {
    if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await {
        let collection = db
            .get_collection(collection_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Collection not found")?;

        let schema = collection.schema.unwrap_or_default();
        let vectorizable_fields: Vec<String> = schema
            .fields
            .iter()
            .filter(|(_, def)| def.vectorize)
            .map(|(name, _)| name.clone())
            .collect();

        if vectorizable_fields.is_empty() {
            return Ok(());
        }

        tracing::info!(
            "[Job] Starting bulk revectorization for collection {}",
            collection_id
        );

        let mut offset = 0;
        let limit = 1000;

        loop {
            let mut query_opts = crate::query::QueryOptions::default();
            query_opts.limit = Some(limit);
            query_opts.offset = Some(offset);

            let records = db
                .list_records(collection_id, query_opts)
                .await
                .map_err(|e| e.to_string())?
                .items;
            if records.is_empty() {
                break; // Pagination complete
            }

            for record in records {
                for field_name in &vectorizable_fields {
                    if let Some(content) = record.data.get(field_name).and_then(|v| v.as_str()) {
                        let def = schema.fields.get(field_name).unwrap();
                        let c_type = if def.r#type == crate::models::schema::FieldType::File {
                            "file"
                        } else {
                            "text"
                        };

                        // [RESOLVED MISMAPPED DEPENDENCY]
                        // Resolve the model name directly via apexkit_vector to preserve crate boundaries.
                        let current_model = if c_type == "file" {
                            apexkit_vector::get_current_vision_model()
                        } else {
                            apexkit_vector::get_current_text_model()
                        };

                        if !force {
                            if db
                                .has_vector(collection_id, record.id, field_name, &current_model)
                                .await
                                .unwrap_or(false)
                            {
                                continue;
                            }
                        }

                        // Generate the embedding synchronously in this worker thread
                        if let Err(e) = handle_generate_embedding(
                            resolver.clone(),
                            tenant_id.clone(),
                            collection_id,
                            record.id,
                            field_name.clone(),
                            content.to_string(),
                            c_type.to_string(),
                            Some(current_model),
                        )
                        .await
                        {
                            tracing::error!(
                                "[Job] Failed to vectorize record {}: {}",
                                record.id,
                                e
                            );
                        }

                        // We strictly sleep for 50ms between generation loops.
                        // This explicitly yields the Tokio task scheduler, giving live HTTP requests
                        // doing Instant Searches a chance to grab the CandleEmbedder Mutex.
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                }
            }
            offset += limit;
        }
        tracing::info!(
            "[Job] Finished bulk revectorization for collection {}",
            collection_id
        );
    }
    Ok(())
}

// [NEW] Handle Tantivy OSE Indexing in Background
pub async fn handle_reindex_collection(
    resolver: Arc<dyn JobContext>,
    tenant_id: Option<String>,
    collection_id: i64,
) -> Result<(), String> {
    if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await {
        tracing::info!(
            "[Job] Starting Tantivy index rebuild for collection {}",
            collection_id
        );
        db.reindex_collection(collection_id)
            .await
            .map_err(|e| e.to_string())?;
        tracing::info!(
            "[Job] Finished Tantivy index rebuild for collection {}",
            collection_id
        );
    }
    Ok(())
}
