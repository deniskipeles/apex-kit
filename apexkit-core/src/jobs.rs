use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use lettre::{Message, SmtpTransport, Transport};
use lettre::transport::smtp::authentication::Credentials;
use std::sync::Arc;
use crate::{Db, VectorProvider, security::{ Vault, EncryptedValue }, schema::CollectionSchema};
use serde_json::Value;
use async_trait::async_trait;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Job {
    SendWelcomeEmail { email: String, user_id: i64 },
    SendPasswordReset { email: String, token: String },
    SendVerification { email: String, token: String },
    ProcessImage { record_id: i64 },
    
    // --- Vectorization Job ---
    GenerateEmbedding { 
        tenant_id: Option<String>, 
        collection_id: i64, 
        record_id: i64, 
        field_name: String, 
        text_content: String,
        model: String,
    },

    // --- Async Search Indexing ---
    IndexRecord {
        tenant_id: Option<String>,
        collection_id: i64,
        record_id: i64,
        data: Value,
        schema: CollectionSchema
    },
    
    DeleteFromIndex {
        tenant_id: Option<String>,
        collection_id: i64,
        record_id: i64
    }
}

// New Trait to resolve DB context dynamically
#[async_trait]
pub trait JobContext: Send + Sync {
    async fn resolve(&self, tenant_id: Option<&str>) -> Option<(Arc<dyn Db>, Arc<dyn VectorProvider>)>;
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

// Updated Worker Signature
pub fn start_background_worker(
    context_resolver: Arc<dyn JobContext>,
    vault: Arc<Vault>,
) -> JobQueue {
    let (tx, mut rx) = mpsc::channel(1000);

    tokio::spawn(async move {
        println!("Background worker started...");
        
        while let Some(job) = rx.recv().await {
            let resolver = context_resolver.clone();
            let vault_clone = vault.clone();

            tokio::spawn(async move {
                match job {
                    // --- Email Jobs ---
                    Job::SendWelcomeEmail { email, user_id: _ } => {
                        if let Some((db, _)) = resolver.resolve(None).await {
                            if let Err(e) = send_email(db, vault_clone, &email, "Welcome to ApexKit!", "Thanks for signing up!").await {
                                eprintln!("[Job] Welcome email failed: {}", e);
                            }
                        }
                    }
                    Job::SendPasswordReset { email, token } => {
                        if let Some((db, _)) = resolver.resolve(None).await {
                            let body = format!("Reset: http://localhost:5000/reset-password?token={}", token);
                            if let Err(e) = send_email(db, vault_clone, &email, "Reset Password", &body).await {
                                eprintln!("[Job] Reset password email failed: {}", e);
                            }
                        }
                    }
                    Job::SendVerification { email, token } => {
                        if let Some((db, _)) = resolver.resolve(None).await {
                            let body = format!("Verify: http://localhost:5000/verify?token={}", token);
                            if let Err(e) = send_email(db, vault_clone, &email, "Verify Email", &body).await {
                                eprintln!("[Job] Verification email failed: {}", e);
                            }
                        }
                    }

                    // --- Vector Generation ---
                    Job::GenerateEmbedding { tenant_id, collection_id, record_id, field_name, text_content, model } => {
                        if let Some((db, vector_provider)) = resolver.resolve(tenant_id.as_deref()).await {
                            match vector_provider.embed(&text_content).await {
                                Ok(vec) => {
                                    if let Err(e) = vector_provider.index(collection_id, record_id, &field_name, &vec).await {
                                        eprintln!("[Job] Failed to index vector: {}", e);
                                    }
                                    if let Err(e) = db.save_vector(collection_id, record_id, &field_name, vec, &model).await {
                                        eprintln!("[Job] Failed to persist vector: {}", e);
                                    }
                                },
                                Err(e) => {
                                    eprintln!("[Job] Failed to generate embedding: {}", e);
                                }
                            }
                        } else {
                            eprintln!("[Job] Failed to resolve context for tenant: {:?}", tenant_id);
                        }
                    }

                    // --- Search Indexing ---
                    Job::IndexRecord { tenant_id, collection_id, record_id, data, schema } => {
                        if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await {
                            if let Err(e) = db.index_record_search(collection_id, record_id, &data, &schema).await {
                                eprintln!("[Job] Search Indexing failed for {}: {}", record_id, e);
                            }
                        }
                    }

                    Job::DeleteFromIndex { tenant_id, collection_id, record_id } => {
                        if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await {
                            if let Err(e) = db.delete_record_search(collection_id, record_id).await {
                                eprintln!("[Job] Search Index Deletion failed for {}: {}", record_id, e);
                            }
                        }
                    }

                    _ => {} 
                }
            });
        }
    });

    JobQueue::new(tx)
}

// --- SMTP CONFIG STRUCT ---
#[derive(Debug, Deserialize)]
struct SmtpSettings {
    enabled: bool,
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>, 
    from_email: String,
}

// UPDATED: Public and returns Result
pub async fn send_email(db: Arc<dyn Db>, vault: Arc<Vault>, to: &str, subject: &str, body: &str) -> Result<(), String> {
    let settings_val = db.get_config("smtp").await.unwrap_or(None);
    
    let settings: SmtpSettings = if let Some(val) = settings_val {
        serde_json::from_value(val).unwrap_or_else(|_| SmtpSettings { 
            enabled: false, 
            host: "".into(), 
            port: 587, 
            username: None, 
            password: None, 
            from_email: "".into() 
        })
    } else {
        // Return error for script usage
        return Err("SMTP settings not configured".to_string());
    };
    
    if !settings.enabled {
        return Err("SMTP is disabled".to_string());
    }

    let decrypted_password = if let Some(encrypted_str) = settings.password {
        match serde_json::from_str::<EncryptedValue>(&encrypted_str) {
            Ok(enc_val) => match vault.decrypt(&enc_val) {
                Ok(pwd) => Some(pwd),
                Err(e) => return Err(format!("Failed to decrypt SMTP password: {}", e)),
            },
            Err(e) => return Err(format!("Failed to deserialize encrypted password: {}", e)),
        }
    } else {
        None
    };
    
    let from_address = format!("{} <{}>", settings.from_email, settings.from_email);

    let email = Message::builder()
        .from(from_address.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
        .to(to.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
        .subject(subject)
        .body(body.to_string())
        .map_err(|e: lettre::error::Error| e.to_string())?;

    let mailer = SmtpTransport::relay(&settings.host)
        .map_err(|e| e.to_string())?
        .port(settings.port)
        .credentials(Credentials::new(
            settings.username.unwrap_or_default(), 
            decrypted_password.unwrap_or_default()
        ))
        .build();

    match mailer.send(&email) {
        Ok(_) => {
            println!("[SMTP] Email sent to {}", to);
            Ok(())
        },
        Err(e) => {
            eprintln!("[SMTP] Could not send email: {}", e);
            Err(e.to_string())
        },
    }
}