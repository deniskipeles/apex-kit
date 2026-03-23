use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom, AsyncWriteExt};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value as JsonValue;
use apexkit_core::batching::WriteForwarder;

pub mod pb {
    tonic::include_proto!("replication");
}
use pb::replication_server::Replication;

pub struct MasterReplicationService;

#[tonic::async_trait]
impl Replication for MasterReplicationService {
    type SubscribeWalStream = ReceiverStream<Result<pb::WalFrame, Status>>;
    type FetchDbSnapshotStream = ReceiverStream<Result<pb::FileChunk, Status>>;

    async fn execute_write(&self, req: Request<pb::WriteRequest>) -> Result<Response<pb::WriteResponse>, Status> {
        let request = req.into_inner();
        
        // Security: Prevent path traversal
        if !request.db_path.starts_with("storage/") || request.db_path.contains("..") {
            return Err(Status::permission_denied("Invalid DB Path"));
        }
        
        let params_json: Vec<JsonValue> = serde_json::from_slice(&request.params)
            .map_err(|_| Status::invalid_argument("Invalid params"))?;

        let mut params = Vec::new();
        for p in params_json {
            match p {
                JsonValue::Null => params.push(rusqlite::types::Value::Null),
                JsonValue::Number(n) => {
                    if let Some(i) = n.as_i64() { params.push(rusqlite::types::Value::Integer(i)); }
                    else if let Some(f) = n.as_f64() { params.push(rusqlite::types::Value::Real(f)); }
                },
                JsonValue::String(s) => params.push(rusqlite::types::Value::Text(s)),
                JsonValue::Bool(b) => params.push(rusqlite::types::Value::Integer(if b { 1 } else { 0 })),
                _ => params.push(rusqlite::types::Value::Text(p.to_string())),
            }
        }

        // Connect to the specific DB identified by the path
        let conn = Connection::open_with_flags(&request.db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| Status::internal(e.to_string()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5)).unwrap();

        let mut stmt = conn.prepare(&request.sql).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let is_insert = request.sql.trim().to_uppercase().starts_with("INSERT");

        if is_insert {
            stmt.execute(rusqlite::params_from_iter(params)).map_err(|e| Status::internal(e.to_string()))?;
            Ok(Response::new(pb::WriteResponse { success: true, insert_id: conn.last_insert_rowid(), error: "".into() }))
        } else {
            let _rows = stmt.execute(rusqlite::params_from_iter(params)).map_err(|e| Status::internal(e.to_string()))?;
            Ok(Response::new(pb::WriteResponse { success: true, insert_id: 0, error: "".into() }))
        }
    }

