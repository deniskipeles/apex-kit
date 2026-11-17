//! Functional tests for API-key and vector persistence stores in the SQLite DB.

mod common;

use apexkit_core::database::traits::{ApiKeyStore, CollectionStore, Db, VectorStore};
use common::{fresh_db, parse_schema, simple_schema};
use serde_json::json;

#[tokio::test]
async fn api_key_create_verify_delete() {
    let (dir, db) = fresh_db("apikeys").await;

    let (raw, key) = db
        .create_api_key("My Key", "root", "root", "sys", vec!["admin".into()], false)
        .await
        .unwrap();
    assert!(!raw.is_empty());
    assert_eq!(key.name, "My Key");
    assert_eq!(key.status, "active");

    // Parse the secret out of the raw key and verify it.
    let parsed = apexkit_core::security::api_keys::parse_and_validate_key(&raw).unwrap();
    let verified = db
        .verify_api_key(&key.tenant_id, &key.key_id, &parsed.secret)
        .await
        .unwrap();
    assert!(verified.is_some());

    // Wrong secret must not verify.
    let bad = db
        .verify_api_key(&key.tenant_id, &key.key_id, "deadbeef")
        .await
        .unwrap();
    assert!(bad.is_none());

    assert_eq!(db.list_api_keys().await.unwrap().len(), 1);

    db.update_api_key(key.id, Some("Renamed".into()), Some("disabled".into()), None, None)
        .await
        .unwrap();
    let keys = db.list_api_keys().await.unwrap();
    assert_eq!(keys[0].name, "Renamed");
    assert_eq!(keys[0].status, "disabled");

    db.delete_api_key(key.id).await.unwrap();
    assert!(db.list_api_keys().await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn vector_save_has_and_fetch() {
    let (dir, db) = fresh_db("vectors").await;
    let col_id = db
        .create_collection("docs", &Some(parse_schema(simple_schema())), None)
        .await
        .unwrap();
    let rec_id = db.create_record(col_id, &json!({"title": "doc"})).await.unwrap();

    let vec = vec![0.1f32, 0.2, 0.3, 0.4];
    db.save_vector(col_id, rec_id, "embedding", vec.clone(), "model-x")
        .await
        .unwrap();

    assert!(
        db.has_vector(col_id, rec_id, "embedding", "model-x")
            .await
            .unwrap()
    );
    assert!(
        !db.has_vector(col_id, rec_id, "embedding", "other-model")
            .await
            .unwrap()
    );

    let stored = db
        .get_vectors_for_collection(col_id, "model-x", 100, 0)
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].0, rec_id);
    assert_eq!(stored[0].1, "embedding");

    let per_record = db.get_record_vectors(col_id, rec_id).await.unwrap();
    assert_eq!(per_record.len(), 1);
    assert_eq!(per_record[0].field_name, "embedding");

    let flushed = db.flush_vectors_by_model("model-x").await.unwrap();
    assert_eq!(flushed, 1);
    assert!(
        !db.has_vector(col_id, rec_id, "embedding", "model-x")
            .await
            .unwrap()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn import_record_bypasses_generation() {
    let (dir, db) = fresh_db("import_rec").await;
    let col_id = db
        .create_collection("things", &Some(parse_schema(simple_schema())), None)
        .await
        .unwrap();

    db.import_record(col_id, 999, &json!({"title": "imported"}))
        .await
        .unwrap();

    let got = db.get_record(col_id, 999, None).await.unwrap().unwrap();
    assert_eq!(got.id, 999);
    assert_eq!(got.data["title"], "imported");

    let _ = std::fs::remove_dir_all(&dir);
}
