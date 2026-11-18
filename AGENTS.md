# AGENTS.md

Repository-specific notes for agents working in the ApexKit workspace.

## Hard constraints

- **Never run cargo.** Do not run `cargo build`, `cargo test`, `cargo check`,
  `cargo install`, `cargo fmt`, `cargo clippy`, or anything that compiles or
  fetches crates. Building this workspace is done by the maintainer on a
  separate machine. Verify Rust changes by inspection, and by driving a
  prebuilt binary or the SQLite files directly from Python.
- Keep new test files under ~500 LOC. Split by concern instead of growing one
  file.

## Layout

- Crates: `apexkit-core`, `apexkit-api`, `apexkit-cli`, `apexkit-vector`.
- Rust integration tests live in `crates/<crate>/tests/` (shared helpers in
  `tests/common/mod.rs`). Inline unit tests use `#[cfg(test)]` modules in `src/`.
- Python integration/stress suite: `apexkit_tools/test/pysuite/` (drives a
  prebuilt binary over HTTP; `conftest.py`, `harness.py`, `api.py`).

## Running the Python suite (no cargo)

```sh
cd apexkit_tools/test/pysuite
python3 -m pytest -q --bin /path/to/apexkit -p no:cacheprovider .
```

`--bin` (or `APEXKIT_BIN`, or `./apexkit`) points at a prebuilt binary; the
harness copies it into a temp workspace and boots it on a free port. The suite
skips itself when no binary is present. It is the only suite meant to be run
locally.

## Testing notes

- The Python suite runs against a released binary, so it may lag the working
  tree. Assert observable behaviour (status codes, totals, round-tripped data),
  not exact error strings, unless you know the binary matches the source.
- The `filter` query param wants JSON (`{"field":"value"}`), not `field=value`.
- `apexkit-core` DB tests use a real on-disk SQLite DB via `tests/common`:
  `fresh_db(tag)` returns `(dir, Arc<dyn Db>)`; DB files are `core.db`,
  `data.db`, `logs.db`, `system.db`, `vectors.db` in that dir.

## Index / uniqueness subsystem (`apexkit-core`)

Uniqueness and lookup indexes are real SQLite **expression indexes** on the
`records` table, reconciled from collection schemas — not the legacy
`_unique_values` side-table (which is dropped on setup).

- Module: `crates/apexkit-core/src/database/sqlite/indexes/`
  (`spec`, `compiler`, `meta`, `diagnostics`, `planner`).
- Naming: unique `idx_u_{col_id}_{field}`, composite `idx_c_{col_id}_{index}`,
  lookup `idx_n_{col_id}_{field}`.
- Diff base: the `_applied_indexes` metadata table (`meta::load_applied`).
- Entry points: `plan_collection`/`plan_all` (read), `execute_plan` (async
  write), `ensure_indexes`/`reconcile_collection` (sync), `field_for_index_name`,
  `translate_constraint_error`, `write_error`.
- Query SQL must emit `json_extract(records.data, '$.field')` (not `->>`/`->`
  chains) so the planner matches these expression indexes. Verify a plan with
  `EXPLAIN QUERY PLAN` against a real `data.db`.
- SQLite reports a violation as `UNIQUE constraint failed: index '<name>'`; the
  index name is decoded back to the field name from the in-memory schema.

## Relations subsystem (`apexkit-core`)

A relation is a single directed edge between two records, stored in `_relations`
as `(origin_rec_id, target_rec_id, properties)` — no collection ids and no
`rel_name`. The pair is unique, and both columns have
`FOREIGN KEY ... REFERENCES records(id) ON DELETE CASCADE`, so deleting either
endpoint removes the edge automatically (`PRAGMA foreign_keys = ON` is set in
`apply_pragmas`).

- `create_relation` is a guarded insert (`SELECT ... WHERE EXISTS`), so a
  dangling id is a no-op rather than a foreign-key error.
- Relation *names* live in the collection schema (`schema.relations`), not on the
  edge. `sync_relations` groups a record's relation fields by target collection
  and treats the union of their values as that collection's edge set; it leaves
  edges to other collections (e.g. those made through the relations API) alone.
- Expand is schema-driven (`crates/apexkit-core/src/query/builder.rs`): forward
  resolves the scalar id to one record, reverse resolves the id to a list.
- Legacy wide-table databases are migrated once by
  `crates/apexkit-core/src/database/sqlite/migrate.rs` (`needs_relations_migration`
  / `migrate_relations`), which collapses named duplicates to one edge.
