use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rquickjs::function::Async;
use rquickjs::{Ctx, Exception, Function, Object, Value};
use rquickjs_serde::{from_value, to_value};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::super::context::ScriptContext;
use crate::realtime::EventScope;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QueueJobInfo {
    pub pid: String,
    pub status: String, // "queued" | "running" | "completed" | "failed" | "timed_out" | "not_found"
    pub runtime_ms: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

struct InternalJobState {
    pid: String,
    scope: EventScope,
    status: String,
    start_time: Option<Instant>,
    runtime_ms: u64,
    result: Option<serde_json::Value>,
    error: Option<String>,
}

static QUEUE_TRACKER: OnceLock<Mutex<HashMap<String, InternalJobState>>> = OnceLock::new();

fn get_queue_tracker() -> &'static Mutex<HashMap<String, InternalJobState>> {
    QUEUE_TRACKER.get_or_init(|| Mutex::new(HashMap::new()))
}

fn scope_to_key(scope: &EventScope) -> String {
    match scope {
        EventScope::Root => "root".to_string(),
        EventScope::Tenant(id) => format!("tenant_{}", id),
        EventScope::Sandbox(id) => format!("sandbox_{}", id),
        EventScope::Channel(c) => format!("channel_{}", c),
    }
}

#[derive(Deserialize, Default)]
struct SpawnOptions {
    #[serde(rename = "timeoutMs")]
    timeout_ms: Option<u64>,
    args: Option<serde_json::Value>,
}

