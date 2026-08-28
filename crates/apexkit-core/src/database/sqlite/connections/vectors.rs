use super::ApexKit;
use crate::database::traits::{IntoSqlVal, RelationStore, VectorStore};
use crate::models::{Record, VectorRecord};
use async_trait::async_trait;
use rusqlite::params;
use std::error::Error as StdError;

#[async_trait]
impl VectorStore for ApexKit {
    async fn save_vector(
        &self,
        collection_id: i64,
        record_id: i64,
        field_name: &str,
        vector: Vec<f32>,
        model: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let vec_json = serde_json::to_string(&vector)?;
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };

        self.vector_batcher
            .insert(
                "INSERT INTO vectors (collection_id,record_id,field_name,vector,model) \
                 VALUES (?1,?2,?3,?4,?5) \
                 ON CONFLICT(collection_id,record_id,field_name,model) \
                 DO UPDATE SET vector = excluded.vector,created_at = CURRENT_TIMESTAMP"
                    .into(),
                vec![
                    collection_id.into_val(),
                    record_id.into_val(),
                    field_name.into_val(),
                    vec_json.into_val(),
                    model.into_val(),
                ],
            )
            .await
            .map_err(map_err)?;

        Ok(())
    }

    async fn has_vector(
        &self,
        collection_id: i64,
        record_id: i64,
        field_name: &str,
        model: &str,
    ) -> std::result::Result<bool, Box<dyn StdError + Send + Sync>> {
        let conn = self.get_vector_read().await;
        let mut stmt = conn.prepare(
            "SELECT 1 FROM vectors WHERE collection_id = ?1 AND record_id = ?2 AND field_name = ?3 AND model = ?4"
        )?;
        let mut rows = stmt.query(params![collection_id, record_id, field_name, model])?;

        Ok(rows.next()?.is_some())
    }

    async fn get_record_vectors(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<Vec<VectorRecord>, Box<dyn StdError + Send + Sync>> {
        let conn = self.get_vector_read().await;
        let mut stmt = conn.prepare(
            "SELECT field_name,vector,model FROM vectors WHERE collection_id = ?1 AND record_id = ?2"
        )?;
        let mut rows = stmt.query(params![collection_id, record_id])?;

        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let field_name: String = row.get(0)?;

            let val = row
                .get_ref(1)
                .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;

            let vector_blob: Vec<u8> = match val {
                rusqlite::types::ValueRef::Blob(b) => b.to_vec(),
                rusqlite::types::ValueRef::Text(s) => s.to_vec(),
                _ => vec![],
            };

            let model: String = row.get(2)?;

            if !vector_blob.is_empty() {
                let vector: Vec<f32> = serde_json::from_slice(&vector_blob).unwrap_or_default();
                results.push(VectorRecord {
                    field_name,
                    vector,
                    model,
                });
            }
        }
        Ok(results)
    }

    async fn get_vectors_for_collection(
        &self,
        collection_id: i64,
        model: &str,
        limit: i64,
        offset: i64,
    ) -> std::result::Result<Vec<(i64, String, Vec<f32>)>, Box<dyn StdError + Send + Sync>> {
        let conn = self.get_vector_read().await;
        // Apply pagination parameters to query
        let mut stmt = conn.prepare(
            "SELECT record_id,field_name,vector FROM vectors WHERE collection_id = ?1 AND model = ?2 LIMIT ?3 OFFSET ?4"
        )?;
        let mut rows = stmt.query(params![collection_id, model.to_string(), limit, offset])?;

        let mut vectors = Vec::new();
        while let Some(row) = rows.next()? {
            let record_id: i64 = row.get(0)?;
            let field_name: String = row.get(1)?;
            let vector_json_str: String = row.get(2)?;

            if let Ok(vector) = serde_json::from_str::<Vec<f32>>(&vector_json_str) {
                vectors.push((record_id, field_name, vector));
            }
        }
        Ok(vectors)
    }

    async fn search_vector(
        &self,
        collection_id: i64,
        field: &str,
        vector: Vec<f32>,
        limit: usize,
    ) -> std::result::Result<Vec<(Record, f32)>, Box<dyn std::error::Error + Send + Sync>> {
        let results = self
            .vector_provider
            .search(collection_id, field, &vector, limit)
            .await
            .map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;

        if results.is_empty() {
            return Ok(vec![]);
        }

        let ids: Vec<i64> = results.iter().map(|(id, _score)| *id).collect();
        let records = self.get_records_by_ids(collection_id, &ids).await?;

        let mut id_to_record: std::collections::HashMap<i64, Record> =
            records.into_iter().map(|r| (r.id, r)).collect();

        let mut final_results = Vec::new();
        for (id, distance_score) in results {
            if let Some(rec) = id_to_record.remove(&id) {
                final_results.push((rec, distance_score));
            }
        }

        Ok(final_results)
    }

    async fn flush_vectors_by_model(
        &self,
        model: &str,
    ) -> std::result::Result<usize, Box<dyn StdError + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };

        // 1. Delete rows
        let affected = self
            .vector_batcher
            .execute(
                "DELETE FROM vectors WHERE model = ?1".into(),
                vec![model.into_val()],
            )
            .await
            .map_err(map_err)?;

        // 2. Reclaim physical disk space and reset page_count
        {
            let conn = self.get_vector_read().await;
            conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;
        }

        Ok(affected as usize)
    }
}
