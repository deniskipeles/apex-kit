use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventScope {
    Root,
    Tenant(String),
    Sandbox(String),
    // Allow custom scopes (e.g. "chat_room_123") defined by scripts
    Channel(String), 
}

impl Default for EventScope {
    fn default() -> Self {
        Self::Root
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum DbEvent {
    Insert { 
        collection_id: i64, 
        record_id: i64, 
        data: Value,
        #[serde(skip_serializing)] 
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
    // [NEW] Custom Event for Scripting
    Custom {
        event: String,      // e.g. "UserTyping"
        data: Value,        // e.g. { "user": "Alice" }
        #[serde(skip_serializing)] 
        scope: EventScope   // e.g. Channel("room_1")
    }
}