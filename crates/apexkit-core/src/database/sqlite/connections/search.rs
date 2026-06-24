use super::ApexKit;
use crate::database::sqlite::utils::row_to_record;
use crate::database::traits::SearchStore;
use crate::models::schema::CollectionSchema;
use crate::models::{InstantResult, Record};
use async_trait::async_trait;
use rusqlite::params;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[async_trait]
impl SearchStore for ApexKit {
    async fn search_records(
        &self,
        collection_id: i64,
        query: &str,
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_search_index(collection_id).await?;

        let ids = self.search.search(collection_id, query, 50)?;
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let id_list = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id,json(data),NULL,created,updated FROM records WHERE id IN ({})",
            id_list
        );
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;

        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(row_to_record(row)?);
        }

        let id_pos: HashMap<i64, usize> =
            ids.iter().enumerate().map(|(pos, id)| (*id, pos)).collect();

        records.sort_by(|a, b| {
            let pos_a = id_pos.get(&a.id).unwrap_or(&usize::MAX);
            let pos_b = id_pos.get(&b.id).unwrap_or(&usize::MAX);
            pos_a.cmp(pos_b)
        });

        Ok(records)
    }

    async fn reindex_collection(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        use crate::database::traits::CollectionStore;
        let col = self
            .get_collection(id)
            .await?
            .ok_or_else(|| format!("Collection {} not found", id))?;
        let schema = col.schema.unwrap_or_default();

        if !schema.fields.values().any(|f| f.ose_indexed) {
            return Ok(());
        }

        self.search
            .delete_index(id)
            .map_err(|e| format!("Search Delete Error: {}", e))?;
        self.search
            .load_index(id, &schema)
            .map_err(|e| format!("Search Load Error: {}", e))?;

        let conn = self.get_data_read().await;
        let mut stmt = conn
            .prepare(
                "SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ?1",
            )
            .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;
        let mut rows = stmt
            .query(params![id])
            .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;

        let mut buffer: Vec<(i64, serde_json::Value)> = Vec::with_capacity(1000);
        let batch_size = 1000;

        while let Some(row) = rows
            .next()
            .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?
        {
            let record = row_to_record(row)?;
            buffer.push((record.id, record.data));

            if buffer.len() >= batch_size {
                self.search
                    .index_batch(id, &buffer, &schema)
                    .map_err(|e| format!("Indexing Error: {}", e))?;
                buffer.clear();
            }
        }

        if !buffer.is_empty() {
            self.search
                .index_batch(id, &buffer, &schema)
                .map_err(|e| format!("Indexing Error: {}", e))?;
        }

        Ok(())
    }

    async fn instant_search(
        &self,
        collection_id: i64,
        query: &str,
        limit: usize,
    ) -> std::result::Result<Vec<InstantResult>, Box<dyn std::error::Error + Send + Sync>> {
        use crate::database::traits::CollectionStore;
        if let Some(col) = self.get_collection(collection_id).await? {
            if let Some(schema) = &col.schema {
                if schema.fields.values().any(|f| f.ose_indexed) {
                    self.search.load_index(collection_id, schema)?;
                } else {
                    return Ok(vec![]);
                }
            }
        }
        let results = self.search.instant_search(collection_id, query, limit)?;
        Ok(results)
    }

    async fn recover_indexes(
        &self,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::database::traits::CollectionStore;
        let collections = self.list_collections().await?;
        let available_cores = std::thread::available_parallelism()?.get();
        let max_concurrency = (available_cores / 2).max(1);

        println!(
            "[Recovery] Starting Index Recovery. Utilizing {}/{} cores.",
            max_concurrency, available_cores
        );

        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrency));
        let recovered_count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        for col in collections {
            let db = self.clone();
            let sem = semaphore.clone();
            let counter = recovered_count.clone();

            let handle = tokio::spawn(async move {
                let task_logic = async {
                    let _permit = sem.acquire().await.ok()?;

                    let db_count_res = {
                        let conn = db.get_data_read().await;
                        let mut stmt = conn
                            .prepare("SELECT COUNT(*) FROM records WHERE collection_id = ?")
                            .ok()?;
                        let mut rows = stmt.query(params![col.id]).ok()?;
                        if let Some(row) = rows.next().ok()? {
                            Some(row.get::<usize, i64>(0).unwrap_or(0) as u64)
                        } else {
                            Some(0)
                        }
                    };

                    let db_count = db_count_res.unwrap_or(0);
                    if db_count == 0 {
                        return Some(());
                    }

                    if let Some(schema) = &col.schema {
                        if !schema.fields.values().any(|f| f.ose_indexed) {
                            return Some(());
                        }
                        let _ = db.search.load_index(col.id, schema);
                    }

                    let idx_count = db.search.get_doc_count(col.id).unwrap_or(0);

                    if db_count != idx_count {
                        println!(
                            "[Recovery] Collection '{}' (ID: {}) mismatch. DB: {}, Index: {}. Re-indexing...",
                            col.name, col.id, db_count, idx_count
                        );
                        if let Err(e) = db.reindex_collection(col.id).await {
                            eprintln!("[Recovery] Failed to re-index collection {}: {}", col.id, e);
                        } else {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Some(())
                };

                task_logic.await;
            });
            handles.push(handle);
        }

        for h in handles {
            let _ = h.await;
        }

        let count = recovered_count.load(Ordering::Relaxed);
        if count > 0 {
            println!("[Recovery] Successfully recovered {} collections.", count);
        } else {
            println!("[Recovery] Indexes healthy.");
        }

        Ok(())
    }

    async fn index_record_search(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
        schema: &CollectionSchema,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.search
            .index_record(collection_id, record_id, data, schema)
            .map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn StdError + Send + Sync>)
    }

    async fn delete_record_search(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.search
            .delete_record(collection_id, record_id)
            .map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn StdError + Send + Sync>)
    }
}
