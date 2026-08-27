use super::super::traits::IntoSqlVal;
use crate::models::schema::CollectionSchema;
use crate::models::{Collection, Record};
use crate::{COMPOSITE_SEPARATOR, batching};
use rusqlite::{Connection, Row};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn row_to_collection(
    row: &Row,
) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
    let schema_str: Option<String> = row.get(2)?;
    let schema = match schema_str {
        Some(s) => serde_json::from_str(&s)?,
        None => None,
    };
    let index: Option<String> = row.get(3).ok();

    Ok(Collection {
        id: row.get(0)?,
        name: row.get(1)?,
        schema,
        index,
    })
}

pub fn row_to_record(
    row: &Row,
) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
    let id = row.get(0)?;

    let val_data = row
        .get_ref(1)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // OPTIMIZED: If the data is stored in SQLite's native JSONB format (Blob),
    // deserialize it directly into a serde_json::Value without converting to a string first.
    let data: serde_json::Value = match val_data {
        rusqlite::types::ValueRef::Blob(b) => {
            // Directly deserialize from the SQLite JSONB binary slice
            match serde_sqlite_jsonb::from_slice(b) {
                Ok(val) => val,
                Err(_) => {
                    // Fallback: If it's a standard text blob masquerading as a blob, try standard parsing
                    serde_json::from_slice(b).unwrap_or(serde_json::json!({}))
                }
            }
        }
        rusqlite::types::ValueRef::Text(s) => {
            // Fallback for legacy text data or if the query used `json(data)`
            serde_json::from_str(std::str::from_utf8(s).unwrap_or("{}"))?
        }
        _ => serde_json::json!({}),
    };

    let expand = if let Ok(val_expand) = row.get_ref(2) {
        match val_expand {
            rusqlite::types::ValueRef::Blob(b) => match serde_sqlite_jsonb::from_slice(b) {
                Ok(val) => Some(val),
                Err(_) => Some(serde_json::from_slice(b).unwrap_or(serde_json::json!({}))),
            },
            rusqlite::types::ValueRef::Text(s) => Some(serde_json::from_str(
                std::str::from_utf8(s).unwrap_or("{}"),
            )?),
            _ => None,
        }
    } else {
        None
    };

    let created: String = row.get(3).unwrap_or_else(|_| "".to_string());
    let updated: String = row.get(4).unwrap_or_else(|_| "".to_string());

    Ok(Record {
        id,
        data,
        expand,
        created,
        updated,
    })
}

pub fn calculate_dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total_size = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                total_size += calculate_dir_size(&entry.path())?;
            } else {
                total_size += metadata.len();
            }
        }
    } else if path.exists() {
        total_size = path.metadata()?.len();
    }
    Ok(total_size)
}

pub fn serialize_unique_val(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => val.to_string(),
    }
}

pub fn check_conflict(
    conn: &Connection,
    key: &str,
    val: &str,
    current_rec_id: Option<i64>,
    err_context: &str,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut stmt =
        conn.prepare("SELECT record_id FROM _unique_values WHERE index_key = ?1 AND value = ?2")?;
    let mut rows = stmt.query(rusqlite::params![key.to_string(), val.to_string()])?;
    if let Some(row) = rows.next()? {
        let existing_id: i64 = row.get(0)?;
        if Some(existing_id) != current_rec_id {
            let msg = format!(
                "Unique constraint violation: {} with value '{}' already exists.",
                err_context, val
            );
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                msg,
            )));
        }
    }
    Ok(())
}

