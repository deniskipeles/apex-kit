pub mod backup;
pub mod data;
pub mod user;

use clap::Subcommand;
use std::sync::Arc;

use apexkit_core::database::sqlite::connections::a_new_database_connection;
use apexkit_core::database::traits::{Db, VectorProvider};
use async_trait::async_trait; // <--- ADD THIS IMPORT

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// User Management
    #[command(subcommand)]
    User(user::UserCmd),

    /// Data Operations (Import/Export/Maintenance)
    #[command(subcommand)]
    Data(data::DataCmd),

    /// Create a full or partial backup of the system
    Backup {
        /// Backup root data. Optional items: vectors.db, logs.db, indexes, or *
        #[arg(long, default_missing_value = "default", num_args = 0..=1)]
        root: Option<String>,

        /// Backup specific tenants. Format: app-0,app-1(*),app-2(vectors.db)
        #[arg(long)]
        tenants: Option<String>,

        /// Custom output file path (e.g., my_backup.tar.gz)
        #[arg(short, long)]
        out: Option<String>,
    },

    /// Restore a backup from an archive
    Restore {
        /// Path to the archive (.tar.gz)
        file: String,

        /// Bypass the interactive confirmation prompt
        #[arg(short, long, action)]
        yes: bool,
    },
}

/// Helper to get a raw database connection purely for CLI tasks without booting the whole server
pub async fn get_cli_db() -> Result<Arc<dyn Db>, String> {
    struct CliVectorProvider;

    #[async_trait]
    impl VectorProvider for CliVectorProvider {
        async fn embed(&self, _t: &str) -> std::result::Result<Vec<f32>, String> {
            Ok(vec![])
        }
        async fn embed_image(&self, _i: &str) -> std::result::Result<Vec<f32>, String> {
            Ok(vec![])
        }
        async fn search(
            &self,
            _c: i64,
            _f: &str,
            _v: &[f32],
            _l: usize,
        ) -> std::result::Result<Vec<(i64, f32)>, String> {
            Ok(vec![])
        }
        async fn index(
            &self,
            _c: i64,
            _r: i64,
            _f: &str,
            _v: &[f32],
        ) -> std::result::Result<(), String> {
            Ok(())
        }
    }

    let raw_db = a_new_database_connection(Arc::new(CliVectorProvider), None, None)
        .await
        .map_err(|e| e.to_string())?;

    Ok(Arc::new(raw_db))
}

pub async fn execute(command: Commands) -> Result<(), String> {
    match command {
        Commands::User(cmd) => user::execute(cmd).await,
        Commands::Data(cmd) => data::execute(cmd).await,
        Commands::Backup { root, tenants, out } => backup::handle_backup(root, tenants, out).await,
        Commands::Restore { file, yes } => backup::handle_restore(file, yes).await,
    }
}
