use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ring::digest::{SHA256, digest};
use rquickjs::function::{Async, Opt};
use rquickjs::{Ctx, Exception, Function, Object, Value};
use rquickjs_serde::{from_value, to_value};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::time::timeout;
use wasmtime::*;
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::preview1::{self, WasiP1Ctx};

use super::super::context::ScriptContext;
use crate::realtime::EventScope;

type WasmRegistry = Arc<Mutex<HashMap<String, (Arc<Mutex<Store<WasmStoreState>>>, Arc<Instance>)>>>;

static INSTANCE_REGISTRY: OnceLock<WasmRegistry> = OnceLock::new();

fn get_instance_registry() -> &'static WasmRegistry {
    INSTANCE_REGISTRY.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

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

pub struct WasmStoreState {
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
    let clean_name = readable_name
        .trim_start_matches('/')
        .trim_start_matches("./");
    if clean_name.is_empty() {
        return;
    }

    let link_path = cache_dir.join(clean_name);
    if let Some(parent) = link_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let target_path = cache_dir.join(target_filename);
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

fn extract_bytes_from_js_value<'js>(val: Value<'js>) -> Result<Vec<u8>, String> {
    if let Some(ab) = rquickjs::ArrayBuffer::from_value(val.clone()) {
        if let Some(bytes) = ab.as_bytes() {
            return Ok(bytes.to_vec());
        }
    }
    if let Ok(ta) = rquickjs::TypedArray::<u8>::from_value(val.clone()) {
        return Ok(ta.as_bytes().unwrap_or_default().to_vec());
    }
    if let Some(s) = val.as_string() {
        let s_str = s.to_string().unwrap_or_default();
        let clean = s_str
            .trim()
            .trim_start_matches("data:image/jpeg;base64,")
            .trim_start_matches("data:image/png;base64,")
            .trim_start_matches("data:image/webp;base64,")
            .trim_start_matches("data:application/wasm;base64,")
            .trim_start_matches("data:application/octet-stream;base64,");
        if let Ok(b) = BASE64.decode(clean) {
            return Ok(b);
        }
    }
    if let Ok(vec) = from_value::<Vec<u8>>(val) {
        return Ok(vec);
    }
    Err("Could not extract WASM byte array from input".to_string())
}

