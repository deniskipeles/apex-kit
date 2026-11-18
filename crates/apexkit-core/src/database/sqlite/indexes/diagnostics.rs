//! Pre-flight duplicate detection and user-facing error translation.
//!
//! SQLite rejects `CREATE UNIQUE INDEX` when duplicates already exist, but its
//! message names only the index. These helpers produce actionable diagnostics
//! (collection, fields, offending values, example record ids) and translate raw
//! constraint failures into stable application errors.

use super::meta::load_applied;
use super::spec::IndexSpec;
use rusqlite::Connection;

/// A single offending value group found while checking for duplicates.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateGroup {
    /// Rendered values, in field order (already stringified for display).
    pub values: Vec<String>,
    pub count: i64,
    pub first_id: i64,
}

/// Finds up to `limit` duplicate value groups for a unique spec.
///
/// Groups where any expression is `NULL` are excluded: SQLite permits multiple
/// `NULL`s in a unique index, so a missing optional field is not a duplicate.
pub fn find_duplicates(
    conn: &Connection,
    spec: &IndexSpec,
    limit: usize,
) -> rusqlite::Result<Vec<DuplicateGroup>> {
    let exprs = spec.expressions();
    if exprs.is_empty() {
        return Ok(vec![]);
    }

    let select_list = exprs.join(", ");
    let not_null: Vec<String> = exprs.iter().map(|e| format!("{} IS NOT NULL", e)).collect();

    let sql = format!(
        "SELECT {}, COUNT(*) AS record_count, MIN(id) AS first_id \
         FROM records \
         WHERE collection_id = ?1 AND {} \
         GROUP BY {} \
         HAVING COUNT(*) > 1 \
         LIMIT ?2",
        select_list,
        not_null.join(" AND "),
        select_list,
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params![spec.collection_id, limit as i64])?;

    let mut groups = Vec::new();
    while let Some(row) = rows.next()? {
        let mut values = Vec::with_capacity(exprs.len());
        for i in 0..exprs.len() {
            let v: Option<rusqlite::types::Value> = row.get(i)?;
            values.push(render_value(v));
        }
        let count: i64 = row.get(exprs.len())?;
        let first_id: i64 = row.get(exprs.len() + 1)?;
        groups.push(DuplicateGroup {
            values,
            count,
            first_id,
        });
    }
    Ok(groups)
}

/// Builds the actionable error message for a failed unique index creation.
pub fn duplicate_error_message(spec: &IndexSpec, groups: &[DuplicateGroup]) -> String {
    let fields = spec.fields.join(", ");
    let mut msg = format!(
        "Cannot create unique index {}: existing duplicate values were found in collection {} for field(s) [{}]:",
        spec.name, spec.collection_id, fields
    );
    for g in groups {
        msg.push_str(&format!(
            "\n  {} (count={}, first_id={})",
            g.values.join(", "),
            g.count,
            g.first_id
        ));
    }
    msg
}

fn render_value(v: Option<rusqlite::types::Value>) -> String {
    match v {
        Some(rusqlite::types::Value::Null) | None => "NULL".to_string(),
        Some(rusqlite::types::Value::Integer(i)) => i.to_string(),
        Some(rusqlite::types::Value::Real(f)) => f.to_string(),
        Some(rusqlite::types::Value::Text(s)) => s,
        Some(rusqlite::types::Value::Blob(_)) => "<blob>".to_string(),
    }
}

/// Translates a raw SQLite constraint error into a user-facing message.
///
/// `lookup_field` maps an index name to a human field name; it is consulted
/// before falling back to parsing, so field names need not be simple
/// identifiers.
pub fn translate_constraint_error(
    raw: &str,
    lookup_field: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    if !raw.contains("UNIQUE constraint failed") {
        return None;
    }

    let index_name = extract_index_name(raw);

    if let Some(name) = index_name {
        if name.starts_with("idx_c_") {
            return Some("Composite unique constraint violation.".to_string());
        }
        if name.starts_with("idx_u_") {
            if let Some(field) = lookup_field(&name) {
                return Some(format!(
                    "Unique constraint violation on '{}': the value already exists in this collection.",
                    field
                ));
            }
            return Some(
                "Unique constraint violation: the value already exists in this collection."
                    .to_string(),
            );
        }
    }

    Some("Unique constraint violation: This value or combination already exists.".to_string())
}

