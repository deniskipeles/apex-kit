use rquickjs::function::{Async, Rest};
use rquickjs::{Ctx, Function, Object, Value};
use rquickjs_serde::{from_value, to_value};
use serde_json::{Value as JsonValue, json};
use std::sync::Arc;

use super::super::context::ScriptContext;
use crate::Db;
use crate::query::{ApexQuery, QueryOptions};
use crate::realtime::EventScope;

// --- HELPERS ---

async fn resolve_collection_local(db: Arc<dyn Db>, identifier: &str) -> Result<i64, String> {
    if let Ok(id) = identifier.parse::<i64>() {
        return Ok(id);
    }
    let cols = db.list_collections().await.map_err(|e| e.to_string())?;
    cols.into_iter()
        .find(|c| c.name == identifier)
        .map(|c| c.id)
        .ok_or_else(|| format!("Collection '{}' not found", identifier))
}

pub async fn resolve_db(
    ctx_str: Option<String>,
    app_ctx: Arc<dyn ScriptContext>,
) -> Result<Arc<dyn Db>, String> {
    if let Some(s) = ctx_str {
        if s.starts_with("tenant:") {
            let tid = s.strip_prefix("tenant:").unwrap();
            return app_ctx
                .resolve_tenant_db(tid)
                .await
                .ok_or(format!("Tenant {} not found", tid));
        }
        if s.starts_with("sandbox:") {
            let sid = s.strip_prefix("sandbox:").unwrap();
            return app_ctx
                .resolve_sandbox_db(sid)
                .await
                .ok_or(format!("Sandbox {} not found", sid));
        }
        if s == "root" {
            return Ok(app_ctx.get_db());
        }
    }

    let scope = app_ctx.get_scope();
    match scope {
        EventScope::Tenant(id) => app_ctx
            .resolve_tenant_db(&id)
            .await
            .ok_or(format!("Current Tenant {} context not found", id)),
        EventScope::Sandbox(id) => app_ctx
            .resolve_sandbox_db(&id)
            .await
            .ok_or(format!("Current Sandbox {} context not found", id)),
        _ => Ok(app_ctx.get_db()),
    }
}

#[derive(Clone, Copy)]
pub enum DbMode {
    Scoped,
    Root,
}

