use apexkit_vector::VectorIndex;

#[test]
fn test_vector_index_operations() {
    let index = VectorIndex::new();

    // Insert multiple vectors with unique IDs for collection 1, field "embedding"
    index.insert(1, 100, "embedding", &[1.0, 0.0, 0.0]);
    index.insert(1, 101, "embedding", &[0.0, 1.0, 0.0]);
    index.insert(1, 102, "embedding", &[0.0, 0.0, 1.0]);

    // Search nearest neighbor to [0.9, 0.1, 0.0]
    let results = index.search(1, "embedding", &[0.9, 0.1, 0.0], 2);

    assert!(!results.is_empty(), "Search results should not be empty");
    assert_eq!(
        results[0].0, 100,
        "The closest match should be record ID 100"
    );

    // Test search with another non-existent field returns empty list
    let empty_results = index.search(1, "non_existent_field", &[1.0, 0.0, 0.0], 5);
    assert!(empty_results.is_empty());
}
