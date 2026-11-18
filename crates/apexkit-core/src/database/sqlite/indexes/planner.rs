//! Diff-based index planner.
//!
//! Reconciliation happens in two phases so it fits both the synchronous test
//! harness (a plain `rusqlite::Connection`) and the production write path (an
//! async [`WriteManager`] batcher that owns the connection):
//!
//! 1. **Plan** — read the applied set, compute the desired set, diff them, and
//!    pre-flight any new unique index for existing duplicates. Pure reads.
//! 2. **Execute** — run the DDL and metadata writes.

use super::compiler::compute_desired_indexes;
use super::diagnostics::{duplicate_error_message, find_duplicates};
use super::meta::{drop_index_if_exists, ensure_table, forget_applied, load_applied, record_applied};
use super::spec::{IndexKind, IndexSpec};
use crate::database::batching::WriteManager;
use crate::database::traits::IntoSqlVal;
use crate::models::schema::CollectionSchema;
use crate::models::Collection;
use rusqlite::Connection;
use std::collections::BTreeMap;

/// The set of operations required to move from applied to desired.
#[derive(Debug, Default, PartialEq)]
pub struct IndexPlan {
    /// Indexes present but no longer wanted.
    pub drop: Vec<String>,
    /// Indexes wanted but not present.
    pub create: Vec<IndexSpec>,
    /// Indexes whose definition changed (dropped then recreated).
    pub recreate: Vec<IndexSpec>,
}

impl IndexPlan {
    /// True when no work is required.
    pub fn is_empty(&self) -> bool {
        self.drop.is_empty() && self.create.is_empty() && self.recreate.is_empty()
    }
}

/// Computes the diff between the desired and applied index sets.
pub fn diff(
    desired: &BTreeMap<String, IndexSpec>,
    applied: &BTreeMap<String, IndexSpec>,
) -> IndexPlan {
    let mut plan = IndexPlan::default();

    for (name, spec) in applied {
        match desired.get(name) {
            None => plan.drop.push(name.clone()),
            Some(d) if !d.same_shape(spec) => plan.recreate.push(d.clone()),
            Some(_) => {}
        }
    }

    for (name, spec) in desired {
        if !applied.contains_key(name) {
            plan.create.push(spec.clone());
        }
    }

    plan
}

/// Validates every CREATE statement in the plan up front.
fn render_create_sql(plan: &IndexPlan) -> Result<Vec<(IndexSpec, String)>, String> {
    let mut create_sql = Vec::new();
    for spec in plan.create.iter().chain(plan.recreate.iter()) {
        create_sql.push((spec.clone(), spec.to_create_sql()?));
    }
    Ok(create_sql)
}

/// Rejects the plan if any new unique index would collide with existing data.
fn preflight_duplicates(conn: &Connection, plan: &IndexPlan) -> Result<(), String> {
    for spec in plan.create.iter().chain(plan.recreate.iter()) {
        if matches!(spec.kind, IndexKind::Unique | IndexKind::Composite) {
            let dups = find_duplicates(conn, spec, 5).map_err(|e| e.to_string())?;
            if !dups.is_empty() {
                return Err(duplicate_error_message(spec, &dups));
            }
        }
    }
    Ok(())
}

/// Read phase: computes the plan for one collection and pre-flights duplicates.
pub fn plan_collection(
    conn: &Connection,
    collection_id: i64,
    schema: &CollectionSchema,
) -> Result<IndexPlan, String> {
    let col = Collection {
        id: collection_id,
        name: String::new(),
        schema: Some(schema.clone()),
        index: None,
    };
    let desired = compute_desired_indexes(std::slice::from_ref(&col));
    let applied: BTreeMap<String, IndexSpec> = load_applied(conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|(_, s)| s.collection_id == collection_id)
        .collect();

    let plan = diff(&desired, &applied);
    render_create_sql(&plan)?;
    preflight_duplicates(conn, &plan)?;
    Ok(plan)
}

/// Read phase: computes the plan for the whole database and pre-flights duplicates.
pub fn plan_all(conn: &Connection, collections: &[Collection]) -> Result<IndexPlan, String> {
    let desired = compute_desired_indexes(collections);
    let applied = load_applied(conn).map_err(|e| e.to_string())?;
    let plan = diff(&desired, &applied);
    render_create_sql(&plan)?;
    preflight_duplicates(conn, &plan)?;
    Ok(plan)
}

/// Execute phase: applies the plan through the async write batcher.
pub async fn execute_plan(
    batcher: &WriteManager,
    plan: &IndexPlan,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if plan.is_empty() {
        return Ok(());
    }

    ensure_table_async(batcher).await?;

    for name in &plan.drop {
        drop_index_if_exists_async(batcher, name).await?;
        forget_applied_async(batcher, name).await?;
    }

    for spec in &plan.recreate {
        drop_index_if_exists_async(batcher, &spec.name).await?;
        forget_applied_async(batcher, &spec.name).await?;
    }

    for spec in plan.create.iter().chain(plan.recreate.iter()) {
        let sql = spec.to_create_sql().map_err(boxed)?;
        batcher.execute(sql, vec![]).await.map_err(boxed)?;
        record_applied_async(batcher, spec).await?;
    }

    Ok(())
}

