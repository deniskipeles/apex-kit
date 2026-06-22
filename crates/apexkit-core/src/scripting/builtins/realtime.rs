use crate::realtime::{DbEvent, EventScope};

use super::super::{context::ACTIVE_CONTEXT, return_json_promise};
use boa_engine::{
    Context, JsArgs, JsString, NativeFunction, object::ObjectInitializer, property::Attribute,
};

pub fn register_realtime(ctx: &mut Context) -> Result<(), String> {
    let send = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let channel = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let evt = args
            .get_or_undefined(1)
            .to_string(ctx)?
            .to_std_string_escaped();
        let data = args
            .get_or_undefined(2)
            .to_json(ctx)
            .unwrap()
            .unwrap_or(serde_json::Value::Null);

        let res = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, _, _, Some(tx), scope)) = &*c.borrow() {
                let scoped_chan = match scope {
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
                Ok(true)
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, res.map(serde_json::Value::Bool))
    });
    let obj = ObjectInitializer::new(ctx)
        .function(send, JsString::from("send"), 3)
        .build();
    ctx.register_global_property(JsString::from("$realtime"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
