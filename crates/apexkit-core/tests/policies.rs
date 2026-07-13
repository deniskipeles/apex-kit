use apexkit_core::auth::{Claims, policies::check_access};
use apexkit_core::database::sqlite::connections::ApexKit;
use apexkit_core::database::traits::{Db, VectorProvider};
use apexkit_core::models::schema::{CollectionSchema, FieldDefinition, FieldType};
use rusqlite::Connection;
use serde_json::json;
use std::sync::Arc;

struct MockVectorProvider;

#[async_trait::async_trait]
impl VectorProvider for MockVectorProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> {
        Ok(vec![])
    }
    async fn embed_image(&self, _img: &str) -> Result<Vec<f32>, String> {
        Ok(vec![])
    }
    async fn embed_text_for_image_search(&self, _t: &str) -> Result<Vec<f32>, String> {
        Ok(vec![])
    }
    async fn search(
        &self,
        _c: i64,
        _f: &str,
        _v: &[f32],
        _l: usize,
    ) -> Result<Vec<(i64, f32)>, String> {
        Ok(vec![])
    }
    async fn index(&self, _c: i64, _r: i64, _f: &str, _v: &[f32]) -> Result<(), String> {
        Ok(())
    }
}

fn generate_temp_dir() -> std::path::PathBuf {
    let rand_id = uuid::Uuid::new_v4().to_string();
    let path = std::env::temp_dir().join(format!("apexkit_policy_test_{}", rand_id));
    std::fs::create_dir_all(&path).unwrap();
    path
}

async fn setup_test_db(base_path: &std::path::Path) -> Arc<dyn Db> {
    let core = Connection::open(base_path.join("core.db")).unwrap();
    let data = Connection::open(base_path.join("data.db")).unwrap();
    let log = Connection::open(base_path.join("logs.db")).unwrap();
    let sys = Connection::open(base_path.join("system.db")).unwrap();
    let vec = Connection::open(base_path.join("vectors.db")).unwrap();

    apexkit_core::database::sqlite::setup::setup_core(&core).unwrap();
    apexkit_core::database::sqlite::setup::setup_data(&data).unwrap();
    apexkit_core::database::sqlite::setup::setup_logs(&log).unwrap();
    apexkit_core::database::sqlite::setup::setup_sys(&sys).unwrap();
    apexkit_core::database::sqlite::setup::setup_vectors(&vec).unwrap();

    let path_str = base_path.to_str().unwrap().to_string();
    let kit = ApexKit::new(
        &path_str,
        core,
        data,
        log,
        sys,
        vec,
        Arc::new(MockVectorProvider),
        None,
        None,
        "root".to_string(),
    );
    Arc::new(kit)
}

#[tokio::test]
async fn test_string_policies() {
    let _dir = generate_temp_dir();

    // 1. Setup Dummy Data
    let claims = Claims {
        sub: "user@test.com".into(),
        uid: 42,
        role: "user".into(),
        exp: 9999999999,
        scope: "root".into(),
    };

    let admin_claims = Claims {
        sub: "admin@test.com".into(),
        uid: 99,
        role: "admin".into(),
        exp: 9999999999,
        scope: "root".into(),
    };

    let record_data = json!({ "owner_id": 42, "title": "My Post" });

    // 2. Test "public"
    assert!(check_access("public", None, None, None, None).await);

    // 3. Test "auth"
    assert!(!check_access("auth", None, None, None, None).await); // Fails (No claims)
    assert!(check_access("auth", Some(&claims), None, None, None).await); // Passes

    // 4. Test "admin"
    assert!(!check_access("admin", Some(&claims), None, None, None).await); // Fails (Is User)
    assert!(check_access("admin", Some(&admin_claims), None, None, None).await); // Passes (Is Admin)

    // 5. Test "owner:id"
    assert!(
        check_access(
            "owner:owner_id",
            Some(&claims),
            Some(&record_data),
            None,
            None
        )
        .await
    ); // Passes (42 == 42)
    assert!(
        !check_access(
            "owner:owner_id",
            Some(&claims),
            Some(&json!({"owner_id": 10})),
            None,
            None
        )
        .await
    ); // Fails (42 != 10)
}

#[tokio::test]
async fn test_json_basic_policies() {
    let _dir = generate_temp_dir();
    let record_data = json!({ "status": "published", "views": 100 });

    // 1. Simple Eq check
    let policy = r#"{"status": "published"}"#;
    assert!(check_access(policy, None, Some(&record_data), None, None).await);

    // 2. Operator checks ($gt)
    let policy_gt = r#"{"views": { "$gt": 50 }}"#;
    assert!(check_access(policy_gt, None, Some(&record_data), None, None).await);

    // 3. Operator checks ($gt fail)
    let policy_fail = r#"{"views": { "$gt": 200 }}"#;
    assert!(!check_access(policy_fail, None, Some(&record_data), None, None).await);
}

