// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/scripting.rs ===========================
use std::sync::Arc;
use serde_json::Value as JsonValue;
use crate::{Db, query::QueryOptions, embeddings::{EmbedderService, EmbedderProvider}, VectorProvider};

// Boa Imports
use boa_engine::{
    Context, JsValue, Source, JsResult, NativeFunction, JsError,
    object::ObjectInitializer, property::Attribute,
    JsString, JsArgs,
    builtins::promise::PromiseState
};

// --- JAVASCRIPT PRELUDE ---
const JS_PRELUDE: &str = r#"
    class Headers {
        constructor(init = {}) {
            this.map = new Map(Object.entries(init));
        }
        get(name) {
            for (const [k, v] of this.map) {
                if (k.toLowerCase() === name.toLowerCase()) return v;
            }
            return null;
        }
        set(name, value) { this.map.set(name, value); }
        entries() { return this.map.entries(); }
    }

    class Request {
        constructor(input, init = {}) {
            this.bodyData = init.body || input?.body || null;
            this.method = init.method || "GET";
            this.headers = new Headers(init.headers || {});
        }
        async json() { return this.bodyData; }
        async text() { return JSON.stringify(this.bodyData); }
    }

    class Response {
        constructor(body, init = {}) {
            this.body = body;
            this.status = init.status || 200;
            this.headers = new Headers(init.headers || {});
        }
    }

    globalThis.Headers = Headers;
    globalThis.Request = Request;
    globalThis.Response = Response;
"#;

#[derive(Clone)]
pub struct ScriptEngine;

impl ScriptEngine {
    pub async fn new() -> Self {
        Self
    }

