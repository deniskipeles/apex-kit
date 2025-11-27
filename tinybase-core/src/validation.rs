use serde::Serialize;
use crate::schema::{CollectionSchema, FieldType};
use serde_json::Value;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Serialize)]
pub enum ValidationError {
    #[error("Missing required field: {0}")]
    MissingRequiredField(String),
    #[error("Invalid type for field '{0}': expected {1}, got {2}")]
    InvalidType(String, String, String),
}

pub fn validate_record(
    schema: &CollectionSchema,
    data: &Value,
) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    let data_map = match data.as_object() {
        Some(map) => map,
        None => {
            errors.push(ValidationError::InvalidType(
                "data".to_string(),
                "object".to_string(),
                "not an object".to_string(),
            ));
            return Err(errors);
        }
    };

    for (field_name, field_def) in &schema.fields {
        match data_map.get(field_name) {
            Some(value) => {
                if !is_correct_type(value, &field_def.r#type) {
                    errors.push(ValidationError::InvalidType(
                        field_name.clone(),
                        format!("{:?}", field_def.r#type),
                        get_value_type(value),
                    ));
                }
            }
            None => {
                if field_def.required {
                    errors.push(ValidationError::MissingRequiredField(field_name.clone()));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn get_value_type(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

fn is_correct_type(value: &Value, field_type: &FieldType) -> bool {
    match field_type {
        FieldType::String => value.is_string(),
        FieldType::Text => value.is_string(),
        FieldType::Number => value.is_number(),
        FieldType::Boolean => value.is_boolean(),
        FieldType::Json => value.is_object() || value.is_array(),
    }
}
