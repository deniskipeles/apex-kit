use base64::{Engine as _, engine::general_purpose::STANDARD};
use rquickjs::function::{Async, Opt};
use rquickjs::{Ctx, Exception, Function, Object, Value};
use rquickjs_serde::{from_value, to_value};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use super::super::context::ScriptContext;
use crate::realtime::EventScope;

// [CRITICAL PERFORMANCE FIX]: Use globally pooled HTTP clients to prevent File Descriptor & Socket exhaustion.
// Creating a new reqwest::Client per script execution leaks connection pools and quickly consumes all available TCP sockets.
static HTTP_CLIENT_DEFAULT: OnceLock<reqwest::Client> = OnceLock::new();
static HTTP_CLIENT_NO_REDIRECT: OnceLock<reqwest::Client> = OnceLock::new();
static HTTP_CLIENT_ERR_REDIRECT: OnceLock<reqwest::Client> = OnceLock::new();

static SCOPED_HTTP_SEMAPHORES: OnceLock<RwLock<HashMap<String, Arc<tokio::sync::Semaphore>>>> =
    OnceLock::new();

fn get_http_semaphore(scope_str: &str) -> Arc<tokio::sync::Semaphore> {
    let limit = std::env::var("SCOPE_HTTP_LIMIT")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(5); // Default to 5 concurrent fetch per scope

    let map = SCOPED_HTTP_SEMAPHORES.get_or_init(|| RwLock::new(HashMap::new()));

    if let Some(sem) = map.read().unwrap().get(scope_str) {
        return sem.clone();
    }

    let mut write_map = map.write().unwrap();
    if let Some(sem) = write_map.get(scope_str) {
        return sem.clone();
    }

    let new_sem = Arc::new(tokio::sync::Semaphore::new(limit));
    write_map.insert(scope_str.to_string(), new_sem.clone());
    new_sem
}

fn get_client(redirect_mode: Option<&str>) -> reqwest::Client {
    let build_client = |policy: reqwest::redirect::Policy| {
        reqwest::Client::builder()
            .redirect(policy)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(50)
            .connect_timeout(std::time::Duration::from_secs(5)) // Fast fail bad hostnames/IPs
            .build()
            .unwrap()
    };

    match redirect_mode {
        Some("manual") => HTTP_CLIENT_NO_REDIRECT
            .get_or_init(|| build_client(reqwest::redirect::Policy::none()))
            .clone(),
        Some("error") => HTTP_CLIENT_ERR_REDIRECT
            .get_or_init(|| {
                build_client(reqwest::redirect::Policy::custom(|attempt| {
                    attempt.error("Redirects not allowed")
                }))
            })
            .clone(),
        _ => HTTP_CLIENT_DEFAULT
            .get_or_init(|| build_client(reqwest::redirect::Policy::default()))
            .clone(),
    }
}

async fn execute_http_request(
    url: String,
    method: String,
    headers_val: Option<HashMap<String, String>>,
    body_val: Option<serde_json::Value>,
    redirect_mode: Option<String>,
    timeout_ms: Option<u64>,
    task_id: Option<u64>,
    scope_str: String,
) -> Result<serde_json::Value, String> {
    let sem = get_http_semaphore(&scope_str);

    // Acquire the permit. If the scope queues up too many requests, they wait here.
    // We add a 30s timeout to the queue itself to prevent script deadlocks.
    let _permit =
        match tokio::time::timeout(std::time::Duration::from_secs(30), sem.acquire()).await {
            Ok(Ok(p)) => p,
            _ => {
                return Err(format!(
                    "HTTP request rejected: Scope '{}' exceeded concurrent fetch limits.",
                    scope_str
                ));
            }
        };

    let client = get_client(redirect_mode.as_deref());

    let req_method = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);

    let mut req_builder = client.request(req_method, &url);

    // Apply timeout strictly to the Request object.
    // Allow scopes to control their timeout, capped to 5 minutes (300,000 ms) instead of 60 seconds.
    let final_timeout = timeout_ms.unwrap_or(15_000).clamp(1, 300_000);
    req_builder = req_builder.timeout(std::time::Duration::from_millis(final_timeout));

    if let Some(h_map) = headers_val {
        for (k, v) in h_map {
            req_builder = req_builder.header(&k, v);
        }
    }

    if let Some(b) = body_val {
        if let Some(s) = b.as_str() {
            req_builder = req_builder.body(s.to_string());
        } else {
            req_builder = req_builder.json(&b);
        }
    }

    if let Some(id) = task_id {
        crate::scripting::scheduler::get_quantum_scheduler().pause_for_io(id);
    }

    let start_time = std::time::Instant::now();
    let res_result = req_builder.send().await;
    let wall_time = start_time.elapsed().as_millis() as u64;

    if let Some(id) = task_id {
        crate::scripting::scheduler::get_quantum_scheduler().resume_from_io(id);
    }

    match res_result {
        Ok(res) => {
            let status = res.status().as_u16();
            let status_text = res.status().canonical_reason().unwrap_or("").to_string();
            let final_url = res.url().to_string();

            let mut res_headers = serde_json::Map::new();
            for (name, value) in res.headers() {
                res_headers.insert(
                    name.as_str().to_string(),
                    json!(value.to_str().unwrap_or("")),
                );
            }

            let bytes = res.bytes().await.unwrap_or_default();
            let body_b64 = STANDARD.encode(&bytes);
            let body_text = String::from_utf8_lossy(&bytes).to_string();

            Ok(json!({
                "ok": (200..300).contains(&status),
                "status": status,
                "statusText": status_text,
                "url": final_url,
                "headers": res_headers,
                "body": body_text,
                "body_b64": body_b64,
                "wallTime": wall_time
            }))
        }
        Err(e) => Err(format!("Network Error: {}", e)),
    }
}

