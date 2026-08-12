use crate::realtime::EventScope;
use regex::Regex;
use serde_json::{Value as JsonValue, json};
use std::collections::HashMap;
use std::sync::Arc;

use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Function, Object, Promise};
use rquickjs_serde::{from_value, to_value};

use super::builtins::{
    register_ai, register_cache, register_cmd, register_console, register_db, register_env,
    register_fetch, register_file_tools, register_fs, register_http, register_mail,
    register_realtime, register_root, register_util, register_zip,
};
use super::context::ScriptContext;

// Helper struct to wrap non-Send QuickJS futures for Tokio's multithreaded runtime
struct SendWrapper<F>(F);
unsafe impl<F> Send for SendWrapper<F> {}

impl<F: std::future::Future> std::future::Future for SendWrapper<F> {
    type Output = F::Output;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
        inner.poll(cx)
    }
}

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

            this.auth = null;
            const authHeader = this.headers.get("authorization");
            if (authHeader && authHeader.startsWith("Bearer ")) {
                try {
                    const token = authHeader.split(" ")[1];
                    const payload = token.split(".")[1];
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
        }

        get dbRef() {
            return this.contextId ? globalThis.$root?.db : globalThis.$db;
        }
        
        tenant(id) { return new ApexKit("tenant:" + id); }
        sandbox(id) { return new ApexKit("sandbox:" + id); }

        _call(method, ...args) {
             if (this.contextId) {
                 return method(this.contextId, ...args);
             } else {
                 return method(...args);
             }
        }

        collection(name) {
            const self = this;
            return {
                list: async (opts) => self._call(self.dbRef.records.list, name, opts),
                get: async (id, opts) => self._call(self.dbRef.records.get, name, id, opts?.expand),
                create: async (data) => self._call(self.dbRef.records.create, name, data),
                update: async (id, data) => self._call(self.dbRef.records.update, name, id, data),
                delete: async (id) => self._call(self.dbRef.records.delete, name, id),
                searchVector: async (f, v, l) => self._call(self.dbRef.records.searchVector, name, f, v, l),
                getVector: async (id) => self._call(self.dbRef.records.getVector, name, id)
            };
        }
        
        async query(queryObject) {
            return this._call(this.dbRef.query, queryObject);
        }
        
        get users() {
            const self = this;
            return {
                create: async (e, p, r) => self._call(self.dbRef.users.create, e, p, r),
                get: async (e) => self._call(self.dbRef.users.get, e)
            };
        }
        
        get collections() {
            const self = this;
            return {
                list: async () => self._call(self.dbRef.collections.list)
            };
        }

        get files() {
            const self = this;
            return {
                list: async (l, o) => self._call(self.dbRef.files.list, l, o)
            };
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
    globalThis.$apex = new ApexKit();
    globalThis.Headers = Headers;
    globalThis.Request = Request;
    globalThis.Response = Response;

    globalThis.fetch = async function(url, options = {}) {
        let headersObj = {};
        if (options.headers) {
            if (options.headers instanceof Headers) {
                options.headers.forEach((v, k) => headersObj[k] = v);
            } else {
                headersObj = options.headers;
            }
        }

        const nativeRes = await $__native_fetch(url, {
            method: options.method || 'GET',
            headers: headersObj,
            body: options.body
        });

        return new Response(nativeRes.body, {
            status: nativeRes.status,
            statusText: nativeRes.statusText,
            headers: nativeRes.headers,
            url: nativeRes.url
        });
    };
"#;

#[derive(Clone)]
pub struct ScriptEngine {
    runtime: AsyncRuntime,
}

unsafe impl Send for ScriptEngine {}
unsafe impl Sync for ScriptEngine {}

impl ScriptEngine {
    // Add DB to with_vfs signature
    pub async fn with_vfs(
        vfs: crate::scripting::module_loader::VfsState,
        db: Arc<dyn crate::Db>,
    ) -> Self {
        let runtime = AsyncRuntime::new().unwrap();

        let rt = runtime.clone();
        SendWrapper(async move {
            rt.set_max_stack_size(0).await;
            rt.set_loader(
                crate::scripting::module_loader::ApexModuleResolver,
                crate::scripting::module_loader::ApexModuleLoader { vfs, db }, // <-- Pass db here
            )
            .await;
        })
        .await;

        Self { runtime }
    }

    pub async fn run_script(
        &self,
        code: &str,
        input_data: JsonValue,
        context: Arc<dyn ScriptContext>,
        _base_url: Option<String>,
        headers: Option<HashMap<String, String>>,
        method: Option<String>,
        url: Option<String>,
    ) -> Result<JsonValue, String> {
        let re_config = Regex::new(r"export\s+const\s+graphql\s*=\s*(\{[\s\S]*?\})(?:;|\n|$)")
            .map_err(|e| e.to_string())?;
        let code_cleaned = re_config.replace_all(code, "");
        let processed_code =
            code_cleaned.replacen("export default", "globalThis.__mainHandler =", 1);

        let timeout_secs = std::env::var("SCRIPT_EXECUTION_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);

        let total_cpu_budget_ms = std::env::var("SCRIPT_MAX_CPU_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300);
        let quantum_slice_ms = 10;

        let scheduler = super::scheduler::get_quantum_scheduler();
        let task_id = scheduler.register_task(
            std::time::Duration::from_millis(total_cpu_budget_ms),
            std::time::Duration::from_millis(quantum_slice_ms),
        );
        let _guard = super::scheduler::QuantumGuard::new(task_id);

        let runtime = self.runtime.clone();

        // ALL rquickjs async calls (set_interrupt_handler, AsyncContext::full, async_with!) MUST BE INSIDE SendWrapper
        let task = SendWrapper(async move {
            runtime
                .set_interrupt_handler(Some(Box::new(move || {
                    super::scheduler::get_quantum_scheduler().check_and_yield(task_id)
                })))
                .await;

            let ctx = AsyncContext::full(&runtime)
                .await
                .map_err(|e| e.to_string())?;

            #[allow(deprecated)]
            rquickjs::async_with!(ctx => |js_ctx| {
                setup_quickjs(&js_ctx, context.clone())?;

                js_ctx
                    .eval::<(), _>(processed_code.as_str())
                    .catch(&js_ctx)
                    .map_err(|e| format!("Script Syntax Error: {}", e))?;

                let req_data = json!({
                    "url": url.unwrap_or_else(|| "http://localhost".to_string()),
                    "method": method.unwrap_or_else(|| "POST".to_string()),
                    "headers": headers.unwrap_or_default(),
                    "bodyData": input_data
                });

                let js_req_data = to_value(js_ctx.clone(), &req_data).map_err(|e| e.to_string())?;
                let globals = js_ctx.globals();

                let req_class: Function = js_ctx.eval("(data) => new Request(data)").unwrap();
                let request_obj: Object = req_class.call((js_req_data,)).map_err(|e| e.to_string())?;

                let handler: Function = globals
                    .get("__mainHandler")
                    .map_err(|_| "No 'export default' found".to_string())?;

                let promise: Promise = handler
                    .call((request_obj,))
                    .catch(&js_ctx)
                    .map_err(|e| e.to_string())?;

                let result_val: rquickjs::Value = promise
                    .into_future::<rquickjs::Value>()
                    .await
                    .catch(&js_ctx)
                    .map_err(|e| format!("Script Execution Error: {}", e))?;

                if let Some(obj) = result_val.as_object() {
                    if obj.contains_key("body").unwrap_or(false)
                        && obj.contains_key("status").unwrap_or(false)
                    {
                        let status_val: rquickjs::Value = obj
                            .get("status")
                            .unwrap_or(rquickjs::Value::new_null(js_ctx.clone()));
                        let status_code: u16 = from_value(status_val).unwrap_or(200);

                        let body_val: rquickjs::Value = obj
                            .get("body")
                            .unwrap_or(rquickjs::Value::new_null(js_ctx.clone()));

                        let body_json = if let Some(js_str) = body_val.as_string() {
                            let rust_str = js_str.to_string().unwrap_or_default();
                            serde_json::from_str(&rust_str)
                                .unwrap_or(serde_json::Value::String(rust_str))
                        } else {
                            from_value(body_val).unwrap_or(serde_json::Value::Null)
                        };

                        return Ok::<JsonValue, String>(json!({
                            "__is_apex_response": true,
                            "status": status_code,
                            "body": body_json
                        }));
                    }
                }

                let json_res: JsonValue = from_value(result_val).unwrap_or(JsonValue::Null);
                Ok::<JsonValue, String>(json_res)
            }).await
        });

        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), task).await {
            Ok(res) => res,
            Err(_) => Err(format!("Script timed out after {} seconds.", timeout_secs)),
        }
    }

    pub async fn run_hook(
        &self,
        code: &str,
        event_data: JsonValue,
        context: Arc<dyn ScriptContext>,
        _base_url: Option<String>,
        _scope: Option<EventScope>,
    ) -> Result<Option<JsonValue>, String> {
        let re_config = Regex::new(r"export\s+const\s+graphql\s*=\s*(\{[\s\S]*?\})(?:;|\n|$)")
            .map_err(|e| e.to_string())?;
        let code_cleaned = re_config.replace_all(code, "");
        let processed_code =
            code_cleaned.replacen("export default", "globalThis.__mainHandler =", 1);

        let timeout_secs = std::env::var("SCRIPT_EXECUTION_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);

        let runtime = self.runtime.clone();

        let task = SendWrapper(async move {
            let ctx = AsyncContext::full(&runtime)
                .await
                .map_err(|e| e.to_string())?;

            #[allow(deprecated)]
            rquickjs::async_with!(ctx => |js_ctx| {
                setup_quickjs(&js_ctx, context.clone())?;

                js_ctx
                    .eval::<(), _>(processed_code.as_str())
                    .catch(&js_ctx)
                    .map_err(|e| format!("Script Syntax Error: {}", e))?;

                let js_event = to_value(js_ctx.clone(), &event_data).map_err(|e| e.to_string())?;
                let globals = js_ctx.globals();

                if let Ok(handler) = globals.get::<_, Function>("__mainHandler") {
                    let promise: Promise = handler
                        .call((js_event,))
                        .catch(&js_ctx)
                        .map_err(|e| e.to_string())?;

                    let result_val: rquickjs::Value = promise
                        .into_future::<rquickjs::Value>()
                        .await
                        .catch(&js_ctx)
                        .map_err(|e| e.to_string())?;

                    if result_val.is_null() || result_val.is_undefined() {
                        return Ok::<Option<JsonValue>, String>(None);
                    }

                    if let Some(b) = result_val.as_bool() {
                        if !b {
                            return Err("Hook blocked operation".to_string());
                        }
                    }

                    let json_val: JsonValue = from_value(result_val).map_err(|e| e.to_string())?;
                    Ok::<Option<JsonValue>, String>(Some(json_val))
                } else {
                    Ok::<Option<JsonValue>, String>(None)
                }
            })
            .await
        });

        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), task).await {
            Ok(res) => res,
            Err(_) => Err(format!("Hook timed out after {} seconds.", timeout_secs)),
        }
    }
}

fn setup_quickjs<'js>(
    ctx: &rquickjs::Ctx<'js>,
    app_ctx: Arc<dyn ScriptContext>,
) -> Result<(), String> {
    ctx.eval::<(), _>(JS_PRELUDE)
        .catch(ctx)
        .map_err(|e| format!("Prelude Error: {}", e))?;

    register_console(ctx, app_ctx.clone())?;
    register_util(ctx, app_ctx.clone())?;
    register_http(ctx, app_ctx.clone())?;
    register_fetch(ctx, app_ctx.clone())?;
    register_file_tools(ctx, app_ctx.clone())?;
    register_fs(ctx, app_ctx.clone())?;
    register_zip(ctx, app_ctx.clone())?;
    register_db(ctx, app_ctx.clone())?;
    register_cmd(ctx, app_ctx.clone())?;
    register_run(ctx, app_ctx.clone())?;
    register_root(ctx, app_ctx.clone())?;
    register_env(ctx, app_ctx.clone())?;
    register_ai(ctx, app_ctx.clone())?;
    register_mail(ctx, app_ctx.clone())?;
    register_realtime(ctx, app_ctx.clone())?;
    register_cache(ctx, app_ctx.clone())?;

    Ok(())
}

