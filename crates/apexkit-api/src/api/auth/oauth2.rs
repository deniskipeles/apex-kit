use crate::{AppError, AppState, AuthResponse, UserDto};
use crate::{BaseUrl, DatabaseConnection};
use apexkit_core::Db;
use apexkit_core::auth;
use apexkit_core::realtime::EventScope;
use apexkit_core::security::vault::{EncryptedValue, Vault};
use axum::Extension;
use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use std::sync::Arc;

// --- GitHub Models ---
#[derive(Deserialize)]
pub struct OauthCallback {
    code: String,
    state: Option<String>, // Can be used for CSRF or Redirect URL
}

#[derive(Deserialize)]
pub struct LoginQuery {
    redirect_to: Option<String>,
}

// --- Unified OAuth Token Model ---
#[derive(Deserialize)]
struct ProviderToken {
    access_token: String,
}

// --- Provider Specific DTOs (Mapped from their respective APIs) ---
#[derive(Deserialize)]
struct GithubUser {
    id: i64, // GitHub uses integers for IDs
    email: Option<String>,
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GoogleUser {
    id: String, // Google uses strings for IDs
    email: Option<String>,
    name: Option<String>,
    picture: Option<String>,
}

// Helper to fetch, dynamically inspect, and decrypt database secrets safely
async fn get_secret(db: Arc<dyn Db>, vault: Arc<Vault>, key: &str) -> Result<String, AppError> {
    let json_opt = db
        .get_config(key)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let json_val = json_opt
        .ok_or_else(|| AppError::UnknownError(format!("Configuration '{}' missing", key)))?;

    // 1. Try to parse as EncryptedValue. If successful, decrypt it.
    if let Ok(enc) = serde_json::from_value::<EncryptedValue>(json_val.clone()) {
        vault
            .decrypt(&enc)
            .map_err(|_| AppError::UnknownError("Decryption failed. Verify master key.".into()))
    } else if let Some(raw_str) = json_val.as_str() {
        // 2. Fallback: If not encrypted, return the raw string directly
        Ok(raw_str.to_string())
    } else {
        Err(AppError::UnknownError("Invalid secret format".into()))
    }
}

// --- GitHub Handlers ---

pub async fn github_login(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    Query(query): Query<LoginQuery>,
) -> Result<Redirect, AppError> {
    let client_id = get_secret(db, state.vault.clone(), "github_client_id").await?;
    let state_param = query.redirect_to.unwrap_or_default();

    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&scope=user:email&state={}",
        client_id, state_param
    );
    Ok(Redirect::to(&url))
}

pub async fn github_callback(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Query(params): Query<OauthCallback>,
) -> Result<Response, AppError> {
    let client_id = get_secret(db.clone(), state.vault.clone(), "github_client_id").await?;
    let client_secret = get_secret(db.clone(), state.vault.clone(), "github_client_secret").await?;

    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let scope_str = match &event_scope {
        EventScope::Tenant(id) => format!("tenant:{}", id),
        EventScope::Sandbox(id) => format!("sandbox:{}", id),
        _ => "root".to_string(),
    };

    let client = reqwest::Client::new();
    let token_res = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", params.code.as_str()),
        ])
        .send()
        .await
        .map_err(|_| AppError::UnknownError("Failed to exchange code".into()))?
        .json::<ProviderToken>()
        .await
        .map_err(|_| AppError::UnknownError("Failed to parse token".into()))?;

    let gh_user = client
        .get("https://api.github.com/user")
        .header("User-Agent", "ApexKit")
        .header(
            "Authorization",
            format!("Bearer {}", token_res.access_token),
        )
        .send()
        .await
        .map_err(|_| AppError::UnknownError("Failed to get user".into()))?
        .json::<GithubUser>()
        .await
        .map_err(|_| AppError::UnknownError("Failed to parse user".into()))?;

    // --- NEW: Explicitly fetch private emails from GitHub ---
    let mut actual_email = gh_user.email.clone();

    if actual_email.is_none() || actual_email.as_ref().unwrap().is_empty() {
        #[derive(Deserialize)]
        struct GithubEmail {
            email: String,
            primary: bool,
            verified: bool,
        }

        if let Ok(emails_res) = client
            .get("https://api.github.com/user/emails")
            .header("User-Agent", "ApexKit")
            .header(
                "Authorization",
                format!("Bearer {}", token_res.access_token),
            )
            .send()
            .await
            && let Ok(emails) = emails_res.json::<Vec<GithubEmail>>().await
        {
            // Prefer primary and verified email
            if let Some(primary) = emails.iter().find(|e| e.primary && e.verified) {
                actual_email = Some(primary.email.clone());
            } else if let Some(first) = emails.first() {
                actual_email = Some(first.email.clone());
            }
        }
    }

    // NORMALIZATION
    let provider_id = gh_user.id.to_string();
    let email = actual_email.unwrap_or_else(|| format!("{}@github.oauth", gh_user.login));
    let name = gh_user.name.unwrap_or(gh_user.login);
    let avatar = gh_user.avatar_url;

    process_oauth_user(
        db,
        provider_id,
        "github".to_string(),
        email,
        name,
        avatar,
        scope_str,
        params.state,
    )
    .await
}

// --- Google Handlers ---

