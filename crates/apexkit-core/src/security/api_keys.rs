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