fn register_run<'js>(
    ctx: &rquickjs::Ctx<'js>,
    app_ctx: Arc<dyn ScriptContext>,
) -> Result<(), String> {
    let globals = ctx.globals();
    let run_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let app_ctx_clone = app_ctx.clone();
    let script_fn = Function::new(
        ctx.clone(),
        rquickjs::function::Async(
            move |js_ctx: rquickjs::Ctx<'js>, name: String, payload: rquickjs::Value<'js>| {
                let app = app_ctx_clone.clone();
                async move {
                    let payload_json: JsonValue = from_value(payload).unwrap_or(json!({}));
                    let current_scope = app.get_scope();

                    let local_db = app.get_db();
                    let mut script_opt = local_db.get_script_by_name(&name).await.ok().flatten();
                    let mut exec_scope = current_scope.clone();

                    if script_opt.is_none() && !matches!(current_scope, EventScope::Root) {
                        if let Some(shared) = app.get_shared_script(&name).await {
                            if shared.visibility == "public" {
                                script_opt = Some(shared);
                                exec_scope = EventScope::Root;
                            }
                        }
                    }

                    if let Some(script) = script_opt {
                        if !script.active {
                            return Err(rquickjs::Error::Exception);
                        }

                        let mut call_payload = payload_json.clone();
                        if let Some(obj) = call_payload.as_object_mut() {
                            obj.insert("__caller_scope".to_string(), json!(current_scope));
                        }

                        let res = app
                            .execute_shared_script(script.code, call_payload, exec_scope)
                            .await
                            .map_err(|_| rquickjs::Error::Exception)?;

                        to_value(js_ctx, &res).map_err(|_| rquickjs::Error::Exception)
                    } else {
                        Err(rquickjs::Error::Exception)
                    }
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    run_obj
        .set("script", script_fn)
        .map_err(|e| e.to_string())?;
    globals.set("$run", run_obj).map_err(|e| e.to_string())?;
    Ok(())
}
