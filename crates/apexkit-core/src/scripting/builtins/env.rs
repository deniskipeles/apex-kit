use super::super::{context::ACTIVE_CONTEXT, return_json_promise};
use boa_engine::{
    Context, JsArgs, JsString, JsValue, NativeFunction, object::ObjectInitializer,
    property::Attribute,
};

pub fn register_env(ctx: &mut Context) -> Result<(), String> {
    let get = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let key = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    if let Ok(Some(val)) = app.get_db().get_config(&key).await {
                        if let Ok(enc) = serde_json::from_value::<
                            crate::security::vault::EncryptedValue,
                        >(val.clone())
                        {
                            return app.get_vault().decrypt(&enc).map_err(|e| e.to_string());
                        }
                        return Ok(val.as_str().unwrap_or("").to_string());
                    }
                    Ok("".to_string())
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result.map(serde_json::Value::String))
    });

    let app_url = ACTIVE_CONTEXT
        .with(|c| c.borrow().as_ref().and_then(|t| t.2.clone()))
        .unwrap_or_default();

    // [NEW] Resolve the SMTP block policy dynamically from the database
    // for the active Tenant/Sandbox/Root scope.
    let smtp_blocked = ACTIVE_CONTEXT.with(|c| {
        if let Some((app, handle, _, _, _)) = &*c.borrow() {
            handle.block_on(async {
                if let Ok(db) = super::db::resolve_db(None, app.clone()).await
                    && let Ok(Some(smtp_val)) = db.get_config("smtp").await
                {
                    return smtp_val
                        .get("block_smtp")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                }
                false
            })
        } else {
            false
        }
    });

    let obj = ObjectInitializer::new(ctx)
        .function(get, JsString::from("get"), 1)
        .property(
            JsString::from("APP_URL"),
            JsString::from(app_url),
            Attribute::all(),
        )
        // [NEW] Expose SMTP_BLOCKED property globally
        .property(
            JsString::from("SMTP_BLOCKED"),
            JsValue::from(smtp_blocked),
            Attribute::all(),
        )
        .build();
    ctx.register_global_property(JsString::from("$env"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
