use crate::workspaces_manager::sandbox::SandboxManager;
use crate::workspaces_manager::tenant::TenantManager;
use apexkit_core::scripting::ScriptEngine;
use apexkit_core::{
    Db, VectorProvider, realtime::DbEvent, security::vault::Vault, storage::StorageBackend,
    workers::JobQueue,
};
use async_graphql::dynamic::Schema;
use governor::{RateLimiter, clock::DefaultClock, state::keyed::DefaultKeyedStateStore};
use metrics_exporter_prometheus::PrometheusHandle;
use moka::future::Cache;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

// 1. Define the specific type for Governor's keyed rate limiter
pub type DynRateLimiter = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<dyn Db>,
    pub tenant_manager: Arc<TenantManager>,
    pub sandbox_manager: Arc<SandboxManager>,
    pub queue: JobQueue,
    pub metrics: Option<PrometheusHandle>,
    pub tx: broadcast::Sender<DbEvent>,
    pub storage: Arc<dyn StorageBackend>,
    pub vault: Arc<Vault>,
    pub schema: Arc<RwLock<Schema>>,
    pub scheduler: Arc<RwLock<crate::system::scheduler::SchedulerService>>,
    pub script_engine: Arc<ScriptEngine>,
    pub css_cache: Arc<RwLock<String>>,
    pub thumb_cache: Cache<String, Arc<Vec<u8>>>,
    pub embedder: Arc<apexkit_core::embeddings::EmbedderService>,
    pub vector_provider: Arc<dyn VectorProvider>,
    pub port: u16,
    // Script Cache (Key -> Value)
    // We use String values to store JSON or Numbers (parsed on retrieval)
    // [RENAMED] Only for Root scope
    pub root_script_cache: Cache<String, String>,
    // [NEW] Track record counts per collection to trigger milestone auto-reindexing
    pub record_count_cache: Cache<String, i64>,
    // [NEW] Stores the active GCRA RateLimiters based on the quota limit
    pub rate_limiters: Cache<u64, Arc<DynRateLimiter>>,
}
