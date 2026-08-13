pub mod backup;
pub mod data;
pub mod user;
pub mod wasm;

use clap::Subcommand;
use std::sync::Arc;

use apexkit_core::database::sqlite::connections::a_new_database_connection;
use apexkit_core::database::traits::{Db, VectorProvider};
use async_trait::async_trait;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// User Management
    #[command(subcommand)]
    User(user::UserCmd),

    /// Data Operations (Import/Export/Maintenance)
    #[command(subcommand)]
    Data(data::DataCmd),

    /// Manage, validate, and cache WASM/WASI binaries
    Wasm {
        /// Download, sanitize, precompile, and cache a WASM binary from a public URL
        #[arg(long, value_name = "URL")]
        get: Option<String>,

        /// Optional custom readable name for symlinking (e.g. ffmpeg.wasm)
        #[arg(short, long)]
        name: Option<String>,

        /// List all cached WASM binaries in .cache/wasm
        #[arg(short, long)]
        list: bool,
    },

    /// Create a full or partial backup of the system
    Backup {
        #[arg(long, default_missing_value = "default", num_args = 0..=1)]
        root: Option<String>,

        #[arg(long)]
        tenants: Option<String>,

        #[arg(short, long)]
        out: Option<String>,
    },

    /// Restore a backup from an archive
    Restore {
        file: String,

        #[arg(short, long, action)]
        yes: bool,
    },
}

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
        async fn embed_text_for_image_search(
            &self,
            _t: &str,
        ) -> std::result::Result<Vec<f32>, String> {
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
        Commands::Wasm { get, name, list } => wasm::execute(get, name, list).await,
        Commands::Backup { root, tenants, out } => backup::handle_backup(root, tenants, out).await,
        Commands::Restore { file, yes } => backup::handle_restore(file, yes).await,
    }
}