    pub async fn run_script(
        &self, 
        code: &str, 
        input_data: JsonValue, 
        db: Arc<dyn Db>,
        embedder: Arc<EmbedderService>,
        vector_provider: Arc<dyn VectorProvider>
    ) -> Result<JsonValue, String> {
        
        let code_owned = code.to_string();
        // Replace export default with a global assignment so we can call it
        let processed_code = code_owned.replacen("export default", "globalThis.__mainHandler =", 1);
        let handle = tokio::runtime::Handle::current();
        
        let result = tokio::task::spawn_blocking(move || -> Result<JsonValue, String> {
            let mut context = Context::default();

            // --- A. LOAD PRELUDE ---
            context.eval(Source::from_bytes(JS_PRELUDE.as_bytes()))
                .map_err(|e| format!("Prelude Error: {}", e))?;

            // --- B. SETUP HELPERS ---
            // Pass db, embedder, and vector_provider to thread local storage
            ACTIVE_DB_CONTEXT.with(|c| { 
                *c.borrow_mut() = Some((
                    db.clone(), 
                    embedder.clone(), 
                    vector_provider.clone(), 
                    handle.clone()
                )); 
            });

            // Helper to create resolved promises for Async JS functions
            let return_promise = |ctx: &mut Context, val: JsResult<JsValue>| -> JsResult<JsValue> {
                let (promise, resolvers) = boa_engine::object::builtins::JsPromise::new_pending(ctx);
                match val {
                    Ok(v) => resolvers.resolve.call(&JsValue::undefined(), &[v], ctx)?,
                    Err(e) => resolvers.reject.call(&JsValue::undefined(), &[e.to_opaque(ctx)], ctx)?,
                };
                Ok(promise.into())
            };

            // 1. log()
            let log_fn = NativeFunction::from_fn_ptr(|_, args, ctx| {
                let msg = args.get(0).map(|v| v.to_string(ctx).unwrap_or_default().to_std_string_escaped()).unwrap_or_default();
                println!("[JS LOG]: {}", msg);
                Ok(JsValue::undefined())
            });
            context.register_global_callable(JsString::from("log"), 1, log_fn).unwrap();

            // 2. $util.uuid()
            let uuid_fn = NativeFunction::from_fn_ptr(|_, _, _| {
                Ok(JsValue::from(JsString::from(uuid::Uuid::new_v4().to_string())))
            });
            let util_obj = ObjectInitializer::new(&mut context)
                .function(uuid_fn, JsString::from("uuid"), 0)
                .build();
            context.register_global_property(JsString::from("$util"), util_obj, Attribute::all()).unwrap();

            // 3. $http (Blocking reqwest inside the thread)
            let http_get = NativeFunction::from_copy_closure(move |_, args, ctx| {
                let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let res = reqwest::blocking::get(&url)
                    .map_err(|e| JsError::from_opaque(JsString::from(e.to_string()).into()))?;
                let text = res.text().unwrap_or_default();
                return_promise(ctx, Ok(JsValue::from(JsString::from(text))))
            });

            let http_post = NativeFunction::from_copy_closure(move |_, args, ctx| {
                let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let body = args.get_or_undefined(1).to_json(ctx).unwrap_or(None).unwrap_or(serde_json::Value::Null);
                
                let client = reqwest::blocking::Client::new();
                let res = client.post(&url).json(&body).send()
                    .map_err(|e| JsError::from_opaque(JsString::from(e.to_string()).into()))?;
                let text = res.text().unwrap_or_default();
                return_promise(ctx, Ok(JsValue::from(JsString::from(text))))
            });

            let http_obj = ObjectInitializer::new(&mut context)
                .function(http_get, JsString::from("get"), 1)
                .function(http_post, JsString::from("post"), 2)
                .build();
            context.register_global_property(JsString::from("$http"), http_obj, Attribute::all()).unwrap();

            // 4. $db (CRUD)
            
            let db_find_one = NativeFunction::from_copy_closure(move |_, args, ctx| {
                let col = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let id = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0) as i64;

                let result = ACTIVE_DB_CONTEXT.with(|c| {
                    if let Some((db, _, _, handle)) = &*c.borrow() {
                        handle.block_on(async {
                            if let Ok(cols) = db.list_collections().await {
                                if let Some(c) = cols.iter().find(|x| x.name == col) {
                                    if let Ok(Some(r)) = db.get_record(c.id, id, None).await {
                                        let mut d = r.data.clone();
                                        if let Some(o) = d.as_object_mut() { o.insert("id".to_string(), serde_json::json!(r.id)); }
                                        return Some(d);
                                    }
                                }
                            }
                            None
                        })
                    } else { None }
                });
                
                let js_val = match result {
                    Some(v) => JsValue::from_json(&v, ctx).unwrap(),
                    None => JsValue::null()
                };
                return_promise(ctx, Ok(js_val))
            });

            let db_find = NativeFunction::from_copy_closure(move |_, args, ctx| {
                let col = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                
                let filter_str = if let Some(val) = args.get(1) {
                    if !val.is_undefined() && !val.is_null() {
                        let json = val.to_json(ctx).unwrap_or(None).unwrap_or(serde_json::Value::Null);
                        Some(json.to_string())
                    } else { None }
                } else { None };

                let result_vec = ACTIVE_DB_CONTEXT.with(|c| {
                    if let Some((db, _, _, handle)) = &*c.borrow() {
                        handle.block_on(async {
                            if let Ok(cols) = db.list_collections().await {
                                if let Some(c) = cols.iter().find(|x| x.name == col) {
                                    let opts = QueryOptions { filter: filter_str, ..Default::default() };
                                    if let Ok(result) = db.list_records(c.id, opts).await {
                                        // Map ListResult.items to JSON
                                        let mapped: Vec<JsonValue> = result.items.into_iter().map(|r| {
                                            let mut d = r.data;
                                            if let Some(o) = d.as_object_mut() { o.insert("id".to_string(), serde_json::json!(r.id)); }
                                            d
                                        }).collect();
                                        return Some(mapped);
                                    }
                                }
                            }
                            None
                        })
                    } else { None }
                });

                let js_val = match result_vec {
                    Some(v) => JsValue::from_json(&serde_json::Value::Array(v), ctx).unwrap(),
                    None => JsValue::from_json(&serde_json::Value::Array(vec![]), ctx).unwrap()
                };
                return_promise(ctx, Ok(js_val))
            });

            let db_insert = NativeFunction::from_copy_closure(move |_, args, ctx| {
                let col = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let data = args.get_or_undefined(1).to_json(ctx).unwrap_or(None).unwrap_or(serde_json::Value::Null);

                let new_id = ACTIVE_DB_CONTEXT.with(|c| {
                    if let Some((db, _, _, handle)) = &*c.borrow() {
                        handle.block_on(async {
                            if let Ok(cols) = db.list_collections().await {
                                if let Some(c) = cols.iter().find(|x| x.name == col) {
                                    if let Ok(id) = db.create_record(c.id, &data).await {
                                        return Some(id);
                                    }
                                }
                            }
                            None
                        })
                    } else { None }
                });

                let res = match new_id {
                    Some(id) => JsValue::from(id),
                    None => JsValue::null()
                };
                return_promise(ctx, Ok(res))
            });

            let db_update = NativeFunction::from_copy_closure(move |_, args, ctx| {
                let col = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let id = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0) as i64;
                let data = args.get_or_undefined(2).to_json(ctx).unwrap_or(None).unwrap_or(serde_json::Value::Null);

                let updated_record = ACTIVE_DB_CONTEXT.with(|c| {
                    if let Some((db, _, _, handle)) = &*c.borrow() {
                        handle.block_on(async {
                            if let Ok(cols) = db.list_collections().await {
                                if let Some(c) = cols.iter().find(|x| x.name == col) {
                                    if let Ok(rec) = db.update_record(c.id, id, &data).await {
                                        let mut d = rec.data;
                                        if let Some(o) = d.as_object_mut() { o.insert("id".to_string(), serde_json::json!(rec.id)); }
                                        return Some(d);
                                    }
                                }
                            }
                            None
                        })
                    } else { None }
                });

                let js_val = match updated_record {
                    Some(v) => JsValue::from_json(&v, ctx).unwrap(),
                    None => JsValue::null()
                };
                return_promise(ctx, Ok(js_val))
            });

            let db_delete = NativeFunction::from_copy_closure(move |_, args, ctx| {
                let col = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let id = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0) as i64;

                let success = ACTIVE_DB_CONTEXT.with(|c| {
                    if let Some((db, _, _, handle)) = &*c.borrow() {
                        handle.block_on(async {
                            if let Ok(cols) = db.list_collections().await {
                                if let Some(c) = cols.iter().find(|x| x.name == col) {
                                    if db.delete_record(c.id, id).await.is_ok() {
                                        return true;
                                    }
                                }
                            }
                            false
                        })
                    } else { false }
                });

                return_promise(ctx, Ok(JsValue::from(success)))
            });
            
