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
    // [NEW] Bulk job for revectorizing an entire collection sequentially
    RevectorizeCollection {
        tenant_id: Option<String>,
        collection_id: i64,
        force: bool,
    },
    // [NEW] Bulk job for rebuilding the Tantivy search index
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
        if let Err(e) = self.sender.send(job).await {
            eprintln!("Failed to enqueue job: {}", e);
        }
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

                match job {
                    Job::SendWelcomeEmail {
                        tenant_id,
                        email,
                        user_id,
                    } => {
                        if let Err(e) = super::tasks::emails::handle_welcome_email(
                            resolver,
                            vault_clone,
                            tenant_id,
                            email,
                            user_id,
                        )
                        .await
                        {
                            eprintln!("[Job] Welcome email failed: {}", e);
                        }
                    }
                    Job::SendPasswordReset {
                        tenant_id,
                        email,
                        token,
                    } => {
                        if let Err(e) = super::tasks::emails::handle_password_reset(
                            resolver,
                            vault_clone,
                            tenant_id,
                            email,
                            token,
                        )
                        .await
                        {
                            eprintln!("[Job] Reset password email failed: {}", e);
                        }
                    }
                    Job::SendVerification {
                        tenant_id,
                        email,
                        token,
                    } => {
                        if let Err(e) = super::tasks::emails::handle_verification_email(
                            resolver,
                            vault_clone,
                            tenant_id,
                            email,
                            token,
                        )
                        .await
                        {
                            eprintln!("[Job] Verification email failed: {}", e);
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
                            resolver,
                            tenant_id,
                            collection_id,
                            record_id,
                            field_name,
                            content,
                            content_type,
                            Some(model),
                        )
                        .await
                        {
                            eprintln!("[Job] Embedding generation failed: {}", e);
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
                            resolver,
                            tenant_id,
                            collection_id,
                            record_id,
                            data,
                            schema,
                        )
                        .await
                        {
                            eprintln!("[Job] Search Indexing failed for {}: {}", record_id, e);
                        }
                    }
                    Job::DeleteFromIndex {
                        tenant_id,
                        collection_id,
                        record_id,
                    } => {
                        if let Err(e) = super::tasks::vectorization::handle_delete_record(
                            resolver,
                            tenant_id,
                            collection_id,
                            record_id,
                        )
                        .await
                        {
                            eprintln!(
                                "[Job] Search Index Deletion failed for {}: {}",
                                record_id, e
                            );
                        }
                    }
                    Job::RevectorizeCollection {
                        tenant_id,
                        collection_id,
                        force,
                    } => {
                        if let Err(e) = super::tasks::vectorization::handle_revectorize_collection(
                            resolver,
                            tenant_id,
                            collection_id,
                            force,
                        )
                        .await
                        {
                            eprintln!("[Job] Bulk revectorization failed: {}", e);
                        }
                    }
                    Job::ReindexCollection {
                        tenant_id,
                        collection_id,
                    } => {
                        if let Err(e) = super::tasks::vectorization::handle_reindex_collection(
                            resolver,
                            tenant_id,
                            collection_id,
                        )
                        .await
                        {
                            eprintln!("[Job] Bulk reindexing failed: {}", e);
                        }
                    }
                    _ => {}
                }
            });
        }
    });

    JobQueue::new(tx)
}
