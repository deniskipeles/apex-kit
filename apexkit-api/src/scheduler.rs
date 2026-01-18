use crate::{AppState, DatabaseConnection};
use tokio_cron_scheduler::{Job, JobScheduler};
use apexkit_core::{models::CronJob, Db, VectorProvider, security::Vault, realtime::EventScope};
use crate::settings::BackupConfigDto;
use std::sync::Arc;
use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;

pub struct SchedulerService {
    scheduler: JobScheduler,
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
                let days = general.and_then(|v| v.get("log_retention_days").and_then(|d| d.as_u64())).unwrap_or(7);
                crate::logging::cleanup_logs("storage/system/logs.db", days);
            })
        });
        if let Ok(j) = cleanup_job { self.scheduler.add(j).await.ok(); }

        // Root User Defined Jobs
        let state_root = state.clone();
        let root_ticker = Job::new_async("0 * * * * *", move |_uuid, _l| {
            let s = state_root.clone();
            Box::pin(async move {
                // Root Context ID is empty or special string
                process_context_crons(s, "root".to_string(), EventScope::Root).await;
            })
        });
        if let Ok(j) = root_ticker { self.scheduler.add(j).await.ok(); }


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
                            process_context_crons(s_inner, tenant_id.clone(), EventScope::Tenant(tenant_id)).await;
                        });
                    }
                }
            })
        });
        if let Ok(j) = tenant_ticker { self.scheduler.add(j).await.ok(); }


        // ---------------------------------------------------------
        // 3. SANDBOX TICKER (Iterates active memory cache only?)
        //    Sandboxes on disk but not in memory are "frozen" usually.
        //    But for consistency, we should list disk too.
        // ---------------------------------------------------------
        let state_sandbox = state.clone();
        let sandbox_ticker = Job::new_async("0 * * * * *", move |_uuid, _l| {
            let s = state_sandbox.clone();
            Box::pin(async move {
                // Note: We need list_sandboxes on manager. Assuming it exists or we add it similar to tenants.
                // If not, we can iterate fs::read_dir("storage/sandboxes") manually here.
                if let Ok(entries) = std::fs::read_dir("storage/sandboxes") {
                    for entry in entries.flatten() {
                        if let Ok(fname) = entry.file_name().into_string() {
                            if fname.starts_with("session_") {
                                let session_id = fname.strip_prefix("session_").unwrap().to_string();
                                let s_inner = s.clone();
                                tokio::spawn(async move {
                                    process_context_crons(s_inner, session_id.clone(), EventScope::Sandbox(session_id)).await;
                                });
                            }
                        }
                    }
                }
            })
        });
        if let Ok(j) = sandbox_ticker { self.scheduler.add(j).await.ok(); }
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

    // 2. Get Jobs Config
    let cron_setting = db.get_config("cron_jobs").await.unwrap_or(None);
    
    if let Some(val) = cron_setting {
        if let Ok(jobs) = serde_json::from_value::<Vec<CronJob>>(val) {
            let now = Utc::now();
            
            for job in jobs {
                if !job.active { continue; }

                if let Ok(schedule) = Schedule::from_str(&job.schedule) {
                    // Check if job should have run in the last minute (since we tick every minute)
                    // We look for the most recent occurrence BEFORE now.
                    // If it happened < 60 seconds ago, we run it.
                    // (Note: This might double-run if ticker logic drifts, robust systems use a 'last_run' DB field)
                    // For ApexKit v1, simple window check is okay.
                    
                    // Actually, 'upcoming' gives future. We need to check if 'now' matches.
                    // Or check if the *previous* occurrence was recent.
                    // Logic: Get upcoming from (Now - 1 minute). If that occurrence is <= Now, run it.
                    
                    let one_min_ago = now - chrono::Duration::seconds(60);
                    // [FIX] Explicitly annotate type for next_run
                    if let Some(next_run) = schedule.after(&one_min_ago).next() {
                         if next_run <= now {
                             execute_job(&state, db.clone(), &context_id, &job, scope.clone()).await;
                         }
                    }
                }
            }
        }
    }
}

async fn execute_job(state: &AppState, db: Arc<dyn Db>, context_id: &str, job: &CronJob, scope: EventScope) {
    tracing::info!("[Scheduler] Executing {} in scope {:?}", job.name, scope);

    // Context Injection Wrapper
    // This allows $db in the script to point to the correct Tenant/Sandbox DB
    struct ScopedContext {
        inner: AppState,
        scoped_db: Arc<dyn Db>,
    }

    impl apexkit_core::ScriptContext for ScopedContext {
        fn get_db(&self) -> Arc<dyn Db> { self.scoped_db.clone() }
        fn get_vault(&self) -> Arc<Vault> { self.inner.vault.clone() }
        fn get_embedder(&self) -> Arc<apexkit_core::embeddings::EmbedderService> { self.inner.embedder.clone() }
        fn get_vector_provider(&self) -> Arc<dyn VectorProvider> { self.inner.vector_provider.clone() }
        fn get_realtime_tx(&self) -> tokio::sync::broadcast::Sender<apexkit_core::realtime::DbEvent> { self.inner.tx.clone() }
        fn resolve_tenant_db(&self, id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>> { self.inner.resolve_tenant_db(id) }
        fn resolve_sandbox_db(&self, id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>> { self.inner.resolve_sandbox_db(id) }
        fn admin_create_tenant(&self, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send>> { self.inner.admin_create_tenant(id) }
    }

    if job.payload.starts_with("/") {
        let base = format!("http://127.0.0.1:{}", state.port);
        let url = match &scope {
            EventScope::Root => format!("{}{}", base, job.payload),
            EventScope::Tenant(id) => format!("{}/tenant/{}{}", base, id, job.payload),
            EventScope::Sandbox(id) => format!("{}/sandbox/{}{}", base, id, job.payload),
            _ => return,
        };
        
        // [FIX] Generate Internal Admin Token for Loopback Request
        // We assume User ID 1 is admin.
        // NOTE: In a real system, use a dedicated "system" user or API key.
        // For simplicity, we mint a token for the first admin user found.
        let admin_email = "scheduler@system.internal"; 
        // Mint token directly using core auth logic (bypassing DB lookup for speed/reliability if secret is known)
        // But we need the secret. State has Vault? No, Vault uses random key. 
        // We use the crate::auth::create_jwt which uses the static SECRET constant in auth.rs (in v0.1.0).
        // If auth.rs uses env var, we are good.
        
        let token = apexkit_core::auth::create_jwt(0, admin_email, "admin").unwrap_or_default();

        let client = reqwest::Client::new();
        let res = client.post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body("{}") // [FIX] Send empty JSON object
            .send()
            .await;
            
            match res {
                Ok(r) => {
                    let status = r.status(); // Capture status first
                    if !status.is_success() {
                        let err_text = r.text().await.unwrap_or_default(); // Consumes r
                        tracing::error!("[Scheduler] Webhook failed {}: {}", status, err_text);
                    } else {
                         tracing::info!("[Scheduler] Webhook success: {}", status);
                    }
                },
                Err(e) => tracing::error!("[Scheduler] Network error: {}", e),
            }
    } else {
        // Script
        tracing::info!("[Scheduler] Executing script {} in scope", job.payload);
        if let Ok(Some(script)) = db.get_script_by_name(&job.payload).await {
            let context = Arc::new(ScopedContext {
                inner: state.clone(),
                scoped_db: db.clone()
            });

            let _ = state.script_engine.run_script(
                &script.code,
                serde_json::json!({ "trigger": "cron", "job": job.name }),
                context,
                None,
                scope
            ).await;
        }
    }
}