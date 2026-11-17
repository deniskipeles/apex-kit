//! Unit tests for schema-driven validation and sanitization.

use apexkit_core::models::schema::{CollectionSchema, FieldDefinition, FieldType};
use apexkit_core::validation::{ValidationError, sanitize_and_validate, validate_record};
use serde_json::json;

fn field(t: FieldType, required: bool) -> FieldDefinition {
    FieldDefinition {
        r#type: t,
        required,
        default: None,
        ose_indexed: false,
        sql_indexed: false,
        vectorize: false,
        auto: false,
        position: 0,
        uid: "u".to_string(),
        unique: None,
        min: None,
        max: None,
        min_length: None,
        max_length: None,
        pattern: None,
        options: None,
        mime_types: None,
        max_size: None,
        dimension: None,
        relation_to: None,
    }
}

fn schema_with(fields: Vec<(&str, FieldDefinition)>) -> CollectionSchema {
    let mut s = CollectionSchema::default();
    for (name, def) in fields {
        s.fields.insert(name.to_string(), def);
    }
    s
}

#[test]
fn missing_required_field_is_reported() {
    let schema = schema_with(vec![("title", field(FieldType::Text, true))]);
    let data = json!({});
    let errs = validate_record(&schema, &data).unwrap_err();
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0], ValidationError::MissingRequiredField("title".into()));
}

#[test]
fn optional_missing_field_is_allowed() {
    let schema = schema_with(vec![("title", field(FieldType::Text, false))]);
    assert!(validate_record(&schema, &json!({})).is_ok());
}

#[test]
fn wrong_type_is_rejected() {
    let schema = schema_with(vec![("views", field(FieldType::Number, true))]);
    let errs = validate_record(&schema, &json!({"views": "not a number"})).unwrap_err();
    assert!(matches!(errs[0], ValidationError::InvalidType(_, _)));
}

#[test]
fn non_object_root_is_rejected() {
    let schema = schema_with(vec![]);
    let errs = validate_record(&schema, &json!([1, 2, 3])).unwrap_err();
    assert!(matches!(errs[0], ValidationError::InvalidType(ref f, _) if f == "root"));
}

#[test]
fn email_field_validates_format() {
    let schema = schema_with(vec![("email", field(FieldType::Email, true))]);
    assert!(validate_record(&schema, &json!({"email": "a@b.com"})).is_ok());
    assert!(validate_record(&schema, &json!({"email": "not-an-email"})).is_err());
}

#[test]
fn url_field_validates_format() {
    let schema = schema_with(vec![("link", field(FieldType::Url, true))]);
    assert!(validate_record(&schema, &json!({"link": "https://example.com/x"})).is_ok());
    assert!(validate_record(&schema, &json!({"link": "::bad::"})).is_err());
}

#[test]
fn date_field_requires_rfc3339() {
    let schema = schema_with(vec![("when", field(FieldType::Date, true))]);
    assert!(validate_record(&schema, &json!({"when": "2024-01-01T00:00:00Z"})).is_ok());
    assert!(validate_record(&schema, &json!({"when": "01/01/2024"})).is_err());
}

#[test]
fn select_field_rejects_value_outside_options() {
    let mut def = field(FieldType::Select, true);
    def.options = Some(vec!["draft".into(), "published".into()]);
    let schema = schema_with(vec![("status", def)]);

    assert!(validate_record(&schema, &json!({"status": "draft"})).is_ok());
    assert!(validate_record(&schema, &json!({"status": "archived"})).is_err());
}

#[test]
fn number_constraints_min_max_are_enforced() {
    let mut def = field(FieldType::Number, true);
    def.min = Some(0.0);
    def.max = Some(100.0);
    let schema = schema_with(vec![("score", def)]);

    assert!(validate_record(&schema, &json!({"score": 50})).is_ok());
    assert!(validate_record(&schema, &json!({"score": -1})).is_err());
    assert!(validate_record(&schema, &json!({"score": 101})).is_err());
}

