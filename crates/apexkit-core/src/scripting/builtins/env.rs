use super::super::context::ScriptContext;
use super::db::resolve_db;
use crate::realtime::EventScope;
use rquickjs::function::Async;
use rquickjs::{Ctx, Function, Object};
use std::sync::Arc;

pub fn register_env<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let env_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // $env.get(key) -> Promise<string>
    let app_get = app_ctx.clone();
    let get_fn =
        Function::new(
            ctx.clone(),
            Async(move |key: String| {
                let app = app_get.clone();
                async move {
                    let db = resolve_db(None, app.clone())
                        .await
                        .unwrap_or_else(|_| app.get_db());

                    // 1. Intercept APP_URL and LOCAL_APP_URL to apply scoping rules
                    if key == "APP_URL" || key == "LOCAL_APP_URL" {
                        let scope = app.get_scope();
                        let scope_path = match scope {
                            EventScope::Root => "".to_string(),
                            EventScope::Tenant(id) => format!("/tenant/{}", id),
                            EventScope::Sandbox(id) => format!("/sandbox/{}", id),
                            _ => "".to_string(),
                        };

                        let origin = if key == "LOCAL_APP_URL" {
                            format!("http://127.0.0.1:{}", app.get_port())
                        } else {
                            let mut configured = None;
                            if let Ok(Some(val)) = db.get_config("general").await {
                                if let Some(url) = val
                                    .get("app_url")
                                    .and_then(|v| v.as_str())
                                    .filter(|s| !s.is_empty())
                                {
                                    configured = Some(url.to_string());
                                }
                            }
                            configured
                                .or_else(|| std::env::var("APP_URL").ok().filter(|s| !s.is_empty()))
                                .unwrap_or_else(|| format!("http://localhost:{}", app.get_port()))
                        };

                        return Ok::<String, rquickjs::Error>(format!(
                            "{}{}",
                            origin.trim_end_matches('/'),
                            scope_path
                        ));
                    }

                    // 2. Attempt to fetch from Database Secrets
                    if let Ok(Some(val)) = db.get_config(&key).await {
                        if let Ok(enc) = serde_json::from_value::<
                            crate::security::vault::EncryptedValue,
                        >(val.clone())
                        {
                            return Ok::<String, rquickjs::Error>(
                                app.get_vault()
                                    .decrypt(&enc)
                                    .map_err(|_| rquickjs::Error::Exception)?,
                            );
                        }

                        let val_str = match val {
                            serde_json::Value::String(s) => s,
                            serde_json::Value::Bool(b) => b.to_string(),
                            serde_json::Value::Number(n) => n.to_string(),
                            _ => val.to_string(),
                        };
                        return Ok::<String, rquickjs::Error>(val_str);
                    }

                    // 3. Fallback to reading from system environment
                    if let Ok(env_val) = std::env::var(&key) {
                        return Ok::<String, rquickjs::Error>(env_val);
                    }

                    Ok::<String, rquickjs::Error>("".to_string())
                }
            }),
        )
        .map_err(|e| e.to_string())?;

    let app_url = format!("http://localhost:{}", app_ctx.get_port());

    env_obj.set("get", get_fn).map_err(|e| e.to_string())?;
    env_obj.set("APP_URL", app_url).map_err(|e| e.to_string())?;
    env_obj
        .set("SMTP_BLOCKED", false)
        .map_err(|e| e.to_string())?;

    globals.set("$env", env_obj).map_err(|e| e.to_string())?;
    Ok(())
}
