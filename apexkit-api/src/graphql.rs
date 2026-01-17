use async_graphql::{dynamic::*, Value as GqlValue};
use async_graphql::dataloader::*;
use apexkit_core::{Db, schema::{FieldType, RelationType}, Record, ListResult, auth::User}; 
use crate::AppState;
use std::sync::Arc;
use std::collections::HashMap;
use regex::Regex;
use serde::Deserialize;
use tracing::{warn, info};
use apexkit_core::realtime::EventScope;

// --- DATALOADERS ---

pub struct UserLoader {
    db: Arc<dyn Db>,
}

impl Loader<i64> for UserLoader {
    type Value = User;
    type Error = String;

    async fn load(&self, keys: &[i64]) -> Result<HashMap<i64, Self::Value>, Self::Error> {
        let users = self.db.get_users_by_ids(keys).await.map_err(|e| e.to_string())?;
        Ok(users.into_iter().map(|u| (u.id, u)).collect())
    }
}

pub struct RelationLoader {
    db: Arc<dyn Db>,
}

impl RelationLoader {
    pub fn new(db: Arc<dyn Db>) -> Self {
        Self { db }
    }
}

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub struct RelationKey {
    pub origin_col_id: i64,
    pub origin_rec_id: i64,
    pub rel_name: String,
    pub target_col_name: String,
}

impl Loader<RelationKey> for RelationLoader {
    type Value = Vec<Record>;
    type Error = String;
    
    async fn load(&self, keys: &[RelationKey]) -> std::result::Result<HashMap<RelationKey, Self::Value>, Self::Error> {
        let keys_cloned = keys.to_vec();
        let mut results = HashMap::new();
        let mut grouped_keys: HashMap<(i64, String), Vec<i64>> = HashMap::new();
        
        for key in &keys_cloned {
            grouped_keys.entry((key.origin_col_id, key.rel_name.clone())).or_default().push(key.origin_rec_id);
        }
        
        let mut target_ids_map: HashMap<RelationKey, Vec<(i64, i64)>> = HashMap::new();
        let mut needed_records: HashMap<String, Vec<i64>> = HashMap::new();

        for ((o_col, rel), o_ids) in grouped_keys {
            for o_id in o_ids {
                if let Ok(links) = self.db.get_related_ids(o_col, o_id, &rel).await {
                    if let Some(key) = keys_cloned.iter().find(|k| k.origin_col_id == o_col && k.origin_rec_id == o_id && k.rel_name == rel) {
                        target_ids_map.insert(key.clone(), links.clone());
                        for (_, t_rec_id) in links {
                            needed_records.entry(key.target_col_name.clone()).or_default().push(t_rec_id);
                        }
                    }
                }
            }
        }

        let cols = self.db.list_collections().await.map_err(|e| e.to_string())?;
        let col_name_to_id: HashMap<String, i64> = cols.into_iter().map(|c| (c.name, c.id)).collect();
        let mut fetched_record_cache: HashMap<(String, i64), Record> = HashMap::new();

        for (t_col_name, t_ids) in needed_records {
            if let Some(t_col_id) = col_name_to_id.get(&t_col_name) {
                if let Ok(recs) = self.db.get_records_by_ids(*t_col_id, &t_ids).await {
                    for r in recs { fetched_record_cache.insert((t_col_name.clone(), r.id), r); }
                }
            }
        }

        for key in keys_cloned {
            let mut records = Vec::new();
            if let Some(links) = target_ids_map.get(&key) {
                for (_, t_rec_id) in links {
                    if let Some(rec) = fetched_record_cache.get(&(key.target_col_name.clone(), *t_rec_id)) {
                        records.push(rec.clone());
                    }
                }
            }
            results.insert(key, records);
        }
        Ok(results)
    }
}

// --- CONFIG EXTRACTION LOGIC ---

#[derive(Deserialize, Debug, Clone)]
struct GraphqlConfig {
    parent: String, 
    name: String,
    args: Option<HashMap<String, String>>,
    #[serde(rename = "returnType")]
    return_type: String,
}

