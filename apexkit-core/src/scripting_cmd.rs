use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::timeout;

use crate::realtime::{DbEvent, EventScope};
use crate::scripting::{ACTIVE_CONTEXT, return_json_promise};
use boa_engine::{
    Context, JsArgs, JsString, NativeFunction, object::ObjectInitializer, property::Attribute,
};
use regex::Regex;
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::io::{AsyncBufReadExt, BufReader};

// --- GLOBAL PROCESS TRACKER ---
struct ProcessInfo {
    pid: u32,
    program: String,
    start_time: Instant,
    status: String,
    exit_code: Option<i32>,
}

static PROCESS_TRACKER: OnceLock<Mutex<HashMap<u32, ProcessInfo>>> = OnceLock::new();
static SEMAPHORE_MAP: OnceLock<RwLock<HashMap<String, Arc<Semaphore>>>> = OnceLock::new();
// [NEW] Use a high starting number for internal job tracking
static JOB_COUNTER: AtomicU32 = AtomicU32::new(100000);

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
    drop(map);

    let mut write_map = get_semaphores().write().unwrap();
    if let Some(sem) = write_map.get(program) {
        return sem.clone();
    }

    if !write_map.contains_key("*") {
        write_map.insert("*".to_string(), Arc::new(Semaphore::new(2)));
    }

    write_map.get("*").unwrap().clone()
}

pub fn register_cmd(ctx: &mut Context) -> Result<(), String> {
    let parse_options = |ctx: &mut Context,
                         val: &boa_engine::JsValue|
     -> (Option<String>, Option<HashMap<String, String>>, Option<u64>) {
        let mut cwd = None;
        let mut envs = None;
        let mut timeout_ms = None;

        if let Ok(Some(json)) = val.to_json(ctx)
            && let Some(obj) = json.as_object()
        {
            cwd = obj
                .get("cwd")
                .and_then(|v: &serde_json::Value| v.as_str().map(|s| s.to_string()));
            timeout_ms = obj
                .get("timeout")
                .and_then(|v: &serde_json::Value| v.as_u64());

            if let Some(e) = obj
                .get("env")
                .and_then(|v: &serde_json::Value| v.as_object())
            {
                let mut map = HashMap::new();
                for (k, v) in e {
                    let val_str = if let Some(s) = v.as_str() {
                        s.to_string()
                    } else {
                        v.to_string()
                    };
                    map.insert(k.clone(), val_str);
                }
                envs = Some(map);
            }
        }
        (cwd, envs, timeout_ms)
    };

    // $cmd.setLimit(program, limit)
    let set_limit_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let program = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let limit = args.get_or_undefined(1).to_number(ctx).unwrap_or(2.0) as usize;

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, _, _, _, scope)) = &*c.borrow() {
                if !matches!(scope, EventScope::Root) {
                    return Err("Access Denied.".into());
                }

                let mut map = get_semaphores().write().unwrap();
                map.insert(program.clone(), Arc::new(Semaphore::new(limit)));

                Ok(json!({ "program": program, "limit": limit, "set": true }))
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // $cmd.run(program, args, options) -> Promise (Waiting)
    let run_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let program = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let args_val = args.get_or_undefined(1);
        let options_val = args.get_or_undefined(2);

        let (cwd, envs, timeout_ms) = parse_options(ctx, options_val);
        let mut cmd_args: Vec<String> = Vec::new();

        if let Ok(Some(json_val)) = args_val.to_json(ctx)
            && let Some(arr) = json_val.as_array()
        {
            for v in arr {
                if let Some(s) = v.as_str() {
                    cmd_args.push(s.to_string());
                } else {
                    cmd_args.push(v.to_string());
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

                    if let Some(dir) = cwd {
                        command.current_dir(dir);
                    }
                    if let Some(vars) = envs {
                        command.envs(vars);
                    }

                    let ms = timeout_ms.unwrap_or(30_000);
                    let future = command.output();

                    match timeout(std::time::Duration::from_millis(ms), future).await {
                        Ok(res) => match res {
                            Ok(output) => Ok(json!({
                                "stdout": String::from_utf8_lossy(&output.stdout),
                                "stderr": String::from_utf8_lossy(&output.stderr),
                                "status": output.status.code().unwrap_or(-1)
                            })),
                            Err(e) => Err(format!("Execution failed: {}", e)),
                        },
                        Err(_) => Err(format!("Command timed out after {}ms", ms)),
                    }
                })
            } else {
                Err("Context lost".into())
            }
        });

        return_json_promise(ctx, result)
    });

    // $cmd.spawn(program, args, options) -> Promise<{ pid }> (Background)
    let spawn_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let program = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let args_val = args.get_or_undefined(1);
        let options_val = args.get_or_undefined(2);

        let (cwd, envs, timeout_val) = parse_options(ctx, options_val);
        let max_time = timeout_val.unwrap_or(3_600_000); // Default 1 hour

        let mut cmd_args: Vec<String> = Vec::new();
        if let Ok(Some(json_val)) = args_val.to_json(ctx)
            && let Some(arr) = json_val.as_array()
        {
            for v in arr {
                if let Some(s) = v.as_str() {
                    cmd_args.push(s.to_string());
                } else {
                    cmd_args.push(v.to_string());
                }
            }
        }

        let mut progress_config = None;
        if let Ok(Some(json)) = options_val.to_json(ctx)
            && let Some(obj) = json.as_object()
            && let Some(prog_obj) = obj.get("onProgress").and_then(|v| v.as_object())
        {
            let regex_str = prog_obj
                .get("regex")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let channel = prog_obj
                .get("channel")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let event_name = prog_obj
                .get("event")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if let (Some(r), Some(c), Some(e)) = (regex_str, channel, event_name) {
                progress_config = Some((r, c, e));
            }
        }

        let internal_id = JOB_COUNTER.fetch_add(1, Ordering::SeqCst);

        {
            let mut tracker = get_tracker().lock().unwrap();
            tracker.insert(
                internal_id,
                ProcessInfo {
                    pid: 0,
                    program: program.clone(),
                    start_time: Instant::now(),
                    status: "queued".to_string(),
                    exit_code: None,
                },
            );
        }

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, tx_opt, scope)) = &*c.borrow() {
                if !matches!(scope, EventScope::Root) {
                    return Err("Access Denied.".into());
                }

                let tx = tx_opt.clone();
                let client_scope = scope.clone();
                let program_clone = program.clone();

                // [CRITICAL FIX]: Acquire semaphore INSIDE the spawned task!
                handle.spawn(async move {
                    let sem = get_semaphore_for_program(&program_clone);

                    // The task pauses here if limit is reached, but the API request already returned!
                    let permit = match sem.clone().acquire_owned().await {
                        Ok(p) => p,
                        Err(e) => {
                            let mut tracker = get_tracker().lock().unwrap();
                            if let Some(info) = tracker.get_mut(&internal_id) {
                                info.status = format!("failed_sem: {}", e);
                            }
                            return;
                        }
                    };

                    {
                        let mut tracker = get_tracker().lock().unwrap();
                        if let Some(info) = tracker.get_mut(&internal_id) {
                            info.status = "running".to_string();
                            info.start_time = Instant::now();
                        }
                    }

                    let mut command = tokio::process::Command::new(&program_clone);
                    command.args(&cmd_args);
                    command.stdout(Stdio::piped());
                    command.stderr(Stdio::piped());
                    command.stdin(Stdio::null());
                    command.kill_on_drop(true);

                    if let Some(dir) = cwd {
                        command.current_dir(dir);
                    }
                    if let Some(vars) = envs {
                        command.envs(vars);
                    }

                    match command.spawn() {
                        Ok(mut child) => {
                            let os_pid = child.id().unwrap_or(0);
                            {
                                let mut tracker = get_tracker().lock().unwrap();
                                if let Some(info) = tracker.get_mut(&internal_id) {
                                    info.pid = os_pid;
                                }
                            }

                            if let Some((regex_pattern, channel_name, event_name)) = progress_config
                                && let Some(stderr) = child.stderr.take()
                                && let Some(broadcaster) = tx
                            {
                                let reader = BufReader::new(stderr);
                                let scoped_channel = if channel_name.contains("::") {
                                    channel_name.clone()
                                } else {
                                    match &client_scope {
                                        EventScope::Root => format!("root::{}", channel_name),
                                        EventScope::Tenant(id) => {
                                            format!("tenant_{}::{}", id, channel_name)
                                        }
                                        EventScope::Sandbox(id) => {
                                            format!("sandbox_{}::{}", id, channel_name)
                                        }
                                        _ => channel_name.clone(),
                                    }
                                };

                                tokio::spawn(async move {
                                    if let Ok(re) = Regex::new(&regex_pattern) {
                                        let mut lines = reader.lines();
                                        while let Ok(Some(line)) = lines.next_line().await {
                                            if let Some(caps) = re.captures(&line) {
                                                let val =
                                                    caps.get(1).map(|m| m.as_str()).unwrap_or(
                                                        caps.get(0)
                                                            .map(|m| m.as_str())
                                                            .unwrap_or(""),
                                                    );
                                                let event = DbEvent::Custom {
                                                    event: event_name.clone(),
                                                    data: json!({ "value": val, "raw": line }),
                                                    scope: EventScope::Channel(
                                                        scoped_channel.clone(),
                                                    ),
                                                };
                                                let _ = broadcaster.send(event);
                                            }
                                        }
                                    }
                                });
                            }

                            let _permit_guard = permit;
                            let wait_future = child.wait();
                            let result =
                                timeout(Duration::from_millis(max_time), wait_future).await;

                            let mut tracker = get_tracker().lock().unwrap();
                            if let Some(info) = tracker.get_mut(&internal_id) {
                                match result {
                                    Ok(Ok(status)) => {
                                        info.status = "completed".to_string();
                                        info.exit_code = status.code();
                                    }
                                    Ok(Err(_)) => {
                                        info.status = "failed".to_string();
                                    }
                                    Err(_) => {
                                        info.status = "timed_out".to_string();
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let mut tracker = get_tracker().lock().unwrap();
                            if let Some(info) = tracker.get_mut(&internal_id) {
                                info.status = format!("spawn_failed: {}", e);
                            }
                        }
                    }
                });

                // Return immediately
                Ok(json!({ "pid": internal_id, "status": "queued" }))
            } else {
                Err("Context lost".into())
            }
        });

        return_json_promise(ctx, result)
    });

    // $cmd.status(pid)
    let status_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let pid = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0) as u32;

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, _, _, _, scope)) = &*c.borrow() {
                if !matches!(scope, EventScope::Root) {
                    return Err("Access Denied.".into());
                }

                let tracker = get_tracker().lock().unwrap();
                if let Some(info) = tracker.get(&pid) {
                    Ok(json!({
                        "pid": info.pid, // Real OS pid if running, 0 if queued
                        "job_id": pid,   // The internal tracking ID
                        "program": info.program,
                        "status": info.status,
                        "exit_code": info.exit_code,
                        "runtime_ms": info.start_time.elapsed().as_millis() as u64
                    }))
                } else {
                    Ok(json!({ "status": "unknown", "job_id": pid }))
                }
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // $cmd.kill(pid)
    let kill_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let internal_pid = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0) as u32;

        let result = ACTIVE_CONTEXT.with(|c| {
             if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                 if !matches!(scope, EventScope::Root) { return Err("Access Denied.".into()); }

                 let os_pid = {
                     let tracker = get_tracker().lock().unwrap();
                     tracker.get(&internal_pid).map(|i| i.pid).unwrap_or(0)
                 };

                 if os_pid == 0 {
                     return Ok(json!({ "killed": false, "error": "Process not running or unkillable" }));
                 }

                 handle.block_on(async {
                     #[cfg(unix)]
                     {
                         use std::process::Command;
                         let output = Command::new("kill").arg("-9").arg(os_pid.to_string()).output();
                         match output {
                             Ok(o) if o.status.success() => Ok(json!({ "killed": true, "job_id": internal_pid })),
                             _ => Ok(json!({ "killed": false, "job_id": internal_pid, "error": "Process not found or access denied" }))
                         }
                     }
                     #[cfg(not(unix))]
                     {
                         Ok(json!({ "killed": false, "error": "Kill not implemented for this platform" }))
                     }
                 })
             } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result)
    });

    let obj = ObjectInitializer::new(ctx)
        .function(run_fn, JsString::from("run"), 3)
        .function(spawn_fn, JsString::from("spawn"), 3)
        .function(status_fn, JsString::from("status"), 1)
        .function(set_limit_fn, JsString::from("setLimit"), 2)
        .function(kill_fn, JsString::from("kill"), 1)
        .build();

    ctx.register_global_property(JsString::from("$cmd"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
