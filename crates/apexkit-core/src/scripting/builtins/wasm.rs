use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ring::digest::{SHA256, digest};
use rquickjs::function::Async;
use rquickjs::{Ctx, Function, Object, Value};
use rquickjs_serde::{from_value, to_value};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use wasmtime::*;
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::preview1::{self, WasiP1Ctx};

use super::super::context::ScriptContext;

fn get_wasm_cache_dir() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(".cache").join("wasm");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn get_or_compile_module(engine: &Engine, bytes: &[u8]) -> Result<Module, String> {
    let hash = crate::utils::to_hex(digest(&SHA256, bytes).as_ref());
    let cache_dir = get_wasm_cache_dir();
    let wasm_file = cache_dir.join(format!("{}.wasm", hash));
    let cwasm_file = cache_dir.join(format!("{}.cwasm", hash));

    // 1. Instant path: load precompiled module from disk cache
    if cwasm_file.exists() {
        if let Ok(module) = unsafe { Module::deserialize_file(engine, &cwasm_file) } {
            return Ok(module);
        }
    }

    // 2. Save raw .wasm to cache directory on first fetch
    if !wasm_file.exists() {
        let _ = fs::write(&wasm_file, bytes);
    }

    // 3. Compile WASM binary
    let module = Module::from_binary(engine, bytes).map_err(|e| e.to_string())?;

    // 4. Save precompiled .cwasm artifact for 0ms future reloads
    if let Ok(serialized) = module.serialize() {
        let _ = fs::write(&cwasm_file, serialized);
    }

    Ok(module)
}

pub fn register_wasm<'js>(ctx: &Ctx<'js>, _app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let wasm_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // 1. $wasm.call(b64, func, args)
    let call_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>,
                  b64: String,
                  func: String,
                  args_val: Value<'js>| async move {
                let args: Vec<f64> = from_value(args_val).unwrap_or_default();

                let res = tokio::task::spawn_blocking(move || {
                    let bytes = BASE64.decode(&b64).map_err(|e| e.to_string())?;
                    let engine = Engine::default();
                    let module = get_or_compile_module(&engine, &bytes)?;

                    let mut store = Store::new(&engine, ());
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
                })
                .await
                .map_err(|_| rquickjs::Error::Exception)?
                .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &res).map_err(|_| rquickjs::Error::Exception)
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // 2. $wasm.runWasi(b64, cli_args)
    let run_wasi_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, b64: String, cli_args_val: Option<Value<'js>>| async move {
                let cli_args: Option<Vec<String>> = cli_args_val.and_then(|v| from_value(v).ok());

                let res = tokio::task::spawn_blocking(move || {
                    let bytes = BASE64.decode(&b64).map_err(|e| e.to_string())?;
                    let engine = Engine::default();
                    let module = get_or_compile_module(&engine, &bytes)?;

                    let mut linker: Linker<WasiP1Ctx> = Linker::new(&engine);
                    preview1::add_to_linker_sync(&mut linker, |t| t)
                        .map_err(|e| e.to_string())?;

                    let mut builder = WasiCtxBuilder::new();
                    builder.inherit_stdio();
                    if let Some(args) = cli_args {
                        for arg in args {
                            builder.arg(&arg);
                        }
                    }

                    let wasi_ctx = builder.build_p1();
                    let mut store = Store::new(&engine, wasi_ctx);

                    let instance = linker
                        .instantiate(&mut store, &module)
                        .map_err(|e| e.to_string())?;
                    let func = instance
                        .get_typed_func::<(), ()>(&mut store, "_start")
                        .map_err(|e| e.to_string())?;

                    func.call(&mut store, ()).map_err(|e| e.to_string())?;
                    Ok::<bool, String>(true)
                })
                .await
                .map_err(|_| rquickjs::Error::Exception)?
                .map_err(|_| rquickjs::Error::Exception)?;

                to_value(js_ctx, &res).map_err(|_| rquickjs::Error::Exception)
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