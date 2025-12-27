// =========================== /teamspace/studios/this_studio/apex/apex-kit/apexkit-api/src/graphql.rs ===========================
use async_graphql::{dynamic::*, Value as GqlValue};
use async_graphql::dataloader::*;
use apexkit_core::{Db, schema::{FieldType, RelationType}, Record, ListResult, auth::User}; 
use crate::AppState;
use std::sync::Arc;
use std::collections::HashMap;

// ... (UserLoader, RelationLoader, RelationKey structs remain the same) ...
// --- 1. USER DATALOADER ---
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

// --- 2. RELATION DATALOADER ---
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
        let mut results: HashMap<RelationKey, Vec<Record>> = HashMap::new();
        let mut grouped_keys: HashMap<(i64, String), Vec<i64>> = HashMap::new();
        
        for key in &keys_cloned {
            grouped_keys.entry((key.origin_col_id, key.rel_name.clone()))
                .or_default()
                .push(key.origin_rec_id);
        }
        
        let mut target_ids_map: HashMap<RelationKey, Vec<(i64, i64)>> = HashMap::new();
        let mut needed_records: HashMap<String, Vec<i64>> = HashMap::new();

        for ((o_col, rel), o_ids) in grouped_keys {
            for o_id in o_ids {
                let links = self.db.get_related_ids(o_col, o_id, &rel)
                    .await
                    .map_err(|e| e.to_string())?;
                
                if let Some(key) = keys_cloned.iter().find(|k| k.origin_col_id == o_col && k.origin_rec_id == o_id && k.rel_name == rel) {
                    target_ids_map.insert(key.clone(), links.clone());
                    for (_, t_rec_id) in links {
                        needed_records.entry(key.target_col_name.clone())
                            .or_default()
                            .push(t_rec_id);
                    }
                }
            }
        }

        let cols = self.db.list_collections().await.map_err(|e| e.to_string())?;
        let col_name_to_id: HashMap<String, i64> = cols.into_iter().map(|c| (c.name, c.id)).collect();
        let mut fetched_record_cache: HashMap<(String, i64), Record> = HashMap::new();

        for (t_col_name, t_ids) in needed_records {
            if let Some(t_col_id) = col_name_to_id.get(&t_col_name) {
                let recs = self.db.get_records_by_ids(*t_col_id, &t_ids)
                    .await
                    .map_err(|e| e.to_string())?;
                for r in recs {
                    fetched_record_cache.insert((t_col_name.clone(), r.id), r);
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

// --- 3. SCHEMA BUILDER ---

pub async fn build_schema(
    state: AppState, 
    loader: Arc<DataLoader<RelationLoader>>
) -> Result<Schema, SchemaError> {
    
    // Initialize User Loader
    let user_loader = DataLoader::new(
        UserLoader { db: state.db.clone() },
        tokio::spawn
    );

    let mut schema_builder = Schema::build("Query", None, None);
    schema_builder = schema_builder.register(Scalar::new("JSON"));

    // --- DEFINE USER TYPE ---
    let mut user_object = Object::new("User");
    user_object = user_object
        .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<User>().unwrap();
                Ok(Some(GqlValue::from(u.id.to_string())))
            })
        }))
        .field(Field::new("email", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<User>().unwrap();
                Ok(Some(GqlValue::from(u.email.clone())))
            })
        }))
        .field(Field::new("role", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<User>().unwrap();
                Ok(Some(GqlValue::from(u.role.clone())))
            })
        }));
    
    schema_builder = schema_builder.register(user_object);

    // --- DEFINE USER LIST TYPE ---
    let mut user_list_object = Object::new("UserList");
    user_list_object = user_list_object
        .field(Field::new("total", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let total = ctx.parent_value.try_downcast_ref::<(i64, Vec<User>)>().unwrap().0;
                Ok(Some(GqlValue::from(total)))
            })
        }))
        .field(Field::new("items", TypeRef::named_nn_list("User"), |ctx| {
            FieldFuture::new(async move {
                let items = &ctx.parent_value.try_downcast_ref::<(i64, Vec<User>)>().unwrap().1;
                Ok(Some(FieldValue::list(items.iter().map(|u| FieldValue::owned_any(u.clone())))))
            })
        }));
    
    schema_builder = schema_builder.register(user_list_object);

    // --- ROOT QUERY ---
    let mut query_root = Object::new("Query");

    query_root = query_root.field(Field::new("status", TypeRef::named(TypeRef::STRING), |_| {
        FieldFuture::new(async { Ok(Some(GqlValue::from("ApexKit is running"))) })
    }));

    // Add 'users' query to Root
    query_root = query_root.field(Field::new("users", TypeRef::named("UserList"), move |ctx| {
        let state = ctx.data::<AppState>().unwrap().clone();
        FieldFuture::new(async move {
            let limit = ctx.args.get("limit").and_then(|v| v.i64().ok()).unwrap_or(20);
            let offset = ctx.args.get("offset").and_then(|v| v.i64().ok()).unwrap_or(0);
            
            // FIX: Convert Result<&str> to Option<String>
            let search = ctx.args.get("search")
                .and_then(|v| v.string().ok())
                .map(|s| s.to_string());

            let total = state.db.count_users(search.clone()).await.map_err(|e| async_graphql::Error::new(e.to_string()))?;
            let items = state.db.list_users(search, limit, offset).await.map_err(|e| async_graphql::Error::new(e.to_string()))?;

            Ok(Some(FieldValue::owned_any((total, items))))
        })
    })
    .argument(InputValue::new("limit", TypeRef::named(TypeRef::INT)))
    .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT)))
    .argument(InputValue::new("search", TypeRef::named(TypeRef::STRING))));


    // --- DYNAMIC COLLECTIONS ---
    let collections = state.db.list_collections().await.unwrap_or_default();
    let col_id_to_name: HashMap<String, String> = collections.iter()
        .map(|c| (c.id.to_string(), c.name.clone()))
        .collect();

    for col in &collections {
        let type_name = capitalize(&col.name);
        
        let mut object = Object::new(&type_name);

        object = object.field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move { 
                let record = ctx.parent_value.try_downcast_ref::<Record>().unwrap();
                Ok(Some(GqlValue::from(record.id.to_string()))) 
            })
        }));

        if let Some(schema) = &col.schema {
            for (field_name, def) in &schema.fields {
                let name_clone = field_name.clone();

                // === SPECIAL HANDLER: OWNER FIELDS ===
                if def.r#type == FieldType::Owner {
                     let field = Field::new(field_name, TypeRef::named("User"), move |ctx| {
                        let name = name_clone.clone(); 
                        FieldFuture::new(async move { 
                            let record = ctx.parent_value.try_downcast_ref::<Record>().unwrap();
                            let user_id = record.data.get(&name)
                                .and_then(|v| v.as_i64())
                                .or_else(|| record.data.get(&name).and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()));
                            
                            if let Some(uid) = user_id {
                                let loader = ctx.data::<DataLoader<UserLoader>>().unwrap();
                                let user = loader.load_one(uid).await.map_err(|e| async_graphql::Error::new(e))?;
                                Ok(user.map(FieldValue::owned_any))
                            } else {
                                Ok(None)
                            }
                        })
                    });
                    object = object.field(field);
                    continue; 
                }

                // === STANDARD FIELDS ===
                let gql_type = match def.r#type {
                    FieldType::Number => TypeRef::FLOAT,
                    FieldType::Boolean => TypeRef::BOOLEAN,
                    _ => TypeRef::STRING,
                };

                let field = Field::new(field_name, TypeRef::named(gql_type), move |ctx| {
                    let name = name_clone.clone(); 
                    FieldFuture::new(async move { 
                        let record = ctx.parent_value.try_downcast_ref::<Record>().unwrap();
                        let val = record.data.get(&name).cloned();
                        map_json_to_gql(val) 
                    })
                });
                object = object.field(field);
            }

            // === RELATIONS ===
            for (rel_name, rel_def) in &schema.relations {
                let raw_target = &rel_def.target_collection;
                let resolved_target_name = col_id_to_name.get(raw_target).unwrap_or(raw_target);
                
                let target_type_name = capitalize(resolved_target_name);
                let t_col_name = resolved_target_name.clone(); 

                let origin_col_id = col.id;
                let r_name = rel_name.clone();
                let is_list = rel_def.relation_type == RelationType::Many;

                let type_ref = if is_list { TypeRef::named_list(&target_type_name) } else { TypeRef::named(&target_type_name) };

                let field = Field::new(rel_name, type_ref, move |ctx| {
                    let r_name = r_name.clone();
                    let t_col_name = t_col_name.clone();
                    
                    FieldFuture::new(async move {
                        let record = ctx.parent_value.try_downcast_ref::<Record>()
                            .map(|r| r.clone())
                            .map_err(|_| async_graphql::Error::new("Internal Type Error"))?;
                            
                        let loader = ctx.data::<Arc<DataLoader<RelationLoader>>>()
                            .map_err(|_| async_graphql::Error::new("Loader missing"))?
                            .clone();
                            
                        let key = RelationKey { 
                            origin_col_id, 
                            origin_rec_id: record.id, 
                            rel_name: r_name, 
                            target_col_name: t_col_name 
                        };

                        let records = loader.load_one(key).await.map_err(|e| async_graphql::Error::new(e))?.unwrap_or_default();
                        if is_list {
                            Ok(Some(FieldValue::list(records.into_iter().map(FieldValue::owned_any))))
                        } else {
                            Ok(records.into_iter().next().map(FieldValue::owned_any))
                        }
                    })
                });
                object = object.field(field);
            }
        }
        
        schema_builder = schema_builder.register(object);

        // 2. Define the List Wrapper Object
        let list_type_name = format!("{}List", type_name);
        let mut list_object = Object::new(&list_type_name);

        list_object = list_object.field(Field::new("total", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let res = ctx.parent_value.try_downcast_ref::<ListResult>()
                    .map_err(|_| async_graphql::Error::new("Internal List Error"))?;
                Ok(Some(GqlValue::from(res.total)))
            })
        }));

        let items_type_name = type_name.clone();
        list_object = list_object.field(Field::new("items", TypeRef::named_nn_list(&items_type_name), move |ctx| {
            FieldFuture::new(async move {
                let res = ctx.parent_value.try_downcast_ref::<ListResult>()
                    .map_err(|_| async_graphql::Error::new("Internal List Error"))?;
                let items: Vec<FieldValue> = res.items.iter()
                    .map(|r| FieldValue::owned_any(r.clone()))
                    .collect();
                Ok(Some(FieldValue::list(items)))
            })
        }));

        schema_builder = schema_builder.register(list_object);

        // 3. Define the Root Query Field
        let col_id = col.id;
        let query_name = col.name.clone();
        let list_type_name_ref = list_type_name.clone();
        
        let list_field = Field::new(query_name, TypeRef::named(&list_type_name_ref), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();
            FieldFuture::new(async move {
                let limit = ctx.args.get("limit").and_then(|v| v.u64().ok());
                let offset = ctx.args.get("offset").and_then(|v| v.u64().ok());
                
                let filter_str = match ctx.args.get("where") {
                    Some(accessor) => {
                        match accessor.deserialize::<serde_json::Value>() {
                            Ok(json_val) => Some(json_val.to_string()),
                            Err(_) => None
                        }
                    },
                    None => None,
                };

                let mut options = apexkit_core::query::QueryOptions::default();
                if limit.is_some() || offset.is_some() {
                    options.limit = limit;
                    options.offset = offset;
                } else {
                    options.limit = Some(100); 
                }
                
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

    schema_builder
        .register(query_root)
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