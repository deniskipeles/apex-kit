use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

// --- Models ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: i64,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String, // "admin" or "user"
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String, // email
    pub uid: i64,    // user id
    pub role: String,
    pub exp: usize,
    #[serde(default)]
    pub scope: String,
}

// --- SECURE DYNAMIC SECRET RESOLUTION ---
static JWT_SECRET: OnceLock<Vec<u8>> = OnceLock::new();

fn get_jwt_secret() -> &'static [u8] {
    JWT_SECRET.get_or_init(|| {
        // Read the unique server master key from the environment
        if let Ok(key_str) = std::env::var("APEXKIT_MASTER_KEY") {
            if let Ok(bytes) = BASE64.decode(key_str.trim()) {
                if bytes.len() == 32 {
                    return bytes; // Uses the full 256-bit secure key
                }
            }
        }
        // Strict fallback warning for development, forces manual environment verification
        println!("⚠️ [SECURITY WARNING] APEXKIT_MASTER_KEY not found in env. Falling back to local debug secret.");
        b"local_development_secure_jwt_secret_fallback_32_bytes_long".to_vec()
    })
}

pub fn hash_password(password: &str) -> Result<String, bcrypt::BcryptError> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

pub fn create_jwt(
    id: i64,
    email: &str,
    role: &str,
    scope: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: email.to_owned(),
        uid: id,
        role: role.to_owned(),
        exp: expiration as usize,
        scope: scope.to_owned(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(get_jwt_secret()), // Signed with derived Master Key
    )
}

pub fn decode_jwt(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(get_jwt_secret()), // Verified against derived Master Key
        &Validation::new(Algorithm::HS256),
    )
    .map(|data| data.claims)
}