            let db_obj = ObjectInitializer::new(&mut context)
                .function(db_find_one, JsString::from("find_one"), 2)
                .function(db_find, JsString::from("find"), 2)
                .function(db_insert, JsString::from("insert"), 2)
                .function(db_update, JsString::from("update"), 3)
                .function(db_delete, JsString::from("delete"), 2)
                .build();
            context.register_global_property(JsString::from("$db"), db_obj, Attribute::all()).unwrap();

            // 5. $ai (Embedder)
            // Usage: await $ai.embed("hello world", "hf")
            let ai_embed = NativeFunction::from_copy_closure(move |_, args, ctx| {
                let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let provider_str = args.get_or_undefined(1).as_string().map(|s| s.to_std_string_escaped()).unwrap_or("local".to_string());

                let (embedding, error) = ACTIVE_DB_CONTEXT.with(|c| {
                    if let Some((db, embedder, vector_provider, handle)) = &*c.borrow() {
                        handle.block_on(async {
                            
                            let provider_lower = provider_str.to_lowercase();
                            
                            // A. Local Provider (ApexVector via VectorProvider trait)
                            if provider_lower == "local" {
                                match vector_provider.embed(&text).await {
                                    Ok(vec) => (Some(vec), None),
                                    Err(e) => (None, Some(e))
                                }
                            } else {
                                // B. Remote Provider (Gemini/HF via EmbedderService)
                                // Fetch API key from settings
                                let settings = db.get_setting("ai").await.unwrap_or(None);
                                let mut api_key = None;
                                if let Some(val) = settings {
                                    if let Some(key) = val.get("api_key").and_then(|v| v.as_str()) {
                                        // In prod, decrypt here.
                                        api_key = Some(key.to_string()); 
                                    }
                                }

                                let provider_enum = match provider_lower.as_str() {
                                    "hf" | "huggingface" => EmbedderProvider::HuggingFace,
                                    "gemini" | "google" => EmbedderProvider::Gemini,
                                    _ => EmbedderProvider::Local, // Fallback
                                };

                                match embedder.generate(&text, provider_enum, api_key).await {
                                    Ok(vec) => (Some(vec), None),
                                    Err(e) => (None, Some(e))
                                }
                            }
                        })
                    } else { (None, Some("Context lost".to_string())) }
                });

                if let Some(err) = error {
                    return Err(JsError::from_opaque(JsString::from(err).into()));
                }
                
                // Convert Vec<f32> to JS Array
                let js_array = boa_engine::object::builtins::JsArray::new(ctx);
                if let Some(vec) = embedding {
                    for (i, val) in vec.iter().enumerate() {
                        // FIX: Added 'true' for strict mode argument in set()
                        js_array.set(i, JsValue::from(*val), true, ctx)?;
                    }
                }
                
                return_promise(ctx, Ok(js_array.into()))
            });

