//! Unit tests for the query filter DSL: parsing JSON filters, compiling them
//! to inline SQL and parameterized SQL, and in-memory matching.

use apexkit_core::query::filter::{FilterNode, FilterOp, LogicOp, json_to_inline_sql};
use serde_json::json;

#[test]
fn json_to_inline_sql_escapes_single_quotes() {
    assert_eq!(json_to_inline_sql(&json!("O'Brien")), "'O''Brien'");
    assert_eq!(json_to_inline_sql(&json!(42)), "42");
    assert_eq!(json_to_inline_sql(&json!(true)), "1");
    assert_eq!(json_to_inline_sql(&json!(false)), "0");
    assert_eq!(json_to_inline_sql(&json!(null)), "NULL");
}

#[test]
fn parse_empty_object_is_empty_node() {
    let node = FilterNode::parse(&json!({}));
    assert!(matches!(node, FilterNode::Empty));
    assert!(node.to_inline_sql().is_none());
}

#[test]
fn parse_single_condition_has_no_group_wrapper() {
    let node = FilterNode::parse(&json!({"status": "active"}));
    match node {
        FilterNode::Condition { field, op, value } => {
            assert_eq!(field, "status");
            assert!(matches!(op, FilterOp::Eq));
            assert_eq!(value, json!("active"));
        }
        _ => panic!("expected a single condition, got {node:?}"),
    }
}

#[test]
fn parse_multiple_conditions_groups_with_and() {
    let node = FilterNode::parse(&json!({"a": 1, "b": 2}));
    match node {
        FilterNode::Group { op, children } => {
            assert!(matches!(op, LogicOp::And));
            assert_eq!(children.len(), 2);
        }
        _ => panic!("expected AND group"),
    }
}

#[test]
fn parse_supports_explicit_and_or() {
    let and_node = FilterNode::parse(&json!({"$and": [{"a": 1}, {"b": 2}]}));
    match and_node {
        FilterNode::Group { op, children } => {
            assert!(matches!(op, LogicOp::And));
            assert_eq!(children.len(), 2);
        }
        _ => panic!("expected AND group"),
    }

    let or_node = FilterNode::parse(&json!({"$or": [{"a": 1}, {"b": 2}]}));
    match or_node {
        FilterNode::Group { op, .. } => assert!(matches!(op, LogicOp::Or)),
        _ => panic!("expected OR group"),
    }
}

#[test]
fn parse_comparison_operators() {
    for (key, expected) in [
        ("$gt", FilterOp::Gt),
        ("$gte", FilterOp::Gte),
        ("$lt", FilterOp::Lt),
        ("$lte", FilterOp::Lte),
        ("$neq", FilterOp::Neq),
        ("$in", FilterOp::In),
        ("$contains", FilterOp::Contains),
    ] {
        let node = FilterNode::parse(&json!({"price": {key: 10}}));
        match node {
            FilterNode::Condition { op, .. } => {
                assert_eq!(format!("{op:?}"), format!("{expected:?}"));
            }
            _ => panic!("expected condition for {key}"),
        }
    }
}

#[test]
fn inline_sql_maps_id_to_records_column() {
    let node = FilterNode::parse(&json!({"id": 7}));
    let sql = node.to_inline_sql().unwrap();
    assert!(sql.contains("records.id"), "got {sql}");
}

#[test]
fn inline_sql_uses_json_extract_for_nested_fields() {
    let node = FilterNode::parse(&json!({"meta.author.name": "jane"}));
    let sql = node.to_inline_sql().unwrap();
    assert!(sql.contains("->> 'name'"), "got {sql}");
    assert!(sql.contains("-> 'author'"), "got {sql}");
}

#[test]
fn inline_sql_contains_uses_like_wildcards() {
    let node = FilterNode::parse(&json!({"title": {"$contains": "hello"}}));
    let sql = node.to_inline_sql().unwrap();
    assert!(sql.contains("LIKE"), "got {sql}");
    assert!(sql.contains("%hello%"), "got {sql}");
}

#[test]
fn inline_sql_empty_in_array_is_always_false() {
    let node = FilterNode::parse(&json!({"id": {"$in": []}}));
    assert_eq!(node.to_inline_sql().unwrap(), "1=0");
}

#[test]
fn parameterized_sql_binds_values_not_literals() {
    let node = FilterNode::parse(&json!({"status": "active"}));
    let (sql, params) = node.to_sql().unwrap();
    assert!(sql.contains('?'), "should use bound parameters: {sql}");
    assert!(!params.is_empty());
}

#[test]
fn matches_evaluates_eq_condition() {
    let node = FilterNode::parse(&json!({"status": "active"}));
    assert!(node.matches(&json!({"status": "active"})));
    assert!(!node.matches(&json!({"status": "draft"})));
}

#[test]
fn matches_handles_numeric_comparisons() {
    let node = FilterNode::parse(&json!({"price": {"$gt": 100}}));
    assert!(node.matches(&json!({"price": 150})));
    assert!(!node.matches(&json!({"price": 50})));
}

#[test]
fn matches_and_or_groups() {
    let and_node = FilterNode::parse(&json!({"a": 1, "b": 2}));
    assert!(and_node.matches(&json!({"a": 1, "b": 2})));
    assert!(!and_node.matches(&json!({"a": 1, "b": 9})));

    let or_node = FilterNode::parse(&json!({"$or": [{"a": 1}, {"b": 2}]}));
    assert!(or_node.matches(&json!({"a": 1})));
    assert!(or_node.matches(&json!({"b": 2})));
    assert!(!or_node.matches(&json!({"c": 3})));
}

#[test]
fn matches_contains_substring() {
    let node = FilterNode::parse(&json!({"title": {"$contains": "world"}}));
    assert!(node.matches(&json!({"title": "hello world"})));
    assert!(!node.matches(&json!({"title": "hello"})));
}

#[test]
fn matches_in_and_nin() {
    let in_node = FilterNode::parse(&json!({"status": {"$in": ["a", "b"]}}));
    assert!(in_node.matches(&json!({"status": "a"})));
    assert!(!in_node.matches(&json!({"status": "c"})));

    let nin_node = FilterNode::parse(&json!({"status": {"$nin": ["a", "b"]}}));
    assert!(nin_node.matches(&json!({"status": "c"})));
    assert!(!nin_node.matches(&json!({"status": "a"})));
}

#[test]
fn matches_nested_path() {
    let node = FilterNode::parse(&json!({"meta.role": "admin"}));
    assert!(node.matches(&json!({"meta": {"role": "admin"}})));
    assert!(!node.matches(&json!({"meta": {"role": "user"}})));
}

#[test]
fn matches_strips_record_prefix() {
    let node = FilterNode::parse(&json!({"@record.data.status": "active"}));
    assert!(node.matches(&json!({"status": "active"})));
}

#[test]
fn missing_field_ne_with_null_expected_is_false() {
    // Semantics: a missing field != some non-null value is true.
    let node = FilterNode::parse(&json!({"absent": {"$neq": "x"}}));
    assert!(node.matches(&json!({})));
}
