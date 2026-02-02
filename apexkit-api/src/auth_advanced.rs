use axum::{
    extract::{Query, State, Path},
    http::{StatusCode},
    response::{Redirect, Response, IntoResponse},
    Json,
};
use serde::{Deserialize, Serialize};
use apexkit_core::{auth, jobs::Job};
use crate::{AppState, AppError, AuthResponse, UserDto};
use apexkit_core::security::EncryptedValue;
use serde_json::json;
use apexkit_core::auth::Claims;
use crate::{DatabaseConnection, IdPath};
use axum::Extension;
use apexkit_core::jobs;
use utoipa::ToSchema;
use apexkit_core::realtime::EventScope;
use std::sync::Arc;
use apexkit_core::{Db, security::Vault};

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

#[derive(Deserialize)]
struct GithubToken {
    access_token: String,
}

#[derive(Deserialize)]
struct GithubUser {
    id: i64,
    email: Option<String>,
    login: String,
    // [NEW] Metadata fields
    avatar_url: Option<String>,
    name: Option<String>,
    html_url: Option<String>,
}

// Helper to fetch and decrypt
async fn get_secret(db: Arc<dyn Db>, vault: Arc<Vault>, key: &str) -> Result<String, AppError> {
    let json_opt = db.get_config(key).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
        
    let json_val = json_opt.ok_or_else(|| AppError::UnknownError(format!("Configuration '{}' missing", key)))?;
    
    let enc: EncryptedValue = serde_json::from_value(json_val)
        .map_err(|_| AppError::UnknownError("Invalid secret format".into()))?;
    
    vault.decrypt(&enc).map_err(|_| AppError::UnknownError("Decryption failed".into()))
}

// --- GitHub Handlers ---

pub async fn github_login(
    DatabaseConnection(db): DatabaseConnection, // [FIX] Inject scoped DB
    State(state): State<AppState>,
    Query(query): Query<LoginQuery>,
) -> Result<Redirect, AppError> {
    let client_id = get_secret(db, state.vault.clone(), "github_client_id").await?;
    
    // Pass the redirect URL in the 'state' parameter (simple approach)
    // For production, this should be signed/encrypted to prevent open redirect vulnerabilities
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
    scope: Option<Extension<EventScope>>, // [FIX] Capture current scope (Root/Tenant)
    Query(params): Query<OauthCallback>,
) -> Result<Response, AppError> {
    let client_id = get_secret(db.clone(), state.vault.clone(), "github_client_id").await?;
    let client_secret = get_secret(db.clone(), state.vault.clone(), "github_client_secret").await?;
    
    // [FIX] Determine Scope String from Context
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let scope_str = match &event_scope {
        EventScope::Tenant(id) => format!("tenant:{}", id),
        EventScope::Sandbox(id) => format!("sandbox:{}", id),
        _ => "root".to_string()
    };

    // 1. Exchange Code
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

    // 3. Find or Create User (In the scoped DB)
    let provider_id = gh_user.id.to_string();
    let existing = db.get_user_by_oauth("github", &provider_id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let user = match existing {
        Some(u) => u,
        None => {
            let metadata = json!({
                "avatar": gh_user.avatar_url,
                "name": gh_user.name.unwrap_or(gh_user.login.clone()),
                "github_url": gh_user.html_url,
                "source": "github"
            });

            let email = gh_user.email.unwrap_or_else(|| format!("{}@github.oauth", gh_user.login));
            let pwd = uuid::Uuid::new_v4().to_string(); 
            let hash = auth::hash_password(&pwd).unwrap();
            
            let u = db.create_user(&email, &hash, "user", Some(metadata)).await
                .map_err(|_| AppError::UnknownError("Email already taken".into()))?;
            
            db.link_oauth(u.id, "github", &provider_id).await
                .map_err(|e| AppError::UnknownError(e.to_string()))?;
            u
        }
    };

    // 4. Issue JWT with CORRECT SCOPE
    let token = auth::create_jwt(user.id, &user.email, &user.role, &scope_str)
        .map_err(|_| AppError::UnknownError("Token failed".into()))?;

    // 5. Handle Response
    if let Some(target) = params.state.filter(|s| !s.is_empty()) {
        let separator = if target.contains('?') { '&' } else { '?' };
        let redirect_url = format!("{}{}{}={}", target, separator, "token", token);
        return Ok(Redirect::to(&redirect_url).into_response());
    }

    Ok(Json(AuthResponse {
        token,
        user: UserDto { id: user.id, email: user.email, role: user.role, metadata: user.metadata, scope: Some(scope_str), },
    }).into_response())
}

// --- Verification Handlers ---

#[derive(Deserialize)]
pub struct VerifyRequest {
    token: String,
}

pub async fn verify_email(
    DatabaseConnection(db): DatabaseConnection, // [FIX] Inject scoped DB
    Query(params): Query<VerifyRequest>,
) -> Result<String, AppError> {
    let user_id = db.consume_auth_token(&params.token, "verify").await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .ok_or(AppError::Unauthorized("Invalid or expired token".into()))?;

    db.set_user_verified(user_id).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok("Email verified successfully!".to_string())
}

#[derive(Deserialize)]
pub struct ResendRequest {
    email: String,
}

pub async fn resend_verification(
    DatabaseConnection(db): DatabaseConnection, // [FIX] Inject scoped DB
    State(state): State<AppState>,
    Json(payload): Json<ResendRequest>,
) -> Result<StatusCode, AppError> {
    if let Some(user) = db.get_user_by_email(&payload.email).await.unwrap() {
        let token = uuid::Uuid::new_v4().to_string();
        db.create_auth_token(user.id, "verify", &token).await.unwrap();
        state.queue.enqueue(Job::SendVerification { email: user.email, token }).await;
    }
    // Always return OK to prevent enumeration
    Ok(StatusCode::OK)
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    responses((status = 200, body = UserDto))
)]
pub async fn get_me(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<UserDto>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    
    // We can fetch fresh data from DB, or just return claims if that's enough.
    // Fetching is safer to ensure user wasn't deleted/banned.
    // Note: get_user_by_email might need to be exposed or we use list with filter. 
    // Ideally, we should have get_user(id) in the Db trait.
    
    // Since 'get_user' by ID isn't explicitly in the Db trait visible here (only list/get_by_email),
    // let's use list with ID filter logic or add get_user(id). 
    // Assuming we can use get_users_by_ids([id]) which IS in the trait.
    
    let users = db.get_users_by_ids(&[claims.uid]).await.map_err(|e| AppError::UnknownError(e.to_string()))?;
    
    let user = users.first().ok_or(AppError::NotFound("User not found".into()))?;

    Ok(Json(UserDto {
        id: user.id,
        email: user.email.clone(),
        role: user.role.clone(),
        metadata: user.metadata.clone(),
        scope: Some(claims.scope),
    }))
}

