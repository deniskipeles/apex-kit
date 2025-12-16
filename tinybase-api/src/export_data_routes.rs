use axum::{
    extract::{Path, State, Query},
    response::{Response},
    http::{header},
    Extension,
};
use serde::Deserialize;
use tinybase_core::{
    auth::Claims, 
    query::QueryOptions,
};
use crate::{AppState, AppError};
use utoipa::{IntoParams, ToSchema}; 
use csv::WriterBuilder;

// --- DTOs ---

#[derive(Deserialize, IntoParams, ToSchema)] 
pub struct ExportQuery {
    /// Format to export (json or csv)
    #[serde(default)]
    #[param(example = "json")]
    pub format: String,
    
    /// Sorting field, e.g. -created
    // FIX: Removed invalid #[param(style = Query)]
    pub sort: Option<String>, 
    
    /// Filter object string, e.g. {"status":"active"}
    // FIX: Removed invalid #[param(style = Query)]
    pub filter: Option<String>,
}


// --- LOGIC ---

fn flatten_record_data(record: &tinybase_core::Record) -> Vec<(String, String)> {
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

fn create_csv_data(records: &[tinybase_core::Record]) -> Result<Vec<u8>, String> {
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


#[utoipa::path(
    get,
    path = "/api/v1/admin/export-data/{id}",
    params(ExportQuery),
    responses((status = 200, description = "Exported data"))
)]
pub async fn export_data_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<ExportQuery>,
) -> Result<Response, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let collection = state.db.get_collection(id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Collection not found".into()))?;
        
    // 1. Prepare Query Options (to fetch ALL records with filter/sort)
    let options = QueryOptions {
        limit: None, // Will be set to Max i64 in DB layer for "all"
        per_page: None,
        offset: None,
        page: None,
        sort: params.sort,
        filter: params.filter,
        expand: None,
    };
    
    // 2. Fetch All Records
    let result = state.db.list_records(id, options).await
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