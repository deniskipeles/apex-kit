use crate::realtime::EventScope;
use serde_json::json;

use super::super::{context::ACTIVE_CONTEXT, return_json_promise};
use boa_engine::{
    Context, JsArgs, JsError, JsString, JsValue, NativeFunction, object::ObjectInitializer,
    property::Attribute,
};

pub fn register_root(ctx: &mut Context) -> Result<(), String> {
    let is_root = ACTIVE_CONTEXT.with(|c| {
        c.borrow()
            .as_ref()
            .map(|t| t.4 == EventScope::Root)
            .unwrap_or(false)
    });

    if is_root {
        // Updated Signature: createTenant(id: string, config?: object)
        let create_tenant = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();

            // Extract config object if present
            let config_val = args.get(1).and_then(|v| v.to_json(ctx).ok()).flatten();

            let (name, tier, owner_id) = if let Some(serde_json::Value::Object(map)) = config_val {
                (
                    map.get("name").and_then(|v| v.as_str()).map(String::from),
                    map.get("tier").and_then(|v| v.as_str()).map(String::from),
                    map.get("owner_id").and_then(|v| v.as_i64()), // Expecting number
                )
            } else {
                (None, None, None)
            };

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        // [CRITICAL FIX]: Register Metadata FIRST so the manager can find it
                        app.get_db()
                            .register_tenant(&id, owner_id, name, tier)
                            .await
                            .map_err(|e| e.to_string())?;

                        // Create Physical Resources SECOND
                        app.admin_create_tenant(id.clone())
                            .await
                            .map_err(|e| e.to_string())?;

                        Ok(true)
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res.map(serde_json::Value::Bool))
        });

        // createSandbox(id: string, config?: object)
        let create_sandbox = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            // Extract config object
            let config_val = args.get(1).and_then(|v| v.to_json(ctx).ok()).flatten();
            let (name, owner_id, expires_at) =
                if let Some(serde_json::Value::Object(map)) = config_val {
                    (
                        map.get("name").and_then(|v| v.as_str()).map(String::from),
                        map.get("owner_id").and_then(|v| v.as_i64()),
                        map.get("expires_at")
                            .and_then(|v| v.as_str())
                            .map(String::from), // ISO String
                    )
                } else {
                    (None, None, None)
                };
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        // 1. Create Physical Resources
                        app.admin_create_sandbox(id.clone())
                            .await
                            .map_err(|e| e.to_string())?;

                        // 2. Register Metadata
                        app.get_db()
                            .register_sandbox(&id, owner_id, name, expires_at, "root", None)
                            .await
                            .map_err(|e| e.to_string())?;

                        Ok(true)
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res.map(serde_json::Value::Bool))
        });

        // 1. createKey(name, config?)
        let create_key = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let config_val = args.get(1).and_then(|v| v.to_json(ctx).ok()).flatten();

            let (tenant_id, issuer, env_type, roles, bypass) =
                if let Some(serde_json::Value::Object(map)) = config_val {
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

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let (raw_key, info) = app
                            .get_db()
                            .create_api_key(&name, &tenant_id, &issuer, &env_type, roles, bypass)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(json!({
                            "key": raw_key,
                            "info": info
                        }))
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        });

        // 2. updateKey(id, updates)
        let update_key = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0) as i64;
            let config_val = args.get(1).and_then(|v| v.to_json(ctx).ok()).flatten();

            let (name, status, roles, bypass) =
                if let Some(serde_json::Value::Object(map)) = config_val {
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

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.get_db()
                            .update_api_key(id, name, status, roles, bypass)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::Value::Bool(true))
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        });

        // 3. deleteKey(id)
        let delete_key = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0) as i64;

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.get_db()
                            .delete_api_key(id)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::Value::Bool(true))
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        });

        // 4. listKeys()
        let list_keys = NativeFunction::from_copy_closure(move |_, _, ctx| {
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        let keys = app
                            .get_db()
                            .list_api_keys()
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(keys).unwrap())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        });

        // 5. updateTenant(id, updates)
        let update_tenant = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let updates = args
                .get_or_undefined(1)
                .to_json(ctx)
                .unwrap()
                .unwrap_or(json!({}));

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.admin_update_tenant(id, updates)
                            .await
                            .map_err(|e| e.to_string())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res.map(|_| serde_json::Value::Bool(true)))
        });

        // 6. deleteTenant(id)
        let delete_tenant = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.admin_delete_tenant(id).await.map_err(|e| e.to_string())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res.map(|_| serde_json::Value::Bool(true)))
        });

        // 7. updateSandbox(id, updates)
        let update_sandbox = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let updates = args
                .get_or_undefined(1)
                .to_json(ctx)
                .unwrap()
                .unwrap_or(json!({}));
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.admin_update_sandbox(id, updates)
                            .await
                            .map_err(|e| e.to_string())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res.map(|_| serde_json::Value::Bool(true)))
        });

        // 8. deleteSandbox(id)
        let delete_sandbox = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        app.admin_delete_sandbox(id)
                            .await
                            .map_err(|e| e.to_string())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res.map(|_| serde_json::Value::Bool(true)))
        });

        // 9. getTenantUsage(id) -> number (bytes)
        let get_tenant_usage = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async { app.admin_get_tenant_usage(id).await })
                } else {
                    Err("Context lost".into())
                }
            });

            // Return number (or null on error/empty)
            match res {
                Ok(bytes) => Ok(JsValue::from(bytes as f64)), // JS uses f64 for numbers
                Err(e) => Err(JsError::from_opaque(JsString::from(e).into())),
            }
        });

        // 10. getSandboxUsage(id)
        let get_sandbox_usage = NativeFunction::from_copy_closure(move |_, args, ctx| {
            let id = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();

            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async { app.admin_get_sandbox_usage(id).await })
                } else {
                    Err("Context lost".into())
                }
            });

            match res {
                Ok(bytes) => Ok(JsValue::from(bytes as f64)),
                Err(e) => Err(JsError::from_opaque(JsString::from(e).into())),
            }
        });

        // 11. listTenants() -> Array
        let list_tenants = NativeFunction::from_copy_closure(move |_, _, ctx| {
            let res = ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, _)) = &*c.borrow() {
                    handle.block_on(async {
                        // list_tenants returns Vec<Tenant>
                        let tenants = app
                            .get_db()
                            .list_tenants()
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::to_value(tenants).unwrap())
                    })
                } else {
                    Err("Context lost".into())
                }
            });
            return_json_promise(ctx, res)
        });

        // Create DB Object for $root.db
        let db_obj = super::db::create_db_object(ctx, super::db::DbMode::Root)?;

        let obj = ObjectInitializer::new(ctx)
            // API Keys
            .function(create_key, JsString::from("createKey"), 2)
            .function(update_key, JsString::from("updateKey"), 2)
            .function(delete_key, JsString::from("deleteKey"), 1)
            .function(list_keys, JsString::from("listKeys"), 0)
            // Tenant Management
            .function(create_tenant, JsString::from("createTenant"), 2)
            .function(update_tenant, JsString::from("updateTenant"), 2)
            .function(delete_tenant, JsString::from("deleteTenant"), 1)
            .function(get_tenant_usage, JsString::from("getTenantDiskUsage"), 1)
            .function(list_tenants, JsString::from("listTenants"), 0)
            // Sandbox Management
            .function(create_sandbox, JsString::from("createSandbox"), 2)
            .function(update_sandbox, JsString::from("updateSandbox"), 2)
            .function(delete_sandbox, JsString::from("deleteSandbox"), 1)
            .function(get_sandbox_usage, JsString::from("getSandboxDiskUsage"), 1)
            .property(JsString::from("db"), db_obj, Attribute::all())
            .build();
        ctx.register_global_property(JsString::from("$root"), obj, Attribute::all())
            .map_err(|e| e.to_string())
    } else {
        ctx.register_global_property(JsString::from("$root"), JsValue::null(), Attribute::all())
            .map_err(|e| e.to_string())
    }
}
