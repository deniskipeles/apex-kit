use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value as JsonValue;
use apexkit_core::batching::WriteForwarder;
use tonic::transport::{Channel, ClientTlsConfig, Certificate};
use std::sync::OnceLock;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio::sync::broadcast;
use apexkit_core::models::ChangesetEvent;
use tonic::metadata::MetadataValue;

pub mod pb {
    tonic::include_proto!("replication");
}
use pb::replication_server::Replication;

// [UPDATED] Global channel to carry the binary changeset to the Replica's applier
pub static DB_SYNC_TX: OnceLock<broadcast::Sender<pb::DbChangeEvent>> = OnceLock::new();

pub fn get_db_sync_tx() -> broadcast::Sender<pb::DbChangeEvent> {
    DB_SYNC_TX.get_or_init(|| {
        let (tx, _) = broadcast::channel(100);
        tx
    }).clone()
}

pub fn parse_db_path(path: &str) -> (String, String) {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 3 && parts[0] == "storage" {
        let db_file = parts.last().unwrap().replace(".db", "");
        if parts[1] == "system" {
            return ("root".to_string(), db_file);
        } else if parts[1] == "tenants" && parts.len() >= 4 {
            return (format!("tenant:{}", parts[2]), db_file);
        } else if parts[1] == "sandboxes" && parts.len() >= 4 {
            let sid = parts[2].replace("session_", "");
            return (format!("sandbox:{}", sid), db_file);
        }
    }
    ("unknown".to_string(), "unknown".to_string())
}

// Helper to resolve DB path from Scope & DB Name
pub fn get_db_path_from_scope(scope: &str, db_name: &str) -> String {
    match scope {
        "root" => format!("storage/system/{}.db", db_name),
        tenant if tenant.starts_with("tenant:") => {
            let id = tenant.strip_prefix("tenant:").unwrap();
            format!("storage/tenants/{}/{}.db", id, db_name)
        },
        sandbox if sandbox.starts_with("sandbox:") => {
            let id = sandbox.strip_prefix("sandbox:").unwrap();
            format!("storage/sandboxes/session_{}/{}.db", id, db_name)
        },
        _ => "".to_string(),
    }
}

// --- SECURITY INTERCEPTORS ---

pub fn client_auth_interceptor(mut req: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
    let master_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
    let token = MetadataValue::try_from(&format!("Bearer {}", master_key))
        .map_err(|_| Status::internal("Invalid master key format for metadata"))?;
    req.metadata_mut().insert("authorization", token);
    Ok(req)
}

pub fn server_auth_interceptor(req: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
    let expected_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
    match req.metadata().get("authorization") {
        Some(token) => {
            if token.to_str().unwrap_or("") == format!("Bearer {}", expected_key) {
                Ok(req)
            } else {
                Err(Status::unauthenticated("Invalid Master Key provided by Replica"))
            }
        },
        None => Err(Status::unauthenticated("Missing Master Key in Replication Request")),
    }
}

async fn build_grpc_channel(master_url: &str) -> Result<Channel, String> {
    let mut endpoint = Channel::from_shared(master_url.to_string())
        .map_err(|e| format!("Invalid Master URL: {}", e))?;

    if master_url.starts_with("https://") {
        let mut tls_config = ClientTlsConfig::new();
        if let Ok(ca_path) = std::env::var("APEX_TLS_CA_PATH") {
            if !ca_path.is_empty() {
                if let Ok(ca_cert) = tokio::fs::read(&ca_path).await {
                    let ca = Certificate::from_pem(ca_cert);
                    tls_config = tls_config.ca_certificate(ca);
                }
            }
        } else {
            tls_config = tls_config.with_native_roots();
        }
        if let Ok(domain) = std::env::var("APEX_TLS_DOMAIN_OVERRIDE") {
             if !domain.is_empty() {
                 tls_config = tls_config.domain_name(domain);
             }
        }
        endpoint = endpoint.tls_config(tls_config).map_err(|e| format!("TLS Config Error: {}", e))?;
    }
    endpoint.connect().await.map_err(|e| format!("Failed to connect: {}", e))
}

