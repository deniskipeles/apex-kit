use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce
};
use rand::{RngCore};
use secrecy::{ExposeSecret, Secret};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use zeroize::Zeroize;
use serde::{Deserialize, Serialize};

// FIX: Removed #[derive(Clone)] to avoid E0277 with secrecy
pub struct MasterKey(Secret<Vec<u8>>);

impl MasterKey {
    pub fn from_string(key_str: String) -> Result<Self, String> {
        let bytes = BASE64.decode(key_str).map_err(|_| "Invalid Base64 Master Key".to_string())?;
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

        let ciphertext = self.cipher.encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| format!("Encryption failure: {}", e))?;

        Ok(EncryptedValue {
            ciphertext: BASE64.encode(ciphertext),
            nonce: BASE64.encode(nonce_bytes),
        })
    }

    pub fn decrypt(&self, value: &EncryptedValue) -> Result<String, String> {
        let ciphertext = BASE64.decode(&value.ciphertext).map_err(|_| "Bad Ciphertext Base64".to_string())?;
        let nonce_bytes = BASE64.decode(&value.nonce).map_err(|_| "Bad Nonce Base64".to_string())?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        // FIX: Explicit .to_string() for error types
        let plaintext_bytes = self.cipher.decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| "Decryption failed (Wrong Master Key or Corrupted Data)".to_string())?;

        String::from_utf8(plaintext_bytes).map_err(|_| "Invalid UTF-8 in secret".to_string())
    }
}