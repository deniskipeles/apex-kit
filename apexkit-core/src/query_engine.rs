use serde::{Deserialize, Serialize};
use crate::Db;
use serde_json::{json, Value};
use crate::filter::FilterNode;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ApexQuery {
    pub from: String, // Collection Name
    
    // SQL Layer
    #[serde(default)]
    pub select: Vec<SelectField>, // e.g. [{"field": "price", "fn": "avg", "as": "avg_price"}]
    #[serde(default)]
    pub r#where: Option<Value>,   // FilterNode JSON
    #[serde(default)]
    pub group_by: Vec<String>,
    #[serde(default)]
    pub sort: Option<String>,     // SQL sort: "-created, category"
    pub limit: Option<u64>,
    pub offset: Option<u64>,

    // Rust Layer (Post-Processing)
    #[serde(default)]
    pub pipeline: Vec<PipelineStep>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum SelectField {
    Simple(String), // "name"
    Complex { 
        field: Option<String>, // "price"
        expr: Option<String>,  // Raw SQL expression (careful!) - "price * qty"
        #[serde(rename = "fn")]
        func: Option<String>,  // "sum", "avg", "count", "min", "max", "year" (strftime)
        #[serde(rename = "as")]
        alias: Option<String> 
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "op", content = "args")]
pub enum PipelineStep {
    #[serde(rename = "cumulative")]
    CumulativeSum { field: String, output_field: String },
    #[serde(rename = "pivot")]
    Pivot { key: String, value: String, agg: String }, // agg: sum, count
    #[serde(rename = "map")]
    Map { expression: String }, // Simple math: "row.a + row.b" (Parsed manually or via boa in future)
}

pub struct QueryBuilder;

impl QueryBuilder {
    pub async fn build(query: &ApexQuery, db: &impl Db) -> Result<(String, Vec<libsql::Value>, Vec<PipelineStep>), String> {
        let cols = db.list_collections().await.map_err(|e| e.to_string())?;
        let col = cols.iter().find(|c| c.name == query.from)
            .ok_or_else(|| format!("Collection '{}' not found", query.from))?;

        let mut params: Vec<libsql::Value> = vec![];
        let mut select_clauses = Vec::new();
        let mut group_clauses = Vec::new();

        // 1. Build SELECT
        if query.select.is_empty() {
            // Default to full record if no select specified (and no aggregation implied by group by)
            if query.group_by.is_empty() {
                 select_clauses.push("records.id".to_string());
                 select_clauses.push("json(records.data) as data".to_string());
                 select_clauses.push("records.created".to_string());
            } else {
                 // If grouping but no select, usually means select the groups
                 for g in &query.group_by {
                     select_clauses.push(Self::resolve_column(g));
                 }
            }
        } else {
            for field_def in &query.select {
                match field_def {
                    SelectField::Simple(name) => {
                        // [FIX] Add explicit alias to clean up the output JSON key
                        // Before: records.data ->> 'customerName'
                        // After: customerName
                        let col_expr = Self::resolve_column(name);
                        select_clauses.push(format!("{} as \"{}\"", col_expr, name));
                    },
                    SelectField::Complex { field, expr, func, alias } => {
                        let base_col = if let Some(e) = expr {
                            e.clone() // Raw expression (e.g. "1")
                        } else if let Some(f) = field {
                            Self::resolve_column(f)
                        } else {
                            "*".to_string() // e.g. for COUNT(*)
                        };

                        let sql_expr = if let Some(f) = func {
                            match f.to_lowercase().as_str() {
                                "count" => format!("COUNT({})", base_col),
                                "sum" => format!("SUM(CAST({} AS REAL))", base_col),
                                "avg" => format!("AVG(CAST({} AS REAL))", base_col),
                                "min" => format!("MIN(CAST({} AS REAL))", base_col),
                                "max" => format!("MAX(CAST({} AS REAL))", base_col),
                                "year" => format!("strftime('%Y', {})", base_col),
                                "month" => format!("strftime('%m', {})", base_col),
                                "day" => format!("strftime('%Y-%m-%d', {})", base_col),
                                _ => base_col // Fallback/Unknown
                            }
                        } else {
                            base_col
                        };

                        if let Some(a) = alias {
                            select_clauses.push(format!("{} as \"{}\"", sql_expr, a));
                        } else {
                            select_clauses.push(sql_expr);
                        }
                    }
                }
            }
        }

        // 2. Build WHERE
        let mut where_sql = format!("WHERE collection_id = {}", col.id);
        if let Some(filter_json) = &query.r#where {
            let node = FilterNode::parse(filter_json);
            if let Some((sql, p)) = node.to_sql() {
                where_sql.push_str(" AND ");
                where_sql.push_str(&sql);
                params.extend(p);
            }
        }

        // 3. Build GROUP BY
        if !query.group_by.is_empty() {
             for g in &query.group_by {
                 group_clauses.push(Self::resolve_column(g));
             }
        }
        let group_sql = if group_clauses.is_empty() { "".to_string() } else { format!("GROUP BY {}", group_clauses.join(", ")) };

        // 4. Build ORDER BY
        let order_sql = if let Some(sort_str) = &query.sort {
            let parts: Vec<String> = sort_str.split(',').map(|s| {
                let s = s.trim();
                let desc = s.starts_with('-');
                let clean = s.trim_start_matches('-');
                let col = Self::resolve_column(clean);
                format!("{} {}", col, if desc { "DESC" } else { "ASC" })
            }).collect();
            format!("ORDER BY {}", parts.join(", "))
        } else {
            "ORDER BY records.id DESC".to_string()
        };

        // 5. Limit/Offset
        let limit = query.limit.unwrap_or(100).min(10000);
        let offset = query.offset.unwrap_or(0);

        let final_sql = format!(
            "SELECT {} FROM records {} {} {} LIMIT {} OFFSET {}",
            select_clauses.join(", "),
            where_sql,
            group_sql,
            order_sql,
            limit,
            offset
        );

        Ok((final_sql, params, query.pipeline.clone()))
    }

    // Handles: id -> records.id,  price -> records.data->>'price'
    fn resolve_column(field: &str) -> String {
        if field == "id" { return "records.id".to_string(); }
        if field == "created" { return "records.created".to_string(); }
        if field == "updated" { return "records.updated".to_string(); }
        
        let parts: Vec<&str> = field.split('.').collect();
        let mut sql = "records.data".to_string();
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                sql = format!("{} ->> '{}'", sql, part); // Extract value
            } else {
                sql = format!("{} -> '{}'", sql, part); // Extract object
            }
        }
        sql
    }
}

