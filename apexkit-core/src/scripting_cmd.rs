use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::collections::HashMap;
use tokio::sync::Semaphore;
use std::time::{Instant, Duration};
use std::process::Stdio;
use tokio::time::timeout;

use boa_engine::{
    Context, NativeFunction, JsString, JsArgs,
    object::ObjectInitializer,
    property::Attribute,
};
use serde_json::json;
use crate::realtime::EventScope;
use crate::scripting::{ACTIVE_CONTEXT, return_json_promise};

// --- GLOBAL PROCESS TRACKER ---
struct ProcessInfo {
    pid: u32,
    program: String,
    start_time: Instant,
    status: String, // "running", "completed", "failed", "timed_out"
    exit_code: Option<i32>,
}

static PROCESS_TRACKER: OnceLock<Mutex<HashMap<u32, ProcessInfo>>> = OnceLock::new();
static SEMAPHORE_MAP: OnceLock<RwLock<HashMap<String, Arc<Semaphore>>>> = OnceLock::new();

fn get_tracker() -> &'static Mutex<HashMap<u32, ProcessInfo>> {
    PROCESS_TRACKER.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_semaphores() -> &'static RwLock<HashMap<String, Arc<Semaphore>>> {
    SEMAPHORE_MAP.get_or_init(|| RwLock::new(HashMap::new()))
}

fn get_semaphore_for_program(program: &str) -> Arc<Semaphore> {
    let map = get_semaphores().read().unwrap();
    if let Some(sem) = map.get(program) {
        return sem.clone();
    }
    
    // If not found in read-lock, check default
    if let Some(default_sem) = map.get("*") {
        return default_sem.clone();
    }
    
    // Release read lock to acquire write lock
    drop(map);
    
    let mut write_map = get_semaphores().write().unwrap();
    // Double check (race condition)
    if let Some(sem) = write_map.get(program) {
        return sem.clone();
    }
    
    // Ensure default exists
    if !write_map.contains_key("*") {
        write_map.insert("*".to_string(), Arc::new(Semaphore::new(2))); // Default limit 2
    }
    
    write_map.get("*").unwrap().clone()
}

