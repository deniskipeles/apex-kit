use regex::Regex;
use std::fmt::Write;
use ring::{rand::{SystemRandom, SecureRandom}, digest, hmac};

/// Converts a byte slice to a lowercase hex string
pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

/// Converts a text string into a URL-friendly slug.
/// Example: "Hello World!" -> "hello-world"
pub fn slugify(text: &str) -> String {
    let re_invalid = Regex::new(r"[^a-z0-9]+").unwrap();
    let lower = text.to_lowercase();
    let slug = re_invalid.replace_all(&lower, "-");
    slug.trim_matches('-').to_string()
}

/// Generates a cryptographically secure random hex string of specified byte length.
/// Example: len=4 -> 8 hex characters (e.g. "a1b2c3d4")
pub fn generate_random_hex(len: usize) -> String {
    let rng = SystemRandom::new();
    let mut bytes = vec![0u8; len];
    rng.fill(&mut bytes).unwrap();
    to_hex(&bytes)
}

/// Computes SHA256 hash of input string
pub fn sha256(text: &str) -> String {
    let hash = digest::digest(&digest::SHA256, text.as_bytes());
    to_hex(hash.as_ref())
}

/// Computes SHA512 hash of input string
pub fn sha512(text: &str) -> String {
    let hash = digest::digest(&digest::SHA512, text.as_bytes());
    to_hex(hash.as_ref())
}

/// Computes HMAC-SHA256 signature
pub fn hmac_sha256(key: &str, data: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key.as_bytes());
    let tag = hmac::sign(&key, data.as_bytes());
    to_hex(tag.as_ref())
}