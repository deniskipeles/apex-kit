use super::ApexKit;
use crate::database::traits::{FileStore, IntoSqlVal};
use crate::models::StoredFile;
use async_trait::async_trait;
use rusqlite::params;
use std::error::Error as StdError;

#[async_trait]
impl FileStore for ApexKit {
    async fn create_file_metadata(
        &self,
        filename: &str,
        original_name: &str,
        mime_type: &str,
        size: i64,
        user_id: Option<i64>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.data_batcher
            .insert(
                "INSERT INTO _storage_files (filename,original_name,mime_type,size,user_id) VALUES (?1,?2,?3,?4,?5)".into(),
                vec![
                    filename.into_val(),
                    original_name.into_val(),
                    mime_type.into_val(),
                    size.into_val(),
                    user_id.into_val(),
                ],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn list_files(
        &self,
        limit: i64,
        offset: i64,
    ) -> std::result::Result<Vec<StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,filename,original_name,mime_type,size,created_at \
             FROM _storage_files \
             ORDER BY created_at DESC \
             LIMIT ?1 OFFSET ?2",
        )?;
        let mut rows = stmt.query(params![limit, offset])?;
        let mut files = Vec::new();
        while let Some(row) = rows.next()? {
            files.push(StoredFile {
                id: row.get(0)?,
                filename: row.get(1)?,
                original_name: row.get(2)?,
                mime_type: row.get(3)?,
                size: row.get(4)?,
                created_at: row.get(5)?,
            });
        }
        Ok(files)
    }

    async fn count_files(
        &self,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM _storage_files")?;
        let mut row = stmt.query([])?;
        if let Some(r) = row.next()? {
            Ok(r.get(0)?)
        } else {
            Ok(0)
        }
    }

    async fn get_file_metadata(
        &self,
        id: i64,
    ) -> std::result::Result<Option<StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,filename,original_name,mime_type,size,created_at FROM _storage_files WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(StoredFile {
                id: row.get(0)?,
                filename: row.get(1)?,
                original_name: row.get(2)?,
                mime_type: row.get(3)?,
                size: row.get(4)?,
                created_at: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_file_by_filename(
        &self,
        filename: &str,
    ) -> std::result::Result<Option<StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,filename,original_name,mime_type,size,created_at FROM _storage_files WHERE filename = ?1"
        )?;
        let mut rows = stmt.query(params![filename])?;
        if let Some(row) = rows.next()? {
            Ok(Some(StoredFile {
                id: row.get(0)?,
                filename: row.get(1)?,
                original_name: row.get(2)?,
                mime_type: row.get(3)?,
                size: row.get(4)?,
                created_at: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn delete_file_metadata(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };
        self.data_batcher
            .execute(
                "DELETE FROM _storage_files WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(map_err)?;
        Ok(())
    }
}
