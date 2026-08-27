use super::super::context::ScriptContext;
use super::db::resolve_db;
use crate::realtime::EventScope;
use rquickjs::function::Async;
use rquickjs::{Ctx, Function, Object};
use rquickjs_serde::to_value;
use std::sync::Arc;

pub fn register_env<'js>(
    ctx: &Ctx<'js>,
    app_ctx: Arc<dyn ScriptContext>,
    base_url: String,
) -> Result<(), String> {
    let globals = ctx.globals();
    let env_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let root_domain = std::env::var("APEXKIT_ROOT_DOMAIN").unwrap_or_default();
    let base_url_clean = base_url.trim_end_matches('/');

    let host_with_port = base_url_clean.split("://").nth(1).unwrap_or(base_url_clean);
    let host = host_with_port.split(':').next().unwrap_or("");

    let scheme = if base_url_clean.starts_with("https://") {
        "https"
    } else {
        "http"
    };
    let is_localhost = host == "localhost"
        || host == "127.0.0.1"
        || host.starts_with("192.168.")
        || host.starts_with("10.");

    // Check if the current host ends with the root domain but is not EXACTLY the root domain.
    // E.g. host = "abc.example.com", root_domain = "example.com".
    let use_subdomain_routing = if !root_domain.is_empty() && !is_localhost {
        host != root_domain && host.ends_with(&root_domain)
    } else {
        false
    };

    let root_url = if !root_domain.is_empty() && !is_localhost {
        format!("{}://{}", scheme, root_domain)
    } else {
        base_url_clean.to_string()
    };

    let scope = app_ctx.get_scope();
    let app_url = match &scope {
        EventScope::Root => root_url.clone(),
        EventScope::Tenant(id) => {
            if use_subdomain_routing {
                format!("{}://{}.{}", scheme, id, root_domain)
            } else {
                format!("{}/tenant/{}", root_url, id)
            }
        }
        EventScope::Sandbox(id) => {
            format!("{}/sandbox/{}", root_url, id)
        }
        _ => root_url.clone(),
    };

    let scope_path = match scope {
        EventScope::Root => "".to_string(),
        EventScope::Tenant(ref id) => format!("/tenant/{}", id),
        EventScope::Sandbox(ref id) => format!("/sandbox/{}", id),
        _ => "".to_string(),
    };

    let local_root = format!("http://127.0.0.1:{}", app_ctx.get_port());
    let local_app = format!("{}{}", local_root, scope_path);

    // Sync Properties
    env_obj
        .set("BASE_URL", root_url.clone())
        .map_err(|e| e.to_string())?;
    env_obj
        .set("APP_URL", app_url.clone())
        .map_err(|e| e.to_string())?;
    env_obj
        .set("LOCAL_BASE_URL", local_root.clone())
        .map_err(|e| e.to_string())?;
    env_obj
        .set("LOCAL_APP_URL", local_app.clone())
        .map_err(|e| e.to_string())?;
    env_obj
        .set("SMTP_BLOCKED", false)
        .map_err(|e| e.to_string())?;

    // $env.get(key) -> Promise<string>
    let app_get = app_ctx.clone();
    let r_url1 = root_url.clone();
    let a_url1 = app_url.clone();
    let lr_url1 = local_root.clone();
    let la_url1 = local_app.clone();
    let get_fn =
        Function::new(
            ctx.clone(),
            Async(move |key: String| {
                let app = app_get.clone();
                let r_url = r_url1.clone();
                let a_url = a_url1.clone();
                let lr_url = lr_url1.clone();
                let la_url = la_url1.clone();

                async move {
                    // Intercept dynamic built-ins to maintain fast synchronous-like behavior
                    if key == "BASE_URL" {
                        return Ok::<String, rquickjs::Error>(r_url);
                    }
                    if key == "APP_URL" {
                        return Ok::<String, rquickjs::Error>(a_url);
                    }
                    if key == "LOCAL_BASE_URL" {
                        return Ok::<String, rquickjs::Error>(lr_url);
                    }
                    if key == "LOCAL_APP_URL" {
                        return Ok::<String, rquickjs::Error>(la_url);
                    }

                    let db = resolve_db(None, app.clone())
                        .await
                        .unwrap_or_else(|_| app.get_db());

                    // 1. Attempt to fetch from Database Secrets
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

                    // 2. Fallback to reading from system environment
                    if let Ok(env_val) = std::env::var(&key) {
                        return Ok::<String, rquickjs::Error>(env_val);
                    }

                    Ok::<String, rquickjs::Error>("".to_string())
                }
            }),
        )
        .map_err(|e| e.to_string())?;

    // $env.has(key) -> Promise<boolean>
    let app_has = app_ctx.clone();
    let has_fn = Function::new(
        ctx.clone(),
        Async(move |key: String| {
            let app = app_has.clone();
            async move {
                if ["BASE_URL", "APP_URL", "LOCAL_BASE_URL", "LOCAL_APP_URL"]
                    .contains(&key.as_str())
                {
                    return Ok::<bool, rquickjs::Error>(true);
                }

                let db = resolve_db(None, app.clone())
                    .await
                    .unwrap_or_else(|_| app.get_db());
                if let Ok(Some(_)) = db.get_config(&key).await {
                    return Ok(true);
                }

                if std::env::var(&key).is_ok() {
                    return Ok(true);
                }

                Ok(false)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // $env.list() -> Promise<Record<string, string>>
    let app_list = app_ctx.clone();
    let r_url2 = root_url.clone();
    let a_url2 = app_url.clone();
    let lr_url2 = local_root.clone();
    let la_url2 = local_app.clone();
    let list_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>| {
            let app = app_list.clone();
            let r_url = r_url2.clone();
            let a_url = a_url2.clone();
            let lr_url = lr_url2.clone();
            let la_url = la_url2.clone();

            async move {
                let mut env_vars = std::collections::HashMap::new();

                env_vars.insert("BASE_URL".to_string(), r_url);
                env_vars.insert("APP_URL".to_string(), a_url);
                env_vars.insert("LOCAL_BASE_URL".to_string(), lr_url);
                env_vars.insert("LOCAL_APP_URL".to_string(), la_url);

                for (k, v) in std::env::vars() {
                    env_vars.insert(k, v);
                }

                let db = resolve_db(None, app.clone())
                    .await
                    .unwrap_or_else(|_| app.get_db());
                if let Ok(configs) = db.list_configs().await {
                    for config in configs {
                        if !config.encrypted {
                            if let Some(val) = config.value {
                                env_vars.insert(config.key, val);
                            }
                        }
                    }
                }

                to_value(js_ctx, &env_vars).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    env_obj.set("get", get_fn).map_err(|e| e.to_string())?;
    env_obj.set("has", has_fn).map_err(|e| e.to_string())?;
    env_obj.set("list", list_fn).map_err(|e| e.to_string())?;

    globals.set("$env", env_obj).map_err(|e| e.to_string())?;
    Ok(())
}
