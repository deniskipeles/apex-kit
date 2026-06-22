use crate::realtime::EventScope;

use super::super::context::ACTIVE_CONTEXT;
use boa_engine::{
    Context, JsString, JsValue, NativeFunction, object::ObjectInitializer, property::Attribute,
};

// --- MODULE REGISTRATIONS ---

pub fn register_console(ctx: &mut Context) -> Result<(), String> {
    let make_logger = |level: &'static str| {
        NativeFunction::from_copy_closure(move |_, args, ctx| {
            let msg = args
                .iter()
                .map(|a| a.to_string(ctx).unwrap_or_default().to_std_string_escaped())
                .collect::<Vec<_>>()
                .join(" ");

            ACTIVE_CONTEXT.with(|c| {
                if let Some((app, handle, _, _, scope)) = &*c.borrow() {
                    let is_root = matches!(scope, EventScope::Root);
                    let debug_mode = std::env::var("DEBUG").unwrap_or_default() == "true";

                    if is_root && debug_mode {
                        if level == "error" {
                            eprintln!("[SCRIPT ERROR] {}", msg);
                        } else {
                            println!("[SCRIPT {}] {}", level.to_uppercase(), msg);
                        }
                    }

                    handle.block_on(async {
                        let db = match scope {
                            EventScope::Tenant(id) => app.resolve_tenant_db(id).await,
                            EventScope::Sandbox(id) => app.resolve_sandbox_db(id).await,
                            _ => Some(app.get_db()),
                        };

                        if let Some(db) = db {
                            let _ = db.log_system_event(level, "script", &msg).await;
                        }
                    });
                }
            });

            Ok(JsValue::undefined())
        })
    };

    let log_fn = make_logger("info");
    let info_fn = make_logger("info");
    let warn_fn = make_logger("warning");
    let error_fn = make_logger("error");

    let obj = ObjectInitializer::new(ctx)
        .function(log_fn, JsString::from("log"), 1)
        .function(info_fn, JsString::from("info"), 1)
        .function(warn_fn, JsString::from("warn"), 1)
        .function(error_fn, JsString::from("error"), 1)
        .build();

    ctx.register_global_property(JsString::from("console"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
