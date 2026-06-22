use super::super::{models::ExpandableItem, traits::IntoSqlVal};
use crate::auth::User;
use crate::database::traits::VectorProvider;
use crate::models;
use crate::models::ai as ai_models;
use crate::models::schema::{CollectionPolicies, CollectionSchema, FieldType};
use crate::models::{AiSession, Plugin};
use crate::models::{ApiKey, ChartPoint, DashboardData, DashboardStats};
use crate::models::{ChangesetEvent, Collection, ListResult, Record};
use crate::query::ApexQuery;
use crate::query::QueryOptions;
use crate::search::SearchManager;
use crate::{Db, batching, embeddings};
use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;

use super::setup::*;
use super::utils::calculate_dir_size;
use super::utils::*;

pub use super::setup::a_new_database_connection;

// --- The Orchestrator: ApexKit ---
#[derive(Clone)]
pub struct ApexKit {
    base_path: String,
    hot_conn_core: Arc<Mutex<Connection>>,
    hot_conn_data: Arc<Mutex<Connection>>,
    hot_conn_log: Arc<Mutex<Connection>>,
    hot_conn_sys: Arc<Mutex<Connection>>,
    hot_conn_vec: Arc<Mutex<Connection>>,

    data_batcher: batching::WriteManager,
    log_batcher: batching::WriteManager,
    vector_batcher: batching::WriteManager,
    core_batcher: batching::WriteManager, // [ADDED]
    sys_batcher: batching::WriteManager,  // [ADDED]

    search: Arc<SearchManager>,
    pub embedder: Arc<embeddings::EmbedderService>,
    pub vector_provider: Arc<dyn VectorProvider>,
}

#[allow(clippy::too_many_arguments)]
impl ApexKit {
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

        apply_pragmas(&core)?;
        apply_pragmas(&data)?;
        apply_pragmas(&log)?;
        apply_pragmas(&sys)?;
        apply_pragmas(&vec)?;

        setup_core(&core)?;
        setup_data(&data)?;
        setup_logs(&log)?;
        setup_sys(&sys)?;
        setup_vectors(&vec)?;

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

    pub fn set_search_manager(&mut self, manager: Arc<SearchManager>) {
        self.search = manager;
    }

    pub async fn get_core_read<'a>(&'a self) -> tokio::sync::MutexGuard<'a, Connection> {
        self.hot_conn_core.lock().await
    }
    pub async fn get_data_read<'a>(&'a self) -> tokio::sync::MutexGuard<'a, Connection> {
        self.hot_conn_data.lock().await
    }
    pub async fn get_log_read<'a>(&'a self) -> tokio::sync::MutexGuard<'a, Connection> {
        self.hot_conn_log.lock().await
    }
    pub async fn get_sys_read<'a>(&'a self) -> tokio::sync::MutexGuard<'a, Connection> {
        self.hot_conn_sys.lock().await
    }
    pub async fn get_vector_read<'a>(&'a self) -> tokio::sync::MutexGuard<'a, Connection> {
        self.hot_conn_vec.lock().await
    }

    pub async fn ensure_search_index(
        &self,
        collection_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(col) = self.get_collection(collection_id).await?
            && let Some(schema) = &col.schema
            && schema.fields.values().any(|f| f.ose_indexed)
        {
            self.search.load_index(collection_id, schema)?;
        }
        Ok(())
    }

    async fn sync_relations(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
        schema: &CollectionSchema,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for (rel_name, rel_def) in &schema.relations {
            if let Some(val) = data.get(rel_name) {
                self.data_batcher.execute(
                    "DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND rel_name=?3".into(),
                    vec![collection_id.into_val(),record_id.into_val(),rel_name.clone().into_val()]
                ).await.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)?;

                // [FIX] Support Arrays of IDs for Multi-Relations
                let mut target_ids = Vec::new();
                if let Value::Array(arr) = val {
                    for v in arr {
                        let tid = match v {
                            Value::String(s) => s.parse::<i64>().unwrap_or(0),
                            Value::Number(n) => n.as_i64().unwrap_or(0),
                            _ => 0,
                        };
                        if tid != 0 {
                            target_ids.push(tid);
                        }
                    }
                } else {
                    let tid = match val {
                        Value::String(s) => s.parse::<i64>().unwrap_or(0),
                        Value::Number(n) => n.as_i64().unwrap_or(0),
                        _ => 0,
                    };
                    if tid != 0 {
                        target_ids.push(tid);
                    }
                }

                if target_ids.is_empty() {
                    continue;
                }

                let identifier = &rel_def.target_collection;
                let mut target_col_id: Option<i64> = None;

                {
                    let conn = self.get_data_read().await;
                    let mut name_stmt =
                        conn.prepare("SELECT id FROM collections WHERE name = ?1")?;
                    let mut name_rows = name_stmt.query(rusqlite::params![identifier.clone()])?;
                    if let Some(r) = name_rows.next()? {
                        target_col_id = Some(r.get(0)?);
                    } else if let Ok(id_num) = identifier.parse::<i64>() {
                        let mut id_stmt =
                            conn.prepare("SELECT id FROM collections WHERE id = ?1")?;
                        let mut id_rows = id_stmt.query(rusqlite::params![id_num])?;
                        if id_rows.next()?.is_some() {
                            target_col_id = Some(id_num);
                        }
                    }
                }

                if let Some(tc_id) = target_col_id {
                    for target_rec_id in target_ids {
                        self.data_batcher.execute(
                            "INSERT OR IGNORE INTO _relations (origin_col_id,origin_rec_id,target_col_id,target_rec_id,rel_name) VALUES (?1,?2,?3,?4,?5)".into(),
                            vec![collection_id.into_val(),record_id.into_val(),tc_id.into_val(),target_rec_id.into_val(),rel_name.clone().into_val()]
                        ).await.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn get_user_policies(&self) -> CollectionPolicies {
        if let Ok(Some(val)) = self.get_config("policy_users").await
            && let Ok(p) = serde_json::from_value(val)
        {
            return p;
        }
        CollectionPolicies {
            read: "admin || owner:id".to_string(),
            create: "public".to_string(),
            update: "admin || owner:id".to_string(),
            delete: "admin".to_string(),
        }
    }

    async fn populate_owners_in_memory(
        &self,
        records: &mut [Record],
        collection_id: i64,
        expand_opt: Option<&String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let expand_str = match expand_opt {
            Some(s) if !s.trim().is_empty() => s,
            _ => return Ok(()),
        };

        let tree = crate::query::builder::build_expand_tree(expand_str);

        let mut root_items: Vec<ExpandableItem> = records
            .iter_mut()
            .map(|r| ExpandableItem {
                data: &r.data,
                expand: &mut r.expand,
            })
            .collect();

        self.hydrate_owners_recursive(&mut root_items, collection_id, &tree)
            .await?;

        Ok(())
    }

    #[allow(clippy::type_complexity)]
    fn hydrate_owners_recursive<'a>(
        &'a self,
        items: &'a mut Vec<ExpandableItem<'a>>,
        collection_id: i64,
        tree: &'a HashMap<String, Vec<String>>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            if items.is_empty() || tree.is_empty() {
                return Ok(());
            }

            let col = match self.get_collection(collection_id).await? {
                Some(c) => c,
                None => return Ok(()),
            };
            let schema = col.schema.unwrap_or_default();

            let mut owner_fields = Vec::new();
            let mut relation_fields = Vec::new();

            let all_collections = self.list_collections().await?;
            let col_map: HashMap<String, i64> = all_collections
                .iter()
                .map(|c| (c.name.clone(), c.id))
                .collect();
            let id_map: HashMap<String, i64> = all_collections
                .iter()
                .map(|c| (c.id.to_string(), c.id))
                .collect();

            for (field_name, sub_paths) in tree {
                if let Some(def) = schema.fields.get(field_name)
                    && def.r#type == FieldType::Owner
                {
                    owner_fields.push(field_name);
                }

                let mut target_col_id = None;

                if let Some(rel_def) = schema.relations.get(field_name) {
                    let target = &rel_def.target_collection;
                    target_col_id = col_map.get(target).or_else(|| id_map.get(target)).cloned();
                } else if let Some(target_id) = col_map.get(field_name) {
                    target_col_id = Some(*target_id);
                }

                if let Some(tid) = target_col_id {
                    relation_fields.push((field_name, tid, sub_paths));
                }
            }

            if !owner_fields.is_empty() {
                for item in items.iter_mut() {
                    if item.expand.is_none() {
                        *item.expand = Some(serde_json::json!({}));
                    }
                }

                let mut user_ids = std::collections::HashSet::new();
                for item in items.iter() {
                    if let Some(obj) = item.data.as_object() {
                        for field in &owner_fields {
                            if let Some(val) = obj.get(*field) {
                                if let Some(uid) = val.as_i64() {
                                    user_ids.insert(uid);
                                } else if let Some(s) = val.as_str()
                                    && let Ok(uid) = s.parse::<i64>()
                                {
                                    user_ids.insert(uid);
                                }
                            }
                        }
                    }
                }

                if !user_ids.is_empty() {
                    let ids_vec: Vec<i64> = user_ids.into_iter().collect();
                    let users = self.get_users_by_ids(&ids_vec).await?;
                    let user_map: HashMap<i64, User> =
                        users.into_iter().map(|u| (u.id, u)).collect();

                    for item in items.iter_mut() {
                        let mut updates = Vec::new();
                        if let Some(obj) = item.data.as_object() {
                            for field in &owner_fields {
                                if let Some(val) = obj.get(*field) {
                                    let uid_opt = val.as_i64().or_else(|| {
                                        val.as_str().and_then(|s| s.parse::<i64>().ok())
                                    });
                                    if let Some(uid) = uid_opt
                                        && let Some(user) = user_map.get(&uid)
                                    {
                                        updates.push((
                                            (*field).clone(),
                                            serde_json::json!({
                                                "id": user.id,
                                                "email": user.email,
                                                "role": user.role,
                                                "metadata": user.metadata
                                            }),
                                        ));
                                    }
                                }
                            }
                        }
                        if let Some(expand_obj) =
                            item.expand.as_mut().and_then(|v| v.as_object_mut())
                        {
                            for (f, v) in updates {
                                expand_obj.insert(f, v);
                            }
                        }
                    }
                }
            }

            for (rel_name, target_id, sub_paths_list) in relation_fields {
                let sub_tree = crate::query::builder::build_expand_tree_from_list(sub_paths_list);
                if sub_tree.is_empty() {
                    continue;
                }

                for item in items.iter_mut() {
                    if let Some(expand_val) = item.expand
                        && let Some(rel_val) = expand_val.get_mut(rel_name)
                    {
                        if let Some(arr) = rel_val.as_array_mut() {
                            self.hydrate_json_values_recursive(arr, target_id, &sub_tree)
                                .await?;
                        } else if rel_val.is_object() {
                            let slice = std::slice::from_mut(rel_val);
                            self.hydrate_json_values_recursive(slice, target_id, &sub_tree)
                                .await?;
                        }
                    }
                }
            }

            Ok(())
        })
    }

    #[allow(clippy::type_complexity)]
    fn hydrate_json_values_recursive<'a>(
        &'a self,
        json_records: &'a mut [Value],
        collection_id: i64,
        tree: &'a HashMap<String, Vec<String>>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            if json_records.is_empty() {
                return Ok(());
            }

            let col = match self.get_collection(collection_id).await? {
                Some(c) => c,
                None => return Ok(()),
            };
            let schema = col.schema.unwrap_or_default();

            let mut owner_fields = Vec::new();
            let mut relation_fields = Vec::new();
            let all_collections = self.list_collections().await?;
            let col_map: HashMap<String, i64> = all_collections
                .iter()
                .map(|c| (c.name.clone(), c.id))
                .collect();
            let id_map: HashMap<String, i64> = all_collections
                .iter()
                .map(|c| (c.id.to_string(), c.id))
                .collect();

            for (field_name, sub_paths) in tree {
                if let Some(def) = schema.fields.get(field_name)
                    && def.r#type == FieldType::Owner
                {
                    owner_fields.push(field_name);
                }
                let mut target_col_id = None;
                if let Some(rel_def) = schema.relations.get(field_name) {
                    target_col_id = col_map
                        .get(&rel_def.target_collection)
                        .or_else(|| id_map.get(&rel_def.target_collection))
                        .cloned();
                } else if let Some(target_id) = col_map.get(field_name) {
                    target_col_id = Some(*target_id);
                }
                if let Some(tid) = target_col_id {
                    relation_fields.push((field_name, tid, sub_paths));
                }
            }

            if !owner_fields.is_empty() {
                let mut user_ids = std::collections::HashSet::new();
                for rec in json_records.iter() {
                    if let Some(data) = rec.get("data") {
                        for field in &owner_fields {
                            if let Some(val) = data.get(*field) {
                                if let Some(uid) = val.as_i64() {
                                    user_ids.insert(uid);
                                } else if let Some(s) = val.as_str()
                                    && let Ok(uid) = s.parse::<i64>()
                                {
                                    user_ids.insert(uid);
                                }
                            }
                        }
                    }
                }

                if !user_ids.is_empty() {
                    let ids_vec: Vec<i64> = user_ids.into_iter().collect();
                    let users = self.get_users_by_ids(&ids_vec).await?;
                    let user_map: HashMap<i64, User> =
                        users.into_iter().map(|u| (u.id, u)).collect();

                    for rec in json_records.iter_mut() {
                        let mut updates = Vec::new();
                        if let Some(data) = rec.get("data") {
                            for field in &owner_fields {
                                if let Some(val) = data.get(*field) {
                                    let uid_opt = val.as_i64().or_else(|| {
                                        val.as_str().and_then(|s| s.parse::<i64>().ok())
                                    });
                                    if let Some(uid) = uid_opt
                                        && let Some(user) = user_map.get(&uid)
                                    {
                                        updates.push((
                                            (*field).clone(),
                                            serde_json::json!({
                                                "id": user.id,
                                                "email": user.email,
                                                "role": user.role,
                                                "metadata": user.metadata
                                            }),
                                        ));
                                    }
                                }
                            }
                        }

                        if !updates.is_empty() {
                            if (rec.get("expand").is_none() || rec.get("expand").unwrap().is_null())
                                && let Some(obj) = rec.as_object_mut()
                            {
                                obj.insert("expand".to_string(), serde_json::json!({}));
                            }
                            if let Some(expand) =
                                rec.get_mut("expand").and_then(|v| v.as_object_mut())
                            {
                                for (f, v) in updates {
                                    expand.insert(f, v);
                                }
                            }
                        }
                    }
                }
            }

            for (rel_name, target_id, sub_paths_list) in relation_fields {
                let sub_tree = crate::query::builder::build_expand_tree_from_list(sub_paths_list);
                if sub_tree.is_empty() {
                    continue;
                }

                for rec in json_records.iter_mut() {
                    if let Some(expand) = rec.get_mut("expand")
                        && let Some(rel_val) = expand.get_mut(rel_name)
                    {
                        if let Some(arr) = rel_val.as_array_mut() {
                            self.hydrate_json_values_recursive(arr, target_id, &sub_tree)
                                .await?;
                        } else if rel_val.is_object() {
                            let slice = std::slice::from_mut(rel_val);
                            self.hydrate_json_values_recursive(slice, target_id, &sub_tree)
                                .await?;
                        }
                    }
                }
            }

            Ok(())
        })
    }

    // --- Unified Internal Write Logic ---
    async fn _write_record_internal(
        &self,
        collection_id: i64,
        record_id: Option<i64>,
        data: &Value,
        validate: bool,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let col = {
            let conn = self.get_data_read().await;
            let mut stmt = conn.prepare("SELECT id,name,schema FROM collections WHERE id = ?1")?;
            let mut rows = stmt.query(rusqlite::params![collection_id])?;
            if let Some(row) = rows.next()? {
                Some(row_to_collection(row)?)
            } else {
                None
            }
        }
        .ok_or("Collection not found")?;

        let schema = col.schema.unwrap_or_default();

        let mut final_data = data.clone();

        // Strip reserved frontend/framework fields to prevent double nesting inside 'data'
        if let Some(obj) = final_data.as_object_mut() {
            obj.remove("id");
            obj.remove("_id"); // Strips legacy and migrated mongo-style keys securely
            obj.remove("created");
            obj.remove("updated");
            obj.remove("expand");
            obj.remove("collectionId");
            obj.remove("collectionName");
        }

        if validate
            && let Err(errors) = crate::validation::sanitize_and_validate(&schema, &mut final_data)
        {
            let err_json = serde_json::to_string(&errors).unwrap();
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Schema Validation Failed: {}", err_json),
            )));
        }

        // OPTIMIZATION: Zero-copy serialization into SQLite's native JSONB binary format
        let jsonb_bytes = serde_sqlite_jsonb::to_vec(&final_data).map_err(Box::new)?;

        // Ensure Read Lock Dropped Before `await` on Batcher
        {
            let conn = self.get_data_read().await;
            enforce_uniqueness(&conn, collection_id, record_id, &final_data, &schema)?;
        }

        let actual_record_id = if let Some(rid) = record_id {
            self.data_batcher
                .execute(
                    "INSERT INTO records (id,collection_id,data) VALUES (?1,?2,?3)".into(),
                    vec![
                        rid.into_val(),
                        collection_id.into_val(),
                        rusqlite::types::Value::Blob(jsonb_bytes),
                    ],
                )
                .await?;
            rid
        } else {
            self.data_batcher
                .insert(
                    "INSERT INTO records (collection_id,data) VALUES (?1,?2)".into(),
                    vec![
                        collection_id.into_val(),
                        rusqlite::types::Value::Blob(jsonb_bytes),
                    ],
                )
                .await?
        };

        let unique_future = commit_uniqueness(
            &self.data_batcher,
            collection_id,
            actual_record_id,
            &final_data,
            &schema,
        );
        let relation_future =
            self.sync_relations(collection_id, actual_record_id, &final_data, &schema);

        tokio::try_join!(unique_future, relation_future)?;

        Ok(actual_record_id)
    }
}

