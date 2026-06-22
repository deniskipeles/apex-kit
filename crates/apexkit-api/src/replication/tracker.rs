use super::pb;
use apexkit_core::models::ChangesetEvent;
use rusqlite::{Connection, params};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tokio::sync::RwLock;
use tonic::Status;

pub struct ReplicaInfo {
    pub id: String,
    pub scopes: HashSet<String>,
    pub buffer: Vec<pb::DbChangeEvent>,
    pub last_seen: Instant,
    pub tx: Option<tokio::sync::mpsc::Sender<Result<pb::DbChangeEvent, Status>>>,
}

static REPLICA_TRACKER: OnceLock<Arc<RwLock<HashMap<String, ReplicaInfo>>>> = OnceLock::new();
pub static REPLICA_ID: OnceLock<String> = OnceLock::new();

pub async fn init_replica_id() -> String {
    let path = "storage/system/.replica_id";
    if let Ok(id) = tokio::fs::read_to_string(path).await {
        let trimmed = id.trim().to_string();
        if !trimmed.is_empty() {
            let _ = REPLICA_ID.set(trimmed.clone());
            return trimmed;
        }
    }
    let new_id = uuid::Uuid::new_v4().to_string();
    let _ = tokio::fs::create_dir_all("storage/system").await;
    let _ = tokio::fs::write(path, &new_id).await;
    let _ = REPLICA_ID.set(new_id.clone());
    new_id
}

pub fn get_replica_tracker() -> Arc<RwLock<HashMap<String, ReplicaInfo>>> {
    REPLICA_TRACKER
        .get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
        .clone()
}

pub async fn register_replica_on_master(id: &str, scopes: &[String]) -> Result<(), Status> {
    let scope_list = scopes.join(",");
    let conn = Connection::open("storage/system/system.db")
        .map_err(|e| Status::internal(e.to_string()))?;
    conn.execute(
        "INSERT OR REPLACE INTO _replicas (id, scopes, last_seen) VALUES (?1, ?2, CURRENT_TIMESTAMP)",
        params![id, scope_list],
    )
    .map_err(|e| Status::internal(e.to_string()))?;
    Ok(())
}

pub async fn init_master_replica_tracker(tx: tokio::sync::broadcast::Sender<ChangesetEvent>) {
    let _ = super::MASTER_CHANGESET_TX.set(tx.clone());
    let mut rx = tx.subscribe();
    let tracker = get_replica_tracker();

    let recovered_state = tokio::task::spawn_blocking(move || {
        let conn = Connection::open("storage/system/system.db").expect("Failed to open system.db");
        let mut stmt = conn
            .prepare("SELECT id, scopes FROM _replicas")
            .expect("Query failed");
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .expect("Query failed");

        let mut initial_map = HashMap::new();
        for (id, scopes_str) in rows.flatten() {
            let scopes = scopes_str.split(',').map(|s| s.to_string()).collect();
            initial_map.insert(
                id.clone(),
                ReplicaInfo {
                    id,
                    scopes,
                    buffer: vec![],
                    last_seen: Instant::now(),
                    tx: None,
                },
            );
        }
        initial_map
    })
    .await
    .unwrap();

    {
        let mut map = tracker.write().await;
        *map = recovered_state;
    }

    tokio::spawn(async move {
        let mut cleanup_interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                Ok(event) = rx.recv() => {
                    if event.db_name == "logs" { continue; }
                    let mut map = tracker.write().await;
                    for (rep_id, info) in map.iter_mut() {
                        if info.scopes.contains(&event.scope) || event.scope == "root" {
                            let pb_event = pb::DbChangeEvent { scope: event.scope.clone(), db_name: event.db_name.clone(), changeset: event.changeset.clone() };
                            if let Some(tx) = &info.tx {
                                if tx.try_send(Ok(pb_event.clone())).is_err() {
                                    tracing::warn!("Replica {} disconnected. Buffering {} changesets.", info.id, info.buffer.len() + 1);
                                    info.tx = None;
                                    info.buffer.push(pb_event);
                                    info.last_seen = Instant::now();
                                } else {
                                    tracing::debug!("📤 [Master] Forwarding changeset ({}/{}) to Replica {}", event.scope, event.db_name, rep_id);
                                }
                            } else {
                                info.buffer.push(pb_event);
                            }
                        }
                    }
                }
                _ = cleanup_interval.tick() => {
                    let mut map = tracker.write().await;
                    let now = Instant::now();
                    map.retain(|id, info| {
                        if info.tx.is_none() && now.duration_since(info.last_seen) > std::time::Duration::from_secs(300) {
                            tracing::warn!("Replica {} disconnected > 5m. Dropping from master.", id);
                            false
                        } else { true }
                    });
                }
            }
        }
    });
}