    async fn subscribe_wal(&self, req: Request<pb::WalRequest>) -> Result<Response<Self::SubscribeWalStream>, Status> {
        let request = req.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        
        if !request.db_path.starts_with("storage/") || request.db_path.contains("..") {
            return Err(Status::permission_denied("Invalid DB Path"));
        }

        tokio::spawn(async move {
            tracing::info!("📡 Replica connected for WAL streaming: {}", request.db_path);
            let wal_path = format!("{}-wal", request.db_path);
            let mut last_offset = request.last_offset;

            loop {
                if let Ok(metadata) = tokio::fs::metadata(&wal_path).await {
                    let new_size = metadata.len();
                    if new_size > last_offset {
                        if let Ok(mut file) = tokio::fs::File::open(&wal_path).await {
                            let _ = file.seek(SeekFrom::Start(last_offset)).await;
                            let mut buffer = vec![0; (new_size - last_offset) as usize];
                            if let Ok(_) = file.read_exact(&mut buffer).await {
                                let frame = pb::WalFrame { data: buffer, new_offset: new_size };
                                if tx.send(Ok(frame)).await.is_err() {
                                    tracing::warn!("Replica disconnected from {}", wal_path);
                                    break;
                                }
                                last_offset = new_size;
                            }
                        }
                    } else if new_size < last_offset {
                        // WAL Truncated (checkpointed)
                        last_offset = 0;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn fetch_db_snapshot(&self, req: Request<pb::SnapshotRequest>) -> Result<Response<Self::FetchDbSnapshotStream>, Status> {
        let db_path = req.into_inner().db_path;
        if !db_path.starts_with("storage/") || db_path.contains("..") {
            return Err(Status::permission_denied("Invalid DB Path"));
        }
        
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            // Force a passive checkpoint to push pending WAL writes into the main DB file
            if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
            }
            
            if let Ok(mut file) = tokio::fs::File::open(&db_path).await {
                let mut buffer = vec![0; 64 * 1024]; // 64KB chunks
                while let Ok(n) = file.read(&mut buffer).await {
                    if n == 0 { break; }
                    let chunk = pb::FileChunk { data: buffer[..n].to_vec() };
                    if tx.send(Ok(chunk)).await.is_err() { break; }
                }
            }
        });
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

pub struct GrpcWriteForwarder {
    pub master_url: String,
}

#[async_trait::async_trait]
impl WriteForwarder for GrpcWriteForwarder {
    async fn forward_write(&self, db_path: String, sql: String, params: Vec<rusqlite::types::Value>) -> Result<(i64, u64), String> {
        let mut client = pb::replication_client::ReplicationClient::connect(self.master_url.clone())
            .await.map_err(|e| e.to_string())?;

        let mut json_params = Vec::new();
        for p in params {
            match p {
                rusqlite::types::Value::Null => json_params.push(serde_json::Value::Null),
                rusqlite::types::Value::Integer(i) => json_params.push(serde_json::json!(i)),
                rusqlite::types::Value::Real(f) => json_params.push(serde_json::json!(f)),
                rusqlite::types::Value::Text(s) => json_params.push(serde_json::json!(s)),
                rusqlite::types::Value::Blob(b) => json_params.push(serde_json::json!(String::from_utf8_lossy(&b).to_string())),
            }
        }

        let req = tonic::Request::new(pb::WriteRequest {
            sql,
            params: serde_json::to_vec(&json_params).unwrap(),
            db_path,
        });

        let res = client.execute_write(req).await.map_err(|e| e.to_string())?.into_inner();
        if res.success {
            Ok((res.insert_id, 1)) 
        } else {
            Err(res.error)
        }
    }
}

pub async fn start_wal_streamer(master_url: String, db_path: String) {
    tracing::info!("🔄 Starting WAL streamer for {} from master: {}", db_path, master_url);
    let mut client = match pb::replication_client::ReplicationClient::connect(master_url.clone()).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to connect to master for WAL: {}", e);
            return;
        }
    };

    let wal_path = format!("{}-wal", db_path);
    let mut last_offset = 0;
    if let Ok(meta) = tokio::fs::metadata(&wal_path).await {
        last_offset = meta.len();
    }

    let req = tonic::Request::new(pb::WalRequest {
        replica_id: uuid::Uuid::new_v4().to_string(),
        last_offset,
        db_path,
    });

    match client.subscribe_wal(req).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            while let Ok(Some(frame)) = stream.message().await {
                if let Ok(mut file) = tokio::fs::OpenOptions::new().create(true).append(true).open(&wal_path).await {
                    if let Err(e) = file.write_all(&frame.data).await {
                        tracing::error!("Failed to write WAL frame: {}", e);
                    }
                }
            }
        }
        Err(e) => tracing::error!("WAL stream error: {}", e),
    }
}

async fn fetch_snapshot_from_master(master_url: &str, db_path: &str) {
    tracing::info!("📥 Fetching initial snapshot for {} from master...", db_path);
    if let Ok(mut client) = pb::replication_client::ReplicationClient::connect(master_url.to_string()).await {
        let req = tonic::Request::new(pb::SnapshotRequest { db_path: db_path.to_string() });
        if let Ok(mut stream) = client.fetch_db_snapshot(req).await.map(|r| r.into_inner()) {
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                tokio::fs::create_dir_all(parent).await.unwrap();
            }
            if let Ok(mut file) = tokio::fs::File::create(db_path).await {
                while let Ok(Some(chunk)) = stream.message().await {
                    file.write_all(&chunk.data).await.unwrap();
                }
            }
        }
    }
}

/// The Lazy-Loader for Environments (Root, Tenant, Sandbox)
pub async fn ensure_replica_env(base_path: &str) {
    if let Ok(master_url) = std::env::var("APEX_MASTER_URL") {
        if !master_url.is_empty() {
            let dbs =["core.db", "data.db", "logs.db", "system.db", "vectors.db"];
            for db in dbs {
                let db_path = format!("{}/{}", base_path, db);
                
                // 1. Fetch snapshot if missing
                if !std::path::Path::new(&db_path).exists() {
                    fetch_snapshot_from_master(&master_url, &db_path).await;
                }
                
                // 2. Spawn WAL Streamer
                tokio::spawn(start_wal_streamer(master_url.clone(), db_path));
            }
        }
    }
}