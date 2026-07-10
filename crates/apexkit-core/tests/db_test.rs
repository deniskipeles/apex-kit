use apexkit_core::database::sqlite::connections::ApexKit;
use apexkit_core::database::traits::{
    CollectionStore, RecordStore, UserStore, ConfigStore, ApiKeyStore, AuditStore, VectorProvider
};
use apexkit_core::models::schema::CollectionSchema;
use rusqlite::Connection;
use serde_json::json;
use std::sync::Arc;

struct MockVectorProvider;

#[async_trait::async_trait]
impl VectorProvider for MockVectorProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> {
        Ok(vec![0.1, 0.2, 0.3])
    }
    async fn embed_image(&self, _base64_image: &str) -> Result<Vec<f32>, String> {
        Ok(vec![0.4, 0.5, 0.6])
    }
    async fn embed_text_for_image_search(&self, _text: &str) -> Result<Vec<f32>, String> {
        Ok(vec![0.7, 0.8, 0.9])
    }
    async fn search(
        &self,
        _col_id: i64,
        _field: &str,
        _vec: &[f32],
        _limit: usize,
    ) -> Result<Vec<(i64, f32)>, String> {
        Ok(vec![(1, 0.99)])
    }
    async fn index(
        &self,
        _col_id: i64,
        _rec_id: i64,
        _field: &str,
        _vec: &[f32],
    ) -> Result<(), String> {
        Ok(())
    }
}

fn generate_temp_dir() -> std::path::PathBuf {
    let rand_id = uuid::Uuid::new_v4().to_string();
    let path = std::env::temp_dir().join(format!("apexkit_test_{}", rand_id));
    std::fs::create_dir_all(&path).unwrap();
    path
}

async fn create_test_db(base_path: &std::path::Path) -> ApexKit {
    let core = Connection::open(base_path.join("core.db")).unwrap();
    let data = Connection::open(base_path.join("data.db")).unwrap();
    let log = Connection::open(base_path.join("logs.db")).unwrap();
    let sys = Connection::open(base_path.join("system.db")).unwrap();
    let vec = Connection::open(base_path.join("vectors.db")).unwrap();

    apexkit_core::database::sqlite::setup::apply_pragmas(&core).unwrap();
    apexkit_core::database::sqlite::setup::apply_pragmas(&data).unwrap();
    apexkit_core::database::sqlite::setup::apply_pragmas(&log).unwrap();
    apexkit_core::database::sqlite::setup::apply_pragmas(&sys).unwrap();
    apexkit_core::database::sqlite::setup::apply_pragmas(&vec).unwrap();

    apexkit_core::database::sqlite::setup::setup_core(&core).unwrap();
    apexkit_core::database::sqlite::setup::setup_data(&data).unwrap();
    apexkit_core::database::sqlite::setup::setup_logs(&log).unwrap();
    apexkit_core::database::sqlite::setup::setup_sys(&sys).unwrap();
    apexkit_core::database::sqlite::setup::setup_vectors(&vec).unwrap();

    let path_str = base_path.to_str().unwrap().to_string();
    ApexKit::new(
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
    )
}

