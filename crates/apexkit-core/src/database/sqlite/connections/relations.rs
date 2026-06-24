use super::ApexKit;
use crate::database::sqlite::utils::row_to_record;
use crate::database::traits::{IntoSqlVal, RelationStore};
use crate::models::Record;
use async_trait::async_trait;
use rusqlite::params;
use std::error::Error as StdError;

#[async_trait]
impl RelationStore for ApexKit {
    async fn create_relation(
        &self,
        origin_col: i64,
        origin_id: i64,
        target_col: i64,
        target_id: i64,
        rel_name: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.data_batcher
            .execute(
                "INSERT INTO _relations (origin_col_id,origin_rec_id,target_col_id,target_rec_id,rel_name) VALUES (?1,?2,?3,?4,?5)".into(),
                vec![
                    origin_col.into_val(),
                    origin_id.into_val(),
                    target_col.into_val(),
                    target_id.into_val(),
                    rel_name.into_val()
                ]
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }

    async fn delete_relation(
        &self,
        origin_col: i64,
        origin_id: i64,
        target_col: i64,
        target_id: i64,
        rel_name: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };
        self.data_batcher
            .execute(
                "DELETE FROM _relations \
                 WHERE origin_col_id=?1 AND origin_rec_id=?2 AND target_col_id=?3 AND target_rec_id=?4 AND rel_name=?5".into(),
                vec![
                    origin_col.into_val(),
                    origin_id.into_val(),
                    target_col.into_val(),
                    target_id.into_val(),
                    rel_name.into_val()
                ]
            )
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn get_related_ids(
        &self,
        origin_col: i64,
        origin_id: i64,
        rel_name: &str,
    ) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(
            "SELECT target_col_id,target_rec_id FROM _relations \
             WHERE origin_col_id=?1 AND origin_rec_id=?2 AND rel_name=?3",
        )?;
        let mut rows = stmt.query(params![origin_col, origin_id, rel_name])?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            results.push((row.get(0)?, row.get(1)?));
        }
        Ok(results)
    }

    async fn get_records_by_ids(
        &self,
        collection_id: i64,
        ids: &[i64],
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let id_list = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ? AND id IN ({})",
            id_list
        );
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params![collection_id])?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(row_to_record(row)?);
        }
        Ok(records)
    }
}
