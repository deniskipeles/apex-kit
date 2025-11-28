// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/scripting.rs ===========================
use tokio::sync::{mpsc, oneshot};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use crate::Db;
use rquickjs::{AsyncRuntime, AsyncContext, Function, Object, Value, Promise, Error};

// The handle we store in AppState (Must be Send + Sync)
#[derive(Clone)]
pub struct ScriptEngine {
    sender: mpsc::Sender<ScriptJob>,
}

struct ScriptJob {
    code: String,
    input: JsonValue,
    resp: oneshot::Sender<Result<JsonValue, String>>,
}

impl ScriptEngine {
    pub async fn new() -> Self {
        let (tx, mut rx) = mpsc::channel::<ScriptJob>(100);

        // FIX: Spawn a dedicated OS thread for the JS Runtime.
        // This avoids "Send" issues because the runtime never leaves this thread.
        std::thread::spawn(move || {
            // Create a LocalSet to run !Send futures (rquickjs)
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let local = tokio::task::LocalSet::new();

            local.block_on(&rt, async move {
                let runtime = AsyncRuntime::new().unwrap();
                let context = AsyncContext::full(&runtime).await.unwrap();

                while let Some(job) = rx.recv().await {
                    let ScriptJob { code, input, resp } = job;
                    let code_script = code.clone();
                    let input_data = input.clone();

                    // Use with() instead of async_with() for simpler lifetime management inside LocalSet
                    // We can spawn the inner async block on the same LocalSet if needed, 
                    // but since we are already in a LocalSet, standard async_with should work if we don't move context out.
                    // However, async_with returns a Future that is !Send, which is fine here inside LocalSet.
                    
                    let execution_future = context.async_with(move |ctx| Box::pin(async move {
                        let global = ctx.globals();
                        
                        let _ = global.set("log", Function::new(ctx.clone(), move |msg: String| {
                            println!("[JS Script]: {}", msg);
                        }));

                        let input_json = serde_json::to_string(&input_data).unwrap();
                        if let Ok(js_input) = ctx.json_parse(input_json) {
                             let _ = global.set("$input", js_input);
                        }

                        if let Ok(http_obj) = Object::new(ctx.clone()) {
                            let get_ctx = ctx.clone();
                            let _ = http_obj.set("get", Function::new(ctx.clone(), move |url: String| {
                                let future_ctx = get_ctx.clone();
                                let future = async move {
                                    match reqwest::get(&url).await {
                                        Ok(res) => res.text().await.unwrap_or_default(),
                                        Err(e) => format!("Error: {}", e),
                                    }
                                };
                                Promise::wrap_future(&future_ctx, future)
                            }));

                            let post_ctx = ctx.clone();
                            let _ = http_obj.set("post", Function::new(ctx.clone(), move |url: String, body: String| {
                                let future_ctx = post_ctx.clone();
                                let future = async move {
                                    let client = reqwest::Client::new();
                                    match client.post(&url).header("Content-Type", "application/json").body(body).send().await {
                                        Ok(res) => res.text().await.unwrap_or_default(),
                                        Err(e) => format!("Error: {}", e),
                                    }
                                };
                                Promise::wrap_future(&future_ctx, future)
                            }));

                            let _ = global.set("$http", http_obj);
                        }

                        let script = format!(
                            r#"
                            async function main() {{
                                {}
                            }}
                            main()
                            "#,
                            code_script
                        );

                        let promise = ctx.eval::<Promise, _>(script)?;
                        let result_val: Value = promise.finish()?;

                        if result_val.is_undefined() || result_val.is_null() {
                             Ok("null".to_string())
                        } else {
                             let json_str: String = ctx.json_stringify(result_val)?
                                .expect("Serialization failed")
                                .to_string()?;
                             Ok(json_str)
                        }
                    }));
                    
                    // Await result inside LocalSet
                    let result_json = execution_future.await;

                    let final_result = result_json
                        .map_err(|e: Error| e.to_string())
                        .and_then(|json_string: String| {
                             serde_json::from_str(&json_string).map_err(|e| e.to_string())
                        });

                    let _ = resp.send(final_result);
                }
            });
        });

        Self { sender: tx }
    }

    pub async fn run_script(
        &self, 
        code: &str, 
        input_data: JsonValue, 
        _db: Arc<dyn Db> 
    ) -> Result<JsonValue, String> {
        let (tx, rx) = oneshot::channel();
        let job = ScriptJob {
            code: code.to_string(),
            input: input_data,
            resp: tx,
        };

        self.sender.send(job).await.map_err(|_| "Script engine dead".to_string())?;
        rx.await.map_err(|_| "Script execution cancelled".to_string())?
    }
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/scripting.rs ends here ===========================