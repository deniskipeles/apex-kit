use base64::{Engine as _, engine::general_purpose::STANDARD};
use rquickjs::function::Async;
use rquickjs::{Ctx, Function, Object, Value};
use rquickjs_serde::{from_value, to_value};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use super::super::context::ScriptContext;

async fn execute_http_request(
    url: String,
    method: String,
    headers_val: Option<HashMap<String, String>>,
    body_val: Option<serde_json::Value>,
    redirect_mode: Option<String>,
) -> Result<serde_json::Value, String> {
    let policy = match redirect_mode.as_deref() {
        Some("manual") => reqwest::redirect::Policy::none(),
        Some("error") => {
            reqwest::redirect::Policy::custom(|attempt| attempt.error("Redirects not allowed"))
        }
        _ => reqwest::redirect::Policy::default(),
    };

    let client = reqwest::Client::builder()
        .redirect(policy)
        .build()
        .map_err(|e| format!("Client Build Error: {}", e))?;

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

    match req_builder.send().await {
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
                "body_b64": body_b64
            }))
        }
        Err(e) => Err(format!("Network Error: {}", e)),
    }
}

pub fn register_fetch<'js>(ctx: &Ctx<'js>, _app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();

    let fetch_ctx = ctx.clone();
    let fetch_fn = Function::new(
        ctx.clone(),
        Async(move |url: String, opts_val: Option<Value<'js>>| {
            let thread_ctx = fetch_ctx.clone();
            async move {
                let mut method = "GET".to_string();
                let mut headers = None;
                let mut body = None;
                let mut redirect = None;

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
                    }
                }

                let res = execute_http_request(url, method, headers, body, redirect)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                // Map JSON response to QuickJS value
                to_value(thread_ctx.clone(), &res).map_err(|_| rquickjs::Error::Exception)
            }
        }),
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
        Async(move |url: String| async move {
            let res = execute_http_request(url, "GET".to_string(), None, None, None)
                .await
                .map_err(|_| rquickjs::Error::Exception)?;

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
        Async(move |url: String, body_val: Value<'js>| async move {
            let body_json: serde_json::Value = from_value(body_val).unwrap_or(json!({}));
            let res = execute_http_request(url, "POST".to_string(), None, Some(body_json), None)
                .await
                .map_err(|_| rquickjs::Error::Exception)?;

            let body = res
                .get("body")
                .and_then(|b| b.as_str())
                .unwrap_or("")
                .to_string();

            Ok::<String, rquickjs::Error>(body)
        }),
    )
    .map_err(|e| e.to_string())?;

    http_obj.set("get", get_fn).map_err(|e| e.to_string())?;
    http_obj.set("post", post_fn).map_err(|e| e.to_string())?;

    globals.set("$http", http_obj).map_err(|e| e.to_string())?;
    Ok(())
}
