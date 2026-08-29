use crate::BaseUrl;
use crate::{AppError, AppState, DatabaseConnection}; // Added DatabaseConnection
use apexkit_core::auth::Claims;
use axum::{
    Extension,
    extract::{Json, State},
};
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use crate::system::dto::{
    AiConfigDto, AppSettingsDto, BackupConfigDto, S3ConfigDto, SecurityConfigDto, SmtpConfigDto,
    StorageConfigDto,
};

// --- Handlers ---

#[utoipa::path(
    get,
    path = "/api/v1/admin/settings",
    responses((status = 200, body = AppSettingsDto))
)]
pub async fn get_settings(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection, // <--- FIXED: Use Injected DB
    State(_state): State<AppState>,             // Only need state if we need vault/etc not in DB
    BaseUrl(base_url): BaseUrl,
) -> Result<Json<AppSettingsDto>, AppError> {
    // 1. Auth Check
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // 2. Fetch all setting groups from the injected DB (Root or Tenant)
    let general = db
        .get_config("general")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .unwrap_or(json!({}));
    let smtp = db
        .get_config("smtp")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let storage = db
        .get_config("storage")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let security = db
        .get_config("security")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let cron_json = db
        .get_config("cron_jobs")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let ai = db
        .get_config("ai")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let backups_json = db
        .get_config("backups")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // [LOGIC] Resolve App URL: DB Config > Request Origin > Default
    let configured_url = general
        .get("app_url")
        .and_then(|v| v.as_str())
        .map(String::from);
    let final_app_url = if configured_url.is_some() && !configured_url.as_ref().unwrap().is_empty()
    {
        configured_url
    } else {
        Some(base_url) // Defaults to http://localhost:PORT or https://domain.com based on request
    };

    // 3. Construct Response
    let mut response = AppSettingsDto {
        app_name: general
            .get("app_name")
            .and_then(|v| v.as_str())
            .map(String::from),
        app_url: final_app_url,
        // Always return Some(bool), defaulting to false
        allow_public_registration: Some(
            general
                .get("allow_public_registration")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        ),
        theme: general
            .get("theme")
            .and_then(|v| v.as_str())
            .map(String::from),
        app_logo: general
            .get("app_logo")
            .and_then(|v| v.as_str())
            .map(String::from),
        logo_width: general
            .get("logo_width")
            .and_then(|v| v.as_str())
            .map(String::from),
        logo_height: general
            .get("logo_height")
            .and_then(|v| v.as_str())
            .map(String::from),
        log_retention_days: general.get("log_retention_days").and_then(|v| v.as_u64()),
        max_site_size_mb: general.get("max_site_size_mb").and_then(|v| v.as_u64()),
        max_sandbox_storage_mb: general
            .get("max_sandbox_storage_mb")
            .and_then(|v| v.as_u64()),
        max_sandbox_vectors: general.get("max_sandbox_vectors").and_then(|v| v.as_i64()),
        max_sandbox_ai_requests: general
            .get("max_sandbox_ai_requests")
            .and_then(|v| v.as_i64()),
        smtp: None,
        storage: None,
        security: None,
        cron_jobs: None,
        ai: None,
        backups: None,
    };

    // SMTP (Mask Password)
    if let Some(smtp_val) = smtp {
        let mut s: SmtpConfigDto =
            serde_json::from_value(smtp_val).unwrap_or_else(|_| SmtpConfigDto {
                enabled: false,
                block_smtp: Some(false),
                host: "".into(),
                port: 587,
                username: None,
                password: None,
                from_email: "".into(),
                template_welcome: None,
                template_reset: None,
                template_verify: None,
            });

        if s.password.is_some() && !s.password.as_ref().unwrap().is_empty() {
            s.password = Some("******".to_string());
        }
        response.smtp = Some(s);
    }

    // Storage (Mask Secret Key)
    if let Some(storage_val) = storage {
        let mut s: StorageConfigDto =
            serde_json::from_value(storage_val).unwrap_or_else(|_| StorageConfigDto {
                active_driver: "local".into(),
                s3: S3ConfigDto {
                    enabled: false,
                    provider: "aws".into(),
                    bucket: "".into(),
                    region: "".into(),
                    endpoint: "".into(),
                    access_key: "".into(),
                    secret_key: None,
                },
            });
        if s.s3.secret_key.is_some() && !s.s3.secret_key.as_ref().unwrap().is_empty() {
            s.s3.secret_key = Some("******".to_string());
        }
        response.storage = Some(s);
    }

    // Security
    if let Some(sec_val) = security {
        response.security = serde_json::from_value(sec_val).ok();
    } else {
        response.security = Some(SecurityConfigDto {
            cors_allow_all: true,
            cors_origins: "".into(),
            tenant_transparency: false,
            global_rate_limit: Some(600),
            tenant_free_rate_limit: Some(120),
            tenant_pro_rate_limit: Some(3000),
        });
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
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    // 1. Update General
    let mut general = db
        .get_config("general")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .unwrap_or(json!({}));

    //  Ensure it is an object. If corrupted/string, force reset to empty object.
    if !general.is_object() {
        general = json!({});
    }

    // Use references (&) to avoid consuming payload fields
    if let Some(v) = &payload.app_name {
        general["app_name"] = json!(v);
    }
    if let Some(v) = &payload.app_url {
        general["app_url"] = json!(v);
    }
    if let Some(v) = &payload.allow_public_registration {
        general["allow_public_registration"] = json!(v);
    }
    if let Some(v) = &payload.theme {
        general["theme"] = json!(v);
    }
    if let Some(v) = &payload.app_logo {
        general["app_logo"] = json!(v);
    }
    if let Some(v) = &payload.logo_width {
        general["logo_width"] = json!(v);
    }
    if let Some(v) = &payload.logo_height {
        general["logo_height"] = json!(v);
    }

    //  Ensure these are mapped too
    if let Some(v) = &payload.log_retention_days {
        general["log_retention_days"] = json!(v);
    }
    if let Some(v) = &payload.max_site_size_mb {
        general["max_site_size_mb"] = json!(v);
    }

    if let Some(v) = &payload.max_sandbox_storage_mb {
        general["max_sandbox_storage_mb"] = json!(v);
    }

    if let Some(v) = &payload.max_sandbox_vectors {
        general["max_sandbox_vectors"] = json!(v);
    }
    if let Some(v) = &payload.max_sandbox_ai_requests {
        general["max_sandbox_ai_requests"] = json!(v);
    }

    db.set_config("general", &general, false)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    // 2. Update SMTP (Encrypt Password if changed)
    if let Some(new_smtp) = &payload.smtp {
        let existing_json = db
            .get_config("smtp")
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
        let mut final_smtp = new_smtp.clone();

        if let Some(existing_val) = existing_json {
            if let Ok(existing_obj) = serde_json::from_value::<SmtpConfigDto>(existing_val) {
                if final_smtp.password.as_deref() == Some("******") {
                    final_smtp.password = existing_obj.password;
                } else if let Some(raw_pass) = &final_smtp.password
                    && !raw_pass.is_empty()
                {
                    let enc = state
                        .vault
                        .encrypt(raw_pass)
                        .map_err(AppError::UnknownError)?;
                    final_smtp.password = Some(serde_json::to_string(&enc).unwrap());
                }
            }
        } else if let Some(raw_pass) = &final_smtp.password
            && !raw_pass.is_empty()
        {
            let enc = state
                .vault
                .encrypt(raw_pass)
                .map_err(AppError::UnknownError)?;
            final_smtp.password = Some(serde_json::to_string(&enc).unwrap());
        }

        db.set_config("smtp", &serde_json::to_value(final_smtp).unwrap(), true)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 3. Update Storage (Encrypt Secret Key if changed)
    if let Some(new_storage) = &payload.storage {
        let existing_json = db
            .get_config("storage")
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
        let mut final_storage = new_storage.clone();

        if let Some(existing_val) = existing_json {
            if let Ok(existing_obj) = serde_json::from_value::<StorageConfigDto>(existing_val) {
                if final_storage.s3.secret_key.as_deref() == Some("******") {
                    final_storage.s3.secret_key = existing_obj.s3.secret_key;
                } else if let Some(raw_key) = &final_storage.s3.secret_key
                    && !raw_key.is_empty()
                {
                    let enc = state
                        .vault
                        .encrypt(raw_key)
                        .map_err(AppError::UnknownError)?;
                    final_storage.s3.secret_key = Some(serde_json::to_string(&enc).unwrap());
                }
            }
        } else if let Some(raw_key) = &final_storage.s3.secret_key
            && !raw_key.is_empty()
        {
            let enc = state
                .vault
                .encrypt(raw_key)
                .map_err(AppError::UnknownError)?;
            final_storage.s3.secret_key = Some(serde_json::to_string(&enc).unwrap());
        }

        db.set_config(
            "storage",
            &serde_json::to_value(final_storage).unwrap(),
            true,
        )
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 4. Update Security (CORS & Rate Limits)
    if let Some(new_sec) = &payload.security {
        db.set_config("security", &serde_json::to_value(new_sec).unwrap(), false)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 5. Update Cron Jobs
    if let Some(jobs) = &payload.cron_jobs {
        db.set_config("cron_jobs", &serde_json::to_value(jobs).unwrap(), false)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 6. Update AI (Encrypt API Key if changed)
    if let Some(new_ai) = &payload.ai {
        let existing_json = db
            .get_config("ai")
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
        let mut final_ai = new_ai.clone();

        if let Some(existing_val) = existing_json {
            if let Ok(existing_obj) = serde_json::from_value::<AiConfigDto>(existing_val) {
                if final_ai.api_key.as_deref() == Some("******") {
                    final_ai.api_key = existing_obj.api_key;
                } else if let Some(raw_key) = &final_ai.api_key
                    && !raw_key.is_empty()
                {
                    let enc = state
                        .vault
                        .encrypt(raw_key)
                        .map_err(AppError::UnknownError)?;
                    final_ai.api_key = Some(serde_json::to_string(&enc).unwrap());
                }
            }
        } else if let Some(raw_key) = &final_ai.api_key
            && !raw_key.is_empty()
        {
            let enc = state
                .vault
                .encrypt(raw_key)
                .map_err(AppError::UnknownError)?;
            final_ai.api_key = Some(serde_json::to_string(&enc).unwrap());
        }

        db.set_config("ai", &serde_json::to_value(final_ai).unwrap(), true)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 7. Update Backups
    if let Some(backups) = &payload.backups {
        db.set_config("backups", &serde_json::to_value(backups).unwrap(), false)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
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
    let general = db
        .get_config("general")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .unwrap_or(json!({}));

    // 2. Extract app_name or default
    let app_name = general
        .get("app_name")
        .and_then(|v| v.as_str())
        .unwrap_or("ApexKit App")
        .to_string();

    Ok(Json(AppNameResponse { app_name }))
}

#[derive(Serialize, ToSchema, Default)]
pub struct ResourceQuotaDetails {
    pub current_storage_mb: f64,
    pub max_storage_mb: i64,
    pub current_vectors: i64,
    pub max_vectors: i64,
    pub current_ai_requests: i64,
    pub max_ai_requests: i64,
    pub temp_storage_multiplier: f64,
    pub max_temp_storage_mb: f64,
}

#[derive(Serialize, ToSchema, Default)]
pub struct AppDetailsResponse {
    pub app_name: String,
    pub app_url: String,
    pub local_app_url: String,
    pub local_base_url: String,
    pub logo_url: String,
    pub scope: String,
    pub scope_type: String,
    pub scope_id: String,
    pub smtp_blocked: bool,
    pub version: String,
    pub resources: ResourceQuotaDetails,
}

#[utoipa::path(
    get,
    path = "/app-details",
    responses((status = 200, body = AppDetailsResponse))
)]
pub async fn get_app_details(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<apexkit_core::realtime::EventScope>>,
) -> Result<Json<AppDetailsResponse>, AppError> {
    use apexkit_core::realtime::EventScope;

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);

    let general = db
        .get_config("general")
        .await
        .unwrap_or(None)
        .unwrap_or(json!({}));
    let smtp = db
        .get_config("smtp")
        .await
        .unwrap_or(None)
        .unwrap_or(json!({}));
    let smtp_blocked = smtp
        .get("block_smtp")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let app_name = general
        .get("app_name")
        .and_then(|v| v.as_str())
        .unwrap_or("ApexKit App")
        .to_string();

    let temp_multiplier = crate::utils::get_temp_limit_multiplier();

    let (scope_type, scope_id, scope_str) = match &event_scope {
        EventScope::Root => ("root".to_string(), "root".to_string(), "root".to_string()),
        EventScope::Tenant(id) => ("tenant".to_string(), id.clone(), format!("tenant:{}", id)),
        EventScope::Sandbox(id) => ("sandbox".to_string(), id.clone(), format!("sandbox:{}", id)),
        _ => ("unknown".to_string(), "".to_string(), "unknown".to_string()),
    };

    let mut current_storage_mb = 0.0;
    let mut max_storage_mb = general
        .get("max_site_size_mb")
        .and_then(|v| v.as_i64())
        .unwrap_or(5000);
    let mut current_vectors = 0;
    let mut max_vectors = 0;
    let mut current_ai_requests = 0;
    let mut max_ai_requests = 0;

    match &event_scope {
        EventScope::Root => {
            // Root has global site limit, unbounded internal limits
            max_vectors = general
                .get("max_sandbox_vectors")
                .and_then(|v| v.as_i64())
                .unwrap_or(10000);
            max_ai_requests = general
                .get("max_sandbox_ai_requests")
                .and_then(|v| v.as_i64())
                .unwrap_or(100);
        }
        EventScope::Tenant(id) => {
            if let Ok(tenants) = state.db.list_tenants().await {
                if let Some(t) = tenants.iter().find(|t| &t.id == id) {
                    current_storage_mb = t.stats.storage_mb;
                    max_storage_mb = t.stats.max_storage_mb;
                    current_vectors = t.stats.vector_count;
                    max_vectors = t.stats.max_vectors;
                    current_ai_requests = t.stats.ai_requests;
                    max_ai_requests = t.stats.max_ai_requests;
                }
            }
        }
        EventScope::Sandbox(id) => {
            if let Ok(sandboxes) = state.db.list_sandboxes(None).await {
                if let Some(sb) = sandboxes.iter().find(|s| &s.id == id) {
                    current_storage_mb = sb.current_storage_mb;
                    max_storage_mb = sb.max_storage_mb;
                    current_vectors = sb.current_vectors;
                    max_vectors = sb.max_vectors;
                    current_ai_requests = sb.current_ai_requests;
                    max_ai_requests = sb.max_ai_requests;
                }
            }
        }
        _ => {}
    }

    let max_temp_storage_mb = (max_storage_mb as f64) * temp_multiplier;

    let port = state.port;
    let local_base_url = format!("http://127.0.0.1:{}", port);
    let scope_subpath = match &event_scope {
        EventScope::Tenant(id) => format!("/tenant/{}", id),
        EventScope::Sandbox(id) => format!("/sandbox/{}", id),
        _ => "".to_string(),
    };

    let local_app_url = format!("{}{}", local_base_url, scope_subpath);
    let app_url = format!("{}{}", base_url, scope_subpath);
    let logo_url = format!("{}/logo", app_url);

    Ok(Json(AppDetailsResponse {
        app_name,
        app_url,
        local_app_url,
        local_base_url,
        logo_url,
        scope: scope_str,
        scope_type,
        scope_id,
        smtp_blocked,
        version: env!("CARGO_PKG_VERSION").to_string(),
        resources: ResourceQuotaDetails {
            current_storage_mb,
            max_storage_mb,
            current_vectors,
            max_vectors,
            current_ai_requests,
            max_ai_requests,
            temp_storage_multiplier: temp_multiplier,
            max_temp_storage_mb,
        },
    }))
}
