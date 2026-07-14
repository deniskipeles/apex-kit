// =========================== apex-kit/crates/apexkit-core/tests/policies.rs start here ===========================
use apexkit_core::auth::{Claims, policies::check_access, policies::compile_to_sql};
use apexkit_core::database::sqlite::connections::ApexKit;
use apexkit_core::database::traits::{Db, VectorProvider};
use apexkit_core::models::schema::{CollectionSchema, FieldDefinition, FieldType};
use rusqlite::Connection;
use serde_json::json;
use std::sync::Arc;

struct MockVectorProvider;

#[async_trait::async_trait]
impl VectorProvider for MockVectorProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> { Ok(vec![]) }
    async fn embed_image(&self, _img: &str) -> Result<Vec<f32>, String> { Ok(vec![]) }
    async fn embed_text_for_image_search(&self, _t: &str) -> Result<Vec<f32>, String> { Ok(vec![]) }
    async fn search(&self, _c: i64, _f: &str, _v: &[f32], _l: usize) -> Result<Vec<(i64, f32)>, String> { Ok(vec![]) }
    async fn index(&self, _c: i64, _r: i64, _f: &str, _v: &[f32]) -> Result<(), String> { Ok(()) }
}

fn generate_temp_dir() -> std::path::PathBuf {
    let rand_id = uuid::Uuid::new_v4().to_string();
    let path = std::env::temp_dir().join(format!("apexkit_policy_stress_{}", rand_id));
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
        &path_str, core, data, log, sys, vec,
        Arc::new(MockVectorProvider), None, None, "root".to_string(),
    );
    Arc::new(kit)
}

#[tokio::test]
async fn test_string_policies() {
    let _dir = generate_temp_dir();
    
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

    assert!(check_access("public", None, None, None, None).await);
    assert!(!check_access("auth", None, None, None, None).await);
    assert!(check_access("auth", Some(&claims), None, None, None).await);
    assert!(!check_access("admin", Some(&claims), None, None, None).await);
    assert!(check_access("admin", Some(&admin_claims), None, None, None).await);
    assert!(check_access("owner:owner_id", Some(&claims), Some(&record_data), None, None).await);
    assert!(!check_access("owner:owner_id", Some(&claims), Some(&json!({"owner_id": 10})), None, None).await);
}

#[tokio::test]
async fn test_json_basic_policies() {
    let _dir = generate_temp_dir();
    let record_data = json!({ "status": "published", "views": 100, "category": "math" });

    // 1. Exact Match
    let policy = r#"{"status": "published"}"#;
    assert!(check_access(policy, None, Some(&record_data), None, None).await);

    // 2. Operator checks ($gt)
    let policy_gt = r#"{"views": { "$gt": 50 }}"#;
    assert!(check_access(policy_gt, None, Some(&record_data), None, None).await);

    let policy_fail = r#"{"views": { "$gt": 200 }}"#;
    assert!(!check_access(policy_fail, None, Some(&record_data), None, None).await);

    // 3. Testing $in
    let policy_in = r#"{"category": { "$in": ["science", "math"] }}"#;
    assert!(check_access(policy_in, None, Some(&record_data), None, None).await);

    // 4. Testing $nin (Not In)
    let policy_nin = r#"{"category": { "$nin": ["science", "history"] }}"#;
    assert!(check_access(policy_nin, None, Some(&record_data), None, None).await); // True (math is not science or history)

    let policy_nin_fail = r#"{"category": { "$nin": ["math", "history"] }}"#;
    assert!(!check_access(policy_nin_fail, None, Some(&record_data), None, None).await); // False (math IS in the excluded list)
}

#[tokio::test]
async fn test_json_get_subquery_policies() {
    let dir = generate_temp_dir();
    let db = setup_test_db(&dir).await;

    let mut profile_schema = CollectionSchema::default();

    let user_id_field = FieldDefinition {
        r#type: FieldType::Number, required: true, default: None, ose_indexed: false, sql_indexed: true, vectorize: false, auto: false, position: 0, uid: "user_id_uid".to_string(), unique: None, min: None, max: None, min_length: None, max_length: None, pattern: None, options: None, mime_types: None, max_size: None, dimension: None, relation_to: None,
    };

    let role_field = FieldDefinition {
        r#type: FieldType::String, required: true, default: None, ose_indexed: false, sql_indexed: true, vectorize: false, auto: false, position: 1, uid: "role_uid".to_string(), unique: None, min: None, max: None, min_length: None, max_length: None, pattern: None, options: None, mime_types: None, max_size: None, dimension: None, relation_to: None,
    };

    profile_schema.fields.insert("user_id".into(), user_id_field);
    profile_schema.fields.insert("role".into(), role_field);
    
    db.create_collection("users_profiles", &Some(profile_schema), None).await.unwrap();

    db.create_record(1, &json!({"user_id": 50, "role": "tutor"})).await.unwrap();
    db.create_record(1, &json!({"user_id": 60, "role": "student"})).await.unwrap();

    let tutor_claims = Claims { sub: "tutor@test.com".into(), uid: 50, role: "user".into(), exp: 9999999999, scope: "root".into() };
    let student_claims = Claims { sub: "student@test.com".into(), uid: 60, role: "user".into(), exp: 9999999999, scope: "root".into() };
    let random_claims = Claims { sub: "rando@test.com".into(), uid: 99, role: "user".into(), exp: 9999999999, scope: "root".into() };

    let assignment_data = json!({
        "studentId": 2,
        "tutorId": 1
    });

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

    let is_tutor_allowed = check_access(complex_policy, Some(&tutor_claims), Some(&assignment_data), None, Some(db.clone())).await;
    assert!(is_tutor_allowed, "Tutor should be allowed to update their assigned task");

    let is_student_allowed = check_access(complex_policy, Some(&student_claims), Some(&assignment_data), None, Some(db.clone())).await;
    assert!(is_student_allowed, "Student should be allowed to update their own task");

    let is_random_allowed = check_access(complex_policy, Some(&random_claims), Some(&assignment_data), None, Some(db.clone())).await;
    assert!(!is_random_allowed, "Random user should be blocked from updating");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_nested_get_subquery_policies() {
    let dir = generate_temp_dir();
    let db = setup_test_db(&dir).await;

    // 1. Create Profile Schema (Collection ID: 1)
    let mut profile_schema = CollectionSchema::default();
    profile_schema.fields.insert("user_id".into(), FieldDefinition {
        r#type: FieldType::Number, required: true, default: None, ose_indexed: false, sql_indexed: true, vectorize: false, auto: false, position: 0, uid: "user_uid".to_string(), unique: None, min: None, max: None, min_length: None, max_length: None, pattern: None, options: None, mime_types: None, max_size: None, dimension: None, relation_to: None,
    });
    db.create_collection("users_profiles", &Some(profile_schema), None).await.unwrap();

    // 2. Create Bids Schema (Collection ID: 2)
    let mut bids_schema = CollectionSchema::default();
    bids_schema.fields.insert("tutorId".into(), FieldDefinition {
        r#type: FieldType::Number, required: true, default: None, ose_indexed: false, sql_indexed: true, vectorize: false, auto: false, position: 0, uid: "tut_uid".to_string(), unique: None, min: None, max: None, min_length: None, max_length: None, pattern: None, options: None, mime_types: None, max_size: None, dimension: None, relation_to: None,
    });
    bids_schema.fields.insert("assignmentId".into(), FieldDefinition {
        r#type: FieldType::Number, required: true, default: None, ose_indexed: false, sql_indexed: true, vectorize: false, auto: false, position: 1, uid: "asg_uid".to_string(), unique: None, min: None, max: None, min_length: None, max_length: None, pattern: None, options: None, mime_types: None, max_size: None, dimension: None, relation_to: None,
    });
    db.create_collection("bids", &Some(bids_schema), None).await.unwrap();

    // 3. Populate Data
    // Tutor profile record ID: 1, Auth User ID: 50
    db.create_record(1, &json!({"user_id": 50})).await.unwrap();
    // Bid ID: 1, placed by tutor profile ID 1, for assignment ID 999
    db.create_record(2, &json!({"tutorId": 1, "assignmentId": 999})).await.unwrap();

    let tutor_claims = Claims { sub: "tutor@test.com".into(), uid: 50, role: "user".into(), exp: 9999999999, scope: "root".into() };

    // 4. Policy containing nested @get() query wrapped in @log() for debugging
    let nested_policy = r#"
    {
      "@log()": {
        "id": {
          "$in": {
            "@get()": {
              "from": "bids",
              "select": ["assignmentId"],
              "where": {
                "tutorId": {
                  "$in": {
                    "@get()": {
                      "from": "users_profiles",
                      "select": ["id"],
                      "where": { "user_id": "@request.auth.id" }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
    "#;

    // Verify evaluation passes (The nested query successfully resolves first the profile ID (1), 
    // then uses it to query the bids, returning [999] as the allowed assignment IDs)
    let is_allowed = check_access(nested_policy, Some(&tutor_claims), Some(&json!({"id": 999})), None, Some(db.clone())).await;
    assert!(is_allowed, "Tutor who placed a bid on assignment 999 should be allowed");

    // Fails for unbidded assignments (e.g., ID 888)
    let is_blocked = check_access(nested_policy, Some(&tutor_claims), Some(&json!({"id": 888})), None, Some(db.clone())).await;
    assert!(!is_blocked, "Tutor who has not bidded on 888 should be blocked");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_policy_sql_compilation() {
    let tutor_claims = Claims { sub: "tutor@test.com".into(), uid: 50, role: "user".into(), exp: 9999999999, scope: "root".into() };
    
    // Test compilation of simple string rule
    let string_rule = "owner:studentId";
    let compiled_sql = compile_to_sql(string_rule, Some(&tutor_claims), None, None).await.unwrap();
    assert_eq!(compiled_sql, "json_extract(records.data, '$.studentId') = '50'");

    // Test compilation of nested logical Operators
    let logical_rule = "public && auth";
    let compiled_logical = compile_to_sql(logical_rule, Some(&tutor_claims), None, None).await.unwrap();
    assert_eq!(compiled_logical, "(1=1 AND 1=1)");

    // Test SQL Compilation of $nin (Not In)
    let json_nin_rule = r#"{"role": {"$nin": ["guest", "banned"]}}"#;
    let compiled_nin = compile_to_sql(json_nin_rule, None, None, None).await.unwrap();
    
    // Check if the boolean AND NOT wrapping is correctly distributed across the array elements
    assert!(compiled_nin.contains("NOT (records.data ->> 'role' = 'guest'"));
    assert!(compiled_nin.contains("AND NOT (records.data ->> 'role' = 'banned'"));
}

#[tokio::test]
async fn test_log_directive_policies() {
    let dir = generate_temp_dir();
    let db = setup_test_db(&dir).await;

    let record_data = json!({ "status": "published", "count": 10 });

    // 1. Wrapping policy in @log(SQL) to test database logging
    let policy_with_log_sql = r#"
    {
        "@log(SQL)": {
            "status": "published",
            "count": {"$gt": 5}
        }
    }
    "#;
    
    // Pass the actual DB instance so the debugger can commit to _system_logs
    let is_allowed = check_access(policy_with_log_sql, None, Some(&record_data), None, Some(db.clone())).await;
    assert!(is_allowed, "@log directive should not interfere with memory evaluation");

    // Fetch the system logs from the DB to verify persistent output
    let (logs, total) = db.list_paginated_logs("system", 1, 10, None, Some("policy_debugger".to_string()), None).await.unwrap();
    assert!(total > 0, "Policy log should be stored inside _system_logs table");
    assert!(
        logs[0]["message"].as_str().unwrap().contains("Log(SQL)"),
        "The log message must contain the parsed SQL representation"
    );

    // 2. Ensure compile_to_sql still works correctly and strips @log()
    let compiled = compile_to_sql(policy_with_log_sql, None, None, Some(db.clone())).await.unwrap();
    
    assert!(compiled.contains("records.data ->> 'status' = 'published'"));
    assert!(compiled.contains("records.data ->> 'count' > 5"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_request_and_record_spec_resolution() {
    let dir = generate_temp_dir();
    let db = setup_test_db(&dir).await;

    // 1. Create a users_profiles collection (ID: 1)
    let mut profile_schema = CollectionSchema::default();
    profile_schema.fields.insert("user_id".into(), FieldDefinition {
        r#type: FieldType::Number, required: true, default: None, ose_indexed: false, sql_indexed: true, vectorize: false, auto: false, position: 0, uid: "user_id_uid".to_string(), unique: None, min: None, max: None, min_length: None, max_length: None, pattern: None, options: None, mime_types: None, max_size: None, dimension: None, relation_to: None,
    });
    db.create_collection("users_profiles", &Some(profile_schema), None).await.unwrap();

    // Insert profile matching auth user ID 10
    db.create_record(1, &json!({"user_id": 10})).await.unwrap(); // Profile ID resolved will be 1

    // Sleep to allow the asynchronous WriteManager batcher to flush to the SQLite disk
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let student_claims = Claims { sub: "student@test.com".into(), uid: 10, role: "user".into(), exp: 9999999999, scope: "root".into() };

    // 2. Incoming payload mimicking a newly created message record
    let incoming_request_data = json!({
        "chatRoomId": "1_2",
        "senderId": 1
    });

    // 3. A creation policy matching senderId in the client request to their verified profile ID
    let policy = r#"
    {"@log(SQL)":{
      "@request.record.senderId": {
        "$in": {
          "@get()": {
            "from": "users_profiles",
            "select": ["id"],
            "where": { "user_id": "@request.auth.id" }
          }
        }
      }
  }}
    "#;

    // Evaluates with record_data as None (on Create), forcing evaluation on the incoming request payload
    let is_allowed = check_access(policy, Some(&student_claims), None, Some(&incoming_request_data), Some(db.clone())).await;
    assert!(is_allowed, "The incoming request payload field senderId (1) matches their profile ID (1) and should pass");

    let _ = std::fs::remove_dir_all(&dir);
}