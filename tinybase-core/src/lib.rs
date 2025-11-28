// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/lib.rs start here ===========================
use crate::auth::User;
use crate::query::QueryOptions;
use crate::schema::CollectionSchema;
use async_trait::async_trait;
use libsql::{params, Builder, Connection, Database, Result, Row};
use search::SearchManager;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

// --- Modules ---
pub mod auth;
pub mod cache;
pub mod events;
pub mod jobs;
pub mod models;
pub mod policies;
pub mod query;
pub mod realtime;
pub mod schema;
pub mod search;
pub mod security;
pub mod storage;
pub mod validation;
pub mod ai_models; 
pub mod script_models;
pub mod scripting;

#[derive(Debug, Clone)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub schema: Option<CollectionSchema>,
}

#[derive(Debug, Clone)]
pub struct Record {
    pub id: i64,
    pub data: Value,
}

#[async_trait]
pub trait Db: Send + Sync {
    // --- Collections ---
    async fn create_collection(
        &self,
        name: &str,
        schema: &Option<CollectionSchema>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    async fn get_collection(
        &self,
        id: i64,
    ) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>>;

    async fn list_collections(
        &self,
    ) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>>;

    async fn update_collection(
        &self,
        id: i64,
        name: Option<String>,
        schema: Option<CollectionSchema>,
    ) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>>;

    async fn delete_collection(&self, id: i64) -> Result<()>;

    // --- Records ---
    async fn create_record(
        &self,
        collection_id: i64,
        data: &Value,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    async fn list_records(
        &self,
        collection_id: i64,
        options: QueryOptions,
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;

    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>>;

    async fn update_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>>;

    async fn delete_record(&self, collection_id: i64, record_id: i64) -> Result<()>;

    // --- Search ---
    async fn search_records(
        &self,
        collection_id: i64,
        query: &str,
    ) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;

    async fn instant_search(
        &self, 
        collection_id: i64, 
        query: &str
    ) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>>;

    // --- Users (Auth) ---
    async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        role: &str,
    ) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>>;

    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;

    async fn list_users(&self) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_user(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Storage Metadata ---
    async fn create_file_metadata(
        &self,
        filename: &str,
        original_name: &str,
        mime_type: &str,
        size: i64,
        user_id: Option<i64>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;

    async fn list_files(&self, limit: i64, offset: i64) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_file_metadata(&self, id: i64) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_file_metadata(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Advanced Auth ---
    async fn get_user_by_oauth(
        &self,
        provider: &str,
        provider_id: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;

    async fn link_oauth(
        &self,
        user_id: i64,
        provider: &str,
        provider_id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn create_auth_token(
        &self,
        user_id: i64,
        token_type: &str,
        token: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn consume_auth_token(
        &self,
        token: &str,
        token_type: &str,
    ) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>>;

    async fn set_user_verified(
        &self,
        user_id: i64,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Secure Config ---
    async fn set_system_config(
        &self,
        key: &str,
        value: &security::EncryptedValue,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn get_system_config(
        &self,
        key: &str,
    ) -> std::result::Result<Option<security::EncryptedValue>, Box<dyn std::error::Error + Send + Sync>>;

    // --- Settings (Robust JSON) ---
    async fn get_setting(&self, key: &str) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;
    async fn save_setting(&self, key: &str, value: serde_json::Value, encrypt: bool) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Audit Logs ---
    async fn log_audit_event(&self, level: &str, message: &str, source: &str, meta: Option<serde_json::Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn list_audit_logs(&self) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;

    // --- AI Actions ---
    async fn list_ai_actions(&self) -> std::result::Result<Vec<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_ai_action(&self, slug: &str) -> std::result::Result<Option<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>>;
    async fn create_ai_action(&self, action: ai_models::CreateActionReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_ai_action(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    // --- Relations ---
    async fn create_relation(
        &self,
        origin_col: i64,
        origin_id: i64,
        target_col: i64,
        target_id: i64,
        rel_name: &str
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn delete_relation(
        &self,
        origin_col: i64,
        origin_id: i64,
        target_col: i64,
        target_id: i64,
        rel_name: &str
    ) -> Result<()>;

    async fn get_related_ids(
        &self,
        origin_col: i64,
        origin_id: i64,
        rel_name: &str
    ) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>>;

    async fn get_records_by_ids(&self, collection_id: i64, ids: &[i64]) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>>;

    // Scripts
    async fn list_scripts(&self) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>>;
    async fn create_script(&self, req: script_models::CreateScriptReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_script(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_script_by_name(&self, name: &str) -> std::result::Result<Option<script_models::Script>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_scripts_by_trigger(&self, trigger: &str) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>>;

    // Templates
    async fn list_templates(&self) -> std::result::Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>>;
    async fn get_template_by_slug(&self, slug: &str) -> std::result::Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>>;
    async fn create_template(&self, req: models::CreateTemplateReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>>;
    async fn update_template(&self, id: i64, content: String, script_id: Option<i64>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_template(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

fn row_to_collection(
    row: &Row,
) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
    let schema_str: Option<String> = row.get(2)?;
    let schema = match schema_str {
        Some(s) => serde_json::from_str(&s)?,
        None => None,
    };
    Ok(Collection {
        id: row.get(0)?,
        name: row.get(1)?,
        schema,
    })
}

fn row_to_record(
    row: &Row,
) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
    let data_str: String = row.get(1)?;
    let data = serde_json::from_str(&data_str)?;
    Ok(Record {
        id: row.get(0)?,
        data,
    })
}

// --- The Orchestrator: TinyBase ---
pub struct TinyBase {
    db: Database,
    search: Arc<SearchManager>,
}

impl TinyBase {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            search: Arc::new(SearchManager::new("./tantivy_indexes")),
        }
    }
    
    async fn ensure_search_index(&self, collection_id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(col) = self.get_collection(collection_id).await? {
            if let Some(schema) = &col.schema {
                if schema.fields.values().any(|f| f.indexed) {
                    self.search.load_index(collection_id, schema)?;
                }
            }
        }
        Ok(())
    }

    // Helper to sync JSON fields to the _relations table
    async fn sync_relations(
        &self, 
        conn: &Connection, 
        collection_id: i64, 
        record_id: i64, 
        data: &Value
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut rows = conn.query("SELECT schema FROM collections WHERE id = ?1", params![collection_id]).await?;
    
        if let Some(row) = rows.next().await? {
            let schema_str: Option<String> = row.get(0)?;
            if let Some(s) = schema_str {
                let schema: CollectionSchema = serde_json::from_str(&s).unwrap_or_default();

                for (rel_name, rel_def) in schema.relations {
                    
                    if let Some(val) = data.get(&rel_name) {
                        
                        // 1. Cleanup existing relations for this field
                        conn.execute(
                            "DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND rel_name=?3",
                            params![collection_id, record_id, rel_name.clone()]
                        ).await?;

                        // 2. Parse Target Record ID
                        let target_rec_id = match val {
                            Value::String(s) => s.parse::<i64>().unwrap_or(0),
                            Value::Number(n) => n.as_i64().unwrap_or(0),
                            _ => 0
                        };

                        if target_rec_id == 0 { continue; }

                        // 3. Resolve Target Collection ID (Name OR ID)
                        let identifier = &rel_def.target_collection;
                        let mut target_col_id: Option<i64> = None;

                        // Attempt A: Lookup by Name
                        let mut name_rows = conn.query(
                            "SELECT id FROM collections WHERE name = ?1", 
                            params![identifier.clone()]
                        ).await?;
                        
                        if let Some(r) = name_rows.next().await? {
                            target_col_id = Some(r.get(0)?);
                        } 
                        // Attempt B: Lookup by ID
                        else if let Ok(id_num) = identifier.parse::<i64>() {
                             let mut id_rows = conn.query("SELECT id FROM collections WHERE id = ?1", params![id_num]).await?;
                             if let Some(_) = id_rows.next().await? {
                                 target_col_id = Some(id_num);
                             }
                        }

                        // 4. Insert Relation
                        if let Some(tc_id) = target_col_id {
                            conn.execute(
                                "INSERT INTO _relations (origin_col_id, origin_rec_id, target_col_id, target_rec_id, rel_name) VALUES (?1, ?2, ?3, ?4, ?5)",
                                params![collection_id, record_id, tc_id, target_rec_id, rel_name]
                            ).await?;
                        } else {
                            println!("RELATION WARNING: Could not resolve target collection '{}'", identifier);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Db for TinyBase {
    // --- Collections ---
    async fn create_collection(
        &self,
        name: &str,
        schema: &Option<CollectionSchema>,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let schema_str = serde_json::to_string(&schema)?;
        conn.execute(
            "INSERT INTO collections (name, schema) VALUES (?1, ?2)",
            params![name, schema_str],
        )
        .await?;
        let id = conn.last_insert_rowid();

        // Init Search Index
        if let Some(s) = schema {
            if s.fields.values().any(|f| f.indexed) {
                self.search.load_index(id, s)?;
            }
        }
        Ok(id)
    }

    async fn get_collection(
        &self,
        id: i64,
    ) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, name, schema FROM collections WHERE id = ?1",
                params![id],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(row_to_collection(&row)?)),
            None => Ok(None),
        }
    }

    async fn list_collections(
        &self,
    ) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn
            .query("SELECT id, name, schema FROM collections", ())
            .await?;
        let mut collections = Vec::new();
        while let Some(row) = rows.next().await? {
            collections.push(row_to_collection(&row)?);
        }
        Ok(collections)
    }

    async fn update_collection(
        &self,
        id: i64,
        name: Option<String>,
        schema: Option<CollectionSchema>,
    ) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        if let Some(name) = name {
            conn.execute(
                "UPDATE collections SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .await?;
        }
        if let Some(schema) = schema {
            let s = serde_json::to_string(&schema)?;
            conn.execute(
                "UPDATE collections SET schema = ?1 WHERE id = ?2",
                params![s, id],
            )
            .await?;
        }
        let collection = self.get_collection(id).await?.ok_or("Not found")?;
        Ok(collection)
    }

    async fn delete_collection(&self, id: i64) -> Result<()> {
        self.db
            .connect()?
            .execute("DELETE FROM collections WHERE id = ?1", params![id])
            .await?;
        Ok(())
    }

    // --- Records (Syncs to Search & Relations) ---
    async fn create_record(
        &self,
        collection_id: i64,
        data: &Value,
    ) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        conn.execute(
            "INSERT INTO records (collection_id, data) VALUES (?1, ?2)",
            params![collection_id, serde_json::to_string(data)?],
        )
        .await?;
        let record_id = conn.last_insert_rowid();

        // Sync Relations
        self.sync_relations(&conn, collection_id, record_id, data).await?;

        // Sync Search
        if let Some(col) = self.get_collection(collection_id).await? {
            if let Some(schema) = &col.schema {
                if schema.fields.values().any(|f| f.indexed) {
                    let _ = self.search.load_index(collection_id, schema);
                    self.search
                        .index_record(collection_id, record_id, data, schema)?;
                }
            }
        }
        Ok(record_id)
    }

    async fn update_record(
        &self,
        collection_id: i64,
        record_id: i64,
        data: &Value,
    ) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        conn.execute(
            "UPDATE records SET data = ?1 WHERE collection_id = ?2 AND id = ?3",
            params![serde_json::to_string(data)?, collection_id, record_id],
        )
        .await?;

        // Sync Relations
        self.sync_relations(&conn, collection_id, record_id, data).await?;

        // Sync Search
        if let Some(col) = self.get_collection(collection_id).await? {
            if let Some(schema) = &col.schema {
                if schema.fields.values().any(|f| f.indexed) {
                    let _ = self.search.load_index(collection_id, schema);
                    self.search
                        .index_record(collection_id, record_id, data, schema)?;
                }
            }
        }

        self.get_record(collection_id, record_id)
            .await?
            .ok_or_else(|| "Not found".into())
    }

    async fn delete_record(&self, collection_id: i64, record_id: i64) -> Result<()> {
        let conn = self.db.connect()?;
        conn.execute(
            "DELETE FROM records WHERE collection_id = ?1 AND id = ?2",
            params![collection_id, record_id],
        ).await?;
        
        // Cleanup relations
        conn.execute("DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2", params![collection_id, record_id]).await?;

        // Cleanup Search
        let _ = self.search.delete_record(collection_id, record_id);
        Ok(())
    }

    // --- Search Logic ---
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

        let id_list = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, data FROM records WHERE id IN ({})", id_list);

        let conn = self.db.connect()?;
        let mut rows = conn.query(&sql, ()).await?;

        let mut records = Vec::new();
        while let Some(row) = rows.next().await? {
            records.push(row_to_record(&row)?);
        }

        Ok(records)
    }

    async fn instant_search(
        &self,
        collection_id: i64,
        query: &str,
    ) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>> {
        // Prevent "Index not loaded" error
        if let Some(col) = self.get_collection(collection_id).await? {
            if let Some(schema) = &col.schema {
                if schema.fields.values().any(|f| f.indexed) {
                    self.search.load_index(collection_id, schema)?;
                } else {
                    return Ok(vec![]);
                }
            }
        }
        let results = self.search.instant_search(collection_id, query, 5)?;
        Ok(results)
    }

    // --- Standard Methods ---
    async fn list_records(&self, collection_id: i64, options: QueryOptions) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let builder = query::SqlBuilder::new(collection_id, options);
        let mut rows = conn.query(&builder.base_sql, builder.params).await?;
        let mut records = Vec::new();
        while let Some(row) = rows.next().await? { 
            records.push(row_to_record(&row)?); 
        }
        Ok(records)
    }

    async fn get_record(
        &self,
        collection_id: i64,
        record_id: i64,
    ) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, data FROM records WHERE collection_id = ?1 AND id = ?2",
                params![collection_id, record_id],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(row_to_record(&row)?)),
            None => Ok(None),
        }
    }

    // --- Auth & Users ---
    async fn create_user(
        &self,
        e: &str,
        p: &str,
        r: &str,
    ) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        conn.execute(
            "INSERT INTO users (email, password_hash, role) VALUES (?1, ?2, ?3)",
            params![e, p, r],
        )
        .await?;
        Ok(User {
            id: conn.last_insert_rowid(),
            email: e.into(),
            password_hash: p.into(),
            role: r.into(),
        })
    }

    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut r = conn
            .query(
                "SELECT id, email, password_hash, role FROM users WHERE email = ?1",
                params![email],
            )
            .await?;
        if let Some(row) = r.next().await? {
            Ok(Some(User {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                role: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn list_users(&self) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, email, password_hash, role FROM users", ()).await?;
        let mut users = Vec::new();
        while let Some(row) = rows.next().await? {
            users.push(User {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                role: row.get(3)?,
            });
        }
        Ok(users)
    }

    async fn delete_user(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        conn.execute("DELETE FROM users WHERE id = ?1", params![id]).await?;
        Ok(())
    }

    async fn get_user_by_oauth(
        &self,
        p: &str,
        pid: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut r = conn.query(
            "SELECT u.id, u.email, u.password_hash, u.role FROM users u JOIN auth_identities ai ON u.id = ai.user_id WHERE ai.provider = ?1 AND ai.provider_id = ?2",
            params![p, pid],
        ).await?;
        if let Some(row) = r.next().await? {
            Ok(Some(User {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                role: row.get(3)?,
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
        self.db.connect()?.execute("INSERT INTO auth_identities (user_id, provider, provider_id) VALUES (?1, ?2, ?3)", params![uid, p, pid]).await?;
        Ok(())
    }

    async fn create_auth_token(
        &self,
        uid: i64,
        t: &str,
        tk: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.db.connect()?.execute("INSERT INTO auth_tokens (token, user_id, type, expires_at) VALUES (?1, ?2, ?3, datetime('now', '+1 hour'))", params![tk, uid, t]).await?;
        Ok(())
    }

    async fn consume_auth_token(
        &self,
        tk: &str,
        t: &str,
    ) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut r = conn.query("SELECT user_id FROM auth_tokens WHERE token = ?1 AND type = ?2 AND expires_at > datetime('now')", params![tk, t]).await?;
        if let Some(row) = r.next().await? {
            let uid: i64 = row.get(0)?;
            conn.execute("DELETE FROM auth_tokens WHERE token = ?1", params![tk]).await?;
            Ok(Some(uid))
        } else {
            Ok(None)
        }
    }

    async fn set_user_verified(&self, uid: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.db.connect()?.execute("UPDATE users SET is_verified = 1 WHERE id = ?1", params![uid]).await?;
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
        self.db.connect()?.execute(
            "INSERT INTO _storage_files (filename, original_name, mime_type, size, user_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![f, o, m, s, u],
        ).await?;
        Ok(self.db.connect()?.last_insert_rowid())
    }

    async fn list_files(&self, limit: i64, offset: i64) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, filename, original_name, mime_type, size, created_at FROM _storage_files ORDER BY created_at DESC LIMIT ?1 OFFSET ?2", params![limit, offset]).await?;
        let mut files = Vec::new();
        while let Some(row) = rows.next().await? {
            files.push(models::StoredFile {
                id: row.get(0)?, filename: row.get(1)?, original_name: row.get(2)?, mime_type: row.get(3)?, size: row.get(4)?, created_at: row.get(5)?,
            });
        }
        Ok(files)
    }

    async fn get_file_metadata(&self, id: i64) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, filename, original_name, mime_type, size, created_at FROM _storage_files WHERE id = ?1", params![id]).await?;
        if let Some(row) = rows.next().await? {
            Ok(Some(models::StoredFile {
                id: row.get(0)?, filename: row.get(1)?, original_name: row.get(2)?, mime_type: row.get(3)?, size: row.get(4)?, created_at: row.get(5)?,
            }))
        } else { Ok(None) }
    }

    async fn delete_file_metadata(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.db.connect()?.execute("DELETE FROM _storage_files WHERE id = ?1", params![id]).await?;
        Ok(())
    }

    // --- Config & Settings ---
    async fn set_system_config(
        &self,
        k: &str,
        v: &security::EncryptedValue,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let j = serde_json::to_string(v)?;
        conn.execute("INSERT INTO _system_config (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![k, j]).await?;
        Ok(())
    }

    async fn get_system_config(
        &self,
        k: &str,
    ) -> std::result::Result<Option<security::EncryptedValue>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut r = conn.query("SELECT value FROM _system_config WHERE key = ?1", params![k]).await?;
        if let Some(row) = r.next().await? {
            let j: String = row.get(0)?;
            Ok(Some(serde_json::from_str(&j)?))
        } else { Ok(None) }
    }

    async fn get_setting(&self, key: &str) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT value FROM _settings WHERE key = ?1", params![key]).await?;
        if let Some(row) = rows.next().await? {
            let v: String = row.get(0)?;
            Ok(Some(serde_json::from_str(&v)?))
        } else { Ok(None) }
    }

    async fn save_setting(&self, key: &str, value: serde_json::Value, encrypted: bool) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let v_str = serde_json::to_string(&value)?;
        conn.execute(
            "INSERT INTO _settings (key, value, encrypted) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, encrypted=excluded.encrypted, updated_at=CURRENT_TIMESTAMP",
            params![key, v_str, encrypted]
        ).await?;
        Ok(())
    }

    // --- Audit Logs ---
    async fn log_audit_event(&self, level: &str, message: &str, source: &str, meta: Option<serde_json::Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let meta_str = serde_json::to_string(&meta).unwrap_or("{}".to_string());
        let _ = conn.execute("INSERT INTO _audit_logs (level, message, source, meta) VALUES (?1, ?2, ?3, ?4)", params![level, message, source, meta_str]).await;
        Ok(())
    }

    async fn list_audit_logs(&self) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, level, message, source, meta, timestamp FROM _audit_logs ORDER BY timestamp DESC LIMIT 100", ()).await?;
        let mut logs = Vec::new();
        while let Some(row) = rows.next().await? {
            let meta_str: Option<String> = row.get(4)?;
            let meta = meta_str.map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null));
            logs.push(serde_json::json!({
                "id": row.get::<i64>(0)?, "level": row.get::<String>(1)?, "message": row.get::<String>(2)?, "source": row.get::<String>(3)?, "meta": meta, "timestamp": row.get::<String>(5)?
            }));
        }
        Ok(logs)
    }

    // --- AI Actions ---
    async fn list_ai_actions(&self) -> std::result::Result<Vec<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, slug, name, model, system_prompt, template, config FROM _ai_actions", ()).await?;
        let mut res = Vec::new();
        while let Some(row) = rows.next().await? {
            let conf_str: String = row.get(6).unwrap_or("{}".to_string());
            res.push(ai_models::AiAction {
                id: row.get(0)?, slug: row.get(1)?, name: row.get(2)?, model: row.get(3)?,
                system_prompt: row.get(4)?, template: row.get(5)?, config: serde_json::from_str(&conf_str).unwrap_or_default()
            });
        }
        Ok(res)
    }

    async fn get_ai_action(&self, slug: &str) -> std::result::Result<Option<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, slug, name, model, system_prompt, template, config FROM _ai_actions WHERE slug = ?1", params![slug]).await?;
        if let Some(row) = rows.next().await? {
            let conf_str: String = row.get(6).unwrap_or("{}".to_string());
            Ok(Some(ai_models::AiAction {
                id: row.get(0)?, slug: row.get(1)?, name: row.get(2)?, model: row.get(3)?,
                system_prompt: row.get(4)?, template: row.get(5)?, config: serde_json::from_str(&conf_str).unwrap_or_default()
            }))
        } else { Ok(None) }
    }

    async fn create_ai_action(&self, req: ai_models::CreateActionReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        conn.execute("INSERT INTO _ai_actions (slug, name, model, system_prompt, template, config) VALUES (?1, ?2, ?3, ?4, ?5, '{}')", params![req.slug, req.name, req.model, req.system_prompt, req.template]).await?;
        Ok(conn.last_insert_rowid())
    }

    async fn delete_ai_action(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.db.connect()?.execute("DELETE FROM _ai_actions WHERE id = ?1", params![id]).await?;
        Ok(())
    }

    // --- Relations (Raw) ---
    async fn create_relation(&self, oc: i64, oi: i64, tc: i64, ti: i64, rn: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.db.connect()?.execute("INSERT INTO _relations (origin_col_id, origin_rec_id, target_col_id, target_rec_id, rel_name) VALUES (?1, ?2, ?3, ?4, ?5)", params![oc, oi, tc, ti, rn]).await?; Ok(())
    }
    async fn delete_relation(&self, oc: i64, oi: i64, tc: i64, ti: i64, rn: &str) -> Result<()> {
        self.db.connect()?.execute("DELETE FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND target_col_id=?3 AND target_rec_id=?4 AND rel_name=?5", params![oc, oi, tc, ti, rn]).await?; Ok(())
    }
    async fn get_related_ids(&self, oc: i64, oi: i64, rn: &str) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT target_col_id, target_rec_id FROM _relations WHERE origin_col_id=?1 AND origin_rec_id=?2 AND rel_name=?3", params![oc, oi, rn]).await?;
        let mut results = Vec::new(); while let Some(row) = rows.next().await? { results.push((row.get(0)?, row.get(1)?)); } Ok(results)
    }
    async fn get_records_by_ids(&self, collection_id: i64, ids: &[i64]) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        if ids.is_empty() { return Ok(vec![]); }
        let id_list = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, data FROM records WHERE collection_id = ? AND id IN ({})", id_list);
        let conn = self.db.connect()?;
        let mut rows = conn.query(&sql, params![collection_id]).await?;
        let mut records = Vec::new();
        while let Some(row) = rows.next().await? { records.push(row_to_record(&row)?); }
        Ok(records)
    }

    // Scripts
    async fn list_scripts(&self) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, name, trigger_type, code, active FROM _scripts", ()).await?;
        let mut res = Vec::new();
        while let Some(row) = rows.next().await? {
            res.push(script_models::Script {
                id: row.get(0)?, name: row.get(1)?, trigger_type: row.get(2)?, code: row.get(3)?, active: row.get(4)?
            });
        }
        Ok(res)
    }
    
    async fn create_script(&self, req: script_models::CreateScriptReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.db.connect()?.execute("INSERT INTO _scripts (name, trigger_type, code) VALUES (?1, ?2, ?3)", params![req.name, req.trigger_type, req.code]).await?;
        Ok(self.db.connect()?.last_insert_rowid())
    }
    
    async fn delete_script(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.db.connect()?.execute("DELETE FROM _scripts WHERE id = ?1", params![id]).await?;
        Ok(())
    }
    
    async fn get_script_by_name(&self, name: &str) -> std::result::Result<Option<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, name, trigger_type, code, active FROM _scripts WHERE name = ?1", params![name]).await?;
        if let Some(row) = rows.next().await? {
            Ok(Some(script_models::Script {
                id: row.get(0)?, name: row.get(1)?, trigger_type: row.get(2)?, code: row.get(3)?, active: row.get(4)?
            }))
        } else { Ok(None) }
    }
    
    async fn get_scripts_by_trigger(&self, trigger: &str) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, name, trigger_type, code, active FROM _scripts WHERE trigger_type = ?1 AND active = 1", params![trigger]).await?;
        let mut res = Vec::new();
        while let Some(row) = rows.next().await? {
            res.push(script_models::Script {
                id: row.get(0)?, name: row.get(1)?, trigger_type: row.get(2)?, code: row.get(3)?, active: row.get(4)?
            });
        }
        Ok(res)
    }

    // Templates
    async fn list_templates(&self) -> std::result::Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, slug, content, script_id, created_at FROM _templates", ()).await?;
        let mut res = Vec::new();
        while let Some(row) = rows.next().await? {
            res.push(models::Template {
                id: row.get(0)?, slug: row.get(1)?, content: row.get(2)?, script_id: row.get(3)?, created_at: row.get(4)?
            });
        }
        Ok(res)
    }

    async fn get_template_by_slug(&self, slug: &str) -> std::result::Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.db.connect()?;
        let mut rows = conn.query("SELECT id, slug, content, script_id, created_at FROM _templates WHERE slug = ?1", params![slug]).await?;
        if let Some(row) = rows.next().await? {
            Ok(Some(models::Template {
                id: row.get(0)?, slug: row.get(1)?, content: row.get(2)?, script_id: row.get(3)?, created_at: row.get(4)?
            }))
        } else { Ok(None) }
    }

    async fn create_template(&self, req: models::CreateTemplateReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.db.connect()?.execute("INSERT INTO _templates (slug, content, script_id) VALUES (?1, ?2, ?3)", params![req.slug, req.content, req.script_id]).await?;
        Ok(self.db.connect()?.last_insert_rowid())
    }

    async fn update_template(&self, id: i64, content: String, script_id: Option<i64>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.db.connect()?.execute("UPDATE _templates SET content = ?1, script_id = ?2 WHERE id = ?3", params![content, script_id, id]).await?;
        Ok(())
    }

    async fn delete_template(&self, id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.db.connect()?.execute("DELETE FROM _templates WHERE id = ?1", params![id]).await?;
        Ok(())
    }
}

// --- Mock Implementation for Testing ---
#[async_trait]
impl Db for Mutex<Connection> {
    // Stub implementations for all traits to pass compilation in test envs
    async fn create_collection(&self, _n: &str, _s: &Option<CollectionSchema>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn get_collection(&self, _id: i64) -> std::result::Result<Option<Collection>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn list_collections(&self) -> std::result::Result<Vec<Collection>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn update_collection(&self, _id: i64, _n: Option<String>, _s: Option<CollectionSchema>) -> std::result::Result<Collection, Box<dyn std::error::Error + Send + Sync>> { Err("Mock".into()) }
    async fn delete_collection(&self, _id: i64) -> Result<()> { Ok(()) }
    async fn create_record(&self, _c: i64, _d: &Value) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn list_records(&self, _c: i64, _o: QueryOptions) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn get_record(&self, _c: i64, _r: i64) -> std::result::Result<Option<Record>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn update_record(&self, _c: i64, _r: i64, _d: &Value) -> std::result::Result<Record, Box<dyn std::error::Error + Send + Sync>> { Err("Mock".into()) }
    async fn delete_record(&self, _c: i64, _r: i64) -> Result<()> { Ok(()) }
    async fn search_records(&self, _c: i64, _q: &str) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn instant_search(&self, _c: i64, _q: &str) -> std::result::Result<Vec<models::InstantResult>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn create_user(&self, _e: &str, _p: &str, _r: &str) -> std::result::Result<User, Box<dyn std::error::Error + Send + Sync>> { Err("Mock".into()) }
    async fn get_user_by_email(&self, _e: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn list_users(&self) -> std::result::Result<Vec<User>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn delete_user(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn create_file_metadata(&self, _f: &str, _o: &str, _m: &str, _s: i64, _u: Option<i64>) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn list_files(&self, _l: i64, _o: i64) -> std::result::Result<Vec<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn get_file_metadata(&self, _id: i64) -> std::result::Result<Option<models::StoredFile>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn delete_file_metadata(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn get_user_by_oauth(&self, _p: &str, _pid: &str) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn link_oauth(&self, _u: i64, _p: &str, _pid: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn create_auth_token(&self, _u: i64, _t: &str, _tk: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn consume_auth_token(&self, _t: &str, _tt: &str) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn set_user_verified(&self, _u: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn set_system_config(&self, _k: &str, _v: &security::EncryptedValue) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn get_system_config(&self, _k: &str) -> std::result::Result<Option<security::EncryptedValue>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn get_setting(&self, _k: &str) -> std::result::Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn save_setting(&self, _k: &str, _v: serde_json::Value, _e: bool) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn log_audit_event(&self, _l: &str, _m: &str, _s: &str, _meta: Option<serde_json::Value>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn list_audit_logs(&self) -> std::result::Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn list_ai_actions(&self) -> std::result::Result<Vec<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn get_ai_action(&self, _slug: &str) -> std::result::Result<Option<ai_models::AiAction>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn create_ai_action(&self, _a: ai_models::CreateActionReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn delete_ai_action(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn create_relation(&self, _oc: i64, _oi: i64, _tc: i64, _ti: i64, _rn: &str) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn delete_relation(&self, _oc: i64, _oi: i64, _tc: i64, _ti: i64, _rn: &str) -> Result<()> { Ok(()) }
    async fn get_related_ids(&self, _oc: i64, _oi: i64, _rn: &str) -> std::result::Result<Vec<(i64, i64)>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn get_records_by_ids(&self, _c: i64, _i: &[i64]) -> std::result::Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    // --- Script Mock ---
    async fn list_scripts(&self) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn create_script(&self, _r: script_models::CreateScriptReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn delete_script(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn get_script_by_name(&self, _n: &str) -> std::result::Result<Option<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn get_scripts_by_trigger(&self, _t: &str) -> std::result::Result<Vec<script_models::Script>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    // --- Template Mock ---
    async fn list_templates(&self) -> std::result::Result<Vec<models::Template>, Box<dyn std::error::Error + Send + Sync>> { Ok(vec![]) }
    async fn get_template_by_slug(&self, _slug: &str) -> std::result::Result<Option<models::Template>, Box<dyn std::error::Error + Send + Sync>> { Ok(None) }
    async fn create_template(&self, _req: models::CreateTemplateReq) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
    async fn update_template(&self, _id: i64, _c: String, _s: Option<i64>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
    async fn delete_template(&self, _id: i64) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
}

// --- Constructor ---
pub async fn a_new_database_connection() -> Result<TinyBase> {
    let db = Builder::new_local("local.db").build().await?;
    setup_database(&db).await?;
    Ok(TinyBase::new(db))
}

async fn setup_database(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    // Core
    conn.execute("CREATE TABLE IF NOT EXISTS collections (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, schema JSON)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS records (id INTEGER PRIMARY KEY AUTOINCREMENT, collection_id INTEGER NOT NULL, data TEXT NOT NULL)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, role TEXT NOT NULL)", ()).await?;
    let _ = conn.execute("ALTER TABLE users ADD COLUMN is_verified BOOLEAN DEFAULT 0", ()).await;

    // Storage
    conn.execute("CREATE TABLE IF NOT EXISTS _storage_files (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL, original_name TEXT NOT NULL, mime_type TEXT NOT NULL, size INTEGER NOT NULL, user_id INTEGER, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", ()).await?;

    // Auth & Config
    conn.execute("CREATE TABLE IF NOT EXISTS auth_identities (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL, provider TEXT NOT NULL, provider_id TEXT NOT NULL)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS auth_tokens (token TEXT PRIMARY KEY, user_id INTEGER NOT NULL, type TEXT NOT NULL, expires_at DATETIME NOT NULL)", ()).await?;
    conn.execute("CREATE TABLE IF NOT EXISTS _system_config (key TEXT PRIMARY KEY, value TEXT NOT NULL)", ()).await?;

    // Relations (Graph Node)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _relations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            origin_col_id INTEGER NOT NULL,
            origin_rec_id INTEGER NOT NULL,
            target_col_id INTEGER NOT NULL,
            target_rec_id INTEGER NOT NULL,
            rel_name TEXT NOT NULL,
            properties JSON
        )", 
        ()
    ).await?;
    let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_relations_origin ON _relations(origin_col_id, origin_rec_id, rel_name)", ()).await;
    let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_relations_target ON _relations(target_col_id, target_rec_id)", ()).await;

    // Settings (Robust JSON config)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _settings (
            key TEXT PRIMARY KEY, 
            value TEXT NOT NULL, 
            encrypted BOOLEAN DEFAULT 0,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        (),
    ).await?;

    // Audit Logs (Analytics)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _audit_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            level TEXT NOT NULL,
            message TEXT NOT NULL,
            source TEXT NOT NULL,
            meta JSON,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        (),
    ).await?;

    // AI Actions
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _ai_actions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            slug TEXT UNIQUE NOT NULL,
            name TEXT NOT NULL,
            model TEXT NOT NULL,
            system_prompt TEXT,
            template TEXT NOT NULL,
            config JSON,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        (),
    ).await?;

    // Scripts
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _scripts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE NOT NULL,
            trigger_type TEXT NOT NULL,
            code TEXT NOT NULL,
            active BOOLEAN DEFAULT 1,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )", ()
    ).await?;

    // Templates
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            slug TEXT UNIQUE NOT NULL,
            content TEXT NOT NULL,
            script_id INTEGER,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )", ()
    ).await?;

    Ok(())
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/lib.rs ends here ===========================