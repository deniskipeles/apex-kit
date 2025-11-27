use async_graphql::{dynamic::*, Value as GqlValue};
use async_graphql::dataloader::*;
use tinybase_core::{Db, schema::{FieldType, RelationType}, Record};
use crate::AppState;
use std::sync::Arc;
use std::collections::HashMap;

pub struct RelationLoader {
    db: Arc<dyn Db>,
}

impl RelationLoader {
    pub fn new(db: Arc<dyn Db>) -> Self {
        Self { db }
    }
}

// Ensure RelationKey derives these to work as HashMap keys
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub struct RelationKey {
    pub origin_col_id: i64,
    pub origin_rec_id: i64,
    pub rel_name: String,
    pub target_col_name: String,
}

impl Loader<RelationKey> for RelationLoader {
    // Define types here
    type Value = Vec<Record>;
    type Error = String;
    
    // REMOVED <'a> and &'a
    async fn load(&self, keys: &[RelationKey]) -> std::result::Result<HashMap<RelationKey, Self::Value>, Self::Error> {
        // 1. Clone keys immediately so they are owned inside the async block
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

        // 2. Fetch Relations
        for ((o_col, rel), o_ids) in grouped_keys {
            for o_id in o_ids {
                let links = self.db.get_related_ids(o_col, o_id, &rel)
                    .await
                    .map_err(|e| e.to_string())?;
                
                // Match back to the specific RelationKey
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

        // 3. Fetch Records
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

        // 4. Assemble Results
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

pub async fn build_schema(state: AppState, loader: Arc<DataLoader<RelationLoader>>) -> Result<Schema, SchemaError> {
    let mut schema_builder = Schema::build("Query", None, None);
    let mut query_root = Object::new("Query");

    // Default field to ensure schema is valid even if empty
    query_root = query_root.field(Field::new("status", TypeRef::named(TypeRef::STRING), |_| {
        FieldFuture::new(async { Ok(Some(GqlValue::from("TinyBase is running"))) })
    }));

    let collections = state.db.list_collections().await.unwrap_or_default();

    // --- FIX START: Create ID -> Name Lookup Map ---
    let col_id_to_name: HashMap<String, String> = collections.iter()
        .map(|c| (c.id.to_string(), c.name.clone()))
        .collect();
    // --- FIX END ---

    for col in &collections {
        let type_name = capitalize(&col.name);
        let mut object = Object::new(&type_name);

        object = object.field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move { 
                let record = ctx.parent_value.try_downcast_ref::<Record>()
                    .map(|r| r.clone())
                    .map_err(|_| async_graphql::Error::new("Internal Type Error"))?;
                Ok(Some(GqlValue::from(record.id.to_string()))) 
            })
        }));

        if let Some(schema) = &col.schema {
            for (field_name, def) in &schema.fields {
                let name_clone = field_name.clone();
                let gql_type = match def.r#type {
                    FieldType::Number => TypeRef::FLOAT,
                    FieldType::Boolean => TypeRef::BOOLEAN,
                    _ => TypeRef::STRING,
                };

                let field = Field::new(field_name, TypeRef::named(gql_type), move |ctx| {
                    let name = name_clone.clone(); 
                    FieldFuture::new(async move { 
                        let record = ctx.parent_value.try_downcast_ref::<Record>()
                            .map(|r| r.clone())
                            .map_err(|_| async_graphql::Error::new("Internal Type Error"))?;
                        let val = record.data.get(&name).cloned();
                        map_json_to_gql(val) 
                    })
                });
                object = object.field(field);
            }

            for (rel_name, rel_def) in &schema.relations {
                // --- FIX START: Resolve Target Name ---
                // If target is "3", look it up to find "users"
                let raw_target = &rel_def.target_collection;
                let resolved_target_name = col_id_to_name.get(raw_target).unwrap_or(raw_target);
                
                let target_type_name = capitalize(resolved_target_name);
                // Capture resolved name for the closure
                let t_col_name = resolved_target_name.clone(); 
                // --- FIX END ---

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

        let col_id = col.id;
        let query_name = col.name.clone();
        let type_name_clone = type_name.clone();
        
        let list_field = Field::new(query_name, TypeRef::named_list(&type_name_clone), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();
            FieldFuture::new(async move {
                let records = state.db.list_records(col_id, Default::default()).await.map_err(|e| async_graphql::Error::new(e.to_string()))?;
                Ok(Some(FieldValue::list(records.into_iter().map(FieldValue::owned_any))))
            })
        });
        query_root = query_root.field(list_field);
    }

    schema_builder
        .register(query_root)
        .data(state)
        .data(loader)
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