pub struct QueryProcessor;

impl QueryProcessor {
    pub fn process(data: Vec<Value>, pipeline: Vec<PipelineStep>) -> Result<Vec<Value>, String> {
        let mut current_data = data;

        for step in pipeline {
            match step {
                PipelineStep::CumulativeSum { field, output_field } => {
                    let mut running_total = 0.0;
                    for row in &mut current_data {
                        if let Some(obj) = row.as_object_mut() {
                            let val = obj.get(&field).and_then(|v| v.as_f64()).unwrap_or(0.0);
                            running_total += val;
                            obj.insert(output_field.clone(), json!(running_total));
                        }
                    }
                },
                PipelineStep::Map { expression: _ } => {
                    // Simple evaluation (e.g. "a + b")
                    // In a real implementation, this would use the `ScriptEngine` (Boa).
                    // For now, let's implement basic field renaming as a placeholder or basic math if possible.
                    // Or, since we have the ScriptEngine available in AppState but not here...
                    // We will skip complex EVAL for this snippet and handle basic transforms if needed.
                    // TODO: Connect to ScriptEngine for `eval`.
                },
                PipelineStep::Pivot { key, value, agg } => {
                    // Simple Pivot: Group whole dataset by `key`
                    // WARNING: This drastically changes the shape of the result set
                    // It returns 1 row usually if pivoting everything, or N rows.
                    // Implementing full pivot logic in Rust is complex.
                    // Simplified: Group all rows into one object: { "CategoryA": 100, "CategoryB": 200 }
                    
                    let mut pivot_map = serde_json::Map::new();
                    
                    for row in &current_data {
                         let k_val = row.get(&key).and_then(|v| v.as_str()).unwrap_or("unknown");
                         let v_val = row.get(&value).and_then(|v| v.as_f64()).unwrap_or(0.0);
                         
                         let current = pivot_map.get(k_val).and_then(|v| v.as_f64()).unwrap_or(0.0);
                         
                         if agg == "sum" {
                             pivot_map.insert(k_val.to_string(), json!(current + v_val));
                         } else if agg == "count" {
                             pivot_map.insert(k_val.to_string(), json!(current + 1.0));
                         }
                    }
                    
                    // Return as a single row array
                    current_data = vec![Value::Object(pivot_map)];
                }
            }
        }
        
        Ok(current_data)
    }
}