use crate::{Db, Collection, Record, schema::CollectionSchema, query::QueryOptions, auth::User, models, ai_models};
use async_trait::async_trait;
use moka::future::Cache;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use crate::ai_models::{AiSession, Plugin};
use crate::ListResult;
use std::error::Error as StdError;
use crate::ApiKey;

#[derive(Clone)]
pub struct CachedDb {
    inner: Arc<dyn Db>,
    collection_cache: Cache<i64, Arc<Collection>>,
    // Key: (collection_id, record_id)
    record_cache: Cache<(i64, i64), Arc<Record>>, 
    // Cache templates by slug
    template_cache: Cache<String, Arc<models::Template>>, 
}

impl CachedDb {
    pub fn new(inner: Arc<dyn Db>) -> Self {
        Self {
            inner,
            // Cache collections for 1 hour, they rarely change
            collection_cache: Cache::builder()
                .time_to_live(Duration::from_secs(3600))
                .build(),
            // Cache records for 5 minutes (max 10k items)
            record_cache: Cache::builder()
                .time_to_live(Duration::from_secs(300))
                .max_capacity(10_000) 
                .build(),
            
            template_cache: Cache::builder()
                .time_to_live(Duration::from_secs(300))
                .build(),
        }
    }
}

#[async_trait]
impl Db for CachedDb {
    // --- Collections ---

    async fn create_collection(
        &self,
        name: &str,
        schema: &Option<CollectionSchema>,
        index: Option<String>,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_collection(name, schema, index).await
    }

    async fn get_collection(
        &self,
        id: i64,
    ) -> Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Try Cache
        if let Some(cached) = self.collection_cache.get(&id).await {
            return Ok(Some((*cached).clone()));
        }

        // 2. Fetch from DB
        let result = self.inner.get_collection(id).await?;
        
