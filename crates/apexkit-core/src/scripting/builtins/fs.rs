use crate::realtime::EventScope;
use serde_json::json;
use std::path::PathBuf;

use super::super::{context::ACTIVE_CONTEXT, return_json_promise};
use super::db::resolve_db;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use boa_engine::{
    Context, JsArgs, JsString, NativeFunction, object::ObjectInitializer, property::Attribute,
};
use std::io::{Cursor, Read, Write};
use std::time::UNIX_EPOCH;
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

fn get_storage_path(subpath: &str) -> String {
    if let Ok(base) = std::env::var("APEXKIT_MOUNTED_FILE_STORAGE") {
        let clean_base = base.trim_end_matches('/');
        let clean_sub = subpath.trim_start_matches('/');
        format!("{}/{}", clean_base, clean_sub)
    } else {
        subpath.to_string()
    }
}

// 1. Resolve READ Path (Scope Root)
fn resolve_read_path(scope: &EventScope, requested_path: &str) -> Result<PathBuf, String> {
    if requested_path.contains("..") {
        return Err("Path traversal forbidden".into());
    }

    let base_dir = match scope {
        EventScope::Root => {
            // Root Admin can read anywhere via prefix
            if let Some(stripped) = requested_path.strip_prefix("tenant:") {
                let parts: Vec<&str> = stripped.splitn(2, '/').collect();
                if parts.len() < 2 {
                    return Err("Invalid format".into());
                }
                format!("storage/tenants/{}/{}", parts[0], parts[1])
            } else if let Some(stripped) = requested_path.strip_prefix("sandbox:") {
                let parts: Vec<&str> = stripped.splitn(2, '/').collect();
                if parts.len() < 2 {
                    return Err("Invalid format".into());
                }
                format!("storage/sandboxes/session_{}/{}", parts[0], parts[1])
            } else {
                format!("storage/system/{}", requested_path)
            }
        }
        EventScope::Tenant(id) => format!("storage/tenants/{}/{}", id, requested_path),
        EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/{}", id, requested_path),
        _ => return Err("Invalid scope".into()),
    };

    Ok(PathBuf::from(get_storage_path(&base_dir)))
}

// 2. Resolve WRITE Path (Scope TMP Only)
fn resolve_write_path(scope: &EventScope, requested_path: &str) -> Result<PathBuf, String> {
    if requested_path.contains("..") {
        return Err("Path traversal forbidden".into());
    }

    // Root Admin can write anywhere (Power User) - OR restrict to root/tmp?
    // Let's restrict Root to its own root/tmp for consistency,
    // unless they explicitly use a prefix to write to a tenant's tmp.

    let base_dir = match scope {
        EventScope::Root => {
            if let Some(stripped) = requested_path.strip_prefix("tenant:") {
                let parts: Vec<&str> = stripped.splitn(2, '/').collect();
                if parts.len() < 2 {
                    return Err("Invalid format".into());
                }
                format!("storage/tenants/{}/tmp/{}", parts[0], parts[1])
            } else if let Some(stripped) = requested_path.strip_prefix("sandbox:") {
                let parts: Vec<&str> = stripped.splitn(2, '/').collect();
                if parts.len() < 2 {
                    return Err("Invalid format".into());
                }
                format!("storage/sandboxes/session_{}/tmp/{}", parts[0], parts[1])
            } else {
                format!("storage/system/tmp/{}", requested_path)
            }
        }
        EventScope::Tenant(id) => format!("storage/tenants/{}/tmp/{}", id, requested_path),
        EventScope::Sandbox(id) => {
            format!("storage/sandboxes/session_{}/tmp/{}", id, requested_path)
        }
        _ => return Err("Invalid scope".into()),
    };

    Ok(PathBuf::from(get_storage_path(&base_dir)))
}

