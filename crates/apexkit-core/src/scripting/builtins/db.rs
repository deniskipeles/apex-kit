use super::super::ScriptContext;
use crate::Db;
use crate::query::ApexQuery;
use crate::query::QueryOptions;
use crate::realtime::EventScope;
use crate::scripting::{ACTIVE_CONTEXT, return_json_promise};
use boa_engine::{
    Context, JsArgs, JsString, JsValue, NativeFunction, object::ObjectInitializer,
    property::Attribute,
};
use std::sync::Arc;

// --- HELPERS ---

// Helper to resolve Collection ID from Name or ID String
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

// Helper to resolve Database based on Context ID string OR Current Scope
pub async fn resolve_db(
    ctx_str: Option<String>,
    app_ctx: Arc<dyn ScriptContext>,
) -> Result<Arc<dyn Db>, String> {
    // 1. Explicit Context Passed in JS (e.g. $root.db.records.list("tenant:123", ...))
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
        // If "root" passed explicitly
        if s == "root" {
            return Ok(app_ctx.get_db()); // Returns Root DB (since get_db defaults to Root in ScopedScriptContext)
        }
    }

    // 2. Implicit Context based on Execution Scope
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
        // Root scope or Channel scope falls back to default (Root DB)
        _ => Ok(app_ctx.get_db()),
    }
}

// --- MODES ---

#[derive(Clone, Copy)]
pub enum DbMode {
    Scoped, // $db: No context arg, uses current scope automatically
    Root,   // $root.db: First arg IS context ID
}

// --- BUILDER ---

