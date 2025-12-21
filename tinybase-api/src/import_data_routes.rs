use axum::{
    extract::{Json, Multipart},
    Extension,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, Map};
use tinybase_core::{
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
    pub schema_updated: bool, // New field to indicate if we added columns
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
            indexed: matches!(r#type_clone, FieldType::String | FieldType::Text), 
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
            uid: tinybase_core::schema::generate_hex_id(),
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
        let id = db.create_collection(&col_name, &Some(inferred_schema)).await
            .map_err(|e| AppError::UnknownError(format!("Failed to create collection: {}", e)))?;
        collection_created = true;
        id
    };

    // 5. Bulk Insert
    let mut records_imported = 0;
    for record_data in parsed_records {
        if record_data.is_object() {
            match db.create_record(collection_id, &record_data).await {
                Ok(_) => records_imported += 1,
                Err(e) => {
                    error!("Failed to insert record into {}: {}", col_name, e);
                }
            }
        }
    }

    Ok(Json(ImportResponseDto {
        collection_id,
        records_imported,
        collection_created,
        schema_updated,
    }))
}