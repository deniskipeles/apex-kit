use super::backends::get_storage_path;
use super::dto::{MigrateStorageReq, MigrationResult, TestS3ConfigReq};
use crate::system::dto::StorageConfigDto;
use crate::{AppError, AppState, DatabaseConnection};
use apexkit_core::auth::Claims;
use apexkit_core::security::vault::EncryptedValue;
use apexkit_core::storage::{LocalStorage, S3Storage, StorageBackend};
use axum::{Extension, Json, extract::State};
use std::sync::Arc;

#[utoipa::path(
    post,
    path = "/api/v1/admin/storage/test",
    request_body = TestS3ConfigReq,
    responses((status = 200, description = "Connection successful"), (status = 400, description = "Connection failed"), (status = 403, description = "Admin only"))
)]
pub async fn test_s3_connection(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Json(payload): Json<TestS3ConfigReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    let saved_json = db
        .get_config("storage")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let saved_config: Option<StorageConfigDto> = if let Some(val) = saved_json {
        serde_json::from_value(val).ok()
    } else {
        None
    };
    let s3_saved = saved_config.map(|c| c.s3).unwrap_or_default();

    let bucket = payload
        .bucket
        .filter(|s| !s.is_empty())
        .unwrap_or(s3_saved.bucket);
    let region = payload
        .region
        .filter(|s| !s.is_empty())
        .unwrap_or(s3_saved.region);
    let endpoint = payload
        .endpoint
        .filter(|s| !s.is_empty())
        .unwrap_or(s3_saved.endpoint);
    let access_key = payload
        .access_key
        .filter(|s| !s.is_empty())
        .unwrap_or(s3_saved.access_key);

    let raw_secret_key = if let Some(pk) = payload
        .secret_key
        .filter(|s| !s.is_empty() && s != "******")
    {
        pk
    } else if let Some(encrypted_str) = s3_saved.secret_key {
        if !encrypted_str.is_empty() {
            let enc: EncryptedValue = serde_json::from_str(&encrypted_str)
                .map_err(|_| AppError::JsonError("Saved secret key format is invalid".into()))?;
            state.vault.decrypt(&enc).map_err(|_| {
                AppError::UnknownError(
                    "Failed to decrypt saved secret key. Master Key mismatch?".into(),
                )
            })?
        } else {
            return Err(AppError::JsonError(
                "Secret key is empty in database. Please enter it.".into(),
            ));
        }
    } else {
        return Err(AppError::JsonError(
            "Secret key is missing. Please enter it explicitly.".into(),
        ));
    };

    if bucket.is_empty() {
        return Err(AppError::JsonError("Bucket is required".into()));
    }

    let final_region = if region.is_empty() {
        "auto".to_string()
    } else {
        region
    };

    let s3 = S3Storage::new_with_creds(
        &bucket,
        &final_region,
        &endpoint,
        "",
        &access_key,
        &raw_secret_key,
        "__root_app__/",
    )
    .await;

    let filename = ".apexkit_test_connectivity";

    s3.save(filename, b"connection_verified", "text/plain")
        .await
        .map_err(|e| AppError::JsonError(format!("Connection failed: {}", e)))?;
    s3.delete(filename).await.map_err(|e| {
        AppError::JsonError(format!(
            "Write succeeded but Delete failed: {}. Check permissions.",
            e
        ))
    })?;

    Ok(Json(
        serde_json::json!({ "success": true, "message": "Successfully connected, uploaded, and deleted test file." }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/storage/migrate",
    request_body = MigrateStorageReq,
    responses((status = 200, body = MigrationResult), (status = 403, description = "Admin only"))
)]
pub async fn migrate_storage(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Json(payload): Json<MigrateStorageReq>,
) -> Result<Json<MigrationResult>, AppError> {
    let claims = auth
        .ok_or(AppError::Unauthorized("Login required".into()))?
        .0;
    if claims.role != "admin" {
        return Err(AppError::Forbidden("Admins only".into()));
    }

    if payload.source == payload.destination {
        return Err(AppError::UnknownError(
            "Source and Destination cannot be the same".into(),
        ));
    }

    let settings_json = db
        .get_config("storage")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let config: StorageConfigDto = if let Some(val) = settings_json {
        serde_json::from_value(val).unwrap_or_default()
    } else {
        return Err(AppError::UnknownError("Storage not configured".into()));
    };

    let build_backend = |req_type: &str| -> Result<Arc<dyn StorageBackend>, AppError> {
        match req_type {
            "local" => Ok(Arc::new(futures::executor::block_on(LocalStorage::new(
                &get_storage_path("storage/system/uploads"),
                "/api/v1/storage/file/",
            )))),
            "s3" => {
                if !config.s3.enabled {
                    return Err(AppError::UnknownError("S3 is disabled in settings".into()));
                }
                let raw_secret = config
                    .s3
                    .secret_key
                    .as_ref()
                    .ok_or(AppError::UnknownError("S3 Secret missing".into()))?;
                let secret_key = if raw_secret.starts_with('{') {
                    let enc: EncryptedValue = serde_json::from_str(raw_secret)
                        .map_err(|_| AppError::UnknownError("Bad key format".into()))?;
                    state
                        .vault
                        .decrypt(&enc)
                        .map_err(|_| AppError::UnknownError("Decrypt fail".into()))?
                } else {
                    raw_secret.clone()
                };

                let s3 = futures::executor::block_on(S3Storage::new_with_creds(
                    &config.s3.bucket,
                    &config.s3.region,
                    &config.s3.endpoint,
                    "",
                    &config.s3.access_key,
                    &secret_key,
                    "__root_app__/",
                ));
                Ok(Arc::new(s3))
            }
            _ => Err(AppError::UnknownError("Invalid storage type".into())),
        }
    };

    let source_backend = build_backend(&payload.source)?;
    let dest_backend = build_backend(&payload.destination)?;

    let mut offset = 0;
    let limit = 50;
    let mut processed_count = 0;
    let mut error_count = 0;

    loop {
        let files = db
            .list_files(limit, offset)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
        if files.is_empty() {
            break;
        }

        for file in files {
            match source_backend.get(&file.filename).await {
                Ok(data) => {
                    if dest_backend
                        .save(&file.filename, &data, &file.mime_type)
                        .await
                        .is_err()
                    {
                        error_count += 1;
                    } else {
                        processed_count += 1;
                    }
                }
                Err(_) => {
                    error_count += 1;
                }
            }
        }
        offset += limit;
    }

    Ok(Json(MigrationResult {
        success: true,
        processed: processed_count,
        errors: error_count,
        message: format!(
            "Processed {} files. {} errors.",
            processed_count, error_count
        ),
    }))
}