fn boxed(e: String) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::other(e))
}

async fn ensure_table_async(
    batcher: &WriteManager,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    batcher
        .execute(
            "CREATE TABLE IF NOT EXISTS _applied_indexes (
                index_name    TEXT PRIMARY KEY,
                collection_id INTEGER NOT NULL,
                kind          TEXT NOT NULL,
                fields        TEXT NOT NULL
            )"
            .into(),
            vec![],
        )
        .await
        .map_err(boxed)?;
    Ok(())
}

async fn drop_index_if_exists_async(
    batcher: &WriteManager,
    name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let quoted = name.replace('"', "\"\"");
    batcher
        .execute(format!("DROP INDEX IF EXISTS \"{}\"", quoted), vec![])
        .await
        .map_err(boxed)?;
    Ok(())
}

async fn forget_applied_async(
    batcher: &WriteManager,
    name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    batcher
        .execute(
            "DELETE FROM _applied_indexes WHERE index_name = ?1".into(),
            vec![name.to_string().into_val()],
        )
        .await
        .map_err(boxed)?;
    Ok(())
}

async fn record_applied_async(
    batcher: &WriteManager,
    spec: &IndexSpec,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fields_json = serde_json::to_string(&spec.fields).unwrap_or_else(|_| "[]".to_string());
    batcher
        .execute(
            "INSERT OR REPLACE INTO _applied_indexes (index_name, collection_id, kind, fields)
             VALUES (?1, ?2, ?3, ?4)"
                .into(),
            vec![
                spec.name.clone().into_val(),
                spec.collection_id.into_val(),
                spec.kind.as_str().to_string().into_val(),
                fields_json.into_val(),
            ],
        )
        .await
        .map_err(boxed)?;
    Ok(())
}

/// Synchronous whole-database reconciliation, for tests and direct use.
pub fn ensure_indexes(conn: &Connection, collections: &[Collection]) -> Result<(), String> {
    let plan = plan_all(conn, collections)?;
    apply_plan(conn, &plan)
}

/// Synchronous single-collection reconciliation, for tests and direct use.
pub fn reconcile_collection(
    conn: &Connection,
    collection_id: i64,
    schema: &CollectionSchema,
) -> Result<(), String> {
    let plan = plan_collection(conn, collection_id, schema)?;
    apply_plan(conn, &plan)
}

