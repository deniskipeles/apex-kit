//! Derives the desired set of indexes from the current collection schemas.
//!
//! This is a small schema-to-DDL compiler: it reads already-parsed schema
//! definitions and produces [`IndexSpec`]s. It never parses SQL.

use super::spec::{IndexKind, IndexSpec};
use crate::models::Collection;
use std::collections::BTreeMap;

/// Naming convention for a single-field index.
pub fn field_index_name(kind: IndexKind, collection_id: i64, field: &str) -> String {
    let prefix = match kind {
        IndexKind::Unique => "idx_u",
        IndexKind::Composite => "idx_c",
        IndexKind::NonUnique => "idx_n",
    };
    format!("{}_{}_{}", prefix, collection_id, field)
}

/// Naming convention for a composite index (keyed by position).
pub fn composite_index_name(collection_id: i64, position: usize) -> String {
    format!("idx_c_{}_{}", collection_id, position)
}

/// Computes the full desired index set for the given collections.
///
/// A `BTreeMap` keeps iteration order deterministic, which makes the diff in
/// the planner reproducible.
pub fn compute_desired_indexes(collections: &[Collection]) -> BTreeMap<String, IndexSpec> {
    let mut desired: BTreeMap<String, IndexSpec> = BTreeMap::new();

    for col in collections {
        let Some(schema) = &col.schema else {
            continue;
        };

        for (field_name, def) in &schema.fields {
            if def.unique.unwrap_or(false) {
                let name = field_index_name(IndexKind::Unique, col.id, field_name);
                desired.insert(
                    name.clone(),
                    IndexSpec {
                        name,
                        collection_id: col.id,
                        kind: IndexKind::Unique,
                        fields: vec![field_name.clone()],
                    },
                );
            } else if def.sql_indexed {
                let name = field_index_name(IndexKind::NonUnique, col.id, field_name);
                desired.insert(
                    name.clone(),
                    IndexSpec {
                        name,
                        collection_id: col.id,
                        kind: IndexKind::NonUnique,
                        fields: vec![field_name.clone()],
                    },
                );
            }
        }

        for (rel_name, def) in &schema.relations {
            // `One` relations were historically unique (one origin record may
            // point at a given target). Preserve that with a unique index; it
            // also satisfies lookups, so no separate non-unique index is needed.
            if def.relation_type == crate::models::schema::RelationType::One {
                let name = field_index_name(IndexKind::Unique, col.id, rel_name);
                desired.insert(
                    name.clone(),
                    IndexSpec {
                        name,
                        collection_id: col.id,
                        kind: IndexKind::Unique,
                        fields: vec![rel_name.clone()],
                    },
                );
            } else if def.sql_indexed {
                let name = field_index_name(IndexKind::NonUnique, col.id, rel_name);
                desired.insert(
                    name.clone(),
                    IndexSpec {
                        name,
                        collection_id: col.id,
                        kind: IndexKind::NonUnique,
                        fields: vec![rel_name.clone()],
                    },
                );
            }
        }

        for (position, group) in schema.composite_unique.iter().enumerate() {
            if group.is_empty() {
                continue;
            }
            let name = composite_index_name(col.id, position);
            desired.insert(
                name.clone(),
                IndexSpec {
                    name,
                    collection_id: col.id,
                    kind: IndexKind::Composite,
                    fields: group.clone(),
                },
            );
        }
    }

    desired
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Collection;
    use crate::models::schema::CollectionSchema;
    use serde_json::json;

    fn collection(id: i64, schema: serde_json::Value) -> Collection {
        Collection {
            id,
            name: format!("c{id}"),
            schema: Some(serde_json::from_value::<CollectionSchema>(schema).unwrap()),
            index: None,
        }
    }

    #[test]
    fn unique_field_produces_unique_index() {
        let cols = vec![collection(
            5,
            json!({ "fields": { "email": { "type": "email", "required": true, "unique": true } } }),
        )];
        let desired = compute_desired_indexes(&cols);
        let spec = desired.get("idx_u_5_email").expect("unique index expected");
        assert_eq!(spec.kind, IndexKind::Unique);
        assert_eq!(spec.fields, vec!["email".to_string()]);
        assert_eq!(spec.collection_id, 5);
    }

    #[test]
    fn sql_indexed_field_produces_non_unique_index() {
        let cols = vec![collection(
            1,
            json!({ "fields": { "status": { "type": "string", "required": false, "sql_indexed": true } } }),
        )];
        let desired = compute_desired_indexes(&cols);
        let spec = desired.get("idx_n_1_status").expect("non-unique index expected");
        assert_eq!(spec.kind, IndexKind::NonUnique);
    }

    #[test]
    fn ose_indexed_field_alone_does_not_produce_sql_index() {
        // `ose_indexed` targets the full-text search engine, not the SQL layer.
        let cols = vec![collection(
            2,
            json!({ "fields": { "title": { "type": "text", "required": false, "ose_indexed": true } } }),
        )];
        let desired = compute_desired_indexes(&cols);
        assert!(!desired.contains_key("idx_n_2_title"));
    }

    #[test]
    fn sql_indexed_relation_produces_non_unique_index() {
        let cols = vec![collection(
            2,
            json!({
                "fields": {},
                "relations": {
                    "author": {
                        "target_collection": "authors",
                        "relation_type": "many",
                        "sql_indexed": true
                    }
                }
            }),
        )];
        let desired = compute_desired_indexes(&cols);
        assert!(desired.contains_key("idx_n_2_author"));
    }

    #[test]
    fn one_relation_produces_unique_index() {
        let cols = vec![collection(
            3,
            json!({
                "fields": {},
                "relations": {
                    "profile": {
                        "target_collection": "profiles",
                        "relation_type": "one"
                    }
                }
            }),
        )];
        let desired = compute_desired_indexes(&cols);
        let spec = desired.get("idx_u_3_profile").expect("unique relation index");
        assert_eq!(spec.kind, IndexKind::Unique);
    }

    #[test]
    fn composite_unique_produces_composite_index() {
        let cols = vec![collection(
            5,
            json!({
                "fields": {
                    "tenant_slug": { "type": "string", "required": true },
                    "sku": { "type": "string", "required": true }
                },
                "composite_unique": [["tenant_slug", "sku"]]
            }),
        )];
        let desired = compute_desired_indexes(&cols);
        let spec = desired.get("idx_c_5_0").expect("composite index expected");
        assert_eq!(spec.kind, IndexKind::Composite);
        assert_eq!(spec.fields, vec!["tenant_slug".to_string(), "sku".to_string()]);
    }

    #[test]
    fn unique_takes_precedence_over_non_unique() {
        let cols = vec![collection(
            3,
            json!({ "fields": { "code": { "type": "string", "required": true, "unique": true, "sql_indexed": true } } }),
        )];
        let desired = compute_desired_indexes(&cols);
        assert!(desired.contains_key("idx_u_3_code"));
        assert!(!desired.contains_key("idx_n_3_code"));
    }

    #[test]
    fn collection_without_schema_is_skipped() {
        let cols = vec![Collection {
            id: 9,
            name: "empty".into(),
            schema: None,
            index: None,
        }];
        assert!(compute_desired_indexes(&cols).is_empty());
    }

    #[test]
    fn empty_composite_group_is_skipped() {
        let cols = vec![collection(4, json!({ "fields": {}, "composite_unique": [[]] }))];
        assert!(compute_desired_indexes(&cols).is_empty());
    }
}
