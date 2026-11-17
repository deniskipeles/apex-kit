//! Unit tests for the security layer: AES-GCM vault and API-key generation.

use apexkit_core::security::api_keys::{
    KeyEnv, generate_api_key, parse_and_validate_key,
};
use apexkit_core::security::vault::{MasterKey, Vault};
use base64::{Engine as _, engine::general_purpose::STANDARD};

fn vault_with_zero_key() -> Vault {
    let key = MasterKey::from_string(STANDARD.encode([0u8; 32])).unwrap();
    Vault::new(&key)
}

#[test]
fn master_key_requires_32_bytes() {
    assert!(MasterKey::from_string(STANDARD.encode([0u8; 32])).is_ok());
    // 16 bytes is too short.
    assert!(MasterKey::from_string(STANDARD.encode([0u8; 16])).is_err());
    // Non-base64 input is rejected.
    assert!(MasterKey::from_string("not base64!!!".to_string()).is_err());
}

#[test]
fn master_key_generate_produces_valid_32_byte_key() {
    let encoded = MasterKey::generate();
    let bytes = STANDARD.decode(&encoded).unwrap();
    assert_eq!(bytes.len(), 32);
    // Generated key must round-trip through the parser.
    assert!(MasterKey::from_string(encoded).is_ok());
}

#[test]
fn generated_password_is_16_chars() {
    let p = MasterKey::generate_random_password();
    assert_eq!(p.len(), 16);
}

#[test]
fn vault_encrypt_decrypt_round_trip() {
    let vault = vault_with_zero_key();
    let secret = "super secret value";
    let enc = vault.encrypt(secret).unwrap();
    assert_ne!(enc.ciphertext, secret);
    assert_eq!(vault.decrypt(&enc).unwrap(), secret);
}

#[test]
fn vault_encryption_uses_fresh_nonce_each_time() {
    let vault = vault_with_zero_key();
    let a = vault.encrypt("same").unwrap();
    let b = vault.encrypt("same").unwrap();
    assert_ne!(a.nonce, b.nonce, "each encryption must use a fresh nonce");
    assert_ne!(a.ciphertext, b.ciphertext);
}

#[test]
fn vault_decrypt_with_wrong_key_fails() {
    let vault1 = vault_with_zero_key();
    let enc = vault1.encrypt("secret").unwrap();

    let other = Vault::new(&MasterKey::from_string(STANDARD.encode([7u8; 32])).unwrap());
    assert!(other.decrypt(&enc).is_err());
}

#[test]
fn vault_decrypt_rejects_corrupt_base64() {
    let vault = vault_with_zero_key();
    let mut enc = vault.encrypt("secret").unwrap();
    enc.ciphertext = "!!!not base64!!!".to_string();
    assert!(vault.decrypt(&enc).is_err());
}

// ---------------------------------------------------------------------------
// API keys
// ---------------------------------------------------------------------------

#[test]
fn generate_sys_key_has_expected_prefix() {
    let (raw, hash, key_id) = generate_api_key("root", KeyEnv::Sys);
    assert!(raw.starts_with("root_sys_prod_"), "got {raw}");
    assert_eq!(hash.len(), 64, "secret hash is sha256 hex");
    assert_eq!(key_id.len(), 8);
}

#[test]
fn generate_tenant_keys_encode_tenant_id() {
    let (raw_sk, _, _) = generate_api_key("acme", KeyEnv::Sk);
    assert!(raw_sk.starts_with("tnt_acme_sk_prod_"), "got {raw_sk}");

    let (raw_pk, _, _) = generate_api_key("acme", KeyEnv::Pk);
    assert!(raw_pk.starts_with("tnt_acme_pk_prod_"), "got {raw_pk}");

    let (raw_tnnt, _, _) = generate_api_key("acme", KeyEnv::Tnnt);
    assert!(raw_tnnt.starts_with("root_tnnt_acme_prod_"), "got {raw_tnnt}");
}

#[test]
fn generated_key_round_trips_through_parser() {
    let (raw, _, key_id) = generate_api_key("root", KeyEnv::Sys);
    let parsed = parse_and_validate_key(&raw).expect("generated key must parse");
    assert_eq!(parsed.issuer, "root");
    assert_eq!(parsed.tenant_id, "root");
    assert!(matches!(parsed.env, KeyEnv::Sys));
    assert_eq!(parsed.key_id, key_id);
}

#[test]
fn tenant_key_round_trips_with_correct_env() {
    let (raw, _, _) = generate_api_key("acme", KeyEnv::Sk);
    let parsed = parse_and_validate_key(&raw).unwrap();
    assert_eq!(parsed.tenant_id, "acme");
    assert!(matches!(parsed.env, KeyEnv::Sk));
}

#[test]
fn parser_rejects_key_with_bad_checksum() {
    let (raw, _, _) = generate_api_key("root", KeyEnv::Sys);
    // Flip the last character of the checksum.
    let mut tampered = raw.clone();
    let last = tampered.pop().unwrap();
    let replacement = if last == 'a' { 'b' } else { 'a' };
    tampered.push(replacement);
    assert!(parse_and_validate_key(&tampered).is_none());
}

#[test]
fn parser_rejects_structurally_invalid_key() {
    assert!(parse_and_validate_key("garbage").is_none());
    assert!(parse_and_validate_key("").is_none());
    assert!(parse_and_validate_key("a_b_c").is_none());
}

#[test]
fn parsed_key_secret_matches_hash() {
    let (raw, hash, _) = generate_api_key("root", KeyEnv::Sys);
    let parsed = parse_and_validate_key(&raw).unwrap();
    // The stored hash is sha256(secret).
    assert_eq!(apexkit_core::utils::sha256(&parsed.secret), hash);
}
