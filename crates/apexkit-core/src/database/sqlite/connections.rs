use crate::database::traits::{Db, VectorProvider};
use crate::models::ChangesetEvent;
use crate::search::SearchManager;
use crate::{batching, embeddings};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

// Declare submodules
pub mod ai;
pub mod api_keys;
pub mod audit;
pub mod collections;
pub mod configs;
pub mod control;
pub mod dashboard;
pub mod files;
pub mod plugins;
pub mod queries;
pub mod records;
pub mod relations;
pub mod sandboxes;
pub mod scripts;
pub mod search;
pub mod templates;
pub mod tenants;
pub mod users;
pub mod vectors;

// Re-export implementations
pub use super::setup::a_new_database_connection;
// pub use ai::*;
// pub use api_keys::*;
// pub use audit::*;
// pub use collections::*;
// pub use configs::*;
// pub use control::*;
// pub use dashboard::*;
// pub use files::*;
// pub use plugins::*;
// pub use queries::*;
// pub use records::*;
// pub use relations::*;
// pub use sandboxes::*;
// pub use scripts::*;
// pub use search::*;
// pub use templates::*;
// pub use tenants::*;
// pub use users::*;
// pub use vectors::*;

/// The main SQLite orchestrator that coordinates separate, isolated databases
/// (core, data, log, system, and vectors) under a unified concurrency manager.
#[derive(Clone)]
pub struct ApexKit {
    pub(crate) base_path: String,
    pub(crate) hot_conn_core: Arc<Mutex<Connection>>,
    pub(crate) hot_conn_data: Arc<Mutex<Connection>>,
    pub(crate) hot_conn_log: Arc<Mutex<Connection>>,
    pub(crate) hot_conn_sys: Arc<Mutex<Connection>>,
    pub(crate) hot_conn_vec: Arc<Mutex<Connection>>,

    pub(crate) data_batcher: batching::WriteManager,
    pub(crate) log_batcher: batching::WriteManager,
    pub(crate) vector_batcher: batching::WriteManager,
    pub(crate) core_batcher: batching::WriteManager,
    pub(crate) sys_batcher: batching::WriteManager,

    pub(crate) search: Arc<SearchManager>,
    pub embedder: Arc<embeddings::EmbedderService>,
    pub vector_provider: Arc<dyn VectorProvider>,
}