pub fn register_queue<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let queue_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // 1. $__native_queue.spawn(codeStr, options) -> Promise<{ pid, status }>
    let app_spawn = app_ctx.clone();
    let spawn_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, code_str: String, opts_val: Option<Value<'js>>| {
                let app = app_spawn.clone();
                async move {
                    let opts: SpawnOptions = opts_val
                        .and_then(|v| from_value(v).ok())
                        .unwrap_or_default();

                    let timeout_ms = opts.timeout_ms.unwrap_or(60_000).min(60_000); // 60s max execution
                    let scope = app.get_scope();
                    let scope_key = scope_to_key(&scope);

                    // --- 5-MINUTE SLIDING WINDOW CPU QUOTA (60s total CPU per 5 mins) ---
                    let now_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let window_5m = now_secs / 300; // 300s = 5 minutes
                    let quota_key = format!("queue_cpu_sec:{}:{}", scope_key, window_5m);

                    let used_secs = app
                        .cache_get(&quota_key)
                        .await
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0);

                    if used_secs >= 60 {
                        let js_err = Exception::from_message(
                            js_ctx.clone(),
                            "QuotaExceeded: Queue CPU limit of 60 seconds per 5 minutes reached.",
                        )
                        .unwrap();
                        return Err(js_ctx.throw(js_err.into()));
                    }

                    // Reserve budget in advance
                    let estimated_secs = (timeout_ms / 1000).max(1);
                    app.cache_incr(&quota_key, estimated_secs as i64).await;

                    let pid = format!(
                        "job_{}",
                        uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string()
                    );

                    {
                        let mut tracker = get_queue_tracker().lock().unwrap();
                        tracker.insert(
                            pid.clone(),
                            InternalJobState {
                                pid: pid.clone(),
                                scope: scope.clone(),
                                status: "queued".to_string(),
                                start_time: None,
                                runtime_ms: 0,
                                result: None,
                                error: None,
                            },
                        );
                    }

                    let pid_clone = pid.clone();
                    let app_exec = app.clone();
                    let scope_clone = scope.clone();
                    let args_payload = opts.args.unwrap_or_else(|| json!({}));

                    // Wrap the provided JS function into an executable runner
                    let is_module_script = code_str.contains("export default");
                    let runnable_code = if is_module_script {
                        code_str
                    } else {
                        format!(
                            r#"
                            const __userFn = {};
                            export default async function(req) {{
                                return await __userFn("{}", req);
                            }}
                            "#,
                            code_str.trim(),
                            pid_clone
                        )
                    };

                    tokio::spawn(async move {
                        {
                            let mut tracker = get_queue_tracker().lock().unwrap();
                            if let Some(state) = tracker.get_mut(&pid_clone) {
                                state.status = "running".to_string();
                                state.start_time = Some(Instant::now());
                            }
                        }

                        let start = Instant::now();
                        let exec_future = app_exec.execute_shared_script(
                            runnable_code,
                            args_payload,
                            scope_clone,
                        );

                        match tokio::time::timeout(Duration::from_millis(timeout_ms), exec_future)
                            .await
                        {
                            Ok(Ok(res)) => {
                                let elapsed = start.elapsed().as_millis() as u64;
                                let mut tracker = get_queue_tracker().lock().unwrap();
                                if let Some(state) = tracker.get_mut(&pid_clone) {
                                    state.status = "completed".to_string();
                                    state.runtime_ms = elapsed;
                                    state.result = Some(res);
                                }
                            }
                            Ok(Err(err_msg)) => {
                                let elapsed = start.elapsed().as_millis() as u64;
                                let mut tracker = get_queue_tracker().lock().unwrap();
                                if let Some(state) = tracker.get_mut(&pid_clone) {
                                    state.status = "failed".to_string();
                                    state.runtime_ms = elapsed;
                                    state.error = Some(err_msg);
                                }
                            }
                            Err(_) => {
                                let mut tracker = get_queue_tracker().lock().unwrap();
                                if let Some(state) = tracker.get_mut(&pid_clone) {
                                    state.status = "timed_out".to_string();
                                    state.runtime_ms = timeout_ms;
                                    state.error =
                                        Some(format!("Execution timed out after {}ms", timeout_ms));
                                }
                            }
                        }
                    });

                    let res = json!({ "pid": pid, "status": "queued" });
                    to_value(js_ctx.clone(), &res).map_err(|e| {
                        let js_err = Exception::from_message(
                            js_ctx.clone(),
                            &format!("Serialization error: {}", e),
                        )
                        .unwrap();
                        js_ctx.throw(js_err.into())
                    })
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // 2. $__native_queue.status(pid) -> Promise<QueueJobInfo> (Strictly Scope Isolated)
    let app_status = app_ctx.clone();
    let status_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, pid: String| {
            let app = app_status.clone();
            async move {
                let current_scope = app.get_scope();
                let tracker = get_queue_tracker().lock().unwrap();

                let val = if let Some(state) = tracker.get(&pid) {
                    // Enforce tenant boundary: non-root cannot inspect jobs from another scope
                    if state.scope == current_scope || matches!(current_scope, EventScope::Root) {
                        let runtime = state
                            .start_time
                            .map(|s| s.elapsed().as_millis() as u64)
                            .unwrap_or(state.runtime_ms);

                        json!({
                            "pid": state.pid,
                            "status": state.status,
                            "runtime_ms": runtime,
                            "error": state.error
                        })
                    } else {
                        json!({ "pid": pid, "status": "not_found" })
                    }
                } else {
                    json!({ "pid": pid, "status": "not_found" })
                };

                to_value(js_ctx.clone(), &val).map_err(|e| {
                    let js_err = Exception::from_message(
                        js_ctx.clone(),
                        &format!("Serialization error: {}", e),
                    )
                    .unwrap();
                    js_ctx.throw(js_err.into())
                })
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // 3. $__native_queue.result(pid) -> Promise<{ pid, status, result, error }> (Strictly Scope Isolated)
    let app_result = app_ctx.clone();
    let result_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, pid: String| {
            let app = app_result.clone();
            async move {
                let current_scope = app.get_scope();
                let tracker = get_queue_tracker().lock().unwrap();

                let val = if let Some(state) = tracker.get(&pid) {
                    // Enforce tenant boundary: non-root cannot access results from another scope
                    if state.scope == current_scope || matches!(current_scope, EventScope::Root) {
                        let runtime = state
                            .start_time
                            .map(|s| s.elapsed().as_millis() as u64)
                            .unwrap_or(state.runtime_ms);

                        json!({
                            "pid": state.pid,
                            "status": state.status,
                            "runtime_ms": runtime,
                            "result": state.result,
                            "error": state.error
                        })
                    } else {
                        json!({ "pid": pid, "status": "not_found" })
                    }
                } else {
                    json!({ "pid": pid, "status": "not_found" })
                };

                to_value(js_ctx.clone(), &val).map_err(|e| {
                    let js_err = Exception::from_message(
                        js_ctx.clone(),
                        &format!("Serialization error: {}", e),
                    )
                    .unwrap();
                    js_ctx.throw(js_err.into())
                })
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    queue_obj
        .set("spawn", spawn_fn)
        .map_err(|e| e.to_string())?;
    queue_obj
        .set("status", status_fn)
        .map_err(|e| e.to_string())?;
    queue_obj
        .set("result", result_fn)
        .map_err(|e| e.to_string())?;

    globals
        .set("$__native_queue", queue_obj)
        .map_err(|e| e.to_string())?;
    Ok(())
}
