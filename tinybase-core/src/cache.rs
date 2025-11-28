// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/cache.rs ===========================
use crate::{Db, Collection, Record, schema::CollectionSchema, query::QueryOptions, auth::User, security, models, ai_models};
use async_trait::async_trait;
use moka::future::Cache;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

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
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_collection(name, schema).await
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

    async fn delete_collection(&self, id: i64) -> crate::Result<()> {
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

    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let key = (collection_id, record_id);
        
        if let Some(cached) = self.record_cache.get(&key).await {
            return Ok(Some((*cached).clone()));
        }

        let result = self.inner.get_record(collection_id, record_id).await?;
        if let Some(rec) = &result {
            self.record_cache.insert(key, Arc::new(rec.clone())).await;
        }
        Ok(result)
    }

    async fn list_records(
        &self,
        collection_id: i64,
        options: QueryOptions,
    ) -> Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
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

    async fn delete_record(&self, collection_id: i64, record_id: i64) -> crate::Result<()> {
        self.inner.delete_record(collection_id, record_id).await?;
        self.record_cache.invalidate(&(collection_id, record_id)).await;
        Ok(())
    }

    // --- Search ---
    async fn search_records(&self, collection_id: i64, query: &str) -> Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.search_records(collection_id, query).await
    }

    async fn instant_search(&self, collection_id: i64, query: &str) -> Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.instant_search(collection_id, query).await
    }

    // --- Users ---

    async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        role: &str,
    ) -> Result<User, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_user(email, password_hash, role).await
    }

    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_user_by_email(email).await
    }

    async fn list_users(&self) -> Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_users().await
    }

    async fn delete_user(&self, id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.delete_user(id).await
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

    async fn get_file_metadata(&self, id: i64) -> Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_file_metadata(id).await
    }

    async fn delete_file_metadata(&self, id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    async fn set_system_config(&self, key: &str, value: &security::EncryptedValue) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.set_system_config(key, value).await
    }
    
    async fn get_system_config(&self, key: &str) -> std::result::Result<Option<security::EncryptedValue>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_system_config(key).await
    }

    async fn get_setting(&self, key: &str) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.get_setting(key).await
    }

    async fn save_setting(&self, key: &str, value: serde_json::Value, encrypt: bool) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.save_setting(key, value, encrypt).await
    }

    // --- Audit Logs ---

    async fn log_audit_event(&self, level: &str, message: &str, source: &str, meta: Option<serde_json::Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.log_audit_event(level, message, source, meta).await
    }

    async fn list_audit_logs(&self) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.list_audit_logs().await
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

    async fn delete_ai_action(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.delete_ai_action(id).await
    }

    // --- Relations ---

    async fn create_relation(&self, oc: i64, oi: i64, tc: i64, ti: i64, rn: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.create_relation(oc, oi, tc, ti, rn).await
    }
    async fn delete_relation(&self, oc: i64, oi: i64, tc: i64, ti: i64, rn: &str) -> crate::Result<()> {
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
    async fn delete_script(&self, id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        self.inner.create_template(req).await
    }

    async fn update_template(&self, id: i64, content: String, script_id: Option<i64>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.update_template(id, content, script_id).await?;
        // We don't know the slug here easily to invalidate just one, so we invalidate all to be safe.
        // Templates change rarely, so this performance hit is negligible.
        self.template_cache.invalidate_all();
        Ok(())
    }

    async fn delete_template(&self, id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.delete_template(id).await?;
        self.template_cache.invalidate_all();
        Ok(())
    }
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/cache.rs ends here ===========================