fn resolve_wasm_bytes(
    app_ctx: &Arc<dyn ScriptContext>,
    input: &str,
    readable_name_opt: Option<&str>,
    is_root: bool,
) -> Result<(Vec<u8>, Option<String>), String> {
    let cache_dir = get_wasm_cache_dir();
    let clean_input = input
        .trim()
        .trim_start_matches("data:application/wasm;base64,")
        .trim_start_matches("data:application/wasi;base64,")
        .trim_start_matches('/')
        .trim_start_matches("./");

    if clean_input.len() < 256 {
        let cached_file = cache_dir.join(clean_input);
        if cached_file.exists() && cached_file.is_file() {
            let bytes = fs::read(&cached_file).map_err(|e| e.to_string())?;
            return Ok((bytes, Some(clean_input.to_string())));
        }
    }

    let bytes = match BASE64.decode(clean_input) {
        Ok(b) if !b.is_empty() => b,
        _ => {
            let storage = app_ctx.get_storage();
            let input_owned = input.to_string();
            match futures::executor::block_on(async { storage.get(&input_owned).await }) {
                Ok(b) => b,
                Err(_) => {
                    return Err(format!(
                        "Invalid WASM input: File '{}' not found in cache or storage.",
                        input
                    ));
                }
            }
        }
    };

    if !is_root && bytes.len() > 200 * 1024 {
        return Err(format!(
            "Tenant WASM upload size ({} KB) exceeds allowed limit (200 KB max).",
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
    if let Ok(module) = unsafe { Module::deserialize(engine, bytes) } {
        return Ok(module);
    }

    let hash = crate::utils::to_hex(digest(&SHA256, bytes).as_ref());
    let cache_dir = get_wasm_cache_dir();
    let wasm_file = cache_dir.join(format!("{}.wasm", hash));
    let cwasm_file = cache_dir.join(format!("{}.cwasm", hash));

    if let Some(rname) = readable_name {
        create_readable_symlink(&cache_dir, rname, &format!("{}.wasm", hash));
        let cwasm_rname = if rname.ends_with(".wasm") {
            format!("{}.cwasm", rname.trim_end_matches(".wasm"))
        } else {
            format!("{}.cwasm", rname)
        };
        create_readable_symlink(&cache_dir, &cwasm_rname, &format!("{}.cwasm", hash));
    }

    if cwasm_file.exists() {
        if let Ok(module) = unsafe { Module::deserialize_file(engine, &cwasm_file) } {
            return Ok(module);
        } else {
            let _ = fs::remove_file(&cwasm_file);
        }
    }

    if !wasm_file.exists() {
        let _ = fs::write(&wasm_file, bytes);
    }

    let module = Module::from_binary(engine, bytes)
        .map_err(|e| format!("failed to parse WebAssembly module: {}", e))?;

    if let Ok(serialized) = module.serialize() {
        let _ = fs::write(&cwasm_file, serialized);
    }

    Ok(module)
}

fn instantiate_wasm_module<'js>(
    ctx: &Ctx<'js>,
    bytes: &[u8],
    _imports_val: Option<Value<'js>>,
    is_root: bool,
) -> Result<Object<'js>, String> {
    let config = Config::new();
    let engine = Engine::new(&config).map_err(|e| e.to_string())?;
    let module = get_or_compile_module(&engine, bytes, None)?;

    let memory_limit_mb = if is_root { 512 } else { 64 };
    let state = WasmStoreState {
        wasi: None,
        limiter: CustomResourceLimiter {
            max_memory_bytes: memory_limit_mb * 1024 * 1024,
        },
    };

    let mut store = Store::new(&engine, state);
    store.limiter(|s| &mut s.limiter);

    let mut linker = Linker::new(&engine);
    linker
        .define_unknown_imports_as_default_values(&module)
        .ok();

    let instance = linker
        .instantiate(&mut store, &module)
        .or_else(|_| Instance::new(&mut store, &module, &[]))
        .map_err(|e| e.to_string())?;

    let exports_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;
    let instance_id = uuid::Uuid::new_v4().to_string();

    let store_arc = Arc::new(Mutex::new(store));
    let instance_arc = Arc::new(instance);

    get_instance_registry().lock().unwrap().insert(
        instance_id.clone(),
        (store_arc.clone(), instance_arc.clone()),
    );

    let mem_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // Allocate the JS ArrayBuffer starting with the WASM module's INITIAL data section
    let buffer_cap = memory_limit_mb * 1024 * 1024;
    let initial_vec = {
        let mut store_guard = store_arc.lock().unwrap();
        if let Some(memory) = instance_arc.get_memory(&mut *store_guard, "memory") {
            let mut v = memory.data(&*store_guard).to_vec();
            if v.len() < buffer_cap {
                v.resize(buffer_cap, 0);
            }
            v
        } else {
            vec![0u8; buffer_cap]
        }
    };

    let js_ab = rquickjs::ArrayBuffer::new(ctx.clone(), initial_vec).map_err(|e| e.to_string())?;
    mem_obj.set("buffer", js_ab).ok();
    exports_obj.set("memory", mem_obj).ok();

    if let Ok(registry) = ctx.globals().get::<_, Object>("__wasm_instances") {
        let _ = registry.set(&instance_id, exports_obj.clone());
    }

    for export_item in module.exports() {
        let name = export_item.name().to_string();
        if name == "memory" {
            continue;
        }

        if let ExternType::Func(func_ty) = export_item.ty() {
            let store_ref = store_arc.clone();
            let inst_ref = instance_arc.clone();
            let instance_id_clone = instance_id.clone();
            let func_name = name.clone();
            let param_types: Vec<ValType> = func_ty.params().collect();
            let result_types: Vec<ValType> = func_ty.results().collect();

            let js_func = Function::new(
                ctx.clone(),
                move |ctx_call: Ctx<'js>,
                      rquickjs::function::Rest(args): rquickjs::function::Rest<Value<'js>>|
                      -> rquickjs::Result<Value<'js>> {
                    let mut store_guard = match store_ref.lock() {
                        Ok(g) => g,
                        Err(_) => {
                            let js_err = Exception::from_message(
                                ctx_call.clone(),
                                "WASM store mutex lock failed",
                            )
                            .unwrap();
                            return Err(ctx_call.throw(js_err.into()));
                        }
                    };

                    // 1. Sync JS Memory -> Wasmtime before execution
                    if let Some(memory) = inst_ref.get_memory(&mut *store_guard, "memory") {
                        if let Ok(registry) =
                            ctx_call.globals().get::<_, Object>("__wasm_instances")
                        {
                            if let Ok(inst_obj) = registry.get::<_, Object>(&instance_id_clone) {
                                if let Ok(mem_obj) = inst_obj.get::<_, Object>("memory") {
                                    if let Ok(js_ab) =
                                        mem_obj.get::<_, rquickjs::ArrayBuffer>("buffer")
                                    {
                                        if let Some(js_bytes) = js_ab.as_bytes() {
                                            let wasm_mem = memory.data_mut(&mut *store_guard);
                                            let copy_len =
                                                std::cmp::min(wasm_mem.len(), js_bytes.len());
                                            wasm_mem[..copy_len]
                                                .copy_from_slice(&js_bytes[..copy_len]);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let wasm_func = match inst_ref.get_func(&mut *store_guard, &func_name) {
                        Some(f) => f,
                        None => {
                            let js_err = Exception::from_message(
                                ctx_call.clone(),
                                &format!("Function '{}' not found", func_name),
                            )
                            .unwrap();
                            return Err(ctx_call.throw(js_err.into()));
                        }
                    };

                    let mut wasm_args = Vec::new();
                    for (i, arg_val) in args.iter().enumerate() {
                        let param_ty = param_types.get(i).cloned().unwrap_or(ValType::F64);
                        let num: f64 = from_value(arg_val.clone()).unwrap_or(0.0);
                        match param_ty {
                            ValType::I32 => wasm_args.push(Val::I32(num as i32)),
                            ValType::I64 => wasm_args.push(Val::I64(num as i64)),
                            ValType::F32 => wasm_args.push(Val::F32((num as f32).to_bits())),
                            ValType::F64 => wasm_args.push(Val::F64(num.to_bits())),
                            _ => wasm_args.push(Val::F64(num.to_bits())),
                        }
                    }

                    let mut results = vec![Val::I32(0); result_types.len()];

                    if let Err(e) = wasm_func.call(&mut *store_guard, &wasm_args, &mut results) {
                        let js_err = Exception::from_message(
                            ctx_call.clone(),
                            &format!("WASM function execution failed: {}", e),
                        )
                        .unwrap();
                        return Err(ctx_call.throw(js_err.into()));
                    }

                    // 2. Sync Wasmtime -> JS Memory in-place after execution
                    if let Some(memory) = inst_ref.get_memory(&mut *store_guard, "memory") {
                        let data_slice = memory.data(&*store_guard);
                        let slice_len = data_slice.len();
                        if let Ok(registry) =
                            ctx_call.globals().get::<_, Object>("__wasm_instances")
                        {
                            if let Ok(inst_obj) = registry.get::<_, Object>(&instance_id_clone) {
                                if let Ok(mem_obj) = inst_obj.get::<_, Object>("memory") {
                                    if let Ok(old_ab) =
                                        mem_obj.get::<_, rquickjs::ArrayBuffer>("buffer")
                                    {
                                        if let Ok(new_ab) = rquickjs::ArrayBuffer::new(
                                            ctx_call.clone(),
                                            data_slice.to_vec(),
                                        ) {
                                            if let Ok(sync_fn) =
                                                ctx_call
                                                    .globals()
                                                    .get::<_, rquickjs::Function>("__apex_sync_mem")
                                            {
                                                let _ = sync_fn
                                                    .call::<_, ()>((old_ab, new_ab, slice_len));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Handle Multi-Value WebAssembly Returns
                    let ret_val = match results.len() {
                        0 => json!(null),
                        1 => match results[0] {
                            Val::I32(i) => json!(i),
                            Val::I64(i) => json!(i),
                            Val::F32(f) => json!(f32::from_bits(f)),
                            Val::F64(f) => json!(f64::from_bits(f)),
                            _ => json!(null),
                        },
                        _ => {
                            let vals: Vec<serde_json::Value> = results
                                .iter()
                                .map(|r| match r {
                                    Val::I32(i) => json!(*i),
                                    Val::I64(i) => json!(*i),
                                    Val::F32(f) => json!(f32::from_bits(*f)),
                                    Val::F64(f) => json!(f64::from_bits(*f)),
                                    _ => json!(null),
                                })
                                .collect();
                            json!(vals)
                        }
                    };

                    to_value(ctx_call.clone(), &ret_val).map_err(|e| {
                        let js_err = Exception::from_message(
                            ctx_call.clone(),
                            &format!("Failed to parse WASM output: {}", e),
                        )
                        .unwrap();
                        ctx_call.throw(js_err.into())
                    })
                },
            )
            .map_err(|e| e.to_string())?;

            exports_obj.set(&name, js_func).map_err(|e| e.to_string())?;
        }
    }

    Ok(exports_obj)
}

pub fn register_wasm<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let wasm_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let get_mem_fn = Function::new(
        ctx.clone(),
        move |js_ctx: Ctx<'js>,
              instance_id: String|
              -> rquickjs::Result<rquickjs::ArrayBuffer<'js>> {
            let registry = get_instance_registry().lock().unwrap();
            if let Some((store_ref, inst_ref)) = registry.get(&instance_id) {
                let mut store_guard = store_ref.lock().unwrap();
                if let Some(memory) = inst_ref.get_memory(&mut *store_guard, "memory") {
                    let data = memory.data(&*store_guard);
                    return rquickjs::ArrayBuffer::new(js_ctx, data.to_vec());
                }
            }
            let js_err =
                Exception::from_message(js_ctx.clone(), "WASM Memory Instance not found").unwrap();
            Err(js_ctx.throw(js_err.into()))
        },
    )
    .map_err(|e| e.to_string())?;

    wasm_obj
        .set("__getMemoryBuffer", get_mem_fn)
        .map_err(|e| e.to_string())?;

    let app_inst = app_ctx.clone();
    let instantiate_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, bytes_val: Value<'js>, Opt(imports_val): Opt<Value<'js>>| {
                let app = app_inst.clone();
                async move {
                    let bytes = match extract_bytes_from_js_value(bytes_val) {
                        Ok(b) => b,
                        Err(e) => {
                            let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                    };

                    let is_root = matches!(app.get_scope(), EventScope::Root);

                    let res_exports =
                        match instantiate_wasm_module(&js_ctx, &bytes, imports_val, is_root) {
                            Ok(exp) => exp,
                            Err(e) => {
                                let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                                return Err(js_ctx.throw(js_err.into()));
                            }
                        };

                    let res_obj = Object::new(js_ctx.clone()).map_err(|_| {
                        let js_err = Exception::from_message(
                            js_ctx.clone(),
                            "Failed to create exports object",
                        )
                        .unwrap();
                        js_ctx.throw(js_err.into())
                    })?;

                    res_obj.set("exports", res_exports).ok();

                    Ok::<Object<'js>, rquickjs::Error>(res_obj)
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    wasm_obj
        .set("__instantiate", instantiate_fn)
        .map_err(|e| e.to_string())?;

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
                    let timeout_ms = opts
                        .timeout_ms
                        .unwrap_or(if is_root { 30_000 } else { 300 });

                    let (bytes, readable_name) =
                        match resolve_wasm_bytes(&app, &input, opts.name.as_deref(), is_root) {
                            Ok(res) => res,
                            Err(e) => {
                                let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                                return Err(js_ctx.throw(js_err.into()));
                            }
                        };

                    let task = tokio::task::spawn_blocking(move || {
                        let config = Config::new();
                        let engine = Engine::new(&config).map_err(|e| e.to_string())?;
                        let module =
                            get_or_compile_module(&engine, &bytes, readable_name.as_deref())?;

                        let state = WasmStoreState {
                            wasi: None,
                            limiter: CustomResourceLimiter {
                                max_memory_bytes: memory_limit_mb * 1024 * 1024,
                            },
                        };

                        let mut store = Store::new(&engine, state);
                        store.limiter(|s| &mut s.limiter);

                        let mut linker = Linker::new(&engine);
                        linker
                            .define_unknown_imports_as_default_values(&module)
                            .ok();

                        let instance = linker
                            .instantiate(&mut store, &module)
                            .or_else(|_| Instance::new(&mut store, &module, &[]))
                            .map_err(|e| e.to_string())?;

                        let export = instance.get_func(&mut store, &func).ok_or_else(|| {
                            format!("Function '{}' not found in WASM exports", func)
                        })?;

                        let func_type = export.ty(&store);
                        let param_types: Vec<ValType> = func_type.params().collect();

                        let mut wasm_args = Vec::with_capacity(args.len());
                        for (i, &a) in args.iter().enumerate() {
                            let param_ty = param_types.get(i).cloned().unwrap_or(ValType::F64);
                            match param_ty {
                                ValType::I32 => wasm_args.push(Val::I32(a as i32)),
                                ValType::I64 => wasm_args.push(Val::I64(a as i64)),
                                ValType::F32 => wasm_args.push(Val::F32((a as f32).to_bits())),
                                ValType::F64 => wasm_args.push(Val::F64(a.to_bits())),
                                _ => wasm_args.push(Val::F64(a.to_bits())),
                            }
                        }

                        let result_types: Vec<ValType> = func_type.results().collect();
                        let mut results = vec![Val::I32(0); result_types.len()];

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
                            let js_err = Exception::from_message(
                                js_ctx.clone(),
                                &format!("Task panicked: {}", e),
                            )
                            .unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                        Err(_) => {
                            let js_err =
                                Exception::from_message(js_ctx.clone(), "WASM execution timed out")
                                    .unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                    };

                    to_value(js_ctx.clone(), &res).map_err(|_| {
                        let js_err =
                            Exception::from_message(js_ctx.clone(), "Failed to serialize result")
                                .unwrap();
                        js_ctx.throw(js_err.into())
                    })
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

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
                    let cli_args: Option<Vec<String>> =
                        cli_args_val.and_then(|v| from_value(v).ok());
                    let opts: WasmOptions = opts_val
                        .and_then(|v| from_value(v).ok())
                        .unwrap_or_default();

                    let is_root = matches!(app.get_scope(), EventScope::Root);
                    let memory_limit_mb = opts.memory_mb.unwrap_or(if is_root { 512 } else { 64 });
                    let timeout_ms = opts
                        .timeout_ms
                        .unwrap_or(if is_root { 30_000 } else { 300 });

                    let (bytes, readable_name) =
                        match resolve_wasm_bytes(&app, &input, opts.name.as_deref(), is_root) {
                            Ok(res) => res,
                            Err(e) => {
                                let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                                return Err(js_ctx.throw(js_err.into()));
                            }
                        };

                    let task = tokio::task::spawn_blocking(move || {
                        let config = Config::new();
                        let engine = Engine::new(&config).map_err(|e| e.to_string())?;
                        let module =
                            get_or_compile_module(&engine, &bytes, readable_name.as_deref())?;

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

                        let mut linker: Linker<WasmStoreState> = Linker::new(&engine);
                        preview1::add_to_linker_sync(&mut linker, |s| s.wasi.as_mut().unwrap())
                            .map_err(|e| e.to_string())?;

                        linker
                            .define_unknown_imports_as_default_values(&module)
                            .ok();

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
                            let js_err = Exception::from_message(
                                js_ctx.clone(),
                                &format!("Task panicked: {}", e),
                            )
                            .unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                        Err(_) => {
                            let js_err =
                                Exception::from_message(js_ctx.clone(), "WASM execution timed out")
                                    .unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                    };

                    to_value(js_ctx.clone(), &res).map_err(|_| {
                        let js_err =
                            Exception::from_message(js_ctx.clone(), "Failed to serialize result")
                                .unwrap();
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
