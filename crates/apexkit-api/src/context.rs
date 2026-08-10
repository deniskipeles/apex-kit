use crate::AppState;
use crate::workspaces_manager::sandbox as sandbox_manager;
use crate::workspaces_manager::sandbox::SandboxManager;
use crate::workspaces_manager::tenant::TenantManager;
use apexkit_core::realtime::EventScope;
use apexkit_core::workers::JobContext;
use apexkit_core::{Db, VectorProvider, security::vault::Vault, storage::StorageBackend};
use std::sync::Arc;

// --- JOB CONTEXT ---
fn get_storage_path(subpath: &str) -> String {
    if let Ok(base) = std::env::var("APEXKIT_MOUNTED_FILE_STORAGE") {
        let clean_base = base.trim_end_matches('/');
        let clean_sub = subpath.trim_start_matches('/');
        format!("{}/{}", clean_base, clean_sub)
    } else {
        subpath.to_string()
    }
}

pub struct GlobalJobContext {
    pub root_db: Arc<dyn Db>,
    pub root_vector_provider: Arc<dyn VectorProvider>,
    pub tenant_manager: Arc<TenantManager>,
    pub sandbox_manager: Arc<SandboxManager>,
    pub vault: Arc<Vault>,
}

#[async_trait::async_trait]
impl JobContext for GlobalJobContext {
    async fn resolve(
        &self,
        scope_id: Option<&str>,
    ) -> Option<(Arc<dyn Db>, Arc<dyn VectorProvider>)> {
        match scope_id {
            Some(id) => {
                if let Ok(ctx) = self.tenant_manager.get_tenant_context(id).await {
                    return Some((ctx.db, ctx.vector_provider));
                }
                if let Ok(db) = self.sandbox_manager.get_sandbox(id).await
                    && let Some(prov) = self.sandbox_manager.get_vector_provider(id).await
                {
                    return Some((db, prov));
                }
                None
            }
            None => Some((self.root_db.clone(), self.root_vector_provider.clone())),
        }
    }

