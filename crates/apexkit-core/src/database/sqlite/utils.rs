use crate::models::{Collection, Record};
use rusqlite::Row;

pub fn row_to_collection(
    row: &Row,
) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
    let schema_str: Option<String> = row.get(2)?;
    let schema = match schema_str {
        Some(s) => serde_json::from_str(&s)?,
        None => None,
    };
    let index: Option<String> = row.get(3).ok();

    Ok(Collection {
        id: row.get(0)?,
        name: row.get(1)?,
        schema,
        index,
    })
}

pub fn row_to_record(
    row: &Row,
) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
    let id = row.get(0)?;

    let val_data = row
        .get_ref(1)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // OPTIMIZED: If the data is stored in SQLite's native JSONB format (Blob),
    // deserialize it directly into a serde_json::Value without converting to a string first.
    let data: serde_json::Value = match val_data {
        rusqlite::types::ValueRef::Blob(b) => {
            // Directly deserialize from the SQLite JSONB binary slice
            match serde_sqlite_jsonb::from_slice(b) {
                Ok(val) => val,
                Err(_) => {
                    // Fallback: If it's a standard text blob masquerading as a blob, try standard parsing
                    serde_json::from_slice(b).unwrap_or(serde_json::json!({}))
                }
            }
        }
        rusqlite::types::ValueRef::Text(s) => {
            // Fallback for legacy text data or if the query used `json(data)`
            serde_json::from_str(std::str::from_utf8(s).unwrap_or("{}"))?
        }
        _ => serde_json::json!({}),
    };

    let expand = if let Ok(val_expand) = row.get_ref(2) {
        match val_expand {
            rusqlite::types::ValueRef::Blob(b) => match serde_sqlite_jsonb::from_slice(b) {
                Ok(val) => Some(val),
                Err(_) => Some(serde_json::from_slice(b).unwrap_or(serde_json::json!({}))),
            },
            rusqlite::types::ValueRef::Text(s) => Some(serde_json::from_str(
                std::str::from_utf8(s).unwrap_or("{}"),
            )?),
            _ => None,
        }
    } else {
        None
    };

    let created: String = row.get(3).unwrap_or_else(|_| "".to_string());
    let updated: String = row.get(4).unwrap_or_else(|_| "".to_string());

    Ok(Record {
        id,
        data,
        expand,
        created,
        updated,
    })
}

pub fn calculate_dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total_size = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                total_size += calculate_dir_size(&entry.path())?;
            } else {
                total_size += metadata.len();
            }
        }
    } else if path.exists() {
        total_size = path.metadata()?.len();
    }
    Ok(total_size)
}
