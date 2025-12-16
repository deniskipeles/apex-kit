use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use lettre::{Message, SmtpTransport, Transport};
use lettre::transport::smtp::authentication::Credentials;
use std::sync::Arc;
use crate::{Db, VectorProvider, security::{ Vault, EncryptedValue }};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Job {
    SendWelcomeEmail { email: String, user_id: i64 },
    SendPasswordReset { email: String, token: String },
    SendVerification { email: String, token: String },
    ProcessImage { record_id: i64 },
    // --- Vectorization Job ---
    GenerateEmbedding { 
        collection_id: i64, 
        record_id: i64, 
        field_name: String, 
        text_content: String 
    },
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

// Updated to accept dependencies needed for processing
pub fn start_background_worker(
    db: Arc<dyn Db>, 
    vector_provider: Arc<dyn VectorProvider>,
    vault: Arc<Vault>,
) -> JobQueue {
    let (tx, mut rx) = mpsc::channel(100);

    tokio::spawn(async move {
        println!("Background worker started...");
        while let Some(job) = rx.recv().await {
            // Clone db for async job task
            let db_clone = db.clone(); 
            let vault_clone = vault.clone();
            match job {
                Job::SendWelcomeEmail { email, .. } => {
                    // Pass DB and Vault
                    send_email(db_clone, vault_clone, &email, "Welcome to TinyBase!", "Thanks for signing up!").await;
                }
                Job::SendPasswordReset { email, token } => {
                    let body = format!("Click here to reset your password: http://localhost:5000/reset-password?token={}", token);
                    send_email(db_clone, vault_clone, &email, "Reset Password", &body).await;
                }
                Job::SendVerification { email, token } => {
                    let body = format!("Verify your email: http://localhost:5000/verify?token={}", token);
                    send_email(db_clone, vault_clone, &email, "Verify Email", &body).await;
                }
                // --- Logic for Vector Generation ---
                Job::GenerateEmbedding { collection_id, record_id, field_name, text_content } => {
                    println!("[Job] Processing vector for {}.{} (Record {})", collection_id, field_name, record_id);
                    
                    // 1. Generate Embedding (Calls Candle or API)
                    match vector_provider.embed(&text_content).await {
                        Ok(vec) => {
                            // 2. Index the result (In-Memory HNSW)
                            if let Err(e) = vector_provider.index(collection_id, record_id, &field_name, &vec).await {
                                eprintln!("[Job] Failed to index vector to HNSW: {}", e);
                            } else {
                                println!("[Job] HNSW Index updated successfully.");
                            }
                            
                            // 3. PERSIST THE VECTOR TO DATABASE
                            if let Err(e) = db_clone.save_vector(collection_id, record_id, &field_name, vec).await {
                                eprintln!("[Job] Failed to persist vector to DB: {}", e);
                            } else {
                                println!("[Job] Vector persisted to DB successfully.");
                            }
                        },
                        Err(e) => {
                            eprintln!("[Job] Failed to generate embedding: {}", e);
                        }
                    }
                }
                _ => {} // Handle others
            }
        }
    });

    JobQueue::new(tx)
}

// --- SMTP CONFIG STRUCT (Minimal internal definition) ---
#[derive(Debug, Deserialize)]
struct SmtpSettings {
    enabled: bool,
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>, // This holds the encrypted string
    from_email: String,
}

async fn send_email(db: Arc<dyn Db>, vault: Arc<Vault>, to: &str, subject: &str, body: &str) {
    // 1. Fetch SMTP configuration from DB
    let settings_val = db.get_setting("smtp").await.unwrap_or(None);
    
    let settings: SmtpSettings = if let Some(val) = settings_val {
        // Fallback or default values needed here if deserialization fails
        serde_json::from_value(val).unwrap_or_else(|_| SmtpSettings { 
            enabled: false, 
            host: "".into(), 
            port: 587, 
            username: None, 
            password: None, 
            from_email: "".into() 
        })
    } else {
        println!("[Mock Email] SMTP not configured. To: {}, Subject: {}", to, subject);
        return;
    };
    
    if !settings.enabled {
        println!("[Mock Email] SMTP is disabled. To: {}, Subject: {}", to, subject);
        return;
    }

    // 2. Decrypt Password
    let decrypted_password = if let Some(encrypted_str) = settings.password {
        match serde_json::from_str::<EncryptedValue>(&encrypted_str) {
            Ok(enc_val) => match vault.decrypt(&enc_val) {
                Ok(pwd) => Some(pwd),
                Err(e) => {
                    eprintln!("Failed to decrypt SMTP password: {}", e);
                    return;
                }
            },
            Err(e) => {
                // If it's not a JSON object, assume it's an old raw password (unlikely but safe check)
                eprintln!("Failed to deserialize encrypted password: {}", e);
                None 
            }
        }
    } else {
        None
    };
    
    // 3. Configure Mailer
    let from_address = format!("{} <{}>", settings.from_email, settings.from_email);

    let email = Message::builder()
        .from(from_address.parse().unwrap())
        .to(to.parse().unwrap())
        .subject(subject)
        .body(body.to_string())
        .unwrap();

    let mailer = SmtpTransport::relay(&settings.host)
        .unwrap()
        .port(settings.port)
        .credentials(Credentials::new(
            settings.username.unwrap_or_default(), 
            decrypted_password.unwrap_or_default()
        ))
        .build();

    // 4. Send Email
    match mailer.send(&email) {
        Ok(_) => println!("[SMTP] Email sent to {}", to),
        Err(e) => eprintln!("[SMTP] Could not send email: {}", e),
    }
}