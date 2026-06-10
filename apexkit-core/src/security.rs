use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand::RngCore;
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub struct MasterKey(Secret<Vec<u8>>);

impl MasterKey {
    pub fn from_string(key_str: String) -> Result<Self, String> {
        let bytes = BASE64
            .decode(key_str)
            .map_err(|_| "Invalid Base64 Master Key".to_string())?;
        if bytes.len() != 32 {
            return Err("Master Key must be exactly 32 bytes (AES-256)".to_string());
        }
        Ok(Self(Secret::new(bytes)))
    }

    pub fn generate() -> String {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        let encoded = BASE64.encode(key);
        key.zeroize();
        encoded
    }

    pub fn generate_random_password() -> String {
        uuid::Uuid::new_v4().to_string().chars().take(16).collect()
    }
}

#[derive(Serialize, Deserialize)]
pub struct EncryptedValue {
    pub ciphertext: String,
    pub nonce: String,
}

pub struct Vault {
    cipher: Aes256Gcm,
}

impl Vault {
    pub fn new(key: &MasterKey) -> Self {
        let key_bytes = key.0.expose_secret();
        let cipher = Aes256Gcm::new_from_slice(key_bytes).expect("Key size is correct");
        Self { cipher }
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<EncryptedValue, String> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| format!("Encryption failure: {}", e))?;

        Ok(EncryptedValue {
            ciphertext: BASE64.encode(ciphertext),
            nonce: BASE64.encode(nonce_bytes),
        })
    }

    pub fn decrypt(&self, value: &EncryptedValue) -> Result<String, String> {
        let ciphertext = BASE64
            .decode(&value.ciphertext)
            .map_err(|_| "Bad Ciphertext Base64".to_string())?;
        let nonce_bytes = BASE64
            .decode(&value.nonce)
            .map_err(|_| "Bad Nonce Base64".to_string())?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext_bytes = self
            .cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| "Decryption failed (Wrong Master Key or Corrupted Data)".to_string())?;

        String::from_utf8(plaintext_bytes).map_err(|_| "Invalid UTF-8 in secret".to_string())
    }
}

// --- NEW COMPOSITE API KEY SYSTEM ---

#[derive(Debug, PartialEq, Clone)]
pub enum KeyEnv {
    Sys,
    Tnnt,
    Sk,
    Pk,
}

pub struct ParsedKey {
    pub issuer: String,    // "root" or "tnt"
    pub env: KeyEnv,       // Sys, Tnnt, Sk, Pk
    pub tenant_id: String, // "root" or tenant_id
    pub secret: String,
    pub key_id: String,
}

/// Generates a highly structured API key
/// Returns: (Raw Token String, Secret Hash, Key ID)
pub fn generate_api_key(tenant_id: &str, env: KeyEnv) -> (String, String, String) {
    let prefix = match env {
        KeyEnv::Sys => "root_sys_prod".to_string(),
        KeyEnv::Tnnt => format!("root_tnnt_{}_prod", tenant_id),
        KeyEnv::Sk => format!("tnt_{}_sk_prod", tenant_id),
        KeyEnv::Pk => format!("tnt_{}_pk_prod", tenant_id),
    };

    // Secure random string
    let secret = crate::utils::generate_random_hex(32); // 64 chars
    let key_id = secret[secret.len() - 8..].to_string(); // Fast-lookup segment

    let payload = format!("{}_{}", prefix, secret);
    let checksum = &crate::utils::sha256(&payload)[0..4];

    let raw_key = format!("{}_{}_{}", prefix, secret, checksum);
    let secret_hash = crate::utils::sha256(&secret);

    (raw_key, secret_hash, key_id)
}

/// Fast-Fail Key Parser. Verifies structural integrity & checksum BEFORE any DB hit.
pub fn parse_and_validate_key(raw_key: &str) -> Option<ParsedKey> {
    let last_underscore = raw_key.rfind('_')?;
    let payload = &raw_key[0..last_underscore];
    let checksum = &raw_key[last_underscore + 1..];

    // Fast-Fail CRC Check
    if checksum != &crate::utils::sha256(payload)[0..4] {
        return None;
    }

    let parts: Vec<&str> = raw_key.split('_').collect();

    // Match: root_sys_prod_SECRET_CHK
    if parts[0] == "root" && parts[1] == "sys" && parts.len() >= 5 {
        let secret = parts[3].to_string();
        return Some(ParsedKey {
            issuer: "root".to_string(),
            env: KeyEnv::Sys,
            tenant_id: "root".to_string(),
            key_id: secret[secret.len() - 8..].to_string(),
            secret,
        });
    }
    // Match: root_tnnt_TID_prod_SECRET_CHK
    if parts[0] == "root" && parts[1] == "tnnt" && parts.len() >= 6 {
        let secret = parts[4].to_string();
        return Some(ParsedKey {
            issuer: "root".to_string(),
            env: KeyEnv::Tnnt,
            tenant_id: parts[2].to_string(),
            key_id: secret[secret.len() - 8..].to_string(),
            secret,
        });
    }
    // Match: tnt_TID_sk_prod_SECRET_CHK or tnt_TID_pk_prod_SECRET_CHK
    if parts[0] == "tnt" && parts.len() >= 6 {
        let env = if parts[2] == "sk" {
            KeyEnv::Sk
        } else {
            KeyEnv::Pk
        };
        let secret = parts[4].to_string();
        return Some(ParsedKey {
            issuer: "tnt".to_string(),
            env,
            tenant_id: parts[1].to_string(),
            key_id: secret[secret.len() - 8..].to_string(),
            secret,
        });
    }

    None
}
