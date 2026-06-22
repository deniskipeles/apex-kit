use super::pb;
use super::tracker::{ReplicaInfo, get_replica_tracker, register_replica_on_master};
use crate::AppState;
use apexkit_core::models::ChangesetEvent;
use apexkit_core::realtime::EventScope;
use pb::replication_server::Replication;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value as JsonValue;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

pub async fn process_master_write(
    event_tx: &tokio::sync::broadcast::Sender<ChangesetEvent>,
    db_path: &str,
    sql: &str,
    params_bytes: &[u8],
) -> Result<(i64, String), String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .unwrap();

    let (scope, db_name) = super::parse_db_path(db_path);
    let mut session = rusqlite::session::Session::new(&conn).unwrap();
    session.attach::<&str>(None).unwrap();

    let params_json: Vec<JsonValue> =
        serde_json::from_slice(params_bytes).map_err(|_| "Invalid params".to_string())?;
    let mut params = Vec::new();
    for p in params_json {
        match p {
            JsonValue::Null => params.push(rusqlite::types::Value::Null),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    params.push(rusqlite::types::Value::Integer(i));
                } else if let Some(f) = n.as_f64() {
                    params.push(rusqlite::types::Value::Real(f));
                }
            }
            JsonValue::Bool(b) => {
                params.push(rusqlite::types::Value::Integer(if b { 1 } else { 0 }))
            }
            JsonValue::Object(obj) => {
                if obj.get("__type").and_then(|v| v.as_str()) == Some("blob") {
                    if let Some(b64) = obj.get("data").and_then(|v| v.as_str()) {
                        use base64::{Engine as _, engine::general_purpose::STANDARD};
                        if let Ok(bytes) = STANDARD.decode(b64) {
                            params.push(rusqlite::types::Value::Blob(bytes));
                        } else {
                            params.push(rusqlite::types::Value::Null);
                        }
                    } else {
                        params.push(rusqlite::types::Value::Null);
                    }
                } else {
                    params.push(rusqlite::types::Value::Text(
                        serde_json::to_string(&obj).unwrap_or_default(),
                    ));
                }
            }
            JsonValue::String(s) => params.push(rusqlite::types::Value::Text(s)),
            _ => params.push(rusqlite::types::Value::Text(p.to_string())),
        }
    }

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let is_insert = sql.trim().to_uppercase().starts_with("INSERT");

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e.to_string());
        }
    };

    let insert_id = if is_insert {
        if let Err(e) = stmt.execute(rusqlite::params_from_iter(params)) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e.to_string());
        }
        conn.last_insert_rowid()
    } else {
        if let Err(e) = stmt.execute(rusqlite::params_from_iter(params)) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e.to_string());
        }
        0
    };

    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;

    if db_name != "logs" {
        let mut changeset_bytes = Vec::new();
        if session.changeset_strm(&mut changeset_bytes).is_ok() && !changeset_bytes.is_empty() {
            tracing::debug!(
                "🔄 [Master] Generated changeset for {}/{} ({} bytes)",
                scope,
                db_name,
                changeset_bytes.len()
            );
            let _ = event_tx.send(ChangesetEvent {
                scope,
                db_name,
                changeset: changeset_bytes,
            });
        }
    }
    Ok((insert_id, "".into()))
}

