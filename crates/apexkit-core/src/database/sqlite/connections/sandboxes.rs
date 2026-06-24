use super::ApexKit;
use crate::database::sqlite::utils::calculate_dir_size;
use crate::database::traits::{IntoSqlVal, SandboxStore};
use crate::models::SandboxMetadata;
use async_trait::async_trait;
use std::error::Error as StdError;

#[async_trait]
impl SandboxStore for ApexKit {
    async fn register_sandbox(
        &self,
        id: &str,
        owner_id: Option<i64>,
        name: Option<String>,
        expires_at: Option<String>,
        scope: &str,
        tenant_id: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "INSERT INTO _sandboxes (id,owner_id,name,expires_at,scope,tenant_id) VALUES (?1,?2,?3,?4,?5,?6) \
                 ON CONFLICT(id) DO UPDATE SET \
                    owner_id=excluded.owner_id, \
                    name=excluded.name, \
                    expires_at=excluded.expires_at, \
                    scope=excluded.scope, \
                    tenant_id=excluded.tenant_id"
                    .into(),
                vec![
                    id.into_val(),
                    owner_id.into_val(),
                    name.into_val(),
                    expires_at.into_val(),
                    scope.into_val(),
                    tenant_id.into_val(),
                ],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn list_sandboxes(
        &self,
        tenant_id: Option<String>,
    ) -> std::result::Result<Vec<SandboxMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut sql = "SELECT id,name,status,expires_at,scope,tenant_id,current_storage_mb,max_storage_mb FROM _sandboxes".to_string();
        let mut params: Vec<rusqlite::types::Value> = vec![];

        if let Some(tid) = tenant_id {
            sql.push_str(" WHERE tenant_id = ?1 ORDER BY created_at DESC");
            params.push(tid.into_val());
        } else {
            sql.push_str(" WHERE tenant_id IS NULL ORDER BY created_at DESC");
        }

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
        let mut res = vec![];

        while let Some(row) = rows.next()? {
            res.push(SandboxMetadata {
                id: row.get(0)?,
                name: row.get(1)?,
                status: row.get(2)?,
                expires_at: row.get(3)?,
                scope: row.get(4).unwrap_or("root".to_string()),
                tenant_id: row.get(5).unwrap_or(None),
                current_storage_mb: row.get(6).unwrap_or(0.0),
                max_storage_mb: row.get(7).unwrap_or(100),
            });
        }
        Ok(res)
    }

    async fn update_sandbox_full(
        &self,
        id: &str,
        name: Option<String>,
        status: Option<String>,
        expires_at: Option<String>,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let mut sets = Vec::new();
        let mut params: Vec<rusqlite::types::Value> = vec![];

        if let Some(n) = name {
            sets.push("name = ?");
            params.push(n.into_val());
        }
        if let Some(s) = status {
            sets.push("status = ?");
            params.push(s.into_val());
        }
        if let Some(e) = expires_at {
            sets.push("expires_at = ?");
            params.push(e.into_val());
        }

        if sets.is_empty() {
            return Ok(());
        }

        params.push(id.to_string().into_val());
        let sql = format!("UPDATE _sandboxes SET {} WHERE id = ?", sets.join(","));
        self.core_batcher
            .execute(sql, params)
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn delete_sandbox_metadata(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.core_batcher
            .execute(
                "DELETE FROM _sandboxes WHERE id = ?1".into(),
                vec![id.to_string().into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn get_sandbox_disk_usage(
        &self,
        sandbox_id: &str,
    ) -> std::result::Result<u64, Box<dyn StdError + Send + Sync>> {
        let path = format!("storage/sandboxes/session_{}", sandbox_id);

        let size =
            tokio::task::spawn_blocking(move || calculate_dir_size(std::path::Path::new(&path)))
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn StdError + Send + Sync>
                })??;

        Ok(size)
    }
}
