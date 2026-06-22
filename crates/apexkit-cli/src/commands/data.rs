use super::get_cli_db;
use apexkit_core::query::QueryOptions;
use clap::Subcommand;
use std::io::Write;

#[derive(Subcommand, Debug)]
pub enum DataCmd {
    /// Force re-index of Tantivy search for a collection
    Reindex {
        /// Collection ID
        id: i64,
    },
    /// Export a collection to JSON (printed to stdout)
    Export {
        /// Collection ID
        id: i64,
        /// Pretty print JSON
        #[arg(long, action)]
        pretty: bool,
    },
    /// Clear all logs from the database
    PruneLogs,
}

pub async fn execute(cmd: DataCmd) -> Result<(), String> {
    let db = get_cli_db().await?;

    match cmd {
        DataCmd::Reindex { id } => {
            println!("Re-indexing collection {}...", id);
            db.reindex_collection(id).await.map_err(|e| e.to_string())?;
            println!("✅ Re-indexing complete.");
            Ok(())
        }
        DataCmd::Export { id, pretty } => {
            let opts = QueryOptions {
                limit: Some(100000),
                ..Default::default()
            };
            let result = db.list_records(id, opts).await.map_err(|e| e.to_string())?;

            let output = if pretty {
                serde_json::to_string_pretty(&result.items).unwrap()
            } else {
                serde_json::to_string(&result.items).unwrap()
            };

            std::io::stdout().write_all(output.as_bytes()).unwrap();
            Ok(())
        }
        DataCmd::PruneLogs => {
            // Raw execution to clear the log tables safely
            println!("Pruning log files and databases...");

            // Execute SQL manually to clean tables
            // To do this strictly via traits, you'd need a clear_logs() method,
            // or you can manipulate the file system directly to delete old rolling log DBs.
            let path = std::path::Path::new("storage/system/logs.db");
            if path.exists() {
                let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
                conn.execute_batch("DELETE FROM _system_logs; DELETE FROM _audit_logs; VACUUM;")
                    .map_err(|e| e.to_string())?;
            }

            println!("✅ Log files pruned.");
            Ok(())
        }
    }
}
