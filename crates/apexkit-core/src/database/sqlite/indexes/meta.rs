//! Persistence for the `_applied_indexes` metadata table.
//!
//! This table is the compiler's diff base: it records exactly which indexes the
//! compiler created, so a later run can reconcile against the current schema.
//! SQLite's own index catalogue cannot be diffed reliably, because an index
//! name alone does not describe the expression it was built over.

use super::spec::{IndexKind, IndexSpec};
use rusqlite::{Connection, params};
use std::collections::BTreeMap;

/// Creates the metadata table if it does not yet exist.
pub fn ensure_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _applied_indexes (
            index_name    TEXT PRIMARY KEY,
            collection_id INTEGER NOT NULL,
            kind          TEXT NOT NULL,
            fields        TEXT NOT NULL
        )",
        [],
    )?;
    Ok(())
}

/// Loads every recorded index spec, keyed by index name.
///
/// Returns an empty map when the metadata table does not exist yet, so the
/// read-only planning path never needs to run DDL.
pub fn load_applied(conn: &Connection) -> rusqlite::Result<BTreeMap<String, IndexSpec>> {
    if !table_exists(conn)? {
        return Ok(BTreeMap::new());
    }

    let mut stmt =
        conn.prepare("SELECT index_name, collection_id, kind, fields FROM _applied_indexes")?;
    let mut rows = stmt.query([])?;

    let mut applied = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let name: String = row.get(0)?;
        let collection_id: i64 = row.get(1)?;
        let kind_str: String = row.get(2)?;
        let fields_json: String = row.get(3)?;

        let Some(kind) = IndexKind::parse(&kind_str) else {
            continue;
        };
        let fields: Vec<String> = serde_json::from_str(&fields_json).unwrap_or_default();

        applied.insert(
            name.clone(),
            IndexSpec {
                name,
                collection_id,
                kind,
                fields,
            },
        );
    }
    Ok(applied)
}

/// Inserts or replaces a single applied-index record.
pub fn record_applied(conn: &Connection, spec: &IndexSpec) -> rusqlite::Result<()> {
    let fields_json = serde_json::to_string(&spec.fields).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT OR REPLACE INTO _applied_indexes (index_name, collection_id, kind, fields)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            spec.name,
            spec.collection_id,
            spec.kind.as_str(),
            fields_json
        ],
    )?;
    Ok(())
}

/// Removes a single applied-index record.
pub fn forget_applied(conn: &Connection, name: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM _applied_indexes WHERE index_name = ?1",
        params![name],
    )?;
    Ok(())
}

/// Drops an index if it exists (no-op otherwise).
pub fn drop_index_if_exists(conn: &Connection, name: &str) -> rusqlite::Result<()> {
    // Index names originate from schema-derived identifiers and are validated
    // by `IndexSpec::to_create_sql`; quote defensively regardless.
    let quoted = name.replace('"', "\"\"");
    conn.execute_batch(&format!("DROP INDEX IF EXISTS \"{}\"", quoted))?;
    Ok(())
}

/// True if an index with this name exists in the SQLite catalogue.
pub fn index_exists(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare("SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1")?;
    let mut rows = stmt.query(params![name])?;
    Ok(rows.next()?.is_some())
}

/// True if the `_applied_indexes` metadata table exists.
pub fn table_exists(conn: &Connection) -> rusqlite::Result<bool> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='_applied_indexes'")?;
    let mut rows = stmt.query([])?;
    Ok(rows.next()?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute(
            "CREATE TABLE records (id INTEGER PRIMARY KEY, collection_id INTEGER, data JSONB)",
            [],
        )
        .unwrap();
        c
    }

    fn spec(name: &str) -> IndexSpec {
        IndexSpec {
            name: name.into(),
            collection_id: 7,
            kind: IndexKind::Unique,
            fields: vec!["email".into()],
        }
    }

    #[test]
    fn ensure_table_is_idempotent() {
        let c = conn();
        ensure_table(&c).unwrap();
        ensure_table(&c).unwrap();
        assert!(load_applied(&c).unwrap().is_empty());
    }

    #[test]
    fn record_and_load_roundtrip() {
        let c = conn();
        ensure_table(&c).unwrap();
        record_applied(&c, &spec("idx_u_7_email")).unwrap();

        let applied = load_applied(&c).unwrap();
        let got = applied.get("idx_u_7_email").expect("recorded");
        assert_eq!(got.collection_id, 7);
        assert_eq!(got.kind, IndexKind::Unique);
        assert_eq!(got.fields, vec!["email".to_string()]);
    }

    #[test]
    fn record_replaces_existing_row() {
        let c = conn();
        ensure_table(&c).unwrap();
        record_applied(&c, &spec("idx_u_7_email")).unwrap();

        let mut updated = spec("idx_u_7_email");
        updated.kind = IndexKind::NonUnique;
        record_applied(&c, &updated).unwrap();

        let applied = load_applied(&c).unwrap();
        assert_eq!(applied.get("idx_u_7_email").unwrap().kind, IndexKind::NonUnique);
        assert_eq!(applied.len(), 1);
    }

    #[test]
    fn forget_removes_row() {
        let c = conn();
        ensure_table(&c).unwrap();
        record_applied(&c, &spec("idx_u_7_email")).unwrap();
        forget_applied(&c, "idx_u_7_email").unwrap();
        assert!(load_applied(&c).unwrap().is_empty());
    }

    #[test]
    fn unknown_kind_rows_are_ignored() {
        let c = conn();
        ensure_table(&c).unwrap();
        c.execute(
            "INSERT INTO _applied_indexes (index_name, collection_id, kind, fields)
             VALUES ('idx_x', 1, 'bogus', '[\"a\"]')",
            [],
        )
        .unwrap();
        assert!(load_applied(&c).unwrap().is_empty());
    }

    #[test]
    fn drop_and_exists_helpers() {
        let c = conn();
        let s = spec("idx_u_7_email");
        c.execute_batch(&s.to_create_sql().unwrap()).unwrap();
        assert!(index_exists(&c, "idx_u_7_email").unwrap());

        drop_index_if_exists(&c, "idx_u_7_email").unwrap();
        assert!(!index_exists(&c, "idx_u_7_email").unwrap());

        // Dropping a missing index is a no-op.
        drop_index_if_exists(&c, "idx_u_7_email").unwrap();
    }
}