impl ApexKit {
    /// Creates a new `ApexKit` instance and initializes batching write-forwarders.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_path: &str,
        core: Connection,
        data: Connection,
        log: Connection,
        sys: Connection,
        vec: Connection,
        vector_provider: Arc<dyn VectorProvider>,
        forwarder: Option<Arc<dyn crate::batching::WriteForwarder>>,
        event_tx: Option<tokio::sync::broadcast::Sender<ChangesetEvent>>,
        scope: String,
    ) -> Self {
        let hot_conn_core = Arc::new(Mutex::new(core));
        let hot_conn_data = Arc::new(Mutex::new(data));
        let hot_conn_log = Arc::new(Mutex::new(log));
        let hot_conn_sys = Arc::new(Mutex::new(sys));
        let hot_conn_vec = Arc::new(Mutex::new(vec));

        let data_batcher = batching::WriteManager::new(
            format!("{}/data.db", base_path),
            hot_conn_data.clone(),
            forwarder.clone(),
            event_tx.clone(),
            scope.clone(),
            "data".to_string(),
        );
        let log_batcher = batching::WriteManager::new(
            format!("{}/logs.db", base_path),
            hot_conn_log.clone(),
            forwarder.clone(),
            event_tx.clone(),
            scope.clone(),
            "logs".to_string(),
        );
        let vector_batcher = batching::WriteManager::new(
            format!("{}/vectors.db", base_path),
            hot_conn_vec.clone(),
            forwarder.clone(),
            event_tx.clone(),
            scope.clone(),
            "vectors".to_string(),
        );
        let core_batcher = batching::WriteManager::new(
            format!("{}/core.db", base_path),
            hot_conn_core.clone(),
            forwarder.clone(),
            event_tx.clone(),
            scope.clone(),
            "core".to_string(),
        );
        let sys_batcher = batching::WriteManager::new(
            format!("{}/system.db", base_path),
            hot_conn_sys.clone(),
            forwarder.clone(),
            event_tx.clone(),
            scope.clone(),
            "system".to_string(),
        );

        Self {
            base_path: base_path.to_string(),
            hot_conn_core,
            hot_conn_data,
            hot_conn_log,
            hot_conn_sys,
            hot_conn_vec,
            vector_provider,
            data_batcher,
            log_batcher,
            vector_batcher,
            core_batcher,
            sys_batcher,
            search: Arc::new(SearchManager::new(&format!("{}/indexes", base_path))),
            embedder: Arc::new(embeddings::EmbedderService::new()),
        }
    }

    /// Initializes folders, opens connections, and applies baseline optimizations/PRAGMAs.
    pub async fn init_filesystem(
        base_path: &str,
        vector_provider: Arc<dyn VectorProvider>,
        forwarder: Option<Arc<dyn crate::batching::WriteForwarder>>,
        event_tx: Option<tokio::sync::broadcast::Sender<ChangesetEvent>>,
        scope: String,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if !Path::new(base_path).exists() {
            std::fs::create_dir_all(base_path)?;
        }

        let core = Connection::open(format!("{}/core.db", base_path))?;
        let data = Connection::open(format!("{}/data.db", base_path))?;
        let log = Connection::open(format!("{}/logs.db", base_path))?;
        let sys = Connection::open(format!("{}/system.db", base_path))?;
        let vec = Connection::open(format!("{}/vectors.db", base_path))?;

        super::setup::apply_pragmas(&core)?;
        super::setup::apply_pragmas(&data)?;
        super::setup::apply_pragmas(&log)?;
        super::setup::apply_pragmas(&sys)?;
        super::setup::apply_pragmas(&vec)?;

        super::setup::setup_core(&core)?;
        super::setup::setup_data(&data)?;
        super::setup::setup_logs(&log)?;
        super::setup::setup_sys(&sys)?;
        super::setup::setup_vectors(&vec)?;

        let instance = Self::new(
            base_path,
            core,
            data,
            log,
            sys,
            vec,
            vector_provider,
            forwarder,
            event_tx,
            scope,
        );

        instance
            .get_core_read()
            .await
            .execute_batch("PRAGMA busy_timeout = 5000;")?;
        instance
            .get_data_read()
            .await
            .execute_batch("PRAGMA busy_timeout = 5000;")?;
        instance
            .get_log_read()
            .await
            .execute_batch("PRAGMA busy_timeout = 5000;")?;
        instance
            .get_sys_read()
            .await
            .execute_batch("PRAGMA busy_timeout = 5000;")?;
        instance
            .get_vector_read()
            .await
            .execute_batch("PRAGMA busy_timeout = 5000;")?;

        Ok(instance)
    }

    /// Sets an alternative search index coordinator manager.
    pub fn set_search_manager(&mut self, manager: Arc<SearchManager>) {
        self.search = manager;
    }

    // --- Active connection accessor locks ---
    pub async fn get_core_read<'a>(&'a self) -> MutexGuard<'a, Connection> {
        self.hot_conn_core.lock().await
    }
    pub async fn get_data_read<'a>(&'a self) -> MutexGuard<'a, Connection> {
        self.hot_conn_data.lock().await
    }
    pub async fn get_log_read<'a>(&'a self) -> MutexGuard<'a, Connection> {
        self.hot_conn_log.lock().await
    }
    pub async fn get_sys_read<'a>(&'a self) -> MutexGuard<'a, Connection> {
        self.hot_conn_sys.lock().await
    }
    pub async fn get_vector_read<'a>(&'a self) -> MutexGuard<'a, Connection> {
        self.hot_conn_vec.lock().await
    }

    /// Assures search indexes are loaded and prepared for execution.
    pub async fn ensure_search_index(
        &self,
        collection_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::database::traits::CollectionStore;
        if let Some(col) = self.get_collection(collection_id).await? {
            if let Some(schema) = &col.schema {
                if schema.fields.values().any(|f| f.ose_indexed) {
                    self.search.load_index(collection_id, schema)?;
                }
            }
        }
        Ok(())
    }
}

/// Dynamic composition implementing `Db` for types implementing all 22 required supertraits.
#[async_trait::async_trait]
impl Db for ApexKit {}
