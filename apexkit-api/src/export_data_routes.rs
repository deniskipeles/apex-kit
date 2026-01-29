use axum::{
    extract::{Path, Query},
    response::{Response},
    http::{header},
    Extension,
};
use serde::Deserialize;
use apexkit_core::{
    auth::Claims, 
    query::QueryOptions,
};
use crate::{AppError};
use utoipa::{IntoParams, ToSchema}; 
use csv::WriterBuilder;
use crate::DatabaseConnection;
use std::collections::HashMap;
use apexkit_core::ai_models::AiAction;
use apexkit_core::script_models::Script;
use apexkit_core::models::Template;

// --- DTOs ---

#[derive(Deserialize, IntoParams, ToSchema)] 
pub struct ExportQuery {
    /// Format to export (json or csv)
    #[serde(default)]
    #[param(example = "json")]
    pub format: String,
    /// Sorting field, e.g. -created
    pub sort: Option<String>,
    /// Filter object string, e.g. {"status":"active"}
    pub filter: Option<String>,
}


// --- LOGIC ---

fn flatten_record_data(record: &apexkit_core::Record) -> Vec<(String, String)> {
    let mut flat_data = vec![
        ("id".to_string(), record.id.to_string()),
        ("created".to_string(), record.data.get("created").map(|v| v.as_str().unwrap_or_default()).unwrap_or_default().to_string()),
        ("updated".to_string(), record.data.get("updated").map(|v| v.as_str().unwrap_or_default()).unwrap_or_default().to_string()),
    ];

    if let Some(map) = record.data.as_object() {
        for (key, val) in map {
            // Simple flattening: Convert complex objects/arrays to JSON string
            let value = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => val.to_string(), // Convert any remaining complex types (JSON, Array) to string
            };
            flat_data.push((key.clone(), value));
        }
    }
    flat_data
}

fn create_csv_data(records: &[apexkit_core::Record]) -> Result<Vec<u8>, String> {
    if records.is_empty() { return Ok(Vec::new()); }
    
    // Use the first record to generate the column headers
    let first_record_data = flatten_record_data(&records[0]);
    let headers: Vec<String> = first_record_data.iter().map(|(k, _)| k.clone()).collect();
    
    let mut writer = WriterBuilder::new().from_writer(vec![]);
    writer.write_record(&headers).map_err(|e| e.to_string())?;

    for record in records {
        let current_record_data = flatten_record_data(record);
        let mut row_values = Vec::new();

        // Ensure we write data in the same order as the headers
        for header in &headers {
            if let Some((_, value)) = current_record_data.iter().find(|(k, _)| k == header) {
                row_values.push(value.as_str());
            } else {
                row_values.push(""); // Missing value
            }
        }
        writer.write_record(row_values).map_err(|e| e.to_string())?;
    }

    writer.flush().map_err(|e| e.to_string())?;
    Ok(writer.into_inner().map_err(|e| e.to_string())?)
}

// Struct to handle nested path params safely
#[derive(Deserialize)]
pub struct ExportPath {
    pub id: i64,
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/export-data/{id}",
    params(ExportQuery),
    responses((status = 200, description = "Exported data"))
)]
pub async fn export_data_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection, 
    Path(path): Path<ExportPath>,
    Query(params): Query<ExportQuery>,
) -> Result<Response, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let id = path.id;

    // Use 'db' (Tenant Context) instead of 'state.db' (Root Context)
    let collection = db.get_collection(id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;
        
    // 1. Prepare Query Options
    let options = QueryOptions {
        limit: Some(1000), 
        per_page: None,
        offset: None,
        page: None,
        sort: params.sort,
        filter: params.filter,
        expand: None,
        fields: None
    };
    
    // 2. Fetch All Records from Tenant DB
    let result = db.list_records(id, options).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
        
    let records = result.items;

    // 3. Format Data & Set Headers
    let (content_type, filename, body_bytes) = match params.format.to_lowercase().as_str() {
        "csv" => {
            let csv_bytes = create_csv_data(&records)
                .map_err(|e| AppError::UnknownError(format!("CSV Export Error: {}", e)))?;
                
            (
                "text/csv; charset=utf-8",
                format!("{}.csv", collection.name),
                csv_bytes,
            )
        },
        _ => { // Default to JSON
            let json_values: Vec<serde_json::Value> = records.into_iter().map(|r| {
                let mut data = r.data;
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("id".to_string(), serde_json::json!(r.id));
                }
                data
            }).collect();
            
            let json_bytes = serde_json::to_vec_pretty(&json_values)
                .map_err(|e| AppError::UnknownError(format!("JSON Export Error: {}", e)))?;
                
            (
                "application/json; charset=utf-8",
                format!("{}.json", collection.name),
                json_bytes,
            )
        }
    };
    
    // 4. Build Response
    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", filename))
        .body(body_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}

