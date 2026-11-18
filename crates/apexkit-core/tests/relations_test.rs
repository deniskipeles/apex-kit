//! Functional tests for relations as first-class edges.
//!
//! A relation is a directed edge keyed by the record-id pair only: the pair is
//! unique, there is no relation name on the edge, and edges are removed by
//! SQLite foreign-key cascade when either endpoint is deleted. These tests drive
//! the real on-disk SQLite database through the `Db` trait.

mod common;

use apexkit_core::database::traits::{CollectionStore, RecordStore, RelationStore};
use apexkit_core::query::QueryOptions;
use common::{fresh_db, parse_schema};
use serde_json::json;

fn authors_schema() -> serde_json::Value {
    json!({
        "fields": { "name": { "type": "text", "required": true } },
        "policies": { "read": "public", "create": "public", "update": "public", "delete": "public" }
    })
}

/// A `books` schema with a forward relation field `author` pointing at `authors`.
fn books_schema_with_author(cascade: bool) -> serde_json::Value {
    books_schema("one", cascade)
}

/// A `books` schema whose `author` relation field has the given cardinality.
fn books_schema(cardinality: &str, cascade: bool) -> serde_json::Value {
    json!({
        "fields": { "title": { "type": "text", "required": true } },
        "relations": {
            "author": {
                "target_collection": "authors",
                "relation_type": cardinality,
                "cascade_on_target_delete": cascade
            }
        },
        "policies": { "read": "public", "create": "public", "update": "public", "delete": "public" }
    })
}

