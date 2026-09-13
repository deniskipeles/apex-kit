use base64::{Engine as _, engine::general_purpose::STANDARD};
use rquickjs::function::{Async, Opt};
use rquickjs::{Ctx, Exception, Function, Object, Value};
use rquickjs_serde::{from_value, to_value};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use super::super::context::ScriptContext;

// [CRITICAL PERFORMANCE FIX]: Use a globally pooled HTTP client.
// Creating a new reqwest::Client per script execution exhausts OS sockets and causes TCP port exhaustion/timeouts.
static SHARED_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn get_shared_client() -> reqwest::Client {
    SHARED_HTTP_CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .pool_idle_timeout(std::time::Duration::from_secs(90))
                .pool_max_idle_per_host(50)
                .build()
                .unwrap()
        })
        .clone()
}

async fn execute_http_request(
    url: String,
    method: String,
    headers_val: Option<HashMap<String, String>>,
    body_val: Option<serde_json::Value>,
    redirect_mode: Option<String>,
    timeout_ms: Option<u64>,
    task_id: Option<u64>,
) -> Result<serde_json::Value, String> {
    // Fallback to custom client only if special configurations are requested
    let client = if redirect_mode.is_some() || timeout_ms.is_some() {
        let policy = match redirect_mode.as_deref() {
            Some("manual") => reqwest::redirect::Policy::none(),
            Some("error") => {
                reqwest::redirect::Policy::custom(|attempt| attempt.error("Redirects not allowed"))
            }
            _ => reqwest::redirect::Policy::default(),
        };

        let mut client_builder = reqwest::Client::builder().redirect(policy);

        if let Some(ms) = timeout_ms {
            client_builder = client_builder.timeout(std::time::Duration::from_millis(ms));
        }

        client_builder
            .build()
            .map_err(|e| format!("Client Build Error: {}", e))?
    } else {
        get_shared_client()
    };

    let req_method = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);

    let mut req_builder = client.request(req_method, &url);

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

pub fn register_fetch<'js>(ctx: &Ctx<'js>, _app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();

    let fetch_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, url: String, Opt(opts_val): Opt<Value<'js>>| async move {
                let task_id: Option<u64> = js_ctx.globals().get("__TASK_ID").ok();
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
                    url, method, headers, body, redirect, timeout_ms, task_id,
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
                    let js_err =
                        Exception::from_message(js_ctx.clone(), "Failed to serialize fetch result")
                            .unwrap();
                    js_ctx.throw(js_err.into())
                })
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    globals
        .set("$__native_fetch", fetch_fn)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn register_http<'js>(ctx: &Ctx<'js>, _app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let http_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let get_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, url: String| async move {
            let task_id: Option<u64> = js_ctx.globals().get("__TASK_ID").ok();
            let res =
                match execute_http_request(url, "GET".to_string(), None, None, None, None, task_id)
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
        }),
    )
    .map_err(|e| e.to_string())?;

    let post_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, url: String, body_val: Value<'js>| async move {
                let task_id: Option<u64> = js_ctx.globals().get("__TASK_ID").ok();
                let body_json: serde_json::Value = from_value(body_val).unwrap_or(json!({}));
                let res = match execute_http_request(
                    url,
                    "POST".to_string(),
                    None,
                    Some(body_json),
                    None,
                    None,
                    task_id,
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
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    http_obj.set("get", get_fn).map_err(|e| e.to_string())?;
    http_obj.set("post", post_fn).map_err(|e| e.to_string())?;

    globals.set("$http", http_obj).map_err(|e| e.to_string())?;
    Ok(())
}
