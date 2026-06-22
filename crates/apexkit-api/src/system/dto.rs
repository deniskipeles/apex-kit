// Added DatabaseConnection
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, Default)]
pub struct AppSettingsDto {
    pub app_name: Option<String>,
    pub app_url: Option<String>,
    pub allow_public_registration: Option<bool>,
    pub theme: Option<String>,
    pub smtp: Option<SmtpConfigDto>,
    pub storage: Option<StorageConfigDto>,
    pub security: Option<SecurityConfigDto>,
    pub cron_jobs: Option<Vec<apexkit_core::models::CronJob>>,
    pub ai: Option<AiConfigDto>,
    pub app_logo: Option<String>,
    pub logo_width: Option<String>,
    pub logo_height: Option<String>,
    pub log_retention_days: Option<u64>,
    pub max_site_size_mb: Option<u64>,
    pub backups: Option<BackupConfigDto>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct SmtpConfigDto {
    pub enabled: bool,
    pub block_smtp: Option<bool>, // <--- NEW
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>, // Masked on GET
    pub from_email: String,
    // Templates
    pub template_welcome: Option<String>, // "Welcome to {{app_name}}!"
    pub template_reset: Option<String>,   // "Click here to reset: {{link}}"
    pub template_verify: Option<String>,  // "Verify email: {{link}}"
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Default)]
pub struct StorageConfigDto {
    pub active_driver: String,
    pub s3: S3ConfigDto,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Default)]
pub struct S3ConfigDto {
    pub enabled: bool,
    pub provider: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: Option<String>, // Masked on GET
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct SecurityConfigDto {
    pub cors_allow_all: bool,
    pub cors_origins: String,
    pub tenant_transparency: bool,
    // [NEW] Rate Limits
    pub global_rate_limit: Option<u64>,
    pub tenant_free_rate_limit: Option<u64>,
    pub tenant_pro_rate_limit: Option<u64>,
}

impl Default for SecurityConfigDto {
    fn default() -> Self {
        Self {
            cors_allow_all: true,
            cors_origins: "".into(),
            tenant_transparency: false,
            global_rate_limit: Some(600),
            tenant_free_rate_limit: Some(120),
            tenant_pro_rate_limit: Some(3000),
        }
    }
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Default)]
pub struct AiConfigDto {
    pub enabled: bool,
    pub provider: String,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub api_key: Option<String>, // Masked on GET
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct BackupConfigDto {
    pub enabled: bool,
    pub schedule: String,    // e.g. "0 0 * * *"
    pub retention: u32,      // Days
    pub destination: String, // "local" | "s3"

    // --- [NEW] Granular Backup Options ---
    #[serde(default = "default_true")]
    pub include_databases: bool,
    #[serde(default)]
    pub include_vectors: bool,
    #[serde(default)]
    pub include_uploads: bool,
    #[serde(default)]
    pub include_indexes: bool,
    #[serde(default)]
    pub include_static_site: bool,
}

fn default_true() -> bool {
    true
}

impl Default for BackupConfigDto {
    fn default() -> Self {
        Self {
            enabled: false,
            schedule: "0 0 * * *".to_string(),
            retention: 7,
            destination: "local".to_string(),
            include_databases: true,
            include_vectors: false, // Default off (can be large)
            include_uploads: false, // Default off (can be large)
            include_indexes: false, // Default off (can be rebuilt)
            include_static_site: false,
        }
    }
}
