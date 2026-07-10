use crate::AppState;
use apexkit_core::{Db, models::CronJob, realtime::EventScope};
use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio_cron_scheduler::{Job, JobScheduler};

pub struct SchedulerService {
    scheduler: JobScheduler,
}

fn calculate_dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total_size = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                total_size += calculate_dir_size(&entry.path())?;
            } else {
                total_size += metadata.len();
            }
        }
    } else if path.exists() {
        total_size = path.metadata()?.len();
    }
    Ok(total_size)
}

impl SchedulerService {
    pub async fn new() -> Self {
        let scheduler = JobScheduler::new().await.unwrap();
        scheduler.start().await.unwrap();
        Self { scheduler }
    }

    pub async fn load_jobs(&self, state: AppState) {
        // ---------------------------------------------------------
        // 1. GLOBAL ROOT JOBS (Static)
        // ---------------------------------------------------------

        // Log Cleanup (Daily 3AM)
        let state_clone = state.clone();
        let cleanup_job = Job::new_async("0 3 * * *", move |_uuid, _l| {
            let s = state_clone.clone();
            Box::pin(async move {
                tracing::info!("[Root] Running Log Retention Cleanup...");
                let general = s.db.get_config("general").await.unwrap_or(None);
                let days = general
                    .and_then(|v| v.get("log_retention_days").and_then(|d| d.as_u64()))
                    .unwrap_or(7);
                crate::system::logging::cleanup_logs("storage/system/logs.db", days);
            })
        });
        if let Ok(j) = cleanup_job {
            self.scheduler.add(j).await.ok();
        }

        // Root User Defined Jobs & Backups
        let state_root = state.clone();
        let root_ticker = Job::new_async("0 * * * * *", move |_uuid, _l| {
            let s = state_root.clone();
            Box::pin(async move {
                // Root Context ID is empty or special string
                process_context_crons(s, "root".to_string(), EventScope::Root).await;
            })
        });
        if let Ok(j) = root_ticker {
            self.scheduler.add(j).await.ok();
        }

        // ---------------------------------------------------------
        // 2. TENANT TICKER (Iterates all active/disk tenants)
        // ---------------------------------------------------------
        let state_tenant = state.clone();
        let tenant_ticker = Job::new_async("0 * * * * *", move |_uuid, _l| {
            let s = state_tenant.clone();
            Box::pin(async move {
                if let Ok(tenants) = s.tenant_manager.list_tenants().await {
                    for tenant_id in tenants {
                        let s_inner = s.clone();
                        tokio::spawn(async move {
                            process_context_crons(
                                s_inner,
                                tenant_id.clone(),
                                EventScope::Tenant(tenant_id),
                            )
                            .await;
                        });
                    }
                }
            })
        });
        if let Ok(j) = tenant_ticker {
            self.scheduler.add(j).await.ok();
        }

        // ---------------------------------------------------------
        // 3. SANDBOX TICKER (Iterates active memory cache only)
        // ---------------------------------------------------------
        let state_sandbox = state.clone();
        let sandbox_ticker = Job::new_async("0 * * * * *", move |_uuid, _l| {
            let s = state_sandbox.clone();
            Box::pin(async move {
                if let Ok(entries) = std::fs::read_dir("storage/sandboxes") {
                    for entry in entries.flatten() {
                        if let Ok(fname) = entry.file_name().into_string()
                            && fname.starts_with("session_")
                        {
                            let session_id = fname.strip_prefix("session_").unwrap().to_string();

                            if s.sandbox_manager.is_active(&session_id) {
                                let s_inner = s.clone();
                                tokio::spawn(async move {
                                    process_context_crons(
                                        s_inner,
                                        session_id.clone(),
                                        EventScope::Sandbox(session_id),
                                    )
                                    .await;
                                });
                            }
                        }
                    }
                }
            })
        });
        if let Ok(j) = sandbox_ticker {
            self.scheduler.add(j).await.ok();
        }

        // ---------------------------------------------------------
        // 4. RESOURCE ANALYTICS (Tenants & Sandboxes)
        // ---------------------------------------------------------
        let state_analytics = state.clone();

        let analytics_cron = std::env::var("RESOURCE_ANALYTICS_CRON")
            .unwrap_or_else(|_| "0 */30 * * * *".to_string());
        let window_mins =
            std::env::var("RESOURCE_ANALYTICS_WINDOW_MINS").unwrap_or_else(|_| "30".to_string());
        let log_penalty_threshold: i64 = std::env::var("SYSTEM_LOGS_PENALTY_THRESHOLD")
            .unwrap_or_else(|_| "1000".to_string())
            .parse()
            .unwrap_or(1000);

        let analytics_job = Job::new_async(analytics_cron.as_str(), move |_uuid, _l| {
            let s = state_analytics.clone();
            let window = window_mins.clone();
            let threshold = log_penalty_threshold;

            Box::pin(async move {
                tracing::info!("[Root] Running Resource Analytics for Tenants & Sandboxes...");

                // --- PROCESS TENANTS ---
                if let Ok(tenants) = s.db.list_tenants().await {
                    for t in tenants {
                        let base_path = format!("storage/tenants/{}", t.id);
                        if !std::path::Path::new(&base_path).exists() {
                            continue;
                        }

                        let mut total_bytes =
                            calculate_dir_size(std::path::Path::new(&base_path)).unwrap_or(0);

                        // Logs.db is free from quotas by default (to exclude audit logs)
                        let logs_db_path = format!("{}/logs.db", base_path);
                        if let Ok(meta) = std::fs::metadata(&logs_db_path) {
                            total_bytes = total_bytes.saturating_sub(meta.len());

                            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                                &logs_db_path,
                                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                            ) {
                                // Check if tenant is dumping unreasonable amounts of system logs in the time window
                                let query = format!(
                                    "SELECT COUNT(*) FROM _system_logs WHERE timestamp >= datetime('now', '-{} minute')",
                                    window
                                );
                                let recent_sys_logs: i64 =
                                    conn.query_row(&query, [], |r| r.get(0)).unwrap_or(0);

                                // Penalize by adding the physical footprint of the system logs back to their storage total
                                if recent_sys_logs > threshold {
                                    if let Ok(sys_size) = conn.query_row("SELECT COALESCE(SUM(length(level) + length(target) + length(message) + 50), 0) FROM _system_logs", [], |r| r.get::<_, i64>(0)) {
                                        total_bytes += sys_size.max(0) as u64;
                                    }
                                }
                            }
                        }

                        let current_storage_mb = (total_bytes as f64) / (1024.0 * 1024.0);
                        let mut current_vectors = 0;
                        let mut current_ai_requests = 0;

                        // Calculate Vectors
                        let vec_db_path = format!("{}/vectors.db", base_path);
                        if let Ok(conn) = rusqlite::Connection::open_with_flags(
                            &vec_db_path,
                            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                        ) {
                            current_vectors = conn
                                .query_row("SELECT COUNT(*) FROM vectors", [], |r| {
                                    r.get::<_, i64>(0)
                                })
                                .unwrap_or(0);

                            // 1. Add Database-persisted vector generations in window
                            let ai_query = format!(
                                "SELECT COUNT(*) FROM vectors WHERE created_at >= datetime('now', '-{} minute')",
                                window
                            );
                            current_ai_requests += conn
                                .query_row(&ai_query, [], |r| r.get::<_, i64>(0))
                                .unwrap_or(0);
                        }

                        // 2. Add Memory-tracked API/Script search generations in window
                        if let Ok(ctx) = s.tenant_manager.get_tenant_context(&t.id).await {
                            current_ai_requests +=
                                ctx.vector_provider.get_and_reset_metrics() as i64;
                        }

                        let _ =
                            s.db.update_tenant_stats(
                                &t.id,
                                current_storage_mb,
                                current_vectors,
                                current_ai_requests,
                            )
                            .await;
                    }
                }

                // --- PROCESS SANDBOXES ---
                if let Ok(sandboxes) = s.db.list_sandboxes(None).await {
                    for sb in sandboxes {
                        let base_path = format!("storage/sandboxes/session_{}", sb.id);
                        if !std::path::Path::new(&base_path).exists() {
                            continue;
                        }

                        let mut total_bytes =
                            calculate_dir_size(std::path::Path::new(&base_path)).unwrap_or(0);

                        // Penalize system logs, exclude audit logs
                        let logs_db_path = format!("{}/logs.db", base_path);
                        if let Ok(meta) = std::fs::metadata(&logs_db_path) {
                            total_bytes = total_bytes.saturating_sub(meta.len());
                            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                                &logs_db_path,
                                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                            ) {
                                let query = format!(
                                    "SELECT COUNT(*) FROM _system_logs WHERE timestamp >= datetime('now', '-{} minute')",
                                    window
                                );
                                let recent_sys_logs: i64 =
                                    conn.query_row(&query, [], |r| r.get(0)).unwrap_or(0);

                                if recent_sys_logs > threshold {
                                    if let Ok(sys_size) = conn.query_row("SELECT COALESCE(SUM(length(level) + length(target) + length(message) + 50), 0) FROM _system_logs", [], |r| r.get::<_, i64>(0)) {
                                        total_bytes += sys_size.max(0) as u64;
                                    }
                                }
                            }
                        }

                        let current_storage_mb = (total_bytes as f64) / (1024.0 * 1024.0);

                        let mut current_vectors = 0;
                        let mut current_ai_requests = 0;

                        // Calculate Vectors
                        let vec_db_path = format!("{}/vectors.db", base_path);
                        if let Ok(conn) = rusqlite::Connection::open_with_flags(
                            &vec_db_path,
                            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                        ) {
                            current_vectors = conn
                                .query_row("SELECT COUNT(*) FROM vectors", [], |r| {
                                    r.get::<_, i64>(0)
                                })
                                .unwrap_or(0);

                            // 1. Add Database-persisted vector generations in window
                            let ai_query = format!(
                                "SELECT COUNT(*) FROM vectors WHERE created_at >= datetime('now', '-{} minute')",
                                window
                            );
                            current_ai_requests += conn
                                .query_row(&ai_query, [], |r| r.get::<_, i64>(0))
                                .unwrap_or(0);
                        }

                        // 2. Add Memory-tracked API/Script search generations in window
                        if let Ok(ctx) = s.sandbox_manager.get_sandbox_context(&sb.id).await {
                            current_ai_requests +=
                                ctx.vector_provider.get_and_reset_metrics() as i64;
                        }

                        let _ =
                            s.db.update_sandbox_stats(
                                &sb.id,
                                current_storage_mb,
                                current_vectors,
                                current_ai_requests,
                            )
                            .await;
                    }
                }
            })
        });

        if let Ok(j) = analytics_job {
            self.scheduler.add(j).await.ok();
        }
    }
}

