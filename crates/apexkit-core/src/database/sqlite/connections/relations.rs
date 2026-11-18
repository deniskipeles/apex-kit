use super::ApexKit;
use crate::database::sqlite::utils::row_to_record;
use crate::database::traits::{IntoSqlVal, RelationStore};
use crate::models::Record;
use async_trait::async_trait;
use rusqlite::params;
use std::error::Error as StdError;

fn map_err(e: String) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::other(e))
}

#[async_trait]
impl RelationStore for ApexKit {
    async fn create_relation(
        &self,
        _origin_col: i64,
        origin_id: i64,
        _target_col: i64,
        target_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // The foreign keys enforce that both endpoints exist. The EXISTS guard
        // makes a dangling target id a no-op instead of a constraint error, so
        // linking to a missing record does not surface as a server error.
        self.data_batcher
            .execute(
                "INSERT OR IGNORE INTO _relations (origin_rec_id,target_rec_id) \
                 SELECT ?1, ?2 \
                 WHERE EXISTS (SELECT 1 FROM records WHERE id = ?1) \
                   AND EXISTS (SELECT 1 FROM records WHERE id = ?2)"
                    .into(),
                vec![origin_id.into_val(), target_id.into_val()],
            )
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn delete_relation(
        &self,
        _origin_col: i64,
        origin_id: i64,
        _target_col: i64,
        target_id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.data_batcher
            .execute(
                "DELETE FROM _relations WHERE origin_rec_id=?1 AND target_rec_id=?2".into(),
                vec![origin_id.into_val(), target_id.into_val()],
            )
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn get_related_ids(
        &self,
        origin_id: i64,
    ) -> std::result::Result<Vec<i64>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt =
            conn.prepare("SELECT target_rec_id FROM _relations WHERE origin_rec_id=?1")?;
        let mut rows = stmt.query(params![origin_id])?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            results.push(row.get(0)?);
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
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!(
            "SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ? AND id IN ({})",
            placeholders
        );

        let mut values: Vec<rusqlite::types::Value> = Vec::with_capacity(ids.len() + 1);
        values.push(collection_id.into_val());
        values.extend(ids.iter().map(|id| id.into_val()));

        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(values))?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(row_to_record(row)?);
        }
        Ok(records)
    }
}