/// Extracts the index name from `UNIQUE constraint failed: index 'name'`.
fn extract_index_name(raw: &str) -> Option<String> {
    let marker = "index '";
    let start = raw.find(marker)? + marker.len();
    let rest = &raw[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

/// Resolves the field name for an index from `_applied_indexes`.
pub fn field_for_index(conn: &Connection, index_name: &str) -> Option<String> {
    let applied = load_applied(conn).ok()?;
    let spec = applied.get(index_name)?;
    if spec.fields.len() == 1 {
        Some(spec.fields[0].clone())
    } else {
        Some(spec.fields.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::sqlite::indexes::meta::{ensure_table, record_applied};
    use crate::database::sqlite::indexes::spec::IndexKind;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute(
            "CREATE TABLE records (id INTEGER PRIMARY KEY, collection_id INTEGER, data JSONB)",
            [],
        )
        .unwrap();
        c
    }

    fn spec() -> IndexSpec {
        IndexSpec {
            name: "idx_u_1_email".into(),
            collection_id: 1,
            kind: IndexKind::Unique,
            fields: vec!["email".into()],
        }
    }

    #[test]
    fn no_duplicates_returns_empty() {
        let c = conn();
        c.execute_batch(
            "INSERT INTO records (collection_id,data) VALUES (1, json('{\"email\":\"a@b.com\"}'));
             INSERT INTO records (collection_id,data) VALUES (1, json('{\"email\":\"c@d.com\"}'));",
        )
        .unwrap();
        assert!(find_duplicates(&c, &spec(), 5).unwrap().is_empty());
    }

    #[test]
    fn duplicates_are_reported_with_count_and_first_id() {
        let c = conn();
        c.execute_batch(
            "INSERT INTO records (collection_id,data) VALUES (1, json('{\"email\":\"x@y.com\"}'));
             INSERT INTO records (collection_id,data) VALUES (1, json('{\"email\":\"x@y.com\"}'));",
        )
        .unwrap();
        let groups = find_duplicates(&c, &spec(), 5).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].values, vec!["x@y.com".to_string()]);
        assert_eq!(groups[0].count, 2);
        assert_eq!(groups[0].first_id, 1);
    }

    #[test]
    fn null_values_are_not_duplicates() {
        let c = conn();
        c.execute_batch(
            "INSERT INTO records (collection_id,data) VALUES (1, json('{}'));
             INSERT INTO records (collection_id,data) VALUES (1, json('{\"email\":null}'));
             INSERT INTO records (collection_id,data) VALUES (1, json('{}'));",
        )
        .unwrap();
        assert!(find_duplicates(&c, &spec(), 5).unwrap().is_empty());
    }

    #[test]
    fn duplicate_error_message_lists_values_and_ids() {
        let groups = vec![DuplicateGroup {
            values: vec!["x@example.com".into()],
            count: 2,
            first_id: 7,
        }];
        let msg = duplicate_error_message(&spec(), &groups);
        assert!(msg.contains("idx_u_1_email"));
        assert!(msg.contains("collection 1"));
        assert!(msg.contains("x@example.com"));
        assert!(msg.contains("count=2"));
        assert!(msg.contains("first_id=7"));
    }

    #[test]
    fn translates_unique_index_error_using_lookup() {
        let raw = "UNIQUE constraint failed: index 'idx_u_5_email'";
        let msg = translate_constraint_error(raw, |name| {
            if name == "idx_u_5_email" {
                Some("email".to_string())
            } else {
                None
            }
        })
        .unwrap();
        assert!(msg.contains("'email'"));
    }

    #[test]
    fn translates_composite_error() {
        let raw = "UNIQUE constraint failed: index 'idx_c_5_0'";
        let msg = translate_constraint_error(raw, |_| None).unwrap();
        assert!(msg.contains("Composite unique constraint"));
    }

    #[test]
    fn non_unique_errors_pass_through_as_none() {
        assert!(translate_constraint_error("database is locked", |_| None).is_none());
    }

    #[test]
    fn field_for_index_reads_metadata() {
        let c = conn();
        ensure_table(&c).unwrap();
        record_applied(&c, &spec()).unwrap();
        assert_eq!(field_for_index(&c, "idx_u_1_email"), Some("email".to_string()));
        assert_eq!(field_for_index(&c, "missing"), None);
    }
}
