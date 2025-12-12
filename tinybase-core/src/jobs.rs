// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/jobs.rs ===========================
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use lettre::{Message, SmtpTransport, Transport};
use lettre::transport::smtp::authentication::Credentials;
use std::env;
use std::sync::Arc;
use crate::{Db, VectorProvider};

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
    _db: Arc<dyn Db>, // Kept for future DB access (e.g. email settings)
    vector_provider: Arc<dyn VectorProvider>
) -> JobQueue {
    let (tx, mut rx) = mpsc::channel(100);

    tokio::spawn(async move {
        println!("Background worker started...");
        while let Some(job) = rx.recv().await {
            match job {
                Job::SendWelcomeEmail { email, .. } => {
                    send_email(&email, "Welcome to TinyBase!", "Thanks for signing up!").await;
                }
                Job::SendPasswordReset { email, token } => {
                    let body = format!("Click here to reset your password: http://localhost:5000/reset-password?token={}", token);
                    send_email(&email, "Reset Password", &body).await;
                }
                Job::SendVerification { email, token } => {
                    let body = format!("Verify your email: http://localhost:5000/verify?token={}", token);
                    send_email(&email, "Verify Email", &body).await;
                }
                // --- Logic for Vector Generation ---
                Job::GenerateEmbedding { collection_id, record_id, field_name, text_content } => {
                    println!("[Job] Processing vector for {}.{} (Record {})", collection_id, field_name, record_id);
                    
                    // 1. Generate Embedding (Calls Candle or API)
                    match vector_provider.embed(&text_content).await {
                        Ok(vec) => {
                            // 2. Index the result (In-Memory HNSW + Persistence)
                            if let Err(e) = vector_provider.index(collection_id, record_id, &field_name, &vec).await {
                                eprintln!("[Job] Failed to index vector: {}", e);
                            } else {
                                println!("[Job] Vector indexed successfully.");
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

async fn send_email(to: &str, subject: &str, body: &str) {
    // Check if SMTP is configured, otherwise mock
    let host = env::var("SMTP_HOST").unwrap_or_default();
    if host.is_empty() {
        println!("[Mock Email] To: {}, Subject: {}\nBody: {}", to, subject, body);
        return;
    }

    let username = env::var("SMTP_USER").unwrap_or_default();
    let password = env::var("SMTP_PASS").unwrap_or_default();

    let email = Message::builder()
        .from("TinyBase <no-reply@tinybase.io>".parse().unwrap())
        .to(to.parse().unwrap())
        .subject(subject)
        .body(body.to_string())
        .unwrap();

    let creds = Credentials::new(username, password);

    // Open connection (using Relay for simplicity, in prod use proper security settings)
    let mailer = SmtpTransport::relay(&host)
        .unwrap()
        .credentials(creds)
        .build();

    match mailer.send(&email) {
        Ok(_) => println!("Email sent to {}", to),
        Err(e) => eprintln!("Could not send email: {}", e),
    }
}