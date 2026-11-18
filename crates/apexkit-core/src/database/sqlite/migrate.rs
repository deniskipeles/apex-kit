//! One-time migration to the edge-only relation model.
//!
//! Legacy databases modelled a relation with a wide row
//! (`origin_col_id`, `origin_rec_id`, `target_col_id`, `target_rec_id`,
//! `rel_name`, `properties`). The current model reduces a relation to a single
//! edge between two records, so the table must be rebuilt: the collection and
//! relation-name columns are dropped, the id pair becomes unique, and foreign
//! keys are added so SQLite removes an edge when either endpoint is deleted.
//!
//! Because the legacy unique index included `rel_name`, two named relations can
//! share an id pair. Those rows collapse to the single edge the new model
//! allows; the rebuild groups by the pair so the surviving row keeps one set of
//! `properties`.

use rusqlite::{Connection, Result};

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1")?;
    Ok(stmt.exists([name])?)
}

fn has_column(conn: &Connection, table: &str, col: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == col {
            return Ok(true);
        }
    }
    Ok(false)
}

/// True when `_relations` still uses the legacy wide layout.
pub fn needs_relations_migration(conn: &Connection) -> Result<bool> {
    if !table_exists(conn, "_relations")? {
        return Ok(false);
    }
    has_column(conn, "_relations", "origin_col_id")
}

/// Rebuilds `_relations` as an edge table, atomically.
pub fn migrate_relations(conn: &Connection) -> Result<()> {
    // Foreign keys cannot be toggled inside a transaction and must be off while
    // the table is dropped and renamed.
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    conn.execute_batch("BEGIN IMMEDIATE;")?;

    if let Err(e) = migrate_inner(conn) {
        let _ = conn.execute_batch("ROLLBACK;");
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        return Err(e);
    }

    conn.execute_batch("COMMIT;")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(())
}

fn migrate_inner(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE _relations_new (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            origin_rec_id INTEGER NOT NULL,
            target_rec_id INTEGER NOT NULL,
            properties    JSON,
            FOREIGN KEY (origin_rec_id) REFERENCES records(id) ON DELETE CASCADE,
            FOREIGN KEY (target_rec_id) REFERENCES records(id) ON DELETE CASCADE
        );",
    )?;

    // GROUP BY collapses the legacy named-relation duplicates to one edge.
    conn.execute_batch(
        "INSERT INTO _relations_new (origin_rec_id, target_rec_id, properties)
            SELECT origin_rec_id, target_rec_id, MAX(properties)
            FROM _relations
            GROUP BY origin_rec_id, target_rec_id;",
    )?;

    conn.execute_batch(
        "DROP TABLE _relations;
         ALTER TABLE _relations_new RENAME TO _relations;
         CREATE UNIQUE INDEX IF NOT EXISTS idx_relations_unique ON _relations(origin_rec_id, target_rec_id);
         CREATE INDEX IF NOT EXISTS idx_relations_origin ON _relations(origin_rec_id);
         CREATE INDEX IF NOT EXISTS idx_relations_target ON _relations(target_rec_id);",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE records (id INTEGER PRIMARY KEY AUTOINCREMENT, collection_id INTEGER NOT NULL, data JSONB NOT NULL, created DATETIME DEFAULT CURRENT_TIMESTAMP, updated DATETIME DEFAULT CURRENT_TIMESTAMP);
             CREATE TABLE _relations (id INTEGER PRIMARY KEY AUTOINCREMENT, origin_col_id INTEGER NOT NULL, origin_rec_id INTEGER NOT NULL, target_col_id INTEGER NOT NULL, target_rec_id INTEGER NOT NULL, rel_name TEXT NOT NULL, properties JSON);
             INSERT INTO records (collection_id, data) VALUES (1, json('{\"a\":1}'));
             INSERT INTO records (collection_id, data) VALUES (1, json('{\"b\":2}'));
             INSERT INTO _relations (origin_col_id,origin_rec_id,target_col_id,target_rec_id,rel_name,properties) VALUES (1,1,1,2,'author','{\"w\":1}');
             INSERT INTO _relations (origin_col_id,origin_rec_id,target_col_id,target_rec_id,rel_name,properties) VALUES (1,1,1,2,'editor','{\"w\":2}');",
        )
        .unwrap();
        c
    }

    #[test]
    fn legacy_layout_is_detected() {
        assert!(needs_relations_migration(&legacy_db()).unwrap());
    }

    #[test]
    fn migration_drops_relation_name_column() {
        let c = legacy_db();
        migrate_relations(&c).unwrap();
        assert!(!has_column(&c, "_relations", "rel_name").unwrap());
        assert!(!has_column(&c, "_relations", "origin_col_id").unwrap());
        assert!(!needs_relations_migration(&c).unwrap());
    }

    #[test]
    fn migration_collapses_duplicate_pairs() {
        let c = legacy_db();
        migrate_relations(&c).unwrap();
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM _relations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn migrated_table_cascades_on_delete() {
        let c = legacy_db();
        migrate_relations(&c).unwrap();
        c.execute("PRAGMA foreign_keys = ON", []).unwrap();
        c.execute("DELETE FROM records WHERE id = 2", []).unwrap();
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM _relations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}
