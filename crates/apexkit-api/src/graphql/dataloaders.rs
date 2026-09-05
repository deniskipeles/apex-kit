use apexkit_core::models::Record;
use apexkit_core::{Db, auth::User};
use async_graphql::dataloader::*;
use std::collections::HashMap;
use std::sync::Arc;

// --- DATALOADERS ---

pub struct UserLoader {
    pub db: Arc<dyn Db>,
}

impl Loader<i64> for UserLoader {
    type Value = User;
    type Error = String;

    async fn load(&self, keys: &[i64]) -> Result<HashMap<i64, Self::Value>, Self::Error> {
        let users = self
            .db
            .get_users_by_ids(keys)
            .await
            .map_err(|e| e.to_string())?;
        Ok(users.into_iter().map(|u| (u.id, u)).collect())
    }
}

pub struct RelationLoader {
    pub db: Arc<dyn Db>,
}

impl RelationLoader {
    pub fn new(db: Arc<dyn Db>) -> Self {
        Self { db }
    }
}

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub struct RelationKey {
    pub origin_col_id: i64,
    pub origin_rec_id: i64,
    pub rel_name: String,
    pub target_col_name: String,
}

impl Loader<RelationKey> for RelationLoader {
    type Value = Vec<Record>;
    type Error = String;

    async fn load(
        &self,
        keys: &[RelationKey],
    ) -> std::result::Result<HashMap<RelationKey, Self::Value>, Self::Error> {
        let keys_cloned = keys.to_vec();
        let mut results = HashMap::new();
        let mut grouped_keys: HashMap<(i64, String), Vec<i64>> = HashMap::new();

        for key in &keys_cloned {
            grouped_keys
                .entry((key.origin_col_id, key.rel_name.clone()))
                .or_default()
                .push(key.origin_rec_id);
        }

        let mut target_ids_map: HashMap<RelationKey, Vec<(i64, i64)>> = HashMap::new();
        let mut needed_records: HashMap<String, Vec<i64>> = HashMap::new();

        for ((o_col, rel), o_ids) in grouped_keys {
            for o_id in o_ids {
                let mut links = self
                    .db
                    .get_related_ids(o_col, o_id, &rel)
                    .await
                    .unwrap_or_default();

                // FALLBACK: If _relations table is empty, check origin_record.data[rel_name] directly
                if links.is_empty() {
                    if let Ok(Some(origin_rec)) = self.db.get_record(o_col, o_id, None).await {
                        if let Some(val) = origin_rec.data.get(&rel) {
                            let mut fallback_ids = Vec::new();
                            if let Some(arr) = val.as_array() {
                                for v in arr {
                                    if let Some(id) = v
                                        .as_i64()
                                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                                    {
                                        fallback_ids.push(id);
                                    }
                                }
                            } else if let Some(id) = val
                                .as_i64()
                                .or_else(|| val.as_str().and_then(|s| s.parse().ok()))
                            {
                                fallback_ids.push(id);
                            }

                            if !fallback_ids.is_empty() {
                                for t_id in fallback_ids {
                                    links.push((0, t_id));
                                }
                            }
                        }
                    }
                }

                if let Some(key) = keys_cloned.iter().find(|k| {
                    k.origin_col_id == o_col && k.origin_rec_id == o_id && k.rel_name == rel
                }) {
                    target_ids_map.insert(key.clone(), links.clone());
                    for (_, t_rec_id) in links {
                        needed_records
                            .entry(key.target_col_name.clone())
                            .or_default()
                            .push(t_rec_id);
                    }
                }
            }
        }

        let cols = self
            .db
            .list_collections()
            .await
            .map_err(|e| e.to_string())?;
        let col_name_to_id: HashMap<String, i64> =
            cols.into_iter().map(|c| (c.name, c.id)).collect();
        let mut fetched_record_cache: HashMap<(String, i64), Record> = HashMap::new();

        for (t_col_name, t_ids) in needed_records {
            if let Some(t_col_id) = col_name_to_id.get(&t_col_name)
                && let Ok(recs) = self.db.get_records_by_ids(*t_col_id, &t_ids).await
            {
                for r in recs {
                    fetched_record_cache.insert((t_col_name.clone(), r.id), r);
                }
            }
        }

        for key in keys_cloned {
            let mut records = Vec::new();
            if let Some(links) = target_ids_map.get(&key) {
                for (_, t_rec_id) in links {
                    if let Some(rec) =
                        fetched_record_cache.get(&(key.target_col_name.clone(), *t_rec_id))
                    {
                        records.push(rec.clone());
                    }
                }
            }
            results.insert(key, records);
        }
        Ok(results)
    }
}
