use super::models::Claims;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use std::sync::OnceLock;

static JWT_SECRET: OnceLock<Vec<u8>> = OnceLock::new();

fn get_jwt_secret() -> &'static [u8] {
    JWT_SECRET.get_or_init(|| {
        if let Ok(key_str) = std::env::var("APEXKIT_MASTER_KEY")
            && let Ok(bytes) = BASE64.decode(key_str.trim())
                && bytes.len() == 32 {
                    return bytes;
                }
        println!("⚠️ [SECURITY WARNING] APEXKIT_MASTER_KEY not found in env. Falling back to local debug secret.");
        b"local_development_secure_jwt_secret_fallback_32_bytes_long".to_vec()
    })
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
        &EncodingKey::from_secret(get_jwt_secret()),
    )
}

pub fn decode_jwt(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(get_jwt_secret()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|data| data.claims)
}
