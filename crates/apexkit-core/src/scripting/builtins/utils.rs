use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
};
use rquickjs::{Ctx, Exception, Function, Object, Value, function::Async};
use std::sync::Arc;

use super::super::context::ScriptContext;
use crate::utils::{generate_random_hex, slugify};

fn throw_err<'js, T>(ctx: &Ctx<'js>, msg: &str) -> rquickjs::Result<T> {
    let err = Exception::from_message(ctx.clone(), msg).unwrap();
    Err(ctx.throw(err.into()))
}

fn extract_bytes<'js>(val: Value<'js>) -> Vec<u8> {
    if let Ok(ta) = rquickjs::TypedArray::<u8>::from_value(val.clone()) {
        if let Some(bytes) = ta.as_bytes() {
            return bytes.to_vec();
        }
    }
    if let Some(ab) = rquickjs::ArrayBuffer::from_value(val.clone()) {
        if let Some(bytes) = ab.as_bytes() {
            return bytes.to_vec();
        }
    }
    if let Some(obj) = val.as_object() {
        if let Ok(ab) = obj.get::<_, rquickjs::ArrayBuffer>("buffer") {
            let offset = obj.get::<_, usize>("byteOffset").unwrap_or(0);
            let length = obj
                .get::<_, usize>("byteLength")
                .unwrap_or_else(|_| ab.as_bytes().map(|b| b.len()).unwrap_or(0));
            if let Some(bytes) = ab.as_bytes() {
                if offset + length <= bytes.len() {
                    return bytes[offset..offset + length].to_vec();
                }
            }
        }
    }
    if let Some(s) = val.as_string() {
        let s_str = s.to_string().unwrap_or_default();
        return s_str.as_bytes().to_vec();
    }
    vec![]
}

