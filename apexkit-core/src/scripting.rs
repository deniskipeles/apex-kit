use std::sync::Arc;
use serde_json::{Value as JsonValue, json};
use crate::{Db, query::QueryOptions, ScriptContext};
use crate::realtime::{DbEvent, EventScope};
use tokio::sync::broadcast;
use regex::Regex;
use std::path::{PathBuf};
use std::cell::RefCell;
use crate::query_engine::ApexQuery;

use boa_engine::{
    Context, JsValue, JsResult, NativeFunction, JsError, JsString, JsArgs,
    object::ObjectInitializer,
    property::Attribute,
    builtins::promise::PromiseState
};
use std::io::{Cursor, Read, Write};
use zip::{ZipArchive, ZipWriter};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use zip::write::FileOptions;
use std::collections::HashMap;
use std::time::UNIX_EPOCH;

// --- PRELUDE ---
const JS_PRELUDE: &str = r#"
    class Headers {
        constructor(init = {}) {
            this.map = new Map();
            if (init instanceof Headers) {
                init.map.forEach((v, k) => this.map.set(k, v));
            } else if (Array.isArray(init)) {
                init.forEach(([k, v]) => this.map.set(k.toLowerCase(), v));
            } else {
                Object.entries(init).forEach(([k, v]) => this.map.set(k.toLowerCase(), String(v)));
            }
        }
        get(name) { return this.map.get(name.toLowerCase()) || null; }
        set(name, value) { this.map.set(name.toLowerCase(), String(value)); }
        has(name) { return this.map.has(name.toLowerCase()); }
        delete(name) { this.map.delete(name.toLowerCase()); }
        forEach(callback) { this.map.forEach((v, k) => callback(v, k, this)); }
    }

    class Response {
        constructor(body, init = {}) {
            this.body = body;
            this.status = init.status || 200;
            this.statusText = init.statusText || "OK";
            this.headers = new Headers(init.headers || {});
            this.ok = this.status >= 200 && this.status < 300;
            this.url = init.url || "";

            this.json = async () => {
                if (typeof this.body === 'object' && this.body !== null) return this.body;
                return JSON.parse(this.body);
            };

            this.text = async () => {
                if (typeof this.body === 'string') return this.body;
                return JSON.stringify(this.body);
            };
        }
    }

    class Request {
        constructor(input, init = {}) {
            if (typeof input === 'object' && input.url) {
                this.url = input.url;
                this.method = init.method || input.method;
                this.bodyData = init.body || input.bodyData;
                this.headers = new Headers(init.headers || input.headers);
            } else {
                this.url = input;
                this.method = init.method || "GET";
                this.bodyData = init.body || null;
                this.headers = new Headers(init.headers || {});
            }
            this.args = this.bodyData || {};
        }
        async json() { return typeof this.bodyData === 'string' ? JSON.parse(this.bodyData) : this.bodyData; }
        async text() { return typeof this.bodyData === 'string' ? this.bodyData : JSON.stringify(this.bodyData); }
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
        
        // Analytical Query
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
    // --- STANDARD FETCH IMPLEMENTATION ---
    globalThis.fetch = async function(url, options = {}) {
        // Normalize headers to a simple object for the Rust layer
        let headersObj = {};
        if (options.headers) {
            if (options.headers instanceof Headers) {
                options.headers.forEach((v, k) => headersObj[k] = v);
            } else {
                headersObj = options.headers;
            }
        }

        // Call Native Rust Implementation
        const nativeRes = await $__native_fetch(url, {
            method: options.method || 'GET',
            headers: headersObj,
            body: options.body
        });

        // Rehydrate into JS Response Object
        return new Response(nativeRes.body, {
            status: nativeRes.status,
            statusText: nativeRes.statusText,
            headers: nativeRes.headers,
            url: nativeRes.url
        });
    };
"#;

thread_local! {
    pub static ACTIVE_CONTEXT: RefCell<Option<(
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
        headers: Option<HashMap<String, String>> // <--- NEW ARGUMENT
    ) -> Result<JsonValue, String> {
        self.execute_js_task(code, context, base_url, move |ctx| {
            let js_body = JsValue::from_json(&input_data, ctx).map_err(|e| e.to_string())?;
            
            // 1. Build Headers Object
            let mut header_init = ObjectInitializer::new(ctx);
            if let Some(h) = headers {
                for (k, v) in h {
                    header_init.property(JsString::from(k), JsString::from(v), Attribute::all());
                }
            }
            let js_headers = header_init.build();

            // 2. Build Request Init
            let request_cls = ctx.global_object().get(JsString::from("Request"), ctx).unwrap();
            let req_init = ObjectInitializer::new(ctx)
                .property(JsString::from("method"), JsString::from("POST"), Attribute::all())
                .property(JsString::from("body"), js_body, Attribute::all())
                .property(JsString::from("headers"), js_headers, Attribute::all()) // <--- INJECT HEADERS
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
        let _actual_scope = scope.unwrap_or(EventScope::Root);
        let wrapped_code = format!(r#"
            (async () => {{
                {}
                const e = globalThis.__hook_context__;
                if (globalThis.__mainHandler) {{ return await globalThis.__mainHandler(e); }}
                return null;
            }})()
        "#, code);

        self.execute_js_task(&wrapped_code, context, base_url, move |ctx| {
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

        // Get TX from context
        let tx = Some(context.get_realtime_tx());
        let execution_scope = context.get_scope();

        let result = tokio::task::spawn_blocking(move || -> Result<R, String> {
            let mut context_boa = Context::default();
            ACTIVE_CONTEXT.with(|c| { *c.borrow_mut() = Some((context, handle.clone(), base_url, tx, execution_scope)); });
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
        register_fetch(ctx)?;
        register_fs(ctx)?;
        register_zip(ctx)?;
        register_db(ctx)?;
        crate::scripting_cmd::register_cmd(ctx)?;
        register_run(ctx)?;
        register_root(ctx)?;
        register_env(ctx)?;
        register_ai(ctx)?;
        register_mail(ctx)?;
        register_realtime(ctx)?;
        register_cache(ctx)?;

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
pub fn return_json_promise(ctx: &mut Context, result: Result<serde_json::Value, String>) -> JsResult<JsValue> {
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
    // 1. Explicit Context Passed in JS
    if let Some(s) = ctx_str {
        if s.starts_with("tenant:") {
            let tid = s.strip_prefix("tenant:").unwrap();
            return app_ctx.resolve_tenant_db(tid).await.ok_or(format!("Tenant {} not found", tid));
        }
        if s.starts_with("sandbox:") {
            let sid = s.strip_prefix("sandbox:").unwrap();
            return app_ctx.resolve_sandbox_db(sid).await.ok_or(format!("Sandbox {} not found", sid));
        }
    }

    // 2. Implicit Context based on Execution Scope
    let scope = app_ctx.get_scope();
    
    match scope {
        EventScope::Tenant(id) => {
            app_ctx.resolve_tenant_db(&id).await.ok_or(format!("Current Tenant {} context not found", id))
        },
        EventScope::Sandbox(id) => {
            app_ctx.resolve_sandbox_db(&id).await.ok_or(format!("Current Sandbox {} context not found", id))
        },
        // [FIX] For Root scope, use the main get_db() which is always the Root DB.
        // This was the missing piece. When a Tenant called a Root script, the scope was
        // correctly set to Root, but this match arm was missing, causing it to fall through
        // and potentially use the wrong DB context from a misconfigured get_db impl.
        EventScope::Root => Ok(app_ctx.get_db()),
        // Fallback for Channel, etc.
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

// 1. Resolve READ Path (Scope Root)
fn resolve_read_path(scope: &EventScope, requested_path: &str) -> Result<PathBuf, String> {
    if requested_path.contains("..") { return Err("Path traversal forbidden".into()); }

    let base_dir = match scope {
        EventScope::Root => {
            // Root Admin can read anywhere via prefix
            if let Some(stripped) = requested_path.strip_prefix("tenant:") {
                 let parts: Vec<&str> = stripped.splitn(2, '/').collect();
                 if parts.len() < 2 { return Err("Invalid format".into()); }
                 format!("storage/tenants/{}/{}", parts[0], parts[1])
            } else if let Some(stripped) = requested_path.strip_prefix("sandbox:") {
                 let parts: Vec<&str> = stripped.splitn(2, '/').collect();
                 if parts.len() < 2 { return Err("Invalid format".into()); }
                 format!("storage/sandboxes/session_{}/{}", parts[0], parts[1])
            } else {
                 format!("storage/system/{}", requested_path)
            }
        },
        EventScope::Tenant(id) => format!("storage/tenants/{}/{}", id, requested_path),
        EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/{}", id, requested_path),
        _ => return Err("Invalid scope".into())
    };
    
    Ok(PathBuf::from(base_dir))
}

// 2. Resolve WRITE Path (Scope TMP Only)
fn resolve_write_path(scope: &EventScope, requested_path: &str) -> Result<PathBuf, String> {
    if requested_path.contains("..") { return Err("Path traversal forbidden".into()); }

    // Root Admin can write anywhere (Power User) - OR restrict to root/tmp? 
    // Let's restrict Root to its own root/tmp for consistency, 
    // unless they explicitly use a prefix to write to a tenant's tmp.
    
    let base_dir = match scope {
        EventScope::Root => {
            if let Some(stripped) = requested_path.strip_prefix("tenant:") {
                 let parts: Vec<&str> = stripped.splitn(2, '/').collect();
                 if parts.len() < 2 { return Err("Invalid format".into()); }
                 format!("storage/tenants/{}/tmp/{}", parts[0], parts[1])
            } else if let Some(stripped) = requested_path.strip_prefix("sandbox:") {
                 let parts: Vec<&str> = stripped.splitn(2, '/').collect();
                 if parts.len() < 2 { return Err("Invalid format".into()); }
                 format!("storage/sandboxes/session_{}/tmp/{}", parts[0], parts[1])
            } else {
                 format!("storage/system/tmp/{}", requested_path)
            }
        },
        EventScope::Tenant(id) => format!("storage/tenants/{}/tmp/{}", id, requested_path),
        EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/tmp/{}", id, requested_path),
        _ => return Err("Invalid scope".into())
    };

    Ok(PathBuf::from(base_dir))
}

fn register_fs(ctx: &mut Context) -> Result<(), String> {

    // $fs.read(path) -> string
    let read_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_read_path(scope, &fname)?; // Read from Root
                handle.block_on(async {
                     if !target.exists() { return Err("File not found".into()); }
                     if target.is_dir() { return Err("Cannot read directory".into()); }
                     tokio::fs::read_to_string(target).await.map_err(|e| e.to_string())
                })
            } else { Err("Context lost".into()) }
        });
        match result {
            Ok(s) => return_json_promise(ctx, Ok(serde_json::Value::String(s))),
            Err(e) => return_json_promise(ctx, Err(e))
        }
    });

    // $fs.write(path, content) -> void (Writes to TMP)
    let write_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let content = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_write_path(scope, &fname)?; // Write to TMP
                handle.block_on(async {
                     if let Some(parent) = target.parent() {
                         tokio::fs::create_dir_all(parent).await.ok();
                     }
                     tokio::fs::write(target, content).await.map_err(|e| e.to_string())?;
                     Ok(json!(true))
                })
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result)
    });

    // $fs.delete(path) -> void (Deletes from TMP)
    let delete_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_write_path(scope, &fname)?; // Delete from TMP only
                handle.block_on(async {
                     if !target.exists() { return Err("File not found in tmp".into()); }
                     if target.is_dir() {
                         tokio::fs::remove_dir_all(target).await.map_err(|e| e.to_string())
                     } else {
                         tokio::fs::remove_file(target).await.map_err(|e| e.to_string())
                     }
                }).map(|_| json!(true))
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result)
    });
    
    // $fs.list(path) -> Array (Lists from Root)
    let list_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_read_path(scope, &fname)?; // List from Root
                handle.block_on(async {
                     if !target.exists() { return Err("Path not found".into()); }
                     
                     let mut entries = Vec::new();
                     let mut dir = tokio::fs::read_dir(target).await.map_err(|e| e.to_string())?;
                     
                     while let Ok(Some(entry)) = dir.next_entry().await {
                         let meta = entry.metadata().await.map_err(|e| e.to_string())?;
                         entries.push(json!({
                             "name": entry.file_name().to_string_lossy(),
                             "isDir": meta.is_dir(),
                             "size": meta.len()
                         }));
                     }
                     Ok(json!(entries))
                })
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result)
    });

    // $fs.exists(path) -> bool (Checks Root)
    let exists_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, _, _, _, scope)) = &*c.borrow() {
                let target = resolve_read_path(scope, &fname); // Check Root
                match target {
                    Ok(p) => Ok(json!(p.exists())),
                    Err(_) => Ok(json!(false))
                }
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result)
    });

    // $fs.mkdir(path) -> void (Creates in TMP)
    let mkdir_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_write_path(scope, &fname)?; // Mkdir in TMP
                handle.block_on(async {
                    tokio::fs::create_dir_all(target).await.map_err(|e| e.to_string())?;
                    Ok(json!(true))
                })
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result)
    });

    // $fs.stat(path) -> { size, created, modified, isDir } (Checks Root)
    let stat_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_read_path(scope, &fname)?;
                handle.block_on(async {
                     let meta = tokio::fs::metadata(target).await.map_err(|e| e.to_string())?;
                     Ok(json!({
                         "size": meta.len(),
                         "isDir": meta.is_dir(),
                         "created": meta.created().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()),
                         "modified": meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64())
                     }))
                })
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result)
    });

    let obj = ObjectInitializer::new(ctx)
        .function(read_fn, JsString::from("read"), 1)
        .function(write_fn, JsString::from("write"), 2)
        .function(delete_fn, JsString::from("delete"), 1)
        .function(list_fn, JsString::from("list"), 1)
        .function(exists_fn, JsString::from("exists"), 1)
        .function(mkdir_fn, JsString::from("mkdir"), 1)
        .function(stat_fn, JsString::from("stat"), 1)
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
                    
                    // CALL THE NEW TRAIT METHOD
                    // map_err needs explicit type for the closure parameter 'e' to fix E0282
                    db.query_engine(query).await.map_err(|e: Box<dyn std::error::Error + Send + Sync>| e.to_string())
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

