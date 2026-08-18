use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rquickjs::function::Async;
use rquickjs::{Ctx, Exception, Function, Object, Value};
use rquickjs_serde::{from_value, to_value};
use serde_json::json;
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

use super::super::context::ScriptContext;
use super::db::resolve_db;
use crate::realtime::EventScope;

fn get_storage_path(subpath: &str) -> String {
    if let Ok(base) = std::env::var("APEXKIT_MOUNTED_FILE_STORAGE") {
        let clean_base = base.trim_end_matches('/');
        let clean_sub = subpath.trim_start_matches('/');
        format!("{}/{}", clean_base, clean_sub)
    } else {
        subpath.to_string()
    }
}

fn get_temp_path(subpath: &str) -> PathBuf {
    let base = std::env::var("APEXKIT_TMP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("apexkit_tmp"));
    let clean_sub = subpath.trim_start_matches('/').trim_start_matches("./");
    base.join(clean_sub)
}

fn resolve_read_path(scope: &EventScope, requested_path: &str) -> Result<PathBuf, String> {
    if requested_path.contains("..") {
        return Err("Path traversal forbidden".into());
    }

    // 1. Check if the file was written to the local temp scratchpad first
    let temp_target = resolve_write_path(scope, requested_path)?;
    if temp_target.exists() {
        return Ok(temp_target);
    }

    // 2. Fallback to persistent mounted storage
    let base_dir = match scope {
        EventScope::Root => {
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

fn resolve_write_path(scope: &EventScope, requested_path: &str) -> Result<PathBuf, String> {
    if requested_path.contains("..") {
        return Err("Path traversal forbidden".into());
    }

    let scoped_subpath = match scope {
        EventScope::Root => {
            if let Some(stripped) = requested_path.strip_prefix("tenant:") {
                let parts: Vec<&str> = stripped.splitn(2, '/').collect();
                if parts.len() < 2 {
                    return Err("Invalid format".into());
                }
                format!("tenants/{}/tmp/{}", parts[0], parts[1])
            } else if let Some(stripped) = requested_path.strip_prefix("sandbox:") {
                let parts: Vec<&str> = stripped.splitn(2, '/').collect();
                if parts.len() < 2 {
                    return Err("Invalid format".into());
                }
                format!("sandboxes/session_{}/tmp/{}", parts[0], parts[1])
            } else {
                format!("system/tmp/{}", requested_path)
            }
        }
        EventScope::Tenant(id) => format!("tenants/{}/tmp/{}", id, requested_path),
        EventScope::Sandbox(id) => {
            format!("sandboxes/session_{}/tmp/{}", id, requested_path)
        }
        _ => return Err("Invalid scope".into()),
    };

    Ok(get_temp_path(&scoped_subpath))
}

pub fn register_file_tools<'js>(
    ctx: &Ctx<'js>,
    app_ctx: Arc<dyn ScriptContext>,
) -> Result<(), String> {
    let globals = ctx.globals();
    let files_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // 1. $files.read(filename) -> Promise<Base64>
    let app_read = app_ctx.clone();
    let read_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, filename: String| {
            let app = app_read.clone();
            async move {
                let storage = app.get_storage();
                match storage.get(&filename).await {
                    Ok(bytes) => Ok::<String, rquickjs::Error>(BASE64.encode(bytes)),
                    Err(e) => {
                        let js_err = Exception::from_message(
                            js_ctx.clone(),
                            &format!("File '{}' not found: {}", filename, e),
                        )
                        .unwrap();
                        Err(js_ctx.throw(js_err.into()))
                    }
                }
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    // 2. $files.save(filename, data, mime) -> Promise<Metadata>
    let app_save = app_ctx.clone();
    let save_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>,
                  filename: String,
                  data_val: Value<'js>,
                  mime: Option<String>| {
                let app = app_save.clone();
                async move {
                    let db = match resolve_db(None, app.clone()).await {
                        Ok(d) => d,
                        Err(e) => {
                            let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                    };
                    let storage = app.get_storage();

                    let mime_type = mime.unwrap_or_else(|| "application/octet-stream".to_string());

                    // Support Base64 String, ArrayBuffer, or Uint8Array directly
                    let bytes = if let Some(s) = data_val.as_string() {
                        let clean = s.to_string().unwrap_or_default();
                        let b64 = clean
                            .trim()
                            .trim_start_matches("data:image/jpeg;base64,")
                            .trim_start_matches("data:image/png;base64,")
                            .trim_start_matches("data:image/webp;base64,")
                            .trim_start_matches("data:application/octet-stream;base64,");
                        match BASE64.decode(b64) {
                            Ok(b) => b,
                            Err(e) => {
                                let js_err = Exception::from_message(
                                    js_ctx.clone(),
                                    &format!("Invalid Base64 payload: {}", e),
                                )
                                .unwrap();
                                return Err(js_ctx.throw(js_err.into()));
                            }
                        }
                    } else if let Some(ab) = rquickjs::ArrayBuffer::from_value(data_val.clone()) {
                        ab.as_bytes().map(|b| b.to_vec()).unwrap_or_default()
                    } else if let Some(obj) = data_val.as_object() {
                        if let Ok(ab) = obj.get::<_, rquickjs::ArrayBuffer>("buffer") {
                            let offset = obj.get::<_, usize>("byteOffset").unwrap_or(0);
                            let length = obj
                                .get::<_, usize>("byteLength")
                                .unwrap_or_else(|_| ab.as_bytes().map(|b| b.len()).unwrap_or(0));
                            if let Some(b) = ab.as_bytes() {
                                if offset + length <= b.len() {
                                    b[offset..offset + length].to_vec()
                                } else {
                                    vec![]
                                }
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };

                    if bytes.is_empty() {
                        let js_err = Exception::from_message(
                            js_ctx.clone(),
                            "Cannot save empty or invalid file data",
                        )
                        .unwrap();
                        return Err(js_ctx.throw(js_err.into()));
                    }

                    let size = bytes.len() as i64;
                    let path = std::path::Path::new(&filename);
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("bin");
                    let storage_filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);

                    if let Err(e) = storage.save(&storage_filename, &bytes, &mime_type).await {
                        let js_err = Exception::from_message(
                            js_ctx.clone(),
                            &format!("Storage save failed: {}", e),
                        )
                        .unwrap();
                        return Err(js_ctx.throw(js_err.into()));
                    }

                    let id = match db
                        .create_file_metadata(&storage_filename, &filename, &mime_type, size, None)
                        .await
                    {
                        Ok(i) => i,
                        Err(e) => {
                            let js_err = Exception::from_message(
                                js_ctx.clone(),
                                &format!("Metadata creation failed: {}", e),
                            )
                            .unwrap();
                            return Err(js_ctx.throw(js_err.into()));
                        }
                    };

                    let url = format!("{}{}", storage.get_public_url_base(), storage_filename);

                    let res = json!({ "id": id, "url": url, "filename": storage_filename });
                    to_value(js_ctx.clone(), &res).map_err(|e| {
                        let js_err = Exception::from_message(
                            js_ctx.clone(),
                            &format!("Response serialization error: {}", e),
                        )
                        .unwrap();
                        js_ctx.throw(js_err.into())
                    })
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // 3. $files.getSignedUrl(filename, ttl) -> Promise<string>
    let app_signed = app_ctx.clone();
    let signed_url_fn = Function::new(
        ctx.clone(),
        Async(
            move |js_ctx: Ctx<'js>, filename: String, ttl_opt: Option<f64>| {
                let app = app_signed.clone();
                async move {
                    let ttl = ttl_opt.unwrap_or(900.0) as u64;
                    let storage = app.get_storage();
                    match storage.get_signed_url(&filename, ttl).await {
                        Ok(url) => Ok::<String, rquickjs::Error>(url),
                        Err(e) => {
                            let js_err = Exception::from_message(
                                js_ctx.clone(),
                                &format!("Get signed URL failed: {}", e),
                            )
                            .unwrap();
                            Err(js_ctx.throw(js_err.into()))
                        }
                    }
                }
            },
        ),
    )
    .map_err(|e| e.to_string())?;

    // 4. $files.delete(filename_or_id) -> Promise<boolean>
    let app_del = app_ctx.clone();
    let delete_file_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, filename_or_id: String| {
            let app = app_del.clone();
            async move {
                let db = match resolve_db(None, app.clone()).await {
                    Ok(d) => d,
                    Err(e) => {
                        let js_err = Exception::from_message(js_ctx.clone(), &e).unwrap();
                        return Err(js_ctx.throw(js_err.into()));
                    }
                };
                let storage = app.get_storage();

                // 1. Look up file metadata from DB by numeric ID or by storage filename
                let file_meta = if let Ok(id) = filename_or_id.parse::<i64>() {
                    db.get_file_metadata(id).await
                } else {
                    db.get_file_by_filename(&filename_or_id).await
                }
                .ok()
                .flatten();

                // 2. Resolve the exact storage filename
                let physical_filename = if let Some(ref meta) = file_meta {
                    meta.filename.clone()
                } else {
                    filename_or_id.clone()
                };

                // 3. Delete physical file from storage backend (local disk or S3)
                if let Err(e) = storage.delete(&physical_filename).await {
                    let js_err = Exception::from_message(
                        js_ctx.clone(),
                        &format!("Storage delete failed: {}", e),
                    )
                    .unwrap();
                    return Err(js_ctx.throw(js_err.into()));
                }

                // 4. Delete metadata row from database registry (_storage_files table)
                if let Some(meta) = file_meta {
                    let _ = db.delete_file_metadata(meta.id).await;
                }

                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    files_obj
        .set("delete", delete_file_fn)
        .map_err(|e| e.to_string())?;
    files_obj.set("read", read_fn).map_err(|e| e.to_string())?;
    files_obj.set("save", save_fn).map_err(|e| e.to_string())?;
    files_obj
        .set("getSignedUrl", signed_url_fn)
        .map_err(|e| e.to_string())?;

    globals
        .set("$files", files_obj)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn register_fs<'js>(ctx: &Ctx<'js>, app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let fs_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let app_read = app_ctx.clone();
    let read_fn = Function::new(
        ctx.clone(),
        Async(move |fname: String| {
            let app = app_read.clone();
            async move {
                let scope = app.get_scope();
                let target =
                    resolve_read_path(&scope, &fname).map_err(|_| rquickjs::Error::Exception)?;

                if !target.exists() || target.is_dir() {
                    return Err(rquickjs::Error::Exception);
                }

                let content = tokio::fs::read_to_string(target)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                Ok::<String, rquickjs::Error>(content)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    let app_write = app_ctx.clone();
    let write_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, fname: String, content: String| {
            let app = app_write.clone();
            async move {
                let scope = app.get_scope();
                let new_bytes = content.len() as u64;

                // Validate scope-relative temp quota before write
                app.check_quota(&format!("temp:{}", new_bytes))
                    .await
                    .map_err(|e| {
                        let js_err = rquickjs::Exception::from_message(js_ctx.clone(), &e).unwrap();
                        js_ctx.throw(js_err.into())
                    })?;

                let target =
                    resolve_write_path(&scope, &fname).map_err(|_| rquickjs::Error::Exception)?;

                if let Some(parent) = target.parent() {
                    tokio::fs::create_dir_all(parent).await.ok();
                }

                tokio::fs::write(target, content)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    let app_del = app_ctx.clone();
    let delete_fn = Function::new(
        ctx.clone(),
        Async(move |fname: String| {
            let app = app_del.clone();
            async move {
                let scope = app.get_scope();
                let target =
                    resolve_write_path(&scope, &fname).map_err(|_| rquickjs::Error::Exception)?;

                if !target.exists() {
                    return Err(rquickjs::Error::Exception);
                }

                if target.is_dir() {
                    tokio::fs::remove_dir_all(target)
                        .await
                        .map_err(|_| rquickjs::Error::Exception)?;
                } else {
                    tokio::fs::remove_file(target)
                        .await
                        .map_err(|_| rquickjs::Error::Exception)?;
                };

                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    let app_list = app_ctx.clone();
    let list_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, fname: String| {
            let app = app_list.clone();
            async move {
                let scope = app.get_scope();
                let target =
                    resolve_read_path(&scope, &fname).map_err(|_| rquickjs::Error::Exception)?;

                if !target.exists() {
                    return Err(rquickjs::Error::Exception);
                }

                let mut entries = Vec::new();
                let mut dir = tokio::fs::read_dir(target)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                while let Ok(Some(entry)) = dir.next_entry().await {
                    let meta = entry
                        .metadata()
                        .await
                        .map_err(|_| rquickjs::Error::Exception)?;
                    entries.push(json!({
                        "name": entry.file_name().to_string_lossy(),
                        "isDir": meta.is_dir(),
                        "size": meta.len()
                    }));
                }

                to_value(js_ctx, &json!(entries)).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    let app_exists = app_ctx.clone();
    let exists_fn = Function::new(
        ctx.clone(),
        Async(move |fname: String| {
            let app = app_exists.clone();
            async move {
                let scope = app.get_scope();
                match resolve_read_path(&scope, &fname) {
                    Ok(p) => Ok::<bool, rquickjs::Error>(p.exists()),
                    Err(_) => Ok::<bool, rquickjs::Error>(false),
                }
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    let app_mkdir = app_ctx.clone();
    let mkdir_fn = Function::new(
        ctx.clone(),
        Async(move |fname: String| {
            let app = app_mkdir.clone();
            async move {
                let scope = app.get_scope();
                let target =
                    resolve_write_path(&scope, &fname).map_err(|_| rquickjs::Error::Exception)?;

                tokio::fs::create_dir_all(target)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                Ok::<bool, rquickjs::Error>(true)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    let app_stat = app_ctx.clone();
    let stat_fn = Function::new(
        ctx.clone(),
        Async(move |js_ctx: Ctx<'js>, fname: String| {
            let app = app_stat.clone();
            async move {
                let scope = app.get_scope();
                let target = resolve_read_path(&scope, &fname).map_err(|_| rquickjs::Error::Exception)?;

                let meta = tokio::fs::metadata(target)
                    .await
                    .map_err(|_| rquickjs::Error::Exception)?;

                let res = json!({
                    "size": meta.len(),
                    "isDir": meta.is_dir(),
                    "created": meta.created().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()),
                    "modified": meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64())
                });

                to_value(js_ctx, &res).map_err(|_| rquickjs::Error::Exception)
            }
        }),
    )
    .map_err(|e| e.to_string())?;

    fs_obj.set("read", read_fn).map_err(|e| e.to_string())?;
    fs_obj.set("write", write_fn).map_err(|e| e.to_string())?;
    fs_obj.set("delete", delete_fn).map_err(|e| e.to_string())?;
    fs_obj.set("list", list_fn).map_err(|e| e.to_string())?;
    fs_obj.set("exists", exists_fn).map_err(|e| e.to_string())?;
    fs_obj.set("mkdir", mkdir_fn).map_err(|e| e.to_string())?;
    fs_obj.set("stat", stat_fn).map_err(|e| e.to_string())?;

    globals.set("$fs", fs_obj).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn register_zip<'js>(ctx: &Ctx<'js>, _app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let zip_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    let get_limit = || -> usize {
        std::env::var("ARCHIVE_LIMIT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10)
            * 1024
            * 1024
    };

    let create_fn = Function::new(
        ctx.clone(),
        move |files_val: Value<'js>| -> rquickjs::Result<String> {
            let limit = get_limit();
            let files_json: serde_json::Value =
                from_value(files_val).map_err(|_| rquickjs::Error::Exception)?;

            let files = files_json.as_object().ok_or(rquickjs::Error::Exception)?;
            let mut buffer = Cursor::new(Vec::new());

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

                    estimated_size += data.len();
                    if estimated_size > limit * 2 {
                        return Err(rquickjs::Error::Exception);
                    }

                    zip.start_file(name, options)
                        .map_err(|_| rquickjs::Error::Exception)?;
                    zip.write_all(&data)
                        .map_err(|_| rquickjs::Error::Exception)?;
                }
                zip.finish().map_err(|_| rquickjs::Error::Exception)?;
            }

            let zip_bytes = buffer.into_inner();
            if zip_bytes.len() > limit {
                return Err(rquickjs::Error::Exception);
            }

            Ok(BASE64.encode(zip_bytes))
        },
    )
    .map_err(|e| e.to_string())?;

    let extract_fn = Function::new(
        ctx.clone(),
        move |js_ctx: Ctx<'js>, b64_str: String| -> rquickjs::Result<rquickjs::Value<'js>> {
            let limit = get_limit();

            let bytes = BASE64
                .decode(&b64_str)
                .map_err(|_| rquickjs::Error::Exception)?;
            if bytes.len() > limit {
                return Err(rquickjs::Error::Exception);
            }

            let cursor = Cursor::new(bytes);
            let mut archive = ZipArchive::new(cursor).map_err(|_| rquickjs::Error::Exception)?;

            let mut output = serde_json::Map::new();
            let mut total_extracted = 0;

            for i in 0..archive.len() {
                let mut file = archive
                    .by_index(i)
                    .map_err(|_| rquickjs::Error::Exception)?;
                if file.is_dir() {
                    continue;
                }

                let name = file.name().to_string();
                let mut content_buf = Vec::new();

                if file.size() > (limit as u64) {
                    return Err(rquickjs::Error::Exception);
                }

                file.read_to_end(&mut content_buf)
                    .map_err(|_| rquickjs::Error::Exception)?;
                total_extracted += content_buf.len();

                if total_extracted > limit {
                    return Err(rquickjs::Error::Exception);
                }

                let val = match String::from_utf8(content_buf.clone()) {
                    Ok(s) => json!(s),
                    Err(_) => json!(BASE64.encode(&content_buf)),
                };
                output.insert(name, val);
            }

            to_value(js_ctx, &serde_json::Value::Object(output))
                .map_err(|_| rquickjs::Error::Exception)
        },
    )
    .map_err(|e| e.to_string())?;

    let inspect_fn = Function::new(
        ctx.clone(),
        move |js_ctx: Ctx<'js>, b64_str: String| -> rquickjs::Result<rquickjs::Value<'js>> {
            let bytes = BASE64
                .decode(&b64_str)
                .map_err(|_| rquickjs::Error::Exception)?;
            let cursor = Cursor::new(bytes.clone());
            let mut archive = ZipArchive::new(cursor).map_err(|_| rquickjs::Error::Exception)?;

            let mut files_meta = Vec::new();
            let mut total_uncompressed: u64 = 0;
            let mut total_compressed: u64 = 0;

            for i in 0..archive.len() {
                let file = archive
                    .by_index(i)
                    .map_err(|_| rquickjs::Error::Exception)?;

                let size = file.size();
                let comp_size = file.compressed_size();
                total_uncompressed += size;
                total_compressed += comp_size;

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

            let res = json!({
                "total_size": bytes.len(),
                "total_uncompressed": total_uncompressed,
                "total_compressed_content": total_compressed,
                "file_count": archive.len(),
                "comment": String::from_utf8_lossy(archive.comment()).to_string(),
                "files": files_meta
            });

            to_value(js_ctx, &res).map_err(|_| rquickjs::Error::Exception)
        },
    )
    .map_err(|e| e.to_string())?;

    zip_obj
        .set("create", create_fn)
        .map_err(|e| e.to_string())?;
    zip_obj
        .set("extract", extract_fn)
        .map_err(|e| e.to_string())?;
    zip_obj
        .set("inspect", inspect_fn)
        .map_err(|e| e.to_string())?;

    globals.set("$zip", zip_obj).map_err(|e| e.to_string())?;
    Ok(())
}
