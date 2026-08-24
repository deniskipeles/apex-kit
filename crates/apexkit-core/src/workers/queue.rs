use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};

use crate::database::traits::{Db, VectorProvider};
use crate::models::schema::CollectionSchema;
use crate::security::vault::Vault;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Job {
    SendWelcomeEmail {
        tenant_id: Option<String>,
        email: String,
        user_id: i64,
    },
    SendPasswordReset {
        tenant_id: Option<String>,
        email: String,
        token: String,
    },
    SendVerification {
        tenant_id: Option<String>,
        email: String,
        token: String,
    },
    ProcessImage {
        record_id: i64,
    },
    GenerateEmbedding {
        tenant_id: Option<String>,
        collection_id: i64,
        record_id: i64,
        field_name: String,
        content: String,
        content_type: String,
        model: String,
    },
    IndexRecord {
        tenant_id: Option<String>,
        collection_id: i64,
        record_id: i64,
        data: Value,
        schema: CollectionSchema,
    },
    DeleteFromIndex {
        tenant_id: Option<String>,
        collection_id: i64,
        record_id: i64,
    },
    // Bulk job for revectorizing an entire collection sequentially
    RevectorizeCollection {
        tenant_id: Option<String>,
        collection_id: i64,
        force: bool,
    },
    // Bulk job for rebuilding the Tantivy search index
    ReindexCollection {
        tenant_id: Option<String>,
        collection_id: i64,
    },
}

#[async_trait::async_trait]
pub trait JobContext: Send + Sync {
    async fn resolve(
        &self,
        tenant_id: Option<&str>,
    ) -> Option<(Arc<dyn Db>, Arc<dyn VectorProvider>)>;

    async fn get_file_bytes(
        &self,
        tenant_id: Option<&str>,
        filename: &str,
    ) -> Result<Vec<u8>, String>;
}

#[derive(Clone)]
pub struct JobQueue {
    sender: mpsc::Sender<Job>,
}

impl JobQueue {
    pub fn new(sender: mpsc::Sender<Job>) -> Self {
        Self { sender }
    }

    pub async fn enqueue(&self, job: Job) {
        let _ = self.sender.send(job).await;
    }
}

pub fn start_background_worker(
    context_resolver: Arc<dyn JobContext>,
    vault: Arc<Vault>,
) -> JobQueue {
    let (tx, mut rx) = mpsc::channel(1000);

    tokio::spawn(async move {
        println!("Background worker started...");

        // Limit concurrent heavy CPU/ML tasks to prevent starving the Tokio blocking pool.
        // 4 is a safe number for a standard server, keeping OS resources available.
        let heavy_job_semaphore = Arc::new(Semaphore::new(4));

        while let Some(job) = rx.recv().await {
            let resolver = context_resolver.clone();
            let vault_clone = vault.clone();
            let heavy_sem = heavy_job_semaphore.clone();

            tokio::spawn(async move {
                // Acquire a permit ONLY if the job is resource-intensive
                let _permit = match &job {
                    Job::GenerateEmbedding { .. }
                    | Job::IndexRecord { .. }
                    | Job::RevectorizeCollection { .. }
                    | Job::ReindexCollection { .. } => {
                        Some(heavy_sem.acquire_owned().await.unwrap())
                    }
                    _ => None,
                };

                // Helper to log job errors cleanly to the active scope DB
                let log_job_error = |tenant_id: Option<String>, msg: String, res: Arc<dyn JobContext>| async move {
                    if let Some((db, _)) = res.resolve(tenant_id.as_deref()).await {
                        let _ = db.log_system_event("error", "background_job", &msg).await;
                    }
                };

                match job {
                    Job::SendWelcomeEmail {
                        tenant_id,
                        email,
                        user_id,
                    } => {
                        if let Err(e) = super::tasks::emails::handle_welcome_email(
                            resolver.clone(),
                            vault_clone,
                            tenant_id.clone(),
                            email,
                            user_id,
                        )
                        .await
                        {
                            log_job_error(tenant_id, format!("Welcome email failed: {}", e), resolver).await;
                        }
                    }
                    Job::SendPasswordReset {
                        tenant_id,
                        email,
                        token,
                    } => {
                        if let Err(e) = super::tasks::emails::handle_password_reset(
                            resolver.clone(),
                            vault_clone,
                            tenant_id.clone(),
                            email,
                            token,
                        )
                        .await
                        {
                            log_job_error(tenant_id, format!("Reset password email failed: {}", e), resolver).await;
                        }
                    }
                    Job::SendVerification {
                        tenant_id,
                        email,
                        token,
                    } => {
                        if let Err(e) = super::tasks::emails::handle_verification_email(
                            resolver.clone(),
                            vault_clone,
                            tenant_id.clone(),
                            email,
                            token,
                        )
                        .await
                        {
                            log_job_error(tenant_id, format!("Verification email failed: {}", e), resolver).await;
                        }
                    }
                    Job::GenerateEmbedding {
                        tenant_id,
                        collection_id,
                        record_id,
                        field_name,
                        content,
                        content_type,
                        model,
                    } => {
                        if let Err(e) = super::tasks::vectorization::handle_generate_embedding(
                            resolver.clone(),
                            tenant_id.clone(),
                            collection_id,
                            record_id,
                            field_name,
                            content,
                            content_type,
                            Some(model),
                        )
                        .await
                        {
                            log_job_error(tenant_id, format!("Embedding generation failed: {}", e), resolver).await;
                        }
                    }
                    Job::IndexRecord {
                        tenant_id,
                        collection_id,
                        record_id,
                        data,
                        schema,
                    } => {
                        if let Err(e) = super::tasks::vectorization::handle_index_record(
                            resolver.clone(),
                            tenant_id.clone(),
                            collection_id,
                            record_id,
                            data,
                            schema,
                        )
                        .await
                        {
                            log_job_error(tenant_id, format!("Search Indexing failed for {}: {}", record_id, e), resolver).await;
                        }
                    }
                    Job::DeleteFromIndex {
                        tenant_id,
                        collection_id,
                        record_id,
                    } => {
                        if let Err(e) = super::tasks::vectorization::handle_delete_record(
                            resolver.clone(),
                            tenant_id.clone(),
                            collection_id,
                            record_id,
                        )
                        .await
                        {
                            log_job_error(tenant_id, format!("Search Index Deletion failed for {}: {}", record_id, e), resolver).await;
                        }
                    }
                    Job::RevectorizeCollection {
                        tenant_id,
                        collection_id,
                        force,
                    } => {
                        if let Err(e) = super::tasks::vectorization::handle_revectorize_collection(
                            resolver.clone(),
                            tenant_id.clone(),
                            collection_id,
                            force,
                        )
                        .await
                        {
                            log_job_error(tenant_id, format!("Bulk revectorization failed: {}", e), resolver).await;
                        }
                    }
                    Job::ReindexCollection {
                        tenant_id,
                        collection_id,
                    } => {
                        if let Err(e) = super::tasks::vectorization::handle_reindex_collection(
                            resolver.clone(),
                            tenant_id.clone(),
                            collection_id,
                        )
                        .await
                        {
                            log_job_error(tenant_id, format!("Bulk reindexing failed: {}", e), resolver).await;
                        }
                    }
                    _ => {}
                }
            });
        }
    });

    JobQueue::new(tx)
}