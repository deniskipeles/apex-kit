use super::ApexKit;
use crate::database::traits::{AuditStore, IntoSqlVal};
use async_trait::async_trait;

#[async_trait]
impl AuditStore for ApexKit {
    async fn log_audit_event(
        &self,
        level: &str,
        message: &str,
        source: &str,
        meta: Option<serde_json::Value>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let meta_str = serde_json::to_string(&meta).unwrap_or("{}".to_string());
        self.log_batcher
            .execute(
                "INSERT INTO _audit_logs (level,message,source,meta) VALUES (?1,?2,?3,?4)".into(),
                vec![
                    level.into_val(),
                    message.into_val(),
                    source.into_val(),
                    meta_str.into_val(),
                ],
            )
            .await
            .map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;
        Ok(())
    }

    async fn list_audit_logs(
        &self,
    ) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_log_read().await;
        let mut stmt = conn.prepare("SELECT id,level,message,source,meta,timestamp FROM _audit_logs ORDER BY timestamp DESC LIMIT 100")?;
        let mut rows = stmt.query([])?;
        let mut logs = Vec::new();
        while let Some(row) = rows.next()? {
            let meta_str: Option<String> = row.get(4)?;
            let meta =
                meta_str.map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null));
            logs.push(serde_json::json!({
                "id": row.get::<usize, i64>(0)?,
                "level": row.get::<usize, String>(1)?,
                "message": row.get::<usize, String>(2)?,
                "source": row.get::<usize, String>(3)?,
                "meta": meta,
                "timestamp": row.get::<usize, String>(5)?
            }));
        }
        Ok(logs)
    }

    async fn log_system_event(
        &self,
        level: &str,
        target: &str,
        message: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.log_batcher
            .execute(
                "INSERT INTO _system_logs (level,target,message) VALUES (?1,?2,?3)".into(),
                vec![level.into_val(), target.into_val(), message.into_val()],
            )
            .await
            .map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;
        Ok(())
    }

    async fn list_paginated_logs(
        &self,
        log_type: &str,
        page: i64,
        per_page: i64,
        level: Option<String>,
        source: Option<String>,
        search: Option<String>,
    ) -> std::result::Result<(Vec<serde_json::Value>, i64), Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_log_read().await;

        let mut where_clauses = Vec::new();
        let mut params: Vec<rusqlite::types::Value> = Vec::new();

        let (table_name, source_col) = if log_type == "audit" {
            ("_audit_logs", "source")
        } else {
            ("_system_logs", "target")
        };

        if let Some(lvl) = level {
            where_clauses.push(format!("{}.level = ?", table_name));
            params.push(lvl.into_val());
        }
        if let Some(src) = source {
            where_clauses.push(format!("{}.{} LIKE ?", table_name, source_col));
            params.push(format!("%{}%", src).into_val());
        }
        if let Some(q) = search {
            where_clauses.push(format!(
                "({}.message LIKE ? OR {}.{} LIKE ?)",
                table_name, table_name, source_col
            ));
            params.push(format!("%{}%", q).into_val());
            params.push(format!("%{}%", q).into_val());
        }

        let where_sql = if where_clauses.is_empty() {
            "".to_string()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) FROM {} {}", table_name, where_sql);
        let mut count_stmt = conn.prepare(&count_sql)?;
        let total: i64 =
            count_stmt.query_row(rusqlite::params_from_iter(params.clone()), |row| row.get(0))?;

        let limit = per_page;
        let offset = (page - 1) * per_page;

        let mut logs = Vec::new();

        if log_type == "audit" {
            let select_sql = format!(
                "SELECT id,level,message,source,meta,timestamp FROM _audit_logs {} ORDER BY timestamp DESC LIMIT {} OFFSET {}",
                where_sql, limit, offset
            );
            let mut stmt = conn.prepare(&select_sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
            while let Some(row) = rows.next()? {
                let meta_str: Option<String> = row.get(4)?;
                let meta =
                    meta_str.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());

                logs.push(serde_json::json!({
                    "id": row.get::<usize, i64>(0)?.to_string(),
                    "level": row.get::<usize, String>(1)?,
                    "message": row.get::<usize, String>(2)?,
                    "source": row.get::<usize, String>(3)?,
                    "meta": meta,
                    "timestamp": row.get::<usize, String>(5)?
                }));
            }
        } else {
            let select_sql = format!(
                "SELECT id,level,target,message,timestamp FROM _system_logs {} ORDER BY timestamp DESC LIMIT {} OFFSET {}",
                where_sql, limit, offset
            );
            let mut stmt = conn.prepare(&select_sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
            while let Some(row) = rows.next()? {
                logs.push(serde_json::json!({
                    "id": row.get::<usize, i64>(0)?.to_string(),
                    "level": row.get::<usize, String>(1)?,
                    "source": row.get::<usize, String>(2)?,
                    "message": row.get::<usize, String>(3)?,
                    "timestamp": row.get::<usize, String>(4)?
                }));
            }
        }

        Ok((logs, total))
    }
}
