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
        let core_conn = self.get_core_read().await;
        let data_conn = self.get_data_read().await;
        let log_conn = self.get_log_read().await;
        let sys_conn = self.get_sys_read().await;
        let vec_conn = self.get_vector_read().await;

        let collections_count: i64 = data_conn.query_row("SELECT COUNT(*) FROM collections", [], |r| r.get(0)).unwrap_or(0);
        let total_records: i64 = data_conn.query_row("SELECT COUNT(*) FROM records", [], |r| r.get(0)).unwrap_or(0);
        let total_vectors: i64 = vec_conn.query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0)).unwrap_or(0);
        let total_files: i64 = data_conn.query_row("SELECT COUNT(*) FROM _storage_files", [], |r| r.get(0)).unwrap_or(0);

        // --- Calculate DB Sizes ---
        let mut db_sizes = Vec::new();
        let mut total_db_bytes: i64 = 0;

        let conns: &[(&str, &rusqlite::Connection)] = &[
            ("core.db", &core_conn),
            ("data.db", &data_conn),
            ("logs.db", &log_conn),
            ("system.db", &sys_conn),
            ("vectors.db", &vec_conn),
        ];

        for (name, conn) in conns {
            let p_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap_or(0);
            let p_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap_or(0);
            let bytes = p_count * p_size;
            total_db_bytes += bytes;
            db_sizes.push(crate::models::DbSizeDetail {
                name: name.to_string(),
                size_mb: (bytes as f64 / 1024.0 / 1024.0 * 100.0).round() / 100.0,
            });
        }
        let db_size_mb = (total_db_bytes as f64 / 1024.0 / 1024.0 * 100.0).round() / 100.0;

        // --- Calculate Index Sizes ---
        let mut index_sizes = Vec::new();
        let mut col_map = std::collections::HashMap::new();
        if let Ok(mut stmt_cm) = data_conn.prepare("SELECT id, name FROM collections") {
            if let Ok(mut rows_cm) = stmt_cm.query([]) {
                while let Some(row) = rows_cm.next().unwrap_or(None) {
                    if let (Ok(id), Ok(name)) = (row.get::<_, i64>(0), row.get::<_, String>(1)) {
                        col_map.insert(id.to_string(), name);
                    }
                }
            }
        }

        let mut total_index_bytes: u64 = 0;
        let indexes_path = format!("{}/indexes", self.base_path);
        if let Ok(entries) = std::fs::read_dir(&indexes_path) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let id_str = entry.file_name().to_string_lossy().to_string();
                    let size_bytes = calculate_dir_size(&entry.path()).unwrap_or(0);
                    if size_bytes > 0 {
                        total_index_bytes += size_bytes;
                        let col_name = col_map.get(&id_str).cloned().unwrap_or_else(|| format!("Unknown ({})", id_str));
                        index_sizes.push(crate::models::IndexSizeDetail {
                            collection_name: col_name,
                            size_mb: (size_bytes as f64 / 1024.0 / 1024.0 * 100.0).round() / 100.0,
                        });
                    }
                }
            }
        }
        // Sort index sizes descending
        index_sizes.sort_by(|a, b| b.size_mb.partial_cmp(&a.size_mb).unwrap_or(std::cmp::Ordering::Equal));
        let indexes_size_mb = (total_index_bytes as f64 / 1024.0 / 1024.0 * 100.0).round() / 100.0;

        // --- Vector Models Breakdown ---
        let mut vector_models = Vec::new();
        if let Ok(mut stmt_vm) = vec_conn.prepare("SELECT model, COUNT(*) FROM vectors GROUP BY model ORDER BY COUNT(*) DESC") {
            if let Ok(mut rows_vm) = stmt_vm.query([]) {
                while let Some(row) = rows_vm.next().unwrap_or(None) {
                    if let (Ok(model), Ok(count)) = (row.get(0), row.get(1)) {
                        vector_models.push(crate::models::VectorModelDetail { model, count });
                    }
                }
            }
        }

        // --- Top Collections Breakdown ---
        let mut top_collections = Vec::new();
        let query = "SELECT c.name, COUNT(r.id) as cnt FROM collections c LEFT JOIN records r ON c.id = r.collection_id GROUP BY c.id ORDER BY cnt DESC";
        if let Ok(mut stmt_tc) = data_conn.prepare(query) {
            if let Ok(mut rows_tc) = stmt_tc.query([]) {
                while let Some(row) = rows_tc.next().unwrap_or(None) {
                    if let (Ok(name), Ok(count)) = (row.get(0), row.get(1)) {
                        top_collections.push(crate::models::CollectionRecordDetail { name, count });
                    }
                }
            }
        }

        // --- Chart & Logs ---
        let sql_chart = "
            SELECT strftime('%Y-%m-%d',timestamp) as day_date, COUNT(*) as req_count, SUM(CASE WHEN level = 'ERROR' OR level = 'error' THEN 1 ELSE 0 END) as err_count 
            FROM _system_logs WHERE timestamp >= date('now','-7 days') GROUP BY day_date
        ";
        let mut daily_stats: HashMap<String, (i64, i64)> = HashMap::new();
        let mut total_requests = 0;
        
        if let Ok(mut stmt_chart) = log_conn.prepare(sql_chart) {
            if let Ok(mut rows) = stmt_chart.query([]) {
                while let Some(row) = rows.next().unwrap_or(None) {
                    if let (Ok(date_str), Ok(reqs), Ok(errs)) = (row.get::<_,String>(0), row.get(1), row.get(2)) {
                        total_requests += reqs;
                        daily_stats.insert(date_str, (reqs, errs));
                    }
                }
            }
        }

        let mut chart_data: Vec<ChartPoint> = Vec::new();
        let now = Utc::now();
        for i in (0..7).rev() {
            let date = now - chrono::Duration::days(i);
            let date_key = date.format("%Y-%m-%d").to_string();
            let day_name = date.format("%a").to_string();
            let (reqs, errs) = daily_stats.get(&date_key).unwrap_or(&(0, 0));
            chart_data.push(ChartPoint { name: day_name, requests: *reqs, errors: *errs });
        }

        let mut recent_logs = Vec::new();
        if let Ok(mut stmt_logs) = log_conn.prepare("SELECT id,level,message,target,timestamp FROM _system_logs ORDER BY timestamp DESC LIMIT 100") {
            if let Ok(mut recent_rows) = stmt_logs.query([]) {
                while let Some(row) = recent_rows.next().unwrap_or(None) {
                    recent_logs.push(serde_json::json!({
                        "id": row.get::<_, i64>(0).unwrap_or(0).to_string(),
                        "level": row.get::<_, String>(1).unwrap_or_default(),
                        "source": row.get::<_, String>(2).unwrap_or_default(),
                        "message": row.get::<_, String>(3).unwrap_or_default(),
                        "timestamp": row.get::<_, String>(4).unwrap_or_default()
                    }));
                }
            }
        }

        Ok(DashboardData {
            stats: DashboardStats {
                total_requests,
                db_size_mb,
                collections_count,
                total_records,
                total_vectors,
                total_files,
                indexes_size_mb,
            },
            chart: chart_data,
            recent_logs,
            db_sizes,
            vector_models,
            index_sizes,
            top_collections,
        })
    }
}