// --- GENERIC PROCESSOR ---

async fn process_context_crons(state: AppState, context_id: String, scope: EventScope) {
    // 1. Resolve DB based on Scope
    let db = match &scope {
        EventScope::Root => state.db.clone(),
        EventScope::Tenant(id) => match state.tenant_manager.get_tenant(id.clone()).await {
            Ok(d) => d,
            Err(_) => return,
        },
        EventScope::Sandbox(id) => match state.sandbox_manager.get_sandbox(id).await {
            Ok(d) => d,
            Err(_) => return,
        },
        _ => return,
    };

    let now = Utc::now();
    let minute_stamp = now.format("%Y%m%d%H%M").to_string(); // Format down to the minute: e.g. 202606101038

    // --- 2. AUTOMATED BACKUPS (With Fast-Fail Lock) ---
    let backup_setting = db.get_config("backups").await.unwrap_or(None);
    if let Some(val) = backup_setting
        && let Ok(config) = serde_json::from_value::<super::dto::BackupConfigDto>(val)
        && config.enabled
    {
        if let Ok(schedule) = Schedule::from_str(&config.schedule) {
            let one_min_ago = now - chrono::Duration::seconds(60);
            if let Some(next_run) = schedule.after(&one_min_ago).next()
                && next_run <= now
            {
                // Check lock before triggering backup
                let lock_key = format!(
                    "backup_lock:{}:{}:{}",
                    context_id, config.schedule, minute_stamp
                );
                if state.root_script_cache.get(&lock_key).await.is_none() {
                    state
                        .root_script_cache
                        .insert(lock_key, "1".to_string())
                        .await;

                    tracing::info!("[Scheduler] Triggering scheduled backup for {:?}", scope);
                    let db_clone = db.clone();
                    let vault_clone = state.vault.clone();
                    let scope_clone = scope.clone();
                    tokio::spawn(async move {
                        if let Err(e) = super::backup::perform_backup(
                            db_clone,
                            vault_clone,
                            config,
                            scope_clone,
                        )
                        .await
                        {
                            tracing::error!("[Scheduler] Backup failed: {}", e);
                        }
                    });
                }
            }
        } else {
            tracing::warn!(
                "[Scheduler] Invalid backup cron schedule: {}",
                config.schedule
            );
        }
    }

    // --- 3. CUSTOM SCRIPT CRON JOBS (With Fast-Fail Lock) ---
    let cron_setting = db.get_config("cron_jobs").await.unwrap_or(None);

    if let Some(val) = cron_setting
        && let Ok(jobs) = serde_json::from_value::<Vec<CronJob>>(val)
    {
        // --- RATE LIMIT CHECK (Tenants/Sandboxes Only) ---
        if !matches!(scope, EventScope::Root) {
            let max_crons = std::env::var("TENANT_MAX_CRONS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(2); // Default 2 jobs

            let interval_mins = std::env::var("TENANT_CRON_INTERVAL")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(5); // Default 5 minutes window

            let current_minute = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                / 60;
            let window_key = current_minute / interval_mins;
            let cache_key = format!("cron_limit:{}:{}", context_id, window_key);

            let current_count = state
                .root_script_cache
                .get(&cache_key)
                .await
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);

            if current_count >= max_crons {
                tracing::warn!(
                    "[Scheduler] Rate limit hit for {}. Skipping jobs.",
                    context_id
                );
                return;
            }
        }

        // Iterate Jobs
        let mut jobs_run = 0;
        for job in jobs {
            if !job.active {
                continue;
            }

            if let Ok(schedule) = Schedule::from_str(&job.schedule) {
                let one_min_ago = now - chrono::Duration::seconds(60);
                if let Some(next_run) = schedule.after(&one_min_ago).next()
                    && next_run <= now
                {
                    // Check lock before executing script
                    let lock_key = format!("cron_lock:{}:{}:{}", context_id, job.id, minute_stamp);
                    if state.root_script_cache.get(&lock_key).await.is_none() {
                        state
                            .root_script_cache
                            .insert(lock_key, "1".to_string())
                            .await;

                        // Execute
                        execute_job(&state, db.clone(), &context_id, &job, scope.clone()).await;
                        jobs_run += 1;
                    }
                }
            }
        }

        // Increment Counter if jobs ran
        if jobs_run > 0 && !matches!(scope, EventScope::Root) {
            let interval_mins = std::env::var("TENANT_CRON_INTERVAL")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(5);
            let current_minute = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                / 60;
            let window_key = current_minute / interval_mins;
            let cache_key = format!("cron_limit:{}:{}", context_id, window_key);

            let current_count = state
                .root_script_cache
                .get(&cache_key)
                .await
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);

            state
                .root_script_cache
                .insert(cache_key, (current_count + jobs_run).to_string())
                .await;
        }
    }
}

