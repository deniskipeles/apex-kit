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

static NODE_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn get_node_id() -> &'static str {
    NODE_ID.get_or_init(|| uuid::Uuid::new_v4().to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum DbEvent {
    Insert {
        collection_id: i64,
        record_id: i64,
        data: Value,
        #[serde(default)]
        node_id: Option<String>,
        scope: EventScope,
    },
    Update {
        collection_id: i64,
        record_id: i64,
        data: Value,
        #[serde(default)]
        node_id: Option<String>,
        scope: EventScope,
    },
    Delete {
        collection_id: i64,
        record_id: i64,
        #[serde(default)]
        node_id: Option<String>,
        scope: EventScope,
    },
    Custom {
        event: String,
        data: Value,
        #[serde(default)]
        node_id: Option<String>,
        #[serde(skip)]
        #[serde(default = "default_true")]
        is_local: bool,
        scope: EventScope,
    },
}

impl DbEvent {
    pub fn node_id(&self) -> Option<&str> {
        match self {
            DbEvent::Insert { node_id, .. }
            | DbEvent::Update { node_id, .. }
            | DbEvent::Delete { node_id, .. }
            | DbEvent::Custom { node_id, .. } => node_id.as_deref(),
        }
    }

    pub fn is_from_current_node(&self) -> bool {
        self.node_id() == Some(get_node_id())
    }
}

fn default_true() -> bool {
    true
}
