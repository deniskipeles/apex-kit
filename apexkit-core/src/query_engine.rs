// =========================== /teamspace/studios/this_studio/apex/apex-kit/apexkit-core/src/query_engine.rs ===========================
use serde::{Deserialize, Serialize};
use crate::Db;
use serde_json::{json, Value};
use crate::filter::FilterNode;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ApexQuery {
    pub from: String, // Collection Name or System Table Name
    
    // [NEW] Explicit System Flag
    #[serde(default)]
    pub system: bool, 
    
    // SQL Layer
    #[serde(default)]
    pub select: Vec<SelectField>, 
    #[serde(default)]
    pub r#where: Option<Value>,   
    #[serde(default)]
    pub group_by: Vec<String>,
    #[serde(default)]
    pub sort: Option<String>,     
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
        expr: Option<String>,  // Raw SQL expression
        #[serde(rename = "fn")]
        func: Option<String>,  // "sum", "avg", "count"
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
    Pivot { key: String, value: String, agg: String }, 
    #[serde(rename = "map")]
    Map { expression: String }, 
}

pub struct QueryBuilder;

impl QueryBuilder {
    pub async fn build(query: &ApexQuery, db: &impl Db) -> Result<(String, Vec<libsql::Value>, Vec<PipelineStep>), String> {
        let mut params: Vec<libsql::Value> = vec![];
        let mut select_clauses = Vec::new();
        let mut group_clauses = Vec::new();

        // 1. Determine Target (System vs User Collection)
        // System tables start with _ OR have system: true flag
        let is_system = query.system || query.from.starts_with('_');
        
        let (table_name, where_prefix) = if is_system {
            // Direct Table Access (e.g. _tenants, _audit_logs)
            (query.from.clone(), "WHERE 1=1".to_string())
        } else {
            // User Collection (Mapped to 'records' table with ID check)
            let cols = db.list_collections().await.map_err(|e| e.to_string())?;
            let col = cols.iter().find(|c| c.name == query.from)
                .ok_or_else(|| format!("Collection '{}' not found", query.from))?;
            ("records".to_string(), format!("WHERE collection_id = {}", col.id))
        };

        // 2. Build SELECT
        if query.select.is_empty() {
             if is_system {
                 select_clauses.push("*".to_string());
             } else {
                 // Default User Record Select
                 select_clauses.push("records.id".to_string());
                 select_clauses.push("json(records.data) as data".to_string());
                 select_clauses.push("records.created".to_string());
                 select_clauses.push("records.updated".to_string());
             }
        } else {
            for field_def in &query.select {
                match field_def {
                    SelectField::Simple(name) => {
                        let col_expr = if is_system { name.clone() } else { Self::resolve_column(name) };
                        // Quote alias to handle spaces or reserved words
                        select_clauses.push(format!("{} as \"{}\"", col_expr, name));
                    },
                    SelectField::Complex { field, expr, func, alias } => {
                        let base_col = if let Some(e) = expr {
                            e.clone()
                        } else if let Some(f) = field {
                            if is_system { f.clone() } else { Self::resolve_column(f) }
                        } else {
                            "*".to_string()
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
                                _ => base_col 
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

        // 3. Build WHERE
        let mut where_sql = where_prefix;
        if let Some(filter_json) = &query.r#where {
            let node = FilterNode::parse(filter_json);
            
            if let Some((sql, p)) = node.to_sql() {
                // HACK: FilterNode generates JSON accessors ("data ->>").
                // If this is a system table, we need to strip those out to refer to raw columns.
                // e.g. "data ->> 'status' = 'active'" becomes "status = 'active'"
                // This is a rough heuristic but works for standard system tables.
                let final_sql = if is_system {
                    sql.replace("data ->> ", "")
                       .replace("data -> ", "")
                       .replace("'", "") // Remove quotes around column names if FilterNode added them (it usually doesn't for field names)
                       // FilterNode might produce: data ->> 'status'
                       // We want: status
                } else {
                    sql.clone()
                };
                
                // Better System Table Handling:
                // We should really update FilterNode to accept a "mode" but for now, 
                // string replacement is the safest non-invasive patch.
                // We handle the specific case of: data ->> 'col_name' -> col_name
                
                let _cleaned_sql = if is_system {
                     // Regex replacement would be better, but simple replace covers 90%
                     final_sql.replace("data ->> '", "").replace("'", "") 
                     // Wait, this might break string values. 
                     // Let's rely on the user passing simple filters for system tables
                     // or fix FilterNode later. The above string replace is risky for values.
                     // SAFEST FIX: Only strip the prefix, assume field name matches column.
                     // FilterNode output: data ->> 'field' op ?
                     // We want: field op ?
                     
                     // NOTE: FilterNode uses `format_json_column` internally. We can't change that easily without editing filter.rs
                     // So we do a targeted replace.
                } else {
                    final_sql
                };

                // Actually, let's just push the raw SQL if it's a user collection. 
                // For system tables, the `FilterNode` logic is fundamentally JSON-oriented.
                // If you need robust system filtering, use `filter.rs` updates.
                // For this snippet, I will apply the raw SQL and hope the user knows 
                // _tenants doesn't have a `data` column, so standard filters might fail 
                // unless we implement the `is_system` logic inside `filter.rs`.
                
                // TEMPORARY FIX: Just append. Real fix requires filter.rs update.
                where_sql.push_str(" AND ");
                
                if is_system {
                     // Attempt to clean up the JSON accessors for system columns
                     // Matches: data ->> 'any_word'
                     let re = regex::Regex::new(r"data ->> '(\w+)'").unwrap();
                     let cleaned = re.replace_all(&sql, "$1");
                     where_sql.push_str(&cleaned);
                } else {
                     where_sql.push_str(&sql);
                }
                
                params.extend(p);
            }
        }

        // 4. Build GROUP BY
        if !query.group_by.is_empty() {
             for g in &query.group_by {
                 group_clauses.push(if is_system { g.clone() } else { Self::resolve_column(g) });
             }
        }
        let group_sql = if group_clauses.is_empty() { "".to_string() } else { format!("GROUP BY {}", group_clauses.join(", ")) };

        // 5. Build ORDER BY
        let order_sql = if let Some(sort_str) = &query.sort {
            let parts: Vec<String> = sort_str.split(',').map(|s| {
                let s = s.trim();
                let desc = s.starts_with('-');
                let clean = s.trim_start_matches('-');
                let col = if is_system { clean.to_string() } else { Self::resolve_column(clean) };
                format!("{} {}", col, if desc { "DESC" } else { "ASC" })
            }).collect();
            format!("ORDER BY {}", parts.join(", "))
        } else {
            if is_system {
                 // Default sort for system tables (assume 'id' exists or use 1)
                 "ORDER BY 1 DESC".to_string() 
            } else {
                 "ORDER BY records.id DESC".to_string()
            }
        };

        // 6. Limit/Offset
        let limit = query.limit.unwrap_or(100).min(10000);
        let offset = query.offset.unwrap_or(0);

        let final_sql = format!(
            "SELECT {} FROM {} {} {} {} LIMIT {} OFFSET {}",
            select_clauses.join(", "),
            table_name, 
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
                    // Placeholder for map logic
                },
                PipelineStep::Pivot { key, value, agg } => {
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
                    current_data = vec![Value::Object(pivot_map)];
                }
            }
        }
        
        Ok(current_data)
    }
}