use super::{ImportDataRequestDto, ImportDataResponseDto};
use crate::{AppError, DatabaseConnection};
use apexkit_core::{
    auth::Claims,
    models::schema::{CollectionSchema, FieldDefinition, FieldType},
};
use axum::{
    Extension,
    extract::{Json, Multipart},
};
use csv::ReaderBuilder;
use futures::{StreamExt, stream};
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::Cursor;
use tracing::{error, info};

// --- LOGIC ---

/// Sanitizes strings to be safe field names (lowercase, snake_case, no special chars)
/// e.g. "First Name" -> "first_name", "Order #" -> "order"
fn sanitize_key(key: &str) -> String {
    // 1. Replace non-alphanumeric characters with underscores
    let re = Regex::new(r"[^a-zA-Z0-9]+").unwrap();
    let replaced = re.replace_all(key, "_").to_string();

    // 2. Lowercase
    let mut normalized = replaced.to_lowercase();

    // 3. Trim leading/trailing underscores
    normalized = normalized.trim_matches('_').to_string();

    // 4. Ensure it doesn't start with a number (prepend _)
    if normalized.chars().next().is_some_and(|c| c.is_numeric()) {
        normalized = format!("_{}", normalized);
    }

    if normalized.is_empty() {
        return "field_unknown".to_string();
    }

    normalized
}

/// Infers a schema by scanning the records
fn infer_schema(data: &[Value]) -> CollectionSchema {
    let mut field_types: HashMap<String, FieldType> = HashMap::new();
    let mut fields: HashMap<String, FieldDefinition> = HashMap::new();

    for record in data.iter().take(100) {
        // Scan up to 100 records for better type detection
        if let Some(obj) = record.as_object() {
            for (key, val) in obj {
                // Ignore system keys to prevent creating custom fields named id or _id
                if key == "id" || key == "_id" {
                    continue;
                }

                let current_type = field_types.get(key);

                let inferred_type = match val {
                    Value::String(_) => FieldType::String,
                    Value::Number(_) => FieldType::Number,
                    Value::Bool(_) => FieldType::Boolean,
                    Value::Array(_) | Value::Object(_) => FieldType::Json,
                    Value::Null => continue, // Skip nulls, can't infer type
                };

                // Conflict resolution: upgrade types based on observed data
                if let Some(existing) = current_type {
                    if existing != &inferred_type {
                        if *existing == FieldType::Number && inferred_type == FieldType::String {
                            field_types.insert(key.clone(), FieldType::String);
                        } else if *existing != FieldType::Json && inferred_type == FieldType::Json {
                            field_types.insert(key.clone(), FieldType::Json);
                        }
                    }
                } else {
                    field_types.insert(key.clone(), inferred_type);
                }
            }
        }
    }

    for (name, r#type) in field_types {
        let r#type_clone = r#type.clone();

        fields.insert(
            name.clone(),
            FieldDefinition {
                r#type: r#type_clone.clone(),
                required: false,
                ose_indexed: matches!(r#type_clone, FieldType::String | FieldType::Text),
                sql_indexed: false,
                auto: false,
                vectorize: false,
                default: None,
                unique: None,
                min: None,
                max: None,
                min_length: None,
                max_length: None,
                pattern: None,
                options: None,
                mime_types: None,
                max_size: None,
                dimension: None,
                relation_to: None,
                position: 0,
                uid: apexkit_core::models::schema::generate_hex_id(),
            },
        );
    }

    CollectionSchema {
        fields,
        policies: Default::default(),
        relations: Default::default(),
        field_history: Default::default(),
        composite_unique: Default::default(),
    }
}

fn parse_csv(data: &[u8]) -> Result<Vec<Value>, String> {
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(Cursor::new(data));

    // Get headers and sanitize them immediately
    let headers_raw = reader.headers().map_err(|e| e.to_string())?.clone();
    let headers: Vec<String> = headers_raw.iter().map(sanitize_key).collect();

    let mut records = Vec::new();

    for result in reader.records() {
        let record = result.map_err(|e| e.to_string())?;
        let mut map = serde_json::Map::new();

        for (i, header) in headers.iter().enumerate() {
            let val = record.get(i).unwrap_or_default().trim();
            // Type coercion
            let json_val = if val.is_empty() {
                Value::Null
            } else if let Ok(n) = val.parse::<f64>() {
                Value::Number(
                    serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0)),
                )
            } else if val.eq_ignore_ascii_case("true") {
                Value::Bool(true)
            } else if val.eq_ignore_ascii_case("false") {
                Value::Bool(false)
            } else if (val.starts_with('{') && val.ends_with('}'))
                || (val.starts_with('[') && val.ends_with(']'))
            {
                // Try parse JSON content inside CSV
                serde_json::from_str(val).unwrap_or(Value::String(val.to_string()))
            } else {
                Value::String(val.to_string())
            };

            // Skip nulls to save space/bandwidth, or keep them? Keeping nulls helps with schema inference
            if !json_val.is_null() {
                map.insert(header.to_string(), json_val);
            }
        }
        records.push(Value::Object(map));
    }
    Ok(records)
}

