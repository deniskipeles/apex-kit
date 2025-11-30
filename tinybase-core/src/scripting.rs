// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/scripting.rs ===========================
use std::sync::Arc;
use serde_json::Value as JsonValue;
use crate::Db;

// Boa Imports
use boa_engine::{
    Context, JsValue, Source, JsResult, NativeFunction, 
    object::ObjectInitializer, property::Attribute,
    JsString
};

#[derive(Clone)]
pub struct ScriptEngine;

impl ScriptEngine {
    pub async fn new() -> Self {
        Self
    }

    pub async fn run_script(
        &self, 
        code: &str, 
        input_data: JsonValue, 
        db: Arc<dyn Db>
    ) -> Result<JsonValue, String> {
        
        let code_owned = code.to_string();
        let handle = tokio::runtime::Handle::current();
        
        // Spawn a blocking thread for the JS Engine
        let result = tokio::task::spawn_blocking(move || -> Result<JsonValue, String> {
            // 1. Initialize Boa Context
            let mut context = Context::default();

            // 2. Setup Helper for Native Functions
            let create_fn = |context: &mut Context, f: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>| -> JsValue {
                let native = NativeFunction::from_fn_ptr(f);
                JsValue::new(boa_engine::object::FunctionObjectBuilder::new(context.realm(), native).build())
            };

            // --- REGISTER $input ---
            match JsValue::from_json(&input_data, &mut context) {
                Ok(js_input) => {
                    let key = JsString::from("$input");
                    if let Err(e) = context.register_global_property(key, js_input, Attribute::all()) {
                        return Err(format!("Failed to register $input: {}", e));
                    }
                },
                Err(e) => return Err(format!("Input Serialization Error: {}", e)),
            }

            // --- REGISTER log() ---
            fn log_impl(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
                let msg = args.get(0)
                    .map(|v| v.to_string(ctx).unwrap_or_default().to_std_string_escaped())
                    .unwrap_or_default();
                println!("[JS LOG]: {}", msg);
                Ok(JsValue::undefined())
            }
            let log_js = create_fn(&mut context, log_impl);
            let log_key = JsString::from("log");
            let _ = context.register_global_property(log_key, log_js, Attribute::all());

            // --- REGISTER $util ---
            fn util_uuid(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
                let id = uuid::Uuid::new_v4().to_string();
                Ok(JsValue::from(JsString::from(id)))
            }
            let uuid_js = create_fn(&mut context, util_uuid);
            
            let mut util_builder = ObjectInitializer::new(&mut context);
            util_builder.property(JsString::from("uuid"), uuid_js, Attribute::all());
            let util_obj = util_builder.build();
            
            let util_key = JsString::from("$util");
            let _ = context.register_global_property(util_key, util_obj, Attribute::all());

            // --- REGISTER $http ---
            fn http_get(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
                let url_val = args.get(0).unwrap_or(&JsValue::undefined()).clone();
                let url = match url_val.to_string(ctx) {
                    Ok(s) => s.to_std_string_escaped(),
                    Err(_) => return Ok(JsValue::null())
                };
                
                println!("[SCRIPT DEBUG] HTTP GET {}", url);
                match reqwest::blocking::get(&url) {
                    Ok(res) => Ok(JsValue::from(JsString::from(res.text().unwrap_or_default()))),
                    Err(e) => { println!("[SCRIPT ERROR] HTTP GET: {}", e); Ok(JsValue::null()) }
                }
            }

            fn http_post(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
                let url_val = args.get(0).unwrap_or(&JsValue::undefined()).clone();
                let url = match url_val.to_string(ctx) {
                    Ok(s) => s.to_std_string_escaped(),
                    Err(_) => return Ok(JsValue::null())
                };

                let body_val = args.get(1).unwrap_or(&JsValue::undefined()).clone();
                
                // FIX: Handle Result<Option<Value>> -> Value
                let body_json = body_val.to_json(ctx)
                    .unwrap_or(None)
                    .unwrap_or(serde_json::Value::Null);

                println!("[SCRIPT DEBUG] HTTP POST {}", url);
                
                let client = reqwest::blocking::Client::new();
                match client.post(&url).json(&body_json).send() {
                    Ok(res) => Ok(JsValue::from(JsString::from(res.text().unwrap_or_default()))),
                    Err(e) => { println!("[SCRIPT ERROR] HTTP POST: {}", e); Ok(JsValue::null()) }
                }
            }

            let get_js = create_fn(&mut context, http_get);
            let post_js = create_fn(&mut context, http_post);
            
            let mut http_builder = ObjectInitializer::new(&mut context);
            http_builder.property(JsString::from("get"), get_js, Attribute::all());
            http_builder.property(JsString::from("post"), post_js, Attribute::all());
            let http_obj = http_builder.build();

            let http_key = JsString::from("$http");
            let _ = context.register_global_property(http_key, http_obj, Attribute::all());

            // --- REGISTER $db (The Bridge) ---
            ACTIVE_DB_CONTEXT.with(|c| {
                *c.borrow_mut() = Some((db.clone(), handle.clone()));
            });

            fn db_find_one(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
                let col_name = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped()).unwrap_or_default();
                let id = args.get(1).and_then(|v| v.as_number()).map(|n| n as i64).unwrap_or(0);

                let result_json = ACTIVE_DB_CONTEXT.with(|c| {
                    if let Some((db, handle)) = &*c.borrow() {
                        handle.block_on(async {
                            if let Ok(cols) = db.list_collections().await {
                                if let Some(col) = cols.iter().find(|c| c.name == col_name) {
                                    if let Ok(Some(rec)) = db.get_record(col.id, id).await {
                                        let mut data = rec.data.clone();
                                        if let Some(obj) = data.as_object_mut() {
                                            obj.insert("id".to_string(), serde_json::json!(rec.id));
                                        }
                                        return Some(data);
                                    }
                                }
                            }
                            None
                        })
                    } else { None }
                });

                match result_json {
                    Some(val) => JsValue::from_json(&val, ctx),
                    None => Ok(JsValue::null())
                }
            }

            fn db_insert(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
                let col_name = args.get(0).and_then(|v| v.as_string()).map(|s| s.to_std_string_escaped()).unwrap_or_default();
                let data_val = args.get(1).unwrap_or(&JsValue::undefined()).clone();
                
                // FIX: Handle Result<Option<Value>> -> Value
                let data_json = data_val.to_json(ctx)
                    .unwrap_or(None)
                    .unwrap_or(serde_json::Value::Null);

                let new_id = ACTIVE_DB_CONTEXT.with(|c| {
                    if let Some((db, handle)) = &*c.borrow() {
                        handle.block_on(async {
                            if let Ok(cols) = db.list_collections().await {
                                if let Some(col) = cols.iter().find(|c| c.name == col_name) {
                                    // data_json is now &Value, which matches &serde_json::Value
                                    if let Ok(id) = db.create_record(col.id, &data_json).await {
                                        return Some(id);
                                    }
                                }
                            }
                            None
                        })
                    } else { None }
                });

                match new_id {
                    Some(id) => Ok(JsValue::from(id)),
                    None => Ok(JsValue::null())
                }
            }

            let find_one_js = create_fn(&mut context, db_find_one);
            let insert_js = create_fn(&mut context, db_insert);

            let mut db_builder = ObjectInitializer::new(&mut context);
            db_builder.property(JsString::from("find_one"), find_one_js, Attribute::all());
            db_builder.property(JsString::from("insert"), insert_js, Attribute::all());
            let db_obj = db_builder.build();

            let db_key = JsString::from("$db");
            let _ = context.register_global_property(db_key, db_obj, Attribute::all());

            // --- EXECUTE ---
            let res_result = context.eval(Source::from_bytes(code_owned.as_bytes()));
            
            ACTIVE_DB_CONTEXT.with(|c| *c.borrow_mut() = None);

            match res_result {
                Ok(res) => {
                    // FIX: Handle Result<Option<Value>> -> Ok(Value)
                    match res.to_json(&mut context) {
                        Ok(json_opt) => Ok(json_opt.unwrap_or(JsonValue::Null)),
                        Err(e) => Err(format!("Output Serialization Error: {}", e))
                    }
                },
                Err(e) => Err(format!("Runtime Error: {}", e))
            }

        }).await;

        match result {
            Ok(inner) => inner,
            Err(e) => Err(format!("System Error (Panic?): {}", e)),
        }
    }
}

// Thread-local storage to pass the DB connection into the Boa Native Functions
use std::cell::RefCell;
thread_local! {
    static ACTIVE_DB_CONTEXT: RefCell<Option<(Arc<dyn Db>, tokio::runtime::Handle)>> = RefCell::new(None);
}
