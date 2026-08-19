use rquickjs::prelude::{Async, Rest};
use rquickjs::{Ctx, Function, Object, Value};
use rquickjs_serde::from_value;
use std::sync::Arc;

use super::super::context::ScriptContext;
use super::db::resolve_db;

pub fn register_console<'js>(
    ctx: &Ctx<'js>,
    app_ctx: Arc<dyn ScriptContext>,
) -> Result<(), String> {
    let globals = ctx.globals();
    let console_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let make_logger = |level: &'static str, app_ctx_inner: Arc<dyn ScriptContext>| {
        let app = app_ctx_inner.clone();

        Function::new(
            ctx.clone(),
            Async(move |args: Rest<Value<'js>>| {
                let app_clone = app.clone();
                async move {
                    let mut formatted = Vec::new();
                    for arg in args.0 {
                        if let Ok(json_val) = from_value::<serde_json::Value>(arg) {
                            if let Some(s) = json_val.as_str() {
                                formatted.push(s.to_string());
                            } else {
                                formatted.push(json_val.to_string());
                            }
                        }
                    }
                    let msg = formatted.join(" ");

                    // ✅ FIX: Resolve the active tenant/sandbox DB instead of hardcoded root DB
                    let db = match resolve_db(None, app_clone.clone()).await {
                        Ok(d) => d,
                        Err(_) => app_clone.get_db(),
                    };

                    let _ = db.log_system_event(level, "script", &msg).await;

                    Ok::<(), rquickjs::Error>(())
                }
            }),
        )
        .unwrap()
    };

    console_obj
        .set("log", make_logger("info", app_ctx.clone()))
        .unwrap();
    console_obj
        .set("info", make_logger("info", app_ctx.clone()))
        .unwrap();
    console_obj
        .set("warn", make_logger("warning", app_ctx.clone()))
        .unwrap();
    console_obj
        .set("error", make_logger("error", app_ctx.clone()))
        .unwrap();

    globals
        .set("console", console_obj)
        .map_err(|e| e.to_string())?;
    Ok(())
}
