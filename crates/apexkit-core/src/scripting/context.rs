use std::cell::RefCell;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::database::traits::{Db, VectorProvider};
use crate::embeddings::EmbedderService;
use crate::realtime::types::{DbEvent, EventScope};
use crate::security::vault::Vault;
use crate::storage::traits::StorageBackend;

pub type ActiveScriptContextTuple = (
    Arc<dyn ScriptContext>,
    tokio::runtime::Handle,
    Option<String>,
    Option<broadcast::Sender<DbEvent>>,
    EventScope,
);

thread_local! {
    pub static ACTIVE_CONTEXT: RefCell<Option<ActiveScriptContextTuple>> = RefCell::new(None);
}

#[allow(clippy::type_complexity)]
pub trait ScriptContext: Send + Sync {
    fn get_db(&self) -> Arc<dyn Db>;
    fn get_vault(&self) -> Arc<Vault>;
    fn get_embedder(&self) -> Arc<EmbedderService>;
    fn get_vector_provider(&self) -> Arc<dyn VectorProvider>;
    fn get_realtime_tx(&self) -> tokio::sync::broadcast::Sender<crate::realtime::DbEvent>;
    fn get_storage(&self) -> Arc<dyn StorageBackend>;
    fn get_scope(&self) -> EventScope;
    fn get_scoped_vector_provider(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Arc<dyn VectorProvider>> + Send>>;
    fn get_shared_script(
        &self,
        name: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<crate::models::Script>> + Send>>;
    fn execute_shared_script(
        &self,
        code: String,
        payload: serde_json::Value,
        scope: crate::realtime::EventScope,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<serde_json::Value, String>> + Send,
        >,
    >;
    fn resolve_tenant_db(
        &self,
        tenant_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>>;
    fn resolve_sandbox_db(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>>;
    fn admin_create_tenant(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send>>;
    fn admin_update_tenant(
        &self,
        id: String,
        updates: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send>>;
    fn admin_delete_tenant(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send>>;
    fn admin_get_tenant_usage(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<u64, String>> + Send>>;
    fn admin_create_sandbox(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send>>;
    fn admin_update_sandbox(
        &self,
        id: String,
        updates: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send>>;
    fn admin_delete_sandbox(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send>>;
    fn admin_get_sandbox_usage(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<u64, String>> + Send>>;
    fn cache_get(
        &self,
        key: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>>;
    fn cache_set(
        &self,
        key: &str,
        val: &str,
        ttl_secs: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;
    fn cache_del(
        &self,
        key: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;
    fn cache_incr(
        &self,
        key: &str,
        delta: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = i64> + Send>>;
    fn cache_list_keys(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<String>> + Send>>;
}