pub fn register_cmd(ctx: &mut Context) -> Result<(), String> {
    
    // Shared helper for option parsing
    let parse_options = |ctx: &mut Context, val: &boa_engine::JsValue| -> (Option<String>, Option<HashMap<String, String>>, Option<u64>) {
        let mut cwd = None;
        let mut envs = None;
        let mut timeout_ms = None;
        
        if let Ok(Some(json)) = val.to_json(ctx) {
            if let Some(obj) = json.as_object() {
                cwd = obj.get("cwd").and_then(|v: &serde_json::Value| v.as_str().map(|s| s.to_string()));
                timeout_ms = obj.get("timeout").and_then(|v: &serde_json::Value| v.as_u64());

                if let Some(e) = obj.get("env").and_then(|v: &serde_json::Value| v.as_object()) {
                    let mut map = HashMap::new();
                    for (k, v) in e {
                        let val_str = if let Some(s) = v.as_str() { s.to_string() } else { v.to_string() };
                        map.insert(k.clone(), val_str);
                    }
                    envs = Some(map);
                }
            }
        }
        (cwd, envs, timeout_ms)
    };

    // $cmd.setLimit(program, limit)
    let set_limit_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let program = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let limit = args.get_or_undefined(1).to_number(ctx).unwrap_or(2.0) as usize;

        let result = ACTIVE_CONTEXT.with(|c| {
             if let Some((_, _, _, _, scope)) = &*c.borrow() {
                 if !matches!(scope, EventScope::Root) { return Err("Access Denied.".into()); }
                 
                 let mut map = get_semaphores().write().unwrap();
                 map.insert(program.clone(), Arc::new(Semaphore::new(limit)));
                 
                 Ok(json!(true))
             } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result)
    });

    // $cmd.run(program, args, options) -> Promise (Waiting)
    let run_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let program = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let args_val = args.get_or_undefined(1);
        let options_val = args.get_or_undefined(2);
        
        let (cwd, envs, timeout_ms) = parse_options(ctx, options_val);
        let mut cmd_args: Vec<String> = Vec::new();

        if let Ok(Some(json_val)) = args_val.to_json(ctx) {
            if let Some(arr) = json_val.as_array() {
                for v in arr {
                    if let Some(s) = v.as_str() { cmd_args.push(s.to_string()); }
                    else { cmd_args.push(v.to_string()); }
                }
            }
        }

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                if !matches!(scope, EventScope::Root) {
                    return Err("Access Denied: $cmd is reserved for Root scripts.".into());
                }

                handle.block_on(async {
                    let sem = get_semaphore_for_program(&program);
                    let _permit = sem.acquire().await.map_err(|e| e.to_string())?;

                    let mut command = tokio::process::Command::new(&program);
                    command.args(&cmd_args);
                    command.stdout(Stdio::piped());
                    command.stderr(Stdio::piped());
                    
                    if let Some(dir) = cwd { command.current_dir(dir); }
                    if let Some(vars) = envs { command.envs(vars); }

                    let ms = timeout_ms.unwrap_or(30_000); 
                    let future = command.output();
                    
                    match timeout(std::time::Duration::from_millis(ms), future).await {
                        Ok(res) => match res {
                            Ok(output) => Ok(json!({
                                "stdout": String::from_utf8_lossy(&output.stdout),
                                "stderr": String::from_utf8_lossy(&output.stderr),
                                "status": output.status.code().unwrap_or(-1)
                            })),
                            Err(e) => Err(format!("Execution failed: {}", e))
                        },
                        Err(_) => Err(format!("Command timed out after {}ms", ms))
                    }
                })
            } else { Err("Context lost".into()) }
        });
        
        return_json_promise(ctx, result)
    });

    // $cmd.spawn(program, args, options) -> Promise<{ pid }> (Background)
    let spawn_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let program = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let args_val = args.get_or_undefined(1);
        let options_val = args.get_or_undefined(2);
        
        let (cwd, envs, timeout_val) = parse_options(ctx, options_val);
        let max_time = timeout_val.unwrap_or(60_000);

        let mut cmd_args: Vec<String> = Vec::new();
        if let Ok(Some(json_val)) = args_val.to_json(ctx) {
            if let Some(arr) = json_val.as_array() {
                for v in arr {
                    if let Some(s) = v.as_str() { cmd_args.push(s.to_string()); }
                    else { cmd_args.push(v.to_string()); }
                }
            }
        }

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                if !matches!(scope, EventScope::Root) {
                    return Err("Access Denied.".into());
                }

                handle.block_on(async {
                     let sem = get_semaphore_for_program(&program);
                     let permit = sem.clone().acquire_owned().await.map_err(|e| e.to_string())?;

                     let mut command = tokio::process::Command::new(&program);
                     command.args(&cmd_args);
                     command.stdout(Stdio::piped());
                     command.stderr(Stdio::piped());
                     command.stdin(Stdio::null());
                     command.kill_on_drop(true); 
                     
                     if let Some(dir) = cwd { command.current_dir(dir); }
                     if let Some(vars) = envs { command.envs(vars); }

                     match command.spawn() {
                         Ok(mut child) => {
                             let id = child.id().unwrap_or(0);
                             
                             {
                                 let mut tracker = get_tracker().lock().unwrap();
                                 tracker.insert(id, ProcessInfo {
                                     pid: id,
                                     program: program.clone(),
                                     start_time: Instant::now(),
                                     status: "running".to_string(),
                                     exit_code: None,
                                 });
                             }

                             tokio::spawn(async move {
                                 let _permit_guard = permit;
                                 let wait_future = child.wait();
                                 let result = timeout(Duration::from_millis(max_time), wait_future).await;
                                 
                                 let mut tracker = get_tracker().lock().unwrap();
                                 if let Some(info) = tracker.get_mut(&id) {
                                     match result {
                                         Ok(Ok(status)) => {
                                             info.status = "completed".to_string();
                                             info.exit_code = status.code();
                                         },
                                         Ok(Err(_)) => { info.status = "failed".to_string(); },
                                         Err(_) => { info.status = "timed_out".to_string(); }
                                     }
                                 }
                             });

                             Ok(json!({ "pid": id, "status": "running" }))
                         },
                         Err(e) => Err(format!("Spawn failed: {}", e))
                     }
                })
            } else { Err("Context lost".into()) }
        });

        return_json_promise(ctx, result)
    });

    // $cmd.status(pid)
    let status_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let pid = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0) as u32;

        let result = ACTIVE_CONTEXT.with(|c| {
             if let Some((_, _, _, _, scope)) = &*c.borrow() {
                 if !matches!(scope, EventScope::Root) { return Err("Access Denied.".into()); }

                 let tracker = get_tracker().lock().unwrap();
                 if let Some(info) = tracker.get(&pid) {
                     Ok(json!({
                         "pid": info.pid,
                         "program": info.program,
                         "status": info.status,
                         "exit_code": info.exit_code,
                         "runtime_ms": info.start_time.elapsed().as_millis() as u64
                     }))
                 } else {
                     Ok(json!({ "status": "unknown" }))
                 }
             } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result)
    });

    let obj = ObjectInitializer::new(ctx)
        .function(run_fn, JsString::from("run"), 3)
        .function(spawn_fn, JsString::from("spawn"), 3)
        .function(status_fn, JsString::from("status"), 1)
        .function(set_limit_fn, JsString::from("setLimit"), 2)
        .build();

    ctx.register_global_property(JsString::from("$cmd"), obj, Attribute::all()).map_err(|e| e.to_string())
}