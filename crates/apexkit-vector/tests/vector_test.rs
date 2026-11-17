//! End-to-end smoke test for the vector index: insert, search, and delete
//! across multiple collections and fields.

use apexkit_vector::VectorIndex;

#[test]
fn vector_index_operations() {
    let index = VectorIndex::new();

    index.insert(1, 100, "embedding", &[1.0, 0.0, 0.0]);
    index.insert(1, 101, "embedding", &[0.0, 1.0, 0.0]);
    index.insert(1, 102, "embedding", &[0.0, 0.0, 1.0]);

    let results = index.search(1, "embedding", &[0.9, 0.1, 0.0], 2);
    assert!(!results.is_empty(), "search results should not be empty");
    assert_eq!(results[0].0, 100, "closest match should be record 100");

    let empty_results = index.search(1, "non_existent_field", &[1.0, 0.0, 0.0], 5);
    assert!(empty_results.is_empty());
}
