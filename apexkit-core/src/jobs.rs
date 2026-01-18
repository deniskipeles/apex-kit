use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use lettre::{Message, SmtpTransport, Transport, SendmailTransport};
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

//  Public and returns Result
pub async fn send_email(db: Arc<dyn Db>, vault: Arc<Vault>, to: &str, subject: &str, body: &str) -> Result<(), String> {
    let settings_val = db.get_config("smtp").await.unwrap_or(None);
    
    // Default: SMTP disabled, try sendmail
    let settings: SmtpSettings = if let Some(val) = settings_val {
        serde_json::from_value(val).unwrap_or_else(|_| SmtpSettings { 
            enabled: false, host: "".into(), port: 587, username: None, password: None, from_email: "noreply@localhost".into() 
        })
    } else {
        SmtpSettings { enabled: false, host: "".into(), port: 587, username: None, password: None, from_email: "noreply@localhost".into() }
    };

    // [FIX] Safer From Address Construction
    let from_address = if settings.from_email.is_empty() {
        "noreply@localhost".to_string()
    } else {
        // Just use the email directly to avoid formatting issues with names
        settings.from_email.clone()
    };

    let email = Message::builder()
        .from(from_address.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
        .to(to.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
        .subject(subject)
        .body(body.to_string())
        .map_err(|e: lettre::error::Error| e.to_string())?;

    // [FIX] Logic: If SMTP enabled, use SMTP. Else, fallback to Sendmail.
    if settings.enabled && !settings.host.is_empty() {
        let decrypted_password = if let Some(encrypted_str) = settings.password {
             // ... decrypt logic ...
             match serde_json::from_str::<EncryptedValue>(&encrypted_str) {
                Ok(enc_val) => match vault.decrypt(&enc_val) {
                    Ok(pwd) => Some(pwd),
                    Err(_) => None,
                },
                Err(_) => None,
            }
        } else { None };

        let mut builder = SmtpTransport::relay(&settings.host)
            .map_err(|e| e.to_string())?
            .port(settings.port);

        if let Some(user) = settings.username {
             builder = builder.credentials(Credentials::new(user, decrypted_password.unwrap_or_default()));
        }

        let mailer = builder.build();
        
        match mailer.send(&email) {
            Ok(_) => { println!("[SMTP] Email sent to {}", to); Ok(()) },
            Err(e) => { eprintln!("[SMTP] Failed: {}", e); Err(e.to_string()) }
        }
    } else {
        // Fallback to local Sendmail
        let mailer = SendmailTransport::new();
        match mailer.send(&email) {
            Ok(_) => { println!("[Sendmail] Email sent to {}", to); Ok(()) },
            Err(e) => { eprintln!("[Sendmail] Failed. Ensure 'sendmail' is installed: {}", e); Err(e.to_string()) }
        }
    }
}