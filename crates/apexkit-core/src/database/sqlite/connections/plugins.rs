use super::ApexKit;
use crate::database::traits::{IntoSqlVal, PluginStore};
use crate::models::Plugin;
use async_trait::async_trait;

#[async_trait]
impl PluginStore for ApexKit {
    async fn save_plugin(
        &self,
        p: &Plugin,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher
            .execute(
                "INSERT INTO _plugins (id,name,version,manifest,description) VALUES (?1,?2,?3,?4,?5)".into(),
                vec![
                    p.id.clone().into_val(),
                    p.name.clone().into_val(),
                    p.version.clone().into_val(),
                    serde_json::to_string(&p.manifest)?.into_val(),
                    p.description.clone().into_val()
                ]
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn list_plugins(
        &self,
    ) -> std::result::Result<Vec<Plugin>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,name,version,manifest,description FROM _plugins ORDER BY created_at DESC",
        )?;
        let mut r = stmt.query([])?;
        let mut p = Vec::new();
        while let Some(row) = r.next()? {
            let m_str: String = row.get(3)?;
            p.push(Plugin {
                id: row.get(0)?,
                name: row.get(1)?,
                version: row.get(2)?,
                manifest: serde_json::from_str(&m_str)?,
                description: row.get(4)?,
            });
        }
        Ok(p)
    }
}