#[async_trait]
impl Db for ApexKit {
    async fn create_api_key(
        &self,
        name: &str,
        tenant_id: &str,
        issuer: &str,
        env_type: &str,
        roles: Vec<String>,
        bypass_cors: bool,
    ) -> std::result::Result<(String, ApiKey), Box<dyn std::error::Error + Send + Sync>> {
        let key_env = match env_type {
            "sys" => crate::security::api_keys::KeyEnv::Sys,
            "tnnt" => crate::security::api_keys::KeyEnv::Tnnt,
            "sk" => crate::security::api_keys::KeyEnv::Sk,
            "pk" => crate::security::api_keys::KeyEnv::Pk,
            _ => crate::security::api_keys::KeyEnv::Sys,
        };

        let (raw_key, secret_hash, key_id) =
            crate::security::api_keys::generate_api_key(tenant_id, key_env);
        let roles_json = serde_json::to_string(&roles)?;

        let id = self.core_batcher.insert(
            "INSERT INTO _api_keys_v2 (name,tenant_id,key_id,secret_hash,issuer,env_type,roles,status,bypass_cors) VALUES (?1,?2,?3,?4,?5,?6,?7,'active',?8)".into(),
            vec![name.into_val(),tenant_id.into_val(),key_id.clone().into_val(),secret_hash.into_val(),issuer.into_val(),env_type.into_val(),roles_json.into_val(),bypass_cors.into_val()]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)?;

        let key_obj = ApiKey {
            id,
            name: name.to_string(),
            tenant_id: tenant_id.to_string(),
            key_id,
            issuer: issuer.to_string(),
            env_type: env_type.to_string(),
            roles,
            status: "active".to_string(),
            bypass_cors,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        Ok((raw_key, key_obj))
    }

    async fn update_api_key(
        &self,
        id: i64,
        name: Option<String>,
        status: Option<String>,
        roles: Option<Vec<String>>,
        bypass_cors: Option<bool>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut sets = Vec::new();
        let mut params: Vec<rusqlite::types::Value> = vec![];

        if let Some(n) = name {
            sets.push("name = ?");
            params.push(n.into_val());
        }
        if let Some(s) = status {
            sets.push("status = ?");
            params.push(s.into_val());
        }
        if let Some(r) = roles {
            let r_str = serde_json::to_string(&r)?;
            sets.push("roles = ?");
            params.push(r_str.into_val());
        }
        if let Some(b) = bypass_cors {
            sets.push("bypass_cors = ?");
            params.push(b.into_val());
        }

        if sets.is_empty() {
            return Ok(());
        }
        params.push(id.into_val());

        let sql = format!("UPDATE _api_keys_v2 SET {} WHERE id = ?", sets.join(","));
        self.core_batcher.execute(sql, params).await.map_err(|e| {
            Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
        })?;

        Ok(())
    }

    async fn list_api_keys(
        &self,
    ) -> std::result::Result<Vec<ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare("SELECT id,name,tenant_id,key_id,issuer,env_type,roles,status,bypass_cors,created_at FROM _api_keys_v2 ORDER BY created_at DESC")?;
        let mut rows = stmt.query([])?;
        let mut keys = Vec::new();
        while let Some(row) = rows.next()? {
            let roles_str: String = row.get(6)?;
            keys.push(ApiKey {
                id: row.get(0)?,
                name: row.get(1)?,
                tenant_id: row.get(2)?,
                key_id: row.get(3)?,
                issuer: row.get(4)?,
                env_type: row.get(5)?,
                roles: serde_json::from_str(&roles_str).unwrap_or_default(),
                status: row.get(7)?,
                bypass_cors: row.get(8).unwrap_or(false),
                created_at: row.get(9)?,
            });
        }
        Ok(keys)
    }

    async fn delete_api_key(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "DELETE FROM _api_keys_v2 WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;
        Ok(())
    }

    async fn verify_api_key(
        &self,
        tenant_id: &str,
        key_id: &str,
        secret: &str,
    ) -> std::result::Result<Option<ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
        let expected_hash = crate::utils::sha256(secret);
        let conn = self.get_core_read().await;

        let mut stmt = conn.prepare("SELECT id,name,tenant_id,key_id,issuer,env_type,roles,status,bypass_cors,created_at FROM _api_keys_v2 WHERE tenant_id = ?1 AND key_id = ?2 AND secret_hash = ?3")?;
        let mut rows = stmt.query(params![tenant_id, key_id, expected_hash])?;

