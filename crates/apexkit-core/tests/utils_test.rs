//! Unit tests for `apexkit_core::utils` — pure helpers used across the system.

use apexkit_core::utils::{
    apply_projection, generate_random_hex, get_temp_path, hmac_sha256, sha256, sha512, slugify,
    to_hex,
};
use serde_json::json;

#[test]
fn to_hex_encodes_lowercase_pairs() {
    assert_eq!(to_hex(&[0x00, 0x0f, 0xff, 0xab]), "000fffab");
    assert_eq!(to_hex(&[]), "");
}

#[test]
fn slugify_normalizes_text() {
    assert_eq!(slugify("Hello World!"), "hello-world");
    assert_eq!(slugify("  Multiple   Spaces  "), "multiple-spaces");
    assert_eq!(slugify("Already-Slugged"), "already-slugged");
    assert_eq!(slugify("!!!"), "");
    assert_eq!(slugify("CamelCase123"), "camelcase123");
}

#[test]
fn generate_random_hex_has_expected_length_and_uniqueness() {
    let a = generate_random_hex(4);
    let b = generate_random_hex(4);
    assert_eq!(a.len(), 8, "4 bytes -> 8 hex chars");
    assert_eq!(b.len(), 8);
    assert_ne!(a, b, "two random values should differ");
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn sha256_matches_known_vector() {
    // sha256("abc") is a well-known test vector.
    assert_eq!(
        sha256("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn sha512_is_128_hex_chars() {
    let h = sha512("abc");
    assert_eq!(h.len(), 128);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn hmac_sha256_is_key_dependent_and_deterministic() {
    let a = hmac_sha256("key1", "message");
    let b = hmac_sha256("key1", "message");
    let c = hmac_sha256("key2", "message");
    assert_eq!(a, b, "same key+data must produce the same HMAC");
    assert_ne!(a, c, "different keys must produce different HMACs");
    assert_eq!(a.len(), 64);
}

#[test]
fn get_temp_path_joins_and_strips_leading_slashes() {
    let p = get_temp_path("/sub/dir/file.txt");
    let s = p.to_string_lossy();
    assert!(s.ends_with("apexkit_tmp/sub/dir/file.txt") || s.ends_with("sub/dir/file.txt"));
    assert!(!s.contains("//"), "no double slashes from leading slash");
}

#[test]
fn apply_projection_include_mode_keeps_only_listed_fields() {
    let mut data = json!({"a": 1, "b": 2, "c": 3});
    apply_projection(&mut data, "a,c");
    assert_eq!(data, json!({"a": 1, "c": 3}));
}

#[test]
fn apply_projection_exclude_mode_drops_listed_fields() {
    let mut data = json!({"a": 1, "b": 2, "c": 3});
    apply_projection(&mut data, "-b");
    assert_eq!(data, json!({"a": 1, "c": 3}));
}

#[test]
fn apply_projection_star_and_empty_are_noops() {
    let original = json!({"a": 1, "b": 2});
    let mut d1 = original.clone();
    apply_projection(&mut d1, "*");
    assert_eq!(d1, original);

    let mut d2 = original.clone();
    apply_projection(&mut d2, "");
    assert_eq!(d2, original);
}
