use axum::{
    extract::{Json, Multipart},
    Extension,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, Map};
use apexkit_core::{
    auth::Claims, 
    schema::{CollectionSchema, FieldDefinition, FieldType}, 
};
use crate::{AppError, DatabaseConnection};
use utoipa::{ToSchema};
use csv::{ReaderBuilder};
use std::io::Cursor;
use tracing::{info, error};
use std::collections::HashMap;
use regex::Regex;
use futures::{stream, StreamExt}; 

// --- DTOs ---

#[derive(Deserialize, ToSchema)]
pub struct ImportRequestDto {
    #[schema(example = "products")]
    pub collection_name: String,
}

#[derive(Serialize, ToSchema)]
pub struct ImportResponseDto {
    pub collection_id: i64,
    pub records_imported: usize,
    pub collection_created: bool,
    pub schema_updated: bool, 
    pub time_taken_to_insert_all: String,
}

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
    if normalized.chars().next().map_or(false, |c| c.is_numeric()) {
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
    
    for record in data.iter().take(100) { // Scan up to 100 records for better type detection
        if let Some(obj) = record.as_object() {
            for (key, val) in obj {
                let current_type = field_types.get(key);
                
                let inferred_type = match val {
                    Value::String(_) => FieldType::String,
                    Value::Number(_) => FieldType::Number,
                    Value::Bool(_) => FieldType::Boolean,
                    Value::Array(_) | Value::Object(_) => FieldType::Json,
                    Value::Null => continue, // Skip nulls, can't infer type
                };

                // Conflict resolution: If we thought it was String but see JSON, upgrade to JSON.
                // If we thought it was Number but see String, upgrade to String.
                if let Some(existing) = current_type {
                     if existing != &inferred_type {
                         // Simple upgrade path: Number -> String -> Json
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
        
        fields.insert(name.clone(), FieldDefinition {
            r#type: r#type_clone.clone(),
            required: false, // Imported data might be sparse
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
            uid: apexkit_core::schema::generate_hex_id(),
        });
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
                Value::Number(serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0)))
            } else if val.eq_ignore_ascii_case("true") {
                Value::Bool(true)
            } else if val.eq_ignore_ascii_case("false") {
                Value::Bool(false)
            } else if (val.starts_with('{') && val.ends_with('}')) || (val.starts_with('[') && val.ends_with(']')) {
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
        _ => return Err("JSON must be an array of objects".into())
    };

    // Sanitize keys in JSON objects
    let sanitized_records: Vec<Value> = array.into_iter().map(|item| {
        if let Value::Object(map) = item {
            let mut new_map = Map::new();
            for (k, v) in map {
                new_map.insert(sanitize_key(&k), v);
            }
            Value::Object(new_map)
        } else {
            item
        }
    }).collect();

    Ok(sanitized_records)
}

// --- HANDLER ---

#[utoipa::path(
    post,
    path = "/api/v1/admin/import-data",
    request_body(content = ImportRequestDto, content_type = "multipart/form-data"),
    responses((status = 202, body = ImportResponseDto))
)]
pub async fn import_data_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, 
    mut multipart: Multipart,
) -> Result<Json<ImportResponseDto>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let mut collection_name: Option<String> = None;
    let mut file_content: Option<(String, Vec<u8>)> = None;

    while let Some(field) = multipart.next_field().await.map_err(|_| AppError::UnknownError("Multipart error".into()))? {
        let name = field.name().unwrap_or_default().to_string();
        
        if name == "collection_name" {
            collection_name = field.text().await.ok();
        } else if name == "file" {
            let mime = field.content_type().unwrap_or_default().to_string();
            let data = field.bytes().await.map_err(|_| AppError::UnknownError("Failed to read file bytes".into()))?.to_vec();
            file_content = Some((mime, data));
        }
    }

    let col_name_raw = collection_name.ok_or(AppError::UnknownError("Missing 'collection_name' field".into()))?;
    // Also sanitize the collection name itself to be safe
    let col_name = sanitize_key(&col_name_raw);
    
    let (mime, data) = file_content.ok_or(AppError::UnknownError("Missing 'file' field".into()))?;

    // 2. Parse Data (With Key Sanitization)
    let parsed_records: Vec<Value> = if mime.contains("text/csv") || mime.contains("application/vnd.ms-excel") {
        parse_csv(&data).map_err(|e| AppError::UnknownError(format!("CSV Parsing Error: {}", e)))?
    } else if mime.contains("json") {
        parse_json_array(&data).map_err(|e| AppError::UnknownError(format!("JSON Parsing Error: {}", e)))?
    } else {
        return Err(AppError::UnknownError(format!("Unsupported file type: {}", mime)));
    };

    if parsed_records.is_empty() {
        return Err(AppError::UnknownError("No records found in the uploaded file.".into()));
    }

    // 3. Infer Schema from the Data
    let inferred_schema = infer_schema(&parsed_records);

    // 4. Check/Create/Update Collection
    let existing_col = db.list_collections().await.unwrap_or_default()
        .into_iter().find(|c| c.name == col_name);
    
    let mut collection_created = false;
    let mut schema_updated = false;
    
    let collection_id = if let Some(col) = existing_col {
        info!("Importing data into existing collection: {}", col_name);
        
        // --- MERGE SCHEMA LOGIC ---
        let mut current_schema = col.schema.unwrap_or_default();
        let mut changed = false;

        for (field_name, field_def) in inferred_schema.fields {
            // If field doesn't exist, add it
            if !current_schema.fields.contains_key(&field_name) {
                current_schema.fields.insert(field_name, field_def);
                changed = true;
            }
        }

        if changed {
            info!("Expanding schema for collection: {}", col_name);
            db.update_collection(col.id, None, Some(current_schema)).await
                .map_err(|e| AppError::UnknownError(format!("Failed to update schema: {}", e)))?;
            schema_updated = true;
        }

        col.id
    } else {
        info!("Creating new collection from import: {}", col_name);
        let id = db.create_collection(&col_name, &Some(inferred_schema), None).await
            .map_err(|e| AppError::UnknownError(format!("Failed to create collection: {}", e)))?;
        collection_created = true;
        id
    };

    // 5. Bulk Insert via `create_record` (High Concurrency Stress Test)
    // We execute 500 insertions in parallel. This fills the WriteManager buffer,
    // forcing it to commit batches instead of individual rows.
    let start_time = std::time::Instant::now();
    
    let records_imported = stream::iter(parsed_records)
        .map(|record_data| {
            let db = db.clone();
            async move {
                if record_data.is_object() {
                    // Standard API call
                    match db.create_record(collection_id, &record_data).await {
                        Ok(_) => 1,
                        Err(e) => {
                            error!("Insert failed: {}", e);
                            0
                        }
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
        tracing::info!("Bulk import finished. Re-indexing collection {} for search...", collection_id);
        if let Err(e) = db_clone.reindex_collection(collection_id).await {
            tracing::error!("Failed to re-index collection {} after import: {}", collection_id, e);
        } else {
            tracing::info!("Re-indexing complete for collection {}.", collection_id);
        }
    });
    // -------------------------------------------------

    Ok(Json(ImportResponseDto {
        collection_id,
        records_imported,
        collection_created,
        schema_updated,
        time_taken_to_insert_all: format!("{:?}", duration),
    }))
}

use apexkit_core::models::Collection;
// [NEW] DTO for Schema Import
#[derive(Deserialize, ToSchema)]
pub struct ImportSchemaRequest {
    pub collections: Vec<Collection>, // Array of full collection objects
    #[serde(default)]
    pub strategy: String, // "skip", "overwrite", "error"
}

#[derive(Serialize, ToSchema)]
pub struct ImportSchemaResponse {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

//  Handler: Import Collections (Schema Only)
#[utoipa::path(
    post,
    path = "/api/v1/admin/import-schema",
    request_body(content = ImportSchemaRequest, content_type = "multipart/form-data"),
    responses((status = 200, body = ImportSchemaResponse))
)]
pub async fn import_schema_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, 
    mut multipart: Multipart,
) -> Result<Json<ImportSchemaResponse>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let mut file_data = Vec::new();
    let mut strategy = "skip".to_string();

    while let Some(field) = multipart.next_field().await.map_err(|_| AppError::UnknownError("Multipart error".into()))? {
        let name = field.name().unwrap_or_default().to_string();
        
        if name == "file" {
            file_data = field.bytes().await.map_err(|_| AppError::UnknownError("Failed to read file".into()))?.to_vec();
        } else if name == "strategy" {
            if let Ok(s) = field.text().await { strategy = s; }
        }
    }

    if file_data.is_empty() { return Err(AppError::UnknownError("No file uploaded".into())); }

    // Parse JSON from file bytes
    let payload: ImportSchemaRequest = serde_json::from_slice(&file_data)
        .map_err(|e| AppError::UnknownError(format!("Invalid JSON Schema File: {}", e)))?;

    let existing_cols = db.list_collections().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let mut stats = ImportSchemaResponse { created: 0, updated: 0, skipped: 0, errors: vec![] };

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
            for (_, rel) in &mut schema.relations {
                if let Some(target_idx) = &rel.target_index {
                    // Try to resolve name from DB first (if it exists and was renamed there)
                    if let Some(db_target) = existing_cols.iter().find(|c| c.index.as_ref() == Some(target_idx)) {
                        rel.target_collection = db_target.name.clone();
                    } 
                    // Fallback to payload map (if it's a new collection in this import)
                    else if let Some(payload_name) = payload_index_map.get(target_idx) {
                        rel.target_collection = payload_name.clone();
                    }
                }
            }
        }

        let effective_strategy = if !strategy.is_empty() { strategy.as_str() } else { payload.strategy.as_str() };
        
        match (exists, effective_strategy) {
             (Some(existing), "overwrite") => {
                if let Err(e) = db.update_collection(existing.id, None, col.schema).await {
                    stats.errors.push(format!("Failed to update {}: {}", col.name, e));
                } else { stats.updated += 1; }
            },
            (Some(_), "error") => return Err(AppError::UnknownError(format!("Collection {} exists", col.name))),
            (Some(_), _) => { stats.skipped += 1; },
            (None, _) => {
                // [UPDATED] Pass the index explicitly
                if let Err(e) = db.create_collection(&col.name, &col.schema, col.index).await {
                    stats.errors.push(format!("Failed to create {}: {}", col.name, e));
                } else { stats.created += 1; }
            }
        }
    }

    Ok(Json(stats))
}

