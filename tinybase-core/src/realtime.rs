// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/realtime.rs start here ===========================
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum DbEvent {
    Insert { collection_id: i64, record_id: i64, data: Value },
    Update { collection_id: i64, record_id: i64, data: Value },
    Delete { collection_id: i64, record_id: i64 },
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/realtime.rs ends here ===========================