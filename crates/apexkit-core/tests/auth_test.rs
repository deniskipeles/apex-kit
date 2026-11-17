//! Unit + functional tests for authentication primitives: password hashing,
//! JWT round-trips, and the policy engine (string and JSON policies).

use apexkit_core::auth::{Claims, create_jwt, decode_jwt, hash_password, verify_password};
use apexkit_core::auth::policies::{check_access, compile_to_sql};
use serde_json::json;

fn claims(role: &str, uid: i64) -> Claims {
    Claims {
        sub: "user@example.com".into(),
        uid,
        role: role.into(),
        exp: 9_999_999_999,
        scope: "root".into(),
    }
}

// ---------------------------------------------------------------------------
// Password hashing
// ---------------------------------------------------------------------------

#[test]
fn hash_and_verify_password_round_trip() {
    let hash = hash_password("s3cr3t-password").unwrap();
    assert!(verify_password("s3cr3t-password", &hash));
    assert!(!verify_password("wrong-password", &hash));
}

#[test]
fn password_hashes_are_salted_and_unique() {
    let a = hash_password("same").unwrap();
    let b = hash_password("same").unwrap();
    assert_ne!(a, b, "bcrypt must salt each hash differently");
}

#[test]
fn verify_password_rejects_garbage_hash() {
    assert!(!verify_password("x", "not-a-bcrypt-hash"));
}

// ---------------------------------------------------------------------------
// JWT
// ---------------------------------------------------------------------------

#[test]
fn jwt_round_trip_preserves_claims() {
    let token = create_jwt(42, "alice@example.com", "admin", "root").unwrap();
    let decoded = decode_jwt(&token).unwrap();
    assert_eq!(decoded.uid, 42);
    assert_eq!(decoded.sub, "alice@example.com");
    assert_eq!(decoded.role, "admin");
    assert_eq!(decoded.scope, "root");
    assert!(decoded.exp > 0);
}

#[test]
fn jwt_decode_rejects_tampered_token() {
    let token = create_jwt(1, "a@b.com", "user", "root").unwrap();
    let tampered = format!("{}x", token);
    assert!(decode_jwt(&tampered).is_err());
}

#[test]
fn jwt_decode_rejects_garbage() {
    assert!(decode_jwt("not.a.jwt").is_err());
}

// ---------------------------------------------------------------------------
// Policy engine — string policies
// ---------------------------------------------------------------------------

#[tokio::test]
async fn public_policy_allows_everyone() {
    assert!(check_access("public", None, None, None, None).await);
}

#[tokio::test]
async fn auth_policy_requires_a_user() {
    assert!(!check_access("auth", None, None, None, None).await);
    assert!(check_access("auth", Some(&claims("user", 1)), None, None, None).await);
}

#[tokio::test]
async fn admin_policy_requires_admin_role() {
    assert!(!check_access("admin", Some(&claims("user", 1)), None, None, None).await);
    assert!(check_access("admin", Some(&claims("admin", 1)), None, None, None).await);
}

#[tokio::test]
async fn admin_role_bypasses_any_policy() {
    // Even a non-matching owner policy must pass for an admin.
    let rec = json!({"owner_id": 999});
    assert!(
        check_access(
            "owner:owner_id",
            Some(&claims("admin", 1)),
            Some(&rec),
            None,
            None
        )
        .await
    );
}

#[tokio::test]
async fn owner_policy_matches_record_owner() {
    let rec = json!({"owner_id": 42});
    assert!(
        check_access(
            "owner:owner_id",
            Some(&claims("user", 42)),
            Some(&rec),
            None,
            None
        )
        .await
    );
    assert!(
        !check_access(
            "owner:owner_id",
            Some(&claims("user", 7)),
            Some(&rec),
            None,
            None
        )
        .await
    );
}

#[tokio::test]
async fn empty_policy_is_denied() {
    assert!(!check_access("", Some(&claims("admin", 1)), None, None, None).await);
    assert!(!check_access("   ", None, None, None, None).await);
}

#[tokio::test]
async fn json_policy_evaluates_against_record() {
    let policy = r#"{"status": "published"}"#;
    let rec = json!({"status": "published"});
    assert!(
        check_access(policy, Some(&claims("user", 1)), Some(&rec), None, None).await
    );
    let rec2 = json!({"status": "draft"});
    assert!(
        !check_access(policy, Some(&claims("user", 1)), Some(&rec2), None, None).await
    );
}

// ---------------------------------------------------------------------------
// Policy engine — SQL compilation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compile_public_policy_to_always_true() {
    let sql = compile_to_sql("public", None, None, None).await.unwrap();
    assert_eq!(sql, "1=1");
}

#[tokio::test]
async fn compile_admin_policy_for_admin_is_always_true() {
    let sql = compile_to_sql("admin", Some(&claims("admin", 1)), None, None)
        .await
        .unwrap();
    assert_eq!(sql, "1=1");
}

#[tokio::test]
async fn compile_json_policy_produces_sql() {
    let sql = compile_to_sql(r#"{"status": "active"}"#, Some(&claims("user", 1)), None, None)
        .await
        .unwrap();
    assert!(!sql.is_empty());
    // The compiled clause must reference the JSON column for `status`.
    assert!(sql.contains("status"), "got {sql}");
}

#[tokio::test]
async fn compile_owner_policy_produces_sql() {
    let sql = compile_to_sql("owner:owner_id", Some(&claims("user", 42)), None, None)
        .await
        .unwrap();
    assert!(!sql.is_empty(), "owner policy must compile to a clause");
}
