use crate::AppState;
// use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use apexkit_core::models::CronJob;
// use crate::settings::AppSettingsDto;

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
        // 1. Add System Maintenance Job (Log Cleanup)
        // Runs daily at 3 AM
        let state_clone = state.clone();
        let cleanup_job = Job::new_async("0 3 * * *", move |_uuid, _l| {
            let s = state_clone.clone();
            Box::pin(async move {
                tracing::info!("Running Log Retention Cleanup...");
                
                // Fetch settings to get retention days
                let general = s.db.get_setting("general").await.unwrap_or(None);
                let mut days = 7; // Default
                if let Some(val) = general {
                    if let Some(d) = val.get("log_retention_days").and_then(|v| v.as_u64()) {
                        days = d;
                    }
                }
                
                // Run cleanup
                crate::logging::cleanup_logs("logs", days);
            })
        });
        
        if let Ok(j) = cleanup_job {
            self.scheduler.add(j).await.ok();
        }

        // 2. User Defined Jobs:
            // 1. Clear existing jobs (naive approach: create new scheduler or remove all)
            // For simplicity in this architecture, we just assume this runs on startup/reload
            // In a production grade, we would track job IDs to update them.
            
            // 2. Fetch Settings
            // let settings_json = state.db.get_setting("backups").await.unwrap_or(None);
            // Note: In your UI, CronJobs are likely inside a general settings blob or specific key.
            // Based on previous context, let's assume they are in a 'cron' setting or part of 'general'.
            // Let's assume they are stored in a key "cron_jobs".
        
        let cron_setting = state.db.get_setting("cron_jobs").await.unwrap_or(None);
        
        if let Some(val) = cron_setting {
            if let Ok(jobs) = serde_json::from_value::<Vec<CronJob>>(val) {
                for job in jobs {
                    if !job.active { continue; }

                    let job_payload = job.payload.clone();
                    let job_name = job.name.clone();

                    // Create the Job
                    // We wrap the async execution in the job closure
                    let j = Job::new_async(job.schedule.as_str(), move |_uuid, _l| {
                        let payload = job_payload.clone();
                        let name = job_name.clone();
                        Box::pin(async move {
                            tracing::info!("Executing Cron Job: {}", name);
                            
                            // Simple implementation: Payload is a URL (Internal or External)
                            // If it starts with /, append localhost
                            let url = if payload.starts_with("/") {
                                format!("http://127.0.0.1:5000{}", payload)
                            } else {
                                payload
                            };

                            let client = reqwest::Client::new();
                            match client.post(&url).send().await {
                                Ok(res) => tracing::info!("Job {} finished: Status {}", name, res.status()),
                                Err(e) => tracing::error!("Job {} failed: {}", name, e),
                            }
                        })
                    });

                    match j {
                        Ok(job_instance) => {
                            if let Err(e) = self.scheduler.add(job_instance).await {
                                tracing::error!("Failed to add job {}: {}", job.name, e);
                            } else {
                                tracing::info!("Scheduled job: {} ({})", job.name, job.schedule);
                            }
                        },
                        Err(e) => tracing::error!("Invalid schedule for {}: {}", job.name, e),
                    }
                }
            }
        }
    }
}