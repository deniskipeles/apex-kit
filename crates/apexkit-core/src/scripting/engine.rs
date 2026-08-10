use crate::realtime::EventScope;
use regex::Regex;
use serde_json::{Value as JsonValue, json};
use std::sync::Arc;

use super::builtins::db::resolve_db;
use boa_engine::{
    Context, JsArgs, JsError, JsResult, JsString, JsValue, NativeFunction,
    builtins::promise::PromiseState, object::ObjectInitializer, property::Attribute,
};
use std::collections::HashMap;

use super::builtins::{
    register_ai, register_cache, register_cmd, register_console, register_db, register_env,
    register_fetch, register_file_tools, register_fs, register_http, register_mail,
    register_realtime, register_root, register_util, register_zip,
};
use super::context::{ACTIVE_CONTEXT, ScriptContext};

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

            // Auto-resolve JWT Auth claims
            this.auth = null;
            const authHeader = this.headers.get("authorization");
            if (authHeader && authHeader.startsWith("Bearer ")) {
                try {
                    const token = authHeader.split(" ")[1];
                    const payload = token.split(".")[1];
                    // Safely pad and format Base64Url to standard Base64
                    let b64 = payload.replace(/-/g, '+').replace(/_/g, '/');
                    while (b64.length % 4) b64 += '=';
                    
                    const decoded = JSON.parse(globalThis.$util ? globalThis.$util.base64Decode(b64) : atob(b64));
                    this.auth = {
                        id: decoded.uid,
                        email: decoded.sub,
                        role: decoded.role,
                        scope: decoded.scope
                    };
                } catch (e) {
                    console.error("[ApexKit] Failed to decode auth token in Request:", e);
                }
            }
        }
        async json() { return typeof this.bodyData === 'string' ? JSON.parse(this.bodyData) : this.bodyData; }
        async text() { return typeof this.bodyData === 'string' ? this.bodyData : JSON.stringify(this.bodyData); }
    }

    class ApexKit {
        constructor(contextId = null) { 
            this.contextId = contextId;
            // Determine which DB object to use
            // If contextId is present, we MUST use $root.db (privileged).
            // If contextId is null, we use $db (scoped).
            this.dbRef = this.contextId ? globalThis.$root?.db : globalThis.$db;
            
            if (this.contextId && !this.dbRef) {
                // If user tried to switch context but $root is missing (i.e. running in tenant script)
                throw new Error("Access Denied: Root scope required for context switching.");
            }
        }
        
        tenant(id) { return new ApexKit("tenant:" + id); }
        sandbox(id) { return new ApexKit("sandbox:" + id); }

        // Helper to pass context ONLY if using root db
        _call(method, ...args) {
             if (this.contextId) {
                 return method(this.contextId, ...args);
             } else {
                 return method(...args);
             }
        }

        collection(name) {
            const self = this;
            const rec = this.dbRef.records;
            return {
                list: async (opts) => self._call(rec.list, name, opts),
                get: async (id, opts) => self._call(rec.get, name, id, opts?.expand),
                create: async (data) => self._call(rec.create, name, data),
                update: async (id, data) => self._call(rec.update, name, id, data),
                delete: async (id) => self._call(rec.delete, name, id),
                searchVector: async (f, v, l) => self._call(rec.searchVector, name, f, v, l),
                getVector: async (id) => self._call(rec.getVector, name, id)
            };
        }
        
        async query(queryObject) {
            return this._call(this.dbRef.query, queryObject);
        }
        
        get users() {
            const self = this;
            const u = this.dbRef.users;
            return {
                create: async (e, p, r) => self._call(u.create, e, p, r),
                get: async (e) => self._call(u.get, e)
            }
        }
        
        get collections() {
            const self = this;
            const c = this.dbRef.collections;
            return {
                list: async () => self._call(c.list)
            }
        }

        get files() {
            const self = this;
            const f = this.dbRef.files;
            return {
                list: async (l, o) => self._call(f.list, l, o)
            }
        }
    }
    
    class URL {
        constructor(urlStr, baseStr) {
            let full = urlStr || "";
            if (baseStr && !full.includes('://')) {
                full = baseStr.replace(/\/$/, '') + '/' + full.replace(/^\//, '');
            }
            this.href = full;
            const parts = full.split('?');
            this.pathname = parts[0] || '';
            const queryStr = parts[1] || '';
            
            const params = new Map();
            if (queryStr) {
                queryStr.split('&').forEach(pair => {
                    const [k, v] = pair.split('=');
                    if (k) params.set(decodeURIComponent(k), decodeURIComponent(v || ''));
                });
            }
            this.searchParams = {
                get: (k) => params.get(k) || null,
                has: (k) => params.has(k),
                getAll: (k) => params.has(k) ? [params.get(k)] : []
            };
        }
    }
    globalThis.URL = URL;

    globalThis.ApexKit = ApexKit;
    // Default instance uses current scope ($db)
    globalThis.$apex = new ApexKit();

    globalThis.Headers = Headers;
    globalThis.Request = Request;
    globalThis.Response = Response;
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
        context: Arc<dyn ScriptContext>,
        base_url: Option<String>,
        headers: Option<HashMap<String, String>>,
        method: Option<String>,
        url: Option<String>,
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
            let request_cls = ctx
                .global_object()
                .get(JsString::from("Request"), ctx)
                .unwrap();
            let req_init = ObjectInitializer::new(ctx)
                .property(
                    JsString::from("method"),
                    JsString::from(method.unwrap_or_else(|| "POST".to_string())),
                    Attribute::all(),
                )
                .property(JsString::from("body"), js_body, Attribute::all())
                .property(JsString::from("headers"), js_headers, Attribute::all())
                .build();

            let url_val = url.unwrap_or_else(|| "http://localhost".to_string());

            let request_obj = request_cls
                .as_constructor()
                .unwrap()
                .construct(
                    &[
                        JsValue::from(JsString::from(url_val)),
                        JsValue::from(req_init),
                    ],
                    Some(&request_cls.as_object().unwrap()),
                    ctx,
                )
                .map_err(|e| format!("Failed to create Request: {}", e))?;

            let handler = ctx
                .global_object()
                .get(JsString::from("__mainHandler"), ctx);
            let promise = match handler {
                Ok(h) if h.is_callable() => {
                    h.as_callable()
                        .unwrap()
                        .call(&JsValue::undefined(), &[request_obj.into()], ctx)
                }
                _ => return Err("No 'export default' found".to_string()),
            };

            let _ = ctx.run_jobs();
            let final_val = Self::resolve_promise(promise, ctx)?;

            // THE FIX: Intercept JS Response objects, extract status, and parse stringified bodies safely
            if let Some(obj) = final_val.as_object()
                && obj
                    .has_property(JsString::from("body"), ctx)
                    .unwrap_or(false)
                && obj
                    .has_property(JsString::from("status"), ctx)
                    .unwrap_or(false)
            {
                let body = obj.get(JsString::from("body"), ctx).unwrap_or_default();
                let status = obj.get(JsString::from("status"), ctx).unwrap_or_default();

                let status_code = status.to_number(ctx).unwrap_or(200.0) as u16;

                // Safely parse body if it is a stringified JSON (fixes double stringification)
                let body_json = if let Some(js_str) = body.as_string() {
                    let rust_str = js_str.to_std_string_escaped();
                    serde_json::from_str(&rust_str).unwrap_or(serde_json::Value::String(rust_str))
                } else {
                    body.to_json(ctx)
                        .unwrap_or(None)
                        .unwrap_or(serde_json::Value::Null)
                };

                return Ok(serde_json::json!({
                    "__is_apex_response": true,
                    "status": status_code,
                    "body": body_json
                }));
            }

            let json = final_val
                .to_json(ctx)
                .unwrap_or(None)
                .unwrap_or(serde_json::Value::Null);
            Ok(serde_json::to_value(json).unwrap_or(JsonValue::Null))
        })
        .await
    }

    pub async fn run_hook(
        &self,
        code: &str,
        event_data: JsonValue,
        context: Arc<dyn ScriptContext>,
        base_url: Option<String>,
        scope: Option<EventScope>,
    ) -> Result<Option<JsonValue>, String> {
        let _actual_scope = scope.unwrap_or(EventScope::Root);
        let wrapped_code = format!(
            r#"
            (async () => {{
                {}
                const e = globalThis.__hook_context__;
                if (globalThis.__mainHandler) {{ return await globalThis.__mainHandler(e); }}
                return null;
            }})()
        "#,
            code
        );

        self.execute_js_task(&wrapped_code, context, base_url, move |ctx| {
            let js_event = JsValue::from_json(&event_data, ctx).map_err(|e| e.to_string())?;
            ctx.register_global_property(
                JsString::from("__hook_context__"),
                js_event.clone(),
                Attribute::all(),
            )
            .unwrap();

            let handler = ctx
                .global_object()
                .get(JsString::from("__mainHandler"), ctx);
            let promise = match handler {
                Ok(h) if h.is_callable() => {
                    h.as_callable()
                        .unwrap()
                        .call(&JsValue::undefined(), &[js_event], ctx)
                }
                _ => return Ok(None),
            };

            let _ = ctx.run_jobs();
            let final_val = Self::resolve_promise(promise, ctx)?;

            if final_val.is_null() || final_val.is_undefined() {
                return Ok(None);
            }

            // [FIX] Only check boolean if the value is actually a boolean type.
            // Objects/Strings/Numbers should NOT trigger this check.
            if let Some(b) = final_val.as_boolean()
                && !b
            {
                return Err("Hook blocked operation".to_string());
            }

            if final_val.is_object() {
                let json = final_val.to_json(ctx).unwrap().unwrap();
                return Ok(Some(json));
            }
            Ok(None)
        })
        .await
    }

    async fn execute_js_task<F, R>(
        &self,
        code: &str,
        context: Arc<dyn ScriptContext>,
        base_url: Option<String>,
        task_logic: F,
    ) -> Result<R, String>
    where
        F: FnOnce(&mut Context) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let re_config = Regex::new(r"export\s+const\s+graphql\s*=\s*(\{[\s\S]*?\})(?:;|\n|$)")
            .map_err(|e| e.to_string())?;
        let code_cleaned = re_config.replace_all(code, "");
        let processed_code =
            code_cleaned.replacen("export default", "globalThis.__mainHandler =", 1);

        let handle = tokio::runtime::Handle::current();

        // Get TX from context
        let tx = Some(context.get_realtime_tx());
        let execution_scope = context.get_scope();

        // 1. Get Timeout
        let timeout_secs = std::env::var("SCRIPT_EXECUTION_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30); // Default 30s

        let execution_future = tokio::task::spawn_blocking(move || -> Result<R, String> {
            let mut context_boa = Context::default();
            ACTIVE_CONTEXT.with(|c| {
                *c.borrow_mut() = Some((context, handle.clone(), base_url, tx, execution_scope));
            });
            Self::setup_boa(&mut context_boa)?;

            // Check for interrupts or inject limiter? Boa has some support but simple OS thread timeout is hard.
            // However, spawn_blocking runs on a thread. We can't easily kill it if it loops infinitely in pure JS (no async yields).
            // But we can timeout the *waiting* for it. The thread might leak if it's an infinite loop,
            // but the API will respond with timeout.
            // Ideally, we inject an instruction limit or use `context_boa.set_interrupt_handler`.

            if let Err(e) =
                context_boa.eval(boa_engine::Source::from_bytes(processed_code.as_bytes()))
            {
                return Err(format!("Script Syntax Error: {}", e));
            }
            task_logic(&mut context_boa)
        });

        // 2. Wrap in Timeout
        match tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            execution_future,
        )
        .await
        {
            Ok(join_res) => match join_res {
                Ok(inner_res) => inner_res,
                Err(e) => Err(format!("System Panic: {}", e)),
            },
            Err(_) => Err(format!("Script timed out after {} seconds.", timeout_secs)),
        }
    }

    fn setup_boa(ctx: &mut Context) -> Result<(), String> {
        ctx.eval(boa_engine::Source::from_bytes(JS_PRELUDE.as_bytes()))
            .map_err(|e| format!("Prelude Error: {}", e))?;

        register_console(ctx)?;
        register_util(ctx)?;
        register_http(ctx)?;
        register_fetch(ctx)?;
        register_file_tools(ctx)?;
        register_fs(ctx)?;
        register_zip(ctx)?;
        register_db(ctx)?;
        register_cmd(ctx)?;
        register_run(ctx)?;
        register_root(ctx)?;
        register_env(ctx)?;
        register_ai(ctx)?;
        register_mail(ctx)?;
        register_realtime(ctx)?;
        register_cache(ctx)?;

        Ok(())
    }

    fn resolve_promise(
        val: Result<JsValue, JsError>,
        _ctx: &mut Context,
    ) -> Result<JsValue, String> {
        let js_val = val.map_err(|e| e.to_string())?;
        if let Some(p) = js_val.as_promise() {
            match p.state() {
                PromiseState::Fulfilled(v) => Ok(v),
                PromiseState::Rejected(err) => Err(format!("Script Rejected: {}", err.display())),
                PromiseState::Pending => {
                    Err("Script did not complete (Pending Promise)".to_string())
                }
            }
        } else {
            Ok(js_val)
        }
    }
}

