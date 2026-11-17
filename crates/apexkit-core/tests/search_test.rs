//! Functional tests for the Tantivy-backed full-text search engine.

mod common;

use apexkit_core::models::schema::CollectionSchema;
use apexkit_core::search::SearchManager;
use common::{parse_schema, temp_dir};
use serde_json::json;

fn indexed_schema() -> CollectionSchema {
    parse_schema(json!({
        "fields": {
            "title": { "type": "text", "required": true, "ose_indexed": true },
            "views": { "type": "number", "required": false, "ose_indexed": true }
        },
        "policies": { "read": "public", "create": "public", "update": "public", "delete": "public" }
    }))
}

#[test]
fn load_index_is_idempotent() {
    let dir = temp_dir("search_load");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();

    mgr.load_index(1, &schema).unwrap();
    mgr.load_index(1, &schema).unwrap();
    assert_eq!(mgr.get_doc_count(1).unwrap(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn index_and_search_returns_matching_record() {
    let dir = temp_dir("search_match");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();
    mgr.load_index(1, &schema).unwrap();

    mgr.index_record(1, 100, &json!({"title": "Semantic Search Rocks"}), &schema)
        .unwrap();

    let hits = mgr.search(1, "Rocks", 10).unwrap();
    assert!(hits.contains(&100), "expected record 100 in {hits:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn search_is_prefix_and_case_insensitive() {
    let dir = temp_dir("search_prefix");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();
    mgr.load_index(1, &schema).unwrap();
    mgr.index_record(1, 1, &json!({"title": "Running Shoes"}), &schema)
        .unwrap();

    // Prefix match on a lowercased token.
    assert!(!mgr.search(1, "runn", 10).unwrap().is_empty());
    // Case-insensitive exact token.
    assert!(!mgr.search(1, "SHOES", 10).unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn instant_search_returns_scored_snippets() {
    let dir = temp_dir("search_instant");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();
    mgr.load_index(1, &schema).unwrap();
    mgr.index_record(1, 5, &json!({"title": "Alpha Beta Gamma"}), &schema)
        .unwrap();

    let results = mgr.instant_search(1, "Beta", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, 5);
    assert!(results[0].score >= 0.0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_query_returns_no_results() {
    let dir = temp_dir("search_empty");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();
    mgr.load_index(1, &schema).unwrap();
    mgr.index_record(1, 1, &json!({"title": "Anything"}), &schema)
        .unwrap();

    assert!(mgr.instant_search(1, "   ", 10).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upsert_replaces_previous_document() {
    let dir = temp_dir("search_upsert");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();
    mgr.load_index(1, &schema).unwrap();

    mgr.index_record(1, 1, &json!({"title": "Original"}), &schema)
        .unwrap();
    mgr.index_record(1, 1, &json!({"title": "Replaced"}), &schema)
        .unwrap();

    assert_eq!(mgr.get_doc_count(1).unwrap(), 1, "upsert must not duplicate");
    assert!(mgr.search(1, "Original", 10).unwrap().is_empty());
    assert!(!mgr.search(1, "Replaced", 10).unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn index_batch_indexes_all_records() {
    let dir = temp_dir("search_batch");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();
    mgr.load_index(1, &schema).unwrap();

    let records = vec![
        (1i64, json!({"title": "apple"})),
        (2i64, json!({"title": "banana"})),
        (3i64, json!({"title": "cherry"})),
    ];
    mgr.index_batch(1, &records, &schema).unwrap();
    assert_eq!(mgr.get_doc_count(1).unwrap(), 3);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn index_batch_empty_is_noop() {
    let dir = temp_dir("search_batch_empty");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();
    mgr.load_index(1, &schema).unwrap();
    mgr.index_batch(1, &[], &schema).unwrap();
    assert_eq!(mgr.get_doc_count(1).unwrap(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn delete_record_removes_from_index() {
    let dir = temp_dir("search_delete");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();
    mgr.load_index(1, &schema).unwrap();
    mgr.index_record(1, 1, &json!({"title": "Ephemeral"}), &schema)
        .unwrap();
    assert_eq!(mgr.get_doc_count(1).unwrap(), 1);

    mgr.delete_record(1, 1).unwrap();
    assert_eq!(mgr.get_doc_count(1).unwrap(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn delete_index_removes_collection() {
    let dir = temp_dir("search_delidx");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();
    mgr.load_index(1, &schema).unwrap();
    mgr.index_record(1, 1, &json!({"title": "x"}), &schema).unwrap();

    mgr.delete_index(1).unwrap();
    // After deletion the collection is no longer loaded, so search errors.
    assert!(mgr.search(1, "x", 10).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn operations_on_unloaded_collection_error() {
    let dir = temp_dir("search_unloaded");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();

    assert!(mgr.index_record(9, 1, &json!({"title": "x"}), &schema).is_err());
    assert!(mgr.instant_search(9, "x", 5).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn numeric_field_is_searchable() {
    let dir = temp_dir("search_number");
    let mgr = SearchManager::new(dir.to_str().unwrap());
    let schema = indexed_schema();
    mgr.load_index(1, &schema).unwrap();
    mgr.index_record(1, 7, &json!({"title": "item", "views": 42}), &schema)
        .unwrap();

    let hits = mgr.search(1, "42", 10).unwrap();
    assert!(hits.contains(&7), "numeric token should match: {hits:?}");

    let _ = std::fs::remove_dir_all(&dir);
}