// Shared HTTP Request Logic
fn execute_http_request(
    url: String,
    method: String,
    headers_val: Option<serde_json::Map<String, serde_json::Value>>,
    body_val: Option<serde_json::Value>,
    redirect_mode: Option<String>, // [NEW] Argument
) -> Result<serde_json::Value, String> {
    
    ACTIVE_CONTEXT.with(|c| {
        if let Some((_, handle, _, _, _)) = &*c.borrow() {
            handle.block_on(async {
                // 1. Configure Redirect Policy
                // Default to 'follow' if not specified
                let policy = match redirect_mode.as_deref() {
                    Some("manual") => reqwest::redirect::Policy::none(), // Don't follow
                    Some("error") => reqwest::redirect::Policy::custom(|attempt| {
                         attempt.error("Redirects not allowed") // Error on redirect
                    }),
                    _ => reqwest::redirect::Policy::default(), // Follow (limit 10)
                };

                let client = reqwest::Client::builder()
                    .redirect(policy) // Use the configured policy
                    .build()
                    .map_err(|e| format!("Client Build Error: {}", e))?;

                let req_method = reqwest::Method::from_bytes(method.as_bytes())
                    .unwrap_or(reqwest::Method::GET);

                let mut req_builder = client.request(req_method, &url);

                // 2. Add Headers
                if let Some(h_map) = headers_val {
                    for (k, v) in h_map {
                        if let Some(val_str) = v.as_str() {
                            req_builder = req_builder.header(&k, val_str);
                        }
                    }
                }

                // 3. Add Body
                if let Some(b) = body_val {
                    // If it's a string, send raw. If object, send JSON.
                    if let Some(s) = b.as_str() {
                        req_builder = req_builder.body(s.to_string());
                    } else {
                        req_builder = req_builder.json(&b);
                    }
                }

                // 4. Execute
                match req_builder.send().await {
                    Ok(res) => {
                        let status = res.status().as_u16();
                        let status_text = res.status().canonical_reason().unwrap_or("").to_string();
                        let final_url = res.url().to_string();
                        
                        let mut res_headers = serde_json::Map::new();
                        for (name, value) in res.headers() {
                            res_headers.insert(
                                name.as_str().to_string(), 
                                json!(value.to_str().unwrap_or(""))
                            );
                        }

                        let body_text = res.text().await.unwrap_or_default();

                        // Return standardized response object
                        Ok(json!({
                            "ok": status >= 200 && status < 300,
                            "status": status,
                            "statusText": status_text,
                            "url": final_url,
                            "headers": res_headers,
                            "body": body_text 
                        }))
                    },
                    Err(e) => Err(format!("Network Error: {}", e))
                }
            })
        } else {
            Err("Script Context Execution Error".into())
        }
    })
}

