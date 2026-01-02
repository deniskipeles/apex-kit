use serde::{Deserialize, Serialize}; // Added Serialize
use std::collections::HashMap;
use crate::schema::{CollectionSchema, FieldType, RelationType};
use crate::filter::FilterNode;

#[derive(Debug, Deserialize, Serialize, Clone)] // Added Serialize
pub struct QueryOptions {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub sort: Option<String>, 
    pub filter: Option<String>,
    pub expand: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            page: Some(1),
            per_page: Some(30),
            sort: None,
            filter: None,
            expand: None,
            limit: None,
            offset: None,
        }
    }
}

#[derive(Debug)]
pub struct SqlBuilder {
    pub base_sql: String,
    pub count_sql: String,
    pub params: Vec<libsql::Value>,
}

// Helper for parsing "a.b, c" -> Tree
pub fn build_expand_tree(input: &str) -> HashMap<String, Vec<String>> {
    let parts = smart_split(input);
    build_expand_tree_from_list(&parts)
}

// Helper for parsing Vec<String> -> Tree
pub fn build_expand_tree_from_list(paths: &[String]) -> HashMap<String, Vec<String>> {
    let mut tree: HashMap<String, Vec<String>> = HashMap::new();
    for path in paths {
        if let Some((root, rest)) = path.split_once('.') {
            tree.entry(root.to_string()).or_default().push(rest.to_string());
        } else {
            tree.entry(path.clone()).or_default();
        }
    }
    tree
}

impl SqlBuilder {
    pub fn new(
        collection_id: i64, 
        collection_name: &str,
        options: QueryOptions,
        schemas: &HashMap<String, CollectionSchema>,
        name_to_id: &HashMap<String, i64>
    ) -> Self {
        let mut params: Vec<libsql::Value> = vec![];
        params.push(collection_id.into());

        let mut where_clause = "WHERE collection_id = ?".to_string();

        if let Some(filter_str) = options.filter {
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&filter_str) {
                let filter_node = FilterNode::parse(&json_val);
                if let Some((filter_sql, filter_params)) = filter_node.to_sql() {
                    where_clause.push_str(&format!(" AND ({})", filter_sql));
                    params.extend(filter_params);
                }
            }
        }

        let count_sql = format!("SELECT COUNT(*) FROM records {}", where_clause);

        // --- SELECT CLAUSE ---
        // Column 1: ID
        // Column 2: Data (as JSON string)
        // Column 3: Expand (as JSON string or NULL)
        
        let expand_col_sql = if let Some(expand_str) = &options.expand {
            if !expand_str.trim().is_empty() {
                let paths = smart_split(expand_str);
                build_expand_json_object(
                    paths, 
                    "records", 
                    0, 
                    collection_name, 
                    collection_id, 
                    schemas, 
                    name_to_id
                )
            } else {
                "NULL".to_string()
            }
        } else {
            "NULL".to_string()
        };

        // Note: json(records.data) ensures we get text instead of blob from JSONB columns
        let mut sql = format!("SELECT records.id, json(records.data), {} FROM records {}", expand_col_sql, where_clause);

        if let Some(sort) = options.sort {
            let desc = sort.starts_with('-');
            let field = if desc { &sort[1..] } else { &sort };
            
            if field == "id" || field == "created" {
                sql.push_str(&format!(" ORDER BY records.{} {}", field, if desc { "DESC" } else { "ASC" }));
            } else {
                sql.push_str(&format!(" ORDER BY records.data ->> '{}' {}", field, if desc { "DESC" } else { "ASC" }));
            }
        } else {
            sql.push_str(" ORDER BY records.id DESC");
        }

        let (limit, offset) = if options.limit.is_some() || options.offset.is_some() {
            let l = options.limit.unwrap_or(30); 
            let o = options.offset.unwrap_or(0);
            (l, o)
        } else {
            let page = options.page.unwrap_or(1).max(1);
            let limit = options.per_page.unwrap_or(30).min(100); 
            let offset = (page - 1) * limit;
            (limit, offset)
        };

        sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

        Self { base_sql: sql, count_sql, params }
    }
}

pub fn smart_split(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    
    for c in input.chars() {
        match c {
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    parts.push(current.trim().to_string());
                }
                current.clear();
            }
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                if depth > 0 { depth -= 1; }
                current.push(c);
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

struct ExpandPart {
    name: String,
    limit: Option<u64>,
    offset: Option<u64>,
}

fn parse_expand_part(input: &str) -> ExpandPart {
    let input = input.trim();
    if let Some(start) = input.find('(') {
        if let Some(end) = input.rfind(')') {
            let name = input[..start].trim().to_string();
            let params = &input[start+1..end];
            let parts: Vec<&str> = params.split(',').collect();
            
            let limit = parts.get(0).and_then(|s| s.trim().parse().ok());
            let offset = parts.get(1).and_then(|s| s.trim().parse().ok());
            
            return ExpandPart { name, limit, offset };
        }
    }
    ExpandPart { name: input.to_string(), limit: None, offset: None }
}

/// Recursively builds the SQL for the `expand` column.
/// Returns a string representing a SQL expression (e.g., `json_object(...)` or `NULL`).
pub fn build_expand_json_object(
    paths: Vec<String>, 
    parent_alias: &str, 
    depth: usize,
    current_col_name: &str,
    current_col_id: i64,
    schemas: &HashMap<String, CollectionSchema>,
    name_to_id: &HashMap<String, i64>
) -> String {
    let mut tree: HashMap<String, Vec<String>> = HashMap::new();
    for path in paths {
        if let Some((root, rest)) = path.split_once('.') {
            tree.entry(root.to_string()).or_default().push(rest.to_string());
        } else {
            tree.entry(path).or_default();
        }
    }

    if tree.is_empty() {
        return "NULL".to_string();
    }

    let mut expand_fields = Vec::new();
    let rel_alias = format!("r{}", depth);
    let target_alias = format!("t{}", depth);

    let default_schema = CollectionSchema::default();
    let schema = schemas.get(&current_col_id.to_string())
        .or_else(|| schemas.get(current_col_name))
        .unwrap_or(&default_schema);

    for (raw_key, sub_paths) in tree {
        let part = parse_expand_part(&raw_key);
        let relation_name = part.name;

        let limit_clause = if let Some(l) = part.limit { format!(" LIMIT {}", l) } else { "".to_string() };
        let offset_clause = if let Some(o) = part.offset { format!(" OFFSET {}", o) } else { "".to_string() };
        let order_clause = if !limit_clause.is_empty() || !offset_clause.is_empty() { 
            format!(" ORDER BY {}.id DESC", rel_alias) 
        } else { "".to_string() };

        // SKIP Owner fields (Handled in Rust application layer)
        if let Some(field_def) = schema.fields.get(&relation_name) {
            if field_def.r#type == FieldType::Owner {
                continue;
            }
        }

        // Determine if Forward or Reverse Relation
        let mut target_col_id = 0;
        let mut target_col_name = String::new();
        let mut reverse_lookup = false;
        let mut forward_relation_type = RelationType::Many; 
        let mut actual_rel_name_in_db = relation_name.clone(); // For reverse lookups, this changes

        // 1. Check Forward Relation (Defined in current schema)
        if let Some(rel_def) = schema.relations.get(&relation_name) {
            target_col_name = rel_def.target_collection.clone();
            target_col_id = *name_to_id.get(&target_col_name).unwrap_or(&0);
            forward_relation_type = rel_def.relation_type.clone();
        } 
        // 2. Check Reverse Relation (Defined in target schema pointing to us)
        else if let Some(reverse_schema) = schemas.get(&relation_name) {
            // Find relation in target that points to us
            // Logic: Target collection is the one named 'relation_name' (e.g., 'comments')
            target_col_name = relation_name.clone();
            target_col_id = *name_to_id.get(&target_col_name).unwrap_or(&0);

            for (r_name, r_def) in &reverse_schema.relations {
                if r_def.target_collection == current_col_name || r_def.target_collection == current_col_id.to_string() {
                    reverse_lookup = true;
                    actual_rel_name_in_db = r_name.clone(); // e.g. 'post_id'
                    break;
                }
            }
        }

        if target_col_id == 0 {
             // Schema mismatch or not found, return error object in JSON structure
             let err_obj = format!("json_object('error', 'Relation \"{}\" not found or schema missing')", relation_name);
             expand_fields.push(format!("'{}', {}", relation_name, err_obj));
             continue;
        }

        // Build Nested Expansion Object (Recursion)
        // This generates the 'expand' JSON object for the Child record
        let nested_expand_sql = build_expand_json_object(
            sub_paths, 
            &target_alias, 
            depth + 1, 
            &target_col_name, 
            target_col_id, 
            schemas, 
            name_to_id
        );

        // Build Full Target Record JSON Structure
        let target_record_json = format!(
            "json_object('id', {}.id, 'data', json({}.data), 'expand', {})",
            target_alias, target_alias, nested_expand_sql
        );

        // Construct Subquery based on cardinality and direction
        let subquery = if !reverse_lookup && forward_relation_type == RelationType::One {
             // Case A: Forward One-to-One -> Return Single Object
             format!(
                "(SELECT {} FROM _relations {} \
                  JOIN records {} ON {}.target_rec_id = {}.id \
                  WHERE {}.origin_rec_id = {}.id \
                  AND {}.rel_name = '{}' \
                  LIMIT 1)",
                target_record_json,
                rel_alias, target_alias,
                rel_alias, target_alias,
                rel_alias, parent_alias,
                rel_alias, relation_name
            )
        } else {
             // Case B: One-to-Many (Forward List or Reverse List) -> Return Array
             let join_condition = if reverse_lookup {
                 // Reverse: We are the 'Target' of the link in _relations
                 // The 'Origin' is the child record we want to fetch
                 // Link: Comment (Origin) -> Post (Target)
                 format!(
                    "WHERE {}.target_rec_id = {}.id \
                     AND {}.origin_col_id = {} \
                     AND {}.target_col_id = {} \
                     AND {}.rel_name = '{}'",
                    rel_alias, parent_alias,
                    rel_alias, target_col_id,   // Origin Col = Child (Comment)
                    rel_alias, current_col_id,  // Target Col = Parent (Post)
                    rel_alias, actual_rel_name_in_db // Relation Name (post_id)
                 )
             } else {
                 // Forward Many: We are 'Origin'
                 format!(
                    "WHERE {}.origin_rec_id = {}.id \
                     AND {}.rel_name = '{}'",
                    rel_alias, parent_alias,
                    rel_alias, relation_name
                 )
             };

             // Join logic
             let join_target = if reverse_lookup {
                 // We want the Origin Record (the child)
                 format!("JOIN records {} ON {}.origin_rec_id = {}.id", target_alias, rel_alias, target_alias)
             } else {
                 // We want the Target Record
                 format!("JOIN records {} ON {}.target_rec_id = {}.id", target_alias, rel_alias, target_alias)
             };

             format!(
                "(SELECT json_group_array(json(sub)) FROM ( \
                    SELECT {} as sub \
                    FROM _relations {} \
                    {} \
                    {} \
                    {} {} {} \
                ))",
                target_record_json,
                rel_alias,
                join_target,
                join_condition,
                order_clause, limit_clause, offset_clause
            )
        };

        expand_fields.push(format!("'{}', {}", relation_name, subquery));
    }

    if expand_fields.is_empty() {
        return "NULL".to_string();
    }

    format!("json_object({})", expand_fields.join(", "))
}