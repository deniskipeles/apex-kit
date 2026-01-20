use std::sync::Arc;
use serde_json::Value as JsonValue;
use crate::{Db, query::QueryOptions, ScriptContext};
use crate::realtime::{DbEvent, EventScope};
use tokio::sync::broadcast;
use regex::Regex;
use std::path::Path;
use std::process::Command;
use std::cell::RefCell;
use crate::query_engine::{QueryEngine, ApexQuery};

use boa_engine::{
    Context, JsValue, JsResult, NativeFunction, JsError, JsString, JsArgs,
    object::ObjectInitializer,
    property::Attribute,
    builtins::promise::PromiseState
};

// --- PRELUDE ---
const JS_PRELUDE: &str = r#"
    class Headers {
        constructor(init = {}) { this.map = new Map(Object.entries(init)); }
        get(name) { for (const [k, v] of this.map) { if (k.toLowerCase() === name.toLowerCase()) return v; } return null; }
        set(name, value) { this.map.set(name, value); }
        entries() { return this.map.entries(); }
    }

    class Request {
        constructor(input, init = {}) {
            this.bodyData = init.body || input?.body || null;
            this.method = init.method || "GET";
            this.headers = new Headers(init.headers || {});
            this.args = this.bodyData || {}; 
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

    class ApexKit {
        constructor(contextId = null) { this.contextId = contextId; }
        
        tenant(id) { if(globalThis.$root) return new ApexKit("tenant:" + id); throw new Error("Root scope required"); }
        sandbox(id) { if(globalThis.$root) return new ApexKit("sandbox:" + id); throw new Error("Root scope required"); }

        collection(name) {
            return {
                list: async (opts) => $db.records.list(this.contextId, name, opts),
                get: async (id, opts) => $db.records.get(this.contextId, name, id, opts?.expand),
                create: async (data) => $db.records.create(this.contextId, name, data),
                update: async (id, data) => $db.records.update(this.contextId, name, id, data),
                delete: async (id) => $db.records.delete(this.contextId, name, id),
                search: async (q) => $db.records.search(this.contextId, name, q),
                searchVector: async (f, v, l) => $db.records.searchVector(this.contextId, name, f, v, l),
                getVector: async (id) => $db.records.getVector(this.contextId, name, id)
            };
        }
        
        // [NEW] Analytical Query
        async query(queryObject) {
            // Inject contextId if not present? Or handle in $db.query wrapper?
            // Since ApexQuery struct doesn't have context field, we rely on $db.query handling it.
            // But $db.query takes 1 arg in register_db (q_val). 
            // We need to update $db.query signature or wrap the object.
            // Let's assume we update $db.query to take (contextId, queryObject) in Rust.
            return await $db.query(this.contextId, queryObject);
        }
        
        get users() {
            return {
                create: async (e, p, r) => $db.users.create(this.contextId, e, p, r),
                get: async (e) => $db.users.get(this.contextId, e),
                list: async (q, l, o) => $db.users.list(this.contextId, q, l, o)
            }
        }
        
        get collections() {
            return {
                list: async () => $db.collections.list(this.contextId),
                create: async (n, s) => $db.collections.create(this.contextId, n, s)
            }
        }

        get files() {
            return {
                list: async (l, o) => $db.files.list(this.contextId, l, o)
            }
        }
    }

    globalThis.Headers = Headers;
    globalThis.Request = Request;
    globalThis.Response = Response;
    globalThis.ApexKit = ApexKit;
    globalThis.$apex = new ApexKit();
"#;

thread_local! {
    static ACTIVE_CONTEXT: RefCell<Option<(
        Arc<dyn ScriptContext>,           
        tokio::runtime::Handle,           
        Option<String>,                   
        Option<broadcast::Sender<DbEvent>>,
        EventScope                        
    )>> = RefCell::new(None);
}

#[derive(Clone)]
pub struct ScriptEngine;

impl ScriptEngine {
    pub async fn new() -> Self { Self }

    pub async fn run_script(
        &self, 
        code: &str, 
        input_data: JsonValue, 
        context: Arc<dyn ScriptContext>,
        base_url: Option<String>,
        scope: EventScope
    ) -> Result<JsonValue, String> {
        self.execute_js_task(code, context, base_url, scope, move |ctx| {
            let js_body = JsValue::from_json(&input_data, ctx).map_err(|e| e.to_string())?;
            
            let request_cls = ctx.global_object().get(JsString::from("Request"), ctx).unwrap();
            let req_init = ObjectInitializer::new(ctx)
                .property(JsString::from("method"), JsString::from("POST"), Attribute::all())
                .property(JsString::from("body"), js_body, Attribute::all())
                .build();
            
            let request_obj = request_cls.as_constructor().unwrap()
                .construct(&[JsValue::undefined(), JsValue::from(req_init)], Some(&request_cls.as_object().unwrap()), ctx)
                .map_err(|e| format!("Failed to create Request: {}", e))?;

            let handler = ctx.global_object().get(JsString::from("__mainHandler"), ctx);
            let promise = match handler {
                Ok(h) if h.is_callable() => h.as_callable().unwrap().call(&JsValue::undefined(), &[request_obj.into()], ctx),
                _ => return Err("No 'export default' found".to_string())
            };

            let _ = ctx.run_jobs();
            let final_val = Self::resolve_promise(promise, ctx)?;

            if let Some(obj) = final_val.as_object() {
                if obj.has_property(JsString::from("body"), ctx).unwrap_or(false) {
                    let body = obj.get(JsString::from("body"), ctx).unwrap_or_default();
                    let json = body.to_json(ctx).unwrap_or(None).unwrap_or(serde_json::Value::Null);
                    return Ok(serde_json::to_value(json).unwrap_or(JsonValue::Null));
                }
            }
            let json = final_val.to_json(ctx).unwrap_or(None).unwrap_or(serde_json::Value::Null);
            Ok(serde_json::to_value(json).unwrap_or(JsonValue::Null))
        }).await
    }
    
    pub async fn run_hook(
        &self,
        code: &str,
        event_data: JsonValue, 
        context: Arc<dyn ScriptContext>,
        base_url: Option<String>,
        scope: Option<EventScope>
    ) -> Result<Option<JsonValue>, String> {
        let actual_scope = scope.unwrap_or(EventScope::Root);
        let wrapped_code = format!(r#"
            (async () => {{
                {}
                const e = globalThis.__hook_context__;
                if (globalThis.__mainHandler) {{ return await globalThis.__mainHandler(e); }}
                return null;
            }})()
        "#, code);

        self.execute_js_task(&wrapped_code, context, base_url, actual_scope, move |ctx| {
            let js_event = JsValue::from_json(&event_data, ctx).map_err(|e| e.to_string())?;
            ctx.register_global_property(JsString::from("__hook_context__"), js_event.clone(), Attribute::all()).unwrap();

            let handler = ctx.global_object().get(JsString::from("__mainHandler"), ctx);
            let promise = match handler {
                Ok(h) if h.is_callable() => h.as_callable().unwrap().call(&JsValue::undefined(), &[js_event], ctx),
                _ => return Ok(None) 
            };

            let _ = ctx.run_jobs();
            let final_val = Self::resolve_promise(promise, ctx)?;

            if final_val.is_null() || final_val.is_undefined() { return Ok(None); }
            if final_val.as_boolean().unwrap_or(false) == false { return Err("Hook blocked operation".to_string()); }
            
            if final_val.is_object() {
                let json = final_val.to_json(ctx).unwrap().unwrap();
                return Ok(Some(json));
            }
            Ok(None)
        }).await
    }

    async fn execute_js_task<F, R>(
        &self,
        code: &str,
        context: Arc<dyn ScriptContext>,
        base_url: Option<String>,
        scope: EventScope,
        task_logic: F
    ) -> Result<R, String>
    where
        F: FnOnce(&mut Context) -> Result<R, String> + Send + 'static,
        R: Send + 'static
    {
        let re_config = Regex::new(r"export\s+const\s+graphql\s*=\s*(\{[\s\S]*?\})(?:;|\n|$)").map_err(|e| e.to_string())?;
        let code_cleaned = re_config.replace_all(code, "");
        let processed_code = code_cleaned.replacen("export default", "globalThis.__mainHandler =", 1);
        
        let handle = tokio::runtime::Handle::current();

        // [NEW] Get TX from context
        let tx = Some(context.get_realtime_tx());

        let result = tokio::task::spawn_blocking(move || -> Result<R, String> {
            let mut context_boa = Context::default();
            ACTIVE_CONTEXT.with(|c| { *c.borrow_mut() = Some((context, handle.clone(), base_url, tx, scope)); });
            Self::setup_boa(&mut context_boa)?;
            if let Err(e) = context_boa.eval(boa_engine::Source::from_bytes(processed_code.as_bytes())) {
                 return Err(format!("Script Syntax Error: {}", e));
            }
            task_logic(&mut context_boa)
        }).await;

        match result {
            Ok(inner) => inner,
            Err(e) => Err(format!("System Panic: {}", e)),
        }
    }

    fn setup_boa(ctx: &mut Context) -> Result<(), String> {
        ctx.eval(boa_engine::Source::from_bytes(JS_PRELUDE.as_bytes())).map_err(|e| format!("Prelude Error: {}", e))?;
        
        register_console(ctx)?;
        register_util(ctx)?;
        register_http(ctx)?;
        register_fs(ctx)?;
        register_archive(ctx)?;
        register_db(ctx)?;
        register_root(ctx)?;
        register_env(ctx)?;
        register_ai(ctx)?;
        register_mail(ctx)?;
        register_realtime(ctx)?;

        Ok(())
    }
    
    fn resolve_promise(val: Result<JsValue, JsError>, _ctx: &mut Context) -> Result<JsValue, String> {
        let js_val = val.map_err(|e| e.to_string())?;
        if let Some(p) = js_val.as_promise() {
             match p.state() {
                 PromiseState::Fulfilled(v) => Ok(v),
                 PromiseState::Rejected(err) => Err(format!("Script Rejected: {}", err.display())),
                 PromiseState::Pending => Err("Script did not complete (Pending Promise)".to_string()),
             }
        } else { Ok(js_val) }
    }
}

// --- JS Return Helper ---
fn return_json_promise(ctx: &mut Context, result: Result<serde_json::Value, String>) -> JsResult<JsValue> {
    let (promise, resolvers) = boa_engine::object::builtins::JsPromise::new_pending(ctx);
    match result {
        Ok(json_val) => {
            let js_val = JsValue::from_json(&json_val, ctx).unwrap_or(JsValue::null());
            resolvers.resolve.call(&JsValue::undefined(), &[js_val], ctx)?;
        },
        Err(e) => {
            let err_msg = JsString::from(e);
            resolvers.reject.call(&JsValue::undefined(), &[err_msg.into()], ctx)?;
        }
    }
    Ok(promise.into())
}

// --- Local Helpers for $db Logic ---

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

async fn resolve_db(ctx_str: Option<String>, app_ctx: Arc<dyn ScriptContext>) -> Result<Arc<dyn Db>, String> {
    match ctx_str.as_deref() {
        Some(s) if s.starts_with("tenant:") => {
            let tid = s.strip_prefix("tenant:").unwrap();
            app_ctx.resolve_tenant_db(tid).await.ok_or(format!("Tenant {} not found", tid))
        },
        Some(s) if s.starts_with("sandbox:") => {
            let sid = s.strip_prefix("sandbox:").unwrap();
            app_ctx.resolve_sandbox_db(sid).await.ok_or(format!("Sandbox {} not found", sid))
        },
        _ => Ok(app_ctx.get_db())
    }
}

// --- MODULE REGISTRATIONS ---

fn register_console(ctx: &mut Context) -> Result<(), String> {
    let log = NativeFunction::from_copy_closure(|_, args, ctx| {
        let msg = args.iter().map(|a| a.to_string(ctx).unwrap_or_default().to_std_string_escaped()).collect::<Vec<_>>().join(" ");
        println!("[SCRIPT] {}", msg);
        Ok(JsValue::undefined())
    });
    let obj = ObjectInitializer::new(ctx)
        .function(log.clone(), JsString::from("log"), 1)
        .function(log.clone(), JsString::from("error"), 1)
        .build();
    ctx.register_global_property(JsString::from("console"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_util(ctx: &mut Context) -> Result<(), String> {
    use crate::utils::{slugify, sha256, sha512, hmac_sha256, generate_random_hex};

    // UUID
    let uuid_fn = NativeFunction::from_fn_ptr(|_, _, _| Ok(JsValue::from(JsString::from(uuid::Uuid::new_v4().to_string()))));
    
    // Slug
    let slug_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        Ok(JsValue::from(JsString::from(slugify(&args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()))))
    });

    // Hash (SHA256 / SHA512)
    let hash_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let alg = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        
        let result = match alg.as_str() {
            "sha256" => sha256(&text),
            "sha512" => sha512(&text),
            _ => return Err(JsError::from_opaque(JsString::from("Unsupported algorithm (use sha256/sha512)").into()))
        };
        Ok(JsValue::from(JsString::from(result)))
    });

    // HMAC
    let hmac_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let key = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        Ok(JsValue::from(JsString::from(hmac_sha256(&key, &text))))
    });

    // Base64 Encode
    let b64_enc_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        Ok(JsValue::from(JsString::from(STANDARD.encode(text))))
    });

    // Base64 Decode
    let b64_dec_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        match STANDARD.decode(text) {
            Ok(bytes) => {
                let s = String::from_utf8(bytes).unwrap_or_default();
                Ok(JsValue::from(JsString::from(s)))
            },
            Err(_) => Err(JsError::from_opaque(JsString::from("Invalid Base64").into()))
        }
    });
    
    // Sleep (Mock/Blocking)
    let sleep_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let ms = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0) as u64;
        std::thread::sleep(std::time::Duration::from_millis(ms));
        Ok(JsValue::undefined())
    });

    // Random Hex
    let random_hex_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let len = args.get_or_undefined(0).to_number(ctx).unwrap_or(16.0) as usize;
        Ok(JsValue::from(JsString::from(generate_random_hex(len))))
    });

    let obj = ObjectInitializer::new(ctx)
        .function(uuid_fn, JsString::from("uuid"), 0)
        .function(slug_fn, JsString::from("slugify"), 1)
        .function(hash_fn, JsString::from("hash"), 2)
        .function(hmac_fn, JsString::from("hmac"), 2)
        .function(b64_enc_fn, JsString::from("base64Encode"), 1)
        .function(b64_dec_fn, JsString::from("base64Decode"), 1)
        .function(sleep_fn, JsString::from("sleep"), 1)
        .function(random_hex_fn, JsString::from("randomHex"), 1)
        .build();

    ctx.register_global_property(JsString::from("$util"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_fs(ctx: &mut Context) -> Result<(), String> {
    let read_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                // Fixed type mismatch (String vs Result)
                let base = match scope {
                    EventScope::Root => "storage/system/uploads".to_string(),
                    EventScope::Tenant(id) => format!("storage/tenants/{}/uploads", id),
                    EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/uploads", id),
                    _ => "storage/tmp".to_string()
                };
                
                if fname.contains("..") { return Err("Invalid path".to_string()); }
                
                handle.block_on(async {
                    tokio::fs::read_to_string(Path::new(&base).join(fname)).await.map_err(|e| e.to_string())
                })
            } else { Err("Context lost".to_string()) }
        });
        
        match result {
            Ok(s) => Ok(JsValue::from(JsString::from(s))),
            Err(e) => Err(JsError::from_opaque(JsString::from(e).into()))
        }
    });
    
    let obj = ObjectInitializer::new(ctx)
        .function(read_fn, JsString::from("readText"), 1)
        .build();
    ctx.register_global_property(JsString::from("$fs"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_db(ctx: &mut Context) -> Result<(), String> {
    
    // --- 1. $db.records ---
    let records_obj = ObjectInitializer::new(ctx)
        // list(ctxId, col, opts)
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
            let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
            let col = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let opts_val = args.get_or_undefined(2).to_json(ctx).unwrap().unwrap_or(serde_json::Value::Null);
            
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let opts: QueryOptions = serde_json::from_value(opts_val).unwrap_or_default();
                        let list = db.list_records(col_id, opts).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(list).unwrap())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("list"), 3)

        // get(ctxId, col, id, expand)
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
            let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
            let col = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let id = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0) as i64;
            let expand = args.get(3).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let rec = db.get_record(col_id, id, expand).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(rec).unwrap_or(serde_json::Value::Null))
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("get"), 4)

        // create(ctxId, col, data)
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
            let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
            let col = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let data = args.get_or_undefined(2).to_json(ctx).unwrap().unwrap_or(serde_json::Value::Null);

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let id = db.create_record(col_id, &data).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::json!({ "id": id }))
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("create"), 3)

        // update(ctxId, col, id, data)
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
            let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
            let col = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let id = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0) as i64;
            let data = args.get_or_undefined(3).to_json(ctx).unwrap().unwrap_or(serde_json::Value::Null);

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let rec = db.update_record(col_id, id, &data).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(rec).unwrap())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("update"), 4)

        // delete(ctxId, col, id)
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
            let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
            let col = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let id = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0) as i64;

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        db.delete_record(col_id, id).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::Value::Bool(true))
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("delete"), 3)

        // searchVector(ctxId, col, field, vector, limit)
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
            let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
            let col = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let field = args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped();
            let vec_val = args.get_or_undefined(3).to_json(ctx).unwrap().unwrap_or(serde_json::Value::Null);
            let limit = args.get_or_undefined(4).to_number(ctx).unwrap_or(10.0) as usize;

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let vector: Vec<f32> = serde_json::from_value(vec_val).unwrap_or_default();
                        let recs = db.search_vector(col_id, &field, vector, limit).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(recs).unwrap())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("searchVector"), 5)

        // getVector(ctxId, col, id)
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
            let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
            let col = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let id = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0) as i64;
            
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let col_id = resolve_collection_local(db.clone(), &col).await?;
                        let vecs = db.get_record_vectors(col_id, id).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(vecs).unwrap())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("getVector"), 3)
        .build();

    // --- 2. $db.query (Analytics Engine) ---
    // $db.query(ctxId, json_query)
    let query_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
        let q_val = args.get_or_undefined(1).to_json(ctx).unwrap().unwrap_or(serde_json::Value::Null);
        
        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    let db = resolve_db(ctx_id, app.clone()).await?;
                    let query: ApexQuery = serde_json::from_value(q_val).map_err(|e| e.to_string())?;
                    QueryEngine::execute(db, query).await.map_err(|e| e.to_string())
                })
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, res)
    });

    // --- 3. $db.users ---
    let users_obj = ObjectInitializer::new(ctx)
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
            let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
            let email = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let u = db.get_user_by_email(&email).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(u).unwrap_or(serde_json::Value::Null))
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("get"), 2)
        
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
            let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
            let email = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let pass = args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped();
            let role = args.get_or_undefined(3).to_string(ctx)?.to_std_string_escaped();
            
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let hash = crate::auth::hash_password(&pass).map_err(|e| e.to_string())?;
                        let u = db.create_user(&email, &hash, &role, None).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(u).unwrap())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("create"), 4)
        .build();

    // --- 4. $db.collections ---
    let cols_obj = ObjectInitializer::new(ctx)
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
             let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
             let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let cols = db.list_collections().await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(cols).unwrap())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("list"), 1)
        .build();

    // --- 5. $db.files ---
    let files_obj = ObjectInitializer::new(ctx)
        // list(ctxId, limit, offset)
        .function(NativeFunction::from_copy_closure(move |_, args, ctx| {
            let ctx_id = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped());
            let limit = args.get_or_undefined(1).to_number(ctx).unwrap_or(20.0) as i64;
            let offset = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0) as i64;

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let db = resolve_db(ctx_id, app.clone()).await?;
                        let files = db.list_files(limit, offset).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(files).unwrap())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        }), JsString::from("list"), 3)
        .build();

    // Build Final Object
    let db_obj = ObjectInitializer::new(ctx)
        .property(JsString::from("records"), records_obj, Attribute::all())
        .property(JsString::from("users"), users_obj, Attribute::all())
        .property(JsString::from("collections"), cols_obj, Attribute::all())
        .property(JsString::from("files"), files_obj, Attribute::all())
        .function(query_fn, JsString::from("query"), 1)
        .build();

    ctx.register_global_property(JsString::from("$db"), db_obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_http(ctx: &mut Context) -> Result<(), String> {
    let fetch = |method: reqwest::Method, url: String, body: Option<serde_json::Value>| {
        let client = reqwest::blocking::Client::new();
        let mut req = client.request(method, url);
        if let Some(b) = body { req = req.json(&b); }
        match req.send() {
            Ok(res) => Ok(JsValue::from(JsString::from(res.text().unwrap_or_default()))),
            Err(e) => Err(JsError::from_opaque(JsString::from(e.to_string()).into()))
        }
    };

    let get = NativeFunction::from_copy_closure(move |_, args, ctx| {
        fetch(reqwest::Method::GET, args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped(), None)
    });
    let post = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let body = args.get_or_undefined(1).to_json(ctx).unwrap_or(None);
        fetch(reqwest::Method::POST, args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped(), body)
    });

    let obj = ObjectInitializer::new(ctx)
        .function(get, JsString::from("get"), 1)
        .function(post, JsString::from("post"), 2)
        .build();
    ctx.register_global_property(JsString::from("$http"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_archive(ctx: &mut Context) -> Result<(), String> {
    let create_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let json_val = args.get_or_undefined(0).to_json(ctx).unwrap();
        let fname = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        let safe_fname = if fname.ends_with(".tar.gz") { fname } else { format!("{}.tar.gz", fname) };

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let (upload_dir, _base_url) = match scope {
                    EventScope::Root => ("storage/system/uploads".to_string(), "/api/v1/storage/file/".to_string()),
                    EventScope::Tenant(id) => (format!("storage/tenants/{}/uploads", id), format!("/tenant/{}/api/v1/storage/file/", id)),
                    EventScope::Sandbox(id) => (format!("storage/sandboxes/session_{}/uploads", id), format!("/sandbox/{}/api/v1/storage/file/", id)),
                    _ => return Err("Invalid scope".to_string())
                };

                let temp_id = uuid::Uuid::new_v4();
                let staging = format!("storage/tmp/{}", temp_id);
                let final_path = format!("{}/{}", upload_dir, safe_fname);

                handle.block_on(async {
                    tokio::task::spawn_blocking(move || {
                        let root = Path::new(&staging);
                        // [FIX] Handle Option
                        if let Some(obj) = json_val.as_ref().and_then(|v| v.as_object()) {
                            fn write_tree(base: &Path, tree: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
                                std::fs::create_dir_all(base).map_err(|e| e.to_string())?;
                                for (k, v) in tree {
                                    if k.contains("..") { return Err("Invalid path".into()); }
                                    let target = base.join(k);
                                    if let Some(s) = v.as_str() { std::fs::write(target, s).map_err(|e| e.to_string())?; }
                                    else if let Some(o) = v.as_object() { write_tree(&target, o)?; }
                                }
                                Ok(())
                            }
                            write_tree(root, obj)?;
                        }
                        
                        std::fs::create_dir_all(&upload_dir).ok();
                        let out = Command::new("tar").arg("-czf").arg(&final_path).arg("-C").arg(&staging).arg(".").output().map_err(|e| e.to_string())?;
                        if !out.status.success() { return Err("Tar failed".into()); }
                        let _ = std::fs::remove_dir_all(root);
                        Ok(safe_fname)
                    }).await.unwrap()
                })
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result.map(|s| serde_json::Value::String(s)))
    });

    let obj = ObjectInitializer::new(ctx)
        .function(create_fn, JsString::from("create"), 2)
        .build();
    ctx.register_global_property(JsString::from("$archive"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_root(ctx: &mut Context) -> Result<(), String> {
    let is_root = ACTIVE_CONTEXT.with(|c| c.borrow().as_ref().map(|t| t.4 == EventScope::Root).unwrap_or(false));
    
    if is_root {
        let create_tenant = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        // [FIX] Error mapping
                        app.admin_create_tenant(id.clone()).await.map_err(|e| e.to_string())?;
                        // [FIX] Error mapping and method call
                        app.get_db().register_tenant(&id, None).await.map_err(|e| e.to_string())?;
                        Ok(true)
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res.map(|b| serde_json::Value::Bool(b)))
        });

        let obj = ObjectInitializer::new(ctx)
            .function(create_tenant, JsString::from("createTenant"), 1)
            .build();
        ctx.register_global_property(JsString::from("$root"), obj, Attribute::all()).map_err(|e| e.to_string())
    } else {
        ctx.register_global_property(JsString::from("$root"), JsValue::null(), Attribute::all()).map_err(|e| e.to_string())
    }
}

