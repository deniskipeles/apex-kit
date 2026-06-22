use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

pub mod ai_actions;
pub mod data;
pub mod schema;
pub mod scripts;
pub mod templates;

// Struct to handle nested path params safely
#[derive(Deserialize)]
pub struct ExportPath {
    pub id: i64,
}
// --- DTOs ---

#[derive(Deserialize, IntoParams, ToSchema)]
pub struct ExportQuery {
    /// Format to export (json or csv)
    #[serde(default)]
    #[param(example = "json")]
    pub format: String,
    /// Sorting field, e.g. -created
    pub sort: Option<String>,
    /// Filter object string, e.g. {"status":"active"}
    pub filter: Option<String>,
}