pub fn enforce_uniqueness(
    conn: &Connection,
    col_id: i64,
    record_id: Option<i64>,
    data: &Value,
    schema: &CollectionSchema,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for (name, def) in &schema.fields {
        if def.unique.unwrap_or(false)
            && let Some(val) = data.get(name)
        {
            if val.is_null() {
                continue;
            }
            let val_str = serialize_unique_val(val);
            let index_key = format!("{}-{}", col_id, def.uid);
            check_conflict(
                conn,
                &index_key,
                &val_str,
                record_id,
                &format!("Field '{}'", name),
            )?;
        }
    }

    for (name, def) in &schema.relations {
        if def.relation_type == crate::models::schema::RelationType::One
            && let Some(val) = data.get(name)
        {
            if val.is_null() {
                continue;
            }
            let val_str = serialize_unique_val(val);
            let index_key = format!("{}-{}", col_id, def.uid);
            check_conflict(
                conn,
                &index_key,
                &val_str,
                record_id,
                &format!("Relation '{}'", name),
            )?;
        }
    }

    for field_group in &schema.composite_unique {
        let mut composite_uids = Vec::new();
        let mut composite_values = Vec::new();
        let mut missing_data = false;

        for field_name in field_group {
            let uid_opt = if let Some(def) = schema.fields.get(field_name) {
                Some(def.uid.clone())
            } else {
                schema.relations.get(field_name).map(|rel| rel.uid.clone())
            };

            if let Some(uid) = uid_opt {
                composite_uids.push(uid);
                if let Some(val) = data.get(field_name) {
                    if val.is_null() {
                        missing_data = true;
                        break;
                    }
                    composite_values.push(serialize_unique_val(val));
                } else {
                    missing_data = true;
                    break;
                }
            } else {
                missing_data = true;
                break;
            }
        }

        if missing_data {
            continue;
        }

        let mut map: BTreeMap<String, String> = BTreeMap::new();
        for (i, uid) in composite_uids.iter().enumerate() {
            map.insert(uid.clone(), composite_values[i].clone());
        }
        let sorted_uids: Vec<&String> = map.keys().collect();
        let sorted_vals: Vec<&String> = map.values().collect();
        let index_key = format!(
            "{}-{}",
            col_id,
            sorted_uids
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>()
                .join("-")
        );
        let value_str = sorted_vals
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>()
            .join(COMPOSITE_SEPARATOR);
        check_conflict(
            conn,
            &index_key,
            &value_str,
            record_id,
            &format!("Combination {:?}", field_group),
        )?;
    }
    Ok(())
}

