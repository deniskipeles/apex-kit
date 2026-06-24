use super::ApexKit;
use crate::database::traits::{AiActionStore, AiSessionStore, IntoSqlVal};
use crate::models::AiSession;
use crate::models::ai::{AiAction, CreateActionReq};
use async_trait::async_trait;
use rusqlite::params;
use std::error::Error as StdError;

#[async_trait]
impl AiActionStore for ApexKit {
    async fn list_ai_actions(
        &self,
    ) -> std::result::Result<Vec<AiAction>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt = conn
            .prepare("SELECT id,slug,name,model,system_prompt,template,config FROM _ai_actions")?;
        let mut rows = stmt.query([])?;
        let mut res = Vec::new();
        while let Some(row) = rows.next()? {
            let conf_str: String = row.get(6).unwrap_or("{}".to_string());
            res.push(AiAction {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                model: row.get(3)?,
                system_prompt: row.get(4)?,
                template: row.get(5)?,
                config: serde_json::from_str(&conf_str).unwrap_or_default(),
            });
        }
        Ok(res)
    }

    async fn get_ai_action(
        &self,
        slug: &str,
    ) -> std::result::Result<Option<AiAction>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare("SELECT id,slug,name,model,system_prompt,template,config FROM _ai_actions WHERE slug = ?1")?;
        let mut rows = stmt.query(params![slug])?;
        if let Some(row) = rows.next()? {
            let conf_str: String = row.get(6).unwrap_or("{}".to_string());
            Ok(Some(AiAction {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                model: row.get(3)?,
                system_prompt: row.get(4)?,
                template: row.get(5)?,
                config: serde_json::from_str(&conf_str).unwrap_or_default(),
            }))
        } else {
            Ok(None)
        }
    }

    async fn create_ai_action(
        &self,
        action: CreateActionReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let config_str = serde_json::to_string(&action.config.unwrap_or(serde_json::json!({})))?;
        let id = self.sys_batcher.insert(
            "INSERT INTO _ai_actions (slug,name,model,system_prompt,template,config) VALUES (?1,?2,?3,?4,?5,?6)".into(),
            vec![
                action.slug.into_val(),
                action.name.into_val(),
                action.model.into_val(),
                action.system_prompt.into_val(),
                action.template.into_val(),
                config_str.into_val()
            ]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(id)
    }

    async fn delete_ai_action(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.sys_batcher
            .execute(
                "DELETE FROM _ai_actions WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
}

#[async_trait]
impl AiSessionStore for ApexKit {
    async fn create_ai_session(
        &self,
        s: &AiSession,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher.execute(
            "INSERT INTO _ai_sessions (id,name,messages,current_manifest,pending_manifest,diff_summary,last_error,created_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)".into(),
            vec![
                s.id.clone().into_val(),
                s.name.clone().into_val(),
                serde_json::to_string(&s.messages)?.into_val(),
                serde_json::to_string(&s.current_manifest)?.into_val(),
                serde_json::to_string(&s.pending_manifest)?.into_val(),
                s.diff_summary.clone().into_val(),
                s.last_error.clone().into_val(),
                s.created_at.clone().into_val()
            ]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn get_ai_session(
        &self,
        id: &str,
    ) -> std::result::Result<Option<AiSession>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,name,messages,current_manifest,pending_manifest,diff_summary,last_error,created_at FROM _ai_sessions WHERE id = ?1"
        )?;
        let mut r = stmt.query(params![id])?;

        if let Some(row) = r.next()? {
            let m_str: String = row.get(2)?;
            let man_str: Option<String> = row.get(3)?;
            let pend_str: Option<String> = row.get(4).unwrap_or(None);

            Ok(Some(AiSession {
                id: row.get(0)?,
                name: row.get(1)?,
                messages: serde_json::from_str(&m_str)?,
                current_manifest: match man_str {
                    Some(s) => serde_json::from_str(&s).ok(),
                    None => None,
                },
                pending_manifest: match pend_str {
                    Some(s) => serde_json::from_str(&s).ok(),
                    None => None,
                },
                diff_summary: row.get(5).unwrap_or(None),
                last_error: row.get(6).unwrap_or(None),
                created_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn update_ai_session(
        &self,
        s: &AiSession,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher.execute(
            "UPDATE _ai_sessions SET messages = ?1,current_manifest = ?2,pending_manifest = ?3,diff_summary = ?4,last_error = ?5 WHERE id = ?6".into(),
            vec![
                serde_json::to_string(&s.messages)?.into_val(),
                serde_json::to_string(&s.current_manifest)?.into_val(),
                serde_json::to_string(&s.pending_manifest)?.into_val(),
                s.diff_summary.clone().into_val(),
                s.last_error.clone().into_val(),
                s.id.clone().into_val()
            ]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn list_ai_sessions(
        &self,
    ) -> std::result::Result<Vec<AiSession>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,name,messages,current_manifest,pending_manifest,diff_summary,last_error,created_at \
             FROM _ai_sessions ORDER BY created_at DESC"
        )?;
        let mut r = stmt.query([])?;
        let mut s = Vec::new();
        while let Some(row) = r.next()? {
            let m_str: String = row.get(2)?;
            let man_str: Option<String> = row.get(3)?;
            let pend_str: Option<String> = row.get(4).unwrap_or(None);

            s.push(AiSession {
                id: row.get(0)?,
                name: row.get(1)?,
                messages: serde_json::from_str(&m_str).unwrap_or_default(),
                current_manifest: match man_str {
                    Some(str) => serde_json::from_str(&str).ok(),
                    None => None,
                },
                pending_manifest: match pend_str {
                    Some(str) => serde_json::from_str(&str).ok(),
                    None => None,
                },
                diff_summary: row.get(5).unwrap_or(None),
                last_error: row.get(6).unwrap_or(None),
                created_at: row.get(7)?,
            });
        }
        Ok(s)
    }

    async fn delete_ai_session(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.sys_batcher
            .execute(
                "DELETE FROM _ai_sessions WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
}