#[tokio::test]
async fn test_json_get_subquery_policies() {
    let dir = generate_temp_dir();
    let db = setup_test_db(&dir).await;

    // 1. Create a dummy "users_profiles" collection schema
    let mut profile_schema = CollectionSchema::default();

    let user_id_field = FieldDefinition {
        r#type: FieldType::Number,
        required: true,
        default: None,
        ose_indexed: false,
        sql_indexed: true,
        vectorize: false,
        auto: false,
        position: 0,
        uid: "user_id_uid".to_string(),
        unique: None,
        min: None,
        max: None,
        min_length: None,
        max_length: None,
        pattern: None,
        options: None,
        mime_types: None,
        max_size: None,
        dimension: None,
        relation_to: None,
    };

    let role_field = FieldDefinition {
        r#type: FieldType::String,
        required: true,
        default: None,
        ose_indexed: false,
        sql_indexed: true,
        vectorize: false,
        auto: false,
        position: 1,
        uid: "role_uid".to_string(),
        unique: None,
        min: None,
        max: None,
        min_length: None,
        max_length: None,
        pattern: None,
        options: None,
        mime_types: None,
        max_size: None,
        dimension: None,
        relation_to: None,
    };

    profile_schema
        .fields
        .insert("user_id".into(), user_id_field);
    profile_schema.fields.insert("role".into(), role_field);

    db.create_collection("users_profiles", &Some(profile_schema), None)
        .await
        .unwrap();

    // 2. Insert Profile Data
    // Tutor profile (users_profiles ID 1 corresponds to auth user ID 50)
    db.create_record(1, &json!({"user_id": 50, "role": "tutor"}))
        .await
        .unwrap();
    // Student profile (users_profiles ID 2 corresponds to auth user ID 60)
    db.create_record(1, &json!({"user_id": 60, "role": "student"}))
        .await
        .unwrap();

    // 3. Claims
    let tutor_claims = Claims {
        sub: "tutor@test.com".into(),
        uid: 50,
        role: "user".into(),
        exp: 9999999999,
        scope: "root".into(),
    };
    let student_claims = Claims {
        sub: "student@test.com".into(),
        uid: 60,
        role: "user".into(),
        exp: 9999999999,
        scope: "root".into(),
    };
    let random_claims = Claims {
        sub: "rando@test.com".into(),
        uid: 99,
        role: "user".into(),
        exp: 9999999999,
        scope: "root".into(),
    };

    // 4. Assignment Record (Assigned to Student profile ID 2, Tutor profile ID 1)
    let assignment_data = json!({
        "studentId": 2,
        "tutorId": 1
    });

    // 5. The Policy (Using @get to resolve users_profiles ID dynamically)
    let complex_policy = r#"
    {
      "$or": [
        {
          "studentId": {
            "$in": {
              "@get()": {
                "from": "users_profiles",
                "select": ["id"],
                "where": { "user_id": "@request.auth.id", "role": "student" }
              }
            }
          }
        },
        {
          "tutorId": {
            "$in": {
              "@get()": {
                "from": "users_profiles",
                "select": ["id"],
                "where": { "user_id": "@request.auth.id", "role": "tutor" }
              }
            }
          }
        }
      ]
    }
    "#;

    // EVALUATION:

    // A. Tutor (User 50) should pass because their profile ID is 1, which matches tutorId: 1
    let is_tutor_allowed = check_access(
        complex_policy,
        Some(&tutor_claims),
        Some(&assignment_data),
        None,
        Some(db.clone()),
    )
    .await;
    assert!(
        is_tutor_allowed,
        "Tutor should be allowed to update their assigned task"
    );

    // B. Student (User 60) should pass because their profile ID is 2, which matches studentId: 2
    let is_student_allowed = check_access(
        complex_policy,
        Some(&student_claims),
        Some(&assignment_data),
        None,
        Some(db.clone()),
    )
    .await;
    assert!(
        is_student_allowed,
        "Student should be allowed to update their own task"
    );

    // C. Random User (User 99) should fail because they don't own it
    let is_random_allowed = check_access(
        complex_policy,
        Some(&random_claims),
        Some(&assignment_data),
        None,
        Some(db.clone()),
    )
    .await;
    assert!(
        !is_random_allowed,
        "Random user should be blocked from updating"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
