use serde::{Deserialize};
use std::collections::HashMap;
use crate::schema::{CollectionSchema, FieldType};
use crate::filter::FilterNode;

#[derive(Debug, Deserialize, Clone)]
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
    pub count_sql: String, // New field for the COUNT(*) query
    pub params: Vec<libsql::Value>,
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
        
        // Init params with collection_id (param #1)
        params.push(collection_id.into());

        // Base WHERE clause
        let mut where_clause = "WHERE collection_id = ?".to_string();

        // 2. Filter Logic
        if let Some(filter_str) = options.filter {
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&filter_str) {
                let filter_node = FilterNode::parse(&json_val);
                if let Some((filter_sql, filter_params)) = filter_node.to_sql() {
                    where_clause.push_str(&format!(" AND ({})", filter_sql));
                    params.extend(filter_params);
                }
            }
        }

        // Generate Count SQL (before adding sort/limit)
        let count_sql = format!("SELECT COUNT(*) FROM records {}", where_clause);

        // 1. Construct the SELECT clause for Data
        let select_clause = if let Some(expand_str) = &options.expand {
            if !expand_str.trim().is_empty() {
                let paths = smart_split(expand_str);
                let expanded_json_sql = build_recursive_select(
                    paths, 
                    "records", 
                    0, 
                    collection_name, 
                    collection_id, 
                    schemas, 
                    name_to_id
                );
                format!("records.id, {} as data", expanded_json_sql)
            } else {
                "records.id, json(records.data) as data".to_string()
            }
        } else {
            "records.id, json(records.data) as data".to_string()
        };

        // Combine into Main Query
        let mut sql = format!("SELECT {} FROM records {}", select_clause, where_clause);

        // 3. Sorting
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

        // 4. Pagination
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

pub fn build_recursive_select(
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
        return format!("json_object('id', {}.id, 'data', json({}.data))", parent_alias, parent_alias);
    }

    let mut expand_objects = Vec::new();
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

        if let Some(field_def) = schema.fields.get(&relation_name) {
            if field_def.r#type == FieldType::Owner {
                let subquery = format!(
                    "(SELECT json_object('id', u.id, 'email', u.email, 'role', u.role) \
                      FROM users u \
                      WHERE u.id = CAST(json_extract({}.data, '$.{}') AS INTEGER))",
                    parent_alias, relation_name
                );
                expand_objects.push(format!("'{}', json({})", relation_name, subquery));
                continue;
            }
        }

        if let Some(rel_def) = schema.relations.get(&relation_name) {
            let target_col_name = rel_def.target_collection.clone();
            
            let target_exists = schemas.contains_key(&target_col_name) || 
                                name_to_id.contains_key(&target_col_name) ||
                                schemas.keys().any(|k| k == &target_col_name);

            if !target_exists {
                let err_obj = format!("json_object('error', 'Target collection \"{}\" schema not found')", target_col_name);
                expand_objects.push(format!("'{}', json({})", relation_name, err_obj));
                continue;
            }

            let target_col_id = *name_to_id.get(&target_col_name).unwrap_or(&0);

            let inner_select = build_recursive_select(sub_paths, &target_alias, depth + 1, &target_col_name, target_col_id, schemas, name_to_id);

            let subquery = format!(
                "(SELECT json_group_array(json(sub_data)) FROM ( \
                    SELECT {} as sub_data \
                    FROM _relations {} \
                    JOIN records {} ON {}.target_rec_id = {}.id \
                    WHERE {}.origin_rec_id = {}.id \
                    AND {}.rel_name = '{}' \
                    {} {} {} \
                ))",
                inner_select,
                rel_alias, target_alias,
                rel_alias, target_alias,
                rel_alias, parent_alias,
                rel_alias, relation_name,
                order_clause, limit_clause, offset_clause
            );

            expand_objects.push(format!("'{}', json({})", relation_name, subquery));
            continue;
        }

        if let Some(reverse_schema) = schemas.get(&relation_name) {
            let mut reverse_field_name = None;
            for (r_name, r_def) in &reverse_schema.relations {
                if r_def.target_collection == current_col_name || 
                   r_def.target_collection == current_col_id.to_string() {
                    reverse_field_name = Some(r_name.clone());
                    break; 
                }
            }

            if let Some(r_field) = reverse_field_name {
                let target_col_name = relation_name.clone(); 
                let target_col_id = *name_to_id.get(&target_col_name).unwrap_or(&0);
                
                let inner_select = build_recursive_select(sub_paths, &target_alias, depth + 1, &target_col_name, target_col_id, schemas, name_to_id);

                let subquery = format!(
                    "(SELECT json_group_array(json(sub_data)) FROM ( \
                        SELECT {} as sub_data \
                        FROM _relations {} \
                        JOIN records {} ON {}.origin_rec_id = {}.id \
                        WHERE {}.target_rec_id = {}.id \
                        AND {}.target_col_id = {} \
                        AND {}.origin_col_id = {} \
                        AND {}.rel_name = '{}' \
                        {} {} {} \
                    ))",
                    inner_select,
                    rel_alias,                          
                    target_alias,                       
                    rel_alias, target_alias,            
                    rel_alias, parent_alias,            
                    rel_alias, current_col_id,          
                    rel_alias, target_col_id,           
                    rel_alias, r_field,                 
                    order_clause, limit_clause, offset_clause
                );

                expand_objects.push(format!("'{}', json({})", relation_name, subquery));
                continue;
            }
        }

        let error_obj = format!(
            "json_object('error', 'Relation \"{}\" not defined in schema for \"{}\" or valid reverse lookup found')", 
            relation_name, current_col_name
        );
        expand_objects.push(format!("'{}', json({})", relation_name, error_obj));
    }

    format!(
        "json_patch(json({}.data), json_object('expand', json_object({})))",
        parent_alias,
        expand_objects.join(", ")
    )
}