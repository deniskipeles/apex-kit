// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/renderer.rs ===========================
use axum::{
    extract::{Path, State, Query},
    response::{Html, IntoResponse, Response},
    http::{HeaderMap}, 
};
use serde_json::{json, Value};
use tera::{Tera, Context, Function}; 
use crate::{AppState, AppError, DatabaseConnection};
use std::collections::HashMap;
use std::sync::Arc;
use tinybase_core::Db;
use tracing::{warn, info};

// --- HELPERS ---
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
            for (k, v) in b { a.insert(k, v); }
        }
        (a, b) => *a = b,
    }
}

// --- TERA DB FUNCTIONS ---
struct FindOneFn { db: Arc<dyn Db> }
impl Function for FindOneFn {
    fn call(&self, args: &HashMap<String, Value>) -> tera::Result<Value> {
        let col = args.get("col").and_then(|v| v.as_str()).ok_or("Missing 'col' argument")?;
        let id_val = args.get("id").ok_or("Missing 'id' argument")?;
        
        let id = if let Some(n) = id_val.as_i64() { n } 
        else if let Some(s) = id_val.as_str() { s.parse::<i64>().unwrap_or(0) } 
        else { return Ok(Value::Null); };
        
        let db = self.db.clone();
        let col_name = col.to_string();

        let result = tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let cols = db.list_collections().await.map_err(|e| e.to_string())?;
                let col_id = cols.into_iter().find(|c| c.name == col_name).map(|c| c.id)
                    .ok_or_else(|| format!("Collection '{}' not found", col_name))?;
                db.get_record(col_id, id, None).await.map_err(|e| e.to_string())
            })
        }).map_err(|e| tera::Error::msg(e))?;

        match result {
            Some(rec) => {
                let mut v = rec.data;
                if let Some(o) = v.as_object_mut() { o.insert("id".into(), json!(rec.id)); }
                Ok(v)
            },
            None => Ok(Value::Null)
        }
    }
}

struct FindFn { db: Arc<dyn Db> }
impl Function for FindFn {
    fn call(&self, args: &HashMap<String, Value>) -> tera::Result<Value> {
        let col = args.get("col").and_then(|v| v.as_str()).ok_or("Missing 'col' argument")?;
        let filter_arg = args.get("filter"); 
        let filter_str = match filter_arg {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Object(_)) => Some(serde_json::to_string(filter_arg.unwrap()).unwrap()),
            _ => None
        };

        let db = self.db.clone();
        let col_name = col.to_string();

        let result = tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let cols = db.list_collections().await.map_err(|e| e.to_string())?;
                let col_id = cols.into_iter().find(|c| c.name == col_name).map(|c| c.id)
                    .ok_or_else(|| format!("Collection '{}' not found", col_name))?;
                
                let mut opts = tinybase_core::query::QueryOptions::default();
                opts.filter = filter_str;
                opts.per_page = Some(100);
                db.list_records(col_id, opts).await.map_err(|e| e.to_string())
            })
        }).map_err(|e| tera::Error::msg(e))?;

        let list: Vec<Value> = result.items.into_iter().map(|r| {
            let mut v = r.data;
            if let Some(o) = v.as_object_mut() { o.insert("id".into(), json!(r.id)); }
            v
        }).collect();

        Ok(json!(list))
    }
}

// --- CORE RENDERER ---

