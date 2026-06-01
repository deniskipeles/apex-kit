use crate::schema::{CollectionSchema, FieldType};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::DateTime;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use url::Url;

#[derive(Error, Debug, PartialEq, Serialize)]
pub enum ValidationError {
    #[error("Missing required field: {0}")]
    MissingRequiredField(String),
    #[error("Invalid type for field '{0}': expected {1}")]
    InvalidType(String, String),
    #[error("Validation failed for field '{0}': {1}")]
    ConstraintViolation(String, String),
}

// Sanitizes data (type coercion, strict schema filtering) AND validates it against the schema
pub fn sanitize_and_validate(
    schema: &CollectionSchema,
    data: &mut Value,
) -> Result<(), Vec<ValidationError>> {
    // 1. Sanitize / Coerce Types & Strip Unknown Fields
    if let Some(map) = data.as_object_mut() {
        // A. Strict Schema Filtering: Remove any field NOT defined in schema.fields or schema.relations
        let mut keys_to_remove = Vec::new();
        for key in map.keys() {
            if !schema.fields.contains_key(key) && !schema.relations.contains_key(key) {
                keys_to_remove.push(key.clone());
            }
        }
        for key in keys_to_remove {
            map.remove(&key);
        }

        // B. Coerce Types for Standard Fields
        for (field_name, field_def) in &schema.fields {
            if let Some(val) = map.get_mut(field_name) {
                // If the field is Owner or Relation, force it to be an Integer
                if (field_def.r#type == FieldType::Owner || field_def.r#type == FieldType::Relation)
                    && let Some(str_val) = val.as_str()
                    && let Ok(num) = str_val.parse::<i64>()
                {
                    *val = serde_json::json!(num);
                }

                // Remove empty strings for nullable numbers/relations/dates to avoid type errors
                if val.as_str().map(|s| s.trim().is_empty()).unwrap_or(false)
                    && (field_def.r#type == FieldType::Number
                        || field_def.r#type == FieldType::Relation
                        || field_def.r#type == FieldType::Owner
                        || field_def.r#type == FieldType::Date)
                {
                    *val = Value::Null;
                }
            }
        }

        // C. Coerce Types for Virtual Relation Arrays/IDs
        for rel_name in schema.relations.keys() {
            if let Some(val) = map.get_mut(rel_name) {
                if let Value::Array(arr) = val {
                    for item in arr.iter_mut() {
                        if let Some(s) = item.as_str()
                            && let Ok(num) = s.parse::<i64>()
                        {
                            *item = serde_json::json!(num);
                        }
                    }
                } else if let Some(s) = val.as_str()
                    && let Ok(num) = s.parse::<i64>()
                {
                    *val = serde_json::json!(num);
                }
            }
        }
    }

    // 2. Validate against schema rules
    validate_record(schema, data)
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
                "root".to_string(),
                "object".to_string(),
            ));
            return Err(errors);
        }
    };

    for (field_name, field_def) in &schema.fields {
        let value = data_map.get(field_name);

        if value.is_none() || value.unwrap().is_null() {
            if field_def.required {
                errors.push(ValidationError::MissingRequiredField(field_name.clone()));
            }
            continue;
        }

        let val = value.unwrap();

        if let Err(msg) = validate_field_type(val, field_def) {
            errors.push(ValidationError::InvalidType(
                field_name.clone(),
                format!("{:?} ({})", field_def.r#type, msg),
            ));
        } else if let Err(msg) = validate_constraints(val, field_def) {
            errors.push(ValidationError::ConstraintViolation(
                field_name.clone(),
                msg,
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_field_type(val: &Value, def: &crate::schema::FieldDefinition) -> Result<(), String> {
    match def.r#type {
        FieldType::String | FieldType::Text | FieldType::File => {
            if !val.is_string() {
                return Err("Expected String".into());
            }
        }
        FieldType::Relation | FieldType::Owner | FieldType::Number => {
            if !val.is_number() {
                return Err("Expected Number".into());
            }
        }
        FieldType::Boolean => {
            if !val.is_boolean() {
                return Err("Expected Boolean".into());
            }
        }
        FieldType::Json => {
            if !val.is_object() && !val.is_array() {
                return Err("Expected Object or Array".into());
            }
        }
        FieldType::Email => {
            let s = val.as_str().ok_or("Expected String")?;
            let re = Regex::new(r"^[\w\-\.]+@([\w-]+\.)+[\w-]{2,4}$").unwrap();
            if !re.is_match(s) {
                return Err("Invalid Email Format".into());
            }
        }
        FieldType::Url => {
            let s = val.as_str().ok_or("Expected String")?;
            if Url::parse(s).is_err() {
                return Err("Invalid URL Format".into());
            }
        }
        FieldType::Date => {
            let s = val.as_str().ok_or("Expected ISO 8601 Date String")?;
            if DateTime::parse_from_rfc3339(s).is_err() {
                return Err("Invalid ISO Date".into());
            }
        }
        FieldType::Select => {
            let s = val.as_str().ok_or("Expected String")?;
            if let Some(opts) = &def.options
                && !opts.contains(&s.to_string())
            {
                return Err(format!("Value must be one of: {:?}", opts));
            }
        }
        FieldType::Vector => {
            let arr = val.as_array().ok_or("Expected Array of Numbers")?;
            for item in arr {
                if !item.is_number() {
                    return Err("Vector must contain numbers".into());
                }
            }
            if let Some(dim) = def.dimension
                && arr.len() != dim
            {
                return Err(format!(
                    "Vector dimension mismatch. Expected {}, got {}",
                    dim,
                    arr.len()
                ));
            }
        }
        FieldType::Blob => {
            let s = val.as_str().ok_or("Expected Base64 String")?;
            if STANDARD.decode(s).is_err() {
                return Err("Invalid Base64 Data".into());
            }
        }
        FieldType::GeoPoint => {
            if let Some(obj) = val.as_object() {
                let lat = obj.get("lat").and_then(|v| v.as_f64());
                let lng = obj
                    .get("lng")
                    .or_else(|| obj.get("lon"))
                    .and_then(|v| v.as_f64());
                if lat.is_none() || lng.is_none() {
                    return Err("GeoPoint must have 'lat' and 'lng' numbers".into());
                }
                let (lat_val, lng_val) = (lat.unwrap(), lng.unwrap());
                if !(-90.0..=90.0).contains(&lat_val) {
                    return Err("Latitude must be between -90 and 90".into());
                }
                if !(-180.0..=180.0).contains(&lng_val) {
                    return Err("Longitude must be between -180 and 180".into());
                }
            } else {
                return Err("GeoPoint must be a JSON object".into());
            }
        }
    }
    Ok(())
}

fn validate_constraints(val: &Value, def: &crate::schema::FieldDefinition) -> Result<(), String> {
    if let Some(n) = val.as_f64() {
        if let Some(min) = def.min
            && n < min
        {
            return Err(format!("Value {} is less than min {}", n, min));
        }
        if let Some(max) = def.max
            && n > max
        {
            return Err(format!("Value {} is greater than max {}", n, max));
        }
    }

    let len = if let Some(s) = val.as_str() {
        Some(s.len())
    } else {
        val.as_array().map(|a| a.len())
    };

    if let Some(l) = len {
        if let Some(min) = def.min_length
            && l < min
        {
            return Err(format!("Length {} is less than min {}", l, min));
        }
        if let Some(max) = def.max_length
            && l > max
        {
            return Err(format!("Length {} is greater than max {}", l, max));
        }
    }

    if let Some(s) = val.as_str()
        && let Some(pat) = &def.pattern
        && let Ok(re) = Regex::new(pat)
        && !re.is_match(s)
    {
        return Err(format!("Value does not match pattern: {}", pat));
    }
    Ok(())
}