fn parse_json_array(data: &[u8]) -> Result<Vec<Value>, String> {
    let raw: Value = serde_json::from_slice(data).map_err(|e| format!("Invalid JSON: {}", e))?;

    let array = match raw {
        Value::Array(a) => a,
        _ => return Err("JSON must be an array of objects".into()),
    };

    // Sanitize keys in JSON objects
    let sanitized_records: Vec<Value> = array
        .into_iter()
        .map(|item| {
            if let Value::Object(map) = item {
                let mut new_map = Map::new();
                for (k, v) in map {
                    new_map.insert(sanitize_key(&k), v);
                }
                Value::Object(new_map)
            } else {
                item
            }
        })
        .collect();

    Ok(sanitized_records)
}

// --- HANDLER ---

#[utoipa::path(
    post,
    path = "/api/v1/admin/import-data",
    request_body(content = ImportDataRequestDto, content_type = "multipart/form-data"),
    responses((status = 202, body = ImportDataResponseDto))
)]
pub async fn import_data_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
    mut multipart: Multipart,
) -> Result<Json<ImportDataResponseDto>, AppError> {
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let mut collection_name: Option<String> = None;
    let mut file_content: Option<(String, Vec<u8>)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::UnknownError("Multipart error".into()))?
    {
        let name = field.name().unwrap_or_default().to_string();

        if name == "collection_name" {
            collection_name = field.text().await.ok();
        } else if name == "file" {
            let mime = field.content_type().unwrap_or_default().to_string();
            let data = field
                .bytes()
                .await
                .map_err(|_| AppError::UnknownError("Failed to read file bytes".into()))?
                .to_vec();
            file_content = Some((mime, data));
        }
    }

    let col_name_raw = collection_name.ok_or(AppError::UnknownError(
        "Missing 'collection_name' field".into(),
    ))?;
    // Also sanitize the collection name itself to be safe
    let col_name = sanitize_key(&col_name_raw);

    let (mime, data) = file_content.ok_or(AppError::UnknownError("Missing 'file' field".into()))?;

    // 2. Parse Data (With Key Sanitization)
    let parsed_records: Vec<Value> = if mime.contains("text/csv")
        || mime.contains("application/vnd.ms-excel")
    {
        parse_csv(&data).map_err(|e| AppError::UnknownError(format!("CSV Parsing Error: {}", e)))?
    } else if mime.contains("json") {
        parse_json_array(&data)
            .map_err(|e| AppError::UnknownError(format!("JSON Parsing Error: {}", e)))?
    } else {
        return Err(AppError::UnknownError(format!(
            "Unsupported file type: {}",
            mime
        )));
    };

    if parsed_records.is_empty() {
        return Err(AppError::UnknownError(
            "No records found in the uploaded file.".into(),
        ));
    }

    // 3. Infer Schema from the Data
    let inferred_schema = infer_schema(&parsed_records);

    // 4. Check/Create/Update Collection
    let existing_col = db
        .list_collections()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|c| c.name == col_name);

    let mut collection_created = false;
    let mut schema_updated = false;

    let collection_id = if let Some(col) = existing_col {
        info!("Importing data into existing collection: {}", col_name);

        // --- MERGE SCHEMA LOGIC ---
        let mut current_schema = col.schema.unwrap_or_default();
        let mut changed = false;

        for (field_name, field_def) in inferred_schema.fields {
            // If field doesn't exist, add it
            if let std::collections::hash_map::Entry::Vacant(e) =
                current_schema.fields.entry(field_name)
            {
                e.insert(field_def);
                changed = true;
            }
        }

        if changed {
            info!("Expanding schema for collection: {}", col_name);
            db.update_collection(col.id, None, Some(current_schema))
                .await
                .map_err(|e| AppError::UnknownError(format!("Failed to update schema: {}", e)))?;
            schema_updated = true;
        }

        col.id
    } else {
        info!("Creating new collection from import: {}", col_name);
        let id = db
            .create_collection(&col_name, &Some(inferred_schema), None)
            .await
            .map_err(|e| AppError::UnknownError(format!("Failed to create collection: {}", e)))?;
        collection_created = true;
        id
    };

    // We execute 500 insertions in parallel. This fills the WriteManager buffer,
    // forcing it to commit batches instead of individual rows.
    // 5. Bulk Insert via create_record / import_record (High Concurrency)
    let start_time = std::time::Instant::now();

    let records_imported = stream::iter(parsed_records)
        .map(|record_data| {
            let db = db.clone();
            async move {
                if record_data.is_object() {
                    let mut data_to_save = record_data.clone();
                    let mut explicit_id = None;

                    // Safely extract and strip "id" or "_id" to treat as the primary key
                    if let Some(obj) = data_to_save.as_object_mut()
                        && let Some(id_val) = obj.remove("id").or_else(|| obj.remove("_id"))
                        && let Some(id_num) = id_val
                            .as_i64()
                            .or_else(|| id_val.as_str().and_then(|s| s.parse::<i64>().ok()))
                    {
                        explicit_id = Some(id_num);
                    }

                    match explicit_id {
                        Some(rid) => {
                            match db.import_record(collection_id, rid, &data_to_save).await {
                                Ok(_) => 1,
                                Err(e) => {
                                    error!("Import failed for record ID {}: {}", rid, e);
                                    0
                                }
                            }
                        }
                        None => match db.create_record(collection_id, &data_to_save).await {
                            Ok(_) => 1,
                            Err(e) => {
                                error!("Insert failed: {}", e);
                                0
                            }
                        },
                    }
                } else {
                    0
                }
            }
        })
        .buffer_unordered(500) // Concurrent Workers: 500
        .collect::<Vec<usize>>()
        .await
        .into_iter()
        .sum();

    let duration = start_time.elapsed();

    // --- [NEW] AUTO RE-INDEX OSE AFTER BULK IMPORT ---
    // We run this in the background so the API returns quickly,
    // but the search index gets fully rebuilt with the newly imported data.
    let db_clone = db.clone();
    tokio::spawn(async move {
        tracing::info!(
            "Bulk import finished. Re-indexing collection {} for search...",
            collection_id
        );
        if let Err(e) = db_clone.reindex_collection(collection_id).await {
            tracing::error!(
                "Failed to re-index collection {} after import: {}",
                collection_id,
                e
            );
        } else {
            tracing::info!("Re-indexing complete for collection {}.", collection_id);
        }
    });
    // -------------------------------------------------

    Ok(Json(ImportDataResponseDto {
        collection_id,
        records_imported,
        collection_created,
        schema_updated,
        time_taken_to_insert_all: format!("{:?}", duration),
    }))
}
