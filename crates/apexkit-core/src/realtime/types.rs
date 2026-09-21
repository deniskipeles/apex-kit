use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventScope {
    #[default]
    Root,
    Tenant(String),
    Sandbox(String),
    // Allow custom scopes (e.g. "chat_room_123") defined by scripts
    Channel(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum DbEvent {
    Insert {
        collection_id: i64,
        record_id: i64,
        data: Value,
        scope: EventScope,
    },
    Update {
        collection_id: i64,
        record_id: i64,
        data: Value,
        scope: EventScope,
    },
    Delete {
        collection_id: i64,
        record_id: i64,
        scope: EventScope,
    },
    // [NEW] Custom Event for Scripting
    Custom {
        event: String, // e.g. "UserTyping"
        data: Value,   // e.g. { "user": "Alice" }
        #[serde(skip)]
        #[serde(default = "default_true")]
        is_local: bool, // Prevents infinite echo loops across distributed nodes
        scope: EventScope, // e.g. Channel("room_1")
    },
}

fn default_true() -> bool { true }