#[tokio::test]
async fn edge_is_directed_from_origin_to_target() {
    let (dir, db) = fresh_db("rel_directed").await;
    let authors = db.create_collection("authors", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let books = db.create_collection("books", &Some(parse_schema(authors_schema())), None).await.unwrap();

    let a = db.create_record(authors, &json!({"name": "Jane"})).await.unwrap();
    let b = db.create_record(books, &json!({"name": "Book"})).await.unwrap();

    db.create_relation(books, b, authors, a).await.unwrap();

    assert_eq!(db.get_related_ids(b).await.unwrap(), vec![a]);
    // The reverse direction is a distinct edge that was never created.
    assert!(db.get_related_ids(a).await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn same_pair_cannot_be_linked_twice() {
    let (dir, db) = fresh_db("rel_unique_pair").await;
    let authors = db.create_collection("authors", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let books = db.create_collection("books", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let a = db.create_record(authors, &json!({"name": "Jane"})).await.unwrap();
    let b = db.create_record(books, &json!({"name": "Book"})).await.unwrap();

    db.create_relation(books, b, authors, a).await.unwrap();
    // Re-adding the identical pair is idempotent, not a duplicate edge.
    db.create_relation(books, b, authors, a).await.unwrap();

    assert_eq!(db.get_related_ids(b).await.unwrap(), vec![a]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn linking_a_missing_endpoint_is_a_noop() {
    let (dir, db) = fresh_db("rel_missing_endpoint").await;
    let authors = db.create_collection("authors", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let books = db.create_collection("books", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let b = db.create_record(books, &json!({"name": "Book"})).await.unwrap();

    // Missing target id: must not raise (the foreign key would otherwise fire).
    db.create_relation(books, b, authors, 999_999).await.unwrap();
    assert!(db.get_related_ids(b).await.unwrap().is_empty());

    // Missing origin id: likewise a no-op.
    db.create_relation(books, 999_999, authors, 1).await.unwrap();

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn deleting_either_endpoint_cascades_the_edge() {
    let (dir, db) = fresh_db("rel_cascade_both").await;
    let authors = db.create_collection("authors", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let books = db.create_collection("books", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let a = db.create_record(authors, &json!({"name": "Jane"})).await.unwrap();
    let b = db.create_record(books, &json!({"name": "Book"})).await.unwrap();
    let b2 = db.create_record(books, &json!({"name": "Book 2"})).await.unwrap();

    db.create_relation(books, b, authors, a).await.unwrap();
    db.create_relation(books, b2, authors, a).await.unwrap();

    // Removing the target (author) drops both inbound edges.
    db.delete_record(authors, a).await.unwrap();
    assert!(db.get_related_ids(b).await.unwrap().is_empty());
    assert!(db.get_related_ids(b2).await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn delete_relation_removes_only_that_edge() {
    let (dir, db) = fresh_db("rel_delete_one").await;
    let authors = db.create_collection("authors", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let books = db.create_collection("books", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let a1 = db.create_record(authors, &json!({"name": "Jane"})).await.unwrap();
    let a2 = db.create_record(authors, &json!({"name": "John"})).await.unwrap();
    let b = db.create_record(books, &json!({"name": "Book"})).await.unwrap();

    db.create_relation(books, b, authors, a1).await.unwrap();
    db.create_relation(books, b, authors, a2).await.unwrap();
    let mut linked = db.get_related_ids(b).await.unwrap();
    linked.sort_unstable();
    assert_eq!(linked, vec![a1, a2]);

    db.delete_relation(books, b, authors, a1).await.unwrap();
    assert_eq!(db.get_related_ids(b).await.unwrap(), vec![a2]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn cascade_on_target_delete_removes_owning_records() {
    let (dir, db) = fresh_db("rel_schema_cascade").await;
    let authors = db.create_collection("authors", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let books = db
        .create_collection("books", &Some(parse_schema(books_schema_with_author(true))), None)
        .await
        .unwrap();

    let a = db.create_record(authors, &json!({"name": "Jane"})).await.unwrap();
    // The book's relation field is a scalar foreign key, so the edge is created
    // by the record write path rather than the relations API.
    let b = db
        .create_record(books, &json!({"title": "Book", "author": a}))
        .await
        .unwrap();
    assert_eq!(db.get_related_ids(b).await.unwrap(), vec![a]);

    // cascade_on_target_delete: deleting the author deletes the book too.
    db.delete_record(authors, a).await.unwrap();
    assert!(db.get_record(books, b, None).await.unwrap().is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn record_field_write_creates_and_replaces_edges() {
    let (dir, db) = fresh_db("rel_field_sync").await;
    let authors = db.create_collection("authors", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let books = db
        .create_collection("books", &Some(parse_schema(books_schema_with_author(false))), None)
        .await
        .unwrap();

    let a1 = db.create_record(authors, &json!({"name": "Jane"})).await.unwrap();
    let a2 = db.create_record(authors, &json!({"name": "John"})).await.unwrap();
    let b = db
        .create_record(books, &json!({"title": "Book", "author": a1}))
        .await
        .unwrap();
    assert_eq!(db.get_related_ids(b).await.unwrap(), vec![a1]);

    // Re-pointing the field replaces the edge.
    db.update_record(books, b, &json!({"author": a2})).await.unwrap();
    assert_eq!(db.get_related_ids(b).await.unwrap(), vec![a2]);

    // Clearing the field removes the edge.
    db.update_record(books, b, &json!({"author": null})).await.unwrap();
    assert!(db.get_related_ids(b).await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn expand_returns_the_linked_record() {
    let (dir, db) = fresh_db("rel_expand").await;
    let authors = db.create_collection("authors", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let books = db
        .create_collection("books", &Some(parse_schema(books_schema_with_author(false))), None)
        .await
        .unwrap();

    let a = db.create_record(authors, &json!({"name": "Jane"})).await.unwrap();
    let _b = db
        .create_record(books, &json!({"title": "Book", "author": a}))
        .await
        .unwrap();

    let mut options = QueryOptions::default();
    options.expand = Some("author".to_string());
    let listed = db.list_records(books, options).await.unwrap();
    assert_eq!(listed.items.len(), 1);

    let expanded = listed.items[0].expand.as_ref().expect("expand present");
    assert_eq!(expanded["author"]["id"], json!(a));
    assert_eq!(expanded["author"]["data"]["name"], json!("Jane"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn reverse_expand_returns_a_list() {
    let (dir, db) = fresh_db("rel_reverse_expand").await;
    let authors = db.create_collection("authors", &Some(parse_schema(authors_schema())), None).await.unwrap();
    let books = db
        .create_collection("books", &Some(parse_schema(books_schema("many", false))), None)
        .await
        .unwrap();

    let a = db.create_record(authors, &json!({"name": "Jane"})).await.unwrap();
    for title in ["One", "Two"] {
        db.create_record(books, &json!({"title": title, "author": a})).await.unwrap();
    }

    // Expanding `books` from the author side returns the owning records.
    let mut options = QueryOptions::default();
    options.expand = Some("books".to_string());
    let listed = db.list_records(authors, options).await.unwrap();
    assert_eq!(listed.items.len(), 1);

    let expanded = listed.items[0].expand.as_ref().expect("expand present");
    let children = expanded["books"].as_array().unwrap();
    assert_eq!(children.len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}
