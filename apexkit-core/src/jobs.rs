use crate::{
    Db, VectorProvider,
    schema::CollectionSchema,
    security::{EncryptedValue, Vault},
};
use async_trait::async_trait;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SendmailTransport, Transport};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

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

    // --- Vectorization Job ---
    GenerateEmbedding {
        tenant_id: Option<String>,
        collection_id: i64,
        record_id: i64,
        field_name: String,
        content: String, // Either text, Base64 image (data:image/...), or a filename
        content_type: String, // "text" or "file"
        model: String,
    },

    // --- Async Search Indexing ---
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
}

// New Trait to resolve DB context dynamically
#[async_trait]
pub trait JobContext: Send + Sync {
    async fn resolve(
        &self,
        tenant_id: Option<&str>,
    ) -> Option<(Arc<dyn Db>, Arc<dyn VectorProvider>)>;
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
                    Job::SendWelcomeEmail {
                        tenant_id,
                        email,
                        user_id: _,
                    } => {
                        if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await {
                            let gen_val = db.get_config("general").await.unwrap_or(None);
                            let app_name = gen_val
                                .as_ref()
                                .and_then(|v| v.get("app_name").and_then(|s| s.as_str()))
                                .unwrap_or("ApexKit")
                                .to_string();

                            let smtp_val = db.get_config("smtp").await.unwrap_or(None);
                            let smtp: SmtpSettings = smtp_val
                                .map(|v| serde_json::from_value(v).unwrap_or_default())
                                .unwrap_or_default();

                            let mut body = smtp
                                .template_welcome
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| "Thanks for signing up!".to_string());
                            body = body.replace("{{app_name}}", &app_name);
                            body = body.replace("{{email}}", &email);

                            let subject = format!("Welcome to {}!", app_name);

                            if let Err(e) =
                                send_email(db, vault_clone, &email, &subject, &body).await
                            {
                                eprintln!("[Job] Welcome email failed: {}", e);
                            }
                        }
                    }
                    Job::SendPasswordReset {
                        tenant_id,
                        email,
                        token,
                    } => {
                        if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await {
                            let gen_val = db.get_config("general").await.unwrap_or(None);
                            let app_name = gen_val
                                .as_ref()
                                .and_then(|v| v.get("app_name").and_then(|s| s.as_str()))
                                .unwrap_or("ApexKit")
                                .to_string();
                            let app_url = gen_val
                                .as_ref()
                                .and_then(|v| v.get("app_url").and_then(|s| s.as_str()))
                                .unwrap_or("http://localhost:5000")
                                .to_string();

                            let smtp_val = db.get_config("smtp").await.unwrap_or(None);
                            let smtp: SmtpSettings = smtp_val
                                .map(|v| serde_json::from_value(v).unwrap_or_default())
                                .unwrap_or_default();

                            let link = format!(
                                "{}/_dashboard/login?token={}",
                                app_url.trim_end_matches('/'),
                                token
                            );
                            let mut body = smtp
                                .template_reset
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| format!("Reset: {}", link));
                            body = body.replace("{{app_name}}", &app_name);
                            body = body.replace("{{email}}", &email);
                            body = body.replace("{{link}}", &link);
                            body = body.replace("{{token}}", &token);

                            let subject = format!("Reset your password for {}", app_name);

                            if let Err(e) =
                                send_email(db, vault_clone, &email, &subject, &body).await
                            {
                                eprintln!("[Job] Reset password email failed: {}", e);
                            }
                        }
                    }
                    Job::SendVerification {
                        tenant_id,
                        email,
                        token,
                    } => {
                        if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await {
                            let gen_val = db.get_config("general").await.unwrap_or(None);
                            let app_name = gen_val
                                .as_ref()
                                .and_then(|v| v.get("app_name").and_then(|s| s.as_str()))
                                .unwrap_or("ApexKit")
                                .to_string();
                            let app_url = gen_val
                                .as_ref()
                                .and_then(|v| v.get("app_url").and_then(|s| s.as_str()))
                                .unwrap_or("http://localhost:5000")
                                .to_string();

                            let smtp_val = db.get_config("smtp").await.unwrap_or(None);
                            let smtp: SmtpSettings = smtp_val
                                .map(|v| serde_json::from_value(v).unwrap_or_default())
                                .unwrap_or_default();

                            let link = format!(
                                "{}/api/v1/auth/verify?token={}",
                                app_url.trim_end_matches('/'),
                                token
                            );
                            let mut body = smtp
                                .template_verify
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| format!("Verify: {}", link));
                            body = body.replace("{{app_name}}", &app_name);
                            body = body.replace("{{email}}", &email);
                            body = body.replace("{{link}}", &link);
                            body = body.replace("{{token}}", &token);

                            let subject = format!("Verify your email for {}", app_name);

                            if let Err(e) =
                                send_email(db, vault_clone, &email, &subject, &body).await
                            {
                                eprintln!("[Job] Verification email failed: {}", e);
                            }
                        }
                    }

                    // --- Vector Generation ---
                    Job::GenerateEmbedding {
                        tenant_id,
                        collection_id,
                        record_id,
                        field_name,
                        content,
                        content_type,
                        model,
                    } => {
                        if let Some((db, vector_provider)) =
                            resolver.resolve(tenant_id.as_deref()).await
                        {
                            let vec_res = if content_type == "file" {
                                // Resolve path based on tenant
                                let fs_root = match tenant_id.as_deref() {
                                    Some(id) if id.starts_with("session_") => {
                                        format!("storage/sandboxes/{}/uploads", id)
                                    }
                                    Some(id) => format!("storage/tenants/{}/uploads", id),
                                    None => "./storage/system/uploads".to_string(),
                                };

                                let file_path = std::path::Path::new(&fs_root).join(&content);

                                if let Ok(bytes) = tokio::fs::read(&file_path).await {
                                    let ext = file_path
                                        .extension()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    if ["jpg", "jpeg", "png", "webp", "gif"].contains(&ext.as_str())
                                    {
                                        use base64::{
                                            Engine as _, engine::general_purpose::STANDARD,
                                        };
                                        let b64 = STANDARD.encode(&bytes);
                                        vector_provider.embed_image(&b64).await
                                    } else {
                                        Err("Only image files are currently supported for vectorization.".into())
                                    }
                                } else {
                                    Err(format!(
                                        "File {} not found on disk for vectorization.",
                                        content
                                    ))
                                }
                            } else if content.starts_with("data:image/") {
                                vector_provider.embed_image(&content).await
                            } else {
                                vector_provider.embed(&content).await
                            };

                            match vec_res {
                                Ok(vec) => {
                                    if let Err(e) = vector_provider
                                        .index(collection_id, record_id, &field_name, &vec)
                                        .await
                                    {
                                        eprintln!("[Job] Failed to index vector: {}", e);
                                    }
                                    if let Err(e) = db
                                        .save_vector(
                                            collection_id,
                                            record_id,
                                            &field_name,
                                            vec,
                                            &model,
                                        )
                                        .await
                                    {
                                        eprintln!("[Job] Failed to persist vector: {}", e);
                                    }
                                    println!(
                                        "[Job] Successfully vectorized {} for record {}",
                                        field_name, record_id
                                    );
                                }
                                Err(e) => {
                                    eprintln!("[Job] Failed to generate embedding: {}", e);
                                }
                            }
                        } else {
                            eprintln!(
                                "[Job] Failed to resolve context for tenant: {:?}",
                                tenant_id
                            );
                        }
                    }

                    // --- Search Indexing ---
                    Job::IndexRecord {
                        tenant_id,
                        collection_id,
                        record_id,
                        data,
                        schema,
                    } => {
                        if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await
                            && let Err(e) = db
                                .index_record_search(collection_id, record_id, &data, &schema)
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
                        if let Some((db, _)) = resolver.resolve(tenant_id.as_deref()).await
                            && let Err(e) = db.delete_record_search(collection_id, record_id).await
                        {
                            eprintln!(
                                "[Job] Search Index Deletion failed for {}: {}",
                                record_id, e
                            );
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
#[derive(Debug, Deserialize, Default)]
struct SmtpSettings {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    block_smtp: bool, // <--- NEW: Root override to block emails entirely
    #[serde(default)]
    host: String,
    #[serde(default)]
    port: u16,
    username: Option<String>,
    password: Option<String>,
    #[serde(default)]
    from_email: String,
    template_welcome: Option<String>,
    template_reset: Option<String>,
    template_verify: Option<String>,
}

//  Public and returns Result
pub async fn send_email(
    db: Arc<dyn Db>,
    vault: Arc<Vault>,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let settings_val = db.get_config("smtp").await.unwrap_or(None);

    // Default: SMTP disabled, try sendmail
    let settings: SmtpSettings = if let Some(val) = settings_val {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        SmtpSettings::default()
    };

    // --- ENFORCE BLOCK ---
    if settings.block_smtp {
        eprintln!("[SMTP] BLOCKED: Outbound email blocked by global 'block_smtp' policy.");
        return Err("Outbound email is currently blocked by system policy.".to_string());
    }

    // Safer From Address Construction
    let from_address = if settings.from_email.is_empty() {
        "noreply@localhost".to_string()
    } else {
        // Just use the email directly to avoid formatting issues with names
        settings.from_email.clone()
    };

    let email = Message::builder()
        .from(
            from_address
                .parse()
                .map_err(|e: lettre::address::AddressError| e.to_string())?,
        )
        .to(to
            .parse()
            .map_err(|e: lettre::address::AddressError| e.to_string())?)
        .subject(subject)
        .body(body.to_string())
        .map_err(|e: lettre::error::Error| e.to_string())?;

    // Logic: If SMTP enabled, use SMTP. Else, fallback to Sendmail.
    if settings.enabled && !settings.host.is_empty() {
        let decrypted_password = if let Some(encrypted_str) = settings.password {
            // ... decrypt logic ...
            match serde_json::from_str::<EncryptedValue>(&encrypted_str) {
                Ok(enc_val) => vault.decrypt(&enc_val).ok(),
                Err(_) => None,
            }
        } else {
            None
        };

        let tls_params = lettre::transport::smtp::client::TlsParameters::new(settings.host.clone())
            .map_err(|e| e.to_string())?;

        let mut builder = lettre::transport::smtp::SmtpTransport::builder_dangerous(&settings.host)
            .port(settings.port);

        // Intelligently select TLS mode based on the port
        builder = match settings.port {
            465 => builder.tls(lettre::transport::smtp::client::Tls::Wrapper(tls_params)),
            25 | 2525 | 1025 => builder.tls(lettre::transport::smtp::client::Tls::None),
            _ => builder.tls(lettre::transport::smtp::client::Tls::Opportunistic(
                tls_params,
            )),
        };

        if let Some(user) = settings.username {
            builder = builder.credentials(Credentials::new(
                user,
                decrypted_password.unwrap_or_default(),
            ));
        }

        let mailer = builder.build();

        match mailer.send(&email) {
            Ok(_) => {
                println!("[SMTP] Email sent to {}", to);
                Ok(())
            }
            Err(e) => {
                eprintln!("[SMTP] Failed: {}", e);
                Err(e.to_string())
            }
        }
    } else {
        // Fallback to local Sendmail
        let mailer = SendmailTransport::new();
        match mailer.send(&email) {
            Ok(_) => {
                println!("[Sendmail] Email sent to {}", to);
                Ok(())
            }
            Err(e) => {
                eprintln!("[Sendmail] Failed. Ensure 'sendmail' is installed: {}", e);
                Err(e.to_string())
            }
        }
    }
}