async fn execute_job(
    state: &AppState,
    db: Arc<dyn Db>,
    _context_id: &str,
    job: &CronJob,
    scope: EventScope,
) {
    tracing::info!("[Scheduler] Executing {} in scope {:?}", job.name, scope);

    let context = Arc::new(crate::ScopedScriptContext {
        state: state.clone(),
        scope: scope.clone(),
    });

    if job.payload.starts_with("/") {
        let base = format!("http://127.0.0.1:{}", state.port);
        let url = match &scope {
            EventScope::Root => format!("{}{}", base, job.payload),
            EventScope::Tenant(id) => format!("{}/tenant/{}{}", base, id, job.payload),
            EventScope::Sandbox(id) => format!("{}/sandbox/{}{}", base, id, job.payload),
            _ => return,
        };

        let admin_email = "scheduler@system.internal";
        let token =
            apexkit_core::auth::create_jwt(0, admin_email, "admin", "root").unwrap_or_default();

        let client = reqwest::Client::new();
        let res = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await;

        match res {
            Ok(r) => {
                let status = r.status();
                if !status.is_success() {
                    let err_text = r.text().await.unwrap_or_default();
                    tracing::error!("[Scheduler] Webhook failed {}: {}", status, err_text);
                } else {
                    tracing::info!("[Scheduler] Webhook success: {}", status);
                }
            }
            Err(e) => tracing::error!("[Scheduler] Network error: {}", e),
        }
    } else {
        tracing::info!("[Scheduler] Executing script {} in scope", job.payload);

        if let Ok(Some(script)) = db.get_script_by_name(&job.payload).await {
            let _ = state
                .script_engine
                .run_script(
                    &script.code,
                    serde_json::json!({ "trigger": "cron", "job": job.name }),
                    context,
                    None,
                    None,
                )
                .await;
        }
    }
}