// --- JS Return Helper ---
pub fn return_json_promise(
    ctx: &mut Context,
    result: Result<serde_json::Value, String>,
) -> JsResult<JsValue> {
    let (promise, resolvers) = boa_engine::object::builtins::JsPromise::new_pending(ctx);
    match result {
        Ok(json_val) => {
            let js_val = JsValue::from_json(&json_val, ctx).unwrap_or(JsValue::null());
            resolvers
                .resolve
                .call(&JsValue::undefined(), &[js_val], ctx)?;
        }
        Err(e) => {
            let err_msg = JsString::from(e);
            resolvers
                .reject
                .call(&JsValue::undefined(), &[err_msg.into()], ctx)?;
        }
    }
    Ok(promise.into())
}

// --- MODULE REGISTRATIONS ---

fn register_run(ctx: &mut Context) -> Result<(), String> {
    let script_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let name = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let payload = args
            .get_or_undefined(1)
            .to_json(ctx)
            .unwrap()
            .unwrap_or(json!({}));

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
                        // Visibility Check: Only 'public' scripts can be shared.
                        if let Some(shared) = app.get_shared_script(&name).await
                            && shared.visibility == "public"
                        {
                            script_opt = Some(shared);
                            // CRITICAL: Switch execution scope to Root for this call.
                            exec_scope = EventScope::Root;
                        }
                    }

                    if let Some(script) = script_opt {
                        if !script.active {
                            return Err("Script is inactive".into());
                        }

                        let mut call_payload = payload.clone();
                        if let Some(obj) = call_payload.as_object_mut() {
                            obj.insert("__caller_scope".to_string(), json!(current_scope));
                        }

                        let res = app
                            .execute_shared_script(script.code, call_payload, exec_scope)
                            .await?;
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

    ctx.register_global_property(JsString::from("$run"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