/// Executes a plan synchronously inside a single transaction.
pub fn apply_plan(conn: &Connection, plan: &IndexPlan) -> Result<(), String> {
    if plan.is_empty() {
        return Ok(());
    }

    ensure_table(conn).map_err(|e| e.to_string())?;

    let create_sql = render_create_sql(plan)?;

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;

    match apply_plan_in_tx(conn, plan, &create_sql) {
        Ok(()) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

fn apply_plan_in_tx(
    conn: &Connection,
    plan: &IndexPlan,
    create_sql: &[(IndexSpec, String)],
) -> Result<(), String> {
    for name in &plan.drop {
        drop_index_if_exists(conn, name).map_err(|e| e.to_string())?;
        forget_applied(conn, name).map_err(|e| e.to_string())?;
    }

    for spec in &plan.recreate {
        drop_index_if_exists(conn, &spec.name).map_err(|e| e.to_string())?;
        forget_applied(conn, &spec.name).map_err(|e| e.to_string())?;
    }

    for (spec, sql) in create_sql {
        conn.execute_batch(sql).map_err(|e| e.to_string())?;
        record_applied(conn, spec).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Maps a generated index name back to its field name(s) for error reporting.
pub fn field_for_index_name(
    collection_id: i64,
    schema: &CollectionSchema,
    index_name: &str,
) -> Option<String> {
    let col = Collection {
        id: collection_id,
        name: String::new(),
        schema: Some(schema.clone()),
        index: None,
    };
    let desired = compute_desired_indexes(std::slice::from_ref(&col));
    desired.get(index_name).map(|s| s.fields.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::schema::CollectionSchema;
    use serde_json::json;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute(
            "CREATE TABLE records (id INTEGER PRIMARY KEY, collection_id INTEGER, data JSONB)",
            [],
        )
        .unwrap();
        c
    }

    fn collection(id: i64, schema: serde_json::Value) -> Collection {
        Collection {
            id,
            name: format!("c{id}"),
            schema: Some(serde_json::from_value::<CollectionSchema>(schema).unwrap()),
            index: None,
        }
    }

    fn unique_email_schema() -> serde_json::Value {
        json!({ "fields": { "email": { "type": "email", "required": true, "unique": true } } })
    }

    fn applied_names(c: &Connection) -> Vec<String> {
        let mut stmt = c
            .prepare("SELECT index_name FROM _applied_indexes ORDER BY index_name")
            .unwrap();
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        rows
    }

    #[test]
    fn first_run_creates_index_and_records_metadata() {
        let c = conn();
        let cols = vec![collection(5, unique_email_schema())];
        ensure_indexes(&c, &cols).unwrap();

        assert_eq!(applied_names(&c), vec!["idx_u_5_email".to_string()]);
        assert!(super::super::meta::index_exists(&c, "idx_u_5_email").unwrap());
    }

    #[test]
    fn second_run_is_a_noop() {
        let c = conn();
        let cols = vec![collection(5, unique_email_schema())];
        ensure_indexes(&c, &cols).unwrap();
        ensure_indexes(&c, &cols).unwrap();
        assert_eq!(applied_names(&c), vec!["idx_u_5_email".to_string()]);
    }

    #[test]
    fn removed_flag_drops_index() {
        let c = conn();
        let with = vec![collection(5, unique_email_schema())];
        ensure_indexes(&c, &with).unwrap();

        let without = vec![collection(
            5,
            json!({ "fields": { "email": { "type": "email", "required": true } } }),
        )];
        ensure_indexes(&c, &without).unwrap();

        assert!(applied_names(&c).is_empty());
        assert!(!super::super::meta::index_exists(&c, "idx_u_5_email").unwrap());
    }

    #[test]
    fn kind_change_recreates_index() {
        let c = conn();
        ensure_indexes(&c, &[collection(5, unique_email_schema())]).unwrap();

        // Now the field is merely sql_indexed: name changes idx_u_ -> idx_n_.
        let changed = vec![collection(
            5,
            json!({ "fields": { "email": { "type": "email", "required": true, "sql_indexed": true } } }),
        )];
        ensure_indexes(&c, &changed).unwrap();

        assert_eq!(applied_names(&c), vec!["idx_n_5_email".to_string()]);
        assert!(!super::super::meta::index_exists(&c, "idx_u_5_email").unwrap());
    }

    #[test]
    fn existing_duplicates_block_unique_index_with_actionable_error() {
        let c = conn();
        c.execute_batch(
            "INSERT INTO records (collection_id,data) VALUES (5, json('{\"email\":\"x@y.com\"}'));
             INSERT INTO records (collection_id,data) VALUES (5, json('{\"email\":\"x@y.com\"}'));",
        )
        .unwrap();

        let err = ensure_indexes(&c, &[collection(5, unique_email_schema())]).unwrap_err();
        assert!(err.contains("idx_u_5_email"), "{err}");
        assert!(err.contains("x@y.com"), "{err}");
        assert!(err.contains("first_id="), "{err}");
        assert!(applied_names(&c).is_empty());
        assert!(!super::super::meta::index_exists(&c, "idx_u_5_email").unwrap());
    }

    #[test]
    fn unique_index_enforces_uniqueness_on_insert() {
        let c = conn();
        ensure_indexes(&c, &[collection(5, unique_email_schema())]).unwrap();
        c.execute_batch("INSERT INTO records (collection_id,data) VALUES (5, json('{\"email\":\"a@b.com\"}'))")
            .unwrap();
        let dup = c.execute_batch("INSERT INTO records (collection_id,data) VALUES (5, json('{\"email\":\"a@b.com\"}'))");
        assert!(dup.is_err());
    }

    #[test]
    fn reconcile_collection_only_touches_its_own_indexes() {
        let c = conn();
        ensure_indexes(&c, &[collection(1, unique_email_schema())]).unwrap();

        let schema: CollectionSchema = serde_json::from_value(unique_email_schema()).unwrap();
        reconcile_collection(&c, 5, &schema).unwrap();

        assert_eq!(
            applied_names(&c),
            vec!["idx_u_1_email".to_string(), "idx_u_5_email".to_string()]
        );
    }

    #[test]
    fn plan_diff_reports_all_three_buckets() {
        let desired = compute_desired_indexes(&[
            collection(1, unique_email_schema()),
            collection(
                2,
                json!({ "fields": { "status": { "type": "string", "required": false, "sql_indexed": true } } }),
            ),
        ]);
        let applied = compute_desired_indexes(&[
            collection(1, unique_email_schema()),
            collection(2, unique_email_schema()),
        ]);
        let plan = diff(&desired, &applied);
        assert!(plan.create.iter().any(|s| s.name == "idx_n_2_status"));
        assert!(plan.drop.iter().any(|n| n == "idx_u_2_email"));
    }

    #[test]
    fn field_lookup_resolves_single_and_composite() {
        let schema: CollectionSchema = serde_json::from_value(json!({
            "fields": {
                "tenant_slug": { "type": "string", "required": true },
                "sku": { "type": "string", "required": true }
            },
            "composite_unique": [["tenant_slug", "sku"]]
        }))
        .unwrap();
        assert_eq!(
            field_for_index_name(5, &schema, "idx_c_5_0"),
            Some("tenant_slug, sku".to_string())
        );
        assert_eq!(field_for_index_name(5, &schema, "nope"), None);
    }
}
