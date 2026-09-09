use crate::AppError;
use apexkit_core::auth::Claims;
use axum::{Extension, Json, extract::Multipart, http::StatusCode};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use wasmtime::{Config, Engine, Module};

#[derive(Serialize, utoipa::ToSchema)]
pub struct WasmModuleDto {
    pub name: String,
    pub size: u64,
    pub is_symlink: bool,
    pub target: Option<String>,
}

fn get_wasm_cache_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("APEXKIT_WASM_CACHE_DIR") {
        let dir = PathBuf::from(custom);
        let _ = fs::create_dir_all(&dir);
        return dir;
    }
    if let Ok(base_storage) = std::env::var("APEXKIT_MOUNTED_FILE_STORAGE") {
        let dir = PathBuf::from(base_storage).join(".cache").join("wasm");
        let _ = fs::create_dir_all(&dir);
        return dir;
    }
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let dir = base.join(".cache").join("wasm");
    let _ = fs::create_dir_all(&dir);
    dir
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct FetchWasmReq {
    pub url: String,
    pub name: Option<String>,
}

async fn process_and_save_wasm(bytes: Vec<u8>, name: String) -> Result<String, AppError> {
    // Validate & Compile AOT using Wasmtime
    let mut config = Config::new();
    config.wasm_threads(true);
    config.wasm_bulk_memory(true);
    config.wasm_multi_memory(true);
    config.wasm_simd(true);

    let engine = Engine::new(&config)
        .map_err(|e| AppError::UnknownError(format!("Wasmtime Engine init failed: {}", e)))?;

    // This will error out if the uploaded file is not a valid WebAssembly binary
    let module = Module::from_binary(&engine, &bytes)
        .map_err(|e| AppError::UnknownError(format!("Invalid WASM binary: {}", e)))?;

    let hash_bytes = digest(&SHA256, &bytes);
    let hash_hex = apexkit_core::utils::to_hex(hash_bytes.as_ref());

    let cache_dir = get_wasm_cache_dir();
    let wasm_file = cache_dir.join(format!("{}.wasm", hash_hex));
    let cwasm_file = cache_dir.join(format!("{}.cwasm", hash_hex));

    // Save Raw
    fs::write(&wasm_file, &bytes).map_err(|e| AppError::UnknownError(e.to_string()))?;

    // Save Precompiled (CWASM)
    if let Ok(serialized) = module.serialize() {
        let _ = fs::write(&cwasm_file, serialized);
    }

    let create_link = |link_name: &str, target: &str| {
        let clean_name = link_name.trim_start_matches('/').trim_start_matches("./");
        if clean_name.is_empty() {
            return;
        }

        let link_path = cache_dir.join(clean_name);
        if let Some(parent) = link_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::remove_file(&link_path);

        #[cfg(unix)]
        let _ = std::os::unix::fs::symlink(cache_dir.join(target), &link_path);
        #[cfg(windows)]
        let _ = std::os::windows::fs::symlink_file(cache_dir.join(target), &link_path);
    };

    create_link(&name, &format!("{}.wasm", hash_hex));
    let cwasm_name = if name.ends_with(".wasm") {
        format!("{}.cwasm", name.trim_end_matches(".wasm"))
    } else {
        format!("{}.cwasm", name)
    };
    create_link(&cwasm_name, &format!("{}.cwasm", hash_hex));

    Ok(hash_hex)
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/wasm",
    responses((status = 200, body = Vec<WasmModuleDto>))
)]
pub async fn list_wasm_handler(
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<WasmModuleDto>>, AppError> {
    if claims.role != "admin" || claims.scope != "root" {
        return Err(AppError::Forbidden("Root Admins only".into()));
    }
    let dir = get_wasm_cache_dir();
    let mut modules = vec![];
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if let Ok(meta) = fs::symlink_metadata(entry.path()) {
                let is_symlink = meta.file_type().is_symlink();
                let target = if is_symlink {
                    fs::read_link(entry.path())
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                } else {
                    None
                };

                if is_symlink || fname.ends_with(".wasm") || fname.ends_with(".cwasm") {
                    modules.push(WasmModuleDto {
                        name: fname,
                        size: meta.len(),
                        is_symlink,
                        target,
                    });
                }
            }
        }
    }
    // Sort symlinks first, then raw hashes
    modules.sort_by(|a, b| {
        b.is_symlink
            .cmp(&a.is_symlink)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(Json(modules))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/wasm/upload",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses((status = 200, description = "WASM processed and cached"))
)]
pub async fn upload_wasm_handler(
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" || claims.scope != "root" {
        return Err(AppError::Forbidden("Root Admins only".into()));
    }

    let mut name_opt = None;
    let mut bytes_opt = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::UnknownError("Multipart error".into()))?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        if field_name == "name" {
            name_opt = field.text().await.ok();
        } else if field_name == "file" {
            let filename = field.file_name().unwrap_or("module.wasm").to_string();
            if name_opt.is_none() {
                name_opt = Some(filename);
            }
            bytes_opt = field.bytes().await.ok().map(|b| b.to_vec());
        }
    }

    let bytes = bytes_opt.ok_or(AppError::UnknownError("No file uploaded".into()))?;
    let name = name_opt.unwrap_or_else(|| "module.wasm".into());

    let hash_hex = process_and_save_wasm(bytes, name).await?;

    Ok(Json(
        serde_json::json!({ "success": true, "hash": hash_hex }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/wasm/fetch",
    request_body = FetchWasmReq,
    responses((status = 200, description = "WASM fetched and cached"))
)]
pub async fn fetch_wasm_handler(
    Extension(claims): Extension<Claims>,
    Json(payload): Json<FetchWasmReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" || claims.scope != "root" {
        return Err(AppError::Forbidden("Root Admins only".into()));
    }

    let client = reqwest::Client::new();
    let res = client
        .get(&payload.url)
        .send()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    if !res.status().is_success() {
        return Err(AppError::UnknownError(format!(
            "Failed to fetch URL: HTTP {}",
            res.status()
        )));
    }

    let bytes = res
        .bytes()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .to_vec();

    let name = payload.name.unwrap_or_else(|| {
        payload
            .url
            .split('/')
            .last()
            .unwrap_or("module.wasm")
            .split('?')
            .next()
            .unwrap_or("module.wasm")
            .to_string()
    });

    let hash_hex = process_and_save_wasm(bytes, name).await?;

    Ok(Json(
        serde_json::json!({ "success": true, "hash": hash_hex }),
    ))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/wasm/{name}",
    responses((status = 204, description = "WASM deleted"))
)]
pub async fn delete_wasm_handler(
    Extension(claims): Extension<Claims>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" || claims.scope != "root" {
        return Err(AppError::Forbidden("Root Admins only".into()));
    }

    let dir = get_wasm_cache_dir();

    // Sanitize path traversal
    let safe_name = name.replace("..", "");
    let path = dir.join(&safe_name);

    if path.exists() {
        fs::remove_file(&path).map_err(|e| AppError::UnknownError(e.to_string()))?;

        // If we delete a .wasm symlink, automatically delete the paired .cwasm symlink
        if safe_name.ends_with(".wasm") {
            let cwasm_name = format!("{}.cwasm", safe_name.trim_end_matches(".wasm"));
            let _ = fs::remove_file(dir.join(cwasm_name));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}
