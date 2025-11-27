// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/query.rs start here ===========================
use serde::{Deserialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct QueryOptions {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub sort: Option<String>, // Format: "-created" or "created"
    pub filter: Option<String>, // Format: JSON map string
    pub expand: Option<String>, // Format: "author, comments.user"
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            page: Some(1),
            per_page: Some(30),
            sort: None,
            filter: None,
            expand: None,
        }
    }
}

#[derive(Debug)]
pub struct SqlBuilder {
    pub base_sql: String,
    pub params: Vec<libsql::Value>,
}

impl SqlBuilder {
    pub fn new(collection_id: i64, options: QueryOptions) -> Self {
        let mut params: Vec<libsql::Value> = vec![];

        // 1. Construct the SELECT clause
        // If 'expand' is present, we generate a complex recursive JSON query.
        // Otherwise, we just fetch the raw data.
        let select_clause = if let Some(expand_str) = &options.expand {
            let paths: Vec<&str> = expand_str.split(',').map(|s| s.trim()).collect();
            // Start recursion from the "records" table
            let expanded_json_sql = build_recursive_select(paths, "records", 0);
            format!("records.id, {} as data", expanded_json_sql)
        } else {
            "records.id, records.data".to_string()
        };

        let mut sql = format!("SELECT {} FROM records WHERE collection_id = ?", select_clause);
        params.push(collection_id.into());

        // 2. Filter Logic
        // Uses SQLite's ->> operator for efficient JSON extraction
        if let Some(filter_str) = options.filter {
            if let Ok(filters) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&filter_str) {
                for (key, val) in filters {
                    sql.push_str(&format!(" AND records.data ->> '{}' = ?", key));
                    
                    // Convert serde Value to LibSQL Value
                    let sql_val = match val {
                        serde_json::Value::String(s) => s.into(),
                        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0).into(),
                        serde_json::Value::Bool(b) => (if b { 1 } else { 0 }).into(),
                        _ => "".into(),
                    };
                    params.push(sql_val);
                }
            }
        }

        // 3. Sorting
        if let Some(sort) = options.sort {
            let desc = sort.starts_with('-');
            let field = if desc { &sort[1..] } else { &sort };
            
            // Handle System Fields vs JSON Fields
            if field == "id" || field == "created" {
                sql.push_str(&format!(" ORDER BY records.{} {}", field, if desc { "DESC" } else { "ASC" }));
            } else {
                // Sort by JSON field
                sql.push_str(&format!(" ORDER BY records.data ->> '{}' {}", field, if desc { "DESC" } else { "ASC" }));
            }
        } else {
            sql.push_str(" ORDER BY records.id DESC");
        }

        // 4. Pagination
        let page = options.page.unwrap_or(1).max(1);
        let limit = options.per_page.unwrap_or(30).min(100); // Max 100 per page
        let offset = (page - 1) * limit;

        sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

        Self { base_sql: sql, params }
    }
}

/// Recursively builds a SQL string to fetch and nest related records.
/// 
/// Uses SQLite's `json_group_array` to aggregate children and `json_patch` to merge
/// them into the parent object under the `expand` key.
///
/// # Arguments
/// * `paths` - A list of dot-notation strings (e.g., ["author.company", "comments"])
/// * `parent_alias` - The SQL table alias of the parent record (e.g., "records" or "t0")
/// * `depth` - Recursion depth, used to generate unique table aliases (r0, t0, etc.)
fn build_recursive_select(paths: Vec<&str>, parent_alias: &str, depth: usize) -> String {
    // 1. Parse paths into a Tree structure
    // e.g., ["author.company", "author.profile"] -> { "author": ["company", "profile"] }
    let mut tree: HashMap<String, Vec<String>> = HashMap::new();
    
    for path in paths {
        if let Some((root, rest)) = path.split_once('.') {
            tree.entry(root.to_string()).or_default().push(rest.to_string());
        } else {
            tree.entry(path.to_string()).or_default();
        }
    }

    // Base Case: No more expansions requested
    if tree.is_empty() {
        return format!("{}.data", parent_alias);
    }

    let mut expand_objects = Vec::new();

    // Unique aliases for this level of recursion to prevent SQL naming collisions
    let rel_alias = format!("r{}", depth);
    let target_alias = format!("t{}", depth);

    for (relation, sub_paths) in tree {
        let sub_path_strs: Vec<&str> = sub_paths.iter().map(|s| s.as_str()).collect();
        
        // Recurse: Determine what to select from the target table
        let inner_select = build_recursive_select(sub_path_strs, &target_alias, depth + 1);

        // Construct the Correlated Subquery
        // - joins _relations (r) with records (t)
        // - matches origin_rec_id with the current parent's ID
        // - aggregates results into a JSON array
        let subquery = format!(
            "(SELECT json_group_array({}) 
              FROM _relations {} 
              JOIN records {} ON {}.target_rec_id = {}.id 
              WHERE {}.origin_rec_id = {}.id 
              AND {}.rel_name = '{}')",
            inner_select,
            rel_alias,      // e.g., FROM _relations r0
            target_alias,   // e.g., JOIN records t0
            rel_alias, target_alias, // ON r0.target = t0.id
            rel_alias, parent_alias, // WHERE r0.origin = parent.id
            rel_alias, relation
        );

        // Add to the list of keys for the 'expand' object
        expand_objects.push(format!("'{}', {}", relation, subquery));
    }

    // Wrap the current data with the expanded data using json_patch
    // Result: { ...original_data, "expand": { "author": [...], "comments": [...] } }
    format!(
        "json_patch({}.data, json_object('expand', json_object({})))",
        parent_alias,
        expand_objects.join(", ")
    )
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/query.rs ends here ===========================