use axum::{
    extract::{State, Json},
    Extension,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use apexkit_core::auth::Claims;
use crate::{AppState, AppError, DatabaseConnection}; // Added DatabaseConnection
use utoipa::ToSchema;
use crate::BaseUrl;

// --- API Models (Unchanged) ---

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
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>, // Masked on GET
    pub from_email: String,
    // Templates
    pub template_welcome: Option<String>,   // "Welcome to {{app_name}}!"
    pub template_reset: Option<String>,     // "Click here to reset: {{link}}"
    pub template_verify: Option<String>,    // "Verify email: {{link}}"
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

#[derive(Serialize, Deserialize, ToSchema, Clone, Default)]
pub struct SecurityConfigDto {
    pub cors_allow_all: bool, 
    pub cors_origins: String, 
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
    pub schedule: String, // e.g. "0 0 * * *"
    pub retention: u32,   // Days
    pub destination: String, // "local" | "s3"
    #[serde(default)]
    pub include_uploads: bool,
    #[serde(default)]
    pub include_indexes: bool,
}

impl Default for BackupConfigDto {
    fn default() -> Self {
        Self {
            enabled: false,
            schedule: "0 0 * * *".to_string(),
            retention: 7,
            destination: "local".to_string(),
            include_uploads: false, // Default off to save space
            include_indexes: false, // Default off (can be rebuilt)
        }
    }
}

// --- Handlers ---

#[utoipa::path(
    get,
    path = "/api/v1/admin/settings",
    responses((status = 200, body = AppSettingsDto))
)]
pub async fn get_settings(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection, // <--- FIXED: Use Injected DB
    State(_state): State<AppState>, // Only need state if we need vault/etc not in DB
    BaseUrl(base_url): BaseUrl,
) -> Result<Json<AppSettingsDto>, AppError> {
    // 1. Auth Check
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // 2. Fetch all setting groups from the injected DB (Root or Tenant)
    let general = db.get_config("general").await.map_err(|e| AppError::UnknownError(e.to_string()))?.unwrap_or(json!({}));
    let smtp = db.get_config("smtp").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let storage = db.get_config("storage").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let security = db.get_config("security").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let cron_json = db.get_config("cron_jobs").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let ai = db.get_config("ai").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let backups_json = db.get_config("backups").await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOGIC] Resolve App URL: DB Config > Request Origin > Default
    let configured_url = general.get("app_url").and_then(|v| v.as_str()).map(String::from);
    let final_app_url = if configured_url.is_some() && !configured_url.as_ref().unwrap().is_empty() {
        configured_url
    } else {
        Some(base_url) // Defaults to http://localhost:PORT or https://domain.com based on request
    };

    // 3. Construct Response
    let mut response = AppSettingsDto {
        app_name: general.get("app_name").and_then(|v| v.as_str()).map(String::from),
        app_url: final_app_url, 
        allow_public_registration: general.get("allow_public_registration").and_then(|v| v.as_bool()),
        theme: general.get("theme").and_then(|v| v.as_str()).map(String::from),
        app_logo: general.get("app_logo").and_then(|v| v.as_str()).map(String::from),
        logo_width: general.get("logo_width").and_then(|v| v.as_str()).map(String::from),
        logo_height: general.get("logo_height").and_then(|v| v.as_str()).map(String::from),
        log_retention_days: general.get("log_retention_days").and_then(|v| v.as_u64()),
        max_site_size_mb: general.get("max_site_size_mb").and_then(|v| v.as_u64()),
        smtp: None,
        storage: None,
        security: None,
        cron_jobs: None,
        ai: None,
        backups: None,
    };

    // SMTP (Mask Password)
    if let Some(smtp_val) = smtp {
        let mut s: SmtpConfigDto = serde_json::from_value(smtp_val).unwrap_or_else(|_| SmtpConfigDto { 
            enabled: false, 
            host: "".into(), 
            port: 587, 
            username: None, 
            password: None, 
            from_email: "".into(),
            template_welcome: None,
            template_reset: None,
            template_verify: None
        });
        
        if s.password.is_some() && !s.password.as_ref().unwrap().is_empty() {
            s.password = Some("******".to_string());
        }
        response.smtp = Some(s);
    }

    // Storage (Mask Secret Key)
    if let Some(storage_val) = storage {
        let mut s: StorageConfigDto = serde_json::from_value(storage_val).unwrap_or_else(|_| StorageConfigDto { active_driver: "local".into(), s3: S3ConfigDto { enabled: false, provider: "aws".into(), bucket: "".into(), region: "".into(), endpoint: "".into(), access_key: "".into(), secret_key: None }});
        if s.s3.secret_key.is_some() && !s.s3.secret_key.as_ref().unwrap().is_empty() {
            s.s3.secret_key = Some("******".to_string());
        }
        response.storage = Some(s);
    }

    // Security
    if let Some(sec_val) = security {
        response.security = serde_json::from_value(sec_val).ok();
    } else {
        response.security = Some(SecurityConfigDto { cors_allow_all: true, cors_origins: "".into() });
    }

    // Cron Jobs
    if let Some(val) = cron_json {
        response.cron_jobs = serde_json::from_value(val).ok();
    } else {
        response.cron_jobs = Some(vec![]);
    }

    // AI (Mask API Key)
    if let Some(ai_val) = ai {
        let mut a: AiConfigDto = serde_json::from_value(ai_val).unwrap_or_default();
        if a.api_key.is_some() && !a.api_key.as_ref().unwrap().is_empty() {
            a.api_key = Some("******".to_string());
        }
        response.ai = Some(a);
    } else {
        response.ai = Some(AiConfigDto::default());
    }

    // Map Backups
    if let Some(val) = backups_json {
        response.backups = serde_json::from_value(val).ok();
    } else {
        response.backups = Some(BackupConfigDto::default());
    }

    Ok(Json(response))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/settings",
    request_body = AppSettingsDto,
    responses((status = 200, body = AppSettingsDto))
)]
pub async fn update_settings(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection, 
    State(state): State<AppState>, 
    Json(payload): Json<AppSettingsDto>,
) -> Result<Json<AppSettingsDto>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // 1. Update General
    let mut general = db.get_config("general").await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .unwrap_or(json!({}));

    //  Ensure it is an object. If corrupted/string, force reset to empty object.
    if !general.is_object() {
        general = json!({});
    }
    
    // Use references (&) to avoid consuming payload fields
    if let Some(v) = &payload.app_name { general["app_name"] = json!(v); }
    if let Some(v) = &payload.app_url { general["app_url"] = json!(v); }
    if let Some(v) = &payload.allow_public_registration { general["allow_public_registration"] = json!(v); }
    if let Some(v) = &payload.theme { general["theme"] = json!(v); }
    if let Some(v) = &payload.app_logo { general["app_logo"] = json!(v); }
    if let Some(v) = &payload.logo_width { general["logo_width"] = json!(v); }
    if let Some(v) = &payload.logo_height { general["logo_height"] = json!(v); }
    
    //  Ensure these are mapped too
    if let Some(v) = &payload.log_retention_days { general["log_retention_days"] = json!(v); }
    if let Some(v) = &payload.max_site_size_mb { general["max_site_size_mb"] = json!(v); }
    
    db.set_config("general", &general, false).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 2. Update SMTP (Encrypt Password if changed)
    if let Some(new_smtp) = &payload.smtp {
        let existing_json = db.get_config("smtp").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        let mut final_smtp = new_smtp.clone();

        if let Some(existing_val) = existing_json {
            if let Ok(existing_obj) = serde_json::from_value::<SmtpConfigDto>(existing_val) {
                if final_smtp.password.as_deref() == Some("******") {
                    final_smtp.password = existing_obj.password;
                } else if let Some(raw_pass) = &final_smtp.password {
                    if !raw_pass.is_empty() {
                        let enc = state.vault.encrypt(raw_pass).map_err(AppError::UnknownError)?;
                        final_smtp.password = Some(serde_json::to_string(&enc).unwrap());
                    }
                }
            }
        } else if let Some(raw_pass) = &final_smtp.password {
             if !raw_pass.is_empty() {
                let enc = state.vault.encrypt(raw_pass).map_err(AppError::UnknownError)?;
                final_smtp.password = Some(serde_json::to_string(&enc).unwrap());
             }
        }

        db.set_config("smtp", &serde_json::to_value(final_smtp).unwrap(), true).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 3. Update Storage (Encrypt Secret Key if changed)
    if let Some(new_storage) = &payload.storage {
        let existing_json = db.get_config("storage").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        let mut final_storage = new_storage.clone();

        if let Some(existing_val) = existing_json {
            if let Ok(existing_obj) = serde_json::from_value::<StorageConfigDto>(existing_val) {
                if final_storage.s3.secret_key.as_deref() == Some("******") {
                    final_storage.s3.secret_key = existing_obj.s3.secret_key;
                } else if let Some(raw_key) = &final_storage.s3.secret_key {
                    if !raw_key.is_empty() {
                        let enc = state.vault.encrypt(raw_key).map_err(AppError::UnknownError)?;
                        final_storage.s3.secret_key = Some(serde_json::to_string(&enc).unwrap());
                    }
                }
            }
        } else if let Some(raw_key) = &final_storage.s3.secret_key {
             if !raw_key.is_empty() {
                let enc = state.vault.encrypt(raw_key).map_err(AppError::UnknownError)?;
                final_storage.s3.secret_key = Some(serde_json::to_string(&enc).unwrap());
             }
        }

        db.set_config("storage", &serde_json::to_value(final_storage).unwrap(), true).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 4. Update Security (CORS)
    if let Some(new_sec) = &payload.security {
        db.set_config("security", &serde_json::to_value(new_sec).unwrap(), false)
            .await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 5. Update Cron Jobs
    if let Some(jobs) = &payload.cron_jobs {
        db.set_config("cron_jobs", &serde_json::to_value(jobs).unwrap(), false)
            .await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 6. Update AI (Encrypt API Key if changed)
    if let Some(new_ai) = &payload.ai {
        let existing_json = db.get_config("ai").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        let mut final_ai = new_ai.clone();

        if let Some(existing_val) = existing_json {
            if let Ok(existing_obj) = serde_json::from_value::<AiConfigDto>(existing_val) {
                if final_ai.api_key.as_deref() == Some("******") {
                    final_ai.api_key = existing_obj.api_key;
                } else if let Some(raw_key) = &final_ai.api_key {
                    if !raw_key.is_empty() {
                        let enc = state.vault.encrypt(raw_key).map_err(AppError::UnknownError)?;
                        final_ai.api_key = Some(serde_json::to_string(&enc).unwrap());
                    }
                }
            }
        } else if let Some(raw_key) = &final_ai.api_key {
             if !raw_key.is_empty() {
                let enc = state.vault.encrypt(raw_key).map_err(AppError::UnknownError)?;
                final_ai.api_key = Some(serde_json::to_string(&enc).unwrap());
             }
        }

        db.set_config("ai", &serde_json::to_value(final_ai).unwrap(), true).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 7. Update Backups
    if let Some(backups) = &payload.backups {
        db.set_config("backups", &serde_json::to_value(backups).unwrap(), false)
            .await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    Ok(Json(payload))
}

// DTO
#[derive(Serialize, ToSchema)]
pub struct AppNameResponse {
    pub app_name: String,
}

// Handler
#[utoipa::path(
    get,
    path = "/app-name",
    responses((status = 200, body = AppNameResponse))
)]
pub async fn get_public_app_name(
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<AppNameResponse>, AppError> {
    
    // 1. Fetch 'general' settings from DB (No auth required)
    let general = db.get_config("general").await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .unwrap_or(json!({}));

    // 2. Extract app_name or default
    let app_name = general.get("app_name")
        .and_then(|v| v.as_str())
        .unwrap_or("ApexKit App")
        .to_string();

    Ok(Json(AppNameResponse { app_name }))
}