fn extract_script_config(code: &str) -> Option<GraphqlConfig> {
    // 1. Capture the JS object block: export const graphql = { ... };
    let re = Regex::new(r"export\s+const\s+graphql\s*=\s*(\{[\s\S]*?\})(?:;|\n|$)").ok()?;
    
    if let Some(caps) = re.captures(code) {
        let mut json_str = caps.get(1)?.as_str().to_string();

        // 2. Remove JS Comments
        if let Ok(re_comments) = Regex::new(r"//.*|/\*[\s\S]*?\*/") {
            json_str = re_comments.replace_all(&json_str, "").to_string();
        }

        // 3. Quote unquoted keys (JS Object -> JSON)
        if let Ok(re_keys) = Regex::new(r"(?m)(^|[\s{,])([a-zA-Z_]\w*)\s*:") {
            json_str = re_keys.replace_all(&json_str, r#"$1"$2":"#).to_string();
        }

        // 4. Remove trailing commas
        if let Ok(re_trailing) = Regex::new(r",\s*([\]}])") {
            json_str = re_trailing.replace_all(&json_str, "$1").to_string();
        }

        match serde_json::from_str::<GraphqlConfig>(&json_str) {
            Ok(cfg) => return Some(cfg),
            Err(e) => {
                warn!("Found 'graphql' config but failed to parse JSON: {}. \nSanitized: {}", e, json_str);
                return None;
            }
        }
    }
    None
}

fn map_type_ref(type_name: &str) -> TypeRef {
    let is_non_null = type_name.ends_with('!');
    let clean_name = type_name.trim_end_matches('!');
    
    let is_list = clean_name.starts_with('[') && clean_name.ends_with(']');
    let inner_name = if is_list { 
        clean_name.trim_start_matches('[').trim_end_matches(']') 
    } else { 
        clean_name 
    };

    let base_ref = match inner_name {
        "String" => TypeRef::named(TypeRef::STRING),
        "Int" => TypeRef::named(TypeRef::INT),
        "Float" => TypeRef::named(TypeRef::FLOAT),
        "Boolean" => TypeRef::named(TypeRef::BOOLEAN),
        "ID" => TypeRef::named(TypeRef::ID),
        "JSON" => TypeRef::named("JSON"),
        _ => TypeRef::named(inner_name), 
    };

    let mut t_ref = if is_list { TypeRef::List(Box::new(base_ref)) } else { base_ref };
    if is_non_null { t_ref = TypeRef::NonNull(Box::new(t_ref)); }
    t_ref
}

// --- SCHEMA BUILDER ---

pub async fn build_schema(
    state: AppState, 
    loader: Arc<DataLoader<RelationLoader>>
) -> Result<Schema, SchemaError> {
    
    let user_loader = DataLoader::new(UserLoader { db: state.db.clone() }, tokio::spawn);

    let mut schema_builder = Schema::build("Query", Some("Mutation"), None);
    schema_builder = schema_builder.register(Scalar::new("JSON"));

    // --- 1. PREPARE STANDARD OBJECTS ---
    
    let mut query_root = Object::new("Query");
    let mut mutation_root = Object::new("Mutation");
    let mut user_object = Object::new("User");
    let mut collection_objects: HashMap<String, Object> = HashMap::new();

    // Standard Query Fields
    query_root = query_root.field(Field::new("status", TypeRef::named(TypeRef::STRING), |_| {
        FieldFuture::new(async { Ok(Some(GqlValue::from("ApexKit is running"))) })
    }));

    mutation_root = mutation_root.field(Field::new("ping", TypeRef::named(TypeRef::STRING), |_| {
        FieldFuture::new(async { Ok(Some(GqlValue::from("pong"))) })
    }));

    // Standard User Fields
    user_object = user_object
        .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| FieldFuture::new(async move {
            let u = ctx.parent_value.try_downcast_ref::<User>()?; 
            Ok(Some(GqlValue::from(u.id.to_string())))
        })))
        .field(Field::new("email", TypeRef::named_nn(TypeRef::STRING), |ctx| FieldFuture::new(async move {
            let u = ctx.parent_value.try_downcast_ref::<User>()?; 
            Ok(Some(GqlValue::from(u.email.clone())))
        })))
        .field(Field::new("role", TypeRef::named_nn(TypeRef::STRING), |ctx| FieldFuture::new(async move {
            let u = ctx.parent_value.try_downcast_ref::<User>()?; 
            Ok(Some(GqlValue::from(u.role.clone())))
        })));

    // Standard UserList
    let mut user_list = Object::new("UserList");
    user_list = user_list
        .field(Field::new("total", TypeRef::named_nn(TypeRef::INT), |ctx| FieldFuture::new(async move {
            let total = ctx.parent_value.try_downcast_ref::<(i64, Vec<User>)>()?.0; 
            Ok(Some(GqlValue::from(total)))
        })))
        .field(Field::new("items", TypeRef::named_nn_list("User"), |ctx| FieldFuture::new(async move {
            let items = &ctx.parent_value.try_downcast_ref::<(i64, Vec<User>)>()?.1; 
            Ok(Some(FieldValue::list(items.iter().map(|u| FieldValue::owned_any(u.clone())))))
        })));
    schema_builder = schema_builder.register(user_list);

    // Users Query
    query_root = query_root.field(Field::new("users", TypeRef::named("UserList"), move |ctx| {
        let state = ctx.data::<AppState>().unwrap().clone();
        FieldFuture::new(async move {
            let limit = ctx.args.get("limit").and_then(|v| v.i64().ok()).unwrap_or(20);
            let offset = ctx.args.get("offset").and_then(|v| v.i64().ok()).unwrap_or(0);
            let search = ctx.args.get("search").and_then(|v| v.string().ok()).map(|s| s.to_string());
            let total = state.db.count_users(search.clone()).await.map_err(|e| async_graphql::Error::new(e.to_string()))?;
            let items = state.db.list_users(search, limit, offset).await.map_err(|e| async_graphql::Error::new(e.to_string()))?;
            Ok(Some(FieldValue::owned_any((total, items))))
        })
    }).argument(InputValue::new("limit", TypeRef::named(TypeRef::INT))).argument(InputValue::new("offset", TypeRef::named(TypeRef::INT))).argument(InputValue::new("search", TypeRef::named(TypeRef::STRING))));

    // --- 2. BUILD COLLECTION OBJECTS ---
    let collections = state.db.list_collections().await.unwrap_or_default();
    let col_id_to_name: HashMap<String, String> = collections.iter().map(|c| (c.id.to_string(), c.name.clone())).collect();

    for col in &collections {
        let type_name = capitalize(&col.name);
        let mut object = Object::new(&type_name);

        object = object.field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move { 
                let record = ctx.parent_value.try_downcast_ref::<Record>()?; 
                Ok(Some(GqlValue::from(record.id.to_string()))) 
            })
        }));

        if let Some(schema) = &col.schema {
            for (field_name, def) in &schema.fields {
                let name_clone = field_name.clone();
                if def.r#type == FieldType::Owner {
                    let field = Field::new(field_name, TypeRef::named("User"), move |ctx| {
                        let name = name_clone.clone(); 
                        FieldFuture::new(async move { 
                            let record = ctx.parent_value.try_downcast_ref::<Record>()?; 
                            let user_id = record.data.get(&name).and_then(|v| v.as_i64()).or_else(|| record.data.get(&name).and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()));
                            if let Some(uid) = user_id {
                                let loader = ctx.data::<DataLoader<UserLoader>>().unwrap();
                                let user = loader.load_one(uid).await.map_err(|e| async_graphql::Error::new(e))?;
                                Ok(user.map(FieldValue::owned_any))
                            } else { Ok(None) }
                        })
                    });
                    object = object.field(field);
                    continue; 
                }

                let gql_type = match def.r#type {
                    FieldType::Number => TypeRef::FLOAT,
                    FieldType::Boolean => TypeRef::BOOLEAN,
                    _ => TypeRef::STRING,
                };
                let field = Field::new(field_name, TypeRef::named(gql_type), move |ctx| {
                    let name = name_clone.clone(); 
                    FieldFuture::new(async move { 
                        let record = ctx.parent_value.try_downcast_ref::<Record>()?; 
                        map_json_to_gql(record.data.get(&name).cloned()) 
                    })
                });
                object = object.field(field);
            }
            
            // Relations
            for (rel_name, rel_def) in &schema.relations {
                let raw_target = &rel_def.target_collection;
                let resolved_target_name = col_id_to_name.get(raw_target).unwrap_or(raw_target);
                let target_type_name = capitalize(resolved_target_name);
                let t_col_name = resolved_target_name.clone(); 
                let origin_col_id = col.id;
                let r_name = rel_name.clone();
                let is_list = rel_def.relation_type == RelationType::Many;
                let type_ref = if is_list { TypeRef::List(Box::new(TypeRef::named(&target_type_name))) } else { TypeRef::named(&target_type_name) };

                let field = Field::new(rel_name, type_ref, move |ctx| {
                    let r_name = r_name.clone();
                    let t_col_name = t_col_name.clone();
                    FieldFuture::new(async move {
                        let record = ctx.parent_value.try_downcast_ref::<Record>()?.clone(); 
                        let loader = ctx.data::<Arc<DataLoader<RelationLoader>>>().unwrap().clone();
                        let key = RelationKey { origin_col_id, origin_rec_id: record.id, rel_name: r_name, target_col_name: t_col_name };
                        let records = loader.load_one(key).await.map_err(|e| async_graphql::Error::new(e))?.unwrap_or_default();
                        if is_list { Ok(Some(FieldValue::list(records.into_iter().map(FieldValue::owned_any)))) } 
                        else { Ok(records.into_iter().next().map(FieldValue::owned_any)) }
                    })
                });
                object = object.field(field);
            }
        }
        
        collection_objects.insert(type_name.clone(), object);
        
        // List Type
        let list_type_name = format!("{}List", type_name);
        let mut list_object = Object::new(&list_type_name);
        list_object = list_object.field(Field::new("total", TypeRef::named_nn(TypeRef::INT), |ctx| FieldFuture::new(async move {
            let res = ctx.parent_value.try_downcast_ref::<ListResult>()?; 
            Ok(Some(GqlValue::from(res.total)))
        })));
        list_object = list_object.field(Field::new("items", TypeRef::named_nn_list(&type_name), move |ctx| FieldFuture::new(async move {
            let res = ctx.parent_value.try_downcast_ref::<ListResult>()?; 
            let items: Vec<FieldValue> = res.items.iter().map(|r| FieldValue::owned_any(r.clone())).collect();
            Ok(Some(FieldValue::list(items)))
        })));
        schema_builder = schema_builder.register(list_object);

        // Root Query Field
        let col_id = col.id;
        let query_name = col.name.clone();
        let list_field = Field::new(query_name, TypeRef::named(&list_type_name), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();
            FieldFuture::new(async move {
                let limit = ctx.args.get("limit").and_then(|v| v.u64().ok());
                let offset = ctx.args.get("offset").and_then(|v| v.u64().ok());
                let filter_str = match ctx.args.get("where") {
                    Some(accessor) => accessor.deserialize::<serde_json::Value>().ok().map(|j| j.to_string()),
                    None => None,
                };
                let mut options = apexkit_core::query::QueryOptions::default();
                if limit.is_some() || offset.is_some() { options.limit = limit; options.offset = offset; } 
                else { options.limit = Some(100); }
                options.filter = filter_str;
                let result = state.db.list_records(col_id, options).await.map_err(|e| async_graphql::Error::new(e.to_string()))?;
                Ok(Some(FieldValue::owned_any(result)))
            })
        })
        .argument(InputValue::new("limit", TypeRef::named(TypeRef::INT)))
        .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT)))
        .argument(InputValue::new("where", TypeRef::named("JSON")));

        query_root = query_root.field(list_field);
    }

    // --- 3. FETCH & INJECT CUSTOM RESOLVERS FROM SCRIPTS ---
    let scripts = state.db.list_scripts().await.unwrap_or_default();
    
    for script in scripts {
        // We look for any script that has the "graphql" trigger
        if script.trigger_type == "graphql" && script.active {
            if let Some(config) = extract_script_config(&script.code) {
                let script_name = script.name.clone();
                let args_def = config.args.clone().unwrap_or_default();
                let return_type = map_type_ref(&config.return_type);
                
                let field_name = config.name.clone();

                let mut field = Field::new(field_name, return_type, move |ctx| {
                    let s_name = script_name.clone();
                    let a_def = args_def.clone();
                    FieldFuture::new(async move {
                        let state = ctx.data::<AppState>().unwrap();
                        
                        // 1. Collect Args
                        let mut script_input = serde_json::Map::new();
                        for (arg_name, _) in &a_def {
                            if let Some(val) = ctx.args.get(arg_name) {
                                let json_val = val.deserialize::<serde_json::Value>().unwrap_or(serde_json::Value::Null);
                                script_input.insert(arg_name.clone(), json_val);
                            }
                        }

                        // 2. Capture Parent Data
                        let parent_data = if let Ok(rec) = ctx.parent_value.try_downcast_ref::<Record>() {
                            Some(serde_json::json!({ "id": rec.id, "data": rec.data }))
                        } else if let Ok(u) = ctx.parent_value.try_downcast_ref::<User>() {
                            Some(serde_json::json!({ "id": u.id, "email": u.email }))
                        } else { None };

                        if let Some(p) = parent_data { script_input.insert("parent".to_string(), p); }

                        // 3. Execute
                        let script_record = state.db.get_script_by_name(&s_name).await
                            .map_err(|e| async_graphql::Error::new(e.to_string()))?
                            .ok_or_else(|| async_graphql::Error::new("Linked script not found"))?;
                        let event_scope = ctx.data::<EventScope>().unwrap_or(&EventScope::Root).clone();

                        let result = state.script_engine.run_script(
                            &script_record.code,
                            serde_json::Value::Object(script_input), 
                            Arc::new(state.clone()), // Pass AppState as ScriptContext
                            None,
                            event_scope
                        ).await.map_err(|e| async_graphql::Error::new(e))?;

                        // FIX: Use `FieldValue::value` + `json_to_gql` for scalars
                        Ok(Some(FieldValue::value(json_to_gql(result))))
                    })
                });

                // Add Arguments
                if let Some(args) = config.args {
                    for (k, v) in args {
                        field = field.argument(InputValue::new(k, map_type_ref(&v)));
                    }
                }

                // Attach to Parent
                match config.parent.as_str() {
                    "Query" => query_root = query_root.field(field),
                    "Mutation" => mutation_root = mutation_root.field(field),
                    "User" => user_object = user_object.field(field),
                    other => {
                        if let Some(obj) = collection_objects.get_mut(other) {
                            let new_obj = std::mem::replace(obj, Object::new(other));
                            *obj = new_obj.field(field);
                        } else {
                            info!("Skipping GraphQL script {}: Parent '{}' not found", script.name, other);
                        }
                    }
                }
            }
        }
    }

    // --- 4. REGISTER ALL OBJECTS ---
    for (_, obj) in collection_objects {
        schema_builder = schema_builder.register(obj);
    }
    
    schema_builder
        .register(user_object)
        .register(query_root)
        .register(mutation_root)
        .data(state)
        .data(loader)
        .data(user_loader) 
        .finish()
}


fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn map_json_to_gql(val: Option<serde_json::Value>) -> async_graphql::Result<Option<GqlValue>> {
    match val {
        Some(serde_json::Value::String(s)) => Ok(Some(GqlValue::from(s))),
        Some(serde_json::Value::Number(n)) => { if let Some(f) = n.as_f64() { Ok(Some(GqlValue::from(f))) } else { Ok(Some(GqlValue::from(0))) } },
        Some(serde_json::Value::Bool(b)) => Ok(Some(GqlValue::from(b))),
        Some(_) => Ok(Some(GqlValue::from("Complex JSON"))),
        None => Ok(None),
    }
}

// [NEW] Recursive JSON -> GraphQL Value Converter
fn json_to_gql(json: serde_json::Value) -> GqlValue {
    match json {
        serde_json::Value::Null => GqlValue::Null,
        serde_json::Value::Bool(b) => GqlValue::Boolean(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                GqlValue::from(i)
            } else if let Some(f) = n.as_f64() {
                GqlValue::from(f)
            } else {
                GqlValue::String(n.to_string())
            }
        },
        serde_json::Value::String(s) => GqlValue::String(s),
        serde_json::Value::Array(arr) => {
            GqlValue::List(arr.into_iter().map(json_to_gql).collect())
        },
        serde_json::Value::Object(obj) => {
            let mut map = async_graphql::indexmap::IndexMap::new();
            for (k, v) in obj {
                map.insert(async_graphql::Name::new(k), json_to_gql(v));
            }
            GqlValue::Object(map)
        }
    }
}