pub fn create_db_object<'js>(
    ctx: &Ctx<'js>,
    app_ctx: Arc<dyn ScriptContext>,
    mode: DbMode,
) -> Result<Object<'js>, String> {
    let db_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // --- 1. RECORDS ---
    let records_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // list(col, opts) OR list(ctx, col, opts)
    let app_list = app_ctx.clone();
    let list_records = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_list.clone();
            async move {
                let (ctx_id, col, opts_val) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let col_name = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let opts = args.0.get(2).cloned();
                        (c_id, col_name, opts)
                    }
                    DbMode::Scoped => {
                        let col_name = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let opts = args.0.get(1).cloned();
                        (None, col_name, opts)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let col_id = resolve_collection_local(db.clone(), &col)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                let mut opts = QueryOptions::default();
                if let Some(v) = opts_val {
                    if let Ok(obj) = from_value::<JsonValue>(v) {
                        if let Some(map) = obj.as_object() {
                            opts.page = map
                                .get("page")
                                .and_then(|v| v.as_f64())
                                .map(|n| n as u64)
                                .or(map.get("page").and_then(|v| v.as_u64()));
                            opts.per_page = map
                                .get("per_page")
                                .and_then(|v| v.as_f64())
                                .map(|n| n as u64)
                                .or(map.get("per_page").and_then(|v| v.as_u64()));
                            opts.limit = map
                                .get("limit")
                                .and_then(|v| v.as_f64())
                                .map(|n| n as u64)
                                .or(map.get("limit").and_then(|v| v.as_u64()));
                            opts.offset = map
                                .get("offset")
                                .and_then(|v| v.as_f64())
                                .map(|n| n as u64)
                                .or(map.get("offset").and_then(|v| v.as_u64()));

                            opts.sort = map.get("sort").and_then(|v| v.as_str()).map(String::from);
                            opts.expand =
                                map.get("expand").and_then(|v| v.as_str()).map(String::from);
                            opts.fields =
                                map.get("fields").and_then(|v| v.as_str()).map(String::from);

                            opts.filter = map.get("filter").map(|v| {
                                if v.is_string() {
                                    v.as_str().unwrap().to_string()
                                } else {
                                    v.to_string()
                                }
                            });
                        }
                    }
                }

                let list = db
                    .list_records(col_id, opts)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &list).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // get(col, id, expand) OR get(ctx, col, id, expand)
    let app_get = app_ctx.clone();
    let get_record = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_get.clone();
            async move {
                let (ctx_id, col, id, expand) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let col_name = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();

                        // Flexible String parsing fallback without hard-locking struct validation
                        let rec_id = match args.0.get(2) {
                            Some(v) if v.is_int() => v.as_int().unwrap() as i64,
                            Some(v) if v.is_float() => v.as_float().unwrap() as i64,
                            Some(v) if v.is_string() => v
                                .as_string()
                                .unwrap()
                                .to_string()
                                .unwrap()
                                .parse()
                                .unwrap_or(0),
                            _ => 0,
                        };

                        let exp = args
                            .0
                            .get(3)
                            .and_then(|v| from_value::<String>(v.clone()).ok());
                        (c_id, col_name, rec_id, exp)
                    }
                    DbMode::Scoped => {
                        let col_name = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();

                        let rec_id = match args.0.get(1) {
                            Some(v) if v.is_int() => v.as_int().unwrap() as i64,
                            Some(v) if v.is_float() => v.as_float().unwrap() as i64,
                            Some(v) if v.is_string() => v
                                .as_string()
                                .unwrap()
                                .to_string()
                                .unwrap()
                                .parse()
                                .unwrap_or(0),
                            _ => 0,
                        };

                        let exp = args
                            .0
                            .get(2)
                            .and_then(|v| from_value::<String>(v.clone()).ok());
                        (None, col_name, rec_id, exp)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let col_id = resolve_collection_local(db.clone(), &col)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let rec = db
                    .get_record(col_id, id, expand)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &rec).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // create(col, data) OR create(ctx, col, data)
    let app_create = app_ctx.clone();
    let create_record = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_create.clone();
            async move {
                let (ctx_id, col, data_val) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let col_name = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let d = args.0.get(2).cloned();
                        (c_id, col_name, d)
                    }
                    DbMode::Scoped => {
                        let col_name = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let d = args.0.get(1).cloned();
                        (None, col_name, d)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let col_id = resolve_collection_local(db.clone(), &col)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                let data: JsonValue = data_val
                    .and_then(|v| from_value(v).ok())
                    .unwrap_or(json!({}));
                let id = db
                    .create_record(col_id, &data)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                let res = json!({ "id": id });
                to_value(js_ctx, &res).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // update(col, id, data) OR update(ctx, col, id, data)
    let app_update = app_ctx.clone();
    let update_record = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_update.clone();
            async move {
                let (ctx_id, col, id, data_val) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let col_name = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();

                        let rec_id = match args.0.get(2) {
                            Some(v) if v.is_int() => v.as_int().unwrap() as i64,
                            Some(v) if v.is_float() => v.as_float().unwrap() as i64,
                            Some(v) if v.is_string() => v
                                .as_string()
                                .unwrap()
                                .to_string()
                                .unwrap()
                                .parse()
                                .unwrap_or(0),
                            _ => 0,
                        };

                        let d = args.0.get(3).cloned();
                        (c_id, col_name, rec_id, d)
                    }
                    DbMode::Scoped => {
                        let col_name = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();

                        let rec_id = match args.0.get(1) {
                            Some(v) if v.is_int() => v.as_int().unwrap() as i64,
                            Some(v) if v.is_float() => v.as_float().unwrap() as i64,
                            Some(v) if v.is_string() => v
                                .as_string()
                                .unwrap()
                                .to_string()
                                .unwrap()
                                .parse()
                                .unwrap_or(0),
                            _ => 0,
                        };

                        let d = args.0.get(2).cloned();
                        (None, col_name, rec_id, d)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let col_id = resolve_collection_local(db.clone(), &col)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                let data: JsonValue = data_val
                    .and_then(|v| from_value(v).ok())
                    .unwrap_or(json!({}));
                let rec = db
                    .update_record(col_id, id, &data)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &rec).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // delete(col, id) OR delete(ctx, col, id)
    let app_delete = app_ctx.clone();
    let delete_record = Function::new(
        ctx.clone(),
        Async(move |args: Rest<Value<'js>>| {
            let app = app_delete.clone();
            async move {
                let (ctx_id, col, id) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let col_name = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();

                        let rec_id = match args.0.get(2) {
                            Some(v) if v.is_int() => v.as_int().unwrap() as i64,
                            Some(v) if v.is_float() => v.as_float().unwrap() as i64,
                            Some(v) if v.is_string() => v
                                .as_string()
                                .unwrap()
                                .to_string()
                                .unwrap()
                                .parse()
                                .unwrap_or(0),
                            _ => 0,
                        };
                        (c_id, col_name, rec_id)
                    }
                    DbMode::Scoped => {
                        let col_name = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();

                        let rec_id = match args.0.get(1) {
                            Some(v) if v.is_int() => v.as_int().unwrap() as i64,
                            Some(v) if v.is_float() => v.as_float().unwrap() as i64,
                            Some(v) if v.is_string() => v
                                .as_string()
                                .unwrap()
                                .to_string()
                                .unwrap()
                                .parse()
                                .unwrap_or(0),
                            _ => 0,
                        };
                        (None, col_name, rec_id)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let col_id = resolve_collection_local(db.clone(), &col)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                db.delete_record(col_id, id)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // searchVector(col, field, vec, limit)
    let app_search_vec = app_ctx.clone();
    let search_vector = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_search_vec.clone();
            async move {
                let (ctx_id, col, field, vec_val, limit) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let col_name = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let f = args
                            .0
                            .get(2)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let v = args.0.get(3).cloned();
                        let l = args
                            .0
                            .get(4)
                            .and_then(|v| from_value::<usize>(v.clone()).ok())
                            .unwrap_or(10);
                        (c_id, col_name, f, v, l)
                    }
                    DbMode::Scoped => {
                        let col_name = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let f = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let v = args.0.get(2).cloned();
                        let l = args
                            .0
                            .get(3)
                            .and_then(|v| from_value::<usize>(v.clone()).ok())
                            .unwrap_or(10);
                        (None, col_name, f, v, l)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let col_id = resolve_collection_local(db.clone(), &col)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                let vector: Vec<f32> = vec_val.and_then(|v| from_value(v).ok()).unwrap_or_default();
                let recs = db
                    .search_vector(col_id, &field, vector, limit)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &recs).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // getVector(col, id)
    let app_get_vec = app_ctx.clone();
    let get_vector = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_get_vec.clone();
            async move {
                let (ctx_id, col, id) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let col_name = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();

                        let rec_id = match args.0.get(2) {
                            Some(v) if v.is_int() => v.as_int().unwrap() as i64,
                            Some(v) if v.is_float() => v.as_float().unwrap() as i64,
                            Some(v) if v.is_string() => v
                                .as_string()
                                .unwrap()
                                .to_string()
                                .unwrap()
                                .parse()
                                .unwrap_or(0),
                            _ => 0,
                        };
                        (c_id, col_name, rec_id)
                    }
                    DbMode::Scoped => {
                        let col_name = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();

                        let rec_id = match args.0.get(1) {
                            Some(v) if v.is_int() => v.as_int().unwrap() as i64,
                            Some(v) if v.is_float() => v.as_float().unwrap() as i64,
                            Some(v) if v.is_string() => v
                                .as_string()
                                .unwrap()
                                .to_string()
                                .unwrap()
                                .parse()
                                .unwrap_or(0),
                            _ => 0,
                        };
                        (None, col_name, rec_id)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let col_id = resolve_collection_local(db.clone(), &col)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                let vecs = db
                    .get_record_vectors(col_id, id)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &vecs).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // instantSearch(col, query, limit)
    let app_instant_search = app_ctx.clone();
    let instant_search = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_instant_search.clone();
            async move {
                let (ctx_id, col, query, limit) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let col_name = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let q = args
                            .0
                            .get(2)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let l = args
                            .0
                            .get(3)
                            .and_then(|v| from_value::<usize>(v.clone()).ok())
                            .unwrap_or(10);
                        (c_id, col_name, q, l)
                    }
                    DbMode::Scoped => {
                        let col_name = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let q = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let l = args
                            .0
                            .get(2)
                            .and_then(|v| from_value::<usize>(v.clone()).ok())
                            .unwrap_or(10);
                        (None, col_name, q, l)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let col_id = resolve_collection_local(db.clone(), &col)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                let results = db
                    .instant_search(col_id, &query, limit)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &results).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    records_obj
        .set("list", list_records)
        .map_err(|e| e.to_string())?;
    records_obj
        .set("get", get_record)
        .map_err(|e| e.to_string())?;
    records_obj
        .set("create", create_record)
        .map_err(|e| e.to_string())?;
    records_obj
        .set("update", update_record)
        .map_err(|e| e.to_string())?;
    records_obj
        .set("delete", delete_record)
        .map_err(|e| e.to_string())?;
    records_obj
        .set("searchVector", search_vector)
        .map_err(|e| e.to_string())?;
    records_obj
        .set("getVector", get_vector)
        .map_err(|e| e.to_string())?;
    records_obj
        .set("instantSearch", instant_search)
        .map_err(|e| e.to_string())?;

    // --- 2. QUERY ---
    let app_query = app_ctx.clone();
    let query_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_query.clone();
            async move {
                let (ctx_id, q_val) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let q = args.0.get(1).cloned();
                        (c_id, q)
                    }
                    DbMode::Scoped => {
                        if args.0.len() >= 2
                            && args.0.first().map(|v| v.is_string()).unwrap_or(false)
                        {
                            let c_id = args
                                .0
                                .get(0)
                                .and_then(|v| v.as_string())
                                .map(|s| s.to_string().unwrap_or_default());
                            let q = args.0.get(1).cloned();
                            (c_id, q)
                        } else {
                            let q = args.0.get(0).cloned();
                            (None, q)
                        }
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                let q_json: JsonValue = q_val.and_then(|v| from_value(v).ok()).unwrap_or(json!({}));
                let query: ApexQuery =
                    serde_json::from_value(q_json).map_err(|_| rquickjs::Error::Exception)?;

                let res = db
                    .query_engine(query)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                to_value(js_ctx, &res).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // --- 3. FILES ---
    let files_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;
    let app_files = app_ctx.clone();
    let files_list = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_files.clone();
            async move {
                let (ctx_id, limit, offset) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let l = args
                            .0
                            .get(1)
                            .and_then(|v| from_value::<i64>(v.clone()).ok())
                            .unwrap_or(20);
                        let o = args
                            .0
                            .get(2)
                            .and_then(|v| from_value::<i64>(v.clone()).ok())
                            .unwrap_or(0);
                        (c_id, l, o)
                    }
                    DbMode::Scoped => {
                        let l = args
                            .0
                            .get(0)
                            .and_then(|v| from_value::<i64>(v.clone()).ok())
                            .unwrap_or(20);
                        let o = args
                            .0
                            .get(1)
                            .and_then(|v| from_value::<i64>(v.clone()).ok())
                            .unwrap_or(0);
                        (None, l, o)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let files = db
                    .list_files(limit, offset)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &files).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    files_obj
        .set("list", files_list)
        .map_err(|e| e.to_string())?;

    // --- 4. USERS ---
    let users_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let app_u_get = app_ctx.clone();
    let users_get = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_u_get.clone();
            async move {
                let (ctx_id, email) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let e = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        (c_id, e)
                    }
                    DbMode::Scoped => {
                        let e = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        (None, e)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let u = db
                    .get_user_by_email(&email)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &u).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    let app_u_create = app_ctx.clone();
    let users_create = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_u_create.clone();
            async move {
                let (ctx_id, email, pass, role) = match mode {
                    DbMode::Root => {
                        let c_id = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default());
                        let e = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let p = args
                            .0
                            .get(2)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let r = args
                            .0
                            .get(3)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        (c_id, e, p, r)
                    }
                    DbMode::Scoped => {
                        let e = args
                            .0
                            .get(0)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let p = args
                            .0
                            .get(1)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        let r = args
                            .0
                            .get(2)
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string().unwrap_or_default())
                            .unwrap_or_default();
                        (None, e, p, r)
                    }
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let hash =
                    crate::auth::hash_password(&pass).map_err(|_| rquickjs::Error::Exception)?;
                let u = db
                    .create_user(&email, &hash, &role, None)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &u).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    users_obj.set("get", users_get).map_err(|e| e.to_string())?;
    users_obj
        .set("create", users_create)
        .map_err(|e| e.to_string())?;

    // --- 5. COLLECTIONS ---
    let cols_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;
    let app_cols = app_ctx.clone();
    let cols_list = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, args: Rest<Value<'js>>| {
            let app = app_cols.clone();
            async move {
                let ctx_id = match mode {
                    DbMode::Root => args
                        .0
                        .get(0)
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string().unwrap_or_default()),
                    DbMode::Scoped => None,
                };

                let db = resolve_db(ctx_id, app.clone())
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;
                let cols = db
                    .list_collections()
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &cols).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    cols_obj.set("list", cols_list).map_err(|e| e.to_string())?;

    // Build Final Object
    db_obj
        .set("records", records_obj)
        .map_err(|e| e.to_string())?;
    db_obj.set("users", users_obj).map_err(|e| e.to_string())?;
    db_obj
        .set("collections", cols_obj)
        .map_err(|e| e.to_string())?;
    db_obj.set("files", files_obj).map_err(|e| e.to_string())?;
    db_obj.set("query", query_fn).map_err(|e| e.to_string())?;

    Ok(db_obj)
}

pub fn register_db<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let db_obj = create_db_object(ctx, app_ctx, DbMode::Scoped)?;
    globals.set("$db", db_obj).map_err(|e| e.to_string())?;
    Ok(())
}
