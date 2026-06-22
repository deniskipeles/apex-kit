use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SendmailTransport, Transport};
use serde::Deserialize;
use std::sync::Arc;

use super::super::queue::JobContext;
use crate::database::traits::Db;
use crate::security::vault::EncryptedValue;
use crate::security::vault::Vault;

#[derive(Debug, Deserialize, Default)]
struct SmtpSettings {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    block_smtp: bool,
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

pub async fn send_email(
    db: Arc<dyn Db>,
    vault: Arc<Vault>,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let settings_val = db.get_config("smtp").await.unwrap_or(None);
    let settings: SmtpSettings = if let Some(val) = settings_val {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        SmtpSettings::default()
    };

    if settings.block_smtp {
        return Err("Outbound email is currently blocked by system policy.".to_string());
    }

    let from_address = if settings.from_email.is_empty() {
        "noreply@localhost".to_string()
    } else {
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

    if settings.enabled && !settings.host.is_empty() {
        let decrypted_password = if let Some(encrypted_str) = settings.password {
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
        mailer.send(&email).map(|_| ()).map_err(|e| e.to_string())
    } else {
        let mailer = SendmailTransport::new();
        mailer.send(&email).map(|_| ()).map_err(|e| e.to_string())
    }
}

pub async fn handle_welcome_email(
    resolver: Arc<dyn JobContext>,
    vault: Arc<Vault>,
    tenant_id: Option<String>,
    email: String,
    _user_id: i64,
) -> Result<(), String> {
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
        send_email(db, vault, &email, &subject, &body).await?;
    }
    Ok(())
}

pub async fn handle_password_reset(
    resolver: Arc<dyn JobContext>,
    vault: Arc<Vault>,
    tenant_id: Option<String>,
    email: String,
    token: String,
) -> Result<(), String> {
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
        send_email(db, vault, &email, &subject, &body).await?;
    }
    Ok(())
}

pub async fn handle_verification_email(
    resolver: Arc<dyn JobContext>,
    vault: Arc<Vault>,
    tenant_id: Option<String>,
    email: String,
    token: String,
) -> Result<(), String> {
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
        send_email(db, vault, &email, &subject, &body).await?;
    }
    Ok(())
}