pub fn create_db_object(
    ctx: &mut Context,
    mode: DbMode,
) -> Result<boa_engine::object::JsObject, String> {
    // Helper to handle arguments based on mode
    // Returns (context_id_string, offset_index)
    // For Scoped: context_id = None, offset = 0
    // For Root: context_id = args[0], offset = 1
    let get_args = |args: &[JsValue], _ctx: &mut Context, m: &DbMode| -> (Option<String>, usize) {
        match m {
            DbMode::Scoped => (None, 0),
            DbMode::Root => {
                let id = args
                    .first()
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_std_string_escaped());
                (id, 1)
            }
        }
    };

    // --- 1. RECORDS ---

    // list(col, opts) OR list(ctx, col, opts)
    let list_records = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let col = args
                .get_or_undefined(offset)
                .to_string(ctx)?
                .to_std_string_escaped();
            let opts_val = args
                .get_or_undefined(offset + 1)
                .to_json(ctx)
                .unwrap()
                .unwrap_or(serde_json::Value::Null);

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        
                        // Robustly parse JS floats (f64) into Rust Integers (u64)
                        let mut opts = QueryOptions::default();
                        if let Some(obj) = opts_val.as_object() {
                            opts.page = obj.get("page").and_then(|v| v.as_f64()).map(|n| n as u64).or(obj.get("page").and_then(|v| v.as_u64()));
                            opts.per_page = obj.get("per_page").and_then(|v| v.as_f64()).map(|n| n as u64).or(obj.get("per_page").and_then(|v| v.as_u64()));
                            opts.limit = obj.get("limit").and_then(|v| v.as_f64()).map(|n| n as u64).or(obj.get("limit").and_then(|v| v.as_u64()));
                            opts.offset = obj.get("offset").and_then(|v| v.as_f64()).map(|n| n as u64).or(obj.get("offset").and_then(|v| v.as_u64()));
                            
                            opts.sort = obj.get("sort").and_then(|v| v.as_str()).map(String::from);
                            opts.expand = obj.get("expand").and_then(|v| v.as_str()).map(String::from);
                            opts.fields = obj.get("fields").and_then(|v| v.as_str()).map(String::from);
                            
                            opts.filter = obj.get("filter").map(|v| {
                                if v.is_string() { v.as_str().unwrap().to_string() } else { v.to_string() }
                            });
                        }

                        let list = db
                            .list_records(col_id, opts)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(list).unwrap())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    // get(col, id, expand) OR get(ctx, col, id, expand)
    let get_record = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let col = args
                .get_or_undefined(offset)
                .to_string(ctx)?
                .to_std_string_escaped();
            let id = args
                .get_or_undefined(offset + 1)
                .to_number(ctx)
                .unwrap_or(0.0) as i64;
            let expand = args
                .get(offset + 2)
                .and_then(|v| v.as_string())
                .map(|s| s.to_std_string_escaped());

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let rec = db
                            .get_record(col_id, id, expand)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(rec).unwrap_or(serde_json::Value::Null))
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    // create(col, data) OR create(ctx, col, data)
    let create_record = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let col = args
                .get_or_undefined(offset)
                .to_string(ctx)?
                .to_std_string_escaped();
            let data = args
                .get_or_undefined(offset + 1)
                .to_json(ctx)
                .unwrap()
                .unwrap_or(serde_json::Value::Null);

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let id = db
                            .create_record(col_id, &data)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::json!({ "id": id }))
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    // update(col, id, data) OR update(ctx, col, id, data)
    let update_record = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let col = args
                .get_or_undefined(offset)
                .to_string(ctx)?
                .to_std_string_escaped();
            let id = args
                .get_or_undefined(offset + 1)
                .to_number(ctx)
                .unwrap_or(0.0) as i64;
            let data = args
                .get_or_undefined(offset + 2)
                .to_json(ctx)
                .unwrap()
                .unwrap_or(serde_json::Value::Null);

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let rec = db
                            .update_record(col_id, id, &data)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(rec).unwrap())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    // delete(col, id) OR delete(ctx, col, id)
    let delete_record = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let col = args
                .get_or_undefined(offset)
                .to_string(ctx)?
                .to_std_string_escaped();
            let id = args
                .get_or_undefined(offset + 1)
                .to_number(ctx)
                .unwrap_or(0.0) as i64;

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        db.delete_record(col_id, id)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::Value::Bool(true))
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    // searchVector(col, field, vec, limit)
    let search_vector = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let col = args
                .get_or_undefined(offset)
                .to_string(ctx)?
                .to_std_string_escaped();
            let field = args
                .get_or_undefined(offset + 1)
                .to_string(ctx)?
                .to_std_string_escaped();
            let vec_val = args
                .get_or_undefined(offset + 2)
                .to_json(ctx)
                .unwrap()
                .unwrap_or(serde_json::Value::Null);
            let limit = args
                .get_or_undefined(offset + 3)
                .to_number(ctx)
                .unwrap_or(10.0) as usize;

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let vector: Vec<f32> = serde_json::from_value(vec_val).unwrap_or_default();
                        let recs = db
                            .search_vector(col_id, &field, vector, limit)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(recs).unwrap())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    // getVector(col, id)
    let get_vector = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let col = args
                .get_or_undefined(offset)
                .to_string(ctx)?
                .to_std_string_escaped();
            let id = args
                .get_or_undefined(offset + 1)
                .to_number(ctx)
                .unwrap_or(0.0) as i64;

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let vecs = db
                            .get_record_vectors(col_id, id)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(vecs).unwrap())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    // instantSearch(col, query, limit) OR instantSearch(ctx, col, query, limit)
    let instant_search = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let col = args
                .get_or_undefined(offset)
                .to_string(ctx)?
                .to_std_string_escaped();
            let query = args
                .get_or_undefined(offset + 1)
                .to_string(ctx)?
                .to_std_string_escaped();
            let limit = args
                .get_or_undefined(offset + 2)
                .to_number(ctx)
                .unwrap_or(10.0) as usize;

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;

                        // Call the Tantivy instant search engine
                        let results = db
                            .instant_search(col_id, &query, limit)
                            .await
                            .map_err(|e| e.to_string())?;

                        Ok(serde_json::to_value(results).unwrap())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    let records_obj = ObjectInitializer::new(ctx)
        .function(list_records, JsString::from("list"), 2)
        .function(get_record, JsString::from("get"), 3)
        .function(create_record, JsString::from("create"), 2)
        .function(update_record, JsString::from("update"), 3)
        .function(delete_record, JsString::from("delete"), 2)
        .function(search_vector, JsString::from("searchVector"), 4)
        .function(get_vector, JsString::from("getVector"), 2)
        .function(instant_search, JsString::from("instantSearch"), 3)
        .build();

    // --- 2. QUERY ---
    let query_fn = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            // Robust Argument Parsing Logic
            let (ctx_id, q_val) = match m {
                DbMode::Root => {
                    // Root mode: explicit context required as first arg
                    let id = args
                        .first()
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_std_string_escaped());
                    let query = args
                        .get_or_undefined(1)
                        .to_json(ctx)
                        .unwrap()
                        .unwrap_or(serde_json::Value::Null);
                    (id, query)
                }
                DbMode::Scoped => {
                    // Scoped mode: check if user is trying to switch context dynamically
                    // If arg[0] is string and arg[1] is object -> It's a context switch
                    // If arg[0] is object -> It's a standard query
                    if args.len() >= 2 && args.first().map(|v| v.is_string()).unwrap_or(false) {
                        let id = args
                            .first()
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_std_string_escaped());
                        let query = args
                            .get_or_undefined(1)
                            .to_json(ctx)
                            .unwrap()
                            .unwrap_or(serde_json::Value::Null);
                        (id, query)
                    } else {
                        let query = args
                            .get_or_undefined(0)
                            .to_json(ctx)
                            .unwrap()
                            .unwrap_or(serde_json::Value::Null);
                        (None, query)
                    }
                }
            };

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        // Resolve the DB based on the optional context ID
                        let db = resolve_db(ctx_id, app.clone()).await?;

                        let query: ApexQuery = serde_json::from_value(q_val)
                            .map_err(|e| format!("Query Parse Error: {}", e))?;

                        // Execute on the resolved DB
                        db.query_engine(query).await.map_err(|e| e.to_string())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    // --- 3. FILES ---
    let files_list = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let limit = args.get_or_undefined(offset).to_number(ctx).unwrap_or(20.0) as i64;
            let offset_val = args
                .get_or_undefined(offset + 1)
                .to_number(ctx)
                .unwrap_or(0.0) as i64;

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let files = db
                            .list_files(limit, offset_val)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(files).unwrap())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    let files_obj = ObjectInitializer::new(ctx)
        .function(files_list, JsString::from("list"), 2)
        .build();

    // --- 4. USERS ---
    let users_get = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let email = args
                .get_or_undefined(offset)
                .to_string(ctx)?
                .to_std_string_escaped();

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let u = db
                            .get_user_by_email(&email)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(u).unwrap_or(serde_json::Value::Null))
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    let users_create = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, offset) = get_args(args, ctx, &m);
            let email = args
                .get_or_undefined(offset)
                .to_string(ctx)?
                .to_std_string_escaped();
            let pass = args
                .get_or_undefined(offset + 1)
                .to_string(ctx)?
                .to_std_string_escaped();
            let role = args
                .get_or_undefined(offset + 2)
                .to_string(ctx)?
                .to_std_string_escaped();

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        // Note: We need to access hash_password which is in auth module of core.
                        // But we are in core, so we can use crate::auth::hash_password
                        let hash = crate::auth::hash_password(&pass).map_err(|e| e.to_string())?;
                        let u = db
                            .create_user(&email, &hash, &role, None)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(u).unwrap())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    let users_obj = ObjectInitializer::new(ctx)
        .function(users_get, JsString::from("get"), 1)
        .function(users_create, JsString::from("create"), 3)
        .build();

    // --- 5. COLLECTIONS ---
    let cols_list = {
        let m = mode;
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let (ctx_id, _offset) = get_args(args, ctx, &m);
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let cols = db.list_collections().await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(cols).unwrap())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        })
    };

    let cols_obj = ObjectInitializer::new(ctx)
        .function(cols_list, JsString::from("list"), 0)
        .build();

    // --- BUILD FINAL OBJECT ---
    let db_obj = ObjectInitializer::new(ctx)
        .property(JsString::from("records"), records_obj, Attribute::all())
        .property(JsString::from("users"), users_obj, Attribute::all())
        .property(JsString::from("collections"), cols_obj, Attribute::all())
        .property(JsString::from("files"), files_obj, Attribute::all())
        .function(query_fn, JsString::from("query"), 1)
        .build();

    Ok(db_obj)
}

// Default registration for $db (Scoped Mode)
pub fn register_db(ctx: &mut Context) -> Result<(), String> {
    let db_obj = create_db_object(ctx, DbMode::Scoped)?;
    ctx.register_global_property(JsString::from("$db"), db_obj, Attribute::all())
        .map_err(|e| e.to_string())
}
