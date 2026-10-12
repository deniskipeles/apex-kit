use rquickjs::function::Async;
use rquickjs::{Ctx, Exception, Function, Object, Value};
use rquickjs_serde::from_value;
use serde_json::json;
use std::sync::Arc;

use super::super::context::ScriptContext;
use crate::realtime::{DbEvent, EventScope};

pub fn register_realtime<'js>(
    ctx: &Ctx<'js>,
    app_ctx: Arc<dyn ScriptContext>,
) -> Result<(), String> {
    let globals = ctx.globals();
    let realtime_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let app_send = app_ctx.clone();
    let send_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, channel: String, evt: String, data_val: Value<'js>| {
            let app = app_send.clone();
            async move {
                // --- PREVENT INFINITE LOOP ---
                let hook_name: Option<String> = js_ctx.globals().get("__HOOK_NAME").unwrap_or(None);
                if hook_name.as_deref() == Some("before_realtime_data_pushed") {
                    let err = Exception::from_message(js_ctx.clone(), "$realtime.send is blocked inside before_realtime_data_pushed to prevent infinite loops.").unwrap();
                    return Err(js_ctx.throw(err.into()));
                }
                // -----------------------------

                let data: serde_json::Value = from_value(data_val).unwrap_or(json!({}));
                let scope = app.get_scope();
                let tx = app.get_realtime_tx();

                let scoped_chan = match &scope {
                    EventScope::Root => format!("root::{}", channel),
                    EventScope::Tenant(id) => format!("tenant_{}::{}", id, channel),
                    EventScope::Sandbox(id) => format!("sandbox_{}::{}", id, channel),
                    _ => channel.clone(),
                };

                let _ = tx.send(DbEvent::Custom {
                    event: evt,
                    data,
                    node_id: Some(crate::realtime::types::get_node_id().to_string()),
                    is_local: true,
                    scope: EventScope::Channel(scoped_chan),
                });

                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    realtime_obj
        .set("send", send_fn)
        .map_err(|e| e.to_string())?;
    globals
        .set("$realtime", realtime_obj)
        .map_err(|e| e.to_string())?;
    Ok(())
}