pub async fn process_master_sync_file(
    state: &AppState,
    scope_str: &str,
    filename: &str,
    mime_type: &str,
    data: &[u8],
) -> Result<(), String> {
    let scope = if scope_str == "root" {
        EventScope::Root
    } else if let Some(tid) = scope_str.strip_prefix("tenant:") {
        EventScope::Tenant(tid.to_string())
    } else if let Some(sid) = scope_str.strip_prefix("sandbox:") {
        EventScope::Sandbox(sid.to_string())
    } else {
        EventScope::Root
    };

    use apexkit_core::storage::StorageBackend;
    let storage = crate::storage::ScopedDynamicStorage::new(state.clone(), scope);
    storage
        .save(filename, data, mime_type)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub struct MasterReplicationService {
    pub event_tx: tokio::sync::broadcast::Sender<ChangesetEvent>,
    pub state: AppState,
}

#[tonic::async_trait]
impl Replication for MasterReplicationService {
    type FetchDbSnapshotStream = ReceiverStream<Result<pb::FileChunk, Status>>;
    type StreamEventsStream = ReceiverStream<Result<pb::DbChangeEvent, Status>>;

    async fn execute_write(
        &self,
        req: Request<pb::WriteRequest>,
    ) -> Result<Response<pb::WriteResponse>, Status> {
        let request = req.into_inner();
        match process_master_write(
            &self.event_tx,
            &request.db_path,
            &request.sql,
            &request.params,
        )
        .await
        {
            Ok((insert_id, error)) => Ok(Response::new(pb::WriteResponse {
                success: true,
                insert_id,
                error,
            })),
            Err(e) => Err(Status::internal(e)),
        }
    }

    async fn fetch_db_snapshot(
        &self,
        req: Request<pb::SnapshotRequest>,
    ) -> Result<Response<Self::FetchDbSnapshotStream>, Status> {
        let db_path = req.into_inner().db_path;
        if !db_path.starts_with("storage/")
            || db_path.contains("..")
            || db_path.ends_with("logs.db")
        {
            return Err(Status::permission_denied("Invalid DB Path"));
        }
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
            }
            use tokio::io::AsyncReadExt;
            if let Ok(mut file) = tokio::fs::File::open(&db_path).await {
                let mut buffer = vec![0; 128 * 1024];
                while let Ok(n) = file.read(&mut buffer).await {
                    if n == 0 {
                        break;
                    }
                    if tx
                        .send(Ok(pb::FileChunk {
                            data: buffer[..n].to_vec(),
                        }))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn stream_events(
        &self,
        req: Request<tonic::Streaming<pb::EventSubscription>>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let mut in_stream = req.into_inner();
        let (tx, rx) = mpsc::channel(1024);

        let first_msg = in_stream
            .message()
            .await
            .map_err(|_| Status::internal("Stream error"))?;
        let sub = first_msg.ok_or(Status::invalid_argument("Missing initial subscription"))?;

        let replica_id = sub.replica_id.clone();
        if let Err(e) = register_replica_on_master(&replica_id, &sub.add_scopes).await {
            tracing::error!("Failed to register replica in DB: {}", e);
        }

        let tracker = get_replica_tracker();
        let require_full_sync = {
            let mut map = tracker.write().await;
            if let Some(info) = map.get_mut(&replica_id) {
                let buffered_count = info.buffer.len();
                if buffered_count > 0 {
                    for evt in info.buffer.drain(..) {
                        let _ = tx.try_send(Ok(evt));
                    }
                }
                info.tx = Some(tx.clone());
                info.scopes.extend(sub.add_scopes.clone());
                false
            } else {
                map.insert(
                    replica_id.clone(),
                    ReplicaInfo {
                        id: replica_id.clone(),
                        scopes: sub.add_scopes.into_iter().collect(),
                        buffer: vec![],
                        last_seen: Instant::now(),
                        tx: Some(tx.clone()),
                    },
                );
                true
            }
        };

        if require_full_sync {
            let _ = tx
                .send(Ok(pb::DbChangeEvent {
                    scope: "system".to_string(),
                    db_name: "FULL_SYNC_REQUIRED".to_string(),
                    changeset: vec![],
                }))
                .await;
        }

        let tracker_clone = tracker.clone();
        let rid_clone = replica_id.clone();

        tokio::spawn(async move {
            while let Ok(Some(msg)) = in_stream.message().await {
                let mut map = tracker_clone.write().await;
                if let Some(info) = map.get_mut(&rid_clone) {
                    info.scopes.extend(msg.add_scopes);
                }
            }
            let mut map = tracker_clone.write().await;
            if let Some(info) = map.get_mut(&rid_clone) {
                info.tx = None;
                info.last_seen = Instant::now();
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn sync_file(
        &self,
        req: Request<pb::SyncFileRequest>,
    ) -> Result<Response<pb::SyncFileResponse>, Status> {
        let request = req.into_inner();
        match process_master_sync_file(
            &self.state,
            &request.scope,
            &request.filename,
            &request.mime_type,
            &request.data,
        )
        .await
        {
            Ok(_) => Ok(Response::new(pb::SyncFileResponse {
                success: true,
                error: "".into(),
            })),
            Err(e) => Ok(Response::new(pb::SyncFileResponse {
                success: false,
                error: e.to_string(),
            })),
        }
    }
}