pub fn register_util<'js>(ctx: &Ctx<'js>, _app_ctx: Arc<dyn ScriptContext>) -> Result<(), String> {
    let globals = ctx.globals();
    let util_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    // 1. $util.uuid() -> String
    let uuid_fn = Function::new(ctx.clone(), move || -> rquickjs::Result<String> {
        Ok(uuid::Uuid::new_v4().to_string())
    })
    .map_err(|e| e.to_string())?;

    // 2. $util.slugify(text) -> String
    let slug_fn = Function::new(
        ctx.clone(),
        move |text: String| -> rquickjs::Result<String> { Ok(slugify(&text)) },
    )
    .map_err(|e| e.to_string())?;

    // 3. $util.hash(data, alg) -> String (hex)
    let hash_fn = Function::new(
        ctx.clone(),
        move |js_ctx: Ctx<'js>, val: Value<'js>, alg: String| -> rquickjs::Result<String> {
            let bytes = extract_bytes(val);
            match alg.to_lowercase().replace("-", "").as_str() {
                "sha256" => {
                    let hash = ring::digest::digest(&ring::digest::SHA256, &bytes);
                    Ok(crate::utils::to_hex(hash.as_ref()))
                }
                "sha384" => {
                    let hash = ring::digest::digest(&ring::digest::SHA384, &bytes);
                    Ok(crate::utils::to_hex(hash.as_ref()))
                }
                "sha512" => {
                    let hash = ring::digest::digest(&ring::digest::SHA512, &bytes);
                    Ok(crate::utils::to_hex(hash.as_ref()))
                }
                other => throw_err(
                    &js_ctx,
                    &format!(
                        "Unsupported hash algorithm '{}'. Supported: 'sha256', 'sha384', 'sha512'",
                        other
                    ),
                ),
            }
        },
    )
    .map_err(|e| e.to_string())?;

    // 4. $util.hmac(data, key) -> String (hex)
    let hmac_fn = Function::new(
        ctx.clone(),
        move |val: Value<'js>, key_val: Value<'js>| -> rquickjs::Result<String> {
            let data_bytes = extract_bytes(val);
            let key_bytes = extract_bytes(key_val);
            let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &key_bytes);
            let tag = ring::hmac::sign(&key, &data_bytes);
            Ok(crate::utils::to_hex(tag.as_ref()))
        },
    )
    .map_err(|e| e.to_string())?;

    // 5. $util.base64Encode(data) -> String
    let b64_enc_fn = Function::new(
        ctx.clone(),
        move |val: Value<'js>| -> rquickjs::Result<String> {
            let bytes = extract_bytes(val);
            Ok(STANDARD.encode(&bytes))
        },
    )
    .map_err(|e| e.to_string())?;

    // 6. $util.base64EncodeBuffer(buffer) -> String
    let b64_enc_buf_fn = Function::new(
        ctx.clone(),
        move |val: Value<'js>| -> rquickjs::Result<String> {
            let bytes = extract_bytes(val);
            Ok(STANDARD.encode(&bytes))
        },
    )
    .map_err(|e| e.to_string())?;

    // 7. $util.base64Decode(text) -> String
    let b64_dec_fn = Function::new(
        ctx.clone(),
        move |js_ctx: Ctx<'js>, text: String| -> rquickjs::Result<String> {
            let decoded = STANDARD
                .decode(&text)
                .or_else(|_| URL_SAFE_NO_PAD.decode(&text))
                .or_else(|_| URL_SAFE.decode(&text))
                .or_else(|_| STANDARD_NO_PAD.decode(&text));

            match decoded {
                Ok(bytes) => {
                    let s = String::from_utf8(bytes).unwrap_or_default();
                    Ok(s)
                }
                Err(e) => throw_err(&js_ctx, &format!("Base64 decoding failed: {}", e)),
            }
        },
    )
    .map_err(|e| e.to_string())?;

    // 8. $util.base64DecodeBuffer(text) -> ArrayBuffer
    let b64_dec_buf_fn = Function::new(
        ctx.clone(),
        move |js_ctx: Ctx<'js>, text: String| -> rquickjs::Result<rquickjs::ArrayBuffer<'js>> {
            let clean = text
                .trim()
                .trim_start_matches("data:image/jpeg;base64,")
                .trim_start_matches("data:image/png;base64,")
                .trim_start_matches("data:image/webp;base64,")
                .trim_start_matches("data:application/octet-stream;base64,");

            let decoded = STANDARD
                .decode(clean)
                .or_else(|_| URL_SAFE_NO_PAD.decode(clean))
                .or_else(|_| URL_SAFE.decode(clean))
                .or_else(|_| STANDARD_NO_PAD.decode(clean));

            match decoded {
                Ok(bytes) => rquickjs::ArrayBuffer::new(js_ctx.clone(), bytes).map_err(|e| {
                    let err = Exception::from_message(js_ctx.clone(), &e.to_string()).unwrap();
                    js_ctx.throw(err.into())
                }),
                Err(e) => throw_err(&js_ctx, &format!("Invalid Base64 buffer: {}", e)),
            }
        },
    )
    .map_err(|e| e.to_string())?;

    // 9. $util.sleep(ms) -> Promise<void>
    let sleep_fn = Function::new(
        ctx.clone(),
        Async(move |ms_opt: Option<u64>| async move {
            let ms = ms_opt.unwrap_or(0);
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            Ok::<(), rquickjs::Error>(())
        }),
    )
    .map_err(|e| e.to_string())?;

    // 10. $util.randomHex(len) -> String
    let random_hex_fn = Function::new(
        ctx.clone(),
        move |len_opt: Option<usize>| -> rquickjs::Result<String> {
            let len = len_opt.unwrap_or(16);
            Ok(generate_random_hex(len))
        },
    )
    .map_err(|e| e.to_string())?;

    // 11. $util.textEncode(string) -> ArrayBuffer
    let text_encode_fn = Function::new(
        ctx.clone(),
        move |js_ctx: Ctx<'js>, text: String| -> rquickjs::Result<rquickjs::ArrayBuffer<'js>> {
            rquickjs::ArrayBuffer::new(js_ctx, text.as_bytes().to_vec())
        },
    )
    .map_err(|e| e.to_string())?;

    // 12. $util.textDecode(buffer) -> String
    let text_decode_fn = Function::new(
        ctx.clone(),
        move |val: Value<'js>| -> rquickjs::Result<String> {
            let bytes = extract_bytes(val);
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        },
    )
    .map_err(|e| e.to_string())?;

    // 13. $util.hexEncode(buffer) -> String
    let hex_enc_fn = Function::new(
        ctx.clone(),
        move |val: Value<'js>| -> rquickjs::Result<String> {
            let bytes = extract_bytes(val);
            Ok(crate::utils::to_hex(&bytes))
        },
    )
    .map_err(|e| e.to_string())?;

    // 14. $util.hexDecode(string) -> ArrayBuffer
    let hex_dec_fn = Function::new(
        ctx.clone(),
        move |js_ctx: Ctx<'js>, hex: String| -> rquickjs::Result<rquickjs::ArrayBuffer<'js>> {
            let clean = hex.trim();
            let bytes = (0..clean.len())
                .step_by(2)
                .filter_map(|i| {
                    if i + 2 <= clean.len() {
                        u8::from_str_radix(&clean[i..i + 2], 16).ok()
                    } else {
                        None
                    }
                })
                .collect::<Vec<u8>>();
            rquickjs::ArrayBuffer::new(js_ctx, bytes)
        },
    )
    .map_err(|e| e.to_string())?;

    util_obj.set("uuid", uuid_fn).map_err(|e| e.to_string())?;
    util_obj
        .set("slugify", slug_fn)
        .map_err(|e| e.to_string())?;
    util_obj.set("hash", hash_fn).map_err(|e| e.to_string())?;
    util_obj.set("hmac", hmac_fn).map_err(|e| e.to_string())?;
    util_obj
        .set("base64Encode", b64_enc_fn)
        .map_err(|e| e.to_string())?;
    util_obj
        .set("base64EncodeBuffer", b64_enc_buf_fn)
        .map_err(|e| e.to_string())?;
    util_obj
        .set("base64Decode", b64_dec_fn)
        .map_err(|e| e.to_string())?;
    util_obj
        .set("base64DecodeBuffer", b64_dec_buf_fn)
        .map_err(|e| e.to_string())?;
    util_obj.set("sleep", sleep_fn).map_err(|e| e.to_string())?;
    util_obj
        .set("randomHex", random_hex_fn)
        .map_err(|e| e.to_string())?;
    util_obj
        .set("textEncode", text_encode_fn)
        .map_err(|e| e.to_string())?;
    util_obj
        .set("textDecode", text_decode_fn)
        .map_err(|e| e.to_string())?;
    util_obj
        .set("hexEncode", hex_enc_fn)
        .map_err(|e| e.to_string())?;
    util_obj
        .set("hexDecode", hex_dec_fn)
        .map_err(|e| e.to_string())?;

    globals.set("$util", util_obj).map_err(|e| e.to_string())?;
    Ok(())
}