    async fn get_file_bytes(
        &self,
        tenant_id: Option<&str>,
        filename: &str,
    ) -> Result<Vec<u8>, String> {
        let (db, scope, base_dir) = match tenant_id {
            Some(id) if id.starts_with("session_") => {
                let sid = id.trim_start_matches("session_");
                let db = self.sandbox_manager.get_sandbox(sid).await?;
                (
                    db,
                    EventScope::Sandbox(sid.to_string()),
                    format!("storage/sandboxes/session_{}/uploads", sid),
                )
            }
            Some(id) => {
                let db = self.tenant_manager.get_tenant(id.to_string()).await?;
                (
                    db,
                    EventScope::Tenant(id.to_string()),
                    format!("storage/tenants/{}/uploads", id),
                )
            }
            None => (
                self.root_db.clone(),
                EventScope::Root,
                "storage/system/uploads".to_string(),
            ),
        };

        // 1. Check if DB has S3 configured
        let config_val = db.get_config("storage").await.map_err(|e| e.to_string())?;
        if let Some(val) = config_val {
            let conf: crate::system::dto::StorageConfigDto =
                serde_json::from_value(val).unwrap_or_default();
            if conf.active_driver == "s3" && conf.s3.enabled {
                let secret_key = if let Some(enc_str) = conf.s3.secret_key {
                    if let Ok(enc) = serde_json::from_str::<
                        apexkit_core::security::vault::EncryptedValue,
                    >(&enc_str)
                    {
                        self.vault.decrypt(&enc).unwrap_or_default()
                    } else {
                        enc_str
                    }
                } else {
                    String::new()
                };

                // If a Tenant or Sandbox configures their own S3 bucket, they own the root of it.
                // No isolation prefix is needed.
                let isolation_prefix = match scope {
                    EventScope::Root => "__root_app__/".to_string(),
                    _ => "".to_string(),
                };

                let s3 = apexkit_core::storage::S3Storage::new_with_creds(
                    &conf.s3.bucket,
                    &conf.s3.region,
                    &conf.s3.endpoint,
                    "",
                    &conf.s3.access_key,
                    &secret_key,
                    &isolation_prefix,
                )
                .await;

                return s3.get(filename).await.map_err(|e| e.to_string());
            }
        }

        // 2. Check Root S3 (if Tenant/Sandbox fallback)
        if !matches!(scope, EventScope::Root) {
            let root_val = self
                .root_db
                .get_config("storage")
                .await
                .map_err(|e| e.to_string())?;
            if let Some(val) = root_val {
                let conf: crate::system::dto::StorageConfigDto =
                    serde_json::from_value(val).unwrap_or_default();
                if conf.active_driver == "s3" && conf.s3.enabled {
                    let secret_key = if let Some(enc_str) = conf.s3.secret_key {
                        if let Ok(enc) = serde_json::from_str::<
                            apexkit_core::security::vault::EncryptedValue,
                        >(&enc_str)
                        {
                            self.vault.decrypt(&enc).unwrap_or_default()
                        } else {
                            enc_str
                        }
                    } else {
                        String::new()
                    };

                    let isolation_prefix = match scope {
                        EventScope::Tenant(ref id) => format!("tenants/{}/uploads/", id),
                        EventScope::Sandbox(ref id) => format!("sandboxes/session_{}/uploads/", id),
                        _ => "".to_string(),
                    };

                    let s3 = apexkit_core::storage::S3Storage::new_with_creds(
                        &conf.s3.bucket,
                        &conf.s3.region,
                        &conf.s3.endpoint,
                        "",
                        &conf.s3.access_key,
                        &secret_key,
                        &isolation_prefix,
                    )
                    .await;

                    return s3.get(filename).await.map_err(|e| e.to_string());
                }
            }
        }

        // 3. Fallback to Local Storage
        let fs_root = get_storage_path(&base_dir);
        let local = apexkit_core::storage::LocalStorage::new(&fs_root, "").await;
        local.get(filename).await.map_err(|e| e.to_string())
    }
}

// [NEW] Wrapper to enforce scoping
pub struct ScopedScriptContext {
    pub state: AppState,
    pub scope: EventScope,
}

impl ScopedScriptContext {
    fn _prefix_key(&self, key: &str) -> String {
        match &self.scope {
            EventScope::Root => format!("root:{}", key), // Root gets its own namespace
            EventScope::Tenant(id) => format!("tenant:{}:{}", id, key),
            EventScope::Sandbox(id) => format!("sandbox:{}:{}", id, key),
            _ => format!("global:{}", key),
        }
    }

    fn _get_default_ttl(&self) -> u64 {
        std::env::var("CACHE_TTL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300) // Default 5 minutes
    }
}
use crate::api::storage::ScopedDynamicStorage;
// Implement the trait for AppState
impl apexkit_core::ScriptContext for ScopedScriptContext {
    fn get_db(&self) -> Arc<dyn Db> {
        self.state.db.clone()
    }

    fn get_vault(&self) -> Arc<Vault> {
        self.state.vault.clone()
    }

    fn get_embedder(&self) -> Arc<apexkit_core::embeddings::EmbedderService> {
        self.state.embedder.clone()
    }

    fn get_vector_provider(&self) -> Arc<dyn VectorProvider> {
        self.state.vector_provider.clone()
    }

    fn get_realtime_tx(&self) -> tokio::sync::broadcast::Sender<apexkit_core::realtime::DbEvent> {
        self.state.tx.clone()
    }

    fn get_storage(&self) -> Arc<dyn StorageBackend> {
        Arc::new(ScopedDynamicStorage::new(
            self.state.clone(),
            self.scope.clone(),
        ))
    }

    fn get_scope(&self) -> EventScope {
        self.scope.clone()
    }

    fn get_port(&self) -> u16 {
        self.state.port
    }

