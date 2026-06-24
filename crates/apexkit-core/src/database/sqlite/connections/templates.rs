use super::ApexKit;
use crate::database::traits::{IntoSqlVal, TemplateStore};
use crate::models::{CreateTemplateReq, Template};
use async_trait::async_trait;
use rusqlite::params;
use std::error::Error as StdError;

#[async_trait]
impl TemplateStore for ApexKit {
    async fn list_templates(
        &self,
    ) -> std::result::Result<Vec<Template>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt =
            conn.prepare("SELECT id,slug,content,script_id,created_at FROM _templates")?;
        let mut r = stmt.query([])?;
        let mut v = Vec::new();
        while let Some(row) = r.next()? {
            v.push(Template {
                id: row.get(0)?,
                slug: row.get(1)?,
                content: row.get(2)?,
                script_id: row.get(3)?,
                created_at: row.get(4)?,
            });
        }
        Ok(v)
    }

    async fn get_template_by_slug(
        &self,
        slug: &str,
    ) -> std::result::Result<Option<Template>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,slug,content,script_id,created_at FROM _templates WHERE slug = ?1",
        )?;
        let mut r = stmt.query(params![slug])?;
        if let Some(row) = r.next()? {
            Ok(Some(Template {
                id: row.get(0)?,
                slug: row.get(1)?,
                content: row.get(2)?,
                script_id: row.get(3)?,
                created_at: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn create_template(
        &self,
        req: CreateTemplateReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let id = self.sys_batcher.insert(
            "INSERT INTO _templates (slug,content,script_id) VALUES (?1,?2,?3) \
             ON CONFLICT(slug) DO UPDATE SET content=excluded.content,script_id=excluded.script_id,created_at=CURRENT_TIMESTAMP".into(),
            vec![req.slug.into_val(), req.content.into_val(), req.script_id.into_val()]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(id)
    }

    async fn update_template(
        &self,
        id: i64,
        content: String,
        script_id: Option<i64>,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.sys_batcher
            .execute(
                "UPDATE _templates SET content = ?1,script_id = ?2 WHERE id = ?3".into(),
                vec![content.into_val(), script_id.into_val(), id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn delete_template(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.sys_batcher
            .execute(
                "DELETE FROM _templates WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
}
