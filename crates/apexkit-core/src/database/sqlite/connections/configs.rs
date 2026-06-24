use super::ApexKit;
use crate::database::traits::{ConfigStore, IntoSqlVal};
use crate::models::ConfigItem;
use async_trait::async_trait;
use rusqlite::params;

#[async_trait]
impl ConfigStore for ApexKit {
    async fn get_config(
        &self,
        key: &str,
    ) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare("SELECT value FROM _system_config_settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let v_str: String = row.get(0)?;
            Ok(Some(serde_json::from_str(&v_str)?))
        } else {
            Ok(None)
        }
    }

    async fn set_config(
        &self,
        key: &str,
        value: &serde_json::Value,
        encrypted: bool,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let v_str = serde_json::to_string(value)?;
        self.core_batcher.execute(
            "INSERT INTO _system_config_settings (key,value,encrypted,updated_at) VALUES (?1,?2,?3,CURRENT_TIMESTAMP) \
             ON CONFLICT(key) DO UPDATE SET value=excluded.value,encrypted=excluded.encrypted,updated_at=CURRENT_TIMESTAMP".into(),
            vec![key.into_val(), v_str.into_val(), encrypted.into_val()]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn list_configs(
        &self,
    ) -> std::result::Result<Vec<ConfigItem>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare(
            "SELECT key,value,encrypted,updated_at FROM _system_config_settings ORDER BY key ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let key: String = row.get(0)?;
            let val_str: String = row.get(1)?;
            let encrypted: bool = row.get(2)?;
            let updated_at: String = row.get(3)?;

            let value = if encrypted {
                Some("******".to_string())
            } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(&val_str) {
                if v.is_string() {
                    Some(v.as_str().unwrap().to_string())
                } else {
                    Some(val_str)
                }
            } else {
                Some(val_str)
            };

            items.push(ConfigItem {
                key,
                value,
                encrypted,
                updated_at,
            });
        }
        Ok(items)
    }

    async fn delete_config(
        &self,
        key: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "DELETE FROM _system_config_settings WHERE key = ?1".into(),
                vec![key.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
}