pub async fn commit_uniqueness(
    batcher: &batching::WriteManager,
    col_id: i64,
    record_id: i64,
    data: &Value,
    schema: &CollectionSchema,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    batcher
        .execute(
            "DELETE FROM _unique_values WHERE record_id = ?1".into(),
            vec![record_id.into_val()],
        )
        .await
        .map_err(|e| {
            Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
        })?;

    for (name, def) in &schema.fields {
        if def.unique.unwrap_or(false)
            && let Some(val) = data.get(name)
            && !val.is_null()
        {
            let index_key = format!("{}-{}", col_id, def.uid);
            let val_str = serialize_unique_val(val);
            batcher
                .execute(
                    "INSERT INTO _unique_values (index_key, value, record_id) VALUES (?1, ?2, ?3)"
                        .into(),
                    vec![
                        index_key.into_val(),
                        val_str.into_val(),
                        record_id.into_val(),
                    ],
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;
        }
    }

    for (name, def) in &schema.relations {
        if def.relation_type == crate::models::schema::RelationType::One
            && let Some(val) = data.get(name)
            && !val.is_null()
        {
            let index_key = format!("{}-{}", col_id, def.uid);
            let val_str = serialize_unique_val(val);
            batcher
                .execute(
                    "INSERT INTO _unique_values (index_key, value, record_id) VALUES (?1, ?2, ?3)"
                        .into(),
                    vec![
                        index_key.into_val(),
                        val_str.into_val(),
                        record_id.into_val(),
                    ],
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;
        }
    }

    for field_group in &schema.composite_unique {
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        let mut missing = false;

        for field_name in field_group {
            let uid_opt = if let Some(def) = schema.fields.get(field_name) {
                Some(def.uid.clone())
            } else {
                schema.relations.get(field_name).map(|rel| rel.uid.clone())
            };

            if let Some(uid) = uid_opt {
                if let Some(val) = data.get(field_name) {
                    if val.is_null() {
                        missing = true;
                        break;
                    }
                    map.insert(uid, serialize_unique_val(val));
                } else {
                    missing = true;
                    break;
                }
            } else {
                missing = true;
                break;
            }
        }

        if !missing {
            let sorted_uids: Vec<&String> = map.keys().collect();
            let sorted_vals: Vec<&String> = map.values().collect();

            let index_key = format!(
                "{}-{}",
                col_id,
                sorted_uids
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<&str>>()
                    .join("-")
            );
            let value_str = sorted_vals
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>()
                .join(COMPOSITE_SEPARATOR);

            batcher
                .execute(
                    "INSERT INTO _unique_values (index_key, value, record_id) VALUES (?1, ?2, ?3)"
                        .into(),
                    vec![
                        index_key.into_val(),
                        value_str.into_val(),
                        record_id.into_val(),
                    ],
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;
        }
    }
    Ok(())
}

pub fn sql_index_name(col_id: i64, field_uid: &str) -> String {
    format!("idx_col_{}_{}", col_id, field_uid)
}

pub async fn reconcile_sql_indexes(
    batcher: &batching::WriteManager,
    col_id: i64,
    new_schema: &CollectionSchema,
    old_schema: Option<&CollectionSchema>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Create SQL indexes for standard fields
    for (name, def) in &new_schema.fields {
        if def.sql_indexed {
            let idx_name = sql_index_name(col_id, &def.uid);
            let exists = old_schema
                .map(|s| s.fields.get(name).map(|f| f.sql_indexed).unwrap_or(false))
                .unwrap_or(false);

            if !exists {
                let sql = format!(
                    "CREATE INDEX IF NOT EXISTS {} ON records (json_extract(data, '$.{}')) WHERE collection_id = {}",
                    idx_name, name, col_id
                );
                batcher.execute(sql, vec![]).await.map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;
            }
        }
    }

    // --- ADD: Create SQL indexes for relation fields ---
    for (name, def) in &new_schema.relations {
        if def.sql_indexed {
            let idx_name = sql_index_name(col_id, &def.uid);
            let exists = old_schema
                .map(|s| {
                    s.relations
                        .get(name)
                        .map(|f| f.sql_indexed)
                        .unwrap_or(false)
                })
                .unwrap_or(false);

            if !exists {
                let sql = format!(
                    "CREATE INDEX IF NOT EXISTS {} ON records (json_extract(data, '$.{}')) WHERE collection_id = {}",
                    idx_name, name, col_id
                );
                batcher.execute(sql, vec![]).await.map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;
            }
        }
    }

    // 2. Drop stale indexes
    if let Some(old) = old_schema {
        for (name, def) in &old.fields {
            let should_drop = if let Some(new_def) = new_schema.fields.get(name) {
                def.sql_indexed && !new_def.sql_indexed
            } else {
                def.sql_indexed
            };

            if should_drop {
                let idx_name = sql_index_name(col_id, &def.uid);
                let sql = format!("DROP INDEX IF EXISTS {}", idx_name);
                batcher.execute(sql, vec![]).await.map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;
            }
        }

        // --- ADD: Drop stale relation indexes ---
        for (name, def) in &old.relations {
            let should_drop = if let Some(new_def) = new_schema.relations.get(name) {
                def.sql_indexed && !new_def.sql_indexed
            } else {
                def.sql_indexed
            };

            if should_drop {
                let idx_name = sql_index_name(col_id, &def.uid);
                let sql = format!("DROP INDEX IF EXISTS {}", idx_name);
                batcher.execute(sql, vec![]).await.map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;
            }
        }
    }

    Ok(())
}
