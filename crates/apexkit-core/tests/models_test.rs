//! Module tests for the schema/model layer: defaults, serde round-trips, and
//! ID generation.

use apexkit_core::models::schema::{
    CollectionPolicies, CollectionSchema, FieldDefinition, FieldType, RelationDefinition,
    RelationType, generate_hex_id,
};
use serde_json::json;

#[test]
fn default_policies_are_secure() {
    let p = CollectionPolicies::default();
    assert_eq!(p.read, "public");
    assert_eq!(p.create, "auth");
    assert_eq!(p.update, "admin");
    assert_eq!(p.delete, "admin");
}

#[test]
fn generate_hex_id_is_nonempty_hex() {
    let id = generate_hex_id();
    assert!(!id.is_empty());
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    // Two calls should (almost surely) differ.
    assert_ne!(generate_hex_id(), generate_hex_id());
}

#[test]
fn schema_deserializes_with_minimal_field_definitions() {
    let v = json!({
        "fields": {
            "title": { "type": "text", "required": true }
        }
    });
    let schema: CollectionSchema = serde_json::from_value(v).unwrap();
    let f = schema.fields.get("title").unwrap();
    assert_eq!(f.r#type, FieldType::Text);
    assert!(f.required);
    // `auto` defaults to true, `uid` is auto-generated.
    assert!(f.auto);
    assert!(!f.uid.is_empty());
    // Default policies apply when omitted.
    assert_eq!(schema.policies.read, "public");
}

#[test]
fn schema_round_trips_through_json() {
    let schema = CollectionSchema {
        fields: {
            let mut m = std::collections::HashMap::new();
            m.insert(
                "n".to_string(),
                FieldDefinition {
                    r#type: FieldType::Number,
                    required: false,
                    default: Some(json!(0)),
                    ose_indexed: true,
                    sql_indexed: true,
                    vectorize: false,
                    auto: false,
                    position: 1,
                    uid: "abc".into(),
                    unique: Some(true),
                    min: Some(0.0),
                    max: Some(10.0),
                    min_length: None,
                    max_length: None,
                    pattern: None,
                    options: None,
                    mime_types: None,
                    max_size: None,
                    dimension: None,
                    relation_to: None,
                },
            );
            m
        },
        policies: CollectionPolicies::default(),
        relations: std::collections::HashMap::new(),
        field_history: std::collections::HashMap::new(),
        composite_unique: vec![],
    };

    let s = serde_json::to_string(&schema).unwrap();
    let back: CollectionSchema = serde_json::from_str(&s).unwrap();
    assert_eq!(back.fields.get("n").unwrap().uid, "abc");
    assert!(back.fields.get("n").unwrap().ose_indexed);
}

#[test]
fn relation_type_serializes_lowercase() {
    assert_eq!(serde_json::to_value(RelationType::One).unwrap(), json!("one"));
    assert_eq!(serde_json::to_value(RelationType::Many).unwrap(), json!("many"));
}

#[test]
fn relation_definition_defaults() {
    let v = json!({
        "target_collection": "users",
        "relation_type": "one"
    });
    let rel: RelationDefinition = serde_json::from_value(v).unwrap();
    assert_eq!(rel.target_collection, "users");
    assert!(!rel.required);
    assert!(!rel.cascade_on_target_delete);
    assert!(!rel.uid.is_empty());
}

#[test]
fn all_field_types_round_trip() {
    let types = [
        FieldType::String,
        FieldType::Text,
        FieldType::Number,
        FieldType::Boolean,
        FieldType::Email,
        FieldType::Url,
        FieldType::Date,
        FieldType::Select,
        FieldType::Json,
        FieldType::File,
        FieldType::Blob,
        FieldType::Relation,
        FieldType::Owner,
        FieldType::Vector,
        FieldType::GeoPoint,
    ];
    for t in types {
        let s = serde_json::to_string(&t).unwrap();
        let back: FieldType = serde_json::from_str(&s).unwrap();
        assert_eq!(back, t);
    }
}