    fn get_scoped_vector_provider(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Arc<dyn VectorProvider>> + Send>> {
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_vp = self.state.vector_provider.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            match scope {
                EventScope::Root => root_vp,
                EventScope::Tenant(id) => {
                    if let Ok(ctx) = tm.get_tenant_context(&id).await {
                        ctx.vector_provider
                    } else {
                        root_vp
                    }
                }
                EventScope::Sandbox(id) => {
                    // Requires get_sandbox_context to be accessible/implemented in SandboxManager
                    if let Ok(ctx) = sm.get_sandbox_context(&id).await {
                        ctx.vector_provider
                    } else {
                        root_vp
                    }
                }
                _ => root_vp,
            }
        })
    }

    fn check_quota(
        &self,
        metric: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let state = self.state.clone();
        let scope = self.scope.clone();
        let metric_str = metric.to_string();

        Box::pin(async move {
            if metric_str == "ai" {
                match crate::utils::check_ai_quota(&state, &scope).await {
                    Ok(_) => Ok(()),
                    Err(e) => Err(e.to_string()),
                }
            } else {
                Ok(())
            }
        })
    }

    fn get_shared_script(
        &self,
        name: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Option<apexkit_core::models::script::Script>> + Send>,
    > {
        let db = self.state.db.clone(); // Root DB
        let n = name.to_string();
        Box::pin(async move { db.get_script_by_name(&n).await.ok().flatten() })
    }

    fn execute_shared_script(
        &self,
        code: String,
        payload: serde_json::Value,
        scope: EventScope,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<serde_json::Value, String>> + Send,
        >,
    > {
        let engine = self.state.script_engine.clone();
        let state = self.state.clone();

        // When executing a shared script, the context MUST have the correct scope.
        let new_ctx = Arc::new(ScopedScriptContext {
            state: state.clone(),
            scope: scope.clone(),
        });

        Box::pin(async move {
            engine
                .run_script(&code, payload, new_ctx, None, None, None, None)
                .await
        })
    }

    // Dynamic Resolution for Tenant Switching
    fn resolve_tenant_db(
        &self,
        tenant_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>> {
        let tm = self.state.tenant_manager.clone();
        let tid = tenant_id.to_string();
        Box::pin(async move { tm.get_tenant(tid).await.ok() })
    }

    // Dynamic Resolution for Sandbox Switching
    fn resolve_sandbox_db(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn Db>>> + Send>> {
        let sm = self.state.sandbox_manager.clone();
        let sid = session_id.to_string();
        Box::pin(async move { sm.get_sandbox(&sid).await.ok() })
    }

    fn admin_create_tenant(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let tm = self.state.tenant_manager.clone();
        Box::pin(async move { tm.create_tenant(id).await.map(|_| ()) })
    }

    fn admin_update_tenant(
        &self,
        id: String,
        updates: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let db = self.state.db.clone();
        let tm = self.state.tenant_manager.clone();
        Box::pin(async move {
            let name = updates
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from);
            let status = updates
                .get("status")
                .and_then(|v| v.as_str())
                .map(String::from);
            let tier = updates
                .get("tier")
                .and_then(|v| v.as_str())
                .map(String::from);

            let max_vectors = updates.get("max_vectors").and_then(|v| v.as_i64());
            // --- NEW ---
            let max_storage_mb = updates.get("max_storage_mb").and_then(|v| v.as_i64());
            let max_ai_requests = updates.get("max_ai_requests").and_then(|v| v.as_i64());

            // 1. Update Metadata
            db.update_tenant_full(
                &id,
                name,
                status,
                tier,
                max_vectors,
                max_storage_mb,
                max_ai_requests,
            )
            .await
            .map_err(|e| e.to_string())?;

            // 2. Invalidate Cache so new status/settings take effect immediately
            tm.invalidate(&id).await;

            Ok(())
        })
    }

    fn admin_delete_tenant(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let db = self.state.db.clone();
        let tm = self.state.tenant_manager.clone();
        Box::pin(async move {
            // 1. Delete Metadata
            db.delete_tenant_metadata(&id)
                .await
                .map_err(|e| e.to_string())?;
            // 2. Delete Files & Cache
            tm.delete_tenant(&id).await?;
            Ok(())
        })
    }

    fn admin_get_tenant_usage(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, String>> + Send>> {
        let db = self.state.db.clone(); // Use Root DB (which has the logic in ApexKit impl)
        Box::pin(async move {
            db.get_tenant_disk_usage(&id)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn admin_create_sandbox(
        &self,
        id: String,
        config: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let sm = self.state.sandbox_manager.clone();
        let db = self.state.db.clone();
        let scope = self.scope.clone();
        
        Box::pin(async move {
            let general_config = db.get_config("general").await.unwrap_or_default();
            let max_storage_mb = general_config
                .as_ref()
                .and_then(|v| v.get("max_sandbox_storage_mb").and_then(|n| n.as_i64()))
                .unwrap_or(100);

            // Parse JavaScript configuration object into Rust CloneStrategy
            let strategy = if let Some(obj) = config.as_object() {
                let strat_str = obj.get("clone_strategy").and_then(|v| v.as_str()).unwrap_or("none");
                match strat_str {
                    "schema" => sandbox_manager::CloneStrategy::SchemaOnly,
                    "partial" => sandbox_manager::CloneStrategy::Partial(
                        obj.get("clone_record_limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize
                    ),
                    "full" => sandbox_manager::CloneStrategy::Full,
                    "selected" => {
                        let parse_arr = |key: &str| -> Vec<String> {
                            obj.get(key)
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.iter().filter_map(|i| i.as_str().map(|s| s.to_string())).collect())
                                .unwrap_or_default()
                        };
                        sandbox_manager::CloneStrategy::Selected {
                            collections: parse_arr("collections"),
                            scripts: parse_arr("scripts"),
                            templates: parse_arr("templates"),
                            record_limit: obj.get("clone_record_limit").and_then(|v| v.as_u64()).map(|n| n as usize),
                        }
                    },
                    "none" => sandbox_manager::CloneStrategy::None,
                    _ => sandbox_manager::CloneStrategy::None,
                }
            } else {
                sandbox_manager::CloneStrategy::None
            };

            sm.create_sandbox(
                &id,
                strategy,
                db,
                scope,
                max_storage_mb,
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
        })
    }

    fn admin_update_sandbox(
        &self,
        id: String,
        updates: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let db = self.state.db.clone();
        let sm = self.state.sandbox_manager.clone();
        Box::pin(async move {
            let name = updates
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from);
            let status = updates
                .get("status")
                .and_then(|v| v.as_str())
                .map(String::from);
            let expires_at = updates
                .get("expires_at")
                .and_then(|v| v.as_str())
                .map(String::from);

            // 1. Update Metadata
            db.update_sandbox_full(&id, name, status, expires_at)
                .await
                .map_err(|e| e.to_string())?;

            // 2. Invalidate Cache
            // (Sandbox manager doesn't strictly check DB status on load like Tenant manager does, but good practice)
            sm.cleanup_sandbox(&id); // Warning: cleanup deletes files. We just want to invalidate cache. 
            // Since sandbox manager is ephemeral, standard eviction handles updates mostly.
            // But if we want to force status check, we might need an `invalidate_cache` method on SandboxManager too.
            // For now, metadata update is sufficient for listing visibility.
            Ok(())
        })
    }

    fn admin_delete_sandbox(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let db = self.state.db.clone();
        let sm = self.state.sandbox_manager.clone();
        Box::pin(async move {
            db.delete_sandbox_metadata(&id)
                .await
                .map_err(|e| e.to_string())?;
            sm.cleanup_sandbox(&id); // Deletes files & cache
            Ok(())
        })
    }

    fn admin_get_sandbox_usage(
        &self,
        id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, String>> + Send>> {
        let db = self.state.db.clone();
        Box::pin(async move {
            db.get_sandbox_disk_usage(&id)
                .await
                .map_err(|e| e.to_string())
        })
    }

    // [UPDATED] Cache Methods
    fn cache_get(
        &self,
        key: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>> {
        let key = key.to_string();
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_cache = self.state.root_script_cache.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            match scope {
                EventScope::Tenant(id) => {
                    if let Ok(ctx) = tm.get_tenant_context(&id).await {
                        return ctx.script_cache.get(&key).await;
                    }
                    None
                }
                EventScope::Sandbox(id) => {
                    if let Ok(ctx) = sm.get_sandbox_context(&id).await {
                        // Need to expose get_sandbox_context
                        return ctx.script_cache.get(&key).await;
                    }
                    None
                }
                _ => root_cache.get(&key).await,
            }
        })
    }

    fn cache_set(
        &self,
        key: &str,
        val: &str,
        _ttl: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let key = key.to_string();
        let val = val.to_string();
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_cache = self.state.root_script_cache.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            match scope {
                EventScope::Tenant(id) => {
                    if let Ok(ctx) = tm.get_tenant_context(&id).await {
                        ctx.script_cache.insert(key, val).await;
                    }
                }
                EventScope::Sandbox(id) => {
                    if let Ok(ctx) = sm.get_sandbox_context(&id).await {
                        ctx.script_cache.insert(key, val).await;
                    }
                }
                _ => {
                    root_cache.insert(key, val).await;
                }
            }
        })
    }

    // For incr, you need read-modify-write on the specific cache instance.
    fn cache_incr(
        &self,
        key: &str,
        delta: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = i64> + Send>> {
        let key = key.to_string();
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_cache = self.state.root_script_cache.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            let cache = match scope {
                EventScope::Tenant(id) => tm
                    .get_tenant_context(&id)
                    .await
                    .ok()
                    .map(|c| c.script_cache),
                EventScope::Sandbox(id) => sm
                    .get_sandbox_context(&id)
                    .await
                    .ok()
                    .map(|c| c.script_cache),
                _ => Some(root_cache),
            };

            if let Some(c) = cache {
                let current_str = c.get(&key).await;
                let current_val = current_str.and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                let new_val = current_val + delta;
                c.insert(key, new_val.to_string()).await;
                new_val
            } else {
                0
            }
        })
    }

    fn cache_del(
        &self,
        key: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let key = key.to_string();
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_cache = self.state.root_script_cache.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            match scope {
                EventScope::Tenant(id) => {
                    if let Ok(ctx) = tm.get_tenant_context(&id).await {
                        ctx.script_cache.invalidate(&key).await;
                    }
                }
                EventScope::Sandbox(id) => {
                    if let Ok(ctx) = sm.get_sandbox_context(&id).await {
                        ctx.script_cache.invalidate(&key).await;
                    }
                }
                _ => {
                    root_cache.invalidate(&key).await;
                }
            }
        })
    }

    // Implementation for listing keys
    fn cache_list_keys(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<String>> + Send>> {
        let tm = self.state.tenant_manager.clone();
        let sm = self.state.sandbox_manager.clone();
        let root_cache = self.state.root_script_cache.clone();
        let scope = self.scope.clone();

        Box::pin(async move {
            let cache = match scope {
                EventScope::Tenant(id) => tm
                    .get_tenant_context(&id)
                    .await
                    .ok()
                    .map(|c| c.script_cache),
                EventScope::Sandbox(id) => sm
                    .get_sandbox_context(&id)
                    .await
                    .ok()
                    .map(|c| c.script_cache),
                _ => Some(root_cache),
            };

            if let Some(c) = cache {
                // moka::future::Cache::iter() is synchronous and returns an iterator over the keys
                c.iter().map(|(k, _)| k.as_ref().clone()).collect()
            } else {
                vec![]
            }
        })
    }
}