// Handler: Export All Collections (Schema Only)
#[utoipa::path(
    get,
    path = "/api/v1/admin/export-schema",
    responses((status = 200, description = "Downloadable JSON"))
)]
pub async fn export_schema_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // 1. Fetch all collections
    let mut collections = db.list_collections().await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 2. [FIX] Normalize Relations (Replace DB IDs with Names/Indexes)
    
    // Build lookup maps
    // ID -> (Name, Index)
    let mut id_lookup: HashMap<String, (String, Option<String>)> = HashMap::new();
    // Name -> (Name, Index) - needed if target is already a name
    let mut name_lookup: HashMap<String, (String, Option<String>)> = HashMap::new();

    for col in &collections {
        let val = (col.name.clone(), col.index.clone());
        id_lookup.insert(col.id.to_string(), val.clone());
        name_lookup.insert(col.name.clone(), val);
    }

    // Iterate through schema relations and normalize
    for col in &mut collections {
        if let Some(schema) = &mut col.schema {
            for (_, rel) in &mut schema.relations {
                let target_raw = &rel.target_collection;
                
                // Case A: Target is an ID (e.g. "17")
                if let Some((name, idx)) = id_lookup.get(target_raw) {
                    rel.target_collection = name.clone(); // Replace ID with Name
                    if rel.target_index.is_none() {
                        rel.target_index = idx.clone(); // Inject UUID Index
                    }
                } 
                // Case B: Target is already a Name (e.g. "issues")
                else if let Some((_, idx)) = name_lookup.get(target_raw) {
                    if rel.target_index.is_none() {
                        rel.target_index = idx.clone(); // Inject UUID Index
                    }
                }
            }
        }
    }

    // 3. Serialize & Return
    let export_obj = serde_json::json!({
        "collections": collections,
        "strategy": "skip", // Default strategy hint
        "exported_at": chrono::Utc::now().to_rfc3339()
    });

    let json_bytes = serde_json::to_vec_pretty(&export_obj)
        .map_err(|e| AppError::UnknownError(format!("Serialization Error: {}", e)))?;

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"apex_schema.json\"")
        .body(json_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}

// Handler: Export Scripts
#[utoipa::path(
    get,
    path = "/api/v1/admin/export-scripts",
    responses((status = 200, description = "Scripts JSON"))
)]
pub async fn export_scripts_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let scripts = db.list_scripts().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let json_bytes = serde_json::to_vec_pretty(&scripts)
        .map_err(|e| AppError::UnknownError(format!("Serialization Error: {}", e)))?;

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"scripts.json\"")
        .body(json_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}

// Handler: Export Templates
#[utoipa::path(
    get,
    path = "/api/v1/admin/export-templates",
    responses((status = 200, description = "Templates JSON"))
)]
pub async fn export_templates_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let templates = db.list_templates().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let json_bytes = serde_json::to_vec_pretty(&templates)
        .map_err(|e| AppError::UnknownError(format!("Serialization Error: {}", e)))?;

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"templates.json\"")
        .body(json_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}

// Handler: Export AI Actions
#[utoipa::path(
    get,
    path = "/api/v1/admin/export-ai-actions",
    responses((status = 200, description = "AI Actions JSON"))
)]
pub async fn export_ai_actions_handler(
    Extension(claims): Extension<Claims>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let actions = db.list_ai_actions().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let json_bytes = serde_json::to_vec_pretty(&actions)
        .map_err(|e| AppError::UnknownError(format!("Serialization Error: {}", e)))?;

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"ai_actions.json\"")
        .body(json_bytes.into())
        .map_err(|e| AppError::UnknownError(format!("Response build failed: {}", e)))
}