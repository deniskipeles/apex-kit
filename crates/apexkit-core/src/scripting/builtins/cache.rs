use super::super::context::ScriptContext;
use rquickjs::function::Async;
use rquickjs::{Ctx, Function, Object, Value};
use rquickjs_serde::from_value;
use std::sync::Arc;

pub fn register_cache<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let cache_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // 1. $cache.get(key) -> Promise<string | null>
    let app_get = app_ctx.clone();
    let get_fn = Function::new(
        ctx.clone(),
        Async(move |key: String| {
            let app = app_get.clone();
            async move {
                let val = app.cache_get(&key).await;
                Ok::<Option<String>, rquickjs::Error>(val)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // 2. $cache.set(key, val, ttl?) -> Promise<void>
    let app_set = app_ctx.clone();
    let set_fn = Function::new(
        ctx.clone(),
        Async(move |key: String, val: Value<'js>, ttl: Option<u64>| {
            let app = app_set.clone();
            async move {
                let val_str = if val.is_object() || val.is_array() {
                    if let Ok(json_val) = from_value::<serde_json::Value>(val) {
                        serde_json::to_string(&json_val).unwrap_or_default()
                    } else {
                        "".to_string()
                    }
                } else if let Some(s) = val.as_string() {
                    s.to_string().unwrap_or_default()
                } else if let Some(b) = val.as_bool() {
                    b.to_string()
                } else if let Some(n) = val.as_int().map(|i| i as f64).or_else(|| val.as_float()) {
                    n.to_string()
                } else {
                    "".to_string()
                };

                app.cache_set(&key, &val_str, ttl).await;
                Ok::<(), rquickjs::Error>(())
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // 3. $cache.delete(key) -> Promise<void>
    let app_del = app_ctx.clone();
    let del_fn = Function::new(
        ctx.clone(),
        Async(move |key: String| {
            let app = app_del.clone();
            async move {
                app.cache_del(&key).await;
                Ok::<(), rquickjs::Error>(())
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // 4. $cache.incr(key, delta?) -> Promise<i64>
    let app_incr = app_ctx.clone();
    let incr_fn = Function::new(
        ctx.clone(),
        Async(move |key: String, delta: Option<i64>| {
            let app = app_incr.clone();
            async move {
                let res = app.cache_incr(&key, delta.unwrap_or(1)).await;
                Ok::<i64, rquickjs::Error>(res)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // 5. $cache.listKeys() -> Promise<Vec<String>>
    let app_list = app_ctx.clone();
    let list_keys_fn = Function::new(
        ctx.clone(),
        Async(move || {
            let app = app_list.clone();
            async move {
                let keys = app.cache_list_keys().await;
                Ok::<Vec<String>, rquickjs::Error>(keys)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    cache_obj.set("get", get_fn).map_err(|e| e.to_string())?;
    cache_obj.set("set", set_fn).map_err(|e| e.to_string())?;
    cache_obj
        .set("delete", del_fn.clone())
        .map_err(|e| e.to_string())?;
    cache_obj.set("del", del_fn).map_err(|e| e.to_string())?; // Alias
    cache_obj.set("incr", incr_fn).map_err(|e| e.to_string())?;
    cache_obj
        .set("listKeys", list_keys_fn)
        .map_err(|e| e.to_string())?;

    globals
        .set("$cache", cache_obj)
        .map_err(|e| e.to_string())?;
    Ok(())
}