pub struct MasterReplicationService {
    pub event_tx: tokio::sync::broadcast::Sender<ChangesetEvent>,
}

#[tonic::async_trait]
impl Replication for MasterReplicationService {
    type SubscribeWalStream = ReceiverStream<Result<pb::WalFrame, Status>>;
    type FetchDbSnapshotStream = ReceiverStream<Result<pb::FileChunk, Status>>;
    type StreamEventsStream = ReceiverStream<Result<pb::DbChangeEvent, Status>>;

    async fn execute_write(&self, req: Request<pb::WriteRequest>) -> Result<Response<pb::WriteResponse>, Status> {
        let request = req.into_inner();
        if !request.db_path.starts_with("storage/") || request.db_path.contains("..") {
            return Err(Status::permission_denied("Invalid DB Path"));
        }
        let params_json: Vec<JsonValue> = serde_json::from_slice(&request.params).map_err(|_| Status::invalid_argument("Invalid params"))?;

        let mut params = Vec::new();
        for p in params_json {
            match p {
                JsonValue::Null => params.push(rusqlite::types::Value::Null),
                JsonValue::Number(n) => {
                    if let Some(i) = n.as_i64() { params.push(rusqlite::types::Value::Integer(i)); }
                    else if let Some(f) = n.as_f64() { params.push(rusqlite::types::Value::Real(f)); }
                },
                JsonValue::Bool(b) => params.push(rusqlite::types::Value::Integer(if b { 1 } else { 0 })),
                JsonValue::Object(obj) => {
                    if obj.get("__type").and_then(|v| v.as_str()) == Some("blob") {
                        if let Some(b64) = obj.get("data").and_then(|v| v.as_str()) {
                            use base64::{Engine as _, engine::general_purpose::STANDARD};
                            if let Ok(bytes) = STANDARD.decode(b64) {
                                params.push(rusqlite::types::Value::Blob(bytes));
                            } else {
                                params.push(rusqlite::types::Value::Null);
                            }
                        } else { params.push(rusqlite::types::Value::Null); }
                    } else {
                        params.push(rusqlite::types::Value::Text(serde_json::to_string(&obj).unwrap_or_default()));
                    }
                },
                JsonValue::String(s) => params.push(rusqlite::types::Value::Text(s)),
                _ => params.push(rusqlite::types::Value::Text(p.to_string())),
            }
        }

        let conn = Connection::open_with_flags(&request.db_path, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(|e| Status::internal(e.to_string()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5)).unwrap();

        let (scope, db_name) = parse_db_path(&request.db_path);
        
        // [NEW] Track the write using Session
        let mut session = rusqlite::session::Session::new(&conn).unwrap();
        session.attach::<&str>(None).unwrap();

        let mut stmt = conn.prepare(&request.sql).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let is_insert = request.sql.trim().to_uppercase().starts_with("INSERT");

        let response = if is_insert {
            stmt.execute(rusqlite::params_from_iter(params)).map_err(|e| Status::internal(e.to_string()))?;
            Ok(Response::new(pb::WriteResponse { success: true, insert_id: conn.last_insert_rowid(), error: "".into() }))
        } else {
            stmt.execute(rusqlite::params_from_iter(params)).map_err(|e| Status::internal(e.to_string()))?;
            Ok(Response::new(pb::WriteResponse { success: true, insert_id: 0, error: "".into() }))
        };

        // Broadcast Changeset
        let mut changeset_bytes = Vec::new();
        if let Ok(_) = session.changeset_strm(&mut changeset_bytes) {
            if !changeset_bytes.is_empty() {
                let _ = self.event_tx.send(ChangesetEvent { 
                    scope, 
                    db_name, 
                    changeset: changeset_bytes 
                });
            }
        }

        response
    }

    async fn subscribe_wal(&self, _req: Request<pb::WalRequest>) -> Result<Response<Self::SubscribeWalStream>, Status> {
        // Obsolete: Kept to satisfy gRPC interface contract
        let (_, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn fetch_db_snapshot(&self, req: Request<pb::SnapshotRequest>) -> Result<Response<Self::FetchDbSnapshotStream>, Status> {
        let db_path = req.into_inner().db_path;
        if !db_path.starts_with("storage/") || db_path.contains("..") { return Err(Status::permission_denied("Invalid DB Path")); }
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            // Force master to flush WAL to main DB file before sending snapshot
            if let Ok(conn) = rusqlite::Connection::open(&db_path) { let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);"); }
            if let Ok(mut file) = tokio::fs::File::open(&db_path).await {
                let mut buffer = vec![0; 128 * 1024];
                while let Ok(n) = file.read(&mut buffer).await {
                    if n == 0 { break; }
                    if tx.send(Ok(pb::FileChunk { data: buffer[..n].to_vec() })).await.is_err() { break; }
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn stream_events(&self, req: Request<tonic::Streaming<pb::EventSubscription>>) -> Result<Response<Self::StreamEventsStream>, Status> {
        let mut in_stream = req.into_inner();
        let (tx, rx) = mpsc::channel(128);
        let mut my_rx = self.event_tx.subscribe();
        
        tokio::spawn(async move {
            let mut subscribed_scopes = HashSet::new();
            loop {
                tokio::select! {
                    msg = in_stream.message() => {
                        if let Ok(Some(sub)) = msg {
                            for scope in sub.add_scopes { subscribed_scopes.insert(scope); }
                        } else { break; }
                    }
                    Ok(event) = my_rx.recv() => {
                        if subscribed_scopes.contains(&event.scope) {
                            let pb_event = pb::DbChangeEvent {
                                scope: event.scope,
                                db_name: event.db_name,
                                changeset: event.changeset, // Directly attach binary diff
                            };
                            if tx.send(Ok(pb_event)).await.is_err() { break; }
                        }
                    }
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

pub struct GrpcWriteForwarder { pub master_url: String }

#[async_trait::async_trait]
impl WriteForwarder for GrpcWriteForwarder {
    async fn forward_write(&self, db_path: String, sql: String, params: Vec<rusqlite::types::Value>) -> Result<(i64, u64), String> {
        let channel = build_grpc_channel(&self.master_url).await?;
        // [FIXED] Use interceptor
        let mut client = pb::replication_client::ReplicationClient::with_interceptor(channel, client_auth_interceptor);
        let mut json_params = Vec::new();
        
        for p in params {
            match p {
                rusqlite::types::Value::Null => json_params.push(serde_json::Value::Null),
                rusqlite::types::Value::Integer(i) => json_params.push(serde_json::json!(i)),
                rusqlite::types::Value::Real(f) => json_params.push(serde_json::json!(f)),
                rusqlite::types::Value::Text(s) => json_params.push(serde_json::json!(s)),
                rusqlite::types::Value::Blob(b) => {
                    use base64::{Engine as _, engine::general_purpose::STANDARD};
                    json_params.push(serde_json::json!({
                        "__type": "blob",
                        "data": STANDARD.encode(&b)
                    }));
                },
            }
        }
        
        let req = tonic::Request::new(pb::WriteRequest { sql, params: serde_json::to_vec(&json_params).unwrap(), db_path });
        let res = client.execute_write(req).await.map_err(|e| e.to_string())?.into_inner();
        if res.success { Ok((res.insert_id, 1)) } else { Err(res.error) }
    }
}

// [FIX] Highly robust, atomic snapshot replacement
pub async fn fetch_snapshot_from_master(master_url: &str, db_path: &str) -> Result<(), String> {
    let channel = build_grpc_channel(master_url).await?;
    // [FIXED] Use interceptor
    let mut client = pb::replication_client::ReplicationClient::with_interceptor(channel, client_auth_interceptor);
        
    let req = tonic::Request::new(pb::SnapshotRequest { db_path: db_path.to_string() });
    
    match client.fetch_db_snapshot(req).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
            }
            
            // 1. Write to temporary file
            let tmp_path = format!("{}.tmp", db_path);
            let mut file = tokio::fs::File::create(&tmp_path).await.map_err(|e| e.to_string())?;
            
            while let Ok(Some(chunk)) = stream.message().await {
                file.write_all(&chunk.data).await.map_err(|e| e.to_string())?;
            }
            file.sync_all().await.map_err(|e| e.to_string())?;
            
            // 2. Atomic replacement of main DB file
            tokio::fs::rename(&tmp_path, db_path).await.map_err(|e| e.to_string())?;
            
            // 3. Purge local WAL and SHM to guarantee SQLite reads from the clean snapshot
            let _ = tokio::fs::remove_file(format!("{}-wal", db_path)).await;
            let _ = tokio::fs::remove_file(format!("{}-shm", db_path)).await;
            
            Ok(())
        }
        Err(e) => {
            tracing::error!("❌ Failed to fetch snapshot for {}: {}", db_path, e);
            Err(e.to_string())
        }
    }
}

pub async fn ensure_replica_env(base_path: &str) {
    if let Ok(master_url) = std::env::var("APEX_MASTER_URL") {
        if !master_url.is_empty() {
            tracing::info!("🔄 [ReplicaEnv] Ensuring DB snapshot existence for path: {}", base_path);
            let dbs = ["core.db", "data.db", "logs.db", "system.db", "vectors.db"];
            for db in dbs {
                let db_path = format!("{}/{}", base_path, db);
                if !std::path::Path::new(&db_path).exists() {
                    tracing::info!("📥 [ReplicaEnv] Database {} is missing locally. Fetching snapshot from Master...", db_path);
                    let res = fetch_snapshot_from_master(&master_url, &db_path).await;
                    if let Err(e) = res {
                        tracing::error!("❌ [ReplicaEnv] Failed to fetch snapshot for {}: {}", db_path, e);
                    } else {
                        tracing::info!("✅ [ReplicaEnv] Successfully fetched snapshot for {}", db_path);
                    }
                }
            }
        }
    }
}

static EVENT_SUB_TX: OnceLock<mpsc::Sender<pb::EventSubscription>> = OnceLock::new();

pub fn add_replica_subscription(scope: &str) {
    if let Some(tx) = EVENT_SUB_TX.get() {
        let tx = tx.clone();
        let s = scope.to_string();
        tokio::spawn(async move {
            let _ = tx.send(pb::EventSubscription {
                replica_id: uuid::Uuid::new_v4().to_string(),
                add_scopes: vec![s],
            }).await;
        });
    }
}

fn get_local_scopes() -> Vec<String> {
    let mut scopes = vec!["root".to_string()];
    if let Ok(entries) = std::fs::read_dir("storage/tenants") {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Ok(name) = entry.file_name().into_string() { scopes.push(format!("tenant:{}", name)); }
                }
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir("storage/sandboxes") {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Ok(name) = entry.file_name().into_string() {
                        if name.starts_with("session_") {
                            let sid = name.strip_prefix("session_").unwrap();
                            scopes.push(format!("sandbox:{}", sid));
                        }
                    }
                }
            }
        }
    }
    scopes
}

pub async fn start_event_streamer(master_url: String) {
    tracing::info!("📡 [EventStreamer] Connected and listening for database updates.");
    
    let channel = match build_grpc_channel(&master_url).await {
        Ok(c) => c,
        Err(e) => { tracing::error!("Connect error: {}", e); return; }
    };
    
    // [FIXED] Use interceptor
    let mut client = pb::replication_client::ReplicationClient::with_interceptor(channel, client_auth_interceptor);
    
    let (sub_tx, sub_rx) = tokio::sync::mpsc::channel(32);
    EVENT_SUB_TX.set(sub_tx.clone()).unwrap();

    let initial_scopes = get_local_scopes();
    let _ = sub_tx.send(pb::EventSubscription {
        replica_id: uuid::Uuid::new_v4().to_string(),
        add_scopes: initial_scopes,
    }).await;

    let request_stream = tokio_stream::wrappers::ReceiverStream::new(sub_rx);

    match client.stream_events(tonic::Request::new(request_stream)).await {
        Ok(response) => {
            let mut stream = response.into_inner();

            // [FIXED] Pass the binary event object directly, not a string path
            while let Ok(Some(event)) = stream.message().await {
                let _ = get_db_sync_tx().send(event);
            }
        }
        Err(e) => tracing::error!("Event Stream failed: {}", e),
    }
}