use hnsw_rs::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

type IndexKey = (i64, String); 

pub struct VectorIndex {
    indexes: Arc<RwLock<HashMap<IndexKey, Hnsw<'static, f32, DistL2>>>>,
}

impl VectorIndex {
    pub fn new() -> Self {
        Self {
            indexes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn insert(&self, col_id: i64, rec_id: i64, field: &str, vector: &[f32]) {
        let key = (col_id, field.to_string());
        
        // Double-check pattern
        let exists = { self.indexes.read().unwrap().contains_key(&key) };
        if !exists {
            let mut writers = self.indexes.write().unwrap();
            if !writers.contains_key(&key) {
                let hnsw = Hnsw::new(16, 100000, 16, 200, DistL2);
                writers.insert(key.clone(), hnsw);
            }
        }

        let readers = self.indexes.read().unwrap();
        if let Some(index) = readers.get(&key) {
            // FIX: Pass as tuple (data, id)
            index.insert((vector, rec_id as usize));
        }
    }

    pub fn search(&self, col_id: i64, field: &str, query: &[f32], limit: usize) -> Vec<(i64, f32)> {
        let readers = self.indexes.read().unwrap();
        let key = (col_id, field.to_string());

        if let Some(index) = readers.get(&key) {
            let results = index.search(query, limit, 24); 
            results.into_iter().map(|n| (n.d_id as i64, n.distance)).collect()
        } else {
            vec![]
        }
    }
    
    pub fn delete(&self, _col_id: i64, _field: &str, _rec_id: i64) {
    }
}