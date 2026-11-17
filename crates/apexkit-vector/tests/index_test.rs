//! Functional tests for the in-memory HNSW `VectorIndex`.
//!
//! These tests exercise the public `insert`/`search` surface and the
//! collection/field keying behaviour that the rest of ApexKit relies on.

use apexkit_vector::VectorIndex;

#[test]
fn insert_and_search_returns_nearest_neighbour_first() {
    let index = VectorIndex::new();

    index.insert(1, 100, "embedding", &[1.0, 0.0, 0.0]);
    index.insert(1, 101, "embedding", &[0.0, 1.0, 0.0]);
    index.insert(1, 102, "embedding", &[0.0, 0.0, 1.0]);

    let results = index.search(1, "embedding", &[0.9, 0.1, 0.0], 2);

    assert!(!results.is_empty(), "search should return matches");
    assert_eq!(results[0].0, 100, "closest match must be record 100");
    assert!(
        results[0].1 <= results[1].1,
        "results must be ordered by ascending L2 distance"
    );
}

#[test]
fn search_returns_at_most_limit_results() {
    let index = VectorIndex::new();
    for id in 0..20 {
        let v = [id as f32, 0.0, 0.0];
        index.insert(7, id, "vec", &v);
    }

    let results = index.search(7, "vec", &[0.0, 0.0, 0.0], 5);
    assert_eq!(results.len(), 5, "limit must cap the number of results");
}

#[test]
fn search_unknown_field_returns_empty() {
    let index = VectorIndex::new();
    index.insert(1, 1, "known", &[1.0, 2.0, 3.0]);

    let results = index.search(1, "unknown", &[1.0, 2.0, 3.0], 10);
    assert!(results.is_empty(), "unknown field must yield no results");
}

#[test]
fn search_unknown_collection_returns_empty() {
    let index = VectorIndex::new();
    index.insert(1, 1, "vec", &[1.0, 0.0]);

    let results = index.search(999, "vec", &[1.0, 0.0], 10);
    assert!(results.is_empty(), "unknown collection must yield no results");
}

#[test]
fn collections_and_fields_are_isolated() {
    let index = VectorIndex::new();

    index.insert(1, 10, "a", &[1.0, 0.0]);
    index.insert(1, 11, "b", &[0.0, 1.0]);
    index.insert(2, 20, "a", &[1.0, 0.0]);

    let col1_field_a = index.search(1, "a", &[1.0, 0.0], 10);
    assert_eq!(col1_field_a.len(), 1);
    assert_eq!(col1_field_a[0].0, 10);

    let col2_field_a = index.search(2, "a", &[1.0, 0.0], 10);
    assert_eq!(col2_field_a.len(), 1);
    assert_eq!(col2_field_a[0].0, 20);

    let col1_field_b = index.search(1, "b", &[0.0, 1.0], 10);
    assert_eq!(col1_field_b.len(), 1);
    assert_eq!(col1_field_b[0].0, 11);
}

#[test]
fn repeated_inserts_accumulate_in_same_index() {
    let index = VectorIndex::new();
    index.insert(3, 1, "f", &[1.0, 0.0]);
    index.insert(3, 2, "f", &[0.0, 1.0]);
    index.insert(3, 3, "f", &[1.0, 1.0]);

    let results = index.search(3, "f", &[1.0, 0.0], 10);
    assert_eq!(results.len(), 3, "all inserted records must be searchable");
}

#[test]
fn default_impl_matches_new() {
    let index = VectorIndex::default();
    index.insert(1, 1, "f", &[1.0, 0.0, 0.0]);
    let results = index.search(1, "f", &[1.0, 0.0, 0.0], 1);
    assert_eq!(results.len(), 1);
}

#[test]
fn delete_is_currently_a_noop_and_does_not_break_search() {
    // `VectorIndex::delete` is intentionally a no-op today; this test pins that
    // behaviour so a future real implementation is a deliberate change.
    let index = VectorIndex::new();
    index.insert(1, 1, "f", &[1.0, 0.0]);
    index.delete(1, "f", 1);

    let results = index.search(1, "f", &[1.0, 0.0], 10);
    assert_eq!(results.len(), 1, "noop delete must not remove the record");
}

#[test]
fn high_dimensional_vectors_are_supported() {
    let index = VectorIndex::new();
    let a: Vec<f32> = (0..384).map(|i| (i as f32) * 0.001).collect();
    let mut b = vec![0.0f32; 384];
    b[0] = 1.0;

    index.insert(1, 1, "f", &a);
    index.insert(1, 2, "f", &b);

    let results = index.search(1, "f", &b, 2);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, 2, "exact match must rank first");
}
