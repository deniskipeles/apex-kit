use super::ApexKit;
use crate::database::sqlite::utils::{
    commit_uniqueness, enforce_uniqueness, row_to_collection, row_to_record,
};
use crate::database::traits::{CollectionStore, IntoSqlVal, RecordStore};
use crate::models::schema::CollectionSchema;
use crate::models::{ListResult, Record};
use crate::query::QueryOptions;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

#[async_trait]
impl RecordStore for ApexKit {
    async fn create_record(
        &self,
        collection_id: i64,
        data: &Value,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self._write_record_internal(collection_id, None, data, true)
            .await
    }

    async fn import_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self._write_record_internal(collection_id, Some(record_id), data, false)
            .await?;
        Ok(())
    }

    async fn list_records(
        &self,
        collection_id: i64,
        options: QueryOptions,
    ) -> std::result::Result<ListResult, Box<dyn std::error::Error + Send + Sync>> {
        let (mut records, total) = {
            let conn = self.get_data_read().await;

            let mut schema_map = HashMap::new();
            let mut id_map = HashMap::new();

            let current_col_name = {
                let mut stmt = conn.prepare("SELECT name FROM collections WHERE id = ?1")?;
                let mut rows = stmt.query(rusqlite::params![collection_id])?;
                if let Some(row) = rows.next()? {
                    row.get::<_, String>(0)?
                } else {
                    return Ok(ListResult {
                        items: vec![],
                        total: 0,
                    });
                }
            };

            if let Some(ref ex) = options.expand {
                if !ex.trim().is_empty() {
                    let mut stmt = conn.prepare("SELECT id,name,schema FROM collections")?;
                    let mut rows = stmt.query([])?;
                    while let Some(row) = rows.next()? {
                        let id: i64 = row.get(0)?;
                        let name: String = row.get(1)?;
                        let schema_str: Option<String> = row.get(2)?;
                        let schema = match schema_str {
                            Some(s) => serde_json::from_str(&s).unwrap_or_default(),
                            None => CollectionSchema::default(),
                        };

                        schema_map.insert(name.clone(), schema.clone());
                        schema_map.insert(id.to_string(), schema);
                        id_map.insert(name, id);
                        id_map.insert(id.to_string(), id);
                    }
                }
            }

            let builder = crate::query::SqlBuilder::new(
                collection_id,
                &current_col_name,
                options.clone(),
                &schema_map,
                &id_map,
            );

            let mut count_stmt = conn.prepare(&builder.count_sql)?;
            let mut count_rows =
                count_stmt.query(rusqlite::params_from_iter(builder.params.clone()))?;
            let total = if let Some(row) = count_rows.next()? {
                row.get::<usize, i64>(0)?
            } else {
                0
            };

            let mut stmt = conn.prepare(&builder.base_sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(builder.params))?;
            let mut recs = Vec::new();
            while let Some(row) = rows.next()? {
                recs.push(row_to_record(row)?);
            }
            (recs, total)
        };

        super::users::populate_owners_in_memory(
            self,
            &mut records,
            collection_id,
            options.expand.as_ref(),
        )
        .await?;

        Ok(ListResult {
            items: records,
            total,
        })
    }

    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
        expand: Option<String>,
    ) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let mut record_opt = {
            let conn = self.get_data_read().await;

            if expand.is_none() || expand.as_ref().unwrap().trim().is_empty() {
                let mut stmt = conn.prepare(
                    "SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ?1 AND id = ?2"
                )?;
                let mut rows = stmt.query(rusqlite::params![collection_id, record_id])?;
                match rows.next()? {
                    Some(row) => Some(row_to_record(row)?),
                    None => None,
                }
            } else {
                let expand_str = expand.clone().unwrap();

                let mut schema_map = HashMap::new();
                let mut id_map = HashMap::new();

                let current_col_name = {
                    let mut stmt = conn.prepare("SELECT name FROM collections WHERE id = ?1")?;
                    let mut rows = stmt.query(rusqlite::params![collection_id])?;
                    if let Some(row) = rows.next()? {
                        row.get::<_, String>(0)?
                    } else {
                        return Ok(None);
                    }
                };

                {
                    let mut stmt = conn.prepare("SELECT id,name,schema FROM collections")?;
                    let mut rows = stmt.query([])?;
                    while let Some(row) = rows.next()? {
                        let id: i64 = row.get(0)?;
                        let name: String = row.get(1)?;
                        let schema_str: Option<String> = row.get(2)?;
                        let schema = match schema_str {
                            Some(s) => serde_json::from_str(&s).unwrap_or_default(),
                            None => CollectionSchema::default(),
                        };
                        schema_map.insert(name.clone(), schema.clone());
                        schema_map.insert(id.to_string(), schema);
                        id_map.insert(name, id);
                        id_map.insert(id.to_string(), id);
                    }
                }

                let paths = crate::query::builder::smart_split(&expand_str);
                let expand_json_sql = crate::query::builder::build_expand_json_object(
                    paths,
                    "records",
                    0,
                    &current_col_name,
                    collection_id,
                    &schema_map,
                    &id_map,
                );

                let sql = format!(
                    "SELECT id,json(data),{},created,updated FROM records WHERE collection_id = ?1 AND id = ?2",
                    expand_json_sql
                );

                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(rusqlite::params![collection_id, record_id])?;
                match rows.next()? {
                    Some(row) => Some(row_to_record(row)?),
                    None => None,
                }
            }
        };

        if let Some(rec) = &mut record_opt {
            let mut single_vec = vec![rec.clone()];
            super::users::populate_owners_in_memory(
                self,
                &mut single_vec,
                collection_id,
                expand.as_ref(),
            )
            .await?;
            *rec = single_vec.pop().unwrap();
        }

        Ok(record_opt)
    }

    async fn update_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
        let (col, existing) = {
            let conn = self.get_data_read().await;

            let col = {
                let mut stmt =
                    conn.prepare("SELECT id,name,schema FROM collections WHERE id = ?1")?;
                let mut rows = stmt.query(rusqlite::params![collection_id])?;
                if let Some(row) = rows.next()? {
                    Some(row_to_collection(row)?)
                } else {
                    None
                }
            }
            .ok_or("Collection not found")?;

            let existing = {
                let mut stmt = conn.prepare(
                    "SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ?1 AND id = ?2"
                )?;
                let mut rows = stmt.query(rusqlite::params![collection_id, record_id])?;
                if let Some(row) = rows.next()? {
                    Some(row_to_record(row)?)
                } else {
                    None
                }
            }
            .ok_or("Record not found")?;

            (col, existing)
        };

        let schema = col.schema.unwrap_or_default();

        let mut merged_data = existing.data.clone();
        if let Some(obj) = merged_data.as_object_mut() {
            if let Some(new_obj) = data.as_object() {
                for (k, v) in new_obj {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }

        if let Err(errors) = crate::validation::sanitize_and_validate(&schema, &mut merged_data) {
            let err_json = serde_json::to_string(&errors).unwrap();
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Schema Validation Failed: {}", err_json),
            )));
        }

        let jsonb_bytes = serde_sqlite_jsonb::to_vec(&merged_data).map_err(Box::new)?;

        {
            let conn = self.get_data_read().await;
            enforce_uniqueness(&conn, collection_id, Some(record_id), &merged_data, &schema)?;
        }

        self.data_batcher.execute(
            "UPDATE records SET data = ?1,updated = CURRENT_TIMESTAMP WHERE collection_id = ?2 AND id = ?3".into(),
            vec![rusqlite::types::Value::Blob(jsonb_bytes), collection_id.into_val(), record_id.into_val()]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)?;

        let unique_future = commit_uniqueness(
            &self.data_batcher,
            collection_id,
            record_id,
            &merged_data,
            &schema,
        );

        let sync_future = self.sync_relations(collection_id, record_id, &merged_data, &schema);

        tokio::try_join!(unique_future, sync_future)?;

        Ok(Record {
            id: record_id,
            data: merged_data,
            expand: None,
            created: existing.created,
            updated: chrono::Utc::now().to_rfc3339(),
        })
    }

    async fn delete_record(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };

        let mut to_delete = vec![(collection_id, record_id)];
        let mut visited = std::collections::HashSet::new();

        // Pre-load schemas to quickly analyze cascade conditions
        let collections = self.list_collections().await?;
        let mut schema_map = std::collections::HashMap::new();
        let mut name_to_id = std::collections::HashMap::new();

        for c in collections {
            if let Some(s) = c.schema {
                schema_map.insert(c.id, s);
            }
            name_to_id.insert(c.name.clone(), c.id);
            name_to_id.insert(c.id.to_string(), c.id);
        }

        // Process Deletions iteratively (Graph Traversal)
        while let Some((col_id, rec_id)) = to_delete.pop() {
            if visited.contains(&(col_id, rec_id)) {
                continue;
            }
            visited.insert((col_id, rec_id));

            // Find any records that point to THIS record and have cascade_on_target_delete = true
            for (other_col_id, other_schema) in &schema_map {
                for (rel_name, rel_def) in &other_schema.relations {
                    if rel_def.cascade_on_target_delete {
                        let targets_us = match name_to_id.get(&rel_def.target_collection) {
                            Some(id) => *id == col_id,
                            None => false,
                        };

                        if targets_us {
                            let origin_ids: Vec<i64> = {
                                let conn = self.get_data_read().await;
                                let mut stmt = conn.prepare("SELECT origin_rec_id FROM _relations WHERE origin_col_id = ?1 AND target_col_id = ?2 AND target_rec_id = ?3 AND rel_name = ?4").map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                                let mut rows = stmt
                                    .query(rusqlite::params![
                                        other_col_id,
                                        col_id,
                                        rec_id,
                                        rel_name
                                    ])
                                    .map_err(|e| {
                                        Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                                    })?;
                                let mut ids = Vec::new();
                                while let Some(row) = rows.next().map_err(|e| {
                                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                                })? {
                                    ids.push(row.get(0).map_err(|e| {
                                        Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                                    })?);
                                }
                                ids
                            };

                            for origin_rec_id in origin_ids {
                                if !visited.contains(&(*other_col_id, origin_rec_id)) {
                                    to_delete.push((*other_col_id, origin_rec_id));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Execute all collected deletions
        for (del_col_id, del_rec_id) in visited {
            let f1 = self.data_batcher.execute(
                "DELETE FROM records WHERE collection_id = ?1 AND id = ?2".into(),
                vec![del_col_id.into_val(), del_rec_id.into_val()],
            );
            let f2 = self.data_batcher.execute(
                "DELETE FROM _unique_values WHERE record_id = ?1".into(),
                vec![del_rec_id.into_val()],
            );
            let f3 = self.data_batcher.execute(
                "DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2".into(),
                vec![del_col_id.into_val(), del_rec_id.into_val()],
            );
            let f4 = self.data_batcher.execute(
                "DELETE FROM _relations WHERE target_col_id=?1 AND target_rec_id=?2".into(),
                vec![del_col_id.into_val(), del_rec_id.into_val()],
            );
            let f5 = self.vector_batcher.execute(
                "DELETE FROM vectors WHERE collection_id = ?1 AND record_id = ?2".into(),
                vec![del_col_id.into_val(), del_rec_id.into_val()],
            );

            let _ = tokio::try_join!(f1, f2, f3, f4).map_err(map_err)?;
            let _ = f5.await.map_err(map_err)?;

            // Keep Tantivy Search Engine in sync
            let _ = self.search.delete_record(del_col_id, del_rec_id);
        }

        Ok(())
    }
}

// --- Internal Helper Implementation ---
impl ApexKit {
    pub(crate) async fn _write_record_internal(
        &self,
        collection_id: i64,
        record_id: Option<i64>,
        data: &Value,
        validate: bool,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let col = {
            let conn = self.get_data_read().await;
            let mut stmt = conn.prepare("SELECT id,name,schema FROM collections WHERE id = ?1")?;
            let mut rows = stmt.query(rusqlite::params![collection_id])?;
            if let Some(row) = rows.next()? {
                Some(row_to_collection(row)?)
            } else {
                None
            }
        }
        .ok_or("Collection not found")?;

        let schema = col.schema.unwrap_or_default();
        let mut final_data = data.clone();

        if let Some(obj) = final_data.as_object_mut() {
            obj.remove("id");
            obj.remove("_id");
            obj.remove("created");
            obj.remove("updated");
            obj.remove("expand");
            obj.remove("collectionId");
            obj.remove("collectionName");
        }

        if validate {
            if let Err(errors) = crate::validation::sanitize_and_validate(&schema, &mut final_data)
            {
                let err_json = serde_json::to_string(&errors).unwrap();
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Schema Validation Failed: {}", err_json),
                )));
            }
        }

        let jsonb_bytes = serde_sqlite_jsonb::to_vec(&final_data).map_err(Box::new)?;

        {
            let conn = self.get_data_read().await;
            enforce_uniqueness(&conn, collection_id, record_id, &final_data, &schema)?;
        }

        let actual_record_id = if let Some(rid) = record_id {
            self.data_batcher
                .execute(
                    "INSERT INTO records (id,collection_id,data) VALUES (?1,?2,?3)".into(),
                    vec![
                        rid.into_val(),
                        collection_id.into_val(),
                        rusqlite::types::Value::Blob(jsonb_bytes),
                    ],
                )
                .await?;
            rid
        } else {
            self.data_batcher
                .insert(
                    "INSERT INTO records (collection_id,data) VALUES (?1,?2)".into(),
                    vec![
                        collection_id.into_val(),
                        rusqlite::types::Value::Blob(jsonb_bytes),
                    ],
                )
                .await?
        };

        let unique_future = commit_uniqueness(
            &self.data_batcher,
            collection_id,
            actual_record_id,
            &final_data,
            &schema,
        );

        let relation_future =
            self.sync_relations(collection_id, actual_record_id, &final_data, &schema);

        tokio::try_join!(unique_future, relation_future)?;

        Ok(actual_record_id)
    }

    pub(crate) async fn sync_relations(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
        schema: &CollectionSchema,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for (rel_name, rel_def) in &schema.relations {
            if let Some(val) = data.get(rel_name) {
                self.data_batcher.execute(
                    "DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND rel_name=?3".into(),
                    vec![collection_id.into_val(), record_id.into_val(), rel_name.clone().into_val()]
                ).await.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)?;

                let mut target_ids = Vec::new();
                if let Value::Array(arr) = val {
                    for v in arr {
                        let tid = match v {
                            Value::String(s) => s.parse::<i64>().unwrap_or(0),
                            Value::Number(n) => n.as_i64().unwrap_or(0),
                            _ => 0,
                        };
                        if tid != 0 {
                            target_ids.push(tid);
                        }
                    }
                } else {
                    let tid = match val {
                        Value::String(s) => s.parse::<i64>().unwrap_or(0),
                        Value::Number(n) => n.as_i64().unwrap_or(0),
                        _ => 0,
                    };
                    if tid != 0 {
                        target_ids.push(tid);
                    }
                }

                if target_ids.is_empty() {
                    continue;
                }

                let identifier = &rel_def.target_collection;
                let mut target_col_id: Option<i64> = None;

                {
                    let conn = self.get_data_read().await;
                    let mut name_stmt =
                        conn.prepare("SELECT id FROM collections WHERE name = ?1")?;
                    let mut name_rows = name_stmt.query(rusqlite::params![identifier.clone()])?;
                    if let Some(r) = name_rows.next()? {
                        target_col_id = Some(r.get(0)?);
                    } else if let Ok(id_num) = identifier.parse::<i64>() {
                        let mut id_stmt =
                            conn.prepare("SELECT id FROM collections WHERE id = ?1")?;
                        let mut id_rows = id_stmt.query(rusqlite::params![id_num])?;
                        if id_rows.next()?.is_some() {
                            target_col_id = Some(id_num);
                        }
                    }
                }

                if let Some(tc_id) = target_col_id {
                    for target_rec_id in target_ids {
                        self.data_batcher.execute(
                            "INSERT OR IGNORE INTO _relations (origin_col_id,origin_rec_id,target_col_id,target_rec_id,rel_name) VALUES (?1,?2,?3,?4,?5)".into(),
                            vec![collection_id.into_val(), record_id.into_val(), tc_id.into_val(), target_rec_id.into_val(), rel_name.clone().into_val()]
                        ).await.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)?;
                    }
                }
            }
        }
        Ok(())
    }
}
