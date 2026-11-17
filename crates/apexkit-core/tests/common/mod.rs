//! Shared test harness for apexkit-core integration tests.
//!
//! Builds a real (temporary, on-disk) ApexKit SQLite database with a mock
//! vector provider, so tests exercise the actual rusqlite-backed code paths
//! instead of mocks.

#![allow(dead_code)]

use apexkit_core::database::sqlite::connections::ApexKit;
use apexkit_core::database::traits::{Db, VectorProvider};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

/// Deterministic mock vector provider: returns small fixed vectors and
/// records nothing. Lets DB-level tests run without loading any model.
pub struct MockVectorProvider {
    pub dims: usize,
}

impl Default for MockVectorProvider {
    fn default() -> Self {
        Self { dims: 3 }
    }
}

#[async_trait::async_trait]
impl VectorProvider for MockVectorProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> {
        Ok(vec![0.1; self.dims])
    }
    async fn embed_image(&self, _base64_image: &str) -> Result<Vec<f32>, String> {
        Ok(vec![0.2; self.dims])
    }
    async fn embed_text_for_image_search(&self, _text: &str) -> Result<Vec<f32>, String> {
        Ok(vec![0.3; self.dims])
    }
    async fn search(
        &self,
        _col_id: i64,
        _field: &str,
        _vec: &[f32],
        _limit: usize,
    ) -> Result<Vec<(i64, f32)>, String> {
        Ok(vec![])
    }
    async fn index(
        &self,
        _col_id: i64,
        _rec_id: i64,
        _field: &str,
        _vec: &[f32],
    ) -> Result<(), String> {
        Ok(())
    }
}

/// Creates a fresh temp directory for an isolated test database.
pub fn temp_dir(tag: &str) -> PathBuf {
    let id = uuid::Uuid::new_v4();
    let path = std::env::temp_dir().join(format!("apexkit_core_{tag}_{id}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// Opens the five logical databases and applies the production setup.
pub fn open_databases(
    base: &std::path::Path,
) -> (
    Connection,
    Connection,
    Connection,
    Connection,
    Connection,
) {
    use apexkit_core::database::sqlite::setup::{
        apply_pragmas, setup_core, setup_data, setup_logs, setup_sys, setup_vectors,
    };

    let core = Connection::open(base.join("core.db")).unwrap();
    let data = Connection::open(base.join("data.db")).unwrap();
    let log = Connection::open(base.join("logs.db")).unwrap();
    let sys = Connection::open(base.join("system.db")).unwrap();
    let vec = Connection::open(base.join("vectors.db")).unwrap();

    for c in [&core, &data, &log, &sys, &vec] {
        apply_pragmas(c).unwrap();
    }
    setup_core(&core).unwrap();
    setup_data(&data).unwrap();
    setup_logs(&log).unwrap();
    setup_sys(&sys).unwrap();
    setup_vectors(&vec).unwrap();

    (core, data, log, sys, vec)
}

/// Builds an `ApexKit` DB (as a trait object) rooted at `base`.
pub fn build_db(base: &std::path::Path) -> Arc<dyn Db> {
    let (core, data, log, sys, vec) = open_databases(base);
    let kit = ApexKit::new(
        base.to_str().unwrap(),
        core,
        data,
        log,
        sys,
        vec,
        Arc::new(MockVectorProvider::default()),
        None,
        None,
        "root".to_string(),
    );
    Arc::new(kit)
}

/// Convenience: temp dir + DB in one call. Returns both so the caller can
/// clean the directory up at the end.
pub async fn fresh_db(tag: &str) -> (PathBuf, Arc<dyn Db>) {
    let dir = temp_dir(tag);
    let db = build_db(&dir);
    (dir, db)
}

/// A schema JSON with a single required `title` text field and public policies.
pub fn simple_schema() -> Value {
    json!({
        "fields": {
            "title": { "type": "text", "required": true },
            "views": { "type": "number", "required": false }
        },
        "policies": {
            "read": "public",
            "create": "public",
            "update": "public",
            "delete": "public"
        }
    })
}

/// Parses a schema JSON value into a `CollectionSchema`.
pub fn parse_schema(v: Value) -> apexkit_core::models::schema::CollectionSchema {
    serde_json::from_value(v).unwrap()
}