pub fn register_fetch<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let app_fetch = app_ctx.clone();

    let fetch_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, url: String, Opt(opts_val): Opt<Value<'js>>| {
                let app = app_fetch.clone();
                async move {
                    let task_id: Option<u64> = js_ctx.globals().get("__TASK_ID").ok();
                    let scope = app.get_scope();
                    let scope_str = match &scope {
                        EventScope::Root => "root".to_string(),
                        EventScope::Tenant(id) => format!("tenant:{}", id),
                        EventScope::Sandbox(id) => format!("sandbox:{}", id),
                        _ => "root".to_string(),
                    };

                    let mut method = "GET".to_string();
                    let mut headers = None;
                    let mut body = None;
                    let mut redirect = None;
                    let mut timeout_ms = None;

                    if let Some(v) = opts_val {
                        if let Ok(opts) = from_value::<serde_json::Value>(v) {
                            if let Some(m) = opts.get("method").and_then(|v| v.as_str()) {
                                method = m.to_string();
                            }
                            if let Some(h) = opts.get("headers") {
                                if let Ok(h_map) =
                                    serde_json::from_value::<HashMap<String, String>>(h.clone())
                                {
                                    headers = Some(h_map);
                                }
                            }
                            body = opts.get("body").cloned();
                            if let Some(r) = opts.get("redirect").and_then(|v| v.as_str()) {
                                redirect = Some(r.to_string());
                            }
                            if let Some(t) = opts.get("timeout").and_then(|v| v.as_u64()) {
                                timeout_ms = Some(t);
                            }
                        }
                    }

                    let res = match execute_http_request(
                        url, method, headers, body, redirect, timeout_ms, task_id, scope_str,
                    )
                    .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                    };

                    to_value(js_ctx.clone(), &res).map_err(|_| {
                        let js_err = Exception::from_message(
                            js_ctx.clone(),
                            "Failed to serialize fetch result",
                        )
                        .unwrap();
                        js_ctx.throw(js_err.into())
                    })
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    globals
        .set("$__native_fetch", fetch_fn)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn register_http<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let http_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let app_get = app_ctx.clone();
    let get_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, url: String| {
            let app = app_get.clone();
            async move {
                let task_id: Option<u64> = js_ctx.globals().get("__TASK_ID").ok();
                let scope = app.get_scope();
                let scope_str = match &scope {
                    EventScope::Root => "root".to_string(),
                    EventScope::Tenant(id) => format!("tenant:{}", id),
                    EventScope::Sandbox(id) => format!("sandbox:{}", id),
                    _ => "root".to_string(),
                };
                let res = match execute_http_request(
                    url,
                    "GET".to_string(),
                    None,
                    None,
                    None,
                    None,
                    task_id,
                    scope_str,
                )
                .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                        return Err(js_ctx.throw(js_err.into()));
                    }
                };

                let body = res
                    .get("body")
                    .and_then(|b| b.as_str())
                    .unwrap_or("")
                    .to_string();

                Ok::<String, rquickjs::Error>(body)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    let app_post = app_ctx.clone();
    let post_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, url: String, body_val: Value<'js>| {
            let app = app_post.clone();
            async move {
                let task_id: Option<u64> = js_ctx.globals().get("__TASK_ID").ok();
                let scope = app.get_scope();
                let scope_str = match &scope {
                    EventScope::Root => "root".to_string(),
                    EventScope::Tenant(id) => format!("tenant:{}", id),
                    EventScope::Sandbox(id) => format!("sandbox:{}", id),
                    _ => "root".to_string(),
                };
                let body_json: serde_json::Value = from_value(body_val).unwrap_or(json!({}));
                let res = match execute_http_request(
                    url,
                    "POST".to_string(),
                    None,
                    Some(body_json),
                    None,
                    None,
                    task_id,
                    scope_str,
                )
                .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                        return Err(js_ctx.throw(js_err.into()));
                    }
                };

                let body = res
                    .get("body")
                    .and_then(|b| b.as_str())
                    .unwrap_or("")
                    .to_string();

                Ok::<String, rquickjs::Error>(body)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    http_obj.set("get", get_fn).map_err(|e| e.to_string())?;
    http_obj.set("post", post_fn).map_err(|e| e.to_string())?;

    globals.set("$http", http_obj).map_err(|e| e.to_string())?;
    Ok(())
}
