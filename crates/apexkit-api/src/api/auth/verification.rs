use crate::DatabaseConnection;
use crate::{AppError, AppState};
use apexkit_core::realtime::EventScope;
use apexkit_core::workers::Job;
use axum::Extension;
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use utoipa::ToSchema;

// --- Verification Handlers ---

#[derive(Deserialize)]
pub struct VerifyRequest {
    token: String,
}

pub async fn verify_email(
    DatabaseConnection(db): DatabaseConnection, // [FIX] Inject scoped DB
    Query(params): Query<VerifyRequest>,
) -> Result<String, AppError> {
    let user_id = db
        .consume_auth_token(&params.token, "verify")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::Unauthorized("Invalid or expired token".into()))?;

    db.set_user_verified(user_id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok("Email verified successfully!".to_string())
}

#[derive(Deserialize, ToSchema)]
pub struct ResendRequest {
    pub email: String,
}

#[derive(Deserialize, ToSchema)]
pub struct RequestPasswordResetReq {
    pub email: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ConfirmPasswordResetReq {
    pub token: String,
    #[schema(example = "newpassword123")]
    pub new_password: String,
}

pub async fn resend_verification(
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection, // [FIX] Inject scoped DB
    State(state): State<AppState>,
    Json(payload): Json<ResendRequest>,
) -> Result<StatusCode, AppError> {
    let tenant_id = crate::utils::get_tenant_id_from_scope(scope.as_ref().map(|e| &e.0));
    if let Some(user) = db.get_user_by_email(&payload.email).await.unwrap() {
        let token = uuid::Uuid::new_v4().to_string();
        db.create_auth_token(user.id, "verify", &token)
            .await
            .unwrap();
        state
            .queue
            .enqueue(Job::SendVerification {
                tenant_id,
                email: user.email,
                token,
            })
            .await;
    }
    // Always return OK to prevent enumeration
    Ok(StatusCode::OK)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/request-password-reset",
    request_body = RequestPasswordResetReq,
    responses((status = 200, description = "Reset email sent"))
)]
pub async fn request_password_reset(
    scope: Option<Extension<EventScope>>,
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Json(payload): Json<RequestPasswordResetReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = crate::utils::get_tenant_id_from_scope(scope.as_ref().map(|e| &e.0));

    // Check if user exists
    if let Ok(Some(user)) = db.get_user_by_email(&payload.email).await {
        let token = uuid::Uuid::new_v4().to_string();
        // Save the reset token to the database
        db.create_auth_token(user.id, "reset", &token)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
        // Enqueue the background email job
        state
            .queue
            .enqueue(Job::SendPasswordReset {
                tenant_id,
                email: user.email,
                token,
            })
            .await;
    }

    // Always return 200 OK to prevent email enumeration attacks
    Ok(Json(
        json!({ "success": true, "message": "If the email exists, a reset link has been sent." }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/confirm-password-reset",
    request_body = ConfirmPasswordResetReq,
    responses((status = 200, description = "Password updated successfully"))
)]
pub async fn confirm_password_reset(
    DatabaseConnection(db): DatabaseConnection,
    Json(payload): Json<ConfirmPasswordResetReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    if payload.new_password.len() < 6 {
        return Err(AppError::JsonError(
            "Password must be at least 6 characters long".into(),
        ));
    }

    // Attempt to consume the token. Will return None if invalid or expired.
    let user_id = db
        .consume_auth_token(&payload.token, "reset")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::Unauthorized(
            "Invalid or expired reset token".into(),
        ))?;

    // Update the password
    db.update_user(user_id, None, None, None, Some(payload.new_password))
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(
        json!({ "success": true, "message": "Password updated successfully" }),
    ))
}
