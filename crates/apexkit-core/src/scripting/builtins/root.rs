use rquickjs::function::Async;
use rquickjs::{Ctx, Exception, Function, Object, Value};
use rquickjs_serde::{from_value, to_value};
use serde_json::json;
use std::sync::Arc;

use super::super::context::ScriptContext;
use crate::realtime::EventScope;

fn throw_err<'js, T>(ctx: &Ctx<'js>, msg: &str) -> rquickjs::Result<T> {
    let err = Exception::from_message(ctx.clone(), msg).unwrap();
    Err(ctx.throw(err.into()))
}

pub fn register_root<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();

    // Security Guard: $root is ONLY available when executing in the Root Scope
    if app_ctx.get_scope() != EventScope::Root {
        globals
            .set("$root", Value::new_null(ctx.clone()))
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    let root_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // Attach DB Object for $root.db
    let db_obj = super::db::create_db_object(ctx, app_ctx.clone(), super::db::DbMode::Root)?;
    root_obj.set("db", db_obj).map_err(|e| e.to_string())?;

    // --- 1. TENANT MANAGEMENT ---

    // $root.createTenant(id, config?)
    let app_c_tenant = app_ctx.clone();
    let create_tenant_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, id: String, config_val: Option<Value<'js>>| {
                let config = config_val.and_then(|v| from_value::<serde_json::Value>(v).ok());
                let app = app_c_tenant.clone();
                async move {
                    let (name, tier, owner_id) =
                        if let Some(serde_json::Value::Object(map)) = config {
                            (
                                map.get("name").and_then(|v| v.as_str()).map(String::from),
                                map.get("tier").and_then(|v| v.as_str()).map(String::from),
                                map.get("owner_id").and_then(|v| v.as_i64()),
                            )
                        } else {
                            (None, None, None)
                        };

                    if let Err(e) = app
                        .get_db()
                        .register_tenant(&id, owner_id, name, tier)
                        .await
                    {
                        return throw_err(&js_ctx, &format!("Failed to register tenant: {}", e));
                    }

                    if let Err(e) = app.admin_create_tenant(id).await {
                        return throw_err(&js_ctx, &format!("Failed to provision tenant: {}", e));
                    }

                    Ok::<bool, rquickjs::Error>(true)
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // $root.updateTenant(id, updates)
    let app_u_tenant = app_ctx.clone();
    let update_tenant_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, id: String, updates_val: Value<'js>| {
                let updates: serde_json::Value = from_value(updates_val).unwrap_or(json!({}));
                let app = app_u_tenant.clone();
                async move {
                    if let Err(e) = app.admin_update_tenant(id, updates).await {
                        return throw_err(&js_ctx, &format!("Failed to update tenant: {}", e));
                    }
                    Ok::<bool, rquickjs::Error>(true)
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // $root.deleteTenant(id)
    let app_d_tenant = app_ctx.clone();
    let delete_tenant_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, id: String| {
            let app = app_d_tenant.clone();
            async move {
                if let Err(e) = app.admin_delete_tenant(id).await {
                    return throw_err(&js_ctx, &format!("Failed to delete tenant: {}", e));
                }
                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // $root.getTenantDiskUsage(id) -> number (bytes)
    let app_usage_tenant = app_ctx.clone();
    let get_tenant_usage_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, id: String| {
            let app = app_usage_tenant.clone();
            async move {
                match app.admin_get_tenant_usage(id).await {
                    Ok(bytes) => Ok::<f64, rquickjs::Error>(bytes as f64),
                    Err(e) => throw_err(&js_ctx, &format!("Failed to get tenant usage: {}", e)),
                }
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // $root.listTenants() -> Array<Tenant>
    let app_list_tenants = app_ctx.clone();
    let list_tenants_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>| {
            let app = app_list_tenants.clone();
            async move {
                let tenants = match app.get_db().list_tenants().await {
                    Ok(t) => t,
                    Err(e) => return throw_err(&js_ctx, &format!("Failed to list tenants: {}", e)),
                };
                to_value(js_ctx.clone(), &json!(tenants)).map_err(|e| {
                    let err = Exception::from_message(js_ctx.clone(), &e.to_string()).unwrap();
                    js_ctx.throw(err.into())
                })
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // --- 2. SANDBOX MANAGEMENT ---

    // $root.createSandbox(id, config?)
    let app_c_sandbox = app_ctx.clone();
    let create_sandbox_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, id: String, config_val: Option<Value<'js>>| {
                let config = config_val
                    .and_then(|v| from_value::<serde_json::Value>(v).ok())
                    .unwrap_or_else(|| json!({}));
                let app = app_c_sandbox.clone();

                async move {
                    let (name, owner_id, expires_at) = if let Some(map) = config.as_object() {
                        (
                            map.get("name").and_then(|v| v.as_str()).map(String::from),
                            map.get("owner_id").and_then(|v| v.as_i64()),
                            map.get("expires_at")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                        )
                    } else {
                        (None, None, None)
                    };

                    if let Err(e) = app.admin_create_sandbox(id.clone(), config).await {
                        return throw_err(&js_ctx, &format!("Failed to provision sandbox: {}", e));
                    }

                    let general_config =
                        app.get_db().get_config("general").await.unwrap_or_default();
                    let max_storage_mb = general_config
                        .as_ref()
                        .and_then(|v| v.get("max_sandbox_storage_mb").and_then(|n| n.as_i64()))
                        .unwrap_or(100);
                    let max_vectors = general_config
                        .as_ref()
                        .and_then(|v| v.get("max_sandbox_vectors").and_then(|n| n.as_i64()))
                        .unwrap_or(10000);
                    let max_ai_requests = general_config
                        .as_ref()
                        .and_then(|v| v.get("max_sandbox_ai_requests").and_then(|n| n.as_i64()))
                        .unwrap_or(100);

                    if let Err(e) = app
                        .get_db()
                        .register_sandbox(
                            &id,
                            owner_id,
                            name,
                            expires_at,
                            "root",
                            None,
                            max_storage_mb,
                            max_vectors,
                            max_ai_requests,
                        )
                        .await
                    {
                        return throw_err(&js_ctx, &format!("Failed to register sandbox: {}", e));
                    }

                    Ok::<bool, rquickjs::Error>(true)
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // $root.updateSandbox(id, updates)
    let app_u_sandbox = app_ctx.clone();
    let update_sandbox_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, id: String, updates_val: Value<'js>| {
                let updates: serde_json::Value = from_value(updates_val).unwrap_or(json!({}));
                let app = app_u_sandbox.clone();
                async move {
                    if let Err(e) = app.admin_update_sandbox(id, updates).await {
                        return throw_err(&js_ctx, &format!("Failed to update sandbox: {}", e));
                    }
                    Ok::<bool, rquickjs::Error>(true)
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // $root.deleteSandbox(id)
    let app_d_sandbox = app_ctx.clone();
    let delete_sandbox_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, id: String| {
            let app = app_d_sandbox.clone();
            async move {
                if let Err(e) = app.admin_delete_sandbox(id).await {
                    return throw_err(&js_ctx, &format!("Failed to delete sandbox: {}", e));
                }
                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // $root.getSandboxDiskUsage(id) -> number
    let app_usage_sandbox = app_ctx.clone();
    let get_sandbox_usage_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, id: String| {
            let app = app_usage_sandbox.clone();
            async move {
                match app.admin_get_sandbox_usage(id).await {
                    Ok(bytes) => Ok::<f64, rquickjs::Error>(bytes as f64),
                    Err(e) => throw_err(&js_ctx, &format!("Failed to get sandbox usage: {}", e)),
                }
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // --- 3. API KEY MANAGEMENT ---

    // $root.createKey(name, config?)
    let app_c_key = app_ctx.clone();
    let create_key_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, name: String, config_val: Option<Value<'js>>| {
                let config = config_val.and_then(|v| from_value::<serde_json::Value>(v).ok());
                let app = app_c_key.clone();

                async move {
                    let (tenant_id, issuer, env_type, roles, bypass) =
                        if let Some(serde_json::Value::Object(map)) = config {
                            (
                                map.get("tenant_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("root")
                                    .to_string(),
                                map.get("issuer")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("root")
                                    .to_string(),
                                map.get("env_type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("sys")
                                    .to_string(),
                                map.get("roles")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                            .collect::<Vec<String>>()
                                    })
                                    .unwrap_or_else(|| vec!["admin".to_string()]),
                                map.get("bypass_cors")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false),
                            )
                        } else {
                            (
                                "root".to_string(),
                                "root".to_string(),
                                "sys".to_string(),
                                vec!["admin".to_string()],
                                false,
                            )
                        };

                    let (raw_key, info) = match app
                        .get_db()
                        .create_api_key(&name, &tenant_id, &issuer, &env_type, roles, bypass)
                        .await
                    {
                        Ok(res) => res,
                        Err(e) => {
                            return throw_err(&js_ctx, &format!("Failed to create API key: {}", e));
                        }
                    };

                    let res = json!({
                        "key": raw_key,
                        "info": info
                    });

                    to_value(js_ctx.clone(), &res).map_err(|e| {
                        let err = Exception::from_message(js_ctx.clone(), &e.to_string()).unwrap();
                        js_ctx.throw(err.into())
                    })
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // $root.updateKey(id, updates)
    let app_u_key = app_ctx.clone();
    let update_key_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, id: i64, config_val: Value<'js>| {
            let config: serde_json::Value = from_value(config_val).unwrap_or(json!({}));
            let app = app_u_key.clone();

            async move {
                let (name, status, roles, bypass) = if let Some(map) = config.as_object() {
                    (
                        map.get("name").and_then(|v| v.as_str()).map(String::from),
                        map.get("status").and_then(|v| v.as_str()).map(String::from),
                        map.get("roles").and_then(|v| v.as_array()).map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect::<Vec<String>>()
                        }),
                        map.get("bypass_cors").and_then(|v| v.as_bool()),
                    )
                } else {
                    (None, None, None, None)
                };

                if let Err(e) = app
                    .get_db()
                    .update_api_key(id, name, status, roles, bypass)
                    .await
                {
                    return throw_err(&js_ctx, &format!("Failed to update API key: {}", e));
                }

                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // $root.deleteKey(id)
    let app_d_key = app_ctx.clone();
    let delete_key_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, id: i64| {
            let app = app_d_key.clone();
            async move {
                if let Err(e) = app.get_db().delete_api_key(id).await {
                    return throw_err(&js_ctx, &format!("Failed to delete API key: {}", e));
                }
                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // $root.listKeys()
    let app_l_key = app_ctx.clone();
    let list_keys_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>| {
            let app = app_l_key.clone();
            async move {
                let keys = match app.get_db().list_api_keys().await {
                    Ok(k) => k,
                    Err(e) => {
                        return throw_err(&js_ctx, &format!("Failed to list API keys: {}", e));
                    }
                };
                to_value(js_ctx.clone(), &json!(keys)).map_err(|e| {
                    let err = Exception::from_message(js_ctx.clone(), &e.to_string()).unwrap();
                    js_ctx.throw(err.into())
                })
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // Bind Functions
    root_obj
        .set("createTenant", create_tenant_fn)
        .map_err(|e| e.to_string())?;
    root_obj
        .set("updateTenant", update_tenant_fn)
        .map_err(|e| e.to_string())?;
    root_obj
        .set("deleteTenant", delete_tenant_fn)
        .map_err(|e| e.to_string())?;
    root_obj
        .set("getTenantDiskUsage", get_tenant_usage_fn)
        .map_err(|e| e.to_string())?;
    root_obj
        .set("listTenants", list_tenants_fn)
        .map_err(|e| e.to_string())?;

    root_obj
        .set("createSandbox", create_sandbox_fn)
        .map_err(|e| e.to_string())?;
    root_obj
        .set("updateSandbox", update_sandbox_fn)
        .map_err(|e| e.to_string())?;
    root_obj
        .set("deleteSandbox", delete_sandbox_fn)
        .map_err(|e| e.to_string())?;
    root_obj
        .set("getSandboxDiskUsage", get_sandbox_usage_fn)
        .map_err(|e| e.to_string())?;

    root_obj
        .set("createKey", create_key_fn)
        .map_err(|e| e.to_string())?;
    root_obj
        .set("updateKey", update_key_fn)
        .map_err(|e| e.to_string())?;
    root_obj
        .set("deleteKey", delete_key_fn)
        .map_err(|e| e.to_string())?;
    root_obj
        .set("listKeys", list_keys_fn)
        .map_err(|e| e.to_string())?;

    globals.set("$root", root_obj).map_err(|e| e.to_string())?;
    Ok(())
}