        // 3. Populate Cache
        if let Some(col) = &result {
            self.collection_cache.insert(id, Arc::new(col.clone())).await;
        }
        Ok(result)
    }

    async fn list_collections(&self) -> Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_collections().await
    }

    async fn update_collection(
        &self,
        id: i64,
        name: Option<String>,
        schema: Option<CollectionSchema>,
    ) -> Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
        let res = self.inner.update_collection(id, name, schema).await?;
        // Invalidate cache
        self.collection_cache.invalidate(&id).await;
        Ok(res)
    }

    // FIX: Updated return type to match trait
    async fn delete_collection(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_collection(id).await?;
        self.collection_cache.invalidate(&id).await;
        Ok(())
    }

    // --- Records ---

    async fn create_record(
        &self,
        collection_id: i64,
        data: &Value,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_record(collection_id, data).await
    }

    async fn import_record(&self, collection_id: i64, record_id: i64, data: &Value) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let res = self.inner.import_record(collection_id, record_id, data).await;
        // Even though it's an import, if we somehow imported into a live DB, we should invalidate cache
        self.record_cache.invalidate(&(collection_id, record_id)).await;
        res
    }

    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
        expand: Option<String>,
    ) -> Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> {
        // 1. If Expanding, bypass cache (Dynamic content)
        if let Some(ex) = &expand {
            if !ex.trim().is_empty() {
                return self.inner.get_record(collection_id, record_id, expand).await;
            }
        }

        let key = (collection_id, record_id);
        
        if let Some(cached) = self.record_cache.get(&key).await {
            return Ok(Some((*cached).clone()));
        }

        // Pass None for expand to get raw record
        let result = self.inner.get_record(collection_id, record_id, None).await?;
        if let Some(rec) = &result {
            self.record_cache.insert(key, Arc::new(rec.clone())).await;
        }
        Ok(result)
    }

    async fn list_records(
        &self,
        collection_id: i64,
        options: QueryOptions,
    ) -> Result<ListResult, Box<dyn std::error::Error + Send + Sync>> {
        // Caching list results is tricky with complex filters/expands.
        // For now, pass through. If you want to cache, use (collection_id, options) as key.
        self.inner.list_records(collection_id, options).await
    }

    async fn update_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> Result<Record, Box<dyn std::error::Error + Send + Sync>> {
        let res = self.inner.update_record(collection_id, record_id, data).await?;
        self.record_cache.invalidate(&(collection_id, record_id)).await;
        Ok(res)
    }

    // FIX: Updated return type to match trait
    async fn delete_record(&self, collection_id: i64, record_id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_record(collection_id, record_id).await?;
        self.record_cache.invalidate(&(collection_id, record_id)).await;
        Ok(())
    }

    // --- Search ---
    async fn search_records(&self, collection_id: i64, query: &str) -> Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.search_records(collection_id, query).await
    }

    async fn instant_search(&self, collection_id: i64, query: &str, limit: usize) -> Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.instant_search(collection_id, query, limit).await
    }

    async fn recover_indexes(&self) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.recover_indexes().await
    }

    async fn index_record_search(&self, c: i64, r: i64, d: &Value, s: &CollectionSchema) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.index_record_search(c, r, d, s).await
    }
    async fn delete_record_search(&self, c: i64, r: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_record_search(c, r).await
    }
    // Add reindex_collection 
    async fn reindex_collection(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        // Pass through to inner (ApexKit)
        self.inner.reindex_collection(id).await
    }

    // --- API keys ---
    async fn create_api_key(&self, name: &str, role: &str, scope: &str, bypass_cors: bool) -> std::result::Result<(String, ApiKey), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_api_key(name, role, scope, bypass_cors).await
    }

    async fn update_api_key(&self, id: i64, name: Option<String>, role: Option<String>, scope: Option<String>, bypass_cors: Option<bool>) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.update_api_key(id, name, role, scope, bypass_cors).await
    }
    
    async fn list_api_keys(&self) -> std::result::Result<Vec<ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_api_keys().await
    }
    
    async fn delete_api_key(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.delete_api_key(id).await
    }
    
    async fn verify_api_key(&self, key: &str) -> std::result::Result<Option<ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.verify_api_key(key).await
    }

    // --- Tenants ---
    // --- Tenant Management (Pass-through) ---
    async fn register_tenant(&self, id: &str, owner_id: Option<i64>, name: Option<String>, tier: Option<String>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.register_tenant(id, owner_id, name, tier).await
    }
    
    async fn get_tenant_status(&self, tenant_id: &str) -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_tenant_status(tenant_id).await
    }

    async fn update_tenant_status(&self, tenant_id: &str, status: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.update_tenant_status(tenant_id, status).await
    }

    async fn update_tenant_full(&self, id: &str, n: Option<String>, s: Option<String>, t: Option<String>) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.update_tenant_full(id, n, s, t).await
    }

    async fn delete_tenant_metadata(&self, id: &str) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_tenant_metadata(id).await
    }

    async fn list_tenants(&self) -> std::result::Result<Vec<models::Tenant>, Box<dyn StdError + Send + Sync>> {
        self.inner.list_tenants().await
    }

    async fn get_tenant_disk_usage(&self, tenant_id: &str) -> std::result::Result<u64, Box<dyn StdError + Send + Sync>> {
        self.inner.get_tenant_disk_usage(tenant_id).await
    }

    // --- Sandboxes ---
    async fn register_sandbox(&self, id: &str, owner_id: Option<i64>, name: Option<String>, expires_at: Option<String>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.register_sandbox(id, owner_id, name, expires_at).await
    }

    async fn update_sandbox_full(&self, id: &str, n: Option<String>, s: Option<String>, e: Option<String>) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.update_sandbox_full(id, n, s, e).await
    }

    async fn delete_sandbox_metadata(&self, id: &str) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_sandbox_metadata(id).await
    }

    async fn get_sandbox_disk_usage(&self, id: &str) -> std::result::Result<u64, Box<dyn StdError + Send + Sync>> {
        self.inner.get_sandbox_disk_usage(id).await
    }

    // --- Vectorization ---

    async fn save_vector(&self, collection_id: i64, record_id: i64, field_name: &str, vector: Vec<f32>, model: &str) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.save_vector(collection_id, record_id, field_name, vector, model).await
    }
    
    async fn has_vector(&self, collection_id: i64, record_id: i64, field_name: &str, model: &str) -> std::result::Result<bool, Box<dyn StdError + Send + Sync>> {
        self.inner.has_vector(collection_id, record_id, field_name, model).await
    }

    async fn get_record_vectors(&self, collection_id: i64, record_id: i64) -> std::result::Result<Vec<models::VectorRecord>, Box<dyn StdError + Send + Sync>> {
        self.inner.get_record_vectors(collection_id, record_id).await
    }
    
    async fn get_vectors_for_collection(&self, collection_id: i64, model: &str) -> std::result::Result<Vec<(i64, String, Vec<f32>)>, Box<dyn StdError + Send + Sync>> {
        self.inner.get_vectors_for_collection(collection_id, model).await
    }
    
    async fn search_vector(&self, collection_id: i64, field: &str, vector: Vec<f32>, limit: usize) -> std::result::Result<Vec<Record>, Box<dyn StdError + Send + Sync>> {
        self.inner.search_vector(collection_id, field, vector, limit).await
    }

    // --- Users ---

    async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        role: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<User, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_user(email, password_hash, role, metadata).await
    }

    async fn import_user(&self, id: i64, email: &str, password_hash: &str, role: &str, metadata: Option<Value>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.import_user(id, email, password_hash, role, metadata).await
    }

    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_user_by_email(email).await
    }

    async fn list_users(&self, query: Option<String>, limit: i64, offset: i64) -> Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_users(query, limit, offset).await
    }

    async fn count_users(&self, query: Option<String>) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.count_users(query).await
    }

    async fn get_users_by_ids(&self, ids: &[i64]) -> Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_users_by_ids(ids).await
    }

    // FIX: Updated return type
    async fn delete_user(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_user(id).await
    }

    async fn update_user(&self, id: i64, email: Option<String>, role: Option<String>, metadata: Option<serde_json::Value>, password: Option<String>) -> std::result::Result<User, Box<dyn StdError + Send + Sync>> {
        self.inner.update_user(id, email, role, metadata, password).await
    }

    // --- Files ---

    async fn create_file_metadata(
        &self,
        filename: &str,
        original_name: &str,
        mime_type: &str,
        size: i64,
        user_id: Option<i64>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_file_metadata(filename, original_name, mime_type, size, user_id).await
    }

    async fn list_files(&self, limit: i64, offset: i64) -> Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_files(limit, offset).await
    }

    async fn count_files(&self) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.count_files().await
    }

    async fn get_file_metadata(&self, id: i64) -> Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_file_metadata(id).await
    }

    // FIX: Updated return type
    async fn delete_file_metadata(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_file_metadata(id).await
    }

    // --- Advanced Auth ---

    async fn get_user_by_oauth(
        &self,
        provider: &str,
        provider_id: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_user_by_oauth(provider, provider_id).await
    }

    async fn link_oauth(
        &self,
        user_id: i64,
        provider: &str,
        provider_id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.link_oauth(user_id, provider, provider_id).await
    }

    async fn create_auth_token(
        &self,
        user_id: i64,
        token_type: &str,
        token: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_auth_token(user_id, token_type, token).await
    }

    async fn consume_auth_token(
        &self,
        token: &str,
        token_type: &str,
    ) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.consume_auth_token(token, token_type).await
    }

    async fn set_user_verified(
        &self,
        user_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.set_user_verified(user_id).await
    }

    // --- Config & Settings ---
    async fn get_config(&self, key: &str) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        // We could cache config here if needed, but it's often better to fetch fresh for settings
        self.inner.get_config(key).await
    }

    async fn set_config(&self, key: &str, value: &serde_json::Value, encrypted: bool) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.set_config(key, value, encrypted).await
    }

    async fn list_configs(&self) -> std::result::Result<Vec<models::ConfigItem>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_configs().await
    }

    async fn delete_config(&self, key: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.delete_config(key).await
    }

    // --- Audit Logs ---

    async fn log_audit_event(&self, level: &str, message: &str, source: &str, meta: Option<serde_json::Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.log_audit_event(level, message, source, meta).await
    }

    async fn list_audit_logs(&self) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_audit_logs().await
    }

    async fn log_system_event(&self, level: &str, target: &str, message: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.log_system_event(level, target, message).await
    }

    // --- AI Actions ---

    async fn list_ai_actions(&self) -> std::result::Result<Vec<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_ai_actions().await
    }

    async fn get_ai_action(&self, slug: &str) -> std::result::Result<Option<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_ai_action(slug).await
    }

    async fn create_ai_action(&self, action: ai_models::CreateActionReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_ai_action(action).await
    }

    // FIX: Updated return type
    async fn delete_ai_action(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_ai_action(id).await
    }

    // --- AI Sessions (Pass-through, no caching for chat state) ---

    async fn create_ai_session(&self, session: &AiSession) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_ai_session(session).await
    }

    async fn get_ai_session(&self, id: &str) -> std::result::Result<Option<AiSession>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_ai_session(id).await
    }

    async fn update_ai_session(&self, session: &AiSession) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.update_ai_session(session).await
    }

    async fn list_ai_sessions(&self) -> std::result::Result<Vec<AiSession>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_ai_sessions().await
    }

    async fn delete_ai_session(&self, id: &str) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_ai_session(id).await
    }

    // --- Plugins (Pass-through) ---

    async fn save_plugin(&self, plugin: &Plugin) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.save_plugin(plugin).await
    }

    async fn list_plugins(&self) -> std::result::Result<Vec<Plugin>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_plugins().await
    }

    // --- Relations ---

    async fn create_relation(&self, oc: i64, oi: i64, tc: i64, ti: i64, rn: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_relation(oc, oi, tc, ti, rn).await
    }
    // FIX: Updated return type
    async fn delete_relation(&self, oc: i64, oi: i64, tc: i64, ti: i64, rn: &str) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_relation(oc, oi, tc, ti, rn).await
    }
    async fn get_related_ids(&self, oc: i64, oi: i64, rn: &str) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_related_ids(oc, oi, rn).await
    }
    async fn get_records_by_ids(&self, c: i64, i: &[i64]) -> Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_records_by_ids(c, i).await
    }

    // --- Scripts ---
    async fn list_scripts(&self) -> Result<Vec<crate::script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_scripts().await
    }
    async fn create_script(&self, req: crate::script_models::CreateScriptReq) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_script(req).await
    }
    // FIX: Updated return type
    async fn delete_script(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_script(id).await
    }
    async fn get_script_by_name(&self, name: &str) -> Result<Option<crate::script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_script_by_name(name).await
    }
    async fn get_scripts_by_trigger(&self, trigger: &str) -> Result<Vec<crate::script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_scripts_by_trigger(trigger).await
    }

    // --- Templates (Cached) ---

    async fn list_templates(&self) -> Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_templates().await
    }

    async fn get_template_by_slug(&self, slug: &str) -> Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Try Cache
        if let Some(cached) = self.template_cache.get(slug).await {
            return Ok(Some((*cached).clone()));
        }

        // 2. Fetch from DB
        let result = self.inner.get_template_by_slug(slug).await?;

        // 3. Populate Cache
        if let Some(tmpl) = &result {
            self.template_cache.insert(slug.to_string(), Arc::new(tmpl.clone())).await;
        }
        Ok(result)
    }

    async fn create_template(&self, req: models::CreateTemplateReq) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let slug = req.slug.clone();
        let id = self.inner.create_template(req).await?;
        self.template_cache.invalidate(&slug).await;
        Ok(id)
    }

    async fn update_template(&self, id: i64, content: String, script_id: Option<i64>) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.update_template(id, content, script_id).await?;
        // Invalidate all for safety or implement lookup to find slug by id
        self.template_cache.invalidate_all();
        Ok(())
    }

    // FIX: Updated return type
    async fn delete_template(&self, id: i64) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.inner.delete_template(id).await?;
        self.template_cache.invalidate_all();
        Ok(())
    }

    async fn get_dashboard_stats(&self) -> Result<crate::models::DashboardData, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_dashboard_stats().await
    }
}