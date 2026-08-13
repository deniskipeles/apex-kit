use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ring::digest::{SHA256, digest};
use rquickjs::function::{Async, Opt};
use rquickjs::{Ctx, Function, Object, Value, Exception};
use rquickjs_serde::{from_value, to_value};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use wasmtime::*;
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::preview1::{self, WasiP1Ctx};

use super::super::context::ScriptContext;
use crate::realtime::EventScope;

#[derive(Deserialize, Default)]
struct WasmOptions {
    name: Option<String>,
    #[serde(rename = "memoryMb")]
    memory_mb: Option<usize>,
    #[serde(rename = "timeoutMs")]
    timeout_ms: Option<u64>,
}

struct CustomResourceLimiter {
    max_memory_bytes: usize,
}

impl ResourceLimiter for CustomResourceLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        Ok(desired <= self.max_memory_bytes)
    }

    fn table_growing(
        &mut self,
        _current: u32,
        desired: u32,
        _maximum: Option<u32>,
    ) -> Result<bool, wasmtime::Error> {
        Ok(desired <= 10_000)
    }
}

struct WasmStoreState {
    wasi: Option<WasiP1Ctx>,
    limiter: CustomResourceLimiter,
}

fn get_wasm_cache_dir() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(".cache").join("wasm");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn create_readable_symlink(cache_dir: &PathBuf, readable_name: &str, target_filename: &str) {
    let clean_name = readable_name.trim_start_matches('/').trim_start_matches("./");
    if clean_name.is_empty() {
        return;
    }

    let link_path = cache_dir.join(clean_name);
    if let Some(parent) = link_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let target_path = cache_dir.join(target_filename);

    // Remove existing link/file if present
    let _ = fs::remove_file(&link_path);

    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(&target_path, &link_path);
    }
    #[cfg(windows)]
    {
        let _ = std::os::windows::fs::symlink_file(&target_path, &link_path);
    }
}

fn resolve_wasm_bytes(
    app_ctx: &Arc<dyn ScriptContext>,
    input: &str,
    readable_name_opt: Option<&str>,
    is_root: bool,
) -> Result<(Vec<u8>, Option<String>), String> {
    let cache_dir = get_wasm_cache_dir();
    let clean_input = input.trim_start_matches('/').trim_start_matches("./");

    // 1. Check if `input` references an existing host WASM file in .cache/wasm/
    // Only check if it's reasonably sized to avoid OS File Name Too Long errors
    if clean_input.len() < 256 {
        let cached_file = cache_dir.join(clean_input);
        if cached_file.exists() && cached_file.is_file() {
            let bytes = fs::read(&cached_file).map_err(|e| e.to_string())?;
            return Ok((bytes, Some(clean_input.to_string())));
        }
    }

    // 2. Decode as Base64 payload
    let bytes = match BASE64.decode(input.trim()) {
        Ok(b) if !b.is_empty() => b,
        _ => {
            // Not valid Base64: Fallback to reading file from host/tenant storage
            let storage = app_ctx.get_storage();
            let handle = tokio::runtime::Handle::current();
            let input_owned = input.to_string();
            match std::thread::spawn(move || handle.block_on(async { storage.get(&input_owned).await }))
                .join()
                .unwrap_or(Err("Read failed".into()))
            {
                Ok(b) => b,
                Err(_) => return Err(format!("Invalid WASM input: File '{}' not found in cache or storage.", input)),
            }
        }
    };

    // 3. Enforce 200 KB size limit for custom non-root tenant WASM uploads
    if !is_root && bytes.len() > 200 * 1024 {
        return Err(format!(
            "Tenant WASM upload size ({} KB) exceeds allowed limit (200 KB max). Use host-provided WASM tools or upgrade subscription.",
            bytes.len() / 1024
        ));
    }

    let name = readable_name_opt.map(|s| s.to_string());
    Ok((bytes, name))
}

