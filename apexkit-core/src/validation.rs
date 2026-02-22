use serde::Serialize;
use crate::schema::{CollectionSchema, FieldType};
use serde_json::Value;
use thiserror::Error;
use regex::Regex;
use url::Url;
use chrono::DateTime;
use base64::{Engine as _, engine::general_purpose::STANDARD};

#[derive(Error, Debug, PartialEq, Serialize)]
pub enum ValidationError {
    #[error("Missing required field: {0}")]
    MissingRequiredField(String),
    #[error("Invalid type for field '{0}': expected {1}")]
    InvalidType(String, String),
    #[error("Validation failed for field '{0}': {1}")]
    ConstraintViolation(String, String),
}

pub fn validate_record(
    schema: &CollectionSchema,
    data: &Value,
) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    let data_map = match data.as_object() {
        Some(map) => map,
        None => {
            errors.push(ValidationError::InvalidType("root".to_string(), "object".to_string()));
            return Err(errors);
        }
    };

    for (field_name, field_def) in &schema.fields {
        let value = data_map.get(field_name);

        // 1. Required Check
        if value.is_none() || value.unwrap().is_null() {
            if field_def.required {
                errors.push(ValidationError::MissingRequiredField(field_name.clone()));
            }
            continue; // Skip further checks if null (unless required)
        }

        let val = value.unwrap();

        // 2. Type & Constraint Check
        if let Err(msg) = validate_field_type(val, field_def) {
            errors.push(ValidationError::InvalidType(field_name.clone(), format!("{:?} ({})", field_def.r#type, msg)));
        } else if let Err(msg) = validate_constraints(val, field_def) {
            errors.push(ValidationError::ConstraintViolation(field_name.clone(), msg));
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
            if !val.is_string() { return Err("Expected String".into()); }
        },
        FieldType::Relation | FieldType::Owner => {
            if !val.is_number() { return Err("Expected Number".into()); }
        },
        FieldType::Number => {
            if !val.is_number() { return Err("Expected Number".into()); }
        },
        FieldType::Boolean => {
            if !val.is_boolean() { return Err("Expected Boolean".into()); }
        },
        FieldType::Json => {
            if !val.is_object() && !val.is_array() { return Err("Expected Object or Array".into()); }
        },
        FieldType::Email => {
            let s = val.as_str().ok_or("Expected String")?;
            // Basic regex for email
            let re = Regex::new(r"^[\w\-\.]+@([\w-]+\.)+[\w-]{2,4}$").unwrap();
            if !re.is_match(s) { return Err("Invalid Email Format".into()); }
        },
        FieldType::Url => {
            let s = val.as_str().ok_or("Expected String")?;
            if Url::parse(s).is_err() { return Err("Invalid URL Format".into()); }
        },
        FieldType::Date => {
            let s = val.as_str().ok_or("Expected ISO 8601 Date String")?;
            if DateTime::parse_from_rfc3339(s).is_err() { return Err("Invalid ISO Date".into()); }
        },
        FieldType::Select => {
            let s = val.as_str().ok_or("Expected String")?;
            if let Some(opts) = &def.options {
                if !opts.contains(&s.to_string()) { return Err(format!("Value must be one of: {:?}", opts)); }
            }
        },
        FieldType::Vector => {
            let arr = val.as_array().ok_or("Expected Array of Numbers")?;
            // Check elements are numbers
            for item in arr {
                if !item.is_number() { return Err("Vector must contain numbers".into()); }
            }
            // Check Dimension
            if let Some(dim) = def.dimension {
                if arr.len() != dim { return Err(format!("Vector dimension mismatch. Expected {}, got {}", dim, arr.len())); }
            }
        },
        FieldType::Blob => {
            let s = val.as_str().ok_or("Expected Base64 String")?;
            // FIX: Use engine to decode
            if STANDARD.decode(s).is_err() { return Err("Invalid Base64 Data".into()); }
        },
        FieldType::GeoPoint => {
            // Expect object: { "lat": f64, "lng": f64 }
            if let Some(obj) = val.as_object() {
                let lat = obj.get("lat").and_then(|v| v.as_f64());
                let lng = obj.get("lng").or_else(|| obj.get("lon")).and_then(|v| v.as_f64());
                
                if lat.is_none() || lng.is_none() {
                    return Err("GeoPoint must have 'lat' and 'lng' (or 'lon') numbers".into());
                }
                
                let lat_val = lat.unwrap();
                let lng_val = lng.unwrap();
                
                if lat_val < -90.0 || lat_val > 90.0 { return Err("Latitude must be between -90 and 90".into()); }
                if lng_val < -180.0 || lng_val > 180.0 { return Err("Longitude must be between -180 and 180".into()); }
            } else {
                return Err("GeoPoint must be a JSON object".into());
            }
        },
        // _ => {} // Fallback for types not explicitly checked above if needed
    }
    Ok(())
}

fn validate_constraints(val: &Value, def: &crate::schema::FieldDefinition) -> Result<(), String> {
    // Number Constraints
    if let Some(n) = val.as_f64() {
        if let Some(min) = def.min {
            if n < min { return Err(format!("Value {} is less than min {}", n, min)); }
        }
        if let Some(max) = def.max {
            if n > max { return Err(format!("Value {} is greater than max {}", n, max)); }
        }
    }

    // String/Array Length Constraints
    let len = if let Some(s) = val.as_str() { Some(s.len()) }
              else if let Some(a) = val.as_array() { Some(a.len()) }
              else { None };

    if let Some(l) = len {
        if let Some(min) = def.min_length {
            if l < min { return Err(format!("Length {} is less than min {}", l, min)); }
        }
        if let Some(max) = def.max_length {
            if l > max { return Err(format!("Length {} is greater than max {}", l, max)); }
        }
    }

    // Regex Pattern (String only)
    if let Some(s) = val.as_str() {
        if let Some(pat) = &def.pattern {
            if let Ok(re) = Regex::new(pat) {
                if !re.is_match(s) { return Err(format!("Value does not match pattern: {}", pat)); }
            }
        }
    }

    Ok(())
}