use apexkit_core::script_models::CreateScriptReq;
use apexkit_core::models::CreateTemplateReq;
use apexkit_core::ai_models::CreateActionReq;
use apexkit_core::ai_models::AiAction;

// --- DTOs ---

#[derive(Serialize, utoipa::ToSchema)]
pub struct ImportResult {
    pub created: usize,
    pub updated: usize,
    pub errors: Vec<String>,
}

// Helper for multipart file reading
async fn read_file_from_multipart(mut multipart: Multipart) -> Result<Vec<u8>, AppError> {
    while let Some(field) = multipart.next_field().await.map_err(|_| AppError::UnknownError("Multipart error".into()))? {
        if field.name() == Some("file") {
            return field.bytes().await.map_err(|_| AppError::UnknownError("Failed to read bytes".into())).map(|b| b.to_vec());
        }
    }
    Err(AppError::UnknownError("No file uploaded".into()))
}

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
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let data = read_file_from_multipart(multipart).await?;
    let items: Vec<apexkit_core::script_models::Script> = serde_json::from_slice(&data)
        .map_err(|e| AppError::UnknownError(format!("Invalid JSON: {}", e)))?;

    let mut result = ImportResult { created: 0, updated: 0, errors: vec![] };

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
            visibility: item.visibility
        };
        
        if let Err(e) = db.create_script(req).await {
             result.errors.push(format!("Failed {}: {}", item.name, e));
        } else {
             result.created += 1; // Actually could be updated too
        }
    }
    Ok(Json(result))
}

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
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let data = read_file_from_multipart(multipart).await?;
    let items: Vec<apexkit_core::models::Template> = serde_json::from_slice(&data)
        .map_err(|e| AppError::UnknownError(format!("Invalid JSON: {}", e)))?;

    let mut result = ImportResult { created: 0, updated: 0, errors: vec![] };

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
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let data = read_file_from_multipart(multipart).await?;
    let items: Vec<AiAction> = serde_json::from_slice(&data)
        .map_err(|e| AppError::UnknownError(format!("Invalid JSON: {}", e)))?;

    let mut result = ImportResult { created: 0, updated: 0, errors: vec![] };

    for item in items {
        let req = CreateActionReq {
            name: item.name.clone(),
            slug: item.slug.clone(),
            model: item.model,
            system_prompt: item.system_prompt,
            template: item.template,
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