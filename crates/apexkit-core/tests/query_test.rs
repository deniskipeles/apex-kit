//! Unit tests for the query builder helpers and the post-processing pipeline.

use apexkit_core::query::builder::{
    build_expand_json_object, build_expand_tree, build_expand_tree_from_list, smart_split,
};
use apexkit_core::query::processor::{PipelineStep, QueryProcessor};
use apexkit_core::query::QueryOptions;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn query_options_defaults() {
    let o = QueryOptions::default();
    assert_eq!(o.page, Some(1));
    assert_eq!(o.per_page, Some(30));
    assert!(o.sort.is_none());
    assert!(o.filter.is_none());
    assert!(o.rls_sql.is_none());
}

#[test]
fn smart_split_splits_on_top_level_commas() {
    assert_eq!(smart_split("a,b,c"), vec!["a", "b", "c"]);
    assert_eq!(smart_split(" a , b "), vec!["a", "b"]);
    assert_eq!(smart_split(""), Vec::<String>::new());
}

#[test]
fn smart_split_ignores_commas_inside_parentheses() {
    let parts = smart_split("posts(10,5),author");
    assert_eq!(parts, vec!["posts(10,5)", "author"]);
}

#[test]
fn build_expand_tree_groups_nested_paths() {
    let tree = build_expand_tree("author,comments.user");
    assert!(tree.contains_key("author"));
    assert_eq!(tree.get("comments").unwrap(), &vec!["user".to_string()]);
}

#[test]
fn build_expand_tree_from_list_matches_string_form() {
    let from_list = build_expand_tree_from_list(&["author".to_string(), "comments.user".to_string()]);
    let from_str = build_expand_tree("author,comments.user");
    assert_eq!(from_list.get("author"), from_str.get("author"));
    assert_eq!(from_list.get("comments"), from_str.get("comments"));
}

#[test]
fn build_expand_json_object_empty_is_null() {
    let sql = build_expand_json_object(
        vec![],
        "records",
        0,
        "posts",
        1,
        &HashMap::new(),
        &HashMap::new(),
    );
    assert_eq!(sql, "NULL");
}

#[test]
fn pipeline_cumulative_sum_accumulates() {
    let data = vec![
        json!({"amount": 10.0}),
        json!({"amount": 5.0}),
        json!({"amount": 2.0}),
    ];
    let steps = vec![PipelineStep::CumulativeSum {
        field: "amount".into(),
        output_field: "running".into(),
    }];
    let out = QueryProcessor::process(data, steps).unwrap();
    assert_eq!(out[0]["running"], json!(10.0));
    assert_eq!(out[1]["running"], json!(15.0));
    assert_eq!(out[2]["running"], json!(17.0));
}

#[test]
fn pipeline_pivot_sums_by_key() {
    let data = vec![
        json!({"category": "a", "amount": 10.0}),
        json!({"category": "b", "amount": 3.0}),
        json!({"category": "a", "amount": 5.0}),
    ];
    let steps = vec![PipelineStep::Pivot {
        key: "category".into(),
        value: "amount".into(),
        agg: "sum".into(),
    }];
    let out = QueryProcessor::process(data, steps).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["a"], json!(15.0));
    assert_eq!(out[0]["b"], json!(3.0));
}

#[test]
fn pipeline_pivot_count_counts_rows() {
    let data = vec![
        json!({"category": "a", "amount": 10.0}),
        json!({"category": "a", "amount": 1.0}),
        json!({"category": "b", "amount": 1.0}),
    ];
    let steps = vec![PipelineStep::Pivot {
        key: "category".into(),
        value: "amount".into(),
        agg: "count".into(),
    }];
    let out = QueryProcessor::process(data, steps).unwrap();
    assert_eq!(out[0]["a"], json!(2.0));
    assert_eq!(out[0]["b"], json!(1.0));
}

#[test]
fn pipeline_empty_steps_returns_input_unchanged() {
    let data = vec![json!({"x": 1})];
    let out = QueryProcessor::process(data.clone(), vec![]).unwrap();
    assert_eq!(out, data);
}

#[test]
fn pipeline_map_is_a_placeholder_noop() {
    let data = vec![json!({"x": 1})];
    let steps = vec![PipelineStep::Map {
        expression: "x + 1".into(),
    }];
    let out = QueryProcessor::process(data.clone(), steps).unwrap();
    assert_eq!(out, data, "map step is currently a no-op");
}
