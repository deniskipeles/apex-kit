use axum::{
    extract::{State, Json},
    Extension,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tinybase_core::auth::Claims;
use crate::{AppState, AppError};
use utoipa::ToSchema;

// --- API Models ---

#[derive(Serialize, Deserialize, ToSchema, Default)]
pub struct AppSettingsDto {
    pub app_name: Option<String>,
    pub app_url: Option<String>,
    pub allow_public_registration: Option<bool>,
    pub theme: Option<String>,
    pub smtp: Option<SmtpConfigDto>,
    pub storage: Option<StorageConfigDto>,
    pub security: Option<SecurityConfigDto>,
    pub ai: Option<AiConfigDto>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Default)]
pub struct AiConfigDto {
    pub enabled: bool,
    pub provider: String, // "gemini"
    pub api_key: Option<String>, // Masked
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Default)]
pub struct SecurityConfigDto {
    pub cors_allow_all: bool, // If true, allow *
    pub cors_origins: String, // Comma-separated list: "https://app.com, http://localhost:3000"
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct SmtpConfigDto {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from_email: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct StorageConfigDto {
    pub active_driver: String,
    pub s3: S3ConfigDto,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct S3ConfigDto {
    pub enabled: bool,
    pub provider: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: Option<String>,
}

// --- Handlers ---

#[utoipa::path(
    get,
    path = "/api/v1/admin/settings",
    responses((status = 200, body = AppSettingsDto))
)]
pub async fn get_settings(
    auth: Option<Extension<Claims>>, // Changed from Extension(claims)
    State(state): State<AppState>,
) -> Result<Json<AppSettingsDto>, AppError> {
    // Check for Auth manually
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let general = state.db.get_setting("general").await.map_err(|e| AppError::UnknownError(e.to_string()))?.unwrap_or(json!({}));
    let smtp = state.db.get_setting("smtp").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let storage = state.db.get_setting("storage").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    let security = state.db.get_setting("security").await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    let mut response = AppSettingsDto {
        app_name: general.get("app_name").and_then(|v| v.as_str()).map(String::from),
        app_url: general.get("app_url").and_then(|v| v.as_str()).map(String::from),
        allow_public_registration: general.get("allow_public_registration").and_then(|v| v.as_bool()),
        theme: general.get("theme").and_then(|v| v.as_str()).map(String::from),
        smtp: None,
        storage: None,
        security: None,
    };

    if let Some(smtp_val) = smtp {
        let mut s: SmtpConfigDto = serde_json::from_value(smtp_val).unwrap_or_else(|_| SmtpConfigDto { enabled: false, host: "".into(), port: 587, username: None, password: None, from_email: "".into() });
        if s.password.is_some() && !s.password.as_ref().unwrap().is_empty() {
            s.password = Some("******".to_string());
        }
        response.smtp = Some(s);
    }

    if let Some(storage_val) = storage {
        let mut s: StorageConfigDto = serde_json::from_value(storage_val).unwrap_or_else(|_| StorageConfigDto { active_driver: "local".into(), s3: S3ConfigDto { enabled: false, provider: "aws".into(), bucket: "".into(), region: "".into(), endpoint: "".into(), access_key: "".into(), secret_key: None }});
        if s.s3.secret_key.is_some() && !s.s3.secret_key.as_ref().unwrap().is_empty() {
            s.s3.secret_key = Some("******".to_string());
        }
        response.storage = Some(s);
    }

    if let Some(sec_val) = security {
        let s: SecurityConfigDto = serde_json::from_value(sec_val)
            .unwrap_or_else(|_| SecurityConfigDto { cors_allow_all: true, cors_origins: "".into() });
        response.security = Some(s);
    } else {
        // Default to permissive if not set yet
        response.security = Some(SecurityConfigDto { cors_allow_all: true, cors_origins: "".into() });
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
    auth: Option<Extension<Claims>>, // Changed from Extension(claims)
    State(state): State<AppState>,
    Json(payload): Json<AppSettingsDto>,
) -> Result<Json<AppSettingsDto>, AppError> {
    // Check for Auth manually
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;

    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    // 1. Update General
    let mut general = state.db.get_setting("general").await.map_err(|e| AppError::UnknownError(e.to_string()))?.unwrap_or(json!({}));
    
    // FIX: Use references (&) so we don't consume 'payload'
    if let Some(v) = &payload.app_name { general["app_name"] = json!(v); }
    if let Some(v) = &payload.app_url { general["app_url"] = json!(v); }
    if let Some(v) = &payload.allow_public_registration { general["allow_public_registration"] = json!(v); }
    if let Some(v) = &payload.theme { general["theme"] = json!(v); }
    
    state.db.save_setting("general", general, false).await.map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 2. Update SMTP
    if let Some(new_smtp) = &payload.smtp { // Borrowing smtp from payload
        let existing_json = state.db.get_setting("smtp").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        let mut final_smtp = new_smtp.clone();

        if let Some(existing_val) = existing_json {
            if let Ok(existing_obj) = serde_json::from_value::<SmtpConfigDto>(existing_val) {
                if final_smtp.password.as_deref() == Some("******") {
                    final_smtp.password = existing_obj.password;
                } else if let Some(raw_pass) = &final_smtp.password {
                    let enc = state.vault.encrypt(raw_pass).map_err(AppError::UnknownError)?;
                    final_smtp.password = Some(serde_json::to_string(&enc).unwrap());
                }
            }
        } else if let Some(raw_pass) = &final_smtp.password {
             let enc = state.vault.encrypt(raw_pass).map_err(AppError::UnknownError)?;
             final_smtp.password = Some(serde_json::to_string(&enc).unwrap());
        }

        state.db.save_setting("smtp", serde_json::to_value(final_smtp).unwrap(), true).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 3. Update Storage
    if let Some(new_storage) = &payload.storage { // Borrowing storage from payload
        let existing_json = state.db.get_setting("storage").await.map_err(|e| AppError::UnknownError(e.to_string()))?;
        let mut final_storage = new_storage.clone();

        if let Some(existing_val) = existing_json {
            if let Ok(existing_obj) = serde_json::from_value::<StorageConfigDto>(existing_val) {
                if final_storage.s3.secret_key.as_deref() == Some("******") {
                    final_storage.s3.secret_key = existing_obj.s3.secret_key;
                } else if let Some(raw_key) = &final_storage.s3.secret_key {
                    let enc = state.vault.encrypt(raw_key).map_err(AppError::UnknownError)?;
                    final_storage.s3.secret_key = Some(serde_json::to_string(&enc).unwrap());
                }
            }
        } else if let Some(raw_key) = &final_storage.s3.secret_key {
             let enc = state.vault.encrypt(raw_key).map_err(AppError::UnknownError)?;
             final_storage.s3.secret_key = Some(serde_json::to_string(&enc).unwrap());
        }

        state.db.save_setting("storage", serde_json::to_value(final_storage).unwrap(), true).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 3. Cors
    if let Some(new_sec) = &payload.security {
        state.db.save_setting("security", serde_json::to_value(new_sec).unwrap(), false)
            .await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    Ok(Json(payload))
}