use super::super::{context::ACTIVE_CONTEXT, return_json_promise};
use boa_engine::{
    Context, JsArgs, JsString, JsValue, NativeFunction, object::ObjectInitializer,
    property::Attribute,
};

// Function to register $cache
pub fn register_cache(ctx: &mut Context) -> Result<(), String> {
    // 1. GET
    let get = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let key = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async { app.cache_get(&key).await })
            } else {
                None
            }
        });

        match res {
            Some(val) => Ok(JsValue::from(JsString::from(val))),
            None => Ok(JsValue::null()),
        }
    });

    // 2. SET
    let set = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let key = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        // Accept string or JSON object (stringify it)
        let val = args.get_or_undefined(1);
        let val_str = if val.is_object() {
            serde_json::to_string(&val.to_json(ctx).unwrap()).unwrap_or_default()
        } else {
            val.to_string(ctx)?.to_std_string_escaped()
        };

        let ttl = args
            .get(2)
            .and_then(|v| v.to_number(ctx).ok())
            .map(|n| n as u64);

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
        let key = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    app.cache_del(&key).await;
                })
            }
        });
        Ok(JsValue::undefined())
    });

    // 4. INCREMENT (For Quotas)
    let incr = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let key = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let delta = args
            .get(1)
            .and_then(|v| v.to_number(ctx).ok())
            .unwrap_or(1.0) as i64;

        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async { app.cache_incr(&key, delta).await })
            } else {
                0
            }
        });
        Ok(JsValue::from(res))
    });

    // 5. LIST KEYS [NEW]
    let list_keys = NativeFunction::from_copy_closure(move |_, _, ctx| {
        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async { app.cache_list_keys().await })
            } else {
                vec![]
            }
        });

        let json_arr = serde_json::to_value(res).unwrap_or(serde_json::Value::Array(vec![]));
        return_json_promise(ctx, Ok(json_arr))
    });

    let obj = ObjectInitializer::new(ctx)
        .function(get, JsString::from("get"), 1)
        .function(set, JsString::from("set"), 3)
        .function(del, JsString::from("delete"), 1)
        .function(incr, JsString::from("incr"), 2)
        .function(list_keys, JsString::from("listKeys"), 0) // Register new function
        .build();

    ctx.register_global_property(JsString::from("$cache"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
