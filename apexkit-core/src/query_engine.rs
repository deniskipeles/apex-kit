// apexkit-core/src/query_engine.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use crate::{Db, query::QueryOptions, Record};
use serde_json::{json, Value};

#[derive(Deserialize, Serialize, Debug)]
pub struct ApexQuery {
    pub from: String,
    #[serde(default)]
    pub select: Vec<String>,
    #[serde(default)]
    pub r#where: Option<Value>, // FilterNode JSON
    #[serde(default)]
    pub sort: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    #[serde(default)]
    pub populate: Vec<String>, // ["author", "comments.author"]
    
    // Aggregations (Optional)
    pub aggregate: Option<HashMap<String, Aggregation>>,
    pub group_by: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
pub enum Aggregation {
    #[serde(rename = "$count")]
    Count(String),
    #[serde(rename = "$sum")]
    Sum(String),
    #[serde(rename = "$avg")]
    Avg(String),
    #[serde(rename = "$min")]
    Min(String),
    #[serde(rename = "$max")]
    Max(String),
}

pub struct QueryEngine;

impl QueryEngine {
    pub async fn execute(
        db: Arc<dyn Db>, 
        query: ApexQuery
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        
        // 1. Resolve Collection ID
        let cols = db.list_collections().await?;
        let col = cols.iter().find(|c| c.name == query.from)
            .ok_or_else(|| format!("Collection '{}' not found", query.from))?;

        // 2. Handle Aggregation (if present)
        if let Some(aggs) = query.aggregate {
            // Aggregation queries run different SQL
            // Note: This requires extending Db trait to support raw-ish queries or building specialized agg method.
            // For now, let's implement in-memory aggregation for MVP to avoid huge DB refactor.
            // Fetch ALL matching records (up to limit) and compute.
            
            // WARNING: Performance impact. Future optimization: push to SQL.
            let opts = QueryOptions {
                filter: query.r#where.map(|v| v.to_string()),
                limit: Some(10000), // Safety cap
                ..Default::default()
            };
            let res = db.list_records(col.id, opts).await?;
            return Self::compute_aggregates(res.items, aggs, query.group_by);
        }

        // 3. Standard List
        let opts = QueryOptions {
            filter: query.r#where.map(|v| v.to_string()),
            sort: query.sort,
            limit: query.limit,
            offset: query.offset,
            expand: Some(query.populate.join(",")),
            ..Default::default()
        };

        let res = db.list_records(col.id, opts).await?;
        
        // 4. Projection (Select specific fields)
        let mut final_items = Vec::new();
        for rec in res.items {
            let mut item_obj = json!(rec.data);
            
            // Inject System Fields
            if let Some(o) = item_obj.as_object_mut() {
                o.insert("id".to_string(), json!(rec.id));
                o.insert("created".to_string(), json!(rec.created));
                o.insert("updated".to_string(), json!(rec.updated));
                if let Some(exp) = rec.expand {
                    o.insert("expand".to_string(), exp);
                }
            }

            if !query.select.is_empty() {
                let mut projected = serde_json::Map::new();
                for field in &query.select {
                    if let Some(val) = item_obj.get(field) {
                        projected.insert(field.clone(), val.clone());
                    }
                }
                final_items.push(Value::Object(projected));
            } else {
                final_items.push(item_obj);
            }
        }

        Ok(json!(final_items))
    }

    fn compute_aggregates(
        records: Vec<Record>, 
        aggs: HashMap<String, Aggregation>,
        group_by: Option<String>
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        
        // 1. Group Data
        let mut groups: HashMap<String, Vec<Record>> = HashMap::new();
        
        // [FIX] Borrow group_by to avoid move
        if let Some(ref field) = group_by {
            for rec in records {
                let key = rec.data.get(field)
                    .map(|v| v.to_string().trim_matches('"').to_string())
                    .unwrap_or("null".to_string());
                groups.entry(key).or_default().push(rec);
            }
        } else {
            // Single group
            groups.insert("all".to_string(), records);
        }

        let mut results = Vec::new();

        for (group_key, group_records) in groups {
            let mut result_row = serde_json::Map::new();
            if group_key != "all" {
                result_row.insert("group".to_string(), json!(group_key));
            }

            for (alias, op) in &aggs {
                let val = match op {
                    Aggregation::Count(_) => json!(group_records.len()),
                    Aggregation::Sum(f) => {
                        let sum: f64 = group_records.iter()
                            .filter_map(|r| r.data.get(f).and_then(|v| v.as_f64()))
                            .sum();
                        json!(sum)
                    },
                    Aggregation::Avg(f) => {
                        let count = group_records.len() as f64;
                        let sum: f64 = group_records.iter()
                            .filter_map(|r| r.data.get(f).and_then(|v| v.as_f64()))
                            .sum();
                        json!(if count > 0.0 { sum / count } else { 0.0 })
                    },
                    Aggregation::Min(f) => {
                         let min = group_records.iter()
                            .filter_map(|r| r.data.get(f).and_then(|v| v.as_f64()))
                            .fold(f64::INFINITY, f64::min);
                         json!(if min == f64::INFINITY { 0.0 } else { min })
                    },
                    Aggregation::Max(f) => {
                         let max = group_records.iter()
                            .filter_map(|r| r.data.get(f).and_then(|v| v.as_f64()))
                            .fold(f64::NEG_INFINITY, f64::max);
                         json!(if max == f64::NEG_INFINITY { 0.0 } else { max })
                    }
                };
                result_row.insert(alias.clone(), val);
            }
            results.push(Value::Object(result_row));
        }

        if results.len() == 1 && group_by.is_none() {
            Ok(Value::Object(results.pop().unwrap().as_object().unwrap().clone()))
        } else {
            Ok(json!(results))
        }
    }
}