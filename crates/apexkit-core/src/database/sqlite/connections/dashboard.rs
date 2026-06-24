use super::ApexKit;
use crate::database::sqlite::utils::calculate_dir_size;
use crate::database::traits::DashboardStore;
use crate::models::{ChartPoint, DashboardData, DashboardStats};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;

#[async_trait]
impl DashboardStore for ApexKit {
    async fn get_dashboard_stats(
        &self,
    ) -> std::result::Result<DashboardData, Box<dyn std::error::Error + Send + Sync>> {
        let data_conn = self.get_data_read().await;
        let log_conn = self.get_log_read().await;
        let sys_conn = self.get_sys_read().await;
        let vec_conn = self.get_vector_read().await;

        let mut stmt1 = data_conn.prepare("SELECT COUNT(*) FROM collections")?;
        let mut row1 = stmt1.query([])?;
        let collections_count: i64 = if let Some(r) = row1.next()? {
            r.get(0)?
        } else {
            0
        };

        let mut stmt2 = data_conn.prepare("SELECT COUNT(*) FROM records")?;
        let mut row2 = stmt2.query([])?;
        let total_records: i64 = if let Some(r) = row2.next()? {
            r.get(0)?
        } else {
            0
        };

        let mut stmt3 = vec_conn.prepare("SELECT COUNT(*) FROM vectors")?;
        let mut row3 = stmt3.query([])?;
        let total_vectors: i64 = if let Some(r) = row3.next()? {
            r.get(0)?
        } else {
            0
        };

        let mut total_bytes: i64 = 0;

        for conn in [&data_conn, &log_conn, &sys_conn, &vec_conn] {
            let mut stmt_c = conn.prepare("PRAGMA page_count")?;
            let mut p_count = stmt_c.query([])?;
            let count: i64 = if let Some(r) = p_count.next()? {
                r.get(0)?
            } else {
                0
            };

            let mut stmt_s = conn.prepare("PRAGMA page_size")?;
            let mut p_size = stmt_s.query([])?;
            let size: i64 = if let Some(r) = p_size.next()? {
                r.get(0)?
            } else {
                0
            };

            total_bytes += count * size;
        }

        let db_size_mb = (total_bytes as f64 / 1024.0 / 1024.0 * 100.0).round() / 100.0;

        let indexes_size_mb =
            (calculate_dir_size(std::path::Path::new(&format!("{}/indexes", self.base_path)))
                .unwrap_or(0) as f64
                / 1024.0
                / 1024.0
                * 100.0)
                .round()
                / 100.0;

        let sql_chart = "
            SELECT \
                strftime('%Y-%m-%d',timestamp) as day_date, \
                COUNT(*) as req_count, \
                SUM(CASE WHEN level = 'ERROR' OR level = 'error' THEN 1 ELSE 0 END) as err_count \
            FROM _system_logs \
            WHERE timestamp >= date('now','-7 days') \
            GROUP BY day_date
        ";

        let mut stmt_chart = log_conn.prepare(sql_chart)?;
        let mut rows = stmt_chart.query([])?;
        let mut daily_stats: HashMap<String, (i64, i64)> = HashMap::new();
        let mut total_requests = 0;

        while let Some(row) = rows.next()? {
            let date_str: String = row.get(0)?;
            let reqs: i64 = row.get(1)?;
            let errs: i64 = row.get(2)?;
            total_requests += reqs;
            daily_stats.insert(date_str, (reqs, errs));
        }

        let mut chart_data: Vec<ChartPoint> = Vec::new();
        let now = Utc::now();
        for i in (0..7).rev() {
            let date = now - chrono::Duration::days(i);
            let date_key = date.format("%Y-%m-%d").to_string();
            let day_name = date.format("%a").to_string();
            let (reqs, errs) = daily_stats.get(&date_key).unwrap_or(&(0, 0));
            chart_data.push(ChartPoint {
                name: day_name,
                requests: *reqs,
                errors: *errs,
            });
        }

        let mut stmt_logs = log_conn.prepare(
            "SELECT id,level,message,target,timestamp FROM _system_logs ORDER BY timestamp DESC LIMIT 100"
        )?;
        let mut recent_rows = stmt_logs.query([])?;

        let mut recent_logs = Vec::new();
        while let Some(row) = recent_rows.next()? {
            recent_logs.push(serde_json::json!({
                "id": row.get::<usize, i64>(0)?.to_string(),
                "level": row.get::<usize, String>(1)?,
                "message": row.get::<usize, String>(2)?,
                "source": row.get::<usize, String>(3)?,
                "timestamp": row.get::<usize, String>(4)?
            }));
        }

        Ok(DashboardData {
            stats: DashboardStats {
                total_requests,
                db_size_mb,
                collections_count,
                total_records,
                total_vectors,
                indexes_size_mb,
            },
            chart: chart_data,
            recent_logs,
        })
    }
}
