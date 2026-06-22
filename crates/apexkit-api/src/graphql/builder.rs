use super::dataloaders::{RelationKey, RelationLoader, UserLoader};
use super::utils::{
    capitalize, extract_script_config, gql_input_to_json, json_to_gql, map_json_to_gql,
    map_type_ref, prepare_data_for_create,
};
use crate::AppState;
use apexkit_core::auth::Claims;
use apexkit_core::auth::policies;
use apexkit_core::models::{ListResult, Record};
use apexkit_core::realtime::EventScope;
use apexkit_core::{
    auth::User,
    models::schema::{FieldType, RelationType},
};
use async_graphql::dataloader::*;
use async_graphql::extensions::Analyzer;
use async_graphql::{Value as GqlValue, dynamic::*};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;
// --- SCHEMA BUILDER ---

pub async fn build_schema(
    state: AppState,
    loader: Arc<DataLoader<RelationLoader>>,
) -> Result<Schema, SchemaError> {
    let user_loader = DataLoader::new(
        UserLoader {
            db: state.db.clone(),
        },
        tokio::spawn,
    );

    let mut schema_builder = Schema::build("Query", Some("Mutation"), None);
    schema_builder = schema_builder
        .register(Scalar::new("JSON"))
        .limit_depth(32)
        .limit_complexity(2000)
        .extension(Analyzer);

    // --- 1. PREPARE STANDARD OBJECTS ---

    let mut query_root = Object::new("Query");
    let mut mutation_root = Object::new("Mutation");
    let mut user_object = Object::new("User");
    let mut collection_objects: HashMap<String, Object> = HashMap::new();

    // Standard Fields
    query_root = query_root.field(Field::new(
        "status",
        TypeRef::named(TypeRef::STRING),
        |_| FieldFuture::new(async { Ok(Some(GqlValue::from("ApexKit is running"))) }),
    ));

    mutation_root =
        mutation_root.field(Field::new("ping", TypeRef::named(TypeRef::STRING), |_| {
            FieldFuture::new(async { Ok(Some(GqlValue::from("pong"))) })
        }));

    // [Keep Standard User & UserList definitions as is...]
    user_object = user_object
        .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<User>()?;
                Ok(Some(GqlValue::from(u.id.to_string())))
            })
        }))
        .field(Field::new(
            "email",
            TypeRef::named_nn(TypeRef::STRING),
            |ctx| {
                FieldFuture::new(async move {
                    let u = ctx.parent_value.try_downcast_ref::<User>()?;
                    Ok(Some(GqlValue::from(u.email.clone())))
                })
            },
        ))
        .field(Field::new(
            "role",
            TypeRef::named_nn(TypeRef::STRING),
            |ctx| {
                FieldFuture::new(async move {
                    let u = ctx.parent_value.try_downcast_ref::<User>()?;
                    Ok(Some(GqlValue::from(u.role.clone())))
                })
            },
        ));

    let mut user_list = Object::new("UserList");
    user_list = user_list
        .field(Field::new(
            "total",
            TypeRef::named_nn(TypeRef::INT),
            |ctx| {
                FieldFuture::new(async move {
                    let total = ctx.parent_value.try_downcast_ref::<(i64, Vec<User>)>()?.0;
                    Ok(Some(GqlValue::from(total)))
                })
            },
        ))
        .field(Field::new("items", TypeRef::named_nn_list("User"), |ctx| {
            FieldFuture::new(async move {
                let items = &ctx.parent_value.try_downcast_ref::<(i64, Vec<User>)>()?.1;
                Ok(Some(FieldValue::list(
                    items.iter().map(|u| FieldValue::owned_any(u.clone())),
                )))
            })
        }));
    schema_builder = schema_builder.register(user_list);

    query_root = query_root.field(
        Field::new("users", TypeRef::named("UserList"), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();
            FieldFuture::new(async move {
                let claims = ctx.data::<Claims>().ok();
                if !matches!(claims, Some(c) if c.role == "admin") {
                    return Err(async_graphql::Error::new("Forbidden: Admins only"));
                }
                let limit = ctx
                    .args
                    .get("limit")
                    .and_then(|v| v.i64().ok())
                    .unwrap_or(20);
                let offset = ctx
                    .args
                    .get("offset")
                    .and_then(|v| v.i64().ok())
                    .unwrap_or(0);
                let search = ctx
                    .args
                    .get("search")
                    .and_then(|v| v.string().ok())
                    .map(|s| s.to_string());
                let total = state
                    .db
                    .count_users(search.clone())
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                let items = state
                    .db
                    .list_users(search, limit, offset)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                Ok(Some(FieldValue::owned_any((total, items))))
            })
        })
        .argument(InputValue::new("limit", TypeRef::named(TypeRef::INT)))
        .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT)))
        .argument(InputValue::new("search", TypeRef::named(TypeRef::STRING))),
    );

    // --- 2. BUILD COLLECTION OBJECTS (QUERIES & MUTATIONS) ---
    let collections = state.db.list_collections().await.unwrap_or_default();
    let col_id_to_name: HashMap<String, String> = collections
        .iter()
        .map(|c| (c.id.to_string(), c.name.clone()))
        .collect();

    for col in &collections {
        let type_name = capitalize(&col.name);
        let col_id = col.id;
        let _col_name = col.name.clone();

        // --- 2A. QUERY TYPES ---
        let mut object = Object::new(&type_name);

        let read_policy = col
            .schema
            .as_ref()
            .map(|s| s.policies.read.clone())
            .unwrap_or("public".to_string());

        let create_policy = col
            .schema
            .as_ref()
            .map(|s| s.policies.create.clone())
            .unwrap_or("auth".to_string());

        let update_policy = col
            .schema
            .as_ref()
            .map(|s| s.policies.update.clone())
            .unwrap_or("admin".to_string());

        let delete_policy = col
            .schema
            .as_ref()
            .map(|s| s.policies.delete.clone())
            .unwrap_or("admin".to_string());

        object = object.field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move {
                let record = ctx.parent_value.try_downcast_ref::<Record>()?;
                Ok(Some(GqlValue::from(record.id.to_string())))
            })
        }));

        // Dynamically build Input Objects for Mutations
        let create_input_name = format!("Create{}Input", type_name);
        let update_input_name = format!("Update{}Input", type_name);

        let mut create_input = InputObject::new(&create_input_name);
        let mut update_input = InputObject::new(&update_input_name);

        if let Some(schema) = &col.schema {
            for (field_name, def) in &schema.fields {
                // [CRITICAL FIX]: Prevent GraphQL Panic
                // Skip 'id' since it is already natively registered above
                if field_name == "id" {
                    continue;
                }

                let name_clone = field_name.clone();
                let is_owner = def.r#type == FieldType::Owner;

                // --- Query Field ---
                if is_owner {
                    let field = Field::new(field_name, TypeRef::named("User"), move |ctx| {
                        let name = name_clone.clone();
                        FieldFuture::new(async move {
                            let record = ctx.parent_value.try_downcast_ref::<Record>()?;
                            let user_id =
                                record.data.get(&name).and_then(|v| v.as_i64()).or_else(|| {
                                    record
                                        .data
                                        .get(&name)
                                        .and_then(|v| v.as_str())
                                        .and_then(|s| s.parse::<i64>().ok())
                                });
                            if let Some(uid) = user_id {
                                let loader = ctx.data::<DataLoader<UserLoader>>().unwrap();
                                let user = loader
                                    .load_one(uid)
                                    .await
                                    .map_err(async_graphql::Error::new)?;
                                Ok(user.map(FieldValue::owned_any))
                            } else {
                                Ok(None)
                            }
                        })
                    });
                    object = object.field(field);
                } else {
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

                // --- Input Fields ---
                let input_type = match def.r#type {
                    FieldType::Number => TypeRef::FLOAT,
                    FieldType::Boolean => TypeRef::BOOLEAN,
                    _ => TypeRef::STRING,
                };

                // Create Input (Respect required, ignore owner if auto-set)
                if !is_owner || !def.auto {
                    // For Create: if required, mark NonNull
                    let create_type_ref = if def.required {
                        TypeRef::named_nn(input_type)
                    } else {
                        TypeRef::named(input_type)
                    };
                    create_input = create_input.field(InputValue::new(field_name, create_type_ref));
                }

                // Update Input (All optional)
                // For Owner fields, we allow updating explicitly via ID string unless it's strictly auto
                update_input =
                    update_input.field(InputValue::new(field_name, TypeRef::named(input_type)));
            }

            // Relations (Read-Only fields for now)
            for (rel_name, rel_def) in &schema.relations {
                let raw_target = &rel_def.target_collection;
                let resolved_target_name = col_id_to_name.get(raw_target).unwrap_or(raw_target);
                let target_type_name = capitalize(resolved_target_name);
                let t_col_name = resolved_target_name.clone();
                let origin_col_id = col.id;
                let r_name = rel_name.clone();
                let is_list = rel_def.relation_type == RelationType::Many;
                let type_ref = if is_list {
                    TypeRef::List(Box::new(TypeRef::named(&target_type_name)))
                } else {
                    TypeRef::named(&target_type_name)
                };

                let field = Field::new(rel_name, type_ref, move |ctx| {
                    let r_name = r_name.clone();
                    let t_col_name = t_col_name.clone();
                    FieldFuture::new(async move {
                        let record = ctx.parent_value.try_downcast_ref::<Record>()?.clone();
                        let loader = ctx
                            .data::<Arc<DataLoader<RelationLoader>>>()
                            .unwrap()
                            .clone();
                        let key = RelationKey {
                            origin_col_id,
                            origin_rec_id: record.id,
                            rel_name: r_name,
                            target_col_name: t_col_name,
                        };
                        let records = loader
                            .load_one(key)
                            .await
                            .map_err(async_graphql::Error::new)?
                            .unwrap_or_default();

                        if is_list {
                            Ok(Some(FieldValue::list(
                                records.into_iter().map(FieldValue::owned_any),
                            )))
                        } else {
                            Ok(records.into_iter().next().map(FieldValue::owned_any))
                        }
                    })
                });
                object = object.field(field);
            }
        }

        // Register the Objects
        collection_objects.insert(type_name.clone(), object);
        schema_builder = schema_builder.register(create_input);
        schema_builder = schema_builder.register(update_input);

        // List Type
        let list_type_name = format!("{}List", type_name);
        let mut list_object = Object::new(&list_type_name);
        list_object = list_object.field(Field::new(
            "total",
            TypeRef::named_nn(TypeRef::INT),
            |ctx| {
                FieldFuture::new(async move {
                    let res = ctx.parent_value.try_downcast_ref::<ListResult>()?;
                    Ok(Some(GqlValue::from(res.total)))
                })
            },
        ));
        list_object = list_object.field(Field::new(
            "items",
            TypeRef::named_nn_list(&type_name),
            move |ctx| {
                FieldFuture::new(async move {
                    let res = ctx.parent_value.try_downcast_ref::<ListResult>()?;
                    let items: Vec<FieldValue> = res
                        .items
                        .iter()
                        .map(|r| FieldValue::owned_any(r.clone()))
                        .collect();
                    Ok(Some(FieldValue::list(items)))
                })
            },
        ));
        schema_builder = schema_builder.register(list_object);

        // --- QUERY FIELDS ---
        let query_name = col.name.clone();
        let policy_clone = read_policy.clone();

        let list_field = Field::new(query_name, TypeRef::named(&list_type_name), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();

            // Access Check
            let claims = ctx.data::<Claims>().ok();
            if !policies::check_access(&policy_clone, claims, None) {
                return FieldFuture::new(async {
                    Err::<Option<FieldValue>, _>(async_graphql::Error::new("Forbidden"))
                });
            }

            FieldFuture::new(async move {
                let limit = ctx.args.get("limit").and_then(|v| v.u64().ok());
                let offset = ctx.args.get("offset").and_then(|v| v.u64().ok());
                let filter_str = match ctx.args.get("where") {
                    Some(accessor) => accessor
                        .deserialize::<serde_json::Value>()
                        .ok()
                        .map(|j| j.to_string()),
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
                let result = state
                    .db
                    .list_records(col_id, options)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                Ok(Some(FieldValue::owned_any(result)))
            })
        })
        .argument(InputValue::new("limit", TypeRef::named(TypeRef::INT)))
        .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT)))
        .argument(InputValue::new("where", TypeRef::named("JSON")));

        query_root = query_root.field(list_field);

        // --- MUTATIONS ---

        // 1. CREATE
        let create_name = format!("create{}", type_name);
        let c_policy = create_policy.clone();
        let c_schema = col.schema.clone().unwrap_or_default();
        let c_col_id = col_id;

        let create_mutation = Field::new(create_name, TypeRef::named(&type_name), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();
            let claims = ctx.data::<Claims>().ok().cloned();
            let p = c_policy.clone();
            let sch = c_schema.clone();

            FieldFuture::new(async move {
                // 1. Check Policy
                if !policies::check_access(&p, claims.as_ref(), None) {
                    return Err(async_graphql::Error::new("Forbidden: Create denied"));
                }

                // 2. Parse Input
                let input_val = ctx
                    .args
                    .get("data")
                    .ok_or(async_graphql::Error::new("Missing data"))?;
                let json_data = gql_input_to_json(input_val.as_value().clone());

                // 3. Prepare & Inject Auto Fields
                let uid = claims.as_ref().map(|c| c.uid);
                let final_data = prepare_data_for_create(json_data, &sch, uid);

                // 4. Create in DB
                let id = state
                    .db
                    .create_record(c_col_id, &final_data)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;

                // 5. Return Created Record
                let rec = state
                    .db
                    .get_record(c_col_id, id, None)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?
                    .ok_or(async_graphql::Error::new("Record creation failed"))?;

                Ok(Some(FieldValue::owned_any(rec)))
            })
        })
        .argument(InputValue::new(
            "data",
            TypeRef::named_nn(&create_input_name),
        ));
        mutation_root = mutation_root.field(create_mutation);

        // 2. UPDATE
        let update_name = format!("update{}", type_name);
        let u_policy = update_policy.clone();
        let u_col_id = col_id;

        let update_mutation = Field::new(update_name, TypeRef::named(&type_name), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();
            let claims = ctx.data::<Claims>().ok().cloned();
            let p = u_policy.clone();

            FieldFuture::new(async move {
                // FIX: Use .ok() to convert Result -> Option inside and_then
                let id_str: String = ctx
                    .args
                    .get("id")
                    .and_then(|v| v.string().ok().map(|s| s.to_string()))
                    .ok_or(async_graphql::Error::new("ID required"))?;

                let id = id_str
                    .parse::<i64>()
                    .map_err(|_| async_graphql::Error::new("Invalid ID format"))?;

                let input_val = ctx
                    .args
                    .get("data")
                    .ok_or(async_graphql::Error::new("Missing data"))?;
                let json_data = gql_input_to_json(input_val.as_value().clone());

                // 1. Fetch Existing (Needed for Policy Check)
                let existing = state
                    .db
                    .get_record(u_col_id, id, None)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?
                    .ok_or(async_graphql::Error::new("Record not found"))?;

                // 2. Check Policy against Existing Data
                if !policies::check_access(&p, claims.as_ref(), Some(&existing.data)) {
                    return Err(async_graphql::Error::new("Forbidden: Update denied"));
                }

                // 3. Update in DB
                let updated = state
                    .db
                    .update_record(u_col_id, id, &json_data)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;

                Ok(Some(FieldValue::owned_any(updated)))
            })
        })
        .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)))
        .argument(InputValue::new(
            "data",
            TypeRef::named_nn(&update_input_name),
        ));
        mutation_root = mutation_root.field(update_mutation);

        // 3. DELETE
        let delete_name = format!("delete{}", type_name);
        let d_policy = delete_policy.clone();
        let d_col_id = col_id;

        let delete_mutation =
            Field::new(delete_name, TypeRef::named(TypeRef::BOOLEAN), move |ctx| {
                let state = ctx.data::<AppState>().unwrap().clone();
                let claims = ctx.data::<Claims>().ok().cloned();
                let p = d_policy.clone();

                FieldFuture::new(async move {
                    // FIX: Use .ok() to convert Result -> Option inside and_then
                    let id_str: String = ctx
                        .args
                        .get("id")
                        .and_then(|v| v.string().ok().map(|s| s.to_string()))
                        .ok_or(async_graphql::Error::new("ID required"))?;

                    let id = id_str
                        .parse::<i64>()
                        .map_err(|_| async_graphql::Error::new("Invalid ID format"))?;

                    // 1. Fetch Existing (Needed for Policy Check)
                    let existing = state
                        .db
                        .get_record(d_col_id, id, None)
                        .await
                        .map_err(|e| async_graphql::Error::new(e.to_string()))?
                        .ok_or(async_graphql::Error::new("Record not found"))?;

                    // 2. Check Policy
                    if !policies::check_access(&p, claims.as_ref(), Some(&existing.data)) {
                        return Err(async_graphql::Error::new("Forbidden: Delete denied"));
                    }

                    // 3. Delete
                    state
                        .db
                        .delete_record(d_col_id, id)
                        .await
                        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

                    Ok(Some(FieldValue::owned_any(true)))
                })
            })
            .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)));
        mutation_root = mutation_root.field(delete_mutation);
    }

    // --- 3. FETCH & INJECT CUSTOM RESOLVERS FROM SCRIPTS ---
    let scripts = state.db.list_scripts().await.unwrap_or_default();

    for script in scripts {
        if script.trigger_type == "graphql"
            && script.active
            && let Some(config) = extract_script_config(&script.code)
        {
            let script_name = script.name.clone();
            let args_def = config.args.clone().unwrap_or_default();
            let return_type = map_type_ref(&config.return_type);

            let field_name = config.name.clone();

            let mut field = Field::new(field_name, return_type, move |ctx| {
                let s_name = script_name.clone();
                let a_def = args_def.clone();
                FieldFuture::new(async move {
                    let state = ctx.data::<AppState>().unwrap();

                    let mut script_input = serde_json::Map::new();
                    for arg_name in a_def.keys() {
                        if let Some(val) = ctx.args.get(arg_name) {
                            let json_val = val
                                .deserialize::<serde_json::Value>()
                                .unwrap_or(serde_json::Value::Null);
                            script_input.insert(arg_name.clone(), json_val);
                        }
                    }

                    let parent_data = if let Ok(rec) = ctx.parent_value.try_downcast_ref::<Record>()
                    {
                        Some(serde_json::json!({ "id": rec.id, "data": rec.data }))
                    } else if let Ok(u) = ctx.parent_value.try_downcast_ref::<User>() {
                        Some(serde_json::json!({ "id": u.id, "email": u.email }))
                    } else {
                        None
                    };

                    if let Some(p) = parent_data {
                        script_input.insert("parent".to_string(), p);
                    }

                    let script_record = state
                        .db
                        .get_script_by_name(&s_name)
                        .await
                        .map_err(|e| async_graphql::Error::new(e.to_string()))?
                        .ok_or_else(|| async_graphql::Error::new("Linked script not found"))?;
                    let event_scope = ctx
                        .data::<EventScope>()
                        .unwrap_or(&EventScope::Root)
                        .clone();

                    let context = Arc::new(crate::ScopedScriptContext {
                        state: state.clone(),
                        scope: event_scope.clone(),
                    });

                    let result = state
                        .script_engine
                        .run_script(
                            &script_record.code,
                            serde_json::Value::Object(script_input),
                            context,
                            None,
                            None,
                        )
                        .await
                        .map_err(async_graphql::Error::new)?;

                    Ok(Some(FieldValue::value(json_to_gql(result))))
                })
            });

            if let Some(args) = config.args {
                for (k, v) in args {
                    field = field.argument(InputValue::new(k, map_type_ref(&v)));
                }
            }

            match config.parent.as_str() {
                "Query" => query_root = query_root.field(field),
                "Mutation" => mutation_root = mutation_root.field(field),
                "User" => user_object = user_object.field(field),
                other => {
                    if let Some(obj) = collection_objects.get_mut(other) {
                        let new_obj = std::mem::replace(obj, Object::new(other));
                        *obj = new_obj.field(field);
                    } else {
                        info!(
                            "Skipping GraphQL script {}: Parent '{}' not found",
                            script.name, other
                        );
                    }
                }
            }
        }
    }

    for (_, obj) in collection_objects {
        schema_builder = schema_builder.register(obj);
    }

    let mut builder = schema_builder
        .register(user_object)
        .register(query_root)
        .register(mutation_root)
        .data(state)
        .data(loader)
        .data(user_loader);

    if std::env::var("APP_ENV").unwrap_or_default() == "production" {
        builder = builder.disable_introspection();
    }

    builder.finish()
}
