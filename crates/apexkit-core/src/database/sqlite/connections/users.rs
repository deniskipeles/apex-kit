use super::ApexKit;
use crate::auth::User;
use crate::database::traits::{
    AuthTokenStore, CollectionStore, ConfigStore, IntoSqlVal, OAuthStore, UserStore,
};
use crate::models::schema::{CollectionPolicies, FieldType};
use crate::models::{ExpandableItem, Record};
use async_trait::async_trait;
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::error::Error as StdError;

#[async_trait]
impl UserStore for ApexKit {
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
    ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
        self.core_batcher
            .execute(
                "DELETE FROM users WHERE id = ?1".into(),
                vec![id.into_val()],
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn StdError + Send + Sync>)?;
        Ok(())
    }

    async fn update_user(
        &self,
        id: i64,
        email: Option<String>,
        role: Option<String>,
        metadata: Option<Value>,
        password: Option<String>,
    ) -> std::result::Result<User, Box<dyn StdError + Send + Sync>> {
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
}

#[async_trait]
impl OAuthStore for ApexKit {
    async fn get_user_by_oauth(
        &self,
        p: &str,
        pid: &str,
    ) -> std::result::Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_core_read().await;
        let mut stmt = conn.prepare(
            "SELECT u.id,u.email,u.password_hash,u.role,u.metadata FROM users u \
             JOIN auth_identities ai ON u.id = ai.user_id WHERE ai.provider = ?1 AND ai.provider_id = ?2"
        )?;
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
}

#[async_trait]
impl AuthTokenStore for ApexKit {
    async fn create_auth_token(
        &self,
        uid: i64,
        t: &str,
        tk: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.core_batcher
            .execute(
                "INSERT INTO auth_tokens (token,user_id,type,expires_at) VALUES (?1,?2,?3,datetime('now','+1 hour'))".into(),
                vec![tk.into_val(), uid.into_val(), t.into_val()]
            )
            .await
            .map_err(|e| Box::new(std::io::Error::other(e)))?;
        Ok(())
    }

    async fn consume_auth_token(
        &self,
        tk: &str,
        t: &str,
    ) -> std::result::Result<Option<i64>, Box<dyn std::error::Error + Send + Sync>> {
        let uid = {
            let conn = self.get_core_read().await;
            let mut stmt = conn.prepare(
                "SELECT user_id FROM auth_tokens WHERE token = ?1 AND type = ?2 AND expires_at > datetime('now')"
            )?;
            let mut r = stmt.query(params![tk, t])?;
            if let Some(row) = r.next()? {
                Some(row.get::<_, i64>(0)?)
            } else {
                None
            }
        };

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
}

// --- Active hydration and user utility operations ---
impl ApexKit {
    pub async fn get_user_policies(&self) -> CollectionPolicies {
        if let Ok(Some(val)) = self.get_config("policy_users").await {
            if let Ok(p) = serde_json::from_value(val) {
                return p;
            }
        }
        CollectionPolicies {
            read: "admin || owner:id".to_string(),
            create: "public".to_string(),
            update: "admin || owner:id".to_string(),
            delete: "admin".to_string(),
        }
    }
}

pub(crate) async fn populate_owners_in_memory(
    kit: &ApexKit,
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

    hydrate_owners_recursive(kit, &mut root_items, collection_id, &tree).await?;

    Ok(())
}

#[allow(clippy::type_complexity)]
fn hydrate_owners_recursive<'a>(
    kit: &'a ApexKit,
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

        let col = match kit.get_collection(collection_id).await? {
            Some(c) => c,
            None => return Ok(()),
        };
        let schema = col.schema.unwrap_or_default();

        let mut owner_fields = Vec::new();
        let mut relation_fields = Vec::new();

        let all_collections = kit.list_collections().await?;
        let col_map: HashMap<String, i64> = all_collections
            .iter()
            .map(|c| (c.name.clone(), c.id))
            .collect();
        let id_map: HashMap<String, i64> = all_collections
            .iter()
            .map(|c| (c.id.to_string(), c.id))
            .collect();

