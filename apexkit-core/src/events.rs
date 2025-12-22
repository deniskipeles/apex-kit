use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum SystemEvent {
    BeforeCreate { collection_id: i64, data: Value },
    AfterCreate { collection_id: i64, record_id: i64, data: Value },
    
    BeforeUpdate { collection_id: i64, record_id: i64, data: Value },
    AfterUpdate { collection_id: i64, record_id: i64, data: Value },
    
    BeforeDelete { collection_id: i64, record_id: i64 },
    AfterDelete { collection_id: i64, record_id: i64 },
}