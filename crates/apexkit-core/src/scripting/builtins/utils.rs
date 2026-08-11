use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
};
use rquickjs::{Ctx, Function, Object, function::Async};
use std::sync::Arc;

use super::super::context::ScriptContext;
use crate::utils::{generate_random_hex, hmac_sha256, sha256, sha512, slugify};

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

    // 3. $util.hash(text, alg) -> String
    let hash_fn = Function::new(
        ctx.clone(),
        move |text: String, alg: String| -> rquickjs::Result<String> {
            match alg.to_lowercase().as_str() {
                "sha256" => Ok(sha256(&text)),
                "sha512" => Ok(sha512(&text)),
                _ => Err(rquickjs::Error::Exception),
            }
        },
    )
    .map_err(|e| e.to_string())?;

    // 4. $util.hmac(text, key) -> String
    let hmac_fn = Function::new(
        ctx.clone(),
        move |text: String, key: String| -> rquickjs::Result<String> {
            Ok(hmac_sha256(&key, &text))
        },
    )
    .map_err(|e| e.to_string())?;

    // 5. $util.base64Encode(text) -> String
    let b64_enc_fn = Function::new(
        ctx.clone(),
        move |text: String| -> rquickjs::Result<String> { Ok(STANDARD.encode(text)) },
    )
    .map_err(|e| e.to_string())?;

    // 6. $util.base64Decode(text) -> String (Handles standard, url-safe, padded, and unpadded)
    let b64_dec_fn = Function::new(
        ctx.clone(),
        move |text: String| -> rquickjs::Result<String> {
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
                Err(_) => Err(rquickjs::Error::Exception),
            }
        },
    )
    .map_err(|e| e.to_string())?;

    // 7. $util.sleep(ms) -> Promise<void> (Non-blocking async sleep)
    let sleep_fn = Function::new(
        ctx.clone(),
        Async(move |ms_opt: Option<u64>| async move {
            let ms = ms_opt.unwrap_or(0);
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            Ok::<(), rquickjs::Error>(())
        }),
    )
    .map_err(|e| e.to_string())?;

    // 8. $util.randomHex(len) -> String
    let random_hex_fn = Function::new(
        ctx.clone(),
        move |len_opt: Option<usize>| -> rquickjs::Result<String> {
            let len = len_opt.unwrap_or(16);
            Ok(generate_random_hex(len))
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
        .set("base64Decode", b64_dec_fn)
        .map_err(|e| e.to_string())?;
    util_obj.set("sleep", sleep_fn).map_err(|e| e.to_string())?;
    util_obj
        .set("randomHex", random_hex_fn)
        .map_err(|e| e.to_string())?;

    globals.set("$util", util_obj).map_err(|e| e.to_string())?;
    Ok(())
}
