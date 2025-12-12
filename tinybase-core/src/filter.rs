// =========================== tinybase-core/src/filter.rs ===========================
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOp {
    Eq, Neq, Gt, Gte, Lt, Lte, In, Nin, Like, Contains
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogicOp {
    And, Or
}

#[derive(Debug, Clone)]
pub enum FilterNode {
    /// Comparison: field, operator, value
    Condition {
        field: String,
        op: FilterOp,
        value: Value,
    },
    /// Logic: AND/OR list of nodes
    Group {
        op: LogicOp,
        children: Vec<FilterNode>,
    },
    Empty,
}

impl FilterNode {
    /// Parses a JSON object into a Filter Tree
    /// Supports MongoDB-style syntax: 
    /// { "age": { "$gt": 18 }, "$or": [...] }
    pub fn parse(json: &Value) -> Self {
        match json {
            Value::Object(map) => {
                let mut conditions = Vec::new();

                for (key, val) in map {
                    if key == "$and" || key == "$or" {
                        // Logical Group
                        let op = if key == "$and" { LogicOp::And } else { LogicOp::Or };
                        if let Value::Array(arr) = val {
                            let children: Vec<FilterNode> = arr.iter().map(FilterNode::parse).collect();
                            conditions.push(FilterNode::Group { op, children });
                        }
                    } else {
                        // Field Condition
                        conditions.push(Self::parse_field_condition(key, val));
                    }
                }

                if conditions.is_empty() {
                    FilterNode::Empty
                } else if conditions.len() == 1 {
                    conditions.pop().unwrap()
                } else {
                    // Implicit top-level AND
                    FilterNode::Group { op: LogicOp::And, children: conditions }
                }
            }
            _ => FilterNode::Empty,
        }
    }

    fn parse_field_condition(field: &str, val: &Value) -> FilterNode {
        if let Value::Object(map) = val {
            // Check for operators: { "age": { "$gt": 18 } }
            // For simplicity, we take the first operator found. Complex fields should use $and
            if let Some((op_key, op_val)) = map.iter().next() {
                let op = match op_key.as_str() {
                    "$eq" => FilterOp::Eq,
                    "$neq" => FilterOp::Neq,
                    "$gt" => FilterOp::Gt,
                    "$gte" => FilterOp::Gte,
                    "$lt" => FilterOp::Lt,
                    "$lte" => FilterOp::Lte,
                    "$in" => FilterOp::In,
                    "$nin" => FilterOp::Nin,
                    "$like" => FilterOp::Like,
                    "$contains" => FilterOp::Contains, // Array contains or String contains
                    _ => return FilterNode::Condition { field: field.to_string(), op: FilterOp::Eq, value: val.clone() } // Treat entire object as equality match
                };
                return FilterNode::Condition { field: field.to_string(), op, value: op_val.clone() };
            }
        }
        
        // Simple Equality: { "status": "active" }
        FilterNode::Condition { field: field.to_string(), op: FilterOp::Eq, value: val.clone() }
    }

    // --- SQL GENERATION (For Database Queries) ---
    
    pub fn to_sql(&self) -> Option<(String, Vec<libsql::Value>)> {
        let mut params = Vec::new();
        let sql = self.build_sql(&mut params)?;
        Some((sql, params))
    }

    fn build_sql(&self, params: &mut Vec<libsql::Value>) -> Option<String> {
        match self {
            FilterNode::Empty => None,
            FilterNode::Group { op, children } => {
                let parts: Vec<String> = children.iter()
                    .filter_map(|c| c.build_sql(params))
                    .collect();
                
                if parts.is_empty() { return None; }
                
                let joiner = match op { LogicOp::And => " AND ", LogicOp::Or => " OR " };
                Some(format!("({})", parts.join(joiner)))
            },
            FilterNode::Condition { field, op, value } => {
                // SQLite JSON Extraction: data ->> 'field' or data -> 'nested' ->> 'field'
                let column = Self::format_json_column(field);
                
                match op {
                    FilterOp::In | FilterOp::Nin => {
                        if let Value::Array(arr) = value {
                            if arr.is_empty() { return Some("1=0".to_string()); } // In empty list is always false
                            let placeholders: Vec<String> = arr.iter().map(|_| "?".to_string()).collect();
                            for v in arr { params.push(json_to_sql_val(v)); }
                            let not = if matches!(op, FilterOp::Nin) { "NOT " } else { "" };
                            Some(format!("{} {}IN ({})", column, not, placeholders.join(",")))
                        } else {
                            None // Invalid syntax
                        }
                    },
                    _ => {
                        let sql_op = match op {
                            FilterOp::Eq => "=",
                            FilterOp::Neq => "!=",
                            FilterOp::Gt => ">",
                            FilterOp::Gte => ">=",
                            FilterOp::Lt => "<",
                            FilterOp::Lte => "<=",
                            FilterOp::Like => "LIKE",
                            FilterOp::Contains => "LIKE", // Simple string contains for SQL
                            _ => "=" 
                        };
                        
                        let mut final_val = json_to_sql_val(value);
                        
                        if matches!(op, FilterOp::Contains) {
                            if let Value::String(s) = value {
                                final_val = libsql::Value::Text(format!("%{}%", s));
                            }
                        }

                        params.push(final_val);
                        Some(format!("{} {} ?", column, sql_op))
                    }
                }
            }
        }
    }

    // Handles dot notation: "address.zip" -> "data -> 'address' ->> 'zip'"
    fn format_json_column(key: &str) -> String {
        if key == "id" { return "id".to_string(); } // Native ID column
        if key == "created" { return "created".to_string(); } // TODO: Add created/updated columns to schema if not json
        
        let parts: Vec<&str> = key.split('.').collect();
        let mut sql = "data".to_string();
        
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                write!(sql, " ->> '{}'", part).unwrap(); // Last one extracts value
            } else {
                write!(sql, " -> '{}'", part).unwrap(); // Intermediates extract json object
            }
        }
        sql
    }

    // --- IN-MEMORY MATCHING (For WebSockets/Subscriptions) ---
    
    pub fn matches(&self, record_data: &Value) -> bool {
        match self {
            FilterNode::Empty => true,
            FilterNode::Group { op, children } => {
                match op {
                    LogicOp::And => children.iter().all(|c| c.matches(record_data)),
                    LogicOp::Or => children.iter().any(|c| c.matches(record_data)),
                }
            },
            FilterNode::Condition { field, op, value } => {
                let record_val = Self::extract_json_val(record_data, field);
                Self::compare_values(record_val, op, value)
            }
        }
    }

    fn extract_json_val<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
        let mut current = data;
        for key in path.split('.') {
            current = current.get(key)?;
        }
        Some(current)
    }

    fn compare_values(actual: Option<&Value>, op: &FilterOp, expected: &Value) -> bool {
        // Handle nulls
        if actual.is_none() {
            return matches!(op, FilterOp::Neq) && !expected.is_null();
        }
        let act = actual.unwrap();

        match op {
            FilterOp::Eq => act == expected,
            FilterOp::Neq => act != expected,
            FilterOp::Gt => json_cmp(act, expected).map(|r| r > 0).unwrap_or(false),
            FilterOp::Gte => json_cmp(act, expected).map(|r| r >= 0).unwrap_or(false),
            FilterOp::Lt => json_cmp(act, expected).map(|r| r < 0).unwrap_or(false),
            FilterOp::Lte => json_cmp(act, expected).map(|r| r <= 0).unwrap_or(false),
            FilterOp::In => {
                if let Value::Array(arr) = expected { arr.contains(act) } else { false }
            },
            FilterOp::Nin => {
                if let Value::Array(arr) = expected { !arr.contains(act) } else { true }
            },
            FilterOp::Contains => {
                if let Value::String(s_act) = act {
                    if let Value::String(s_exp) = expected {
                        return s_act.contains(s_exp);
                    }
                }
                false
            }
            _ => false
        }
    }
}

// Helpers
fn json_to_sql_val(v: &Value) -> libsql::Value {
    match v {
        Value::String(s) => s.clone().into(),
        Value::Number(n) => n.as_f64().unwrap_or(0.0).into(),
        Value::Bool(b) => if *b { 1 } else { 0 }.into(),
        Value::Null => libsql::Value::Null,
        _ => v.to_string().into(),
    }
}

// Compare two JSON values numerically or lexically
fn json_cmp(a: &Value, b: &Value) -> Option<i32> {
    if let (Some(n1), Some(n2)) = (a.as_f64(), b.as_f64()) {
        if n1 < n2 { return Some(-1); }
        if n1 > n2 { return Some(1); }
        return Some(0);
    }
    if let (Some(s1), Some(s2)) = (a.as_str(), b.as_str()) {
        return Some(s1.cmp(s2) as i32);
    }
    None
}