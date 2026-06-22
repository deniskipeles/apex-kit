use serde_json::json;

use super::super::{context::ACTIVE_CONTEXT, return_json_promise};
use boa_engine::{
    Context, JsArgs, JsString, NativeFunction, object::ObjectInitializer, property::Attribute,
};

// Shared HTTP Request Logic
fn execute_http_request(
    url: String,
    method: String,
    headers_val: Option<serde_json::Map<String, serde_json::Value>>,
    body_val: Option<serde_json::Value>,
    redirect_mode: Option<String>, // Argument
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

                let req_method =
                    reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);

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
                                json!(value.to_str().unwrap_or("")),
                            );
                        }

                        let body_text = res.text().await.unwrap_or_default();

                        // Return standardized response object
                        Ok(json!({
                            "ok": (200..300).contains(&status),
                            "status": status,
                            "statusText": status_text,
                            "url": final_url,
                            "headers": res_headers,
                            "body": body_text
                        }))
                    }
                    Err(e) => Err(format!("Network Error: {}", e)),
                }
            })
        } else {
            Err("Script Context Execution Error".into())
        }
    })
}

pub fn register_fetch(ctx: &mut Context) -> Result<(), String> {
    let fetch_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let url = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let opts = args
            .get_or_undefined(1)
            .to_json(ctx)
            .unwrap()
            .unwrap_or(json!({}));

        let method = opts
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_string();

        let headers = opts.get("headers").and_then(|h| h.as_object()).cloned();

        let body = opts.get("body").cloned();

        let redirect_mode = opts
            .get("redirect")
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
        Attribute::all(),
    )
    .map_err(|e| e.to_string())
}

pub fn register_http(ctx: &mut Context) -> Result<(), String> {
    // $http.get(url)
    let get = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let url = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        // Pass None for redirect (default follow)
        let result = execute_http_request(url, "GET".to_string(), None, None, None);

        // Map result: Extract "body" string or return error
        let mapped_result = result.map(|json_val| {
            // Legacy $http.get returns the raw body string
            json_val
                .get("body")
                .and_then(|b| b.as_str())
                .unwrap_or("")
                .to_string()
        });

        // Convert to JsString for Boa
        match mapped_result {
            Ok(s) => return_json_promise(ctx, Ok(serde_json::Value::String(s))),
            Err(e) => return_json_promise(ctx, Err(e)),
        }
    });

    // $http.post(url, body)
    let post = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let url = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let body_arg = args.get_or_undefined(1).to_json(ctx).unwrap();

        // CALL SHARED HELPER
        let result = execute_http_request(url, "POST".to_string(), None, body_arg, None);

        let mapped_result = result.map(|json_val| {
            json_val
                .get("body")
                .and_then(|b| b.as_str())
                .unwrap_or("")
                .to_string()
        });

        match mapped_result {
            Ok(s) => return_json_promise(ctx, Ok(serde_json::Value::String(s))),
            Err(e) => return_json_promise(ctx, Err(e)),
        }
    });

    let obj = ObjectInitializer::new(ctx)
        .function(get, JsString::from("get"), 1)
        .function(post, JsString::from("post"), 2)
        .build();

    ctx.register_global_property(JsString::from("$http"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