async fn render_view_core(
    db: Arc<dyn Db>,
    state: AppState,
    slug: String,
    params: HashMap<String, String>,
    headers: HeaderMap,
    body: String,
    source_label: &str,
) -> Result<Response, AppError> {
    info!("[Renderer] Serving '{}' from source: {}", slug, source_label);

    let all_templates = db.list_templates().await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let target_template = all_templates.iter()
        .find(|t| t.slug == slug)
        .ok_or_else(|| AppError::NotFound(format!("Template '{}' not found", slug)))?;

    // Context Setup
    let is_htmx = headers.contains_key("HX-Request");
    let mut context_data = json!({
        "params": params,
        "headers": headers_to_map(&headers),
        "is_htmx": is_htmx,
    });

    if !body.is_empty() {
        if let Ok(j) = serde_json::from_str::<Value>(&body) { merge_json(&mut context_data, json!({"body": j})); } 
        else if let Ok(f) = serde_qs::from_str::<HashMap<String, String>>(&body) { merge_json(&mut context_data, json!({"body": f})); }
        else { merge_json(&mut context_data, json!({"body_raw": body})); }
    }

    if let Some(script_id) = target_template.script_id {
        let scripts = db.list_scripts().await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        if let Some(script) = scripts.into_iter().find(|s| s.id == script_id) {
            let script_res = state.script_engine.run_script(&script.code, context_data.clone(), db.clone(), state.embedder.clone(), state.vector_provider.clone(),  state.vault.clone())
                .await.map_err(|e| AppError::UnknownError(format!("Script Error: {}", e)))?;
            merge_json(&mut context_data, script_res);
        }
    }

    // Engine Setup
    let mut tera = Tera::default();
    let register_helpers = |t: &mut Tera| {
        t.register_function("db_find", FindFn { db: db.clone() });
        t.register_function("db_find_one", FindOneFn { db: db.clone() });
        t.register_filter("debug", |value: &Value, _: &HashMap<String, Value>| {
            Ok(serde_json::to_string_pretty(value).unwrap().into())
        });
    };
    register_helpers(&mut tera);

    // --- ROBUST TEMPLATE LOADING (Multi-Pass) ---
    let template_vec: Vec<(String, String)> = all_templates.iter()
        .map(|t| (t.slug.clone(), t.content.clone()))
        .collect();
    
    // 1. Try Batch Load
    if let Err(e) = tera.add_raw_templates(template_vec.clone()) {
        warn!("Batch template load failed: {}. Switching to resilient loading.", e);
        
        // 2. Resilient Load Loop
        // We loop up to 3 times to resolve out-of-order dependencies (e.g. child loaded before parent)
        tera = Tera::default(); 
        register_helpers(&mut tera);
        
        let mut pending = template_vec;
        let mut passes = 0;
        
        while !pending.is_empty() && passes < 3 {
            let mut next_pending = Vec::new();
            let mut made_progress = false;

            for (tslug, content) in pending {
                if let Err(_) = tera.add_raw_template(&tslug, &content) {
                    // Failed (maybe parent missing?), keep for next pass
                    next_pending.push((tslug, content));
                } else {
                    // Success
                    made_progress = true;
                }
            }

            pending = next_pending;
            passes += 1;

            if !made_progress {
                // No templates successfully added in this pass, stop trying to prevent infinite loop
                break;
            }
        }

        // 3. Check if Critical Template is Missing
        // If the requested slug is still in pending, it means it failed to compile even after retries.
        // We try to add it one last time to capture the specific error message for the user.
        if pending.iter().any(|(s, _)| s == &slug) {
             if let Some((_, content)) = pending.iter().find(|(s, _)| s == &slug) {
                 if let Err(err) = tera.add_raw_template(&slug, content) {
                     return Err(AppError::UnknownError(format!("Template Compilation Error ('{}'): {}", slug, err)));
                 }
             }
        }
    }

    // Render
    let context = Context::from_value(context_data)
        .map_err(|e| AppError::UnknownError(format!("Context Error: {}", e)))?;

    let rendered = tera.render(&slug, &context)
        .map_err(|e| AppError::UnknownError(format!("Render Error: {}", e)))?;

    Ok(Html(rendered).into_response())
}

// --- PUBLIC HANDLERS ---

pub async fn render_view(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<HashMap<String, String>>, 
    headers: HeaderMap,
    body: String, 
) -> Result<Response, AppError> {
    render_view_core(db, state, slug, params, headers, body, "Production").await
}

pub async fn render_sandbox_view(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Path((session_id, slug)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>, 
    headers: HeaderMap,
    body: String, 
) -> Result<Response, AppError> {
    let label = format!("Sandbox {}", session_id);
    render_view_core(db, state, slug, params, headers, body, &label).await
}