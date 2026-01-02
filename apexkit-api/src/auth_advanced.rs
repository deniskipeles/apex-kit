use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
    Json,
};
use serde::Deserialize;
// use std::env;
use apexkit_core::{auth, jobs::Job};
use crate::{AppState, AppError, AuthResponse, UserDto};
use apexkit_core::security::EncryptedValue;

// --- GitHub Models ---
#[derive(Deserialize)]
pub struct OauthCallback {
    code: String,
}

#[derive(Deserialize)]
struct GithubToken {
    access_token: String,
}

#[derive(Deserialize)]
struct GithubUser {
    id: i64,
    email: Option<String>,
    login: String,
}

// Helper to fetch and decrypt
async fn get_secret(state: &AppState, key: &str) -> Result<String, AppError> {
    let json_opt = state.db.get_config(key).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
        
    let json_val = json_opt.ok_or_else(|| AppError::UnknownError(format!("Configuration '{}' missing", key)))?;
    
    // Deserialize JSON to EncryptedValue
    let enc: EncryptedValue = serde_json::from_value(json_val)
        .map_err(|_| AppError::UnknownError("Invalid secret format".into()))?;
    
    state.vault.decrypt(&enc).map_err(|_| AppError::UnknownError("Decryption failed".into()))
}

// --- GitHub Handlers ---

pub async fn github_login(State(state): State<AppState>) -> Result<Redirect, AppError> {
    // DYNAMIC FETCH
    let client_id = get_secret(&state, "github_client_id").await?;
    
    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&scope=user:email",
        client_id
    );
    Ok(Redirect::to(&url))
}

pub async fn github_callback(
    State(state): State<AppState>,
    Query(params): Query<OauthCallback>,
) -> Result<Json<AuthResponse>, AppError> {
    // DYNAMIC FETCH
    let client_id = get_secret(&state, "github_client_id").await?;
    let client_secret = get_secret(&state, "github_client_secret").await?;
    
    // 1. Exchange Code for Token
    let client = reqwest::Client::new();
    let token_res = client.post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", params.code.as_str()),
        ])
        .send()
        .await
        .map_err(|_| AppError::UnknownError("Failed to exchange code".into()))?
        .json::<GithubToken>()
        .await
        .map_err(|_| AppError::UnknownError("Failed to parse token".into()))?;

    // 2. Get User Info
    let gh_user = client.get("https://api.github.com/user")
        .header("User-Agent", "ApexKit")
        .header("Authorization", format!("Bearer {}", token_res.access_token))
        .send()
        .await
        .map_err(|_| AppError::UnknownError("Failed to get user".into()))?
        .json::<GithubUser>()
        .await
        .map_err(|_| AppError::UnknownError("Failed to parse user".into()))?;

    // 3. Find or Create User
    let provider_id = gh_user.id.to_string();
    let existing = state.db.get_user_by_oauth("github", &provider_id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let user = match existing {
        Some(u) => u,
        None => {
            // Create new user
            let email = gh_user.email.unwrap_or_else(|| format!("{}@github.oauth", gh_user.login));
            let pwd = uuid::Uuid::new_v4().to_string(); // Random pwd
            let hash = auth::hash_password(&pwd).unwrap();
            
            let u = state.db.create_user(&email, &hash, "user", None).await
                .map_err(|_| AppError::UnknownError("Email already taken".into()))?;
            
            state.db.link_oauth(u.id, "github", &provider_id).await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;
            u
        }
    };

    // 4. Issue JWT
    let token = auth::create_jwt(user.id, &user.email, &user.role)
        .map_err(|_| AppError::UnknownError("Token failed".into()))?;

    Ok(Json(AuthResponse {
        token,
        user: UserDto { id: user.id, email: user.email, role: user.role, metadata: user.metadata },
    }))
}

// --- Verification Handlers ---

#[derive(Deserialize)]
pub struct VerifyRequest {
    token: String,
}

pub async fn verify_email(
    State(state): State<AppState>,
    Query(params): Query<VerifyRequest>,
) -> Result<String, AppError> {
    let user_id = state.db.consume_auth_token(&params.token, "verify").await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::Unauthorized("Invalid or expired token".into()))?;

    state.db.set_user_verified(user_id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok("Email verified successfully!".to_string())
}

#[derive(Deserialize)]
pub struct ResendRequest {
    email: String,
}

pub async fn resend_verification(
    State(state): State<AppState>,
    Json(payload): Json<ResendRequest>,
) -> Result<StatusCode, AppError> {
    if let Some(user) = state.db.get_user_by_email(&payload.email).await.unwrap() {
        let token = uuid::Uuid::new_v4().to_string();
        state.db.create_auth_token(user.id, "verify", &token).await.unwrap();
        state.queue.enqueue(Job::SendVerification { email: user.email, token }).await;
    }
    // Always return OK to prevent enumeration
    Ok(StatusCode::OK)
}