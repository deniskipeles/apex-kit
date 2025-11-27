// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/jobs.rs start here ===========================
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use lettre::{Message, SmtpTransport, Transport};
use lettre::transport::smtp::authentication::Credentials;
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Job {
    SendWelcomeEmail { email: String, user_id: i64 },
    SendPasswordReset { email: String, token: String },
    SendVerification { email: String, token: String },
    ProcessImage { record_id: i64 },
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

pub fn start_background_worker() -> JobQueue {
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
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/jobs.rs ends here ===========================