pub fn register_file_tools(ctx: &mut Context) -> Result<(), String> {
    // Reuses the logic from register_zip's save_file_fn and read_file_fn
    // We can just copy the NativeFunction definitions since they are closures capturing env.
    // Or refactor to shared helpers.

    // For now, I will implement them as part of a new `$files` object for cleanliness,
    // or global if strictly requested. The prompt says "$saveFile and $readFile", implying globals or tools.
    // Let's make a `$files` object.

    // Reuse logic:

    // $files.read(filename) -> Base64
    let read_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        // See implementation below
        let filename = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    let storage = app.get_storage();
                    match storage.get(&filename).await {
                        Ok(bytes) => Ok(json!(
                            base64::engine::general_purpose::STANDARD.encode(bytes)
                        )),
                        Err(e) => Err(format!("Read failed: {}", e)),
                    }
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // $files.save(filename, base64, mime) -> Metadata
    let save_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let filename = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let b64_data = args
            .get_or_undefined(1)
            .to_string(ctx)?
            .to_std_string_escaped();
        let mime_type = args
            .get(2)
            .and_then(|v| v.as_string())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or("application/octet-stream".to_string());

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    // Resolve DB (Dynamic)
                    // We need resolve_db helper from scripting_db.rs? It's private there.
                    // We should move resolve_db to a shared location or make it public.
                    // Assuming we fix visibility or copy logic (it's short):

                    // Logic to get DB:
                    let scope = app.get_scope();
                    let db = match scope {
                        EventScope::Tenant(id) => app
                            .resolve_tenant_db(&id)
                            .await
                            .ok_or("Tenant DB not found".to_string())?,
                        EventScope::Sandbox(id) => app
                            .resolve_sandbox_db(&id)
                            .await
                            .ok_or("Sandbox DB not found".to_string())?,
                        _ => app.get_db(),
                    };

                    let storage = app.get_storage();
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(&b64_data)
                        .map_err(|_| "Invalid Base64")?;
                    let size = bytes.len() as i64;

                    // Rename
                    let path = std::path::Path::new(&filename);
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("bin");
                    let storage_filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);

                    storage
                        .save(&storage_filename, &bytes, &mime_type)
                        .await
                        .map_err(|e| e.to_string())?;

                    let id = db
                        .create_file_metadata(&storage_filename, &filename, &mime_type, size, None)
                        .await
                        .map_err(|e| e.to_string())?;
                    let url = format!("{}{}", storage.get_public_url_base(), storage_filename);

                    Ok(json!({ "id": id, "url": url, "filename": storage_filename }))
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // $files.getSignedUrl(filename, ttl_seconds?) -> string (URL)
    let signed_url_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let filename = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        // Default to 15 minutes (900s) if not specified
        let ttl = args
            .get(1)
            .and_then(|v| v.to_number(ctx).ok())
            .unwrap_or(900.0) as u64;

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    let storage = app.get_storage();
                    match storage.get_signed_url(&filename, ttl).await {
                        Ok(url) => Ok(json!(url)),
                        Err(e) => Err(format!("Signing failed: {}", e)),
                    }
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // $files.registerMetadata(filename, options) -> Metadata
    // options: { originalName?, mimeType?, size? }
    let register_metadata_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let filename = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let opts = args
            .get_or_undefined(1)
            .to_json(ctx)
            .unwrap()
            .unwrap_or(serde_json::json!({}));

        let original_name = opts
            .get("originalName")
            .and_then(|v| v.as_str())
            .unwrap_or(&filename)
            .to_string();
        let mime_type = opts
            .get("mimeType")
            .and_then(|v| v.as_str())
            .unwrap_or("application/octet-stream")
            .to_string();
        let size = opts.get("size").and_then(|v| v.as_i64()).unwrap_or(0);

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    let db = resolve_db(None, app.clone()).await?;

                    // 1. Check for existing filename to ensure consistency
                    if let Ok(Some(_)) = db.get_file_by_filename(&filename).await {
                        return Err(format!(
                            "File '{}' is already registered in metadata.",
                            filename
                        ));
                    }

                    // 2. Register in Metadata DB (Does NOT generate UUID, uses provided filename)
                    let id = db
                        .create_file_metadata(&filename, &original_name, &mime_type, size, None)
                        .await
                        .map_err(|e| e.to_string())?;

                    let storage = app.get_storage();
                    let public_url = format!("{}{}", storage.get_public_url_base(), filename);

                    Ok(serde_json::json!({
                        "id": id,
                        "filename": filename,
                        "url": public_url,
                        "size": size
                    }))
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    let obj = ObjectInitializer::new(ctx)
        .function(read_fn, JsString::from("read"), 1)
        .function(save_fn, JsString::from("save"), 3)
        .function(register_metadata_fn, JsString::from("registerMetadata"), 2)
        .function(signed_url_fn, JsString::from("getSignedUrl"), 2)
        .build();

    ctx.register_global_property(JsString::from("$files"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}

pub fn register_fs(ctx: &mut Context) -> Result<(), String> {
    // $fs.read(path) -> string
    let read_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_read_path(scope, &fname)?; // Read from Root
                handle.block_on(async {
                    if !target.exists() {
                        return Err("File not found".into());
                    }
                    if target.is_dir() {
                        return Err("Cannot read directory".into());
                    }
                    tokio::fs::read_to_string(target)
                        .await
                        .map_err(|e| e.to_string())
                })
            } else {
                Err("Context lost".into())
            }
        });
        match result {
            Ok(s) => return_json_promise(ctx, Ok(serde_json::Value::String(s))),
            Err(e) => return_json_promise(ctx, Err(e)),
        }
    });

    // $fs.write(path, content) -> void (Writes to TMP)
    let write_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let content = args
            .get_or_undefined(1)
            .to_string(ctx)?
            .to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_write_path(scope, &fname)?; // Write to TMP
                handle.block_on(async {
                    if let Some(parent) = target.parent() {
                        tokio::fs::create_dir_all(parent).await.ok();
                    }
                    tokio::fs::write(target, content)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(json!(true))
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // $fs.delete(path) -> void (Deletes from TMP)
    let delete_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_write_path(scope, &fname)?; // Delete from TMP only
                handle
                    .block_on(async {
                        if !target.exists() {
                            return Err("File not found in tmp".into());
                        }
                        if target.is_dir() {
                            tokio::fs::remove_dir_all(target)
                                .await
                                .map_err(|e| e.to_string())
                        } else {
                            tokio::fs::remove_file(target)
                                .await
                                .map_err(|e| e.to_string())
                        }
                    })
                    .map(|_| json!(true))
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // $fs.list(path) -> Array (Lists from Root)
    let list_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_read_path(scope, &fname)?; // List from Root
                handle.block_on(async {
                    if !target.exists() {
                        return Err("Path not found".into());
                    }

                    let mut entries = Vec::new();
                    let mut dir = tokio::fs::read_dir(target)
                        .await
                        .map_err(|e| e.to_string())?;

                    while let Ok(Some(entry)) = dir.next_entry().await {
                        let meta = entry.metadata().await.map_err(|e| e.to_string())?;
                        entries.push(json!({
                            "name": entry.file_name().to_string_lossy(),
                            "isDir": meta.is_dir(),
                            "size": meta.len()
                        }));
                    }
                    Ok(json!(entries))
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // $fs.exists(path) -> bool (Checks Root)
    let exists_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, _, _, _, scope)) = &*c.borrow() {
                let target = resolve_read_path(scope, &fname); // Check Root
                match target {
                    Ok(p) => Ok(json!(p.exists())),
                    Err(_) => Ok(json!(false)),
                }
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // $fs.mkdir(path) -> void (Creates in TMP)
    let mkdir_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_write_path(scope, &fname)?; // Mkdir in TMP
                handle.block_on(async {
                    tokio::fs::create_dir_all(target)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(json!(true))
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // $fs.stat(path) -> { size, created, modified, isDir } (Checks Root)
    let stat_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let fname = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((_, handle, _, _, scope)) = &*c.borrow() {
                let target = resolve_read_path(scope, &fname)?;
                handle.block_on(async {
                     let meta = tokio::fs::metadata(target).await.map_err(|e| e.to_string())?;
                     Ok(json!({
                         "size": meta.len(),
                         "isDir": meta.is_dir(),
                         "created": meta.created().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()),
                         "modified": meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64())
                     }))
                })
            } else { Err("Context lost".into()) }
        });
        return_json_promise(ctx, result)
    });

    let obj = ObjectInitializer::new(ctx)
        .function(read_fn, JsString::from("read"), 1)
        .function(write_fn, JsString::from("write"), 2)
        .function(delete_fn, JsString::from("delete"), 1)
        .function(list_fn, JsString::from("list"), 1)
        .function(exists_fn, JsString::from("exists"), 1)
        .function(mkdir_fn, JsString::from("mkdir"), 1)
        .function(stat_fn, JsString::from("stat"), 1)
        .build();
    ctx.register_global_property(JsString::from("$fs"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}

pub fn register_zip(ctx: &mut Context) -> Result<(), String> {
    fn _resolve_storage_path(scope: &EventScope) -> String {
        let base = match scope {
            EventScope::Root => "storage/system/uploads".to_string(),
            EventScope::Tenant(id) => format!("storage/tenants/{}/uploads", id),
            EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}/uploads", id),
            _ => "storage/tmp".to_string(),
        };
        get_storage_path(&base)
    }

    let get_limit = || -> usize {
        std::env::var("ARCHIVE_LIMIT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10)
            * 1024
            * 1024
    };

    // 1. CREATE
    let create_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let files_val = args
            .get_or_undefined(0)
            .to_json(ctx)
            .unwrap()
            .unwrap_or(json!({}));
        let limit = get_limit();

        let result = (|| -> Result<String, String> {
            let files = files_val
                .as_object()
                .ok_or("Input must be an object {filename: content}")?;
            let mut buffer = Cursor::new(Vec::new());

            // Scope for ZipWriter to enforce borrow drop
            {
                let mut zip = ZipWriter::new(&mut buffer);
                let options = FileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(0o755);

                let mut estimated_size = 0;

                for (name, content_val) in files {
                    let content_str = content_val.as_str().unwrap_or("");
                    let data = if content_str.len() % 4 == 0
                        && !content_str.contains(char::is_whitespace)
                    {
                        BASE64
                            .decode(content_str)
                            .unwrap_or_else(|_| content_str.as_bytes().to_vec())
                    } else {
                        content_str.as_bytes().to_vec()
                    };

                    // Check uncompressed size accumulation to prevent DoS before compression
                    estimated_size += data.len();
                    if estimated_size > limit * 2 {
                        // Allow some slack for compression overhead? No, slack for uncompressed input vs output limit.
                        // Actually, if we want to limit the OUTPUT zip size, we can't easily check it inside the loop efficiently without flushing.
                        // Limiting input size is a good proxy.
                        return Err(format!(
                            "Input data size exceeds safety limit of {} bytes",
                            limit
                        ));
                    }

                    zip.start_file(name, options).map_err(|e| e.to_string())?;
                    zip.write_all(&data).map_err(|e| e.to_string())?;
                }
                zip.finish().map_err(|e| e.to_string())?;
            } // ZipWriter dropped here, releasing borrow on buffer

            let zip_bytes = buffer.into_inner();

            // Final check on actual archive size
            if zip_bytes.len() > limit {
                return Err(format!(
                    "Final archive size {} exceeds limit {}",
                    zip_bytes.len(),
                    limit
                ));
            }

            Ok(BASE64.encode(zip_bytes))
        })();

        return_json_promise(ctx, result.map(serde_json::Value::String))
    });

    // 2. EXTRACT
    let extract_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let b64_str = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let limit = get_limit();

        let result = (|| -> Result<serde_json::Value, String> {
            let bytes = BASE64
                .decode(&b64_str)
                .map_err(|_| "Invalid Base64".to_string())?;
            if bytes.len() > limit {
                return Err("Archive exceeds limit".into());
            }

            let cursor = Cursor::new(bytes);
            let mut archive = ZipArchive::new(cursor).map_err(|e| format!("Invalid Zip: {}", e))?;

            let mut output = serde_json::Map::new();
            let mut total_extracted = 0;

            for i in 0..archive.len() {
                let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
                if file.is_dir() {
                    continue;
                }

                let name = file.name().to_string();
                let mut content_buf = Vec::new();

                if file.size() > (limit as u64) {
                    return Err(format!("File {} too large", name));
                }

                file.read_to_end(&mut content_buf)
                    .map_err(|_| "Read fail".to_string())?;
                total_extracted += content_buf.len();

                if total_extracted > limit {
                    return Err("Total extracted size exceeds limit".into());
                }

                let val = match String::from_utf8(content_buf.clone()) {
                    Ok(s) => json!(s),
                    Err(_) => json!(BASE64.encode(&content_buf)),
                };
                output.insert(name, val);
            }
            Ok(serde_json::Value::Object(output))
        })();
        return_json_promise(ctx, result)
    });

    // 3. INSPECT (Metadata)
    let inspect_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let b64_str = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let result = (|| -> Result<serde_json::Value, String> {
            let bytes = BASE64
                .decode(&b64_str)
                .map_err(|_| "Invalid Base64".to_string())?;
            let cursor = Cursor::new(bytes.clone());
            let mut archive = ZipArchive::new(cursor).map_err(|e| format!("Invalid Zip: {}", e))?;

            let mut files_meta = Vec::new();
            let mut total_uncompressed: u64 = 0;
            let mut total_compressed: u64 = 0;

            for i in 0..archive.len() {
                let file = archive.by_index(i).map_err(|e| e.to_string())?;

                let size = file.size();
                let comp_size = file.compressed_size();
                total_uncompressed += size;
                total_compressed += comp_size;

                // FIX: DateTime is a struct, not Option
                let dt = file.last_modified();
                let modified_str = format!(
                    "{}-{:02}-{:02} {:02}:{:02}:{:02}",
                    dt.year(),
                    dt.month(),
                    dt.day(),
                    dt.hour(),
                    dt.minute(),
                    dt.second()
                );

                files_meta.push(json!({
                    "name": file.name(),
                    "size": size,
                    "compressed_size": comp_size,
                    "is_dir": file.is_dir(),
                    "comment": file.comment(),
                    "modified": modified_str,
                    "compression_method": format!("{:?}", file.compression())
                }));
            }

            Ok(json!({
                "total_size": bytes.len(),
                "total_uncompressed": total_uncompressed,
                "total_compressed_content": total_compressed,
                "file_count": archive.len(),
                "comment": String::from_utf8_lossy(archive.comment()).to_string(),
                "files": files_meta
            }))
        })();

        return_json_promise(ctx, result)
    });

    // 4. READ FILE (Scope Aware -> Base64)
    let read_file_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let filename = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    // Use get_storage() from context (ScopedDynamicStorage)
                    let storage = app.get_storage();

                    match storage.get(&filename).await {
                        Ok(bytes) => Ok(json!(BASE64.encode(bytes))),
                        Err(e) => Err(format!("Read failed: {}", e)),
                    }
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    // 5. SAVE FILE (Base64 -> Scope Aware Storage)
    let save_file_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let filename = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let b64_data = args
            .get_or_undefined(1)
            .to_string(ctx)?
            .to_std_string_escaped();
        let mime_type = args
            .get(2)
            .and_then(|v| v.as_string())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or("application/zip".to_string());

        let result = ACTIVE_CONTEXT.with(|c| {
            if let Some((app, handle, _, _, _)) = &*c.borrow() {
                handle.block_on(async {
                    // 1. Resolve DB (Tenant/Sandbox aware)
                    let db = resolve_db(None, app.clone()).await?;

                    // 2. Resolve Storage (Tenant/Sandbox aware via ScopedDynamicStorage)
                    let storage = app.get_storage();

                    let bytes = BASE64
                        .decode(&b64_data)
                        .map_err(|_| "Invalid Base64".to_string())?;
                    let size = bytes.len() as i64;

                    // 3. Generate unique storage filename
                    let path = std::path::Path::new(&filename);
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("bin");
                    let storage_filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);

                    // 4. Save to Storage (S3 or Local)
                    storage
                        .save(&storage_filename, &bytes, &mime_type)
                        .await
                        .map_err(|e| e.to_string())?;

                    // 5. Register in Metadata DB
                    // Pass None for user_id as script context doesn't implicitly carry a user unless passed in args,
                    // or we could extract it from ACTIVE_CONTEXT if we stored auth claims there (we don't currently).
                    let id = db
                        .create_file_metadata(&storage_filename, &filename, &mime_type, size, None)
                        .await
                        .map_err(|e| e.to_string())?;

                    let public_url =
                        format!("{}{}", storage.get_public_url_base(), storage_filename);

                    Ok(json!({
                        "id": id,
                        "filename": storage_filename,
                        "original_name": filename,
                        "url": public_url,
                        "size": size
                    }))
                })
            } else {
                Err("Context lost".into())
            }
        });
        return_json_promise(ctx, result)
    });

    let obj = ObjectInitializer::new(ctx)
        .function(create_fn, JsString::from("create"), 1)
        .function(extract_fn, JsString::from("extract"), 1)
        .function(inspect_fn, JsString::from("inspect"), 1)
        .function(read_file_fn, JsString::from("readFile"), 1)
        .function(save_file_fn, JsString::from("saveFile"), 2)
        .build();

    ctx.register_global_property(JsString::from("$zip"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
