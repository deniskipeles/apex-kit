use super::ApexKit;
use crate::database::sqlite::utils::calculate_dir_size;
use crate::database::traits::{IntoSqlVal, TenantStore};
use crate::models;
use async_trait::async_trait;
use rusqlite::params;
use std::error::Error as StdError;

#[async_trait]
impl TenantStore for ApexKit {
    async fn register_tenant(
        &self,
        id: &str,
        owner_id: Option<i64>,
        name: Option<String>,
        tier: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "INSERT INTO _tenants (id,owner_id,name,tier) VALUES (?1,?2,?3,?4) \
                 ON CONFLICT(id) DO UPDATE SET \
                    owner_id=excluded.owner_id, \
                    name=excluded.name, \
                    tier=excluded.tier, \
                    updated_at=CURRENT_TIMESTAMP"
                    .into(),
                vec![
                    id.into_val(),
                    owner_id.into_val(),
                    name.into_val(),
                    tier.unwrap_or("free".to_string()).into_val(),
                ],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn get_tenant_status(
        &self,
        tenant_id: &str,
    ) -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare("SELECT status FROM _tenants WHERE id = ?1")?;
        let mut rows = stmt.query(params![tenant_id])?;

        if let Some(row) = rows.next()? {
            let status: String = row.get(0)?;
            Ok(status)
        } else {
            Ok("not_found".to_string())
        }
    }

    async fn update_tenant_status(
        &self,
        tenant_id: &str,
        status: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "UPDATE _tenants SET status = ?1,updated_at = CURRENT_TIMESTAMP WHERE id = ?2"
                    .into(),
                vec![status.into_val(), tenant_id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn update_tenant_full(
        &self,
        id: &str,
        name: Option<String>,
        status: Option<String>,
        tier: Option<String>,
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
        if let Some(t) = tier {
            sets.push("tier = ?");
            params.push(t.into_val());
        }

        if sets.is_empty() {
            return Ok(());
        }

        sets.push("updated_at = CURRENT_TIMESTAMP");
        params.push(id.to_string().into_val());

        let sql = format!("UPDATE _tenants SET {} WHERE id = ?", sets.join(","));
        self.core_batcher
            .execute(sql, params)
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn delete_tenant_metadata(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.core_batcher
            .execute(
                "DELETE FROM _tenants WHERE id = ?1".into(),
                vec![id.to_string().into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn list_tenants(
        &self,
    ) -> std::result::Result<Vec<models::Tenant>, Box<dyn StdError + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,name,status,tier,max_storage_mb,current_storage_mb,max_vectors,current_vectors,max_ai_requests,current_ai_requests,created_at \
             FROM _tenants \
             ORDER BY created_at DESC"
        )?;
        let mut rows = stmt.query([])?;

        let mut tenants = Vec::new();
        while let Some(row) = rows.next()? {
            tenants.push(models::Tenant {
                id: row.get(0)?,
                name: row.get(1).unwrap_or(None),
                status: row.get(2)?,
                tier: row.get(3)?,
                stats: models::TenantStats {
                    storage_mb: row.get(5)?,
                    max_storage_mb: row.get(4)?,
                    vector_count: row.get(7)?,
                    max_vectors: row.get(6)?,
                    ai_requests: row.get(9)?,
                    max_ai_requests: row.get(8)?,
                },
                created_at: row.get(10)?,
            });
        }
        Ok(tenants)
    }

    async fn get_tenant_disk_usage(
        &self,
        tenant_id: &str,
    ) -> std::result::Result<u64, Box<dyn StdError + Send + Sync>> {
        let path = format!("storage/tenants/{}", tenant_id);

        let size =
            tokio::task::spawn_blocking(move || calculate_dir_size(std::path::Path::new(&path)))
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn StdError + Send + Sync>
                })??;

        Ok(size)
    }
}