fn register_fetch(ctx: &mut Context) -> Result<(), String> {
    let fetch_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let opts = args.get_or_undefined(1).to_json(ctx).unwrap().unwrap_or(json!({}));

        let method = opts.get("method").and_then(|v| v.as_str()).unwrap_or("GET").to_string();
        
        let headers = opts.get("headers")
            .and_then(|h| h.as_object())
            .cloned();

        let body = opts.get("body").cloned();
        
        let redirect_mode = opts.get("redirect")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // CALL SHARED HELPER
        let result = execute_http_request(url, method, headers, body, redirect_mode);

        return_json_promise(ctx, result)
    });

    let fetch_obj = boa_engine::object::FunctionObjectBuilder::new(ctx.realm(), fetch_fn)
        .name("$__native_fetch")
        .length(2)
        .build();

    ctx.register_global_property(
        JsString::from("$__native_fetch"), 
        fetch_obj, 
        Attribute::all()
    ).map_err(|e| e.to_string())
}

fn register_http(ctx: &mut Context) -> Result<(), String> {
    // $http.get(url)
    let get = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // Pass None for redirect (default follow)
        let result = execute_http_request(url, "GET".to_string(), None, None, None);

        // Map result: Extract "body" string or return error
        let mapped_result = result.map(|json_val| {
            // Legacy $http.get returns the raw body string
            json_val.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string()
        });

        // Convert to JsString for Boa
        match mapped_result {
            Ok(s) => return_json_promise(ctx, Ok(serde_json::Value::String(s))),
            Err(e) => return_json_promise(ctx, Err(e))
        }
    });

    // $http.post(url, body)
    let post = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let body_arg = args.get_or_undefined(1).to_json(ctx).unwrap();

        // CALL SHARED HELPER
        let result = execute_http_request(url, "POST".to_string(), None, body_arg, None);

        let mapped_result = result.map(|json_val| {
            json_val.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string()
        });

        match mapped_result {
            Ok(s) => return_json_promise(ctx, Ok(serde_json::Value::String(s))),
            Err(e) => return_json_promise(ctx, Err(e))
        }
    });

    let obj = ObjectInitializer::new(ctx)
        .function(get, JsString::from("get"), 1)
        .function(post, JsString::from("post"), 2)
        .build();
        
    ctx.register_global_property(JsString::from("$http"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_zip(ctx: &mut Context) -> Result<(), String> {
    
    fn _resolve_storage_path(scope: &EventScope) -> String {
        match scope {
            EventScope::Root => "storage/system/uploads".to_string(),
            EventScope::Tenant(id) => format!("storage/tenants/{}/uploads", id),
            EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/uploads", id),
            _ => "storage/tmp".to_string(),
        }
    }

    let get_limit = || -> usize {
        std::env::var("ARCHIVE_LIMIT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10) * 1024 * 1024 
    };

    // 1. CREATE
    let create_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let files_val = args.get_or_undefined(0).to_json(ctx).unwrap().unwrap_or(json!({}));
        let limit = get_limit();

        let result = (|| -> Result<String, String> {
            let files = files_val.as_object().ok_or("Input must be an object {filename: content}")?;
            let mut buffer = Cursor::new(Vec::new());
            
            // Scope for ZipWriter to enforce borrow drop
            {
                let mut zip = ZipWriter::new(&mut buffer);
                let options = FileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(0o755);

                let mut estimated_size = 0;

                for (name, content_val) in files {
                    let content_str = content_val.as_str().unwrap_or("");
                    let data = if content_str.len() % 4 == 0 && !content_str.contains(char::is_whitespace) {
                         BASE64.decode(content_str).unwrap_or_else(|_| content_str.as_bytes().to_vec())
                    } else {
                         content_str.as_bytes().to_vec()
                    };

                    // Check uncompressed size accumulation to prevent DoS before compression
                    estimated_size += data.len();
                    if estimated_size > limit * 2 { // Allow some slack for compression overhead? No, slack for uncompressed input vs output limit.
                        // Actually, if we want to limit the OUTPUT zip size, we can't easily check it inside the loop efficiently without flushing.
                        // Limiting input size is a good proxy.
                        return Err(format!("Input data size exceeds safety limit of {} bytes", limit));
                    }

                    zip.start_file(name, options).map_err(|e| e.to_string())?;
                    zip.write_all(&data).map_err(|e| e.to_string())?;
                }
                zip.finish().map_err(|e| e.to_string())?;
            } // ZipWriter dropped here, releasing borrow on buffer

            let zip_bytes = buffer.into_inner();
            
            // Final check on actual archive size
            if zip_bytes.len() > limit {
                return Err(format!("Final archive size {} exceeds limit {}", zip_bytes.len(), limit));
            }
            
            Ok(BASE64.encode(zip_bytes))
        })();

        return_json_promise(ctx, result.map(|s| serde_json::Value::String(s)))
    });

    // 2. EXTRACT
    let extract_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let b64_str = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let limit = get_limit();

        let result = (|| -> Result<serde_json::Value, String> {
            let bytes = BASE64.decode(&b64_str).map_err(|_| "Invalid Base64".to_string())?;
            if bytes.len() > limit { return Err("Archive exceeds limit".into()); }

            let cursor = Cursor::new(bytes);
            let mut archive = ZipArchive::new(cursor).map_err(|e| format!("Invalid Zip: {}", e))?;
            
            let mut output = serde_json::Map::new();
            let mut total_extracted = 0;

            for i in 0..archive.len() {
                let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
                if file.is_dir() { continue; }
                
                let name = file.name().to_string();
                let mut content_buf = Vec::new();
                
                if file.size() > (limit as u64) { return Err(format!("File {} too large", name)); }
                
                file.read_to_end(&mut content_buf).map_err(|_| "Read fail".to_string())?;
                total_extracted += content_buf.len();
                
                if total_extracted > limit { return Err("Total extracted size exceeds limit".into()); }

                let val = match String::from_utf8(content_buf.clone()) {
                    Ok(s) => json!(s),
                    Err(_) => json!(BASE64.encode(&content_buf))
                };
                output.insert(name, val);
            }
            Ok(serde_json::Value::Object(output))
        })();
        return_json_promise(ctx, result)
    });

    // 3. INSPECT (Metadata)
    let inspect_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let b64_str = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        
        let result = (|| -> Result<serde_json::Value, String> {
            let bytes = BASE64.decode(&b64_str).map_err(|_| "Invalid Base64".to_string())?;
            let cursor = Cursor::new(bytes.clone());
            let mut archive = ZipArchive::new(cursor).map_err(|e| format!("Invalid Zip: {}", e))?;

            let mut files_meta = Vec::new();
            let mut total_uncompressed: u64 = 0;
            let mut total_compressed: u64 = 0;

            for i in 0..archive.len() {
                let file = archive.by_index(i).map_err(|e| e.to_string())?;
                
                let size = file.size();
                let comp_size = file.compressed_size();
                total_uncompressed += size;
                total_compressed += comp_size;

                // FIX: DateTime is a struct, not Option
                let dt = file.last_modified();
                let modified_str = format!("{}-{:02}-{:02} {:02}:{:02}:{:02}", 
                    dt.year(), dt.month(), dt.day(), dt.hour(), dt.minute(), dt.second());

                files_meta.push(json!({
                    "name": file.name(),
                    "size": size,
                    "compressed_size": comp_size,
                    "is_dir": file.is_dir(),
                    "comment": file.comment(),
                    "modified": modified_str,
                    "compression_method": format!("{:?}", file.compression())
                }));
            }

            Ok(json!({
                "total_size": bytes.len(),
                "total_uncompressed": total_uncompressed,
                "total_compressed_content": total_compressed,
                "file_count": archive.len(),
                "comment": String::from_utf8_lossy(archive.comment()).to_string(),
                "files": files_meta
            }))
        })();

        return_json_promise(ctx, result)
    });

    // 4. READ FILE (Scope Aware -> Base64)
    let read_file_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let filename = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    // Use get_storage() from context (ScopedDynamicStorage)
                    let storage = app.get_storage();
                    
                    match storage.get(&filename).await {
                        Ok(bytes) => Ok(json!(BASE64.encode(bytes))),
                        Err(e) => Err(format!("Read failed: {}", e))
                    }
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // 5. SAVE FILE (Base64 -> Scope Aware Storage)
    let save_file_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let filename = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let b64_data = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        let mime_type = args.get(2).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped()).unwrap_or("application/zip".to_string());

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    // 1. Resolve DB (Tenant/Sandbox aware)
                    let db = resolve_db(None, app.clone()).await?;
                    
                    // 2. Resolve Storage (Tenant/Sandbox aware via ScopedDynamicStorage)
                    let storage = app.get_storage();
                    
                    let bytes = BASE64.decode(&b64_data).map_err(|_| "Invalid Base64".to_string())?;
                    let size = bytes.len() as i64;
                    
                    // 3. Generate unique storage filename
                    let path = std::path::Path::new(&filename);
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("bin");
                    let storage_filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
                    
                    // 4. Save to Storage (S3 or Local)
                    storage.save(&storage_filename, &bytes, &mime_type).await.map_err(|e| e.to_string())?;
                    
                    // 5. Register in Metadata DB
                    // Pass None for user_id as script context doesn't implicitly carry a user unless passed in args, 
                    // or we could extract it from ACTIVE_CONTEXT if we stored auth claims there (we don't currently).
                    let id = db.create_file_metadata(&storage_filename, &filename, &mime_type, size, None).await
                        .map_err(|e| e.to_string())?;
                        
                    let public_url = format!("{}{}", storage.get_public_url_base(), storage_filename);

                    Ok(json!({
                        "id": id,
                        "filename": storage_filename, 
                        "original_name": filename,
                        "url": public_url,
                        "size": size
                    }))
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    let obj = ObjectInitializer::new(ctx)
        .function(create_fn, JsString::from("create"), 1)
        .function(extract_fn, JsString::from("extract"), 1)
        .function(inspect_fn, JsString::from("inspect"), 1)
        .function(read_file_fn, JsString::from("readFile"), 1)
        .function(save_file_fn, JsString::from("saveFile"), 2)
        .build();

    ctx.register_global_property(JsString::from("$zip"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_run(ctx: &mut Context) -> Result<(), String> { 
    let script_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let payload = args.get_or_undefined(1).to_json(ctx).unwrap().unwrap_or(json!({}));

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _base_url, _, current_scope)) = &*c.borrow() {
                handle.block_on(async {
                    
                    // 1. Get the DB for the CURRENT scope (e.g. Tenant DB)
                    let local_db = resolve_db(None, app.clone()).await?;
                    
                    // 2. Try to find the script in the LOCAL scope first.
                    let mut script_opt = local_db.get_script_by_name(&name).await.ok().flatten();
                    let mut exec_scope = current_scope.clone();

                    // 3. If NOT FOUND LOCALLY and we are NOT in Root, check Root for a public script.
                    if script_opt.is_none() && !matches!(current_scope, EventScope::Root) {
                        if let Some(shared) = app.get_shared_script(&name).await {
                             // Visibility Check: Only 'public' scripts can be shared.
                             if shared.visibility == "public" {
                                 script_opt = Some(shared);
                                 // CRITICAL: Switch execution scope to Root for this call.
                                 exec_scope = EventScope::Root;
                             }
                        }
                    }

                    if let Some(script) = script_opt {
                        if !script.active { return Err("Script is inactive".into()); }
                        
                        let mut call_payload = payload.clone();
                        if let Some(obj) = call_payload.as_object_mut() {
                             obj.insert("__caller_scope".to_string(), json!(current_scope));
                        }
                        
                        let res = app.execute_shared_script(script.code, call_payload, exec_scope).await?;
                        Ok(res)
                        
                    } else {
                        Err(format!("Script '{}' not found or not accessible.", name))
                    }
                })
            } else {
                Err("Context lost".into())
            }
        });

        return_json_promise(ctx, result)
    });

    let obj = ObjectInitializer::new(ctx)
        .function(script_fn, JsString::from("script"), 2)
        .build();

    ctx.register_global_property(JsString::from("$run"), obj, Attribute::all()).map_err(|e| e.to_string())
}