        for (field_name, sub_paths) in tree {
            if let Some(def) = schema.fields.get(field_name) {
                if def.r#type == FieldType::Owner {
                    owner_fields.push(field_name);
                }
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

            let mut user_ids = HashSet::new();
            for item in items.iter() {
                if let Some(obj) = item.data.as_object() {
                    for field in &owner_fields {
                        if let Some(val) = obj.get(*field) {
                            if let Some(uid) = val.as_i64() {
                                user_ids.insert(uid);
                            } else if let Some(s) = val.as_str() {
                                if let Ok(uid) = s.parse::<i64>() {
                                    user_ids.insert(uid);
                                }
                            }
                        }
                    }
                }
            }

            if !user_ids.is_empty() {
                let ids_vec: Vec<i64> = user_ids.into_iter().collect();
                let users = kit.get_users_by_ids(&ids_vec).await?;
                let user_map: HashMap<i64, User> = users.into_iter().map(|u| (u.id, u)).collect();

                for item in items.iter_mut() {
                    let mut updates = Vec::new();
                    if let Some(obj) = item.data.as_object() {
                        for field in &owner_fields {
                            if let Some(val) = obj.get(*field) {
                                let uid_opt = val
                                    .as_i64()
                                    .or_else(|| val.as_str().and_then(|s| s.parse::<i64>().ok()));
                                if let Some(uid) = uid_opt {
                                    if let Some(user) = user_map.get(&uid) {
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
                    }
                    if let Some(expand_obj) = item.expand.as_mut().and_then(|v| v.as_object_mut()) {
                        for (f, v) in updates {
                            expand_obj.insert(f, v);
                        }
                    }
                }
            }
        }

        for (rel_name, target_id, sub_paths_list) in relation_fields {
            let sub_tree = crate::query::builder::build_expand_tree(&sub_paths_list.join(","));
            if sub_tree.is_empty() {
                continue;
            }

            for item in items.iter_mut() {
                if let Some(expand_val) = item.expand {
                    if let Some(rel_val) = expand_val.get_mut(rel_name) {
                        if let Some(arr) = rel_val.as_array_mut() {
                            hydrate_json_values_recursive(kit, arr, target_id, &sub_tree).await?;
                        } else if rel_val.is_object() {
                            let slice = std::slice::from_mut(rel_val);
                            hydrate_json_values_recursive(kit, slice, target_id, &sub_tree).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    })
}

#[allow(clippy::type_complexity)]
fn hydrate_json_values_recursive<'a>(
    kit: &'a ApexKit,
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

        let col = match kit.get_collection(collection_id).await? {
            Some(c) => c,
            None => return Ok(()),
        };
        let schema = col.schema.unwrap_or_default();

        let mut owner_fields = Vec::new();
        let mut relation_fields = Vec::new();
        let all_collections = kit.list_collections().await?;
        let col_map: HashMap<String, i64> = all_collections
            .iter()
            .map(|c| (c.name.clone(), c.id))
            .collect();
        let id_map: HashMap<String, i64> = all_collections
            .iter()
            .map(|c| (c.id.to_string(), c.id))
            .collect();

        for (field_name, sub_paths) in tree {
            if let Some(def) = schema.fields.get(field_name) {
                if def.r#type == FieldType::Owner {
                    owner_fields.push(field_name);
                }
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
            let mut user_ids = HashSet::new();
            for rec in json_records.iter() {
                if let Some(data) = rec.get("data") {
                    for field in &owner_fields {
                        if let Some(val) = data.get(*field) {
                            if let Some(uid) = val.as_i64() {
                                user_ids.insert(uid);
                            } else if let Some(s) = val.as_str() {
                                if let Ok(uid) = s.parse::<i64>() {
                                    user_ids.insert(uid);
                                }
                            }
                        }
                    }
                }
            }

            if !user_ids.is_empty() {
                let ids_vec: Vec<i64> = user_ids.into_iter().collect();
                let users = kit.get_users_by_ids(&ids_vec).await?;
                let user_map: HashMap<i64, User> = users.into_iter().map(|u| (u.id, u)).collect();

                for rec in json_records.iter_mut() {
                    let mut updates = Vec::new();
                    if let Some(data) = rec.get("data") {
                        for field in &owner_fields {
                            if let Some(val) = data.get(*field) {
                                let uid_opt = val
                                    .as_i64()
                                    .or_else(|| val.as_str().and_then(|s| s.parse::<i64>().ok()));
                                if let Some(uid) = uid_opt {
                                    if let Some(user) = user_map.get(&uid) {
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
                    }

                    if !updates.is_empty() {
                        if rec.get("expand").is_none() || rec.get("expand").unwrap().is_null() {
                            if let Some(obj) = rec.as_object_mut() {
                                obj.insert("expand".to_string(), serde_json::json!({}));
                            }
                        }
                        if let Some(expand) = rec.get_mut("expand").and_then(|v| v.as_object_mut())
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
            let sub_tree = crate::query::builder::build_expand_tree(&sub_paths_list.join(","));
            if sub_tree.is_empty() {
                continue;
            }

            for rec in json_records.iter_mut() {
                if let Some(expand) = rec.get_mut("expand") {
                    if let Some(rel_val) = expand.get_mut(rel_name) {
                        if let Some(arr) = rel_val.as_array_mut() {
                            hydrate_json_values_recursive(kit, arr, target_id, &sub_tree).await?;
                        } else if rel_val.is_object() {
                            let slice = std::slice::from_mut(rel_val);
                            hydrate_json_values_recursive(kit, slice, target_id, &sub_tree).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    })
}
