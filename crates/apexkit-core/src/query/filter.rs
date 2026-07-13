use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Nin,
    Like,
    Contains,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogicOp {
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum FilterNode {
    Condition {
        field: String,
        op: FilterOp,
        value: Value,
    },
    Group {
        op: LogicOp,
        children: Vec<FilterNode>,
    },
    Empty,
}

pub fn json_to_inline_sql(val: &Value) -> String {
    match val {
        Value::String(s) => format!("'{}'", s.replace("'", "''")),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => {
            if *b {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        Value::Null => "NULL".to_string(),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_to_inline_sql).collect();
            format!("({})", items.join(","))
        }
        _ => format!("'{}'", val.to_string().replace("'", "''")),
    }
}

impl FilterNode {
    pub fn parse(json: &Value) -> Self {
        match json {
            Value::Object(map) => {
                let mut conditions = Vec::new();

                for (key, val) in map {
                    if key == "$and" || key == "$or" {
                        let op = if key == "$and" {
                            LogicOp::And
                        } else {
                            LogicOp::Or
                        };
                        if let Value::Array(arr) = val {
                            let children: Vec<FilterNode> =
                                arr.iter().map(FilterNode::parse).collect();
                            conditions.push(FilterNode::Group { op, children });
                        }
                    } else {
                        conditions.push(Self::parse_field_condition(key, val));
                    }
                }

                if conditions.is_empty() {
                    FilterNode::Empty
                } else if conditions.len() == 1 {
                    conditions.pop().unwrap()
                } else {
                    FilterNode::Group {
                        op: LogicOp::And,
                        children: conditions,
                    }
                }
            }
            _ => FilterNode::Empty,
        }
    }

    fn parse_field_condition(field: &str, val: &Value) -> FilterNode {
        if let Value::Object(map) = val {
            if let Some((op_key, op_val)) = map.iter().next() {
                let op = match op_key.as_str() {
                    "$eq" | "eq" | "_eq" => FilterOp::Eq,
                    "$neq" | "neq" | "_neq" => FilterOp::Neq,
                    "$gt" | "gt" | "_gt" => FilterOp::Gt,
                    "$gte" | "gte" | "_gte" => FilterOp::Gte,
                    "$lt" | "lt" | "_lt" => FilterOp::Lt,
                    "$lte" | "lte" | "_lte" => FilterOp::Lte,
                    "$in" | "in" | "_in" => FilterOp::In,
                    "$nin" | "nin" | "_nin" => FilterOp::Nin,
                    "$like" | "like" | "_like" => FilterOp::Like,
                    "$contains" | "contains" | "_contains" => FilterOp::Contains,
                    _ => {
                        return FilterNode::Condition {
                            field: field.to_string(),
                            op: FilterOp::Eq,
                            value: val.clone(),
                        };
                    }
                };

                return FilterNode::Condition {
                    field: field.to_string(),
                    op,
                    value: op_val.clone(),
                };
            }
        }

        FilterNode::Condition {
            field: field.to_string(),
            op: FilterOp::Eq,
            value: val.clone(),
        }
    }

    pub fn to_inline_sql(&self) -> Option<String> {
        match self {
            FilterNode::Empty => None,
            FilterNode::Group { op, children } => {
                let parts: Vec<String> =
                    children.iter().filter_map(|c| c.to_inline_sql()).collect();
                if parts.is_empty() {
                    return None;
                }
                let joiner = match op {
                    LogicOp::And => " AND ",
                    LogicOp::Or => " OR ",
                };
                Some(format!("({})", parts.join(joiner)))
            }
            FilterNode::Condition { field, op, value } => {
                let column = Self::format_json_column(field);

                match op {
                    FilterOp::In | FilterOp::Nin => {
                        if let Value::Array(arr) = value {
                            if arr.is_empty() {
                                return Some("1=0".to_string());
                            }
                            let mut sql_clauses = Vec::new();
                            for v in arr {
                                let inline_val = json_to_inline_sql(v);
                                sql_clauses.push(format!(
                                    "({} = {} OR CAST({} AS TEXT) = CAST({} AS TEXT))",
                                    column, inline_val, column, inline_val
                                ));
                            }
                            let joiner = if matches!(op, FilterOp::Nin) {
                                " AND NOT "
                            } else {
                                " OR "
                            };
                            let prefix = if matches!(op, FilterOp::Nin) {
                                "NOT "
                            } else {
                                ""
                            };
                            Some(format!("({}{})", prefix, sql_clauses.join(joiner)))
                        } else {
                            None
                        }
                    }
                    _ => {
                        let sql_op = match op {
                            FilterOp::Eq => "=",
                            FilterOp::Neq => "!=",
                            FilterOp::Gt => ">",
                            FilterOp::Gte => ">=",
                            FilterOp::Lt => "<",
                            FilterOp::Lte => "<=",
                            FilterOp::Like | FilterOp::Contains => "LIKE",
                            _ => "=",
                        };

                        let inline_val = if matches!(op, FilterOp::Contains) && value.is_string() {
                            format!("'%{}%'", value.as_str().unwrap().replace("'", "''"))
                        } else {
                            json_to_inline_sql(value)
                        };

                        if matches!(op, FilterOp::Eq) {
                            Some(format!(
                                "({} = {} OR CAST({} AS TEXT) = CAST({} AS TEXT))",
                                column, inline_val, column, inline_val
                            ))
                        } else if matches!(op, FilterOp::Neq) {
                            Some(format!(
                                "({} != {} AND CAST({} AS TEXT) != CAST({} AS TEXT))",
                                column, inline_val, column, inline_val
                            ))
                        } else {
                            Some(format!("{} {} {}", column, sql_op, inline_val))
                        }
                    }
                }
            }
        }
    }

    pub fn to_sql(&self) -> Option<(String, Vec<rusqlite::types::Value>)> {
        let mut params = Vec::new();
        let sql = self.build_sql(&mut params)?;
        Some((sql, params))
    }

    fn build_sql(&self, params: &mut Vec<rusqlite::types::Value>) -> Option<String> {
        match self {
            FilterNode::Empty => None,
            FilterNode::Group { op, children } => {
                let parts: Vec<String> = children
                    .iter()
                    .filter_map(|c| c.build_sql(params))
                    .collect();

                if parts.is_empty() {
                    return None;
                }

                let joiner = match op {
                    LogicOp::And => " AND ",
                    LogicOp::Or => " OR ",
                };
                Some(format!("({})", parts.join(joiner)))
            }
            FilterNode::Condition { field, op, value } => {
                let column = Self::format_json_column(field);

                match op {
                    FilterOp::In | FilterOp::Nin => {
                        if let Value::Array(arr) = value {
                            if arr.is_empty() {
                                return Some("1=0".to_string());
                            }
                            let mut sql_clauses = Vec::new();
                            for v in arr {
                                let val = json_to_sql_val(v);
                                params.push(val.clone());
                                params.push(val);
                                sql_clauses.push(format!(
                                    "({} = ? OR CAST({} AS TEXT) = CAST(? AS TEXT))",
                                    column, column
                                ));
                            }
                            let joiner = if matches!(op, FilterOp::Nin) {
                                " AND NOT "
                            } else {
                                " OR "
                            };
                            let prefix = if matches!(op, FilterOp::Nin) {
                                "NOT "
                            } else {
                                ""
                            };
                            Some(format!("({}{})", prefix, sql_clauses.join(joiner)))
                        } else {
                            None
                        }
                    }
                    _ => {
                        let sql_op = match op {
                            FilterOp::Eq => "=",
                            FilterOp::Neq => "!=",
                            FilterOp::Gt => ">",
                            FilterOp::Gte => ">=",
                            FilterOp::Lt => "<",
                            FilterOp::Lte => "<=",
                            FilterOp::Like => "LIKE",
                            FilterOp::Contains => "LIKE",
                            _ => "=",
                        };

                        let mut final_val = json_to_sql_val(value);

                        if matches!(op, FilterOp::Contains)
                            && let Value::String(s) = value
                        {
                            final_val = rusqlite::types::Value::Text(format!("%{}%", s));
                        }

                        if matches!(op, FilterOp::Eq) {
                            params.push(final_val.clone());
                            params.push(final_val);
                            Some(format!(
                                "({} = ? OR CAST({} AS TEXT) = CAST(? AS TEXT))",
                                column, column
                            ))
                        } else if matches!(op, FilterOp::Neq) {
                            params.push(final_val.clone());
                            params.push(final_val);
                            Some(format!(
                                "({} != ? AND CAST({} AS TEXT) != CAST(? AS TEXT))",
                                column, column
                            ))
                        } else {
                            params.push(final_val);
                            Some(format!("{} {} ?", column, sql_op))
                        }
                    }
                }
            }
        }
    }

    fn format_json_column(key: &str) -> String {
        let clean_key = if key.starts_with("@record.data.") {
            key.strip_prefix("@record.data.").unwrap()
        } else if key.starts_with("@record.") {
            key.strip_prefix("@record.").unwrap()
        } else {
            key
        };

        if clean_key == "id" {
            return "records.id".to_string();
        }
        if clean_key == "created" {
            return "records.created".to_string();
        }
        if clean_key == "updated" {
            return "records.updated".to_string();
        }

        let parts: Vec<&str> = clean_key.split('.').collect();
        let mut sql = "records.data".to_string();

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                write!(sql, " ->> '{}'", part).unwrap();
            } else {
                write!(sql, " -> '{}'", part).unwrap();
            }
        }
        sql
    }

    pub fn matches(&self, record_data: &Value) -> bool {
        match self {
            FilterNode::Empty => true,
            FilterNode::Group { op, children } => match op {
                LogicOp::And => children.iter().all(|c| c.matches(record_data)),
                LogicOp::Or => children.iter().any(|c| c.matches(record_data)),
            },
            FilterNode::Condition { field, op, value } => {
                let record_val = Self::extract_json_val(record_data, field);
                Self::compare_values(record_val, op, value)
            }
        }
    }

    fn extract_json_val<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
        let clean_path = if path.starts_with("@record.data.") {
            path.strip_prefix("@record.data.").unwrap()
        } else if path.starts_with("@record.") {
            path.strip_prefix("@record.").unwrap()
        } else {
            path
        };
        let mut current = data;
        for key in clean_path.split('.') {
            current = current.get(key)?;
        }
        Some(current)
    }

    fn compare_values(actual: Option<&Value>, op: &FilterOp, expected: &Value) -> bool {
        if actual.is_none() {
            return matches!(op, FilterOp::Neq) && !expected.is_null();
        }
        let act = actual.unwrap();

        let act_str = match act {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => act.to_string(),
        };

        let exp_str = match expected {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => expected.to_string(),
        };

        match op {
            FilterOp::Eq => act_str == exp_str,
            FilterOp::Neq => act_str != exp_str,
            FilterOp::Gt => json_cmp(act, expected).map(|r| r > 0).unwrap_or(false),
            FilterOp::Gte => json_cmp(act, expected).map(|r| r >= 0).unwrap_or(false),
            FilterOp::Lt => json_cmp(act, expected).map(|r| r < 0).unwrap_or(false),
            FilterOp::Lte => json_cmp(act, expected).map(|r| r <= 0).unwrap_or(false),
            FilterOp::In => {
                if let Value::Array(arr) = expected {
                    arr.iter().any(|item| {
                        let item_str = match item {
                            Value::String(s) => s.clone(),
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            _ => item.to_string(),
                        };
                        act_str == item_str
                    })
                } else {
                    false
                }
            }
            FilterOp::Nin => {
                if let Value::Array(arr) = expected {
                    arr.iter().all(|item| {
                        let item_str = match item {
                            Value::String(s) => s.clone(),
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            _ => item.to_string(),
                        };
                        act_str != item_str
                    })
                } else {
                    true
                }
            }
            FilterOp::Contains => {
                if let Value::String(s_act) = act
                    && let Value::String(s_exp) = expected
                {
                    return s_act.contains(s_exp);
                }
                false
            }
            _ => false,
        }
    }
}

fn json_to_sql_val(v: &Value) -> rusqlite::types::Value {
    match v {
        Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        Value::Number(n) => rusqlite::types::Value::Real(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
        Value::Null => rusqlite::types::Value::Null,
        _ => rusqlite::types::Value::Text(v.to_string()),
    }
}

fn json_cmp(a: &Value, b: &Value) -> Option<i32> {
    if let (Some(n1), Some(n2)) = (a.as_f64(), b.as_f64()) {
        if n1 < n2 {
            return Some(-1);
        }
        if n1 > n2 {
            return Some(1);
        }
        return Some(0);
    }
    if let (Some(s1), Some(s2)) = (a.as_str(), b.as_str()) {
        return Some(s1.cmp(s2) as i32);
    }
    None
}