fn register_root(ctx: &mut Context) -> Result<(), String> {
    let is_root = ACTIVE_CONTEXT.with(|c| c.borrow().as_ref().map(|t| t.4 == EventScope::Root).unwrap_or(false));
    
    if is_root {
        // Updated Signature: createTenant(id: string, config?: object)
        let create_tenant = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            
            // Extract config object if present
            let config_val = args.get(1).and_then(|v| v.to_json(ctx).ok()).flatten();
            
            let (name, tier, owner_id) = if let Some(serde_json::Value::Object(map)) = config_val {
                (
                    map.get("name").and_then(|v| v.as_str()).map(String::from),
                    map.get("tier").and_then(|v| v.as_str()).map(String::from),
                    map.get("owner_id").and_then(|v| v.as_i64()) // Expecting number
                )
            } else {
                (None, None, None)
            };

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        // 1. Create Physical Resources
                        app.admin_create_tenant(id.clone()).await.map_err(|e| e.to_string())?;
                        
                        // 2. Register Metadata with injected values
                        app.get_db().register_tenant(&id, owner_id, name, tier).await.map_err(|e| e.to_string())?;
                        
                        Ok(true)
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res.map(|b| serde_json::Value::Bool(b)))
        });

        // createSandbox(id: string, config?: object)
        let create_sandbox = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            // Extract config object
            let config_val = args.get(1).and_then(|v| v.to_json(ctx).ok()).flatten();
            let (name, owner_id, expires_at) = if let Some(serde_json::Value::Object(map)) = config_val {
                (
                    map.get("name").and_then(|v| v.as_str()).map(String::from),
                    map.get("owner_id").and_then(|v| v.as_i64()),
                    map.get("expires_at").and_then(|v| v.as_str()).map(String::from) // ISO String
                )
            } else {
                (None, None, None)
            };
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        // 1. Create Physical Resources
                        app.admin_create_sandbox(id.clone()).await.map_err(|e| e.to_string())?;

                        // 2. Register Metadata
                        app.get_db().register_sandbox(&id, owner_id, name, expires_at).await.map_err(|e| e.to_string())?;
                        
                        Ok(true)
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res.map(|b| serde_json::Value::Bool(b)))
        });

        // 1. createKey(name, config?)
        let create_key = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let config_val = args.get(1).and_then(|v| v.to_json(ctx).ok()).flatten();
            
            let (role, scope, bypass) = if let Some(serde_json::Value::Object(map)) = config_val {
                (
                    map.get("role").and_then(|v| v.as_str()).unwrap_or("admin").to_string(),
                    map.get("scope").and_then(|v| v.as_str()).unwrap_or("root").to_string(),
                    map.get("bypass_cors").and_then(|v| v.as_bool()).unwrap_or(false)
                )
            } else {
                ("admin".to_string(), "root".to_string(), false)
            };

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let (raw_key, info) = app.get_db().create_api_key(&name, &role, &scope, bypass).await.map_err(|e| e.to_string())?;
                        Ok(json!({
                            "key": raw_key,
                            "info": info
                        }))
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        });

        // 2. updateKey(id, updates)
        let update_key = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0) as i64;
            let config_val = args.get(1).and_then(|v| v.to_json(ctx).ok()).flatten();

            let (name, role, scope, bypass) = if let Some(serde_json::Value::Object(map)) = config_val {
                (
                    map.get("name").and_then(|v| v.as_str()).map(String::from),
                    map.get("role").and_then(|v| v.as_str()).map(String::from),
                    map.get("scope").and_then(|v| v.as_str()).map(String::from),
                    map.get("bypass_cors").and_then(|v| v.as_bool())
                )
            } else {
                (None, None, None, None)
            };

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.get_db().update_api_key(id, name, role, scope, bypass).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::Value::Bool(true))
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        });

        // 3. deleteKey(id)
        let delete_key = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0) as i64;
            
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.get_db().delete_api_key(id).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::Value::Bool(true))
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        });
        
        // 4. listKeys()
        let list_keys = NativeFunction::from_copy_closure(move |_, _, ctx| {
             let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let keys = app.get_db().list_api_keys().await.map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(keys).unwrap())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res)
        });

        // 5. updateTenant(id, updates)
        let update_tenant = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let updates = args.get_or_undefined(1).to_json(ctx).unwrap().unwrap_or(json!({}));
            
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.admin_update_tenant(id, updates).await.map_err(|e| e.to_string())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res.map(|_| serde_json::Value::Bool(true)))
        });

        // 6. deleteTenant(id)
        let delete_tenant = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.admin_delete_tenant(id).await.map_err(|e| e.to_string())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res.map(|_| serde_json::Value::Bool(true)))
        });

        // 7. updateSandbox(id, updates)
        let update_sandbox = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let updates = args.get_or_undefined(1).to_json(ctx).unwrap().unwrap_or(json!({}));
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.admin_update_sandbox(id, updates).await.map_err(|e| e.to_string())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res.map(|_| serde_json::Value::Bool(true)))
        });

        // 8. deleteSandbox(id)
        let delete_sandbox = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.admin_delete_sandbox(id).await.map_err(|e| e.to_string())
                    })
                } else { Err("Context lost".into()) }
            });
            return_json_promise(ctx, res.map(|_| serde_json::Value::Bool(true)))
        });

        // 9. getTenantUsage(id) -> number (bytes)
        let get_tenant_usage = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.admin_get_tenant_usage(id).await
                    })
                } else { Err("Context lost".into()) }
            });
            
            // Return number (or null on error/empty)
            match res {
                Ok(bytes) => Ok(JsValue::from(bytes as f64)), // JS uses f64 for numbers
                Err(e) => Err(JsError::from_opaque(JsString::from(e).into()))
            }
        });

        // 10. getSandboxUsage(id)
        let get_sandbox_usage = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.admin_get_sandbox_usage(id).await
                    })
                } else { Err("Context lost".into()) }
            });
            
            match res {
                Ok(bytes) => Ok(JsValue::from(bytes as f64)),
                Err(e) => Err(JsError::from_opaque(JsString::from(e).into()))
            }
        });

        let obj = ObjectInitializer::new(ctx)
        // API Keys
            .function(create_key, JsString::from("createKey"), 2)
            .function(update_key, JsString::from("updateKey"), 2)
            .function(delete_key, JsString::from("deleteKey"), 1)
            .function(list_keys, JsString::from("listKeys"), 0)
            // Tenant Management
            .function(create_tenant, JsString::from("createTenant"), 2) 
            .function(update_tenant, JsString::from("updateTenant"), 2)
            .function(delete_tenant, JsString::from("deleteTenant"), 1)
            .function(get_tenant_usage, JsString::from("getTenantDiskUsage"), 1)
            // Sandbox Management
            .function(create_sandbox, JsString::from("createSandbox"), 2)
            .function(update_sandbox, JsString::from("updateSandbox"), 2)
            .function(delete_sandbox, JsString::from("deleteSandbox"), 1)
            .function(get_sandbox_usage, JsString::from("getSandboxDiskUsage"), 1)
            .build();
        ctx.register_global_property(JsString::from("$root"), obj, Attribute::all()).map_err(|e| e.to_string())
    } else {
        ctx.register_global_property(JsString::from("$root"), JsValue::null(), Attribute::all()).map_err(|e| e.to_string())
    }
}

