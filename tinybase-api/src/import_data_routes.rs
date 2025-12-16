use axum::{
    extract::{State, Json, Multipart},
    Extension,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value};
use tinybase_core::{
    auth::Claims, 
    schema::{CollectionSchema, FieldDefinition, FieldType}, 
};
use crate::{AppState, AppError};
use utoipa::{ToSchema};
use csv::{ReaderBuilder};
use std::io::Cursor;
use tracing::{info, error};
use std::collections::HashMap;

// --- DTOs ---

#[derive(Deserialize, ToSchema)]
pub struct ImportRequestDto {
    #[schema(example = "products")]
    pub collection_name: String,
}

#[derive(Serialize, ToSchema)] // FIX: Added #[derive(Serialize)]
pub struct ImportResponseDto {
    pub collection_id: i64,
    pub records_imported: usize,
    pub collection_created: bool,
}

// --- LOGIC ---

/// Infers a schema by scanning the first few records of CSV or JSON data.
fn infer_schema(data: &[Value], _collection_name: &str) -> CollectionSchema { // FIX: prefix with _
    let mut field_types: HashMap<String, FieldType> = HashMap::new();
    let mut fields: HashMap<String, FieldDefinition> = HashMap::new();
    
    for record in data.iter().take(50) { // Scan first 50 records
        if let Some(obj) = record.as_object() {
            for (key, val) in obj {
                if !field_types.contains_key(key) {
                    let inferred_type = match val {
                        Value::String(_) => FieldType::String,
                        Value::Number(_) => FieldType::Number,
                        Value::Bool(_) => FieldType::Boolean,
                        Value::Array(_) | Value::Object(_) => FieldType::Json,
                        _ => FieldType::String, // Default fallback
                    };
                    field_types.insert(key.clone(), inferred_type);
                } else if field_types.get(key) == Some(&FieldType::String) && val.is_number() {
                    // Promote to Number if a number is found later? Or stick to String.
                    // For simplicity, we stick to the first type unless a Json object is found.
                } else if field_types.get(key) != Some(&FieldType::Json) && (val.is_array() || val.is_object()) {
                    field_types.insert(key.clone(), FieldType::Json);
                }
            }
        }
    }

    for (name, r#type) in field_types {
        let r#type_clone = r#type.clone(); 
        
        fields.insert(name.clone(), FieldDefinition {
            r#type: r#type_clone.clone(),
            required: false, 
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

    let headers = reader.headers().map_err(|e| e.to_string())?.clone();
    let mut records = Vec::new();

    for result in reader.records() {
        let record = result.map_err(|e| e.to_string())?;
        let mut map = serde_json::Map::new();

        for (i, header) in headers.iter().enumerate() {
            let val = record.get(i).unwrap_or_default().trim();
            // Simple type coercion based on appearance
            let json_val = if let Ok(n) = val.parse::<f64>() {
                Value::Number(serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0)))
            } else if val.eq_ignore_ascii_case("true") {
                Value::Bool(true)
            } else if val.eq_ignore_ascii_case("false") {
                Value::Bool(false)
            } else if val.starts_with('{') || val.starts_with('[') {
                serde_json::from_str(val).unwrap_or(Value::String(val.to_string()))
            } else {
                Value::String(val.to_string())
            };
            map.insert(header.to_string(), json_val);
        }
        records.push(Value::Object(map));
    }
    Ok(records)
}

fn parse_json_array(data: &[u8]) -> Result<Vec<Value>, String> {
    serde_json::from_slice(data).map_err(|e| format!("Invalid JSON Array format: {}", e))
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
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ImportResponseDto>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let mut collection_name: Option<String> = None;
    let mut file_content: Option<(String, Vec<u8>)> = None; // (mime_type, content)

    // 1. Process Multipart Form
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

    let col_name = collection_name.ok_or(AppError::UnknownError("Missing 'collection_name' field".into()))?;
    let (mime, data) = file_content.ok_or(AppError::UnknownError("Missing 'file' field".into()))?;

    // 2. Parse Data
    let parsed_records: Vec<Value> = if mime.contains("text/csv") {
        parse_csv(&data).map_err(|e| AppError::UnknownError(format!("CSV Parsing Error: {}", e)))?
    } else if mime.contains("json") {
        parse_json_array(&data).map_err(|e| AppError::UnknownError(format!("JSON Parsing Error: {}", e)))?
    } else {
        return Err(AppError::UnknownError(format!("Unsupported file type: {}", mime)));
    };

    if parsed_records.is_empty() {
        return Err(AppError::UnknownError("No records found in the uploaded file.".into()));
    }

    // 3. Check/Create Collection
    let existing_col = state.db.list_collections().await.unwrap_or_default().into_iter().find(|c| c.name == col_name);
    let mut collection_created = false;
    let collection_id = if let Some(col) = existing_col {
        info!("Importing data into existing collection: {}", col_name);
        col.id
    } else {
        // Infer Schema and Create Collection
        let schema = infer_schema(&parsed_records, &col_name);
        info!("Collection {} not found. Inferring schema and creating.", col_name);
        let id = state.db.create_collection(&col_name, &Some(schema)).await
            .map_err(|e| AppError::UnknownError(format!("Failed to create collection: {}", e)))?;
        collection_created = true;
        id
    };

    // 4. Bulk Insert (Directly calling DB for now)
    let mut records_imported = 0;
    for record_data in parsed_records {
        if record_data.is_object() {
            match state.db.create_record(collection_id, &record_data).await {
                Ok(_) => records_imported += 1,
                Err(e) => {
                    error!("Failed to insert record into {}: {}", col_name, e);
                    // Continue processing other records
                }
            }
        }
    }

    // 5. Success
    Ok(Json(ImportResponseDto {
        collection_id,
        records_imported,
        collection_created,
    }))
}