fn register_env(ctx: &mut Context) -> Result<(), String> {
    let get = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    if let Ok(Some(val)) = app.get_db().get_config(&key).await {
                         if let Ok(enc) = serde_json::from_value::<crate::security::EncryptedValue>(val.clone()) {
                             return app.get_vault().decrypt(&enc).map_err(|e| e.to_string());
                         }
                         return Ok(val.as_str().unwrap_or("").to_string());
                    }
                    Ok("".to_string())
                })
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result.map(|s| serde_json::Value::String(s)))
    });
    
    let app_url = ACTIVE_CONTEXT.with(|c| c.borrow().as_ref().and_then(|t| t.2.clone())).unwrap_or_default();
    
    let obj = ObjectInitializer::new(ctx)
        .function(get, JsString::from("get"), 1)
        .property(JsString::from("APP_URL"), JsString::from(app_url), Attribute::all())
        .build();
    ctx.register_global_property(JsString::from("$env"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_ai(ctx: &mut Context) -> Result<(), String> {
    let embed = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    app.get_vector_provider().embed(&text).await
                })
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, res.map(|v| serde_json::to_value(v).unwrap()))
    });
    let obj = ObjectInitializer::new(ctx)
        .function(embed, JsString::from("embed"), 1)
        .build();
    ctx.register_global_property(JsString::from("$ai"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_mail(ctx: &mut Context) -> Result<(), String> {
    let send = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let to = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let subj = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        let body = args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped();
        
        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    crate::jobs::send_email(app.get_db(), app.get_vault(), &to, &subj, &body).await
                })
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, res.map(|_| serde_json::Value::Bool(true)))
    });
    let obj = ObjectInitializer::new(ctx).function(send, JsString::from("send"), 3).build();
    ctx.register_global_property(JsString::from("$mail"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_realtime(ctx: &mut Context) -> Result<(), String> {
    let send = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let channel = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let evt = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        let data = args.get_or_undefined(2).to_json(ctx).unwrap().unwrap_or(serde_json::Value::Null);

        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, _, _, Some(tx), scope)) = &*c.borrow() {
                 let scoped_chan = match scope {
                     EventScope::Root => format!("root::{}", channel),
                     EventScope::Tenant(id) => format!("tenant_{}::{}", id, channel),
                     EventScope::Sandbox(id) => format!("sandbox_{}::{}", id, channel),
                     _ => channel.clone()
                 };
                 let _ = tx.send(DbEvent::Custom { event: evt, data, scope: EventScope::Channel(scoped_chan) });
                 Ok(true)
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, res.map(|b| serde_json::Value::Bool(b)))
    });
    let obj = ObjectInitializer::new(ctx).function(send, JsString::from("send"), 3).build();
    ctx.register_global_property(JsString::from("$realtime"), obj, Attribute::all()).map_err(|e| e.to_string())
}
// =========================== /teamspace/studios/this_studio/apex/apex-kit/apexkit-core/src/scripting.rs ends here ===========================