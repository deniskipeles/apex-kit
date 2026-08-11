use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use regex::Regex;
use rquickjs::function::Async;
use rquickjs::{Ctx, Function, Object, Value};
use rquickjs_serde::{from_value, to_value};
use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Semaphore;
use tokio::time::timeout;

use super::super::context::ScriptContext;
use crate::realtime::{DbEvent, EventScope};

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

#[derive(Deserialize, Default)]
struct CmdOptions {
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    timeout: Option<u64>,
    #[serde(rename = "onProgress")]
    on_progress: Option<ProgressConfig>,
}

#[derive(Deserialize)]
struct ProgressConfig {
    regex: Option<String>,
    channel: Option<String>,
    event: Option<String>,
}

pub fn register_cmd<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let cmd_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // 1. $cmd.setLimit(program, limit)
    let app_limit = app_ctx.clone();
    let set_limit_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, program: String, limit: u32| {
            let app = app_limit.clone();
            async move {
                if !matches!(app.get_scope(), EventScope::Root) {
                    return Err(rquickjs::Error::Exception);
                }

                let mut map = get_semaphores().write().unwrap();
                map.insert(program.clone(), Arc::new(Semaphore::new(limit as usize)));

                let res = json!({ "program": program, "limit": limit, "set": true });
                to_value(js_ctx, &res).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // 2. $cmd.run(program, args, options) -> Promise (Waiting)
    let app_run = app_ctx.clone();
    let run_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>,
                  program: String,
                  args_val: Value<'js>,
                  opts_val: Option<Value<'js>>| {
                let cmd_args: Vec<String> = from_value(args_val).unwrap_or_default();
                let opts: CmdOptions = opts_val
                    .and_then(|v| from_value(v).ok())
                    .unwrap_or_default();
                let app = app_run.clone();

                async move {
                    if !matches!(app.get_scope(), EventScope::Root) {
                        return Err(rquickjs::Error::Exception);
                    }

                    let sem = get_semaphore_for_program(&program);
                    let _permit = sem
                        .acquire()
                        .await
                        .map_err(|_| rquickjs::Error::Exception)?;

                    let mut command = tokio::process::Command::new(&program);
                    command.args(&cmd_args);
                    command.stdout(Stdio::piped());
                    command.stderr(Stdio::piped());

                    if let Some(dir) = opts.cwd {
                        command.current_dir(dir);
                    }
                    if let Some(vars) = opts.env {
                        command.envs(vars);
                    }

                    let ms = opts.timeout.unwrap_or(30_000);
                    let future = command.output();

                    let val = match timeout(Duration::from_millis(ms), future).await {
                        Ok(res) => match res {
                            Ok(output) => json!({
                                "stdout": String::from_utf8_lossy(&output.stdout),
                                "stderr": String::from_utf8_lossy(&output.stderr),
                                "status": output.status.code().unwrap_or(-1)
                            }),
                            Err(_) => return Err(rquickjs::Error::Exception),
                        },
                        Err(_) => return Err(rquickjs::Error::Exception),
                    };

                    to_value(js_ctx, &val).map_err(|_| rquickjs::Error::Exception)
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // 3. $cmd.spawn(program, args, options) -> Promise<{ pid }> (Background)
    let app_spawn = app_ctx.clone();
    let spawn_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>,
                  program: String,
                  args_val: Value<'js>,
                  opts_val: Option<Value<'js>>| {
                let cmd_args: Vec<String> = from_value(args_val).unwrap_or_default();
                let opts: CmdOptions = opts_val
                    .and_then(|v| from_value(v).ok())
                    .unwrap_or_default();
                let app = app_spawn.clone();

                async move {
                    if !matches!(app.get_scope(), EventScope::Root) {
                        return Err(rquickjs::Error::Exception);
                    }

                    let max_time = opts.timeout.unwrap_or(3_600_000); // Default 1 hour
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

                    let client_scope = app.get_scope();
                    let tx = Some(app.get_realtime_tx());
                    let program_clone = program.clone();

                    tokio::spawn(async move {
                        let sem = get_semaphore_for_program(&program_clone);

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

                        if let Some(dir) = opts.cwd {
                            command.current_dir(dir);
                        }
                        if let Some(vars) = opts.env {
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

                                if let Some(prog) = opts.on_progress
                                    && let (
                                        Some(regex_pattern),
                                        Some(channel_name),
                                        Some(event_name),
                                    ) = (prog.regex, prog.channel, prog.event)
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

                    let val = json!({ "pid": internal_id, "status": "queued" });
                    to_value(js_ctx, &val).map_err(|_| rquickjs::Error::Exception)
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // 4. $cmd.status(pid)
    let app_status = app_ctx.clone();
    let status_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, pid: u32| {
            let app = app_status.clone();
            async move {
                if !matches!(app.get_scope(), EventScope::Root) {
                    return Err(rquickjs::Error::Exception);
                }

                let tracker = get_tracker().lock().unwrap();
                let val = if let Some(info) = tracker.get(&pid) {
                    json!({
                        "pid": info.pid,
                        "job_id": pid,
                        "program": info.program,
                        "status": info.status,
                        "exit_code": info.exit_code,
                        "runtime_ms": info.start_time.elapsed().as_millis() as u64
                    })
                } else {
                    json!({ "status": "unknown", "job_id": pid })
                };

                to_value(js_ctx, &val).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // 5. $cmd.kill(pid)
    let app_kill = app_ctx.clone();
    let kill_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, internal_pid: u32| {
            let app = app_kill.clone();
            async move {
                if !matches!(app.get_scope(), EventScope::Root) {
                    return Err(rquickjs::Error::Exception);
                }

                let os_pid = {
                    let tracker = get_tracker().lock().unwrap();
                    tracker.get(&internal_pid).map(|i| i.pid).unwrap_or(0)
                };

                if os_pid == 0 {
                    let val = json!({ "killed": false, "error": "Process not running or unkillable" });
                    return to_value(js_ctx, &val).map_err(|_| rquickjs::Error::Exception);
                }

                #[cfg(unix)]
                {
                    use std::process::Command;
                    let output = Command::new("kill").arg("-9").arg(os_pid.to_string()).output();
                    let val = match output {
                        Ok(o) if o.status.success() => json!({ "killed": true, "job_id": internal_pid }),
                        _ => json!({ "killed": false, "job_id": internal_pid, "error": "Process not found or access denied" })
                    };
                    to_value(js_ctx, &val).map_err(|_| rquickjs::Error::Exception)
                }
                #[cfg(not(unix))]
                {
                    let val = json!({ "killed": false, "error": "Kill not implemented for this platform" });
                    to_value(js_ctx, &val).map_err(|_| rquickjs::Error::Exception)
                }
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    cmd_obj.set("run", run_fn).map_err(|e| e.to_string())?;
    cmd_obj.set("spawn", spawn_fn).map_err(|e| e.to_string())?;
    cmd_obj
        .set("status", status_fn)
        .map_err(|e| e.to_string())?;
    cmd_obj
        .set("setLimit", set_limit_fn)
        .map_err(|e| e.to_string())?;
    cmd_obj.set("kill", kill_fn).map_err(|e| e.to_string())?;

    globals.set("$cmd", cmd_obj).map_err(|e| e.to_string())?;
    Ok(())
}
