use super::dataloaders::{RelationKey, RelationLoader, UserLoader};
use super::utils::{
    capitalize, extract_script_config, gql_input_to_json, json_to_gql, map_json_to_gql,
    map_type_ref, prepare_data_for_create,
};
use crate::AppState;
use apexkit_core::auth::Claims;
use apexkit_core::auth::policies;
use apexkit_core::models::{ListResult, Record};
use apexkit_core::query::QueryOptions;
use apexkit_core::realtime::EventScope;
use apexkit_core::{
    auth::User,
    models::schema::{FieldType, RelationType},
};
use async_graphql::dataloader::*;
use async_graphql::extensions::Analyzer;
use async_graphql::{Value as GqlValue, dynamic::*};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::info;

// --- COLLISION TRACKER HELPER ---
// Ensures we never panic on "Field already exists". If a collision occurs,
// it preserves the data by appending the requested suffix (e.g. __rrc_list1)
fn get_unique_field_name(
    object_name: &str,
    desired_name: &str,
    tracker: &mut HashMap<String, HashSet<String>>,
) -> String {
    let set = tracker.entry(object_name.to_string()).or_default();
    if set.insert(desired_name.to_string()) {
        return desired_name.to_string();
    }

    let mut counter = 1;
    loop {
        let candidate = format!("{}___rrc_list{}", desired_name, counter);
        if set.insert(candidate.clone()) {
            tracing::warn!(
                "⚠️ [GraphQL] Field '{}' conflicted on object '{}'. Renamed to '{}' to prevent data loss.",
                desired_name,
                object_name,
                candidate
            );
            return candidate;
        }
        counter += 1;
    }
}

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

    let collections = state.db.list_collections().await.unwrap_or_default();
    let col_id_to_name: HashMap<String, String> = collections
        .iter()
        .map(|c| (c.id.to_string(), c.name.clone()))
        .collect();

    // 0. FETCH USER POLICIES
    let policy_users_json = state.db.get_config("policy_users").await.unwrap_or(None);
    let user_read_policy = if let Some(val) = policy_users_json {
        let parsed_val = match val {
            serde_json::Value::String(s) => {
                serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
            }
            other => other,
        };
        if let Ok(p) =
            serde_json::from_value::<apexkit_core::models::schema::CollectionPolicies>(parsed_val)
        {
            p.read
        } else {
            "admin || owner:id".to_string()
        }
    } else {
        "admin || owner:id".to_string()
    };

    // --- FIELD TRACKER ---
    let mut field_tracker: HashMap<String, HashSet<String>> = HashMap::new();

    // --- 1. PRE-CALCULATE REVERSE RELATIONS & TARGET POLICIES ---

    // A. User Reverse Owner Relations: (col_id, col_name, owner_field_name, target_policy)
    let mut user_reverse_fields = Vec::new();
    let mut user_field_names_count: HashMap<String, usize> = HashMap::new();

    for col in &collections {
        if let Some(schema) = &col.schema {
            let col_policy = schema.policies.read.clone();
            for (field_name, def) in &schema.fields {
                if def.r#type == FieldType::Owner {
                    user_reverse_fields.push((
                        col.id,
                        col.name.clone(),
                        field_name.clone(),
                        col_policy.clone(),
                    ));
                    *user_field_names_count.entry(col.name.clone()).or_insert(0) += 1;
                }
            }
        }
    }

    // B. Collection Reverse Relations: Target Col ID -> Vec<(Origin Col ID, Origin Col Name, Rel Field Name, Origin Policy)>
    let mut reverse_relations_map: HashMap<i64, Vec<(i64, String, String, String)>> =
        HashMap::new();
    for other_col in &collections {
        if let Some(schema) = &other_col.schema {
            let origin_policy = schema.policies.read.clone();
            for (rel_name, def) in &schema.relations {
                let target_col_id_opt = collections
                    .iter()
                    .find(|c| {
                        c.name == def.target_collection || c.id.to_string() == def.target_collection
                    })
                    .map(|c| c.id);

                if let Some(target_id) = target_col_id_opt {
                    reverse_relations_map.entry(target_id).or_default().push((
                        other_col.id,
                        other_col.name.clone(),
                        rel_name.clone(),
                        origin_policy.clone(),
                    ));
                }
            }
        }
    }

    // --- 2. PREPARE STANDARD OBJECTS ---

    let mut query_root = Object::new("Query");
    let mut mutation_root = Object::new("Mutation");
    let mut user_object = Object::new("_AuthUser");
    let mut collection_objects: HashMap<String, Object> = HashMap::new();

    // Standard System Fields
    let fname = get_unique_field_name("Query", "_status", &mut field_tracker);
    query_root = query_root.field(Field::new(fname, TypeRef::named(TypeRef::STRING), |_| {
        FieldFuture::new(async { Ok(Some(GqlValue::from("ApexKit is running"))) })
    }));

    let fname = get_unique_field_name("Mutation", "_ping", &mut field_tracker);
    mutation_root = mutation_root.field(Field::new(fname, TypeRef::named(TypeRef::STRING), |_| {
        FieldFuture::new(async { Ok(Some(GqlValue::from("pong"))) })
    }));

    let fname = get_unique_field_name("_AuthUser", "id", &mut field_tracker);
    user_object = user_object.field(Field::new(fname, TypeRef::named_nn(TypeRef::ID), |ctx| {
        FieldFuture::new(async move {
            let u = ctx.parent_value.try_downcast_ref::<User>()?;
            Ok(Some(GqlValue::from(u.id.to_string())))
        })
    }));

    let fname = get_unique_field_name("_AuthUser", "email", &mut field_tracker);
    user_object = user_object.field(Field::new(
        fname,
        TypeRef::named_nn(TypeRef::STRING),
        |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<User>()?;
                Ok(Some(GqlValue::from(u.email.clone())))
            })
        },
    ));

    let fname = get_unique_field_name("_AuthUser", "role", &mut field_tracker);
    user_object = user_object.field(Field::new(
        fname,
        TypeRef::named_nn(TypeRef::STRING),
        |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<User>()?;
                Ok(Some(GqlValue::from(u.role.clone())))
            })
        },
    ));

    // --- 3. HYDRATE USER REVERSE RELATIONS ---
    for (col_id, col_name, owner_field, target_policy) in &user_reverse_fields {
        let col_id = *col_id;
        let owner_field_clone = owner_field.clone();
        let t_policy = target_policy.clone();
        let list_type = format!("{}List", capitalize(col_name));

        let is_duplicate = *user_field_names_count.get(col_name).unwrap_or(&0) > 1;
        let base_key = if is_duplicate {
            format!("{}_via_{}", col_name, owner_field)
        } else {
            col_name.clone()
        };

        let fname = get_unique_field_name("_AuthUser", &base_key, &mut field_tracker);

        user_object = user_object.field(
            Field::new(fname, TypeRef::named(&list_type), move |ctx| {
                let s_col_id = col_id;
                let s_owner_field = owner_field_clone.clone();
                let policy_rule = t_policy.clone();

                FieldFuture::new(async move {
                    let state = ctx.data::<AppState>().unwrap().clone();
                    let parent_user = ctx.parent_value.try_downcast_ref::<User>()?;
                    let claims = ctx.data::<Claims>().ok();

                    let rls_sql = policies::compile_to_sql(
                        &policy_rule,
                        claims,
                        None,
                        Some(state.db.clone()),
                    )
                    .await
                    .unwrap_or("1=0".to_string());
                    if rls_sql == "1=0" {
                        return Ok(Some(FieldValue::owned_any(ListResult {
                            items: vec![],
                            total: 0,
                        })));
                    }

                    let limit = ctx.args.get("limit").and_then(|v| v.u64().ok());
                    let offset = ctx.args.get("offset").and_then(|v| v.u64().ok());

                    let user_filter = serde_json::json!({ s_owner_field.clone(): parent_user.id });

                    let filter_str = match ctx.args.get("where") {
                        Some(accessor) => {
                            let mut user_where = accessor
                                .deserialize::<serde_json::Value>()
                                .unwrap_or(serde_json::json!({}));
                            if let Some(obj) = user_where.as_object_mut() {
                                obj.insert(
                                    s_owner_field.clone(),
                                    serde_json::json!(parent_user.id),
                                );
                            } else {
                                user_where = user_filter;
                            }
                            Some(user_where.to_string())
                        }
                        None => Some(user_filter.to_string()),
                    };

                    let mut options = QueryOptions::default();
                    options.limit = limit.or(Some(100));
                    options.offset = offset;
                    options.filter = filter_str;
                    options.rls_sql = Some(rls_sql);

                    let result = state
                        .db
                        .list_records(s_col_id, options)
                        .await
                        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

                    Ok(Some(FieldValue::owned_any(result)))
                })
            })
            .argument(InputValue::new("limit", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("where", TypeRef::named("JSON"))),
        );
    }

    let mut user_list = Object::new("_AuthUserList");
    let fname_total = get_unique_field_name("_AuthUserList", "total", &mut field_tracker);
    let fname_items = get_unique_field_name("_AuthUserList", "items", &mut field_tracker);

    user_list = user_list
        .field(Field::new(
            fname_total,
            TypeRef::named_nn(TypeRef::INT),
            |ctx| {
                FieldFuture::new(async move {
                    let total = ctx.parent_value.try_downcast_ref::<(i64, Vec<User>)>()?.0;
                    Ok(Some(GqlValue::from(total)))
                })
            },
        ))
        .field(Field::new(
            fname_items,
            TypeRef::named_nn_list("_AuthUser"),
            |ctx| {
                FieldFuture::new(async move {
                    let items = &ctx.parent_value.try_downcast_ref::<(i64, Vec<User>)>()?.1;
                    Ok(Some(FieldValue::list(
                        items.iter().map(|u| FieldValue::owned_any(u.clone())),
                    )))
                })
            },
        ));
    schema_builder = schema_builder.register(user_list);

    let u_list_policy = user_read_policy.clone();
    let fname_users = get_unique_field_name("Query", "_users", &mut field_tracker);

    query_root = query_root.field(
        Field::new(fname_users, TypeRef::named("_AuthUserList"), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();
            let policy = u_list_policy.clone();

            FieldFuture::new(async move {
                let claims = ctx.data::<Claims>().ok();

                if !policies::check_access(&policy, claims, None, None, Some(state.db.clone()))
                    .await
                {
                    return Err(async_graphql::Error::new(
                        "Forbidden: Access denied by policy",
                    ));
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

    // --- 4. BUILD COLLECTION OBJECTS (QUERIES & MUTATIONS) ---
    for col in &collections {
        let type_name = capitalize(&col.name);
        let col_id = col.id;

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

        let fname_id = get_unique_field_name(&type_name, "id", &mut field_tracker);
        object = object.field(Field::new(
            fname_id,
            TypeRef::named_nn(TypeRef::ID),
            |ctx| {
                FieldFuture::new(async move {
                    let record = ctx.parent_value.try_downcast_ref::<Record>()?;
                    Ok(Some(GqlValue::from(record.id.to_string())))
                })
            },
        ));

        let create_input_name = format!("Create{}Input", type_name);
        let update_input_name = format!("Update{}Input", type_name);

        let mut create_input = InputObject::new(&create_input_name);
        let mut update_input = InputObject::new(&update_input_name);

        let mut create_fields_count = 0;
        let mut update_fields_count = 0;

        if let Some(schema) = &col.schema {
            for (field_name, def) in &schema.fields {
                if field_name == "id" {
                    continue;
                }

                let name_clone = field_name.clone();
                let is_owner = def.r#type == FieldType::Owner;
                let fname = get_unique_field_name(&type_name, field_name, &mut field_tracker);

                if is_owner {
                    let u_read_policy = user_read_policy.clone();

                    let field = Field::new(fname, TypeRef::named("_AuthUser"), move |ctx| {
                        let name = name_clone.clone();
                        let policy_rule = u_read_policy.clone();

                        // Extract state from the GraphQL Context dynamically
                        let state = ctx.data::<AppState>().unwrap().clone();
                        let db_for_policy = state.db.clone();

                        FieldFuture::new(async move {
                            let record = ctx.parent_value.try_downcast_ref::<Record>()?;

                            let user_id = record
                                .data
                                .get(&name)
                                .and_then(|v| v.as_i64())
                                .or_else(|| {
                                    record
                                        .data
                                        .get(&name)
                                        .and_then(|v| v.as_f64())
                                        .map(|f| f as i64)
                                })
                                .or_else(|| {
                                    record
                                        .data
                                        .get(&name)
                                        .and_then(|v| v.as_str())
                                        .and_then(|s| s.parse::<i64>().ok())
                                });

                            if let Some(uid) = user_id {
                                let loader = ctx.data::<DataLoader<UserLoader>>().unwrap();
                                let claims = ctx.data::<Claims>().ok();

                                let user_opt = loader
                                    .load_one(uid)
                                    .await
                                    .map_err(async_graphql::Error::new)?;

                                if let Some(u) = user_opt {
                                    let u_val = serde_json::json!({
                                        "id": u.id,
                                        "email": u.email,
                                        "role": u.role,
                                        "metadata": u.metadata
                                    });

                                    // Pass the extracted db_for_policy
                                    if policies::check_access(
                                        &policy_rule,
                                        claims,
                                        Some(&u_val),
                                        None,
                                        Some(db_for_policy),
                                    )
                                    .await
                                    {
                                        return Ok(Some(FieldValue::owned_any(u)));
                                    }
                                }
                                Ok(None)
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
                        FieldType::Json => "JSON",
                        _ => TypeRef::STRING,
                    };
                    let field = Field::new(fname, TypeRef::named(gql_type), move |ctx| {
                        let name = name_clone.clone();
                        FieldFuture::new(async move {
                            let record = ctx.parent_value.try_downcast_ref::<Record>()?;
                            map_json_to_gql(record.data.get(&name).cloned())
                        })
                    });
                    object = object.field(field);
                }

                let input_type = match def.r#type {
                    FieldType::Number => TypeRef::FLOAT,
                    FieldType::Boolean => TypeRef::BOOLEAN,
                    _ => TypeRef::STRING,
                };

                if !is_owner || !def.auto {
                    let create_type_ref = if def.required {
                        TypeRef::named_nn(input_type)
                    } else {
                        TypeRef::named(input_type)
                    };
                    create_input = create_input.field(InputValue::new(field_name, create_type_ref));
                    create_fields_count += 1;
                }

                update_input =
                    update_input.field(InputValue::new(field_name, TypeRef::named(input_type)));
                update_fields_count += 1;
            }

            for (rel_name, rel_def) in &schema.relations {
                let raw_target = &rel_def.target_collection;
                let resolved_target_name = col_id_to_name.get(raw_target).unwrap_or(raw_target);
                let target_type_name = capitalize(resolved_target_name);
                let t_col_name = resolved_target_name.clone();
                let origin_col_id = col.id;
                let r_name = rel_name.clone();
                let is_list = rel_def.relation_type == RelationType::Many;

                let target_read_policy = collections
                    .iter()
                    .find(|c| &c.name == resolved_target_name)
                    .and_then(|c| c.schema.as_ref())
                    .map(|s| s.policies.read.clone())
                    .unwrap_or("public".to_string());

                let type_ref = if is_list {
                    TypeRef::List(Box::new(TypeRef::named(&target_type_name)))
                } else {
                    TypeRef::named(&target_type_name)
                };

                let fname = get_unique_field_name(&type_name, rel_name, &mut field_tracker);

                let field = Field::new(fname, type_ref, move |ctx| {
                    let r_name = r_name.clone();
                    let t_col_name = t_col_name.clone();
                    let policy_rule = target_read_policy.clone();

                    // Extract state from the GraphQL Context dynamically
                    let state = ctx.data::<AppState>().unwrap().clone();
                    let db_for_policy = state.db.clone();

                    FieldFuture::new(async move {
                        let record = ctx.parent_value.try_downcast_ref::<Record>()?.clone();
                        let claims = ctx.data::<Claims>().ok();

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

                        let mut filtered_records = Vec::new();
                        for r in records {
                            // Clone the extracted db_for_policy per loop iteration
                            if policies::check_access(
                                &policy_rule,
                                claims,
                                Some(&r.data),
                                None,
                                Some(db_for_policy.clone()),
                            )
                            .await
                            {
                                filtered_records.push(r);
                            }
                        }

                        if is_list {
                            Ok(Some(FieldValue::list(
                                filtered_records.into_iter().map(FieldValue::owned_any),
                            )))
                        } else {
                            Ok(filtered_records
                                .into_iter()
                                .next()
                                .map(FieldValue::owned_any))
                        }
                    })
                });
                object = object.field(field);

                let create_rel_type = if is_list {
                    if rel_def.required {
                        TypeRef::named_nn_list_nn(TypeRef::ID)
                    } else {
                        TypeRef::named_list(TypeRef::ID)
                    }
                } else {
                    if rel_def.required {
                        TypeRef::named_nn(TypeRef::ID)
                    } else {
                        TypeRef::named(TypeRef::ID)
                    }
                };

                let update_rel_type = if is_list {
                    TypeRef::named_list(TypeRef::ID)
                } else {
                    TypeRef::named(TypeRef::ID)
                };

                create_input = create_input.field(InputValue::new(rel_name, create_rel_type));
                create_fields_count += 1;

                update_input = update_input.field(InputValue::new(rel_name, update_rel_type));
                update_fields_count += 1;
            }
        }

        // --- 4A. HYDRATE COLLECTION REVERSE RELATIONS ---
        if let Some(rev_rels) = reverse_relations_map.get(&col_id) {
            let mut field_names_count: HashMap<String, usize> = HashMap::new();
            for (_, origin_col_name, _, _) in rev_rels {
                *field_names_count
                    .entry(origin_col_name.clone())
                    .or_insert(0) += 1;
            }

            for (origin_col_id, origin_col_name, rel_field, origin_policy) in rev_rels {
                let origin_col_id = *origin_col_id;
                let rel_field_clone = rel_field.clone();
                let o_policy = origin_policy.clone();
                let list_type = format!("{}List", capitalize(origin_col_name));

                let is_duplicate = *field_names_count.get(origin_col_name).unwrap_or(&0) > 1;
                let base_key = if is_duplicate {
                    format!("{}_via_{}", origin_col_name, rel_field)
                } else {
                    origin_col_name.clone()
                };

                let fname = get_unique_field_name(&type_name, &base_key, &mut field_tracker);

                object = object.field(
                    Field::new(fname, TypeRef::named(&list_type), move |ctx| {
                        let s_col_id = origin_col_id;
                        let s_rel_field = rel_field_clone.clone();
                        let policy_rule = o_policy.clone();

                        FieldFuture::new(async move {
                            let state = ctx.data::<AppState>().unwrap().clone();
                            let parent_record = ctx.parent_value.try_downcast_ref::<Record>()?;
                            let claims = ctx.data::<Claims>().ok();

                            let rls_sql = policies::compile_to_sql(
                                &policy_rule,
                                claims,
                                None,
                                Some(state.db.clone()),
                            )
                            .await
                            .unwrap_or("1=0".to_string());
                            if rls_sql == "1=0" {
                                return Ok(Some(FieldValue::owned_any(ListResult {
                                    items: vec![],
                                    total: 0,
                                })));
                            }

                            let limit = ctx.args.get("limit").and_then(|v| v.u64().ok());
                            let offset = ctx.args.get("offset").and_then(|v| v.u64().ok());

                            let rel_filter =
                                serde_json::json!({ s_rel_field.clone(): parent_record.id });

                            let filter_str = match ctx.args.get("where") {
                                Some(accessor) => {
                                    let mut user_where = accessor
                                        .deserialize::<serde_json::Value>()
                                        .unwrap_or(serde_json::json!({}));
                                    if let Some(obj) = user_where.as_object_mut() {
                                        obj.insert(
                                            s_rel_field.clone(),
                                            serde_json::json!(parent_record.id),
                                        );
                                    } else {
                                        user_where = rel_filter;
                                    }
                                    Some(user_where.to_string())
                                }
                                None => Some(rel_filter.to_string()),
                            };

                            let mut options = QueryOptions::default();
                            options.limit = limit.or(Some(100));
                            options.offset = offset;
                            options.filter = filter_str;
                            options.rls_sql = Some(rls_sql);

                            let result = state
                                .db
                                .list_records(s_col_id, options)
                                .await
                                .map_err(|e| async_graphql::Error::new(e.to_string()))?;

                            Ok(Some(FieldValue::owned_any(result)))
                        })
                    })
                    .argument(InputValue::new("limit", TypeRef::named(TypeRef::INT)))
                    .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT)))
                    .argument(InputValue::new("where", TypeRef::named("JSON"))),
                );
            }
        }

        if create_fields_count == 0 {
            create_input = create_input.field(InputValue::new(
                "clientMutationId",
                TypeRef::named(TypeRef::STRING),
            ));
        }
        if update_fields_count == 0 {
            update_input = update_input.field(InputValue::new(
                "clientMutationId",
                TypeRef::named(TypeRef::STRING),
            ));
        }

        collection_objects.insert(type_name.clone(), object);
        schema_builder = schema_builder.register(create_input);
        schema_builder = schema_builder.register(update_input);

        // List Type
        let list_type_name = format!("{}List", type_name);
        let mut list_object = Object::new(&list_type_name);

        let fname_total = get_unique_field_name(&list_type_name, "total", &mut field_tracker);
        list_object = list_object.field(Field::new(
            fname_total,
            TypeRef::named_nn(TypeRef::INT),
            |ctx| {
                FieldFuture::new(async move {
                    let res = ctx.parent_value.try_downcast_ref::<ListResult>()?;
                    Ok(Some(GqlValue::from(res.total)))
                })
            },
        ));

        let fname_items = get_unique_field_name(&list_type_name, "items", &mut field_tracker);
        list_object = list_object.field(Field::new(
            fname_items,
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

        let fname_q = get_unique_field_name("Query", &query_name, &mut field_tracker);
        let list_field = Field::new(fname_q, TypeRef::named(&list_type_name), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();

            FieldFuture::new({
                let value = policy_clone.clone();
                async move {
                    let claims = ctx.data::<Claims>().ok();

                    let rls_sql =
                        policies::compile_to_sql(&value, claims, None, Some(state.db.clone()))
                            .await
                            .map_err(|e| async_graphql::Error::new(e))?;
                    if rls_sql == "1=0" {
                        return Err(async_graphql::Error::new("Forbidden"));
                    }

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
                    options.rls_sql = Some(rls_sql);

                    let result = state
                        .db
                        .list_records(col_id, options)
                        .await
                        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                    Ok(Some(FieldValue::owned_any(result)))
                }
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

        let fname_c = get_unique_field_name("Mutation", &create_name, &mut field_tracker);
        let create_mutation = Field::new(fname_c, TypeRef::named(&type_name), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();
            let claims = ctx.data::<Claims>().ok().cloned();
            let p = c_policy.clone();
            let sch = c_schema.clone();

            FieldFuture::new(async move {
                let input_val = ctx
                    .args
                    .get("data")
                    .ok_or(async_graphql::Error::new("Missing data"))?;
                let json_data = gql_input_to_json(input_val.as_value().clone());

                if !policies::check_access(
                    &p,
                    claims.as_ref(),
                    None,
                    Some(&json_data),
                    Some(state.db.clone()),
                )
                .await
                {
                    return Err(async_graphql::Error::new("Forbidden: Create denied"));
                }

                let input_val = ctx
                    .args
                    .get("data")
                    .ok_or(async_graphql::Error::new("Missing data"))?;
                let json_data = gql_input_to_json(input_val.as_value().clone());

                let uid = claims.as_ref().map(|c| c.uid);
                let final_data = prepare_data_for_create(json_data, &sch, uid);

                let id = state
                    .db
                    .create_record(c_col_id, &final_data)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;

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

        let fname_u = get_unique_field_name("Mutation", &update_name, &mut field_tracker);
        let update_mutation = Field::new(fname_u, TypeRef::named(&type_name), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();
            let claims = ctx.data::<Claims>().ok().cloned();
            let p = u_policy.clone();

            FieldFuture::new(async move {
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

                let existing = state
                    .db
                    .get_record(u_col_id, id, None)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?
                    .ok_or(async_graphql::Error::new("Record not found"))?;

                if !policies::check_access(
                    &p,
                    claims.as_ref(),
                    Some(&existing.data),
                    Some(&json_data),
                    Some(state.db.clone()),
                )
                .await
                {
                    return Err(async_graphql::Error::new("Forbidden: Update denied"));
                }

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

        let fname_d = get_unique_field_name("Mutation", &delete_name, &mut field_tracker);
        let delete_mutation = Field::new(fname_d, TypeRef::named(TypeRef::BOOLEAN), move |ctx| {
            let state = ctx.data::<AppState>().unwrap().clone();
            let claims = ctx.data::<Claims>().ok().cloned();
            let p = d_policy.clone();

            FieldFuture::new(async move {
                let id_str: String = ctx
                    .args
                    .get("id")
                    .and_then(|v| v.string().ok().map(|s| s.to_string()))
                    .ok_or(async_graphql::Error::new("ID required"))?;

                let id = id_str
                    .parse::<i64>()
                    .map_err(|_| async_graphql::Error::new("Invalid ID format"))?;

                let existing = state
                    .db
                    .get_record(d_col_id, id, None)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?
                    .ok_or(async_graphql::Error::new("Record not found"))?;

                if !policies::check_access(
                    &p,
                    claims.as_ref(),
                    Some(&existing.data),
                    None,
                    Some(state.db.clone()),
                )
                .await
                {
                    return Err(async_graphql::Error::new("Forbidden: Delete denied"));
                }

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

    // --- 5. FETCH & INJECT CUSTOM RESOLVERS FROM SCRIPTS ---
    let scripts = state.db.list_scripts().await.unwrap_or_default();

    for script in scripts {
        if script.trigger_type == "graphql"
            && script.active
            && let Some(config) = extract_script_config(&script.code)
        {
            let script_name = script.name.clone();
            let args_def = config.args.clone().unwrap_or_default();
            let return_type = map_type_ref(&config.return_type);

            // Create Field
            let parent_obj_name = config.parent.clone();
            let fname = get_unique_field_name(&parent_obj_name, &config.name, &mut field_tracker);

            let mut field = Field::new(fname, return_type, move |ctx| {
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

            match parent_obj_name.as_str() {
                "Query" => {
                    query_root = query_root.field(field);
                }
                "Mutation" => {
                    mutation_root = mutation_root.field(field);
                }
                "User" | "_AuthUser" => {
                    user_object = user_object.field(field);
                }
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
