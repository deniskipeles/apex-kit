//! Schema-to-DDL index compiler.
//!
//! Replaces the old `_unique_values` side-index with real SQLite expression
//! indexes over `records`. Those indexes do two jobs that the side table could
//! not: they enforce uniqueness atomically with the record write, and SQLite's
//! query planner can use the very same `json_extract` expression to satisfy
//! lookups.
//!
//! Layout:
//! - [`spec`]        — [`spec::IndexSpec`] and SQL rendering.
//! - [`compiler`]    — derives the desired index set from schemas.
//! - [`meta`]        — `_applied_indexes` persistence (the diff base).
//! - [`diagnostics`] — duplicate detection and error translation.
//! - [`planner`]     — diff-based reconciliation.

pub mod compiler;
pub mod diagnostics;
pub mod meta;
pub mod planner;
pub mod spec;

pub use compiler::{composite_index_name, compute_desired_indexes, field_index_name};
pub use planner::{
    IndexPlan, apply_plan, ensure_indexes, execute_plan, field_for_index_name, plan_all,
    plan_collection, reconcile_collection,
};
pub use spec::{IndexKind, IndexSpec};
