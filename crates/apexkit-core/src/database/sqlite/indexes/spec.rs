//! Index specifications and their SQL rendering.
//!
//! An [`IndexSpec`] is the compiler's unit of work: a description of one index
//! over the `records` table. The generated SQL always leads with
//! `collection_id` so the index is usable for the common
//! `WHERE collection_id = ? AND <field> = ?` lookup shape.

use serde::{Deserialize, Serialize};

/// The three index families the compiler emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexKind {
    /// `unique: true` on a single field.
    Unique,
    /// An entry from `composite_unique`.
    Composite,
    /// `sql_indexed` on a field or relation (a plain lookup index).
    NonUnique,
}

impl IndexKind {
    /// Stable string used in the `_applied_indexes` metadata table.
    pub fn as_str(self) -> &'static str {
        match self {
            IndexKind::Unique => "unique",
            IndexKind::Composite => "composite",
            IndexKind::NonUnique => "non_unique",
        }
    }

    /// Parses the value stored in `_applied_indexes.kind`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unique" => Some(IndexKind::Unique),
            "composite" => Some(IndexKind::Composite),
            "non_unique" => Some(IndexKind::NonUnique),
            _ => None,
        }
    }

    fn is_unique(self) -> bool {
        matches!(self, IndexKind::Unique | IndexKind::Composite)
    }
}

/// A desired index over `records`.
///
/// `fields` are schema field (or relation) *names*, never arbitrary user input:
/// collection and field names are validated to `[A-Za-z0-9_]+` before a schema
/// is persisted, and the generated index name is validated again here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexSpec {
    pub name: String,
    pub collection_id: i64,
    pub kind: IndexKind,
    pub fields: Vec<String>,
}

/// Escapes a single-quoted SQL string literal body.
fn escape_single_quotes(s: &str) -> String {
    s.replace('\'', "''")
}

/// Only `[A-Za-z0-9_]` identifiers may reach raw DDL.
fn is_safe_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl IndexSpec {
    /// Renders the `CREATE [UNIQUE] INDEX` statement for this spec.
    ///
    /// Fails (rather than interpolating) if any identifier is unsafe, so a bad
    /// schema can never inject DDL.
    pub fn to_create_sql(&self) -> Result<String, String> {
        if !is_safe_identifier(&self.name) {
            return Err(format!("unsafe index name: {}", self.name));
        }

        let mut expressions = vec!["collection_id".to_string()];
        for field in &self.fields {
            if !is_safe_identifier(field) {
                return Err(format!("unsafe field name: {}", field));
            }
            expressions.push(format!(
                "json_extract(data, '$.{}')",
                escape_single_quotes(field)
            ));
        }

        let unique = if self.kind.is_unique() { "UNIQUE " } else { "" };
        Ok(format!(
            "CREATE {}INDEX {} ON records ({})",
            unique,
            self.name,
            expressions.join(", ")
        ))
    }

    /// The per-expression JSON paths used for duplicate detection, in order.
    pub fn expressions(&self) -> Vec<String> {
        self.fields
            .iter()
            .map(|f| format!("json_extract(data, '$.{}')", escape_single_quotes(f)))
            .collect()
    }

    /// True when the two specs would render identical SQL.
    pub fn same_shape(&self, other: &IndexSpec) -> bool {
        self.collection_id == other.collection_id
            && self.kind == other.kind
            && self.fields == other.fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: IndexKind, fields: &[&str]) -> IndexSpec {
        IndexSpec {
            name: "idx_test".into(),
            collection_id: 5,
            kind,
            fields: fields.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn unique_single_field_sql() {
        let s = IndexSpec {
            name: "idx_u_5_email".into(),
            ..spec(IndexKind::Unique, &["email"])
        };
        assert_eq!(
            s.to_create_sql().unwrap(),
            "CREATE UNIQUE INDEX idx_u_5_email ON records (collection_id, json_extract(data, '$.email'))"
        );
    }

    #[test]
    fn non_unique_single_field_sql() {
        let s = IndexSpec {
            name: "idx_n_1_status".into(),
            collection_id: 1,
            ..spec(IndexKind::NonUnique, &["status"])
        };
        assert_eq!(
            s.to_create_sql().unwrap(),
            "CREATE INDEX idx_n_1_status ON records (collection_id, json_extract(data, '$.status'))"
        );
    }

    #[test]
    fn composite_uses_separate_expressions() {
        let s = IndexSpec {
            name: "idx_c_5_0".into(),
            ..spec(IndexKind::Composite, &["tenant_slug", "sku"])
        };
        assert_eq!(
            s.to_create_sql().unwrap(),
            "CREATE UNIQUE INDEX idx_c_5_0 ON records (collection_id, json_extract(data, '$.tenant_slug'), json_extract(data, '$.sku'))"
        );
    }

    #[test]
    fn unsafe_identifiers_are_rejected() {
        let s = IndexSpec {
            name: "idx_u_5_email; DROP TABLE records".into(),
            ..spec(IndexKind::Unique, &["email"])
        };
        assert!(s.to_create_sql().is_err());

        let s = IndexSpec {
            name: "idx_u_5_email".into(),
            ..spec(IndexKind::Unique, &["em'ail"])
        };
        assert!(s.to_create_sql().is_err());
    }

    #[test]
    fn same_shape_compares_all_components() {
        let a = spec(IndexKind::Unique, &["email"]);
        let mut b = a.clone();
        assert!(a.same_shape(&b));
        b.kind = IndexKind::NonUnique;
        assert!(!a.same_shape(&b));
        b = a.clone();
        b.collection_id = 6;
        assert!(!a.same_shape(&b));
        b = a.clone();
        b.fields = vec!["other".into()];
        assert!(!a.same_shape(&b));
    }

    #[test]
    fn kind_roundtrips_through_strings() {
        for k in [IndexKind::Unique, IndexKind::Composite, IndexKind::NonUnique] {
            assert_eq!(IndexKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(IndexKind::parse("bogus"), None);
    }
}
