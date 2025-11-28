use axum::{
    extract::{Path, State, Query},
    response::{Html, IntoResponse, Response},
    http::HeaderMap, 
};
use serde_json::{json, Value};
use tera::{Tera, Context};
use crate::{AppState, AppError};
use std::collections::HashMap;

// --- THE DYNAMIC RENDERER ---

pub async fn render_view(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<HashMap<String, String>>, 
    headers: HeaderMap,
    // FIX: Changed Option<String> to String to satisfy Axum Handler trait
    body: String, 
) -> Result<Response, AppError> {

    // 1. Fetch Template
    let template = state.db.get_template_by_slug(&slug).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::NotFound("Template not found".into()))?;

    // 2. Prepare Data Context
    let mut context_data = json!({
        "params": params,
        "headers": headers_to_map(&headers),
    });

    // FIX: Check if body string is not empty before parsing
    if !body.is_empty() {
        // Try parsing as JSON first
        if let Ok(j) = serde_json::from_str::<Value>(&body) {
            merge_json(&mut context_data, json!({"body": j}));
        } 
        // Try parsing as Form Data (standard for HTMX)
        else if let Ok(form_data) = serde_qs::from_str::<HashMap<String, String>>(&body) {
            merge_json(&mut context_data, json!({"body": form_data}));
        }
        else {
            // Raw string fallback
            merge_json(&mut context_data, json!({"body_raw": body}));
        }
    }

    // 3. Execute Attached Script (If any)
    if let Some(script_id) = template.script_id {
        let scripts = state.db.list_scripts().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        if let Some(script) = scripts.into_iter().find(|s| s.id == script_id) {
            
            // Run Script with Request Data
            let script_result = state.script_engine.run_script(
                &script.code, 
                context_data.clone(), 
                state.db.clone()
            ).await.map_err(|e| AppError::UnknownError(format!("Script Error: {}", e)))?;

            // Merge script result into context
            merge_json(&mut context_data, script_result);
        }
    }

    // 4. Render with Tera
    let mut tera = Tera::default();
    
    // We register the template string "on the fly"
    tera.add_raw_template(&slug, &template.content)
        .map_err(|e| AppError::UnknownError(format!("Template Syntax Error: {}", e)))?;

    let context = Context::from_value(context_data)
        .map_err(|e| AppError::UnknownError(format!("Context Error: {}", e)))?;

    let rendered = tera.render(&slug, &context)
        .map_err(|e| AppError::UnknownError(format!("Render Error: {}", e)))?;

    Ok(Html(rendered).into_response())
}

// Helpers
fn headers_to_map(headers: &HeaderMap) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            map.insert(k.to_string(), val.to_string());
        }
    }
    map
}

fn merge_json(a: &mut Value, b: Value) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge_json(a.entry(k).or_insert(Value::Null), v);
            }
        }
        (a, b) => *a = b,
    }
}