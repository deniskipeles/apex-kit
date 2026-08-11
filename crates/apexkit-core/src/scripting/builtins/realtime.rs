// =========================== apex-kit/crates/apexkit-core/src/scripting/builtins/realtime.rs start here ===========================
use std::sync::Arc;
use rquickjs::function::Async;
use rquickjs::{Ctx, Function, Object, Value};
use rquickjs_serde::from_value;
use serde_json::json;

use super::super::context::ScriptContext;
use crate::realtime::{DbEvent, EventScope};

pub fn register_realtime<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let realtime_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let app_send = app_ctx.clone();
    let send_fn = Function::new(
        ctx.clone(),
        Async(move |channel: String, evt: String, data_val: Value<'js>| {
            let app = app_send.clone();
            async move {
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
                    scope: EventScope::Channel(scoped_chan),
                });

                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    realtime_obj.set("send", send_fn).map_err(|e| e.to_string())?;
    globals.set("$realtime", realtime_obj).map_err(|e| e.to_string())?;
    Ok(())
}
// =========================== apex-kit/crates/apexkit-core/src/scripting/builtins/realtime.rs ends here ===========================