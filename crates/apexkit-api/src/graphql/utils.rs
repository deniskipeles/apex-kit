use apexkit_core::models::schema::{CollectionSchema, FieldType};
use async_graphql::{Value as GqlValue, dynamic::*};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use tracing::warn;

// --- CONFIG EXTRACTION LOGIC ---

#[derive(Deserialize, Debug, Clone)]
pub struct GraphqlConfig {
    pub parent: String,
    pub name: String,
    pub args: Option<HashMap<String, String>>,
    #[serde(rename = "returnType")]
    pub return_type: String,
}

pub fn extract_script_config(code: &str) -> Option<GraphqlConfig> {
    let re = Regex::new(r"export\s+const\s+graphql\s*=\s*(\{[\s\S]*?\})(?:;|\n|$)").ok()?;

    if let Some(caps) = re.captures(code) {
        let mut json_str = caps.get(1)?.as_str().to_string();

        if let Ok(re_comments) = Regex::new(r"//.*|/\*[\s\S]*?\*/") {
            json_str = re_comments.replace_all(&json_str, "").to_string();
        }

        if let Ok(re_keys) = Regex::new(r"(?m)(^|[\s{,])([a-zA-Z_]\w*)\s*:") {
            json_str = re_keys.replace_all(&json_str, r#"$1"$2":"#).to_string();
        }

        if let Ok(re_trailing) = Regex::new(r",\s*([\]}])") {
            json_str = re_trailing.replace_all(&json_str, "$1").to_string();
        }

        match serde_json::from_str::<GraphqlConfig>(&json_str) {
            Ok(cfg) => return Some(cfg),
            Err(e) => {
                warn!(
                    "Found 'graphql' config but failed to parse JSON: {}. \nSanitized: {}",
                    e, json_str
                );
                return None;
            }
        }
    }
    None
}

pub fn map_type_ref(type_name: &str) -> TypeRef {
    let is_non_null = type_name.ends_with('!');
    let clean_name = type_name.trim_end_matches('!');

    let is_list = clean_name.starts_with('[') && clean_name.ends_with(']');
    let inner_name = if is_list {
        clean_name.trim_start_matches('[').trim_end_matches(']')
    } else {
        clean_name
    };

    let base_ref = match inner_name {
        "String" => TypeRef::named(TypeRef::STRING),
        "Int" => TypeRef::named(TypeRef::INT),
        "Float" => TypeRef::named(TypeRef::FLOAT),
        "Boolean" => TypeRef::named(TypeRef::BOOLEAN),
        "ID" => TypeRef::named(TypeRef::ID),
        "JSON" => TypeRef::named("JSON"),
        _ => TypeRef::named(inner_name),
    };

    let mut t_ref = if is_list {
        TypeRef::List(Box::new(base_ref))
    } else {
        base_ref
    };
    if is_non_null {
        t_ref = TypeRef::NonNull(Box::new(t_ref));
    }
    t_ref
}

// --- HELPERS ---

// Convert GraphQL Input Value -> Serde JSON
pub fn gql_input_to_json(val: GqlValue) -> serde_json::Value {
    match val {
        GqlValue::Null => serde_json::Value::Null,
        GqlValue::String(s) => serde_json::Value::String(s),
        GqlValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                json!(i)
            } else if let Some(f) = n.as_f64() {
                json!(f)
            } else {
                json!(n.to_string()) // BigInt fallback
            }
        }
        GqlValue::Boolean(b) => serde_json::Value::Bool(b),
        GqlValue::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj {
                map.insert(k.to_string(), gql_input_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        GqlValue::List(arr) => {
            serde_json::Value::Array(arr.into_iter().map(gql_input_to_json).collect())
        }
        _ => serde_json::Value::String(val.to_string()), // Enum/Binary fallbacks
    }
}

// Logic to inject owner ID / dates (Matches REST API logic)
pub fn prepare_data_for_create(
    mut data: serde_json::Value,
    schema: &CollectionSchema,
    user_id: Option<i64>,
) -> serde_json::Value {
    if let Some(obj) = data.as_object_mut() {
        for (name, def) in &schema.fields {
            if !obj.contains_key(name) {
                match def.r#type {
                    FieldType::Owner if def.auto => {
                        if let Some(uid) = user_id {
                            obj.insert(name.clone(), json!(uid.to_string())); // Store as string ID usually
                        }
                    }
                    FieldType::Date if def.auto => {
                        obj.insert(name.clone(), json!(chrono::Utc::now().to_rfc3339()));
                    }
                    _ => {
                        if let Some(default) = &def.default {
                            obj.insert(name.clone(), default.clone());
                        }
                    }
                }
            }
        }
    }
    data
}

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub fn map_json_to_gql(val: Option<serde_json::Value>) -> async_graphql::Result<Option<GqlValue>> {
    match val {
        Some(serde_json::Value::String(s)) => Ok(Some(GqlValue::from(s))),
        Some(serde_json::Value::Number(n)) => {
            if let Some(f) = n.as_f64() {
                Ok(Some(GqlValue::from(f)))
            } else {
                Ok(Some(GqlValue::from(0)))
            }
        }
        Some(serde_json::Value::Bool(b)) => Ok(Some(GqlValue::from(b))),
        Some(_) => Ok(Some(GqlValue::from("Complex JSON"))),
        None => Ok(None),
    }
}

pub fn json_to_gql(json: serde_json::Value) -> GqlValue {
    match json {
        serde_json::Value::Null => GqlValue::Null,
        serde_json::Value::Bool(b) => GqlValue::Boolean(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                GqlValue::from(i)
            } else if let Some(f) = n.as_f64() {
                GqlValue::from(f)
            } else {
                GqlValue::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => GqlValue::String(s),
        serde_json::Value::Array(arr) => GqlValue::List(arr.into_iter().map(json_to_gql).collect()),
        serde_json::Value::Object(obj) => {
            let mut map = async_graphql::indexmap::IndexMap::new();
            for (k, v) in obj {
                map.insert(async_graphql::Name::new(k), json_to_gql(v));
            }
            GqlValue::Object(map)
        }
    }
}