// Function to register $cache
fn register_cache(ctx: &mut Context) -> Result<(), String> {
    // 1. GET
    let get = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    app.cache_get(&key).await
                })
            } else { None }
        });
        
        match res {
            Some(val) => Ok(JsValue::from(JsString::from(val))),
            None => Ok(JsValue::null())
        }
    });

    // 2. SET
    let set = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // Accept string or JSON object (stringify it)
        let val = args.get_or_undefined(1);
        let val_str = if val.is_object() {
             serde_json::to_string(&val.to_json(ctx).unwrap()).unwrap_or_default()
        } else {
             val.to_string(ctx)?.to_std_string_escaped()
        };

        let ttl = args.get(2).and_then(|v| v.to_number(ctx).ok()).map(|n| n as u64);

        ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    app.cache_set(&key, &val_str, ttl).await;
                })
            }
        });
        Ok(JsValue::undefined())
    });

    // 3. DELETE
    let del = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async { app.cache_del(&key).await; })
            }
        });
        Ok(JsValue::undefined())
    });

    // 4. INCREMENT (For Quotas)
    let incr = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let delta = args.get(1).and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0) as i64;

        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    app.cache_incr(&key, delta).await
                })
            } else { 0 }
        });
        Ok(JsValue::from(res))
    });

    let obj = ObjectInitializer::new(ctx)
        .function(get, JsString::from("get"), 1)
        .function(set, JsString::from("set"), 3)
        .function(del, JsString::from("delete"), 1)
        .function(incr, JsString::from("incr"), 2)
        .build();

    ctx.register_global_property(JsString::from("$cache"), obj, Attribute::all()).map_err(|e| e.to_string())
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