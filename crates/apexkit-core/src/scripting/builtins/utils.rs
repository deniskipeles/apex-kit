use boa_engine::{
    Context, JsArgs, JsError, JsString, JsValue, NativeFunction, object::ObjectInitializer,
    property::Attribute,
};

pub fn register_util(ctx: &mut Context) -> Result<(), String> {
    use crate::utils::{generate_random_hex, hmac_sha256, sha256, sha512, slugify};

    // UUID
    let uuid_fn = NativeFunction::from_fn_ptr(|_, _, _| {
        Ok(JsValue::from(JsString::from(
            uuid::Uuid::new_v4().to_string(),
        )))
    });

    // Slug
    let slug_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        Ok(JsValue::from(JsString::from(slugify(
            &args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped(),
        ))))
    });

    // Hash (SHA256 / SHA512)
    let hash_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let alg = args
            .get_or_undefined(1)
            .to_string(ctx)?
            .to_std_string_escaped();

        let result = match alg.as_str() {
            "sha256" => sha256(&text),
            "sha512" => sha512(&text),
            _ => {
                return Err(JsError::from_opaque(
                    JsString::from("Unsupported algorithm (use sha256/sha512)").into(),
                ));
            }
        };
        Ok(JsValue::from(JsString::from(result)))
    });

    // HMAC
    let hmac_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let key = args
            .get_or_undefined(1)
            .to_string(ctx)?
            .to_std_string_escaped();
        Ok(JsValue::from(JsString::from(hmac_sha256(&key, &text))))
    });

    // Base64 Encode
    let b64_enc_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        Ok(JsValue::from(JsString::from(STANDARD.encode(text))))
    });

    // Base64 Decode (Robust: Handles Standard, URL-Safe, Padded, and Unpadded)
    let b64_dec_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let text = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        use base64::{
            Engine as _,
            engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
        };

        // Try multiple decoding engines gracefully
        let decoded = STANDARD
            .decode(&text)
            .or_else(|_| URL_SAFE_NO_PAD.decode(&text))
            .or_else(|_| URL_SAFE.decode(&text))
            .or_else(|_| STANDARD_NO_PAD.decode(&text));

        match decoded {
            Ok(bytes) => {
                let s = String::from_utf8(bytes).unwrap_or_default();
                Ok(JsValue::from(JsString::from(s)))
            }
            Err(_) => Err(JsError::from_opaque(
                JsString::from("Invalid Base64 format").into(),
            )),
        }
    });

    // Sleep (Mock/Blocking)
    let sleep_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let ms = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0) as u64;
        std::thread::sleep(std::time::Duration::from_millis(ms));
        Ok(JsValue::undefined())
    });

    // Random Hex
    let random_hex_fn = NativeFunction::from_copy_closure(move |_, args, ctx| {
        let len = args.get_or_undefined(0).to_number(ctx).unwrap_or(16.0) as usize;
        Ok(JsValue::from(JsString::from(generate_random_hex(len))))
    });

    let obj = ObjectInitializer::new(ctx)
        .function(uuid_fn, JsString::from("uuid"), 0)
        .function(slug_fn, JsString::from("slugify"), 1)
        .function(hash_fn, JsString::from("hash"), 2)
        .function(hmac_fn, JsString::from("hmac"), 2)
        .function(b64_enc_fn, JsString::from("base64Encode"), 1)
        .function(b64_dec_fn, JsString::from("base64Decode"), 1)
        .function(sleep_fn, JsString::from("sleep"), 1)
        .function(random_hex_fn, JsString::from("randomHex"), 1)
        .build();

    ctx.register_global_property(JsString::from("$util"), obj, Attribute::all())
        .map_err(|e| e.to_string())
}