            let ai_obj = ObjectInitializer::new(&mut context)
                .function(ai_embed, JsString::from("embed"), 2)
                .build();
            context.register_global_property(JsString::from("$ai"), ai_obj, Attribute::all()).unwrap();

            // --- C. EXECUTE USER SCRIPT ---
            if let Err(e) = context.eval(Source::from_bytes(processed_code.as_bytes())) {
                return Err(format!("Script Syntax Error: {}", e));
            }

            // --- D. INVOKE EXPORTED HANDLER ---
            let js_body = JsValue::from_json(&input_data, &mut context).map_err(|e| e.to_string())?;
            
            let request_cls = context.global_object().get(JsString::from("Request"), &mut context).unwrap();
            let req_init = ObjectInitializer::new(&mut context)
                .property(JsString::from("method"), JsString::from("POST"), Attribute::all())
                .property(JsString::from("body"), js_body, Attribute::all())
                .build();
            
            let request_obj = request_cls.as_constructor().unwrap()
                .construct(
                    &[JsValue::undefined(), JsValue::from(req_init)], 
                    Some(&request_cls.as_object().unwrap()), 
                    &mut context
                )
                .map_err(|e| format!("Failed to create Request: {}", e))?;

            let handler = context.global_object().get(JsString::from("__mainHandler"), &mut context);
            
            let promise = match handler {
                Ok(h) if h.is_callable() => {
                    h.as_callable().unwrap().call(&JsValue::undefined(), &[request_obj.into()], &mut context)
                },
                _ => return Err("No 'export default' function found in script".to_string())
            };

            let _ = context.run_jobs();

            let final_val = match promise {
                Ok(v) => {
                    if let Some(p) = v.as_promise() {
                        match p.state() {
                            PromiseState::Fulfilled(val) => val,
                            PromiseState::Rejected(err) => return Err(format!("Script Rejected: {}", err.display())),
                            PromiseState::Pending => return Err("Script did not complete (Pending Promise)".to_string()),
                        }
                    } else {
                        v
                    }
                },
                Err(e) => return Err(format!("Execution Error: {}", e))
            };

            if let Some(obj) = final_val.as_object() {
                let body = obj.get(JsString::from("body"), &mut context).unwrap_or_default();
                let json = body.to_json(&mut context).unwrap_or(None).unwrap_or(serde_json::Value::Null);
                Ok(serde_json::to_value(json).unwrap_or(JsonValue::Null))
            } else {
                Err("Script must return a Response object".to_string())
            }

        }).await;

        ACTIVE_DB_CONTEXT.with(|c| *c.borrow_mut() = None);

        match result {
            Ok(inner) => inner,
            Err(e) => Err(format!("System Panic: {}", e)),
        }
    }
}

use std::cell::RefCell;
thread_local! {
    // Thread Local Context holding DB, Embedder, VectorProvider, and Async Runtime Handle
    static ACTIVE_DB_CONTEXT: RefCell<Option<(Arc<dyn Db>, Arc<EmbedderService>, Arc<dyn VectorProvider>, tokio::runtime::Handle)>> = RefCell::new(None);
}