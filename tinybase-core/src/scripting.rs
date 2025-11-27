// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/scripting.rs start here ===========================
use rquickjs::{AsyncContext, AsyncRuntime, Module, Value as JsValue, Function, Error};
use serde_json::Value;
use crate::events::SystemEvent;
use std::sync::Arc;

pub struct ScriptEngine {
    runtime: AsyncRuntime,
    context: AsyncContext,
}

impl ScriptEngine {
    pub async fn new() -> Self {
        let runtime = AsyncRuntime::new().unwrap();
        let context = AsyncContext::full(&runtime).await.unwrap();
        
        Self { runtime, context }
    }

    /// Load all active scripts from the DB into the context
    pub async fn load_script(&self, name: &str, code: &str) -> Result<(), String> {
        self.context.with(|ctx| {
            // We evaluate the script as a module or global code. 
            // For simplicity, we assume scripts define global functions matching event names.
            // e.g. "function onBeforeCreate(event) { ... }"
            ctx.eval::<(), _>(code).map_err(|e| e.to_string())
        }).await
    }

    /// Execute a hook. Returns modified data if the hook returns it, otherwise None.
    pub async fn run_hook(&self, event: &SystemEvent) -> Result<Option<Value>, String> {
        let (fn_name, arg) = match event {
            SystemEvent::BeforeCreate { .. } => ("onBeforeCreate", serde_json::to_string(event).unwrap()),
            SystemEvent::AfterCreate { .. } => ("onAfterCreate", serde_json::to_string(event).unwrap()),
            SystemEvent::BeforeUpdate { .. } => ("onBeforeUpdate", serde_json::to_string(event).unwrap()),
            SystemEvent::AfterUpdate { .. } => ("onAfterUpdate", serde_json::to_string(event).unwrap()),
            SystemEvent::BeforeDelete { .. } => ("onBeforeDelete", serde_json::to_string(event).unwrap()),
            SystemEvent::AfterDelete { .. } => ("onAfterDelete", serde_json::to_string(event).unwrap()),
        };

        self.context.with(move |ctx| {
            let global = ctx.globals();
            
            // Check if function exists
            if !global.contains_key(fn_name).unwrap_or(false) {
                return Ok(None); 
            }

            let func: Function = global.get(fn_name).map_err(|e| e.to_string())?;
            let event_arg = ctx.json_parse(arg).map_err(|e| e.to_string())?;

            let result: JsValue = func.call((event_arg,)).map_err(|e: Error| e.to_string())?;

            if result.is_undefined() || result.is_null() {
                Ok(None)
            } else {
                // If script returned data, convert back to Rust JSON
                // This assumes the user returned the modified 'data' object
                // rquickjs doesn't have a direct "to_serde_value" in 0.6 without features, 
                // so we JSON.stringify inside JS or use specific type coercion.
                // Simplest for now: JSON.stringify the result in JS or assume returned object
                let json_str: String = ctx.json_stringify(result)?.to_string().map_err(|e| e.to_string())?;
                let val: Value = serde_json::from_str(&json_str).map_err(|e| e.to_string())?;
                Ok(Some(val))
            }
        }).await
    }
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/scripting.rs ends here ===========================