#[tokio::test]
async fn test_database_collection_lifecycle() {
    let dir = generate_temp_dir();
    let db = create_test_db(&dir).await;

    // 1. Create collection with schema
    let schema_val = json!({
        "fields": {
            "title": {
                "type": "text",
                "required": true,
                "ose_indexed": false,
                "sql_indexed": false,
                "vectorize": false,
                "auto": false,
                "position": 0,
                "uid": "title_uid"
            }
        },
        "policies": {
            "read": "public",
            "create": "auth",
            "update": "admin",
            "delete": "admin"
        }
    });
    let schema: CollectionSchema = serde_json::from_value(schema_val).unwrap();

    let col_id = db
        .create_collection("posts", &Some(schema), None)
        .await
        .unwrap();
    assert!(col_id > 0);

    // 2. Fetch collection
    let col = db.get_collection(col_id).await.unwrap().unwrap();
    assert_eq!(col.name, "posts");

    // 3. List collections
    let list = db.list_collections().await.unwrap();
    assert!(!list.is_empty());
    assert_eq!(list[0].name, "posts");

    // 4. Update collection name
    let updated_col = db
        .update_collection(col_id, Some("articles".to_string()), None)
        .await
        .unwrap();
    assert_eq!(updated_col.name, "articles");

    // 5. Delete collection
    db.delete_collection(col_id).await.unwrap();
    let col_after_delete = db.get_collection(col_id).await.unwrap();
    assert!(col_after_delete.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_records() {
    let dir = generate_temp_dir();
    let db = create_test_db(&dir).await;

    let schema_val = json!({
        "fields": {
            "name": {
                "type": "string",
                "required": true,
                "ose_indexed": false,
                "sql_indexed": false,
                "vectorize": false,
                "auto": false,
                "position": 0,
                "uid": "name_uid"
            },
            "price": {
                "type": "number",
                "required": true,
                "ose_indexed": false,
                "sql_indexed": false,
                "vectorize": false,
                "auto": false,
                "position": 1,
                "uid": "price_uid"
            }
        },
        "policies": {
            "read": "public",
            "create": "auth",
            "update": "admin",
            "delete": "admin"
        }
    });
    let schema: CollectionSchema = serde_json::from_value(schema_val).unwrap();

    let col_id = db.create_collection("items", &Some(schema), None).await.unwrap();

    // 1. Create record
    let data = json!({ "name": "Apple", "price": 1.2 });
    let rec_id = db.create_record(col_id, &data).await.unwrap();
    assert!(rec_id > 0);

    // 2. Get record
    let rec = db.get_record(col_id, rec_id, None).await.unwrap().unwrap();
    assert_eq!(rec.data["name"], "Apple");

    // 3. List records
    let list = db
        .list_records(col_id, Default::default())
        .await
        .unwrap();
    assert_eq!(list.total, 1);
    assert_eq!(list.items[0].data["name"], "Apple");

    // 4. Update record
    let update_data = json!({ "name": "Gala Apple", "price": 1.5 });
    let updated_rec = db
        .update_record(col_id, rec_id, &update_data)
        .await
        .unwrap();
    assert_eq!(updated_rec.data["name"], "Gala Apple");

    // 5. Delete record
    db.delete_record(col_id, rec_id).await.unwrap();
    let rec_after_delete = db.get_record(col_id, rec_id, None).await.unwrap();
    assert!(rec_after_delete.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_users() {
    let dir = generate_temp_dir();
    let db = create_test_db(&dir).await;

    // 1. Create user
    let user = db
        .create_user("user@example.com", "hash", "user", Some(json!({"age": 30})))
        .await
        .unwrap();
    assert_eq!(user.email, "user@example.com");

    // 2. Get user by email
    let user_by_email = db.get_user_by_email("user@example.com").await.unwrap().unwrap();
    assert_eq!(user_by_email.role, "user");

    // 3. Count users
    let count = db.count_users(None).await.unwrap();
    assert_eq!(count, 1);

    // 4. Update user
    let updated = db
        .update_user(
            user.id,
            Some("new_email@example.com".to_string()),
            Some("admin".to_string()),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(updated.email, "new_email@example.com");
    assert_eq!(updated.role, "admin");

    // 5. Delete user
    db.delete_user(user.id).await.unwrap();
    let after_delete = db.get_user_by_email("new_email@example.com").await.unwrap();
    assert!(after_delete.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_configs() {
    let dir = generate_temp_dir();
    let db = create_test_db(&dir).await;

    // 1. Set config
    db.set_config("smtp_host", &json!("smtp.mailtrap.io"), false)
        .await
        .unwrap();

    // 2. Get config
    let host = db.get_config("smtp_host").await.unwrap().unwrap();
    assert_eq!(host, "smtp.mailtrap.io");

    // 3. List configs
    let list = db.list_configs().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].key, "smtp_host");

    // 4. Delete config
    db.delete_config("smtp_host").await.unwrap();
    let host_after_delete = db.get_config("smtp_host").await.unwrap();
    assert!(host_after_delete.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_api_keys() {
    let dir = generate_temp_dir();
    let db = create_test_db(&dir).await;

    // 1. Create API key with a valid key_env (e.g. "sys")
    let (plain_key, api_key) = db
        .create_api_key(
            "Test Key",
            "tenant_abc",
            "issuer_xyz",
            "sys",
            vec!["admin".to_string()],
            false,
        )
        .await
        .unwrap();
    assert!(!plain_key.is_empty());
    assert_eq!(api_key.name, "Test Key");

    // 2. List API keys
    let list = db.list_api_keys().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Test Key");

    // 3. Parse and validate the API key payload to extract the actual secret part
    let parsed_key = apexkit_core::security::api_keys::parse_and_validate_key(&plain_key).unwrap();

    // 4. Verify API key using the extracted secret
    let verified = db
        .verify_api_key(&api_key.tenant_id, &api_key.key_id, &parsed_key.secret)
        .await
        .unwrap();
    assert!(verified.is_some());

    // 5. Update API Key details
    db.update_api_key(api_key.id, Some("Renamed Key".to_string()), None, None, None)
        .await
        .unwrap();

    let list_after_update = db.list_api_keys().await.unwrap();
    assert_eq!(list_after_update[0].name, "Renamed Key");

    // 6. Delete API Key
    db.delete_api_key(api_key.id).await.unwrap();
    let list_after_delete = db.list_api_keys().await.unwrap();
    assert!(list_after_delete.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_database_audit_logs() {
    let dir = generate_temp_dir();
    let db = create_test_db(&dir).await;

    // 1. Log audit event
    db.log_audit_event("info", "User Registered", "auth", Some(json!({"user_id": 1})))
        .await
        .unwrap();

    // 2. List audit logs
    let logs = db.list_audit_logs().await.unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0]["message"], "User Registered");

    // 3. List paginated logs
    let (paginated, total) = db
        .list_paginated_logs("audit", 1, 10, None, None, None)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(paginated.len(), 1);
    assert_eq!(paginated[0]["message"], "User Registered");

    let _ = std::fs::remove_dir_all(&dir);
}
