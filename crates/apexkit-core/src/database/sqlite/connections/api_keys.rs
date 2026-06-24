use super::ApexKit;
use crate::database::traits::{ApiKeyStore, IntoSqlVal};
use crate::models::ApiKey;
use async_trait::async_trait;
use rusqlite::params;
use std::error::Error as StdError;

#[async_trait]
impl ApiKeyStore for ApexKit {
    async fn create_api_key(
        &self,
        name: &str,
        tenant_id: &str,
        issuer: &str,
        env_type: &str,
        roles: Vec<String>,
        bypass_cors: bool,
    ) -> std::result::Result<(String, ApiKey), Box<dyn std::error::Error + Send + Sync>> {
        let key_env = match env_type {
            "sys" => crate::security::api_keys::KeyEnv::Sys,
            "tnnt" => crate::security::api_keys::KeyEnv::Tnnt,
            "sk" => crate::security::api_keys::KeyEnv::Sk,
            "pk" => crate::security::api_keys::KeyEnv::Pk,
            _ => crate::security::api_keys::KeyEnv::Sys,
        };

        let (raw_key, secret_hash, key_id) =
            crate::security::api_keys::generate_api_key(tenant_id, key_env);
        let roles_json = serde_json::to_string(&roles)?;

        let id = self.core_batcher.insert(
            "INSERT INTO _api_keys_v2 (name,tenant_id,key_id,secret_hash,issuer,env_type,roles,status,bypass_cors) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,'active',?8)".into(),
            vec![
                name.into_val(),
                tenant_id.into_val(),
                key_id.clone().into_val(),
                secret_hash.into_val(),
                issuer.into_val(),
                env_type.into_val(),
                roles_json.into_val(),
                bypass_cors.into_val()
            ]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)?;

        let key_obj = ApiKey {
            id,
            name: name.to_string(),
            tenant_id: tenant_id.to_string(),
            key_id,
            issuer: issuer.to_string(),
            env_type: env_type.to_string(),
            roles,
            status: "active".to_string(),
            bypass_cors,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        Ok((raw_key, key_obj))
    }

    async fn update_api_key(
        &self,
        id: i64,
        name: Option<String>,
        status: Option<String>,
        roles: Option<Vec<String>>,
        bypass_cors: Option<bool>,
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
        if let Some(r) = roles {
            let r_str = serde_json::to_string(&r)?;
            sets.push("roles = ?");
            params.push(r_str.into_val());
        }
        if let Some(b) = bypass_cors {
            sets.push("bypass_cors = ?");
            params.push(b.into_val());
        }

        if sets.is_empty() {
            return Ok(());
        }
        params.push(id.into_val());

        let sql = format!("UPDATE _api_keys_v2 SET {} WHERE id = ?", sets.join(","));
        self.core_batcher.execute(sql, params).await.map_err(|e| {
            Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
        })?;

        Ok(())
    }

    async fn list_api_keys(
        &self,
    ) -> std::result::Result<Vec<ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,name,tenant_id,key_id,issuer,env_type,roles,status,bypass_cors,created_at \
             FROM _api_keys_v2 \
             ORDER BY created_at DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut keys = Vec::new();
        while let Some(row) = rows.next()? {
            let roles_str: String = row.get(6)?;
            keys.push(ApiKey {
                id: row.get(0)?,
                name: row.get(1)?,
                tenant_id: row.get(2)?,
                key_id: row.get(3)?,
                issuer: row.get(4)?,
                env_type: row.get(5)?,
                roles: serde_json::from_str(&roles_str).unwrap_or_default(),
                status: row.get(7)?,
                bypass_cors: row.get(8).unwrap_or(false),
                created_at: row.get(9)?,
            });
        }
        Ok(keys)
    }

    async fn delete_api_key(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "DELETE FROM _api_keys_v2 WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;
        Ok(())
    }

    async fn verify_api_key(
        &self,
        tenant_id: &str,
        key_id: &str,
        secret: &str,
    ) -> std::result::Result<Option<ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
        let expected_hash = crate::utils::sha256(secret);
        let conn = self.get_core_read().await;

        let mut stmt = conn.prepare(
            "SELECT id,name,tenant_id,key_id,issuer,env_type,roles,status,bypass_cors,created_at \
             FROM _api_keys_v2 WHERE tenant_id = ?1 AND key_id = ?2 AND secret_hash = ?3",
        )?;
        let mut rows = stmt.query(params![tenant_id, key_id, expected_hash])?;

        if let Some(row) = rows.next()? {
            let status: String = row.get(7)?;
            if status != "active" {
                return Ok(None);
            }

            let roles_str: String = row.get(6)?;

            return Ok(Some(ApiKey {
                id: row.get(0)?,
                name: row.get(1)?,
                tenant_id: row.get(2)?,
                key_id: row.get(3)?,
                issuer: row.get(4)?,
                env_type: row.get(5)?,
                roles: serde_json::from_str(&roles_str).unwrap_or_default(),
                status,
                bypass_cors: row.get(8).unwrap_or(false),
                created_at: row.get(9)?,
            }));
        }
        Ok(None)
    }
}
