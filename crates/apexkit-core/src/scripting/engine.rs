use crate::realtime::EventScope;
use regex::Regex;
use serde_json::{Value as JsonValue, json};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Function, Object, Promise};
use rquickjs_serde::{from_value, to_value};

use super::builtins::{
    register_ai, register_cache, register_cmd, register_console, register_db, register_env,
    register_fetch, register_file_tools, register_fs, register_http, register_mail,
    register_realtime, register_root, register_util, register_wasm, register_zip,
};
use super::context::ScriptContext;

static EXECUTION_SEMAPHORE: OnceLock<Semaphore> = OnceLock::new();
fn get_execution_semaphore() -> &'static Semaphore {
    EXECUTION_SEMAPHORE.get_or_init(|| Semaphore::new(50)) // Caps concurrent scripts at 50
}

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
            this._body_b64 = init._body_b64 || null;

            this.json = async () => {
                if (typeof this.body === 'object' && this.body !== null) return this.body;
                return JSON.parse(this.body);
            };

            this.text = async () => {
                if (typeof this.body === 'string') return this.body;
                return JSON.stringify(this.body);
            };

            this.arrayBuffer = async () => {
                if (this._body_b64 && globalThis.$util && globalThis.$util.base64DecodeBuffer) {
                    try {
                        return globalThis.$util.base64DecodeBuffer(this._body_b64);
                    } catch (e) {
                        // Safe fallback for edge-case buffer parsing
                        let bin = globalThis.$util.base64Decode(this._body_b64);
                        let buf = new Uint8Array(bin.length);
                        for (let i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
                        return buf.buffer;
                    }
                }
                
                // If body is already a TypedArray or ArrayBuffer, extract it safely
                if (this.body instanceof ArrayBuffer) return this.body;
                if (ArrayBuffer.isView(this.body)) {
                    return this.body.buffer.slice(this.body.byteOffset, this.body.byteOffset + this.body.byteLength);
                }

                let txt = await this.text();
                let encoder = new TextEncoder();
                return encoder.encode(txt).buffer;
            };
        }
    }

    class URL {
        constructor(urlStr, baseStr) {
            let full = String(urlStr || "");
            if (baseStr && !full.includes('://')) {
                let baseClean = String(baseStr).replace(/\/$/, '');
                full = baseClean + '/' + full.replace(/^\//, '');
            }
            
            this.href = full;
            
            let match = full.match(/^(https?:)\/\/([^\/?#]+)([^?#]*)(?:\?([^#]*))?(?:#(.*))?$/i);
            if (match) {
                this.protocol = match[1].toLowerCase();
                this.host = match[2];
                let hostParts = this.host.split(':');
                this.hostname = hostParts[0];
                this.port = hostParts[1] || "";
                this.origin = this.protocol + "//" + this.host;
                this.pathname = match[3] || "/";
                this.search = match[4] ? "?" + match[4] : "";
                this.hash = match[5] ? '#' + match[5] : "";
            } else {
                this.protocol = "http:";
                this.host = "localhost";
                this.hostname = "localhost";
                this.port = "";
                this.origin = "http://localhost";
                let parts = full.split('#');
                this.hash = parts[1] ? '#' + parts[1] : "";
                let pathAndSearch = parts[0].split('?');
                this.pathname = pathAndSearch[0] || "/";
                this.search = pathAndSearch[1] ? "?" + pathAndSearch[1] : "";
            }

            const params = new Map();
            let rawQuery = this.search.startsWith('?') ? this.search.slice(1) : this.search;
            if (rawQuery) {
                rawQuery.split('&').forEach(pair => {
                    if (!pair) return;
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

        toString() {
            return this.origin + this.pathname + this.search + this.hash;
        }
    }

    class Request {
        constructor(input, init = {}) {
            if (typeof input === 'object' && input !== null && input.url) {
                this.url = String(input.url);
                this.method = String(init.method || input.method || "GET").toUpperCase();
                this.bodyData = init.body !== undefined ? init.body : (input.bodyData !== undefined ? input.bodyData : null);
                this.headers = new Headers(init.headers || input.headers || {});
            } else {
                this.url = String(input || "");
                this.method = String(init.method || "GET").toUpperCase();
                this.bodyData = init.body !== undefined ? init.body : null;
                this.headers = new Headers(init.headers || {});
            }
            this.args = this.bodyData || {};

            try {
                let u = new URL(this.url);
                let pathRegex = /^(\/(?:tenant|sandbox)\/[^\/]+)?(?:\/api\/v1)?\/(?:run|webhook)\/[^\/]+/;
                let cleanPath = u.pathname.replace(pathRegex, "") || "/";
                this.url = u.origin + cleanPath + u.search + u.hash;
            } catch (e) {}

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
                } catch (e) {}
            }
        }

        async json() {
            if (this.bodyData === null || this.bodyData === undefined) return {};
            if (typeof this.bodyData === 'string') {
                try { return JSON.parse(this.bodyData); } catch (e) { return {}; }
            }
            return this.bodyData;
        }

        async text() {
            if (this.bodyData === null || this.bodyData === undefined) return "";
            if (typeof this.bodyData === 'string') return this.bodyData;
            return JSON.stringify(this.bodyData);
        }

        async arrayBuffer() {
            const txt = await this.text();
            const encoder = new TextEncoder();
            return encoder.encode(txt).buffer;
        }

        clone() {
            return new Request(this.url, {
                method: this.method,
                headers: this.headers,
                body: this.bodyData
            });
        }
    }

    class WebAssemblyMemory {
        constructor(buffer) {
            this.buffer = buffer;
        }
    }

    class WebAssemblyInstance {
        constructor(exports) {
            this.exports = exports;
        }
    }

    class WebAssemblyModule {
        constructor(b64) {
            this.b64 = b64;
        }
    }

    globalThis.WebAssembly = {
        Memory: WebAssemblyMemory,
        Instance: WebAssemblyInstance,
        Module: WebAssemblyModule,

        _toBase64: async function(bufferSource) {
            if (typeof bufferSource === 'string') return bufferSource;
            if (bufferSource instanceof WebAssemblyModule) return bufferSource.b64;

            let ab;
            if (bufferSource instanceof ArrayBuffer) {
                ab = bufferSource;
            } else if (ArrayBuffer.isView(bufferSource)) {
                ab = bufferSource.buffer.slice(bufferSource.byteOffset, bufferSource.byteOffset + bufferSource.byteLength);
            } else if (bufferSource && typeof bufferSource.arrayBuffer === 'function') {
                ab = await bufferSource.arrayBuffer();
            } else {
                return String(bufferSource);
            }

            if (globalThis.$util && globalThis.$util.base64EncodeBuffer) {
                return globalThis.$util.base64EncodeBuffer(ab);
            }

            const uint8 = new Uint8Array(ab);
            let binary = '';
            const chunkSize = 8192;
            for (let i = 0; i < uint8.length; i += chunkSize) {
                binary += String.fromCharCode.apply(null, uint8.subarray(i, i + chunkSize));
            }
            return btoa(binary);
        },

        async instantiate(bufferSource, importObject = {}) {
            let b64 = await this._toBase64(bufferSource);
            const res = await $wasm.__instantiate(b64, importObject);
            
            const module = new WebAssemblyModule(b64);
            const instance = new WebAssemblyInstance(res.exports);
            
            if (bufferSource instanceof WebAssemblyModule) {
                return instance;
            }
            return { module, instance };
        },

        async instantiateStreaming(source, importObject = {}) {
            const response = await source;
            const ab = await response.arrayBuffer();
            return await this.instantiate(ab, importObject);
        },

        async compile(bufferSource) {
            let b64 = await this._toBase64(bufferSource);
            return new WebAssemblyModule(b64);
        }
    };

    class ApexKit {
        constructor(contextId = null) { 
            this.contextId = contextId;
            this.dbRef = this.contextId ? globalThis.$root?.db : globalThis.$db;
            
            if (this.contextId && !this.dbRef) {
                throw new Error("Access Denied: Root scope required for context switching.");
            }
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
    
    globalThis.URL = URL;
    globalThis.ApexKit = ApexKit;
    globalThis.$apex = new ApexKit();
    globalThis.Headers = Headers;
    globalThis.Request = Request;
    globalThis.Response = Response;

    globalThis.fetch = async function(url, options = {}) {
        let urlString = typeof url === 'object' && url !== null ? (url.href || url.url || String(url)) : String(url);

        if (!urlString.startsWith("http://") && !urlString.startsWith("https://")) {
            let baseUrl = "http://localhost:5000";
            if (globalThis.$env && globalThis.$env.APP_URL) {
                baseUrl = globalThis.$env.APP_URL;
            }
            urlString = baseUrl.replace(/\/$/, "") + "/" + urlString.replace(/^\//, "");
        }

        let headersObj = {};
        if (options.headers) {
            if (options.headers instanceof Headers) {
                options.headers.forEach((v, k) => headersObj[k] = v);
            } else {
                headersObj = options.headers;
            }
        }

        const nativeRes = await $__native_fetch(urlString, {
            method: options.method || 'GET',
            headers: headersObj,
            body: options.body
        });

        let res = new Response(nativeRes.body, {
            status: nativeRes.status,
            statusText: nativeRes.statusText,
            headers: nativeRes.headers,
            url: nativeRes.url
        });
        res._body_b64 = nativeRes.body_b64;
        return res;
    };
"#;

#[derive(Clone)]
pub struct ScriptEngine {
    pub vfs: crate::scripting::module_loader::VfsState,
    pub db: Option<Arc<dyn crate::Db>>,
}

unsafe impl Send for ScriptEngine {}
unsafe impl Sync for ScriptEngine {}

impl ScriptEngine {
    pub async fn new() -> Self {
        Self::with_vfs(crate::scripting::module_loader::VfsState::default(), None).await
    }

    pub async fn with_vfs(
        vfs: crate::scripting::module_loader::VfsState,
        db: Option<Arc<dyn crate::Db>>,
    ) -> Self {
        Self { vfs, db }
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
        let _permit = get_execution_semaphore()
            .acquire()
            .await
            .map_err(|_| "Server is too busy. Please try again later.".to_string())?;

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
            .unwrap_or(30000);

        let runtime = AsyncRuntime::new().map_err(|e| e.to_string())?;
        let vfs = self.vfs.clone();
        let db = self.db.clone();

        let start_time = std::time::Instant::now();
        let max_duration = std::time::Duration::from_millis(total_cpu_budget_ms);
        let tokio_handle = tokio::runtime::Handle::current();

        let task = SendWrapper(async move {
            runtime.set_max_stack_size(0).await;

            if let Some(db_arc) = db {
                runtime
                    .set_loader(
                        super::module_loader::ApexModuleResolver,
                        super::module_loader::ApexModuleLoader {
                            vfs,
                            db: db_arc,
                            tokio_handle,
                        },
                    )
                    .await;
            } else {
                struct DummyLoader;
                impl rquickjs::loader::Loader for DummyLoader {
                    fn load<'js>(
                        &mut self,
                        _ctx: &rquickjs::Ctx<'js>,
                        _name: &str,
                        _attrs: Option<rquickjs::loader::ImportAttributes<'js>>,
                    ) -> rquickjs::Result<rquickjs::Module<'js, rquickjs::module::Declared>>
                    {
                        Err(rquickjs::Error::Unknown)
                    }
                }
                runtime
                    .set_loader(super::module_loader::ApexModuleResolver, DummyLoader)
                    .await;
            }

            runtime
                .set_interrupt_handler(Some(Box::new(move || start_time.elapsed() > max_duration)))
                .await;

            let ctx = AsyncContext::full(&runtime)
                .await
                .map_err(|e| e.to_string())?;

            #[allow(deprecated)]
            let res = rquickjs::async_with!(ctx => |js_ctx| {
                setup_quickjs(&js_ctx, context.clone())?;

                let module_name = format!("exec_{}", uuid::Uuid::new_v4());
                let _ = rquickjs::Module::evaluate(js_ctx.clone(), module_name, processed_code.as_bytes())
                    .catch(&js_ctx)
                    .map_err(|e| format!("Script Module Error: {}", e))?;

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
                    .map_err(|_| "No 'export default' found in script".to_string())?;

                let promise: Promise = handler
                    .call((request_obj,))
                    .catch(&js_ctx)
                    .map_err(|e| e.to_string())?;

                let result_val: rquickjs::Value = promise
                    .into_future::<rquickjs::Value>()
                    .await
                    .catch(&js_ctx)
                    .map_err(|e| e.to_string())?;

                let res_val = if let Some(obj) = result_val.as_object() {
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

                        json!({
                            "__is_apex_response": true,
                            "status": status_code,
                            "body": body_json
                        })
                    } else {
                        from_value(result_val).unwrap_or(JsonValue::Null)
                    }
                } else {
                    from_value(result_val).unwrap_or(JsonValue::Null)
                };

                js_ctx.run_gc();
                Ok::<JsonValue, String>(res_val)
            }).await;

            runtime.idle().await;
            res
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
        let _permit = get_execution_semaphore()
            .acquire()
            .await
            .map_err(|_| "Server is too busy. Please try again later.".to_string())?;

        let re_config = Regex::new(r"export\s+const\s+graphql\s*=\s*(\{[\s\S]*?\})(?:;|\n|$)")
            .map_err(|e| e.to_string())?;
        let code_cleaned = re_config.replace_all(code, "");
        let processed_code =
            code_cleaned.replacen("export default", "globalThis.__mainHandler =", 1);

        let timeout_secs = std::env::var("SCRIPT_EXECUTION_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);

        let runtime = AsyncRuntime::new().map_err(|e| e.to_string())?;
        let vfs = self.vfs.clone();
        let db = self.db.clone();
        let tokio_handle = tokio::runtime::Handle::current();

        let task = SendWrapper(async move {
            runtime.set_max_stack_size(0).await;

            if let Some(db_arc) = db {
                runtime
                    .set_loader(
                        super::module_loader::ApexModuleResolver,
                        super::module_loader::ApexModuleLoader {
                            vfs,
                            db: db_arc,
                            tokio_handle,
                        },
                    )
                    .await;
            }

            let ctx = AsyncContext::full(&runtime)
                .await
                .map_err(|e| e.to_string())?;

            #[allow(deprecated)]
            let res = rquickjs::async_with!(ctx => |js_ctx| {
                setup_quickjs(&js_ctx, context.clone())?;

                let module_name = format!("exec_{}", uuid::Uuid::new_v4());
                let _ = rquickjs::Module::evaluate(js_ctx.clone(), module_name, processed_code.as_bytes())
                    .catch(&js_ctx)
                    .map_err(|e| format!("Script Module Error: {}", e))?;

                let js_event = to_value(js_ctx.clone(), &event_data).map_err(|e| e.to_string())?;
                let globals = js_ctx.globals();

                let res_val = if let Ok(handler) = globals.get::<_, Function>("__mainHandler") {
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
                        None
                    } else if let Some(b) = result_val.as_bool() {
                        if !b {
                            return Err("Hook blocked operation".to_string());
                        }
                        None
                    } else {
                        let json_val: JsonValue = from_value(result_val).map_err(|e| e.to_string())?;
                        Some(json_val)
                    }
                } else {
                    None
                };

                js_ctx.run_gc();
                Ok::<Option<JsonValue>, String>(res_val)
            }).await;

            runtime.idle().await;
            res
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
    register_wasm(ctx, app_ctx.clone())?;

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