        if let Some(row) = rows.next()? {
            let status: String = row.get(7)?;
            if status != "active" {
                return Ok(None);
            }

            let roles_str: String = row.get(6)?;

            return Ok(Some(ApiKey {
                id: row.get(0)?,
                name: row.get(1)?,
                tenant_id: row.get(2)?,
                key_id: row.get(3)?,
                issuer: row.get(4)?,
                env_type: row.get(5)?,
                roles: serde_json::from_str(&roles_str).unwrap_or_default(),
                status,
                bypass_cors: row.get(8).unwrap_or(false),
                created_at: row.get(9)?,
            }));
        }
        Ok(None)
    }

    // --- Data DB (Collections/Records) ---

    async fn create_collection(
        &self,
        name: &str,
        schema: &Option<CollectionSchema>,
        index: Option<String>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let schema_str = serde_json::to_string(&schema)?;
        let idx = index.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let id = self
            .data_batcher
            .insert(
                "INSERT INTO collections (name,schema,index_key) VALUES (?1,?2,?3)".into(),
                vec![name.into_val(), schema_str.into_val(), idx.into_val()],
            )
            .await
            .map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;

        if let Some(s) = schema {
            if s.fields.values().any(|f| f.ose_indexed) {
                self.search.load_index(id, s)?;
            }
            reconcile_sql_indexes(&self.data_batcher, id, s, None).await?;
        }
        Ok(id)
    }

    async fn update_collection(
        &self,
        id: i64,
        name: Option<String>,
        schema: Option<CollectionSchema>,
    ) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
        let existing = self.get_collection(id).await?.ok_or("Not found")?;
        let old_schema = existing.schema;

        if let Some(n) = name {
            self.data_batcher
                .execute(
                    "UPDATE collections SET name = ?1 WHERE id = ?2".into(),
                    vec![n.into_val(), id.into_val()],
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;
        }

        if let Some(s) = &schema {
            let s_str = serde_json::to_string(&s)?;
            self.data_batcher
                .execute(
                    "UPDATE collections SET schema = ?1 WHERE id = ?2".into(),
                    vec![s_str.into_val(), id.into_val()],
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })?;

            // --- EFFICIENT SQL MIGRATION FOR RENAMED & DELETED FIELDS ---
            if let Some(old_s) = &old_schema {
                // 1. Handle standard field renames within the JSON blob
                for (new_name, new_def) in &s.fields {
                    if let Some((old_name, _)) = old_s
                        .fields
                        .iter()
                        .find(|(_, old_def)| old_def.uid == new_def.uid)
                        && old_name != new_name
                    {
                        let sql = format!(
                            "UPDATE records SET data = json_remove(json_set(data,'$.{}',json_extract(data,'$.{}')),'$.{}') WHERE collection_id = ? AND json_type(data,'$.{}') IS NOT NULL",
                            new_name, old_name, old_name, old_name
                        );
                        self.data_batcher
                            .execute(sql, vec![id.into_val()])
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;
                    }
                }

                // 2. Handle relational field renames within the JSON blob AND linking table
                for (new_name, new_def) in &s.relations {
                    if let Some((old_name, _)) = old_s
                        .relations
                        .iter()
                        .find(|(_, old_def)| old_def.uid == new_def.uid)
                        && old_name != new_name
                    {
                        // Update internal JSON record data
                        let sql = format!(
                            "UPDATE records SET data = json_remove(json_set(data,'$.{}',json_extract(data,'$.{}')),'$.{}') WHERE collection_id = ? AND json_type(data,'$.{}') IS NOT NULL",
                            new_name, old_name, old_name, old_name
                        );
                        self.data_batcher
                            .execute(sql, vec![id.into_val()])
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;

                        // Update _relations SQL lookup table to maintain constraint mappings
                        let rel_sql = "UPDATE _relations SET rel_name = ? WHERE origin_col_id = ? AND rel_name = ?".to_string();
                        self.data_batcher
                            .execute(
                                rel_sql,
                                vec![
                                    new_name.clone().into_val(),
                                    id.into_val(),
                                    old_name.clone().into_val(),
                                ],
                            )
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;
                    }
                }

                // 3. Handle Deleted Standard Fields (Remove from JSON)
                for (old_name, old_def) in &old_s.fields {
                    let still_exists = s.fields.values().any(|new_def| new_def.uid == old_def.uid);
                    if !still_exists {
                        let sql = format!(
                            "UPDATE records SET data = json_remove(data,'$.{}') WHERE collection_id = ? AND json_type(data,'$.{}') IS NOT NULL",
                            old_name, old_name
                        );
                        self.data_batcher
                            .execute(sql, vec![id.into_val()])
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;
                    }
                }

                // 4. Handle Deleted Relations (Remove from JSON AND _relations linking table)
                for (old_name, old_def) in &old_s.relations {
                    let still_exists = s
                        .relations
                        .values()
                        .any(|new_def| new_def.uid == old_def.uid);
                    if !still_exists {
                        // Remove from JSON
                        let sql = format!(
                            "UPDATE records SET data = json_remove(data,'$.{}') WHERE collection_id = ? AND json_type(data,'$.{}') IS NOT NULL",
                            old_name, old_name
                        );
                        self.data_batcher
                            .execute(sql, vec![id.into_val()])
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;

                        // Delete orphaned links in the relations table
                        let rel_sql =
                            "DELETE FROM _relations WHERE origin_col_id = ? AND rel_name = ?";
                        self.data_batcher
                            .execute(
                                rel_sql.to_string(),
                                vec![id.into_val(), old_name.clone().into_val()],
                            )
                            .await
                            .map_err(|e| {
                                Box::new(std::io::Error::other(e))
                                    as Box<dyn std::error::Error + Send + Sync>
                            })?;
                    }
                }
            }
            // --------------------------------------------------------------

            reconcile_sql_indexes(&self.data_batcher, id, s, old_schema.as_ref()).await?;

            if s.fields.values().any(|f| f.ose_indexed) {
                self.search.load_index(id, s)?;
            }
        }

        self.get_collection(id).await?.ok_or("Not found".into())
    }

    async fn get_collection(
        &self,
        id: i64,
    ) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt =
            conn.prepare("SELECT id,name,schema,index_key FROM collections WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_collection(row)?)),
            None => Ok(None),
        }
    }

    async fn list_collections(
        &self,
    ) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare("SELECT id,name,schema,index_key FROM collections")?;
        let mut rows = stmt.query([])?;
        let mut cols = Vec::new();
        while let Some(row) = rows.next()? {
            cols.push(row_to_collection(row)?);
        }
        Ok(cols)
    }

    async fn delete_collection(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };

        self.data_batcher
            .execute(
                "DELETE FROM records WHERE collection_id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(map_err)?;
        self.data_batcher
            .execute(
                "DELETE FROM _relations WHERE origin_col_id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(map_err)?;
        self.data_batcher
            .execute(
                "DELETE FROM _relations WHERE target_col_id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(map_err)?;
        self.data_batcher
            .execute(
                "DELETE FROM collections WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(map_err)?;
        let _ = self.search.delete_index(id);
        Ok(())
    }

    async fn create_record(
        &self,
        collection_id: i64,
        data: &Value,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        // Reuse unified logic with validation enabled
        self._write_record_internal(collection_id, None, data, true)
            .await
    }

    async fn import_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Reuse unified logic with validation disabled (import assumes raw data is valid)
        self._write_record_internal(collection_id, Some(record_id), data, false)
            .await?;
        Ok(())
    }

    async fn update_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
        let (col, existing) = {
            let conn = self.get_data_read().await;

            let col = {
                let mut stmt =
                    conn.prepare("SELECT id,name,schema FROM collections WHERE id = ?1")?;
                let mut rows = stmt.query(rusqlite::params![collection_id])?;
                if let Some(row) = rows.next()? {
                    Some(row_to_collection(row)?)
                } else {
                    None
                }
            }
            .ok_or("Col not found")?;

            let existing = {
                // Keep json(data) on read to let SQLite's C-engine handle formatting
                let mut stmt = conn.prepare("SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ?1 AND id = ?2")?;
                let mut rows = stmt.query(rusqlite::params![collection_id,record_id])?;
                if let Some(row) = rows.next()? { Some(row_to_record(row)?) } else { None }
            }.ok_or("Rec not found")?;

            (col, existing)
        };

        let schema = col.schema.unwrap_or_default();

        let mut merged_data = existing.data.clone();
        if let Some(obj) = merged_data.as_object_mut()
            && let Some(new_obj) = data.as_object()
        {
            for (k, v) in new_obj {
                obj.insert(k.clone(), v.clone());
            }
        }

        if let Err(errors) = crate::validation::sanitize_and_validate(&schema, &mut merged_data) {
            let err_json = serde_json::to_string(&errors).unwrap();
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Schema Validation Failed: {}", err_json),
            )));
        }

        // OPTIMIZATION: Zero-copy serialization into SQLite's native JSONB binary format
        let jsonb_bytes = serde_sqlite_jsonb::to_vec(&merged_data).map_err(Box::new)?;

        {
            let conn = self.get_data_read().await;
            enforce_uniqueness(&conn, collection_id, Some(record_id), &merged_data, &schema)?;
        }

        self.data_batcher.execute(
            "UPDATE records SET data = ?1,updated = CURRENT_TIMESTAMP WHERE collection_id = ?2 AND id = ?3".into(),
            vec![rusqlite::types::Value::Blob(jsonb_bytes),collection_id.into_val(),record_id.into_val()]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)?;

        let unique_future = commit_uniqueness(
            &self.data_batcher,
            collection_id,
            record_id,
            &merged_data,
            &schema,
        );
        let relation_future = self.sync_relations(collection_id, record_id, &merged_data, &schema);

        tokio::try_join!(unique_future, relation_future)?;

        Ok(Record {
            id: record_id,
            data: merged_data,
            expand: serde_json::json!({}).into(),
            created: existing.created,
            updated: chrono::Utc::now().to_rfc3339(),
        })
    }

    async fn delete_record(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };

        let f1 = self.data_batcher.execute(
            "DELETE FROM records WHERE collection_id = ?1 AND id = ?2".into(),
            vec![collection_id.into_val(), record_id.into_val()],
        );
        let f2 = self.data_batcher.execute(
            "DELETE FROM _unique_values WHERE record_id = ?1".into(),
            vec![record_id.into_val()],
        );
        let f3 = self.data_batcher.execute(
            "DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2".into(),
            vec![collection_id.into_val(), record_id.into_val()],
        );
        let f4 = self.data_batcher.execute(
            "DELETE FROM _relations WHERE target_col_id=?1 AND target_rec_id=?2".into(),
            vec![collection_id.into_val(), record_id.into_val()],
        );
        let f5 = self.vector_batcher.execute(
            "DELETE FROM vectors WHERE collection_id = ?1 AND record_id = ?2".into(),
            vec![collection_id.into_val(), record_id.into_val()],
        );

        let _ = tokio::try_join!(f1, f2, f3, f4).map_err(map_err)?;
        let _ = f5.await.map_err(map_err)?;

        Ok(())
    }

    async fn index_record_search(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
        schema: &CollectionSchema,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.search
            .index_record(collection_id, record_id, data, schema)
            .map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn StdError + Send + Sync>)
    }

    async fn delete_record_search(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.search
            .delete_record(collection_id, record_id)
            .map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn StdError + Send + Sync>)
    }

    async fn list_records(
        &self,
        collection_id: i64,
        options: QueryOptions,
    ) -> std::result::Result<ListResult, Box<dyn std::error::Error + Send + Sync>> {
        let (mut records, total) = {
            let conn = self.get_data_read().await;

            let mut schema_map = HashMap::new();
            let mut id_map = HashMap::new();

            let current_col_name = {
                let mut stmt = conn.prepare("SELECT name FROM collections WHERE id = ?1")?;
                let mut rows = stmt.query(rusqlite::params![collection_id])?;
                if let Some(row) = rows.next()? {
                    row.get::<_, String>(0)?
                } else {
                    return Ok(ListResult {
                        items: vec![],
                        total: 0,
                    });
                }
            };

            if let Some(ref ex) = options.expand
                && !ex.trim().is_empty()
            {
                let mut stmt = conn.prepare("SELECT id,name,schema FROM collections")?;
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    let id: i64 = row.get(0)?;
                    let name: String = row.get(1)?;
                    let schema_str: Option<String> = row.get(2)?;
                    let schema = match schema_str {
                        Some(s) => serde_json::from_str(&s).unwrap_or_default(),
                        None => CollectionSchema::default(),
                    };

                    schema_map.insert(name.clone(), schema.clone());
                    schema_map.insert(id.to_string(), schema);
                    id_map.insert(name, id);
                    id_map.insert(id.to_string(), id);
                }
            }

            let builder = crate::query::SqlBuilder::new(
                collection_id,
                &current_col_name,
                options.clone(),
                &schema_map,
                &id_map,
            );

            let mut count_stmt = conn.prepare(&builder.count_sql)?;
            let mut count_rows =
                count_stmt.query(rusqlite::params_from_iter(builder.params.clone()))?;
            let total = if let Some(row) = count_rows.next()? {
                row.get::<usize, i64>(0)?
            } else {
                0
            };

            let mut stmt = conn.prepare(&builder.base_sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(builder.params))?;
            let mut recs = Vec::new();
            while let Some(row) = rows.next()? {
                recs.push(row_to_record(row)?);
            }
            (recs, total)
        };

        self.populate_owners_in_memory(&mut records, collection_id, options.expand.as_ref())
            .await?;

        Ok(ListResult {
            items: records,
            total,
        })
    }

    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
        expand: Option<String>,
    ) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let mut record_opt = {
            let conn = self.get_data_read().await;

            if expand.is_none() || expand.as_ref().unwrap().trim().is_empty() {
                let mut stmt = conn.prepare("SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ?1 AND id = ?2")?;
                let mut rows = stmt.query(rusqlite::params![collection_id, record_id])?;
                match rows.next()? {
                    Some(row) => Some(row_to_record(row)?),
                    None => None,
                }
            } else {
                let expand_str = expand.clone().unwrap();

                let mut schema_map = HashMap::new();
                let mut id_map = HashMap::new();

                let current_col_name = {
                    let mut stmt = conn.prepare("SELECT name FROM collections WHERE id = ?1")?;
                    let mut rows = stmt.query(rusqlite::params![collection_id])?;
                    if let Some(row) = rows.next()? {
                        row.get::<_, String>(0)?
                    } else {
                        return Ok(None);
                    }
                };

                {
                    let mut stmt = conn.prepare("SELECT id,name,schema FROM collections")?;
                    let mut rows = stmt.query([])?;
                    while let Some(row) = rows.next()? {
                        let id: i64 = row.get(0)?;
                        let name: String = row.get(1)?;
                        let schema_str: Option<String> = row.get(2)?;
                        let schema = match schema_str {
                            Some(s) => serde_json::from_str(&s).unwrap_or_default(),
                            None => CollectionSchema::default(),
                        };
                        schema_map.insert(name.clone(), schema.clone());
                        schema_map.insert(id.to_string(), schema);
                        id_map.insert(name, id);
                        id_map.insert(id.to_string(), id);
                    }
                }

                let paths = crate::query::builder::smart_split(&expand_str);
                let expand_json_sql = crate::query::builder::build_expand_json_object(
                    paths,
                    "records",
                    0,
                    &current_col_name,
                    collection_id,
                    &schema_map,
                    &id_map,
                );

                let sql = format!(
                    "SELECT id,json(data),{},created,updated FROM records WHERE collection_id = ?1 AND id = ?2",
                    expand_json_sql
                );

                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(rusqlite::params![collection_id, record_id])?;
                match rows.next()? {
                    Some(row) => Some(row_to_record(row)?),
                    None => None,
                }
            }
        };

        if let Some(rec) = &mut record_opt {
            let mut single_vec = vec![rec.clone()];
            self.populate_owners_in_memory(&mut single_vec, collection_id, expand.as_ref())
                .await?;
            *rec = single_vec.pop().unwrap();
        }

        Ok(record_opt)
    }

    async fn search_records(
        &self,
        collection_id: i64,
        query: &str,
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_search_index(collection_id).await?;

        let ids = self.search.search(collection_id, query, 50)?;
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let id_list = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id,json(data),NULL,created,updated FROM records WHERE id IN ({})",
            id_list
        );
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;

        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(row_to_record(row)?);
        }

        let id_pos: std::collections::HashMap<i64, usize> =
            ids.iter().enumerate().map(|(pos, id)| (*id, pos)).collect();

        records.sort_by(|a, b| {
            let pos_a = id_pos.get(&a.id).unwrap_or(&usize::MAX);
            let pos_b = id_pos.get(&b.id).unwrap_or(&usize::MAX);
            pos_a.cmp(pos_b)
        });

        Ok(records)
    }

    async fn recover_indexes(&self) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let collections = self.list_collections().await?;
        let available_cores = std::thread::available_parallelism()?.get();
        let max_concurrency = (available_cores / 2).max(1);

        println!(
            "[Recovery] Starting Index Recovery. Utilizing {}/{} cores.",
            max_concurrency, available_cores
        );

        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrency));
        let recovered_count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        for col in collections {
            let db = self.clone();
            let sem = semaphore.clone();
            let counter = recovered_count.clone();

            let handle = tokio::spawn(async move {
                let task_logic = async {
                    let _permit = sem.acquire().await.ok()?;

                    let db_count_res = {
                        let conn = db.get_data_read().await;
                        let mut stmt = conn
                            .prepare("SELECT COUNT(*) FROM records WHERE collection_id = ?")
                            .ok()?;
                        let mut rows = stmt.query(params![col.id]).ok()?;
                        if let Some(row) = rows.next().ok()? {
                            Some(row.get::<usize, i64>(0).unwrap_or(0) as u64)
                        } else {
                            Some(0)
                        }
                    };

                    let db_count = db_count_res.unwrap_or(0);
                    if db_count == 0 {
                        return Some(());
                    }

                    if let Some(schema) = &col.schema {
                        if !schema.fields.values().any(|f| f.ose_indexed) {
                            return Some(());
                        }
                        let _ = db.search.load_index(col.id, schema);
                    }

                    let idx_count = db.search.get_doc_count(col.id).unwrap_or(0);

                    if db_count != idx_count {
                        println!(
                            "[Recovery] Collection '{}' (ID: {}) mismatch. DB: {},Index: {}. Re-indexing...",
                            col.name, col.id, db_count, idx_count
                        );
                        if let Err(e) = db.reindex_collection(col.id).await {
                            eprintln!("[Recovery] Failed to re-index collection {}: {}", col.id, e);
                        } else {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Some(())
                };

                task_logic.await;
            });
            handles.push(handle);
        }

        for h in handles {
            let _ = h.await;
        }

        let count = recovered_count.load(Ordering::Relaxed);
        if count > 0 {
            println!("[Recovery] Successfully recovered {} collections.", count);
        } else {
            println!("[Recovery] Indexes healthy.");
        }

        Ok(())
    }

    async fn reindex_collection(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let col = self
            .get_collection(id)
            .await?
            .ok_or_else(|| format!("Collection {} not found", id))?;
        let schema = col.schema.unwrap_or_default();

        if !schema.fields.values().any(|f| f.ose_indexed) {
            return Ok(());
        }

        self.search
            .delete_index(id)
            .map_err(|e| format!("Search Delete Error: {}", e))?;
        self.search
            .load_index(id, &schema)
            .map_err(|e| format!("Search Load Error: {}", e))?;

        let conn = self.get_data_read().await;
        let mut stmt = conn
            .prepare(
                "SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ?1",
            )
            .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;
        let mut rows = stmt
            .query(params![id])
            .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;

        let mut buffer: Vec<(i64, serde_json::Value)> = Vec::with_capacity(1000);
        let batch_size = 1000;

        while let Some(row) = rows
            .next()
            .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?
        {
            let record = row_to_record(row)?;
            buffer.push((record.id, record.data));

            if buffer.len() >= batch_size {
                self.search
                    .index_batch(id, &buffer, &schema)
                    .map_err(|e| format!("Indexing Error: {}", e))?;
                buffer.clear();
            }
        }

        if !buffer.is_empty() {
            self.search
                .index_batch(id, &buffer, &schema)
                .map_err(|e| format!("Indexing Error: {}", e))?;
        }

        Ok(())
    }

    async fn instant_search(
        &self,
        collection_id: i64,
        query: &str,
        limit: usize,
    ) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>>
    {
        if let Some(col) = self.get_collection(collection_id).await?
            && let Some(schema) = &col.schema
        {
            if schema.fields.values().any(|f| f.ose_indexed) {
                self.search.load_index(collection_id, schema)?;
            } else {
                return Ok(vec![]);
            }
        }
        let results = self.search.instant_search(collection_id, query, limit)?;
        Ok(results)
    }

    // --- Core DB (Users,Auth,Settings) ---
    async fn create_user(
        &self,
        e: &str,
        p: &str,
        r: &str,
        metadata: Option<Value>,
    ) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>> {
        let meta = metadata.unwrap_or(json!({}));
        let meta_str = serde_json::to_string(&meta)?;
        let id = self
            .core_batcher
            .insert(
                "INSERT INTO users (email,password_hash,role,metadata) VALUES (?1,?2,?3,?4)".into(),
                vec![
                    e.into_val(),
                    p.into_val(),
                    r.into_val(),
                    meta_str.into_val(),
                ],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(User {
            id,
            email: e.into(),
            password_hash: p.into(),
            role: r.into(),
            metadata: Some(meta),
        })
    }

    async fn import_user(
        &self,
        id: i64,
        e: &str,
        p: &str,
        r: &str,
        metadata: Option<Value>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let meta = metadata.unwrap_or(json!({}));
        let meta_str = serde_json::to_string(&meta)?;
        self.core_batcher
            .execute(
                "INSERT INTO users (id,email,password_hash,role,metadata) VALUES (?1,?2,?3,?4,?5)"
                    .into(),
                vec![
                    id.into_val(),
                    e.into_val(),
                    p.into_val(),
                    r.into_val(),
                    meta_str.into_val(),
                ],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut stmt = conn
            .prepare("SELECT id,email,password_hash,role,metadata FROM users WHERE email = ?1")?;
        let mut r = stmt.query(params![email])?;
        if let Some(row) = r.next()? {
            let meta_str: String = row.get(4).unwrap_or("{}".to_string());
            Ok(Some(User {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                role: row.get(3)?,
                metadata: serde_json::from_str(&meta_str).ok(),
            }))
        } else {
            Ok(None)
        }
    }

    async fn list_users(
        &self,
        query: Option<String>,
        limit: i64,
        offset: i64,
    ) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let sql = if let Some(q) = query {
            format!(
                "SELECT id,email,password_hash,role,metadata FROM users WHERE email LIKE '%{}%' ORDER BY id DESC LIMIT {} OFFSET {}",
                q, limit, offset
            )
        } else {
            format!(
                "SELECT id,email,password_hash,role,metadata FROM users ORDER BY id DESC LIMIT {} OFFSET {}",
                limit, offset
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        let mut users = Vec::new();
        while let Some(row) = rows.next()? {
            let meta_str: String = row.get(4).unwrap_or("{}".to_string());
            users.push(User {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                role: row.get(3)?,
                metadata: serde_json::from_str(&meta_str).ok(),
            });
        }
        Ok(users)
    }

    async fn count_users(
        &self,
        query: Option<String>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let sql = if let Some(q) = query {
            format!("SELECT COUNT(*) FROM users WHERE email LIKE '%{}%'", q)
        } else {
            "SELECT COUNT(*) FROM users".to_string()
        };
        let mut stmt = conn.prepare(&sql)?;
        let mut row = stmt.query([])?;
        if let Some(r) = row.next()? {
            Ok(r.get(0)?)
        } else {
            Ok(0)
        }
    }

    async fn get_users_by_ids(
        &self,
        ids: &[i64],
    ) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let id_list = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id,email,password_hash,role,metadata FROM users WHERE id IN ({})",
            id_list
        );

        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        let mut users = Vec::new();
        while let Some(row) = rows.next()? {
            let meta_str: String = row.get(4).unwrap_or("{}".to_string());
            users.push(User {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                role: row.get(3)?,
                metadata: serde_json::from_str(&meta_str).ok(),
            });
        }
        Ok(users)
    }

    async fn delete_user(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "DELETE FROM users WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn update_user(
        &self,
        id: i64,
        email: Option<String>,
        role: Option<String>,
        metadata: Option<serde_json::Value>,
        password: Option<String>,
    ) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>> {
        let mut sets = Vec::new();
        let mut params: Vec<rusqlite::types::Value> = vec![];
        if let Some(e) = &email {
            sets.push("email = ?");
            params.push(e.as_str().into_val());
        }
        if let Some(r) = &role {
            sets.push("role = ?");
            params.push(r.as_str().into_val());
        }
        if let Some(p) = &password {
            let hash = crate::auth::hash_password(p)?;
            sets.push("password_hash = ?");
            params.push(hash.into_val());
        }
        if let Some(m) = &metadata {
            let m_str = serde_json::to_string(m)?;
            sets.push("metadata = ?");
            params.push(m_str.into_val());
        }

        if sets.is_empty() {
            let conn = self.get_core_read().await;
            let mut stmt = conn
                .prepare("SELECT id,email,password_hash,role,metadata FROM users WHERE id = ?1")?;
            let mut r = stmt.query(params![id])?;
            if let Some(row) = r.next()? {
                let meta_str: String = row.get(4).unwrap_or("{}".to_string());
                return Ok(User {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    password_hash: row.get(2)?,
                    role: row.get(3)?,
                    metadata: serde_json::from_str(&meta_str).ok(),
                });
            }
            return Err("User not found".into());
        }

        params.push(id.into_val());
        let sql = format!("UPDATE users SET {} WHERE id = ?", sets.join(","));
        self.core_batcher
            .execute(sql, params)
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;

        let conn = self.get_core_read().await;
        let mut stmt =
            conn.prepare("SELECT id,email,password_hash,role,metadata FROM users WHERE id = ?1")?;
        let mut r = stmt.query(params![id])?;
        if let Some(row) = r.next()? {
            let meta_str: String = row.get(4).unwrap_or("{}".to_string());
            Ok(User {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                role: row.get(3)?,
                metadata: serde_json::from_str(&meta_str).ok(),
            })
        } else {
            Err("User not found".into())
        }
    }

    async fn get_user_by_oauth(
        &self,
        p: &str,
        pid: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare("SELECT u.id,u.email,u.password_hash,u.role,u.metadata FROM users u JOIN auth_identities ai ON u.id = ai.user_id WHERE ai.provider = ?1 AND ai.provider_id = ?2")?;
        let mut r = stmt.query(params![p, pid])?;
        if let Some(row) = r.next()? {
            let meta_str: String = row.get(4).unwrap_or("{}".to_string());
            Ok(Some(User {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                role: row.get(3)?,
                metadata: serde_json::from_str(&meta_str).ok(),
            }))
        } else {
            Ok(None)
        }
    }
    async fn link_oauth(
        &self,
        uid: i64,
        p: &str,
        pid: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "INSERT INTO auth_identities (user_id,provider,provider_id) VALUES (?1,?2,?3)"
                    .into(),
                vec![uid.into_val(), p.into_val(), pid.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
    async fn create_auth_token(
        &self,
        uid: i64,
        t: &str,
        tk: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher.execute("INSERT INTO auth_tokens (token,user_id,type,expires_at) VALUES (?1,?2,?3,datetime('now','+1 hour'))".into(),vec![tk.into_val(),uid.into_val(),t.into_val()]).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
    async fn consume_auth_token(
        &self,
        tk: &str,
        t: &str,
    ) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>> {
        // Scope the sync DB operations
        let uid = {
            let conn = self.get_core_read().await;
            let mut stmt = conn.prepare("SELECT user_id FROM auth_tokens WHERE token = ?1 AND type = ?2 AND expires_at > datetime('now')")?;
            let mut r = stmt.query(params![tk, t])?;
            if let Some(row) = r.next()? {
                Some(row.get::<_, i64>(0)?)
            } else {
                None
            }
        };

        // Now safe to await
        if let Some(user_id) = uid {
            self.core_batcher
                .execute(
                    "DELETE FROM auth_tokens WHERE token = ?1".into(),
                    vec![tk.into_val()],
                )
                .await
                .map_err(|e| Box::new(std::io::Error::other(e)))?;
            Ok(Some(user_id))
        } else {
            Ok(None)
        }
    }
    async fn set_user_verified(
        &self,
        uid: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "UPDATE users SET is_verified = 1 WHERE id = ?1".into(),
                vec![uid.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
    async fn get_config(
        &self,
        key: &str,
    ) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare("SELECT value FROM _system_config_settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let v_str: String = row.get(0)?;
            Ok(Some(serde_json::from_str(&v_str)?))
        } else {
            Ok(None)
        }
    }
    async fn set_config(
        &self,
        key: &str,
        value: &serde_json::Value,
        encrypted: bool,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let v_str = serde_json::to_string(value)?;
        self.core_batcher.execute(
            "INSERT INTO _system_config_settings (key,value,encrypted,updated_at) VALUES (?1,?2,?3,CURRENT_TIMESTAMP) 
             ON CONFLICT(key) DO UPDATE SET value=excluded.value,encrypted=excluded.encrypted,updated_at=CURRENT_TIMESTAMP".into(),
            vec![key.into_val(),v_str.into_val(),encrypted.into_val()]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
    async fn list_configs(
        &self,
    ) -> std::result::Result<Vec<models::ConfigItem>, Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare(
            "SELECT key,value,encrypted,updated_at FROM _system_config_settings ORDER BY key ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let key: String = row.get(0)?;
            let val_str: String = row.get(1)?;
            let encrypted: bool = row.get(2)?;
            let updated_at: String = row.get(3)?;

            let value = if encrypted {
                Some("******".to_string())
            } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(&val_str) {
                if v.is_string() {
                    Some(v.as_str().unwrap().to_string())
                } else {
                    Some(val_str)
                }
            } else {
                Some(val_str)
            };

            items.push(models::ConfigItem {
                key,
                value,
                encrypted,
                updated_at,
            });
        }
        Ok(items)
    }
    async fn delete_config(
        &self,
        key: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "DELETE FROM _system_config_settings WHERE key = ?1".into(),
                vec![key.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    // --- Files ---
    async fn create_file_metadata(
        &self,
        f: &str,
        o: &str,
        m: &str,
        s: i64,
        u: Option<i64>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.data_batcher.insert("INSERT INTO _storage_files (filename,original_name,mime_type,size,user_id) VALUES (?1,?2,?3,?4,?5)".into(),vec![f.into_val(),o.into_val(),m.into_val(),s.into_val(),u.into_val()]).await.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)
    }
    async fn list_files(
        &self,
        limit: i64,
        offset: i64,
    ) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare("SELECT id,filename,original_name,mime_type,size,created_at FROM _storage_files ORDER BY created_at DESC LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(params![limit, offset])?;
        let mut files = Vec::new();
        while let Some(row) = rows.next()? {
            files.push(models::StoredFile {
                id: row.get(0)?,
                filename: row.get(1)?,
                original_name: row.get(2)?,
                mime_type: row.get(3)?,
                size: row.get(4)?,
                created_at: row.get(5)?,
            });
        }
        Ok(files)
    }
    async fn count_files(
        &self,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM _storage_files")?;
        let mut row = stmt.query([])?;
        if let Some(r) = row.next()? {
            Ok(r.get(0)?)
        } else {
            Ok(0)
        }
    }
    async fn get_file_metadata(
        &self,
        id: i64,
    ) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare("SELECT id,filename,original_name,mime_type,size,created_at FROM _storage_files WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(models::StoredFile {
                id: row.get(0)?,
                filename: row.get(1)?,
                original_name: row.get(2)?,
                mime_type: row.get(3)?,
                size: row.get(4)?,
                created_at: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }
    async fn get_file_by_filename(
        &self,
        filename: &str,
    ) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare("SELECT id,filename,original_name,mime_type,size,created_at FROM _storage_files WHERE filename = ?1")?;
        let mut rows = stmt.query(params![filename])?;
        if let Some(row) = rows.next()? {
            Ok(Some(models::StoredFile {
                id: row.get(0)?,
                filename: row.get(1)?,
                original_name: row.get(2)?,
                mime_type: row.get(3)?,
                size: row.get(4)?,
                created_at: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }
    async fn delete_file_metadata(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };
        self.data_batcher
            .execute(
                "DELETE FROM _storage_files WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(map_err)?;
        Ok(())
    }

    // --- Logs (Use Batcher) ---
    async fn log_audit_event(
        &self,
        level: &str,
        message: &str,
        source: &str,
        meta: Option<serde_json::Value>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let meta_str = serde_json::to_string(&meta).unwrap_or("{}".to_string());
        self.log_batcher
            .execute(
                "INSERT INTO _audit_logs (level,message,source,meta) VALUES (?1,?2,?3,?4)".into(),
                vec![
                    level.into_val(),
                    message.into_val(),
                    source.into_val(),
                    meta_str.into_val(),
                ],
            )
            .await
            .map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;
        Ok(())
    }
    async fn list_audit_logs(
        &self,
    ) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_log_read().await;
        let mut stmt = conn.prepare("SELECT id,level,message,source,meta,timestamp FROM _audit_logs ORDER BY timestamp DESC LIMIT 100")?;
        let mut rows = stmt.query([])?;
        let mut logs = Vec::new();
        while let Some(row) = rows.next()? {
            let meta_str: Option<String> = row.get(4)?;
            let meta =
                meta_str.map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null));
            logs.push(serde_json::json!({ "id": row.get::<usize,i64>(0)?,"level": row.get::<usize,String>(1)?,"message": row.get::<usize,String>(2)?,"source": row.get::<usize,String>(3)?,"meta": meta,"timestamp": row.get::<usize,String>(5)? }));
        }
        Ok(logs)
    }
    async fn log_system_event(
        &self,
        level: &str,
        target: &str,
        message: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.log_batcher
            .execute(
                "INSERT INTO _system_logs (level,target,message) VALUES (?1,?2,?3)".into(),
                vec![level.into_val(), target.into_val(), message.into_val()],
            )
            .await
            .map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;
        Ok(())
    }

    async fn list_paginated_logs(
        &self,
        log_type: &str, // "system" or "audit"
        page: i64,
        per_page: i64,
        level: Option<String>,
        source: Option<String>,
        search: Option<String>,
    ) -> std::result::Result<(Vec<serde_json::Value>, i64), Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_log_read().await;

        let mut where_clauses = Vec::new();
        let mut params: Vec<rusqlite::types::Value> = Vec::new();

        let (table_name, source_col) = if log_type == "audit" {
            ("_audit_logs", "source")
        } else {
            ("_system_logs", "target")
        };

        if let Some(lvl) = level {
            where_clauses.push(format!("{}.level = ?", table_name));
            params.push(lvl.into_val());
        }
        if let Some(src) = source {
            where_clauses.push(format!("{}.{} LIKE ?", table_name, source_col));
            params.push(format!("%{}%", src).into_val());
        }
        if let Some(q) = search {
            where_clauses.push(format!(
                "({}.message LIKE ? OR {}.{} LIKE ?)",
                table_name, table_name, source_col
            ));
            params.push(format!("%{}%", q).into_val());
            params.push(format!("%{}%", q).into_val());
        }

        let where_sql = if where_clauses.is_empty() {
            "".to_string()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        // Get total matching
        let count_sql = format!("SELECT COUNT(*) FROM {} {}", table_name, where_sql);
        let mut count_stmt = conn.prepare(&count_sql)?;
        let total: i64 =
            count_stmt.query_row(rusqlite::params_from_iter(params.clone()), |row| row.get(0))?;

        let limit = per_page;
        let offset = (page - 1) * per_page;

        let mut logs = Vec::new();

        if log_type == "audit" {
            let select_sql = format!(
                "SELECT id,level,message,source,meta,timestamp FROM _audit_logs {} ORDER BY timestamp DESC LIMIT {} OFFSET {}",
                where_sql, limit, offset
            );
            let mut stmt = conn.prepare(&select_sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
            while let Some(row) = rows.next()? {
                let meta_str: Option<String> = row.get(4)?;
                let meta =
                    meta_str.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());

                logs.push(serde_json::json!({
                    "id": row.get::<usize,i64>(0)?.to_string(),
                    "level": row.get::<usize,String>(1)?,
                    "message": row.get::<usize,String>(2)?,
                    "source": row.get::<usize,String>(3)?,
                    "meta": meta,
                    "timestamp": row.get::<usize,String>(5)?
                }));
            }
        } else {
            let select_sql = format!(
                "SELECT id,level,target,message,timestamp FROM _system_logs {} ORDER BY timestamp DESC LIMIT {} OFFSET {}",
                where_sql, limit, offset
            );
            let mut stmt = conn.prepare(&select_sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
            while let Some(row) = rows.next()? {
                logs.push(serde_json::json!({
                    "id": row.get::<usize,i64>(0)?.to_string(),
                    "level": row.get::<usize,String>(1)?,
                    "source": row.get::<usize,String>(2)?,
                    "message": row.get::<usize,String>(3)?,
                    "timestamp": row.get::<usize,String>(4)?
                }));
            }
        }

        Ok((logs, total))
    }

    // --- System DB ---
    async fn list_ai_actions(
        &self,
    ) -> std::result::Result<Vec<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_sys_read().await;
        let mut stmt = conn
            .prepare("SELECT id,slug,name,model,system_prompt,template,config FROM _ai_actions")?;
        let mut rows = stmt.query([])?;
        let mut res = Vec::new();
        while let Some(row) = rows.next()? {
            let conf_str: String = row.get(6).unwrap_or("{}".to_string());
            res.push(ai_models::AiAction {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                model: row.get(3)?,
                system_prompt: row.get(4)?,
                template: row.get(5)?,
                config: serde_json::from_str(&conf_str).unwrap_or_default(),
            });
        }
        Ok(res)
    }
    async fn get_ai_action(
        &self,
        slug: &str,
    ) -> std::result::Result<Option<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare("SELECT id,slug,name,model,system_prompt,template,config FROM _ai_actions WHERE slug = ?1")?;
        let mut rows = stmt.query(params![slug])?;
        if let Some(row) = rows.next()? {
            let conf_str: String = row.get(6).unwrap_or("{}".to_string());
            Ok(Some(ai_models::AiAction {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                model: row.get(3)?,
                system_prompt: row.get(4)?,
                template: row.get(5)?,
                config: serde_json::from_str(&conf_str).unwrap_or_default(),
            }))
        } else {
            Ok(None)
        }
    }
    async fn create_ai_action(
        &self,
        action: ai_models::CreateActionReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let config_str = serde_json::to_string(&action.config.unwrap_or(serde_json::json!({})))?;
        let id = self.sys_batcher.insert(
            "INSERT INTO _ai_actions (slug,name,model,system_prompt,template,config) VALUES (?1,?2,?3,?4,?5,?6)".into(),
            vec![
                action.slug.into_val(),
                action.name.into_val(),
                action.model.into_val(),
                action.system_prompt.into_val(),
                action.template.into_val(),
                config_str.into_val()
            ]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(id)
    }
    async fn delete_ai_action(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher
            .execute(
                "DELETE FROM _ai_actions WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
    async fn create_ai_session(
        &self,
        s: &AiSession,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher.execute(
            "INSERT INTO _ai_sessions (id,name,messages,current_manifest,pending_manifest,diff_summary,last_error,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)".into(),
            vec![
                s.id.clone().into_val(),
                s.name.clone().into_val(),
                serde_json::to_string(&s.messages)?.into_val(),
                serde_json::to_string(&s.current_manifest)?.into_val(),
                serde_json::to_string(&s.pending_manifest)?.into_val(),
                s.diff_summary.clone().into_val(),
                s.last_error.clone().into_val(),
                s.created_at.clone().into_val()
            ]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
    async fn get_ai_session(
        &self,
        id: &str,
    ) -> std::result::Result<Option<AiSession>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare("SELECT id,name,messages,current_manifest,pending_manifest,diff_summary,last_error,created_at FROM _ai_sessions WHERE id = ?1")?;
        let mut r = stmt.query(params![id])?;

        if let Some(row) = r.next()? {
            let m_str: String = row.get(2)?;
            let man_str: Option<String> = row.get(3)?;
            let pend_str: Option<String> = row.get(4).unwrap_or(None);

            Ok(Some(AiSession {
                id: row.get(0)?,
                name: row.get(1)?,
                messages: serde_json::from_str(&m_str)?,
                current_manifest: match man_str {
                    Some(s) => serde_json::from_str(&s).ok(),
                    None => None,
                },
                pending_manifest: match pend_str {
                    Some(s) => serde_json::from_str(&s).ok(),
                    None => None,
                },
                diff_summary: row.get(5).unwrap_or(None),
                last_error: row.get(6).unwrap_or(None),
                created_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn update_ai_session(
        &self,
        s: &AiSession,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher.execute(
            "UPDATE _ai_sessions SET messages = ?1,current_manifest = ?2,pending_manifest = ?3,diff_summary = ?4,last_error = ?5 WHERE id = ?6".into(),
            vec![
                serde_json::to_string(&s.messages)?.into_val(),
                serde_json::to_string(&s.current_manifest)?.into_val(),
                serde_json::to_string(&s.pending_manifest)?.into_val(),
                s.diff_summary.clone().into_val(),
                s.last_error.clone().into_val(),
                s.id.clone().into_val()
            ]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn list_ai_sessions(
        &self,
    ) -> std::result::Result<Vec<AiSession>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare("SELECT id,name,messages,current_manifest,pending_manifest,diff_summary,last_error,created_at FROM _ai_sessions ORDER BY created_at DESC")?;
        let mut r = stmt.query([])?;
        let mut s = Vec::new();
        while let Some(row) = r.next()? {
            let m_str: String = row.get(2)?;
            let man_str: Option<String> = row.get(3)?;
            let pend_str: Option<String> = row.get(4).unwrap_or(None);

            s.push(AiSession {
                id: row.get(0)?,
                name: row.get(1)?,
                messages: serde_json::from_str(&m_str).unwrap_or_default(),
                current_manifest: match man_str {
                    Some(str) => serde_json::from_str(&str).ok(),
                    None => None,
                },
                pending_manifest: match pend_str {
                    Some(str) => serde_json::from_str(&str).ok(),
                    None => None,
                },
                diff_summary: row.get(5).unwrap_or(None),
                last_error: row.get(6).unwrap_or(None),
                created_at: row.get(7)?,
            });
        }
        Ok(s)
    }

    async fn delete_ai_session(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher
            .execute(
                "DELETE FROM _ai_sessions WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn save_plugin(
        &self,
        p: &Plugin,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher.execute("INSERT INTO _plugins (id,name,version,manifest,description) VALUES (?1,?2,?3,?4,?5)".into(),vec![p.id.clone().into_val(),p.name.clone().into_val(),p.version.clone().into_val(),serde_json::to_string(&p.manifest)?.into_val(),p.description.clone().into_val()]).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
    async fn list_plugins(
        &self,
    ) -> std::result::Result<Vec<Plugin>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,name,version,manifest,description FROM _plugins ORDER BY created_at DESC",
        )?;
        let mut r = stmt.query([])?;
        let mut p = Vec::new();
        while let Some(row) = r.next()? {
            let m_str: String = row.get(3)?;
            p.push(Plugin {
                id: row.get(0)?,
                name: row.get(1)?,
                version: row.get(2)?,
                manifest: serde_json::from_str(&m_str)?,
                description: row.get(4)?,
            });
        }
        Ok(p)
    }
    async fn list_scripts(
        &self,
    ) -> std::result::Result<
        Vec<crate::models::script::Script>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,name,trigger_type,code,active,target_collection,visibility FROM _scripts",
        )?;
        let mut r = stmt.query([])?;
        let mut v = Vec::new();
        while let Some(row) = r.next()? {
            v.push(crate::models::script::Script {
                id: row.get(0)?,
                name: row.get(1)?,
                trigger_type: row.get(2)?,
                code: row.get(3)?,
                active: row.get(4)?,
                target_collection: row.get(5)?,
                visibility: row.get(6).unwrap_or("private".to_string()),
            });
        }
        Ok(v)
    }

    async fn create_script(
        &self,
        req: crate::models::script::CreateScriptReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let id = self.sys_batcher.insert(
            "INSERT INTO _scripts (name,trigger_type,code,target_collection,visibility,active) VALUES (?1,?2,?3,?4,?5,?6) 
             ON CONFLICT(name) DO UPDATE SET trigger_type=excluded.trigger_type,code=excluded.code,target_collection=excluded.target_collection,visibility=excluded.visibility,active=excluded.active,created_at=CURRENT_TIMESTAMP".into(),
            vec![req.name.into_val(),req.trigger_type.into_val(),req.code.into_val(),req.target_collection.into_val(),req.visibility.into_val(),req.active.into_val()]
        ).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(id) // Note: this might return 0 on UPDATE due to SQLite last_insert_rowid quirks if replacing
    }

    async fn get_script_by_name(
        &self,
        name: &str,
    ) -> std::result::Result<
        Option<crate::models::script::Script>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare("SELECT id,name,trigger_type,code,active,target_collection,visibility FROM _scripts WHERE name = ?1")?;
        let mut r = stmt.query(params![name])?;
        if let Some(row) = r.next()? {
            Ok(Some(crate::models::script::Script {
                id: row.get(0)?,
                name: row.get(1)?,
                trigger_type: row.get(2)?,
                code: row.get(3)?,
                active: row.get(4)?,
                target_collection: row.get(5)?,
                visibility: row.get(6).unwrap_or("private".to_string()),
            }))
        } else {
            Ok(None)
        }
    }

    async fn delete_script(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher
            .execute(
                "DELETE FROM _scripts WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn get_scripts_by_trigger(
        &self,
        t: &str,
    ) -> std::result::Result<
        Vec<crate::models::script::Script>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare("SELECT id,name,trigger_type,code,active,target_collection,visibility FROM _scripts WHERE trigger_type = ?1 AND active = 1")?;
        let mut r = stmt.query(params![t])?;
        let mut v = Vec::new();
        while let Some(row) = r.next()? {
            v.push(crate::models::script::Script {
                id: row.get(0)?,
                name: row.get(1)?,
                trigger_type: row.get(2)?,
                code: row.get(3)?,
                active: row.get(4)?,
                target_collection: row.get(5)?,
                visibility: row.get(6).unwrap_or("private".to_string()),
            });
        }
        Ok(v)
    }
    async fn list_templates(
        &self,
    ) -> std::result::Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_sys_read().await;
        let mut stmt =
            conn.prepare("SELECT id,slug,content,script_id,created_at FROM _templates")?;
        let mut r = stmt.query([])?;
        let mut v = Vec::new();
        while let Some(row) = r.next()? {
            v.push(models::Template {
                id: row.get(0)?,
                slug: row.get(1)?,
                content: row.get(2)?,
                script_id: row.get(3)?,
                created_at: row.get(4)?,
            });
        }
        Ok(v)
    }
    async fn get_template_by_slug(
        &self,
        s: &str,
    ) -> std::result::Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>>
    {
        let conn = self.get_sys_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,slug,content,script_id,created_at FROM _templates WHERE slug = ?1",
        )?;
        let mut r = stmt.query(params![s])?;
        if let Some(row) = r.next()? {
            Ok(Some(models::Template {
                id: row.get(0)?,
                slug: row.get(1)?,
                content: row.get(2)?,
                script_id: row.get(3)?,
                created_at: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }
    async fn create_template(
        &self,
        req: models::CreateTemplateReq,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let id = self.sys_batcher.insert("INSERT INTO _templates (slug,content,script_id) VALUES (?1,?2,?3) ON CONFLICT(slug) DO UPDATE SET content=excluded.content,script_id=excluded.script_id,created_at=CURRENT_TIMESTAMP".into(),vec![req.slug.into_val(),req.content.into_val(),req.script_id.into_val()]).await.map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(id)
    }
    async fn update_template(
        &self,
        id: i64,
        content: String,
        script_id: Option<i64>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher
            .execute(
                "UPDATE _templates SET content = ?1,script_id = ?2 WHERE id = ?3".into(),
                vec![content.into_val(), script_id.into_val(), id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }
    async fn delete_template(
        &self,
        id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sys_batcher
            .execute(
                "DELETE FROM _templates WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn create_relation(
        &self,
        oc: i64,
        oi: i64,
        tc: i64,
        ti: i64,
        rn: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.data_batcher.execute("INSERT INTO _relations (origin_col_id,origin_rec_id,target_col_id,target_rec_id,rel_name) VALUES (?1,?2,?3,?4,?5)".into(),vec![oc.into_val(),oi.into_val(),tc.into_val(),ti.into_val(),rn.into_val()]).await.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }
    async fn delete_relation(
        &self,
        oc: i64,
        oi: i64,
        tc: i64,
        ti: i64,
        rn: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };
        self.data_batcher.execute("DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND target_col_id=?3 AND target_rec_id=?4 AND rel_name=?5".into(),vec![oc.into_val(),oi.into_val(),tc.into_val(),ti.into_val(),rn.into_val()]).await.map_err(map_err)?;
        Ok(())
    }
    async fn get_related_ids(
        &self,
        oc: i64,
        oi: i64,
        rn: &str,
    ) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare("SELECT target_col_id,target_rec_id FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND rel_name=?3")?;
        let mut rows = stmt.query(params![oc, oi, rn])?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            results.push((row.get(0)?, row.get(1)?));
        }
        Ok(results)
    }
    async fn get_records_by_ids(
        &self,
        collection_id: i64,
        ids: &[i64],
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let id_list = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id,json(data),NULL,created,updated FROM records WHERE collection_id = ? AND id IN ({})",
            id_list
        );
        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params![collection_id])?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(row_to_record(row)?);
        }
        Ok(records)
    }

    // --- Tenants ---
    async fn register_tenant(
        &self,
        id: &str,
        owner_id: Option<i64>,
        name: Option<String>,
        tier: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "INSERT INTO _tenants (id,owner_id,name,tier) VALUES (?1,?2,?3,?4)
             ON CONFLICT(id) DO UPDATE SET 
                owner_id=excluded.owner_id,
                name=excluded.name,
                tier=excluded.tier,
                updated_at=CURRENT_TIMESTAMP"
                    .into(),
                vec![
                    id.into_val(),
                    owner_id.into_val(),
                    name.into_val(),
                    tier.unwrap_or("free".to_string()).into_val(),
                ],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn get_tenant_status(
        &self,
        tenant_id: &str,
    ) -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare("SELECT status FROM _tenants WHERE id = ?1")?;
        let mut rows = stmt.query(params![tenant_id])?;

        if let Some(row) = rows.next()? {
            let status: String = row.get(0)?;
            Ok(status)
        } else {
            Ok("not_found".to_string())
        }
    }

    async fn update_tenant_status(
        &self,
        tenant_id: &str,
        status: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "UPDATE _tenants SET status = ?1,updated_at = CURRENT_TIMESTAMP WHERE id = ?2"
                    .into(),
                vec![status.into_val(), tenant_id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn update_tenant_full(
        &self,
        id: &str,
        name: Option<String>,
        status: Option<String>,
        tier: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut sets = Vec::new();
        let mut params: Vec<rusqlite::types::Value> = vec![];

        if let Some(n) = name {
            sets.push("name = ?");
            params.push(n.into_val());
        }
        if let Some(s) = status {
            sets.push("status = ?");
            params.push(s.into_val());
        }
        if let Some(t) = tier {
            sets.push("tier = ?");
            params.push(t.into_val());
        }

        if sets.is_empty() {
            return Ok(());
        }

        sets.push("updated_at = CURRENT_TIMESTAMP");
        params.push(id.to_string().into_val());

        let sql = format!("UPDATE _tenants SET {} WHERE id = ?", sets.join(","));
        self.core_batcher
            .execute(sql, params)
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn delete_tenant_metadata(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "DELETE FROM _tenants WHERE id = ?1".into(),
                vec![id.to_string().into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn list_tenants(
        &self,
    ) -> std::result::Result<Vec<models::Tenant>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare(
            "SELECT id,name,status,tier,max_storage_mb,current_storage_mb,max_vectors,current_vectors,max_ai_requests,current_ai_requests,created_at 
             FROM _tenants 
             ORDER BY created_at DESC"
        )?;
        let mut rows = stmt.query([])?;

        let mut tenants = Vec::new();
        while let Some(row) = rows.next()? {
            tenants.push(models::Tenant {
                id: row.get(0)?,
                name: row.get(1).unwrap_or(None),
                status: row.get(2)?,
                tier: row.get(3)?,
                stats: models::TenantStats {
                    storage_mb: row.get(5)?,
                    max_storage_mb: row.get(4)?,
                    vector_count: row.get(7)?,
                    max_vectors: row.get(6)?,
                    ai_requests: row.get(9)?,
                    max_ai_requests: row.get(8)?,
                },
                created_at: row.get(10)?,
            });
        }
        Ok(tenants)
    }

    async fn get_tenant_disk_usage(
        &self,
        tenant_id: &str,
    ) -> std::result::Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let path = format!("storage/tenants/{}", tenant_id);

        let size =
            tokio::task::spawn_blocking(move || calculate_dir_size(std::path::Path::new(&path)))
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })??;

        Ok(size)
    }

    // --- Sandboxes ---
    async fn register_sandbox(
        &self,
        id: &str,
        owner_id: Option<i64>,
        name: Option<String>,
        expires_at: Option<String>,
        scope: &str,
        tenant_id: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "INSERT INTO _sandboxes (id,owner_id,name,expires_at,scope,tenant_id) VALUES (?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(id) DO UPDATE SET 
                    owner_id=excluded.owner_id,
                    name=excluded.name,
                    expires_at=excluded.expires_at,
                    scope=excluded.scope,
                    tenant_id=excluded.tenant_id"
                    .into(),
                vec![
                    id.into_val(),
                    owner_id.into_val(),
                    name.into_val(),
                    expires_at.into_val(),
                    scope.into_val(),
                    tenant_id.into_val(),
                ],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn list_sandboxes(
        &self,
        tenant_id: Option<String>,
    ) -> std::result::Result<
        Vec<crate::models::SandboxMetadata>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let conn = self.get_core_read().await;
        let mut sql = "SELECT id,name,status,expires_at,scope,tenant_id,current_storage_mb,max_storage_mb FROM _sandboxes".to_string();
        let mut params: Vec<rusqlite::types::Value> = vec![];

        if let Some(tid) = tenant_id {
            // Tenant sees ONLY their own sandboxes
            sql.push_str(" WHERE tenant_id = ?1 ORDER BY created_at DESC");
            params.push(tid.into_val());
        } else {
            // Root sees ONLY root sandboxes (tenant_id IS NULL)
            sql.push_str(" WHERE tenant_id IS NULL ORDER BY created_at DESC");
        }

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
        let mut res = vec![];

        while let Some(row) = rows.next()? {
            res.push(crate::models::SandboxMetadata {
                id: row.get(0)?,
                name: row.get(1)?,
                status: row.get(2)?,
                expires_at: row.get(3)?,
                scope: row.get(4).unwrap_or("root".to_string()),
                tenant_id: row.get(5).unwrap_or(None),
                current_storage_mb: row.get(6).unwrap_or(0.0),
                max_storage_mb: row.get(7).unwrap_or(100),
            });
        }
        Ok(res)
    }

    async fn update_sandbox_full(
        &self,
        id: &str,
        name: Option<String>,
        status: Option<String>,
        expires_at: Option<String>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut sets = Vec::new();
        let mut params: Vec<rusqlite::types::Value> = vec![];

        if let Some(n) = name {
            sets.push("name = ?");
            params.push(n.into_val());
        }
        if let Some(s) = status {
            sets.push("status = ?");
            params.push(s.into_val());
        }
        if let Some(e) = expires_at {
            sets.push("expires_at = ?");
            params.push(e.into_val());
        }

        if sets.is_empty() {
            return Ok(());
        }

        params.push(id.to_string().into_val());
        let sql = format!("UPDATE _sandboxes SET {} WHERE id = ?", sets.join(","));
        self.core_batcher
            .execute(sql, params)
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn delete_sandbox_metadata(
        &self,
        id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "DELETE FROM _sandboxes WHERE id = ?1".into(),
                vec![id.to_string().into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn get_sandbox_disk_usage(
        &self,
        sandbox_id: &str,
    ) -> std::result::Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let path = format!("storage/sandboxes/session_{}", sandbox_id);

        let size =
            tokio::task::spawn_blocking(move || calculate_dir_size(std::path::Path::new(&path)))
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })??;

        Ok(size)
    }

    // --- Dashboard ---
    async fn get_dashboard_stats(
        &self,
    ) -> std::result::Result<DashboardData, Box<dyn std::error::Error + Send + Sync>> {
        let data_conn = self.get_data_read().await;
        let log_conn = self.get_log_read().await;
        let sys_conn = self.get_sys_read().await;
        let vec_conn = self.get_vector_read().await;

        let mut stmt1 = data_conn.prepare("SELECT COUNT(*) FROM collections")?;
        let mut row1 = stmt1.query([])?;
        let collections_count: i64 = if let Some(r) = row1.next()? {
            r.get(0)?
        } else {
            0
        };

        let mut stmt2 = data_conn.prepare("SELECT COUNT(*) FROM records")?;
        let mut row2 = stmt2.query([])?;
        let total_records: i64 = if let Some(r) = row2.next()? {
            r.get(0)?
        } else {
            0
        };

        let mut stmt3 = vec_conn.prepare("SELECT COUNT(*) FROM vectors")?;
        let mut row3 = stmt3.query([])?;
        let total_vectors: i64 = if let Some(r) = row3.next()? {
            r.get(0)?
        } else {
            0
        };

        let mut total_bytes: i64 = 0;

        for conn in [&data_conn, &log_conn, &sys_conn, &vec_conn] {
            let mut stmt_c = conn.prepare("PRAGMA page_count")?;
            let mut p_count = stmt_c.query([])?;
            let count: i64 = if let Some(r) = p_count.next()? {
                r.get(0)?
            } else {
                0
            };

            let mut stmt_s = conn.prepare("PRAGMA page_size")?;
            let mut p_size = stmt_s.query([])?;
            let size: i64 = if let Some(r) = p_size.next()? {
                r.get(0)?
            } else {
                0
            };

            total_bytes += count * size;
        }

        let db_size_mb = (total_bytes as f64 / 1024.0 / 1024.0 * 100.0).round() / 100.0;

        let indexes_size_mb =
            (calculate_dir_size(std::path::Path::new(&format!("{}/indexes", self.base_path)))
                .unwrap_or(0) as f64
                / 1024.0
                / 1024.0
                * 100.0)
                .round()
                / 100.0;

        let sql_chart = "
            SELECT 
                strftime('%Y-%m-%d',timestamp) as day_date,
                COUNT(*) as req_count,
                SUM(CASE WHEN level = 'ERROR' OR level = 'error' THEN 1 ELSE 0 END) as err_count 
            FROM _system_logs 
            WHERE timestamp >= date('now','-7 days') 
            GROUP BY day_date
        ";

        let mut stmt_chart = log_conn.prepare(sql_chart)?;
        let mut rows = stmt_chart.query([])?;
        let mut daily_stats: HashMap<String, (i64, i64)> = HashMap::new();
        let mut total_requests = 0;

        while let Some(row) = rows.next()? {
            let date_str: String = row.get(0)?;
            let reqs: i64 = row.get(1)?;
            let errs: i64 = row.get(2)?;
            total_requests += reqs;
            daily_stats.insert(date_str, (reqs, errs));
        }

        let mut chart_data: Vec<ChartPoint> = Vec::new();
        let now = Utc::now();
        for i in (0..7).rev() {
            let date = now - chrono::Duration::days(i);
            let date_key = date.format("%Y-%m-%d").to_string();
            let day_name = date.format("%a").to_string();
            let (reqs, errs) = daily_stats.get(&date_key).unwrap_or(&(0, 0));
            chart_data.push(ChartPoint {
                name: day_name,
                requests: *reqs,
                errors: *errs,
            });
        }

        let mut stmt_logs = log_conn.prepare("SELECT id,level,message,target,timestamp FROM _system_logs ORDER BY timestamp DESC LIMIT 10")?;
        let mut recent_rows = stmt_logs.query([])?;

        let mut recent_logs = Vec::new();
        while let Some(row) = recent_rows.next()? {
            recent_logs.push(serde_json::json!({
                "id": row.get::<usize,i64>(0)?.to_string(),
                "level": row.get::<usize,String>(1)?,
                "message": row.get::<usize,String>(2)?,
                "source": row.get::<usize,String>(3)?,
                "timestamp": row.get::<usize,String>(4)?
            }));
        }

        Ok(DashboardData {
            stats: DashboardStats {
                total_requests,
                db_size_mb,
                collections_count,
                total_records,
                total_vectors,
                indexes_size_mb,
            },
            chart: chart_data,
            recent_logs,
        })
    }

    async fn save_vector(
        &self,
        collection_id: i64,
        record_id: i64,
        field_name: &str,
        vector: Vec<f32>,
        model: &str,
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        let vec_json = serde_json::to_string(&vector)?;
        let map_err = |e: String| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(std::io::Error::other(e))
        };

        self.vector_batcher
            .insert(
                "INSERT INTO vectors (collection_id,record_id,field_name,vector,model) 
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(collection_id,record_id,field_name,model) 
             DO UPDATE SET vector = excluded.vector,created_at = CURRENT_TIMESTAMP"
                    .into(),
                vec![
                    collection_id.into_val(),
                    record_id.into_val(),
                    field_name.into_val(),
                    vec_json.into_val(),
                    model.into_val(),
                ],
            )
            .await
            .map_err(map_err)?;

        Ok(())
    }

    async fn has_vector(
        &self,
        collection_id: i64,
        record_id: i64,
        field_name: &str,
        model: &str,
    ) -> std::result::Result<bool, Box<dyn StdError + Send + Sync>> {
        let conn = self.get_vector_read().await;
        let mut stmt = conn.prepare("SELECT 1 FROM vectors WHERE collection_id = ?1 AND record_id = ?2 AND field_name = ?3 AND model = ?4")?;
        let mut rows = stmt.query(params![collection_id, record_id, field_name, model])?;

        Ok(rows.next()?.is_some())
    }

    async fn get_record_vectors(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<Vec<models::VectorRecord>, Box<dyn StdError + Send + Sync>> {
        let conn = self.get_vector_read().await;
        let mut stmt = conn.prepare("SELECT field_name,vector,model FROM vectors WHERE collection_id = ?1 AND record_id = ?2")?;
        let mut rows = stmt.query(params![collection_id, record_id])?;

        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let field_name: String = row.get(0)?;

            let val = row
                .get_ref(1)
                .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;

            let vector_blob: Vec<u8> = match val {
                rusqlite::types::ValueRef::Blob(b) => b.to_vec(),
                rusqlite::types::ValueRef::Text(s) => s.to_vec(),
                _ => vec![],
            };

            let model: String = row.get(2)?;

            if !vector_blob.is_empty() {
                let vector: Vec<f32> = serde_json::from_slice(&vector_blob).unwrap_or_default();
                results.push(models::VectorRecord {
                    field_name,
                    vector,
                    model,
                });
            }
        }
        Ok(results)
    }

    async fn get_vectors_for_collection(
        &self,
        collection_id: i64,
        model: &str,
    ) -> std::result::Result<Vec<(i64, String, Vec<f32>)>, Box<dyn StdError + Send + Sync>> {
        let conn = self.get_vector_read().await;
        let mut stmt = conn.prepare("SELECT record_id,field_name,vector FROM vectors WHERE collection_id = ?1 AND model = ?2")?;
        let mut rows = stmt.query(params![collection_id, model.to_string()])?;

        let mut vectors = Vec::new();
        while let Some(row) = rows.next()? {
            let record_id: i64 = row.get(0)?;
            let field_name: String = row.get(1)?;
            let vector_json_str: String = row.get(2)?;

            if let Ok(vector) = serde_json::from_str::<Vec<f32>>(&vector_json_str) {
                vectors.push((record_id, field_name, vector));
            }
        }
        Ok(vectors)
    }

    async fn search_vector(
        &self,
        collection_id: i64,
        field: &str,
        vector: Vec<f32>,
        limit: usize,
    ) -> std::result::Result<Vec<(Record, f32)>, Box<dyn std::error::Error + Send + Sync>> {
        let results = self
            .vector_provider
            .search(collection_id, field, &vector, limit)
            .await
            .map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            })?;

        if results.is_empty() {
            return Ok(vec![]);
        }

        let ids: Vec<i64> = results.iter().map(|(id, _score)| *id).collect();
        let records = self.get_records_by_ids(collection_id, &ids).await?;

        // Restore the correct AI sorting order by mapping the DB records back to the HNSW scores
        let mut id_to_record: std::collections::HashMap<i64, Record> =
            records.into_iter().map(|r| (r.id, r)).collect();

        let mut final_results = Vec::new();
        for (id, distance_score) in results {
            if let Some(rec) = id_to_record.remove(&id) {
                final_results.push((rec, distance_score));
            }
        }

        Ok(final_results)
    }

    async fn query_engine(
        &self,
        query: ApexQuery,
    ) -> std::result::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let (sql, params, pipeline) = crate::query::QueryBuilder::build(&query, self).await?;

        let conn = self.get_data_read().await;
        let mut stmt = conn.prepare(&sql)?;
        let column_names: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        let mut rows = stmt.query(rusqlite::params_from_iter(params))?;

        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            let mut row_map = serde_json::Map::new();

            for (i, name) in column_names.iter().enumerate() {
                let val = row.get_ref(i)?;
                let json_val = match val {
                    rusqlite::types::ValueRef::Integer(i) => serde_json::json!(i),
                    rusqlite::types::ValueRef::Real(f) => serde_json::json!(f),
                    rusqlite::types::ValueRef::Text(s) => {
                        let text = std::str::from_utf8(s).unwrap_or("");
                        if (text.starts_with('{') && text.ends_with('}'))
                            || (text.starts_with('[') && text.ends_with(']'))
                        {
                            serde_json::from_str(text).unwrap_or(serde_json::json!(text))
                        } else {
                            serde_json::json!(text)
                        }
                    }
                    rusqlite::types::ValueRef::Blob(b) => {
                        serde_json::json!(crate::utils::to_hex(b))
                    }
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                };
                row_map.insert(name.clone(), json_val);
            }
            results.push(serde_json::Value::Object(row_map));
        }

        if !pipeline.is_empty() {
            results = crate::query::QueryProcessor::process(results, pipeline)?;
        }

        Ok(serde_json::Value::Array(results))
    }

    async fn reload_connections(
        &self,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut core = self.hot_conn_core.lock().await;
        *core = Connection::open(format!("{}/core.db", self.base_path))?;
        apply_pragmas(&core)?;

        let mut data = self.hot_conn_data.lock().await;
        *data = Connection::open(format!("{}/data.db", self.base_path))?;
        apply_pragmas(&data)?;

        let mut log = self.hot_conn_log.lock().await;
        *log = Connection::open(format!("{}/logs.db", self.base_path))?;
        apply_pragmas(&log)?;

        let mut sys = self.hot_conn_sys.lock().await;
        *sys = Connection::open(format!("{}/system.db", self.base_path))?;
        apply_pragmas(&sys)?;

        let mut vec = self.hot_conn_vec.lock().await;
        *vec = Connection::open(format!("{}/vectors.db", self.base_path))?;
        apply_pragmas(&vec)?;

        Ok(())
    }
}
