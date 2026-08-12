use super::ApexKit;
use crate::database::traits::{IntoSqlVal, ScriptStore};
use crate::models::script::{CreateScriptReq, Script};
use async_trait::async_trait;
use rusqlite::params;

#[async_trait]
impl ScriptStore for ApexKit {
    async fn list_scripts(
        &self,
    ) -> std::result::Result<Vec<Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        // [UPDATED] Select metadata
        let mut stmt = conn.prepare(
            "SELECT id,name,trigger_type,code,active,target_collection,visibility,metadata FROM _scripts",
        )?;
        let mut r = stmt.query([])?;
        let mut v = Vec::new();
        while let Some(row) = r.next()? {
            let meta_str: Option<String> = row.get(7)?;
            v.push(Script {
                id: row.get(0)?,
                name: row.get(1)?,
                trigger_type: row.get(2)?,
                code: row.get(3)?,
                active: row.get(4)?,
                target_collection: row.get(5)?,
                visibility: row.get(6).unwrap_or("private".to_string()),
                metadata: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
            });
        }
        Ok(v)
    }

    async fn create_script(
        &self,
        req: CreateScriptReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let meta_val = req
            .metadata
            .map(|m| serde_json::to_string(&m).unwrap_or_default());

        let id = self.sys_batcher.insert(
            "INSERT INTO _scripts (name,trigger_type,code,target_collection,visibility,active,metadata) VALUES (?1,?2,?3,?4,?5,?6,?7) \
             ON CONFLICT(name) DO UPDATE SET trigger_type=excluded.trigger_type,code=excluded.code,target_collection=excluded.target_collection,\
             visibility=excluded.visibility,active=excluded.active,metadata=excluded.metadata,created_at=CURRENT_TIMESTAMP".into(),
            vec![
                req.name.into_val(),
                req.trigger_type.into_val(),
                req.code.into_val(),
                req.target_collection.into_val(),
                req.visibility.into_val(),
                req.active.into_val(),
                meta_val.into_val()
            ]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(id)
    }

    async fn delete_script(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher
            .execute(
                "DELETE FROM _scripts WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn get_script_by_name(
        &self,
        name: &str,
    ) -> std::result::Result<Option<Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        // [UPDATED] Select metadata
        let mut stmt = conn.prepare(
            "SELECT id,name,trigger_type,code,active,target_collection,visibility,metadata FROM _scripts WHERE name = ?1"
        )?;
        let mut r = stmt.query(params![name])?;
        if let Some(row) = r.next()? {
            let meta_str: Option<String> = row.get(7)?;
            Ok(Some(Script {
                id: row.get(0)?,
                name: row.get(1)?,
                trigger_type: row.get(2)?,
                code: row.get(3)?,
                active: row.get(4)?,
                target_collection: row.get(5)?,
                visibility: row.get(6).unwrap_or("private".to_string()),
                metadata: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_scripts_by_trigger(
        &self,
        trigger: &str,
    ) -> std::result::Result<Vec<Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,name,trigger_type,code,active,target_collection,visibility,metadata \
             FROM _scripts \
             WHERE trigger_type = ?1 AND active = 1",
        )?;
        let mut r = stmt.query(params![trigger])?;
        let mut v = Vec::new();
        while let Some(row) = r.next()? {
            let meta_str: Option<String> = row.get(7)?;
            v.push(Script {
                id: row.get(0)?,
                name: row.get(1)?,
                trigger_type: row.get(2)?,
                code: row.get(3)?,
                active: row.get(4)?,
                target_collection: row.get(5)?,
                visibility: row.get(6).unwrap_or("private".to_string()),
                metadata: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
            });
        }
        Ok(v)
    }
}
