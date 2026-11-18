use super::ApexKit;
use crate::database::sqlite::indexes::diagnostics::translate_constraint_error;
use crate::database::sqlite::indexes::field_for_index_name;
use crate::database::sqlite::utils::{row_to_collection, row_to_record};
use crate::database::traits::{CollectionStore, IntoSqlVal, RecordStore};
use crate::models::schema::CollectionSchema;
use crate::models::{ListResult, Record};
use crate::query::QueryOptions;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// Maps a raw SQLite write failure into the stable application error used by
/// the API layer. Uniqueness is now enforced by real expression indexes, so a
/// conflict surfaces as `UNIQUE constraint failed: index '<name>'`; the index
/// name is decoded back to its field name from the in-memory schema, so no
/// database round-trip is needed while a write lock is held. Any other failure
/// keeps its raw message and is not reported as a conflict.
fn write_error(
    raw: &str,
    collection_id: i64,
    schema: &CollectionSchema,
) -> Box<dyn std::error::Error + Send + Sync> {
    match translate_constraint_error(raw, |name| {
        field_for_index_name(collection_id, schema, name)
    }) {
        Some(msg) => Box::new(std::io::Error::new(std::io::ErrorKind::AlreadyExists, msg)),
        None => Box::new(std::io::Error::other(raw.to_string())),
    }
}

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
                // Applied SQL CAST logic for dynamic string/int matching
                // [V2 MIGRATION SAFE]: Cast the parameter instead of the column to maintain O(1) Index lookups
                let mut stmt = conn.prepare(
                    "SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ?1 AND id IN (?2, CAST(?2 AS TEXT), CAST(?2 AS INTEGER))"
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

                // Applied SQL CAST logic
                // [V2 MIGRATION SAFE]: Cast the parameter instead of the column to maintain O(1) Index lookups
                let sql = format!(
                    "SELECT id,json(data),{},created,updated FROM records WHERE collection_id = ?1 AND id IN (?2, CAST(?2 AS TEXT), CAST(?2 AS INTEGER))",
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
                // [V2 MIGRATION SAFE]: Cast the parameter instead of the column to maintain O(1) Index lookups
                let mut stmt = conn.prepare(
                    "SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ?1 AND id IN (?2, CAST(?2 AS TEXT), CAST(?2 AS INTEGER))"
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

        // Uniqueness is enforced by the expression indexes created at collection
        // creation/update time, so this UPDATE fails atomically on a conflict.
        // [V2 MIGRATION SAFE]: Cast the parameter instead of the column to maintain O(1) Index lookups
        if let Err(e) = self.data_batcher.execute(
            "UPDATE records SET data = ?1,updated = CURRENT_TIMESTAMP WHERE collection_id = ?2 AND id IN (?3, CAST(?3 AS TEXT), CAST(?3 AS INTEGER))".into(),
            vec![rusqlite::types::Value::Blob(jsonb_bytes), collection_id.into_val(), record_id.into_val()]
        ).await {
            return Err(write_error(&e, collection_id, &schema));
        }

        let sync_future = self.sync_relations(collection_id, record_id, &merged_data, &schema);

        if let Err(e) = sync_future.await {
            // COMPENSATING ROLLBACK: Restore previous data state if relation sync failed
            if let Ok(old_jsonb) = serde_sqlite_jsonb::to_vec(&existing.data) {
                let _ = self.data_batcher.execute(
                    "UPDATE records SET data = ?1, updated = ?2 WHERE collection_id = ?3 AND id IN (?4, CAST(?4 AS TEXT), CAST(?4 AS INTEGER))".into(),
                    vec![
                        rusqlite::types::Value::Blob(old_jsonb),
                        existing.updated.clone().into_val(),
                        collection_id.into_val(),
                        record_id.into_val()
                    ]
                ).await;
            }
            // Restore previous relations
            let _ = self
                .sync_relations(collection_id, record_id, &existing.data, &schema)
                .await;

            let err_msg = e.to_string();
            if err_msg.contains("UNIQUE constraint failed") {
                return Err(write_error(&err_msg, collection_id, &schema));
            }
            return Err(e);
        }

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

        // `cascade_on_target_delete` is a per-relation schema rule, so the set of
        // records to remove is still resolved in Rust. Edge rows themselves are
        // no longer deleted by hand: the `_relations` foreign keys cascade in
        // SQLite, so removing the records below removes their edges atomically.
        let mut to_delete = vec![(collection_id, record_id)];
        let mut visited = std::collections::HashSet::new();

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

        while let Some((col_id, rec_id)) = to_delete.pop() {
            if visited.contains(&(col_id, rec_id)) {
                continue;
            }
            visited.insert((col_id, rec_id));

            for (other_col_id, other_schema) in &schema_map {
                for rel_def in other_schema.relations.values() {
                    if !rel_def.cascade_on_target_delete {
                        continue;
                    }
                    let targets_us = name_to_id
                        .get(&rel_def.target_collection)
                        .map(|id| *id == col_id)
                        .unwrap_or(false);
                    if !targets_us {
                        continue;
                    }

                    // Origins in the declaring collection that point at the
                    // record being removed, matched on the id pair alone.
                    let origin_ids: Vec<i64> = {
                        let conn = self.get_data_read().await;
                        let mut stmt = conn.prepare(
                            "SELECT r.id FROM _relations rel \
                             JOIN records r ON r.id = rel.origin_rec_id \
                             WHERE rel.target_rec_id = ?1 AND r.collection_id = ?2",
                        )?;
                        let mut rows = stmt.query(rusqlite::params![rec_id, other_col_id])?;
                        let mut ids = Vec::new();
                        while let Some(row) = rows.next()? {
                            ids.push(row.get(0)?);
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

        for (del_col_id, del_rec_id) in visited {
            let f1 = self.data_batcher.execute(
                "DELETE FROM records WHERE collection_id = ?1 AND id = ?2".into(),
                vec![del_col_id.into_val(), del_rec_id.into_val()],
            );
            let f4 = self.vector_batcher.execute(
                "DELETE FROM vectors WHERE collection_id = ?1 AND record_id = ?2".into(),
                vec![del_col_id.into_val(), del_rec_id.into_val()],
            );

            let _ = f1.await.map_err(map_err)?;
            let _ = f4.await.map_err(map_err)?;

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

        // Uniqueness is enforced by the expression indexes created when the
        // collection schema was written, so this INSERT is atomic on conflict.
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
                .await
                .map_err(|e| write_error(&e, collection_id, &schema))?;
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
                .await
                .map_err(|e| write_error(&e, collection_id, &schema))?
        };

        let relation_future =
            self.sync_relations(collection_id, actual_record_id, &final_data, &schema);

        if let Err(e) = relation_future.await {
            // COMPENSATING ROLLBACK: Prevent leaked records if relation sync failed
            let _ = self
                .data_batcher
                .execute(
                    "DELETE FROM records WHERE collection_id = ?1 AND id = ?2".into(),
                    vec![collection_id.into_val(), actual_record_id.into_val()],
                )
                .await;
            // Edges cascade from the records delete above.

            // Format the raw SQLite constraint error for the user
            let err_msg = e.to_string();
            if err_msg.contains("UNIQUE constraint failed") {
                return Err(write_error(&err_msg, collection_id, &schema));
            }
            return Err(e);
        }

        Ok(actual_record_id)
    }

    pub(crate) async fn sync_relations(
        &self,
        _collection_id: i64,
        record_id: i64,
        data: &Value,
        schema: &CollectionSchema,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err =
            |e: String| -> Box<dyn std::error::Error + Send + Sync> { Box::new(std::io::Error::other(e)) };

        // An edge is keyed only by the id pair, so edges cannot be attributed to
        // an individual relation field. Fields are therefore grouped by target
        // collection: the union of every present field's value becomes that
        // collection's edge set for this record. Edges to other collections are
        // left alone (e.g. those created directly through the relations API).
        let mut by_target: std::collections::HashMap<i64, Vec<i64>> =
            std::collections::HashMap::new();

        for (rel_name, rel_def) in &schema.relations {
            let Some(val) = data.get(rel_name) else {
                continue;
            };

            let identifier = &rel_def.target_collection;
            let target_col_id: Option<i64> = {
                let conn = self.get_data_read().await;
                let mut name_stmt = conn.prepare("SELECT id FROM collections WHERE name = ?1")?;
                let mut name_rows = name_stmt.query(rusqlite::params![identifier.clone()])?;
                if let Some(r) = name_rows.next()? {
                    Some(r.get(0)?)
                } else if let Ok(id_num) = identifier.parse::<i64>() {
                    let mut id_stmt = conn.prepare("SELECT id FROM collections WHERE id = ?1")?;
                    let mut id_rows = id_stmt.query(rusqlite::params![id_num])?;
                    if id_rows.next()?.is_some() {
                        Some(id_num)
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            let Some(tc_id) = target_col_id else {
                continue;
            };

            let bucket = by_target.entry(tc_id).or_default();
            let mut push = |v: &Value| {
                if let Some(tid) = v
                    .as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
                    && tid != 0
                {
                    bucket.push(tid);
                }
            };
            match val {
                Value::Array(arr) => arr.iter().for_each(&mut push),
                other => push(other),
            }
        }

        for (tc_id, mut target_ids) in by_target {
            target_ids.sort_unstable();
            target_ids.dedup();

            // Drop this origin's existing edges into the target collection, then
            // re-add the union computed above.
            self.data_batcher
                .execute(
                    "DELETE FROM _relations WHERE origin_rec_id = ?1 AND target_rec_id IN (SELECT id FROM records WHERE collection_id = ?2)".into(),
                    vec![record_id.into_val(), tc_id.into_val()],
                )
                .await
                .map_err(map_err)?;

            for target_rec_id in target_ids {
                // The EXISTS guard keeps a dangling id in the payload from
                // violating the foreign key; a missing target is simply skipped.
                self.data_batcher
                    .execute(
                        "INSERT OR IGNORE INTO _relations (origin_rec_id,target_rec_id) \
                         SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM records WHERE id = ?2)".into(),
                        vec![record_id.into_val(), target_rec_id.into_val()],
                    )
                    .await
                    .map_err(map_err)?;
            }
        }
        Ok(())
    }
}