fn get_or_compile_module(
    engine: &Engine,
    bytes: &[u8],
    readable_name: Option<&str>,
) -> Result<Module, String> {
    let hash = crate::utils::to_hex(digest(&SHA256, bytes).as_ref());
    let cache_dir = get_wasm_cache_dir();
    let wasm_file = cache_dir.join(format!("{}.wasm", hash));
    let cwasm_file = cache_dir.join(format!("{}.cwasm", hash));

    if let Some(rname) = readable_name {
        create_readable_symlink(&cache_dir, rname, &format!("{}.cwasm", hash));
    }

    if cwasm_file.exists() {
        if let Ok(module) = unsafe { Module::deserialize_file(engine, &cwasm_file) } {
            return Ok(module);
        }
    }

    if !wasm_file.exists() {
        let _ = fs::write(&wasm_file, bytes);
    }

    let module = Module::from_binary(engine, bytes).map_err(|e| e.to_string())?;

    if let Ok(serialized) = module.serialize() {
        let _ = fs::write(&cwasm_file, serialized);
    }

    Ok(module)
}

pub fn register_wasm<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let wasm_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // 1. $wasm.call(b64OrName, func, args, options?)
    let app_call = app_ctx.clone();
    let call_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>,
                  input: String,
                  func: String,
                  args_val: Value<'js>,
                  Opt(opts_val): Opt<Value<'js>>| {
                let app = app_call.clone();
                async move {
                    let args: Vec<f64> = from_value(args_val).unwrap_or_default();
                    let opts: WasmOptions = opts_val
                        .and_then(|v| from_value(v).ok())
                        .unwrap_or_default();

                    let is_root = matches!(app.get_scope(), EventScope::Root);

                    let memory_limit_mb = opts.memory_mb.unwrap_or(if is_root { 512 } else { 64 });
                    let timeout_ms = opts.timeout_ms.unwrap_or(if is_root { 30_000 } else { 300 });

                    let (bytes, readable_name) = match resolve_wasm_bytes(
                        &app,
                        &input,
                        opts.name.as_deref(),
                        is_root,
                    ) {
                        Ok(res) => res,
                        Err(e) => {
                            let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                    };

                    let task = tokio::task::spawn_blocking(move || {
                        let mut config = Config::new();
                        config.consume_fuel(true);

                        let engine = Engine::new(&config).map_err(|e| e.to_string())?;
                        let module = get_or_compile_module(&engine, &bytes, readable_name.as_deref())?;

                        let state = WasmStoreState {
                            wasi: None,
                            limiter: CustomResourceLimiter {
                                max_memory_bytes: memory_limit_mb * 1024 * 1024,
                            },
                        };

                        let mut store = Store::new(&engine, state);
                        store.limiter(|s| &mut s.limiter);
                        let _ = store.set_fuel(100_000_000);

                        let instance =
                            Instance::new(&mut store, &module, &[]).map_err(|e| e.to_string())?;

                        let export = instance
                            .get_func(&mut store, &func)
                            .ok_or_else(|| format!("Function '{}' not found in WASM exports", func))?;

                        let wasm_args: Vec<Val> =
                            args.iter().map(|&a| Val::F64(a.to_bits())).collect();
                        let mut results = vec![Val::I32(0)];

                        export
                            .call(&mut store, &wasm_args, &mut results)
                            .map_err(|e| e.to_string())?;

                        let ret_val = match results.first() {
                            Some(Val::I32(i)) => *i as f64,
                            Some(Val::I64(i)) => *i as f64,
                            Some(Val::F32(f)) => f32::from_bits(*f) as f64,
                            Some(Val::F64(f)) => f64::from_bits(*f),
                            _ => 0.0,
                        };

                        Ok::<f64, String>(ret_val)
                    });

                    let res = match timeout(Duration::from_millis(timeout_ms), task).await {
                        Ok(Ok(Ok(val))) => val,
                        Ok(Ok(Err(e))) => {
                            let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                        Ok(Err(e)) => {
                            let js_err = Exception::from_message(js_ctx.clone(), &format!("Task panicked: {}", e)).unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                        Err(_) => {
                            let js_err = Exception::from_message(js_ctx.clone(), "WASM execution timed out").unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                    };

                    to_value(js_ctx.clone(), &res).map_err(|_| {
                        let js_err = Exception::from_message(js_ctx.clone(), "Failed to serialize result").unwrap();
                        js_ctx.throw(js_err.into())
                    })
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // 2. $wasm.runWasi(b64OrName, cliArgs?, options?)
    let app_wasi = app_ctx.clone();
    let run_wasi_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>,
                  input: String,
                  Opt(cli_args_val): Opt<Value<'js>>,
                  Opt(opts_val): Opt<Value<'js>>| {
                let app = app_wasi.clone();
                async move {
                    let cli_args: Option<Vec<String>> = cli_args_val.and_then(|v| from_value(v).ok());
                    let opts: WasmOptions = opts_val
                        .and_then(|v| from_value(v).ok())
                        .unwrap_or_default();

                    let is_root = matches!(app.get_scope(), EventScope::Root);

                    let memory_limit_mb = opts.memory_mb.unwrap_or(if is_root { 512 } else { 64 });
                    let timeout_ms = opts.timeout_ms.unwrap_or(if is_root { 30_000 } else { 300 });

                    let (bytes, readable_name) = match resolve_wasm_bytes(
                        &app,
                        &input,
                        opts.name.as_deref(),
                        is_root,
                    ) {
                        Ok(res) => res,
                        Err(e) => {
                            let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                    };

                    let task = tokio::task::spawn_blocking(move || {
                        let mut config = Config::new();
                        config.consume_fuel(true);

                        let engine = Engine::new(&config).map_err(|e| e.to_string())?;
                        let module = get_or_compile_module(&engine, &bytes, readable_name.as_deref())?;

                        let mut linker: Linker<WasmStoreState> = Linker::new(&engine);
                        preview1::add_to_linker_sync(&mut linker, |s| s.wasi.as_mut().unwrap())
                            .map_err(|e| e.to_string())?;

                        let mut builder = WasiCtxBuilder::new();
                        builder.inherit_stdio();
                        if let Some(args) = cli_args {
                            for arg in args {
                                builder.arg(&arg);
                            }
                        }

                        let wasi_ctx = builder.build_p1();
                        let state = WasmStoreState {
                            wasi: Some(wasi_ctx),
                            limiter: CustomResourceLimiter {
                                max_memory_bytes: memory_limit_mb * 1024 * 1024,
                            },
                        };

                        let mut store = Store::new(&engine, state);
                        store.limiter(|s| &mut s.limiter);
                        let _ = store.set_fuel(100_000_000);

                        let instance = linker
                            .instantiate(&mut store, &module)
                            .map_err(|e| e.to_string())?;
                        let func = instance
                            .get_typed_func::<(), ()>(&mut store, "_start")
                            .map_err(|e| e.to_string())?;

                        func.call(&mut store, ()).map_err(|e| e.to_string())?;
                        Ok::<bool, String>(true)
                    });

                    let res = match timeout(Duration::from_millis(timeout_ms), task).await {
                        Ok(Ok(Ok(val))) => val,
                        Ok(Ok(Err(e))) => {
                            let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                        Ok(Err(e)) => {
                            let js_err = Exception::from_message(js_ctx.clone(), &format!("Task panicked: {}", e)).unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                        Err(_) => {
                            let js_err = Exception::from_message(js_ctx.clone(), "WASM execution timed out").unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                    };

                    to_value(js_ctx.clone(), &res).map_err(|_| {
                        let js_err = Exception::from_message(js_ctx.clone(), "Failed to serialize result").unwrap();
                        js_ctx.throw(js_err.into())
                    })
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    wasm_obj.set("call", call_fn).map_err(|e| e.to_string())?;
    wasm_obj
        .set("runWasi", run_wasi_fn)
        .map_err(|e| e.to_string())?;

    globals.set("$wasm", wasm_obj).map_err(|e| e.to_string())?;
    Ok(())
}