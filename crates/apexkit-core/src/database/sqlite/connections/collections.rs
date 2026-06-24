use super::ApexKit;
use crate::database::sqlite::utils::{reconcile_sql_indexes, row_to_collection};
use crate::database::traits::{CollectionStore, IntoSqlVal};
use crate::models::Collection;
use crate::models::schema::CollectionSchema;
use async_trait::async_trait;
use rusqlite::params;

#[async_trait]
impl CollectionStore for ApexKit {
    async fn create_collection(
        &self,
        name: &str,
        schema: &Option<CollectionSchema>,
        index: Option<String>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let schema_str = serde_json::to_string(&schema)?;
        let idx = index.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let id = self
            .data_batcher
            .insert(
                "INSERT INTO collections (name,schema,index_key) VALUES (?1,?2,?3)".into(),
                vec![name.into_val(), schema_str.into_val(), idx.into_val()],
            )
            .await
            .map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;

        if let Some(s) = schema {
            if s.fields.values().any(|f| f.ose_indexed) {
                self.search.load_index(id, s)?;
            }
            reconcile_sql_indexes(&self.data_batcher, id, s, None).await?;
        }
        Ok(id)
    }

    async fn get_collection(
        &self,
        id: i64,
    ) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt =
            conn.prepare("SELECT id,name,schema,index_key FROM collections WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_collection(row)?)),
            None => Ok(None),
        }
    }

    async fn list_collections(
        &self,
    ) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare("SELECT id,name,schema,index_key FROM collections")?;
        let mut rows = stmt.query([])?;
        let mut cols = Vec::new();
        while let Some(row) = rows.next()? {
            cols.push(row_to_collection(row)?);
        }
        Ok(cols)
    }

    async fn update_collection(
        &self,
        id: i64,
        name: Option<String>,
        schema: Option<CollectionSchema>,
    ) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
        let existing = self.get_collection(id).await?.ok_or("Not found")?;
        let old_schema = existing.schema;

        if let Some(n) = name {
            self.data_batcher
                .execute(
                    "UPDATE collections SET name = ?1 WHERE id = ?2".into(),
                    vec![n.into_val(), id.into_val()],
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;
        }

        if let Some(s) = &schema {
            let s_str = serde_json::to_string(&s)?;
            self.data_batcher
                .execute(
                    "UPDATE collections SET schema = ?1 WHERE id = ?2".into(),
                    vec![s_str.into_val(), id.into_val()],
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;

            // --- SQL Migration for Renamed & Deleted Fields ---
            if let Some(old_s) = &old_schema {
                // 1. Handle standard field renames within the JSON blob
                for (new_name, new_def) in &s.fields {
                    if let Some((old_name, _)) = old_s
                        .fields
                        .iter()
                        .find(|(_, old_def)| old_def.uid == new_def.uid)
                        && old_name != new_name
                    {
                        let sql = format!(
                            "UPDATE records SET data = json_remove(json_set(data,'$.{}',json_extract(data,'$.{}')),'$.{}') \
                             WHERE collection_id = ? AND json_type(data,'$.{}') IS NOT NULL",
                            new_name, old_name, old_name, old_name
                        );
                        self.data_batcher
                            .execute(sql, vec![id.into_val()])
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;
                    }
                }

                // 2. Handle relational field renames within the JSON blob and linkages table
                for (new_name, new_def) in &s.relations {
                    if let Some((old_name, _)) = old_s
                        .relations
                        .iter()
                        .find(|(_, old_def)| old_def.uid == new_def.uid)
                        && old_name != new_name
                    {
                        let sql = format!(
                            "UPDATE records SET data = json_remove(json_set(data,'$.{}',json_extract(data,'$.{}')),'$.{}') \
                             WHERE collection_id = ? AND json_type(data,'$.{}') IS NOT NULL",
                            new_name, old_name, old_name, old_name
                        );
                        self.data_batcher
                            .execute(sql, vec![id.into_val()])
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;

                        let rel_sql = "UPDATE _relations SET rel_name = ? WHERE origin_col_id = ? AND rel_name = ?".to_string();
                        self.data_batcher
                            .execute(
                                rel_sql,
                                vec![
                                    new_name.clone().into_val(),
                                    id.into_val(),
                                    old_name.clone().into_val(),
                                ],
                            )
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;
                    }
                }

                // 3. Handle Deleted Standard Fields (Remove from JSON)
                for (old_name, old_def) in &old_s.fields {
                    let still_exists = s.fields.values().any(|new_def| new_def.uid == old_def.uid);
                    if !still_exists {
                        let sql = format!(
                            "UPDATE records SET data = json_remove(data,'$.{}') WHERE collection_id = ? AND json_type(data,'$.{}') IS NOT NULL",
                            old_name, old_name
                        );
                        self.data_batcher
                            .execute(sql, vec![id.into_val()])
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;
                    }
                }

                // 4. Handle Deleted Relations (Remove from JSON and linkage table)
                for (old_name, old_def) in &old_s.relations {
                    let still_exists = s
                        .relations
                        .values()
                        .any(|new_def| new_def.uid == old_def.uid);
                    if !still_exists {
                        let sql = format!(
                            "UPDATE records SET data = json_remove(data,'$.{}') WHERE collection_id = ? AND json_type(data,'$.{}') IS NOT NULL",
                            old_name, old_name
                        );
                        self.data_batcher
                            .execute(sql, vec![id.into_val()])
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;

                        let rel_sql =
                            "DELETE FROM _relations WHERE origin_col_id = ? AND rel_name = ?";
                        self.data_batcher
                            .execute(
                                rel_sql.to_string(),
                                vec![id.into_val(), old_name.clone().into_val()],
                            )
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;
                    }
                }
            }

            reconcile_sql_indexes(&self.data_batcher, id, s, old_schema.as_ref()).await?;

            if s.fields.values().any(|f| f.ose_indexed) {
                self.search.load_index(id, s)?;
            }
        }

        self.get_collection(id)
            .await?
            .ok_or_else(|| "Not found".into())
    }

    async fn delete_collection(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };

        self.data_batcher
            .execute(
                "DELETE FROM records WHERE collection_id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(map_err)?;
        self.data_batcher
            .execute(
                "DELETE FROM _relations WHERE origin_col_id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(map_err)?;
        self.data_batcher
            .execute(
                "DELETE FROM _relations WHERE target_col_id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(map_err)?;
        self.data_batcher
            .execute(
                "DELETE FROM collections WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(map_err)?;
        let _ = self.search.delete_index(id);
        Ok(())
    }
}