#[test]
fn string_length_and_pattern_constraints_are_enforced() {
    let mut def = field(FieldType::String, true);
    def.min_length = Some(3);
    def.max_length = Some(5);
    def.pattern = Some("^[a-z]+$".into());
    let schema = schema_with(vec![("code", def)]);

    assert!(validate_record(&schema, &json!({"code": "abcd"})).is_ok());
    assert!(validate_record(&schema, &json!({"code": "ab"})).is_err());
    assert!(validate_record(&schema, &json!({"code": "abcdef"})).is_err());
    assert!(validate_record(&schema, &json!({"code": "ABCD"})).is_err());
}

#[test]
fn vector_dimension_mismatch_is_rejected() {
    let mut def = field(FieldType::Vector, true);
    def.dimension = Some(3);
    let schema = schema_with(vec![("emb", def)]);

    assert!(validate_record(&schema, &json!({"emb": [1.0, 2.0, 3.0]})).is_ok());
    assert!(validate_record(&schema, &json!({"emb": [1.0, 2.0]})).is_err());
    assert!(validate_record(&schema, &json!({"emb": ["x", 2, 3]})).is_err());
}

#[test]
fn geoppoint_bounds_are_enforced() {
    let schema = schema_with(vec![("loc", field(FieldType::GeoPoint, true))]);
    assert!(validate_record(&schema, &json!({"loc": {"lat": 10.0, "lng": 20.0}})).is_ok());
    assert!(validate_record(&schema, &json!({"loc": {"lat": 91.0, "lng": 20.0}})).is_err());
    assert!(validate_record(&schema, &json!({"loc": {"lat": 10.0, "lng": 181.0}})).is_err());
    assert!(validate_record(&schema, &json!({"loc": {"lat": 10.0}})).is_err());
}

#[test]
fn blob_field_requires_valid_base64() {
    let schema = schema_with(vec![("data", field(FieldType::Blob, true))]);
    assert!(validate_record(&schema, &json!({"data": "aGVsbG8="})).is_ok());
    assert!(validate_record(&schema, &json!({"data": "!!!not base64!!!"})).is_err());
}

#[test]
fn relation_field_rejects_arrays() {
    let mut s = CollectionSchema::default();
    s.fields.insert("author".to_string(), field(FieldType::Relation, true));
    let errs = validate_record(&s, &json!({"author": [1, 2]})).unwrap_err();
    assert!(matches!(errs[0], ValidationError::InvalidType(_, _)));
}

#[test]
fn sanitize_strips_unknown_fields() {
    let schema = schema_with(vec![("title", field(FieldType::Text, true))]);
    let mut data = json!({"title": "ok", "secret": "drop me"});
    sanitize_and_validate(&schema, &mut data).unwrap();
    assert_eq!(data, json!({"title": "ok"}));
}

#[test]
fn sanitize_coerces_numeric_strings_for_relations() {
    let mut s = CollectionSchema::default();
    s.fields.insert("author".to_string(), field(FieldType::Relation, true));
    let mut data = json!({"author": "42"});
    sanitize_and_validate(&s, &mut data).unwrap();
    assert_eq!(data["author"], json!(42), "string id must be coerced to number");
}

#[test]
fn sanitize_nullifies_blank_numeric_fields() {
    let schema = schema_with(vec![("views", field(FieldType::Number, false))]);
    let mut data = json!({"views": "   "});
    sanitize_and_validate(&schema, &mut data).unwrap();
    assert_eq!(data["views"], json!(null));
}

#[test]
fn multiple_errors_are_collected() {
    let schema = schema_with(vec![
        ("title", field(FieldType::Text, true)),
        ("views", field(FieldType::Number, true)),
    ]);
    let errs = validate_record(&schema, &json!({"views": "nope"})).unwrap_err();
    assert!(errs.len() >= 2, "both missing title and bad views reported");
}