pub async fn google_login(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    uri: axum::http::Uri, // [FIX]: Use Uri extractor instead of Request
    Query(query): Query<LoginQuery>,
) -> Result<Redirect, AppError> {
    let client_id = get_secret(db, state.vault.clone(), "google_client_id").await?;
    let state_param = query.redirect_to.unwrap_or_default();

    // Construct exact callback URL based on current context
    let path = uri.path();
    let callback_path = format!("{}/callback", path);
    let redirect_uri = format!("{}{}", base_url, callback_path); // base_url.0 accesses the inner String

    let url = reqwest::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", "email profile"),
            ("state", state_param.as_str()),
        ],
    )
    .map_err(|e| AppError::UnknownError(e.to_string()))?
    .to_string();

    Ok(Redirect::to(&url))
}

pub async fn google_callback(
    DatabaseConnection(db): DatabaseConnection,
    State(state): State<AppState>,
    BaseUrl(base_url): BaseUrl,
    scope: Option<Extension<EventScope>>,
    uri: axum::http::Uri,
    Query(params): Query<OauthCallback>,
) -> Result<Response, AppError> {
    let client_id = get_secret(db.clone(), state.vault.clone(), "google_client_id").await?;
    let client_secret = get_secret(db.clone(), state.vault.clone(), "google_client_secret").await?;

    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let scope_str = match &event_scope {
        EventScope::Tenant(id) => format!("tenant:{}", id),
        EventScope::Sandbox(id) => format!("sandbox:{}", id),
        _ => "root".to_string(),
    };

    let redirect_uri = format!("{}{}", base_url, uri.path());

    // 1. Exchange Code
    let client = reqwest::Client::new();
    let token_res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", params.code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|_| AppError::UnknownError("Failed to exchange code".into()))?
        .json::<ProviderToken>()
        .await
        .map_err(|_| AppError::UnknownError("Failed to parse token".into()))?;

    // 2. Get User Info
    let g_user = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .header(
            "Authorization",
            format!("Bearer {}", token_res.access_token),
        )
        .send()
        .await
        .map_err(|_| AppError::UnknownError("Failed to get user".into()))?
        .json::<GoogleUser>()
        .await
        .map_err(|_| AppError::UnknownError("Failed to parse user".into()))?;

    // 3. Find or Create User
    let provider_id = g_user.id.clone();

    // [FIXED] Reject login if Google does not return an email, rather than making one up.
    let email = g_user.email.clone().ok_or_else(|| {
        AppError::UnknownError(
            "Google account has no associated email address. Required for login.".into(),
        )
    })?;

    let name = g_user
        .name
        .unwrap_or_else(|| email.split('@').next().unwrap_or("User").to_string());
    let avatar = g_user.picture;

    // This will now seamlessly link to an existing account if the email matches
    process_oauth_user(
        db,
        provider_id,
        "google".to_string(),
        email,
        name,
        avatar,
        scope_str,
        params.state,
    )
    .await
}

// --- Shared Internal Logic for OAuth Convergence ---
async fn process_oauth_user(
    db: Arc<dyn apexkit_core::Db>,
    provider_id: String,
    provider_name: String,
    email: String,
    name: String,
    avatar: Option<String>,
    scope_str: String,
    redirect_target: Option<String>,
) -> Result<Response, AppError> {
    // 1. Check if this specific OAuth identity (e.g. GitHub ID 12345) already exists
    let existing_identity = db
        .get_user_by_oauth(&provider_name, &provider_id)
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let user = match existing_identity {
        Some(u) => u, // User has logged in with this provider before
        None => {
            // 2. Check if a user with this email already exists from a different provider or email/password
            if let Ok(Some(existing_user)) = db.get_user_by_email(&email).await {
                // Email exists! Securely link this new OAuth method to the existing account
                db.link_oauth(existing_user.id, &provider_name, &provider_id)
                    .await
                    .map_err(|e| AppError::UnknownError(e.to_string()))?;

                existing_user
            } else {
                // 3. Completely new user. Create account and link identity.
                let metadata = serde_json::json!({
                    "avatar": avatar,
                    "name": name,
                    "provider": provider_name
                });

                let pwd = uuid::Uuid::new_v4().to_string();
                let hash = auth::hash_password(&pwd).unwrap();

                let u = db
                    .create_user(&email, &hash, "user", Some(metadata))
                    .await
                    .map_err(|_| AppError::UnknownError("Failed to create new user".into()))?;

                db.link_oauth(u.id, &provider_name, &provider_id)
                    .await
                    .map_err(|e| AppError::UnknownError(e.to_string()))?;

                u
            }
        }
    };

    // Generate JWT for the resolved user
    let token = auth::create_jwt(user.id, &user.email, &user.role, &scope_str)
        .map_err(|_| AppError::UnknownError("Token failed".into()))?;

    if let Some(target) = redirect_target.filter(|s| !s.is_empty()) {
        let separator = if target.contains('?') { '&' } else { '?' };
        let redirect_url = format!("{}{}{}={}", target, separator, "token", token);
        return Ok(Redirect::to(&redirect_url).into_response());
    }

    Ok(Json(AuthResponse {
        token,
        user: UserDto {
            id: user.id,
            email: user.email,
            role: user.role,
            metadata: user.metadata,
            scope: Some(scope_str),
        },
    })
    .into_response())
}