#[derive(Serialize, ToSchema)]
pub struct RolesResponse {
    pub roles: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/roles",
    responses((status = 200, body = RolesResponse))
)]
pub async fn list_roles_handler(
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Json<RolesResponse>, AppError> {
    // 1. Try to fetch from config
    let roles = if let Ok(Some(val)) = db.get_config("APEX_AUTH_ROLES").await {
        // [FIX] Handle potential double-encoding or string-wrapped JSON
        if let Some(s) = val.as_str() {
             // If it's a string, try to parse it as JSON array
             serde_json::from_str::<Vec<String>>(s).unwrap_or_else(|_| vec!["admin".to_string(), "user".to_string()])
        } else if val.is_array() {
             // If it's already an array value
             serde_json::from_value::<Vec<String>>(val).unwrap_or_else(|_| vec!["admin".to_string(), "user".to_string()])
        } else {
            // Default roles
             vec!["admin".to_string(), "user".to_string()]
        }
    } else {
        // Default roles
        vec!["admin".to_string(), "user".to_string()]
    };
    Ok(Json(RolesResponse { roles }))
}

#[derive(Deserialize, ToSchema)]
pub struct TestEmailReq {
    pub email: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/smtp/test",
    request_body = TestEmailReq,
    responses((status = 200, description = "Email Sent"))
)]
pub async fn test_email_handler(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Json(payload): Json<TestEmailReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let subject = "Test Email from ApexKit";
    let body = "If you are reading this, your SMTP or Sendmail configuration is working correctly.";

    jobs::send_email(state.db.clone(), state.vault.clone(), &payload.email, subject, body).await
        .map_err(|e| AppError::UnknownError(format!("Failed to send: {}", e)))?;

    Ok(Json(json!({ "success": true, "message": "Email sent." })))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateUserReq {
    pub email: Option<String>,
    pub role: Option<String>,
    pub password: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub metadata: Option<serde_json::Value>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/users/{id}",
    request_body = UpdateUserReq,
    params(IdPath),
    responses((status = 200, body = UserDto))
)]
pub async fn update_user_handler(
    auth: Option<Extension<Claims>>,
    DatabaseConnection(db): DatabaseConnection,
    Path(path): Path<IdPath>,
    Json(payload): Json<UpdateUserReq>
) -> Result<Json<UserDto>, AppError> {
    let claims = auth.ok_or(AppError::Unauthorized("Login required".into()))?.0;
    if claims.role != "admin" { return Err(AppError::Forbidden("Admins only".into())); }

    let user_id = path.id.parse::<i64>().map_err(|_| AppError::JsonError("Invalid User ID".into()))?;

    // Pass password to DB layer
    let u = db.update_user(user_id, payload.email, payload.role, payload.metadata, payload.password).await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    Ok(Json(UserDto {
        id: u.id,
        email: u.email,
        role: u.role,
        metadata: u.metadata,
        scope: None 
    }))
}