// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/realtime.rs ===========================
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventScope {
    Root,
    Tenant(String),
    Sandbox(String),
}

impl Default for EventScope {
    fn default() -> Self {
        Self::Root
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum DbEvent {
    Insert { 
        collection_id: i64, 
        record_id: i64, 
        data: Value,
        #[serde(skip_serializing)] // Don't send scope to client, just use for filtering
        scope: EventScope 
    },
    Update { 
        collection_id: i64, 
        record_id: i64, 
        data: Value,
        #[serde(skip_serializing)] 
        scope: EventScope 
    },
    Delete { 
        collection_id: i64, 
        record_id: i64,
        #[serde(skip_serializing)] 
        scope: EventScope 
    },
}