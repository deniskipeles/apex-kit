//! Functional tests for the schema-to-DDL index compiler.
//!
//! These exercise the refactor that replaced the `_unique_values` side-table
//! with real SQLite expression indexes over `records`. They drive the public
//! `Db` trait end to end and then inspect the resulting `data.db` directly, so
//! both the observable behaviour (rejected duplicates) and the physical
//! artifacts (indexes, `_applied_indexes`) are asserted.

mod common;

use apexkit_core::database::traits::{CollectionStore, RecordStore};
use common::{fresh_db, parse_schema};
use rusqlite::Connection;
use serde_json::{json, Value};

/// Opens `data.db` read-only for catalogue inspection.
fn open_data(dir: &std::path::Path) -> Connection {
    Connection::open(dir.join("data.db")).unwrap()
}

fn index_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='index' ORDER BY name")
        .unwrap();
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    rows
}

fn applied_index_names(conn: &Connection) -> Vec<String> {
    // The metadata table is created lazily on the first index write, so a
    // collection without indexes legitimately has no table at all.
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='_applied_indexes'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !exists {
        return Vec::new();
    }

    let mut stmt = conn
        .prepare("SELECT index_name FROM _applied_indexes ORDER BY index_name")
        .unwrap();
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    rows
}

fn unique_email_schema() -> Value {
    json!({
        "fields": { "email": { "type": "email", "required": true, "unique": true } },
        "policies": { "read": "public", "create": "public", "update": "public", "delete": "public" }
    })
}

#[tokio::test]
async fn unique_field_creates_index_and_records_metadata() {
    let (dir, db) = fresh_db("idx_unique_create").await;
    let schema = parse_schema(unique_email_schema());

    let col_id = db
        .create_collection("contacts", &Some(schema), None)
        .await
        .unwrap();

    let conn = open_data(&dir);
    let expected = format!("idx_u_{col_id}_email");
    assert!(
        index_names(&conn).contains(&expected),
        "physical index {expected} must exist: {:?}",
        index_names(&conn)
    );
    assert_eq!(applied_index_names(&conn), vec![expected.clone()]);

    // The index must be a UNIQUE index (uniqueness is its job).
    let sql: String = conn
        .query_row(
            &format!("SELECT sql FROM sqlite_master WHERE type='index' AND name='{expected}'"),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(sql.contains("CREATE UNIQUE INDEX"), "got {sql}");
    assert!(sql.contains("json_extract(data, '$.email')"), "got {sql}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn duplicate_insert_is_rejected_with_stable_error() {
    let (dir, db) = fresh_db("idx_unique_dup").await;
    let schema = parse_schema(unique_email_schema());
    let col_id = db
        .create_collection("contacts", &Some(schema), None)
        .await
        .unwrap();

    db.create_record(col_id, &json!({"email": "a@b.com"}))
        .await
        .unwrap();

    let err = db
        .create_record(col_id, &json!({"email": "a@b.com"}))
        .await
        .expect_err("duplicate must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("Unique constraint violation"), "got {msg}");
    assert!(msg.contains("email"), "should name the field: {msg}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn duplicate_update_is_rejected() {
    let (dir, db) = fresh_db("idx_unique_update").await;
    let schema = parse_schema(unique_email_schema());
    let col_id = db
        .create_collection("contacts", &Some(schema), None)
        .await
        .unwrap();

    let first = db
        .create_record(col_id, &json!({"email": "one@b.com"}))
        .await
        .unwrap();
    let second = db
        .create_record(col_id, &json!({"email": "two@b.com"}))
        .await
        .unwrap();

    // Colliding an update with an existing value must fail atomically.
    let err = db
        .update_record(col_id, second, &json!({"email": "one@b.com"}))
        .await
        .expect_err("colliding update must fail");
    assert!(
        err.to_string().contains("Unique constraint violation"),
        "got {err}"
    );

    // ...and the original rows must be intact.
    let got = db.get_record(col_id, second, None).await.unwrap().unwrap();
    assert_eq!(got.data["email"], "two@b.com");
    let got_first = db.get_record(col_id, first, None).await.unwrap().unwrap();
    assert_eq!(got_first.data["email"], "one@b.com");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn null_values_do_not_collide() {
    let (dir, db) = fresh_db("idx_unique_nulls").await;
    let schema = parse_schema(json!({
        "fields": { "email": { "type": "email", "required": false, "unique": true } }
    }));
    let col_id = db
        .create_collection("contacts", &Some(schema), None)
        .await
        .unwrap();

    // SQLite allows many NULLs in a unique index; missing values must not clash.
    db.create_record(col_id, &json!({"email": null}))
        .await
        .unwrap();
    db.create_record(col_id, &json!({})).await.unwrap();

    let listed = db
        .list_records(col_id, Default::default())
        .await
        .unwrap();
    assert_eq!(listed.total, 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn composite_unique_is_enforced() {
    let (dir, db) = fresh_db("idx_composite").await;
    let schema = parse_schema(json!({
        "fields": {
            "tenant": { "type": "string", "required": true },
            "sku": { "type": "string", "required": true }
        },
        "composite_unique": [["tenant", "sku"]]
    }));
    let col_id = db
        .create_collection("items", &Some(schema), None)
        .await
        .unwrap();

    db.create_record(col_id, &json!({"tenant": "acme", "sku": "A1"}))
        .await
        .unwrap();

    // Same pair rejected.
    let dup = db
        .create_record(col_id, &json!({"tenant": "acme", "sku": "A1"}))
        .await;
    assert!(dup.is_err(), "duplicate composite pair must fail");

    // Different pair accepted.
    db.create_record(col_id, &json!({"tenant": "acme", "sku": "A2"}))
        .await
        .unwrap();
    db.create_record(col_id, &json!({"tenant": "other", "sku": "A1"}))
        .await
        .unwrap();

    let conn = open_data(&dir);
    assert!(applied_index_names(&conn).contains(&format!("idx_c_{col_id}_0")));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn dropping_unique_flag_drops_the_index() {
    let (dir, db) = fresh_db("idx_drop_unique").await;
    let col_id = db
        .create_collection("contacts", &Some(parse_schema(unique_email_schema())), None)
        .await
        .unwrap();

    let conn = open_data(&dir);
    assert!(applied_index_names(&conn).contains(&format!("idx_u_{col_id}_email")));
    drop(conn);

    // Re-write the *stored* schema (same field uids) without the unique flag.
    let mut schema = db
        .get_collection(col_id)
        .await
        .unwrap()
        .unwrap()
        .schema
        .unwrap();
    schema.fields.get_mut("email").unwrap().unique = Some(false);
    db.update_collection(col_id, None, Some(schema)).await.unwrap();

    let conn = open_data(&dir);
    assert!(
        applied_index_names(&conn).is_empty(),
        "metadata must be cleared: {:?}",
        applied_index_names(&conn)
    );
    assert!(!index_names(&conn).contains(&format!("idx_u_{col_id}_email")));

    // Duplicates are now permitted.
    db.create_record(col_id, &json!({"email": "x@y.com"}))
        .await
        .unwrap();
    db.create_record(col_id, &json!({"email": "x@y.com"}))
        .await
        .unwrap();

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn deleting_collection_drops_its_indexes() {
    let (dir, db) = fresh_db("idx_drop_collection").await;
    let col_id = db
        .create_collection("contacts", &Some(parse_schema(unique_email_schema())), None)
        .await
        .unwrap();
    db.create_record(col_id, &json!({"email": "a@b.com"}))
        .await
        .unwrap();

    db.delete_collection(col_id).await.unwrap();

    let conn = open_data(&dir);
    assert!(
        !index_names(&conn).contains(&format!("idx_u_{col_id}_email")),
        "index must be dropped with its collection"
    );
    assert!(applied_index_names(&conn).is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn preflight_rejects_unique_index_over_existing_duplicates() {
    let (dir, db) = fresh_db("idx_preflight").await;

    // First create the collection *without* uniqueness and insert duplicates.
    let plain = parse_schema(json!({
        "fields": { "email": { "type": "email", "required": false } }
    }));
    let col_id = db
        .create_collection("contacts", &Some(plain), None)
        .await
        .unwrap();
    db.create_record(col_id, &json!({"email": "dup@x.com"}))
        .await
        .unwrap();
    db.create_record(col_id, &json!({"email": "dup@x.com"}))
        .await
        .unwrap();

    // Now demand uniqueness (same stored field, so no rename migration runs):
    // the migration must fail with an actionable error and change nothing.
    let mut strict = db
        .get_collection(col_id)
        .await
        .unwrap()
        .unwrap()
        .schema
        .unwrap();
    strict.fields.get_mut("email").unwrap().unique = Some(true);
    let err = db
        .update_collection(col_id, None, Some(strict))
        .await
        .expect_err("existing duplicates must block the unique index");
    let msg = err.to_string();
    assert!(msg.contains("duplicate"), "actionable message expected: {msg}");
    assert!(msg.contains("dup@x.com"), "offending value expected: {msg}");

    // No unique index may have been created.
    let conn = open_data(&dir);
    assert!(applied_index_names(&conn).is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn sql_indexed_field_creates_usable_lookup_index() {
    let (dir, db) = fresh_db("idx_sql_indexed").await;
    let schema = parse_schema(json!({
        "fields": {
            "status": { "type": "string", "required": false, "sql_indexed": true },
            "title": { "type": "text", "required": false }
        }
    }));
    let col_id = db
        .create_collection("posts", &Some(schema), None)
        .await
        .unwrap();

    for i in 0..5 {
        db.create_record(
            col_id,
            &json!({"status": if i % 2 == 0 { "published" } else { "draft" }}),
        )
        .await
        .unwrap();
    }

    let conn = open_data(&dir);
    let name = format!("idx_n_{col_id}_status");
    assert!(index_names(&conn).contains(&name), "lookup index expected");

    // The planner must actually choose the expression index for the shape the
    // engine emits (`json_extract(records.data, ...)`).
    let plan: String = conn
        .query_row(
            &format!(
                "EXPLAIN QUERY PLAN SELECT * FROM records \
                 WHERE collection_id = {col_id} \
                 AND json_extract(records.data, '$.status') = 'published'"
            ),
            [],
            |r| r.get(3),
        )
        .unwrap();
    assert!(
        plan.contains(&name),
        "planner should use {name}, plan was: {plan}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn ensure_indexes_is_idempotent_across_restart() {
    let (dir, db) = fresh_db("idx_idempotent").await;
    let col_id = db
        .create_collection("contacts", &Some(parse_schema(unique_email_schema())), None)
        .await
        .unwrap();

    let conn = open_data(&dir);
    let before = applied_index_names(&conn);
    drop(conn);
    assert_eq!(before.len(), 1);

    // A second reconcile of the same schema (as happens on every startup) must
    // not error, duplicate metadata rows, or rebuild the index.
    let stored = db
        .get_collection(col_id)
        .await
        .unwrap()
        .unwrap()
        .schema
        .unwrap();
    db.update_collection(col_id, None, Some(stored)).await.unwrap();

    let conn = open_data(&dir);
    assert_eq!(applied_index_names(&conn), before);
    assert_eq!(applied_index_names(&conn).len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}
