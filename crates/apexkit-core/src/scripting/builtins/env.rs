use super::super::{context::ACTIVE_CONTEXT, return_json_promise};
use crate::realtime::EventScope;
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
            if let Some((app, handle, base_url_opt, _, scope)) = &*c.borrow() {
                handle.block_on(async {
                    // 1. Intercept APP_URL and LOCAL_APP_URL to apply scoping rules
                    if key == "APP_URL" || key == "LOCAL_APP_URL" {
                        // Determine the scoped path suffix
                        let scope_path = match scope {
                            EventScope::Root => "".to_string(),
                            EventScope::Tenant(id) => format!("/tenant/{}", id),
                            EventScope::Sandbox(id) => format!("/sandbox/{}", id),
                            _ => "".to_string(),
                        };

                        let origin = if key == "LOCAL_APP_URL" {
                            // THE FIX: Natively construct the local URL using the context's port
                            format!("http://127.0.0.1:{}", app.get_port())
                        } else {
                            // Try DB config general.app_url
                            let mut configured = None;
                            if let Ok(Some(val)) = app.get_db().get_config("general").await {
                                if let Some(url) = val
                                    .get("app_url")
                                    .and_then(|v| v.as_str())
                                    .filter(|s| !s.is_empty())
                                {
                                    configured = Some(url.to_string());
                                }
                            }
                            // Fallback to process env APP_URL, then base_url
                            configured
                                .or_else(|| std::env::var("APP_URL").ok().filter(|s| !s.is_empty()))
                                .unwrap_or_else(|| {
                                    base_url_opt
                                        .clone()
                                        .unwrap_or_else(|| "http://localhost:5000".to_string())
                                })
                        };

                        return Ok(format!("{}{}", origin.trim_end_matches('/'), scope_path));
                    }

                    // 2. Attempt to fetch from Database Secrets
                    if let Ok(Some(val)) = app.get_db().get_config(&key).await {
                        // Check if it's an encrypted wrapper
                        if let Ok(enc) = serde_json::from_value::<
                            crate::security::vault::EncryptedValue,
                        >(val.clone())
                        {
                            return app.get_vault().decrypt(&enc).map_err(|e| e.to_string());
                        }
                        return Ok(val.as_str().unwrap_or("").to_string());
                    }

                    // 3. Fallback to reading from the .env file / system process environment
                    if let Ok(env_val) = std::env::var(&key) {
                        return Ok(env_val);
                    }

                    Ok("".to_string())
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result.map(serde_json::Value::String))
    });

    let (app_url, smtp_blocked) = ACTIVE_CONTEXT.with(|c| {
        if let Some((app, handle, base_url_opt, _, _)) = &*c.borrow() {
            let url = base_url_opt.clone().unwrap_or_default();
            let blocked = handle.block_on(async {
                if let Ok(db) = super::db::resolve_db(None, app.clone()).await
                    && let Ok(Some(smtp_val)) = db.get_config("smtp").await
                {
                    return smtp_val
                        .get("block_smtp")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                }
                false
            });
            (url, blocked)
        } else {
            ("".to_string(), false)
        }
    });

    let obj = ObjectInitializer::new(ctx)
        .function(get, JsString::from("get"), 1)
        .property(
            JsString::from("APP_URL"), // Preserved global variable without scope path mapping
            JsString::from(app_url),
            Attribute::all(),
        )
        .property(
            JsString::from("SMTP_BLOCKED"),
            JsValue::from(smtp_blocked),
            Attribute::all(),
        )
        .build();

    ctx.register_global_property(JsString::from("$env"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
