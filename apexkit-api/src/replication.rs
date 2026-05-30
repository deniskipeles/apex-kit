use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::Value as JsonValue;
use apexkit_core::batching::WriteForwarder;
use tonic::transport::{Channel, ClientTlsConfig, Certificate};
use std::sync::OnceLock;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio::sync::broadcast;
use apexkit_core::models::ChangesetEvent;
use tonic::metadata::MetadataValue;
use std::time::{Instant, Duration};
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use apexkit_core::realtime::EventScope;
use crate::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message as AxumWsMessage};
use axum::extract::{State, Query};
use serde::{Serialize, Deserialize};
use futures_util::{SinkExt, StreamExt};

pub static USE_HTTP_FALLBACK: AtomicBool = AtomicBool::new(false);

pub mod pb {
    tonic::include_proto!("replication");
}
use pb::replication_server::Replication;

pub static DB_SYNC_TX: OnceLock<broadcast::Sender<pb::DbChangeEvent>> = OnceLock::new();
pub static EVENT_SUB_TX: OnceLock<mpsc::Sender<pb::EventSubscription>> = OnceLock::new();

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
        if parts[1] == "system" { return ("root".to_string(), db_file); } 
        else if parts[1] == "tenants" && parts.len() >= 4 { return (format!("tenant:{}", parts[2]), db_file); } 
        else if parts[1] == "sandboxes" && parts.len() >= 4 { 
            let sid = parts[2].replace("session_", "");
            return (format!("sandbox:{}", sid), db_file); 
        }
    }
    ("unknown".to_string(), "unknown".to_string())
}

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
                Err(Status::unauthenticated("Invalid Master Key"))
            }
        },
        None => Err(Status::unauthenticated("Missing Master Key")),
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
        endpoint = endpoint.tls_config(tls_config).map_err(|e| format!("TLS Config Error: {}", e))?;
    }
    endpoint.connect().await.map_err(|e| format!("Failed to connect: {}", e))
}

// --- MASTER REPLICA TRACKER ---
pub struct ReplicaInfo {
    pub id: String,
    pub scopes: HashSet<String>,
    pub buffer: Vec<pb::DbChangeEvent>,
    pub last_seen: Instant,
    pub tx: Option<tokio::sync::mpsc::Sender<Result<pb::DbChangeEvent, Status>>>,
}

static REPLICA_TRACKER: OnceLock<Arc<RwLock<HashMap<String, ReplicaInfo>>>> = OnceLock::new();
static REPLICA_ID: OnceLock<String> = OnceLock::new();

// [NEW] Global Master Changeset Transmitter Static
pub static MASTER_CHANGESET_TX: OnceLock<broadcast::Sender<ChangesetEvent>> = OnceLock::new();

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
    REPLICA_TRACKER.get_or_init(|| Arc::new(RwLock::new(HashMap::new()))).clone()
}

pub async fn register_replica_on_master(id: &str, scopes: &[String]) -> Result<(), Status> {
    let scope_list = scopes.join(",");
    let conn = Connection::open("storage/system/system.db").map_err(|e| Status::internal(e.to_string()))?;
    conn.execute("INSERT OR REPLACE INTO _replicas (id, scopes, last_seen) VALUES (?1, ?2, CURRENT_TIMESTAMP)", 
                 params![id, scope_list]).map_err(|e| Status::internal(e.to_string()))?;
    Ok(())
}

// [UPDATED] Accepts Sender directly to populate the static and subscribe internally
pub async fn init_master_replica_tracker(tx: tokio::sync::broadcast::Sender<ChangesetEvent>) {
    let _ = MASTER_CHANGESET_TX.set(tx.clone());
    let mut rx = tx.subscribe();
    
    let tracker = get_replica_tracker();
    
    // 1. Recover from DB
    let recovered_state = tokio::task::spawn_blocking(move || {
        let conn = Connection::open("storage/system/system.db").expect("Failed to open system.db");
        let mut stmt = conn.prepare("SELECT id, scopes FROM _replicas").expect("Query failed");
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).expect("Query execution failed");

        let mut initial_map = HashMap::new();
        for row in rows {
            if let Ok((id, scopes_str)) = row {
                let scopes = scopes_str.split(',').map(|s| s.to_string()).collect();
                initial_map.insert(id.clone(), ReplicaInfo {
                    id,
                    scopes,
                    buffer: vec![],
                    last_seen: Instant::now(),
                    tx: None,
                });
            }
        }
        initial_map
    }).await.unwrap();

    {
        let mut map = tracker.write().await;
        *map = recovered_state;
    }

    // 2. Track Changesets and Disconnections
    tokio::spawn(async move {
        let mut cleanup_interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            tokio::select! {
                Ok(event) = rx.recv() => {
                    let mut map = tracker.write().await;
                    for (_, info) in map.iter_mut() {
                        if info.scopes.contains(&event.scope) || event.scope == "root" {
                            let pb_event = pb::DbChangeEvent {
                                scope: event.scope.clone(),
                                db_name: event.db_name.clone(),
                                changeset: event.changeset.clone(),
                            };
                            if let Some(tx) = &info.tx {
                                if tx.try_send(Ok(pb_event.clone())).is_err() {
                                    tracing::warn!("Replica {} disconnected. Buffering {} changesets.", info.id, info.buffer.len() + 1);
                                    info.tx = None;
                                    info.buffer.push(pb_event);
                                    info.last_seen = Instant::now();
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
                        if info.tx.is_none() && now.duration_since(info.last_seen) > Duration::from_secs(300) {
                            tracing::warn!("Replica {} disconnected > 5m. Dropping from master.", id);
                            false
                        } else {
                            true
                        }
                    });
                }
            }
        }
    });
}

// --- SHARED MASTER LOGIC ---

pub async fn process_master_write(event_tx: &tokio::sync::broadcast::Sender<ChangesetEvent>, db_path: &str, sql: &str, params_bytes: &[u8]) -> Result<(i64, String), String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5)).unwrap();

    let (scope, db_name) = parse_db_path(db_path);
    let mut session = rusqlite::session::Session::new(&conn).unwrap();
    session.attach::<&str>(None).unwrap();

    let params_json: Vec<JsonValue> = serde_json::from_slice(params_bytes).map_err(|_| "Invalid params".to_string())?;
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
                        } else { params.push(rusqlite::types::Value::Null); }
                    } else { params.push(rusqlite::types::Value::Null); }
                } else {
                    params.push(rusqlite::types::Value::Text(serde_json::to_string(&obj).unwrap_or_default()));
                }
            },
            JsonValue::String(s) => params.push(rusqlite::types::Value::Text(s)),
            _ => params.push(rusqlite::types::Value::Text(p.to_string())),
        }
    }

    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let is_insert = sql.trim().to_uppercase().starts_with("INSERT");

    let insert_id = if is_insert {
        stmt.execute(rusqlite::params_from_iter(params)).map_err(|e| e.to_string())?;
        conn.last_insert_rowid()
    } else {
        stmt.execute(rusqlite::params_from_iter(params)).map_err(|e| e.to_string())?;
        0
    };

    // Broadcast
    let mut changeset_bytes = Vec::new();
    if let Ok(_) = session.changeset_strm(&mut changeset_bytes) {
        let _ = event_tx.send(ChangesetEvent { scope, db_name, changeset: changeset_bytes });
    }
    
    Ok((insert_id, "".into()))
}

pub async fn process_master_sync_file(state: &AppState, scope_str: &str, filename: &str, mime_type: &str, data: &[u8]) -> Result<(), String> {
    let scope = if scope_str == "root" { EventScope::Root } 
    else if let Some(tid) = scope_str.strip_prefix("tenant:") { EventScope::Tenant(tid.to_string()) } 
    else if let Some(sid) = scope_str.strip_prefix("sandbox:") { EventScope::Sandbox(sid.to_string()) } 
    else { EventScope::Root };

    use apexkit_core::storage::StorageBackend;
    let storage = crate::storage::ScopedDynamicStorage::new(state.clone(), scope);
    
    storage.save(filename, data, mime_type).await.map_err(|e| e.to_string())?;
    Ok(())
}

// --- WS & REST FALLBACK IMPLEMENTATIONS (SERVER) ---

// [NEW] Dedicated Middleware for HTTP/WS Replication Routes (Matches gRPC Interceptor)
pub async fn master_auth_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let expected_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
    
    // Prevent empty master key bypass
    if expected_key.is_empty() {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    let auth_header = req.headers().get("authorization").and_then(|h| h.to_str().ok()).unwrap_or("");
    let expected = format!("Bearer {}", expected_key);
    
    if auth_header != expected {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    
    Ok(next.run(req).await)
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "payload")]
pub enum WsReplMsg {
    Subscribe { replica_id: String, add_scopes: Vec<String> },
    Ping,
    Pong,
    DbEvent { scope: String, db_name: String, changeset: String }, // Base64
    FullSyncRequired,
}

#[derive(Deserialize, Serialize)]
pub struct FallbackWriteReq { pub sql: String, pub params: Vec<u8>, pub db_path: String }
#[derive(Deserialize, Serialize)]
pub struct FallbackWriteRes { pub success: bool, pub insert_id: i64, pub error: String }

#[derive(Deserialize)]
pub struct FallbackSnapshotReq { pub db_path: String }

#[derive(Deserialize, Serialize)]
pub struct FallbackSyncFileReq { pub scope: String, pub filename: String, pub mime_type: String, pub data: String } // Base64 data
#[derive(Deserialize, Serialize)]
pub struct FallbackSyncFileRes { pub success: bool, pub error: String }

pub async fn fallback_write_handler(
    State(_state): State<AppState>,
    axum::extract::Json(req): axum::extract::Json<FallbackWriteReq>
) -> Result<axum::Json<FallbackWriteRes>, axum::http::StatusCode> {
    // Fetch the replication event channel from the global static
    let event_tx = MASTER_CHANGESET_TX.get().ok_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    
    match process_master_write(event_tx, &req.db_path, &req.sql, &req.params).await {
        Ok((insert_id, error)) => Ok(axum::Json(FallbackWriteRes { success: true, insert_id, error })),
        Err(e) => Ok(axum::Json(FallbackWriteRes { success: false, insert_id: 0, error: e }))
    }
}

pub async fn fallback_snapshot_handler(
    Query(req): Query<FallbackSnapshotReq>
) -> Result<axum::response::Response, axum::http::StatusCode> {
    // Auth handled by master_auth_middleware
    if !req.db_path.starts_with("storage/") || req.db_path.contains("..") { return Err(axum::http::StatusCode::FORBIDDEN); }
    
    if let Ok(conn) = rusqlite::Connection::open(&req.db_path) { let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);"); }
    
    let file = tokio::fs::File::open(&req.db_path).await.map_err(|_| axum::http::StatusCode::NOT_FOUND)?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);
    
    Ok(axum::response::Response::builder()
        .header("Content-Type", "application/octet-stream")
        .body(body)
        .unwrap())
}

pub async fn fallback_sync_file_handler(
    State(state): State<AppState>,
    axum::extract::Json(req): axum::extract::Json<FallbackSyncFileReq>
) -> Result<axum::Json<FallbackSyncFileRes>, axum::http::StatusCode> {
    // Auth handled by master_auth_middleware
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let data = STANDARD.decode(&req.data).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    
    match process_master_sync_file(&state, &req.scope, &req.filename, &req.mime_type, &data).await {
        Ok(_) => Ok(axum::Json(FallbackSyncFileRes { success: true, error: "".into() })),
        Err(e) => Ok(axum::Json(FallbackSyncFileRes { success: false, error: e }))
    }
}

pub async fn ws_replication_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> axum::response::Response {
    // Auth handled by master_auth_middleware
    ws.on_upgrade(move |socket| handle_ws_replica(socket, state))
}

async fn handle_ws_replica(mut socket: WebSocket, _state: AppState) {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let tracker = get_replica_tracker();
    let mut current_replica_id = String::new();
    
    loop {
        tokio::select! {
            Some(Ok(msg)) = socket.recv() => {
                if let AxumWsMessage::Text(t) = msg {
                    if let Ok(ws_msg) = serde_json::from_str::<WsReplMsg>(&t) {
                        match ws_msg {
                            WsReplMsg::Subscribe { replica_id, add_scopes } => {
                                current_replica_id = replica_id.clone();
                                let _ = register_replica_on_master(&replica_id, &add_scopes).await;
                                
                                let require_full_sync = {
                                    let mut map = tracker.write().await;
                                    if let Some(info) = map.get_mut(&replica_id) {
                                        if !info.buffer.is_empty() {
                                            use base64::{Engine as _, engine::general_purpose::STANDARD};
                                            for evt in info.buffer.drain(..) {
                                                let out = WsReplMsg::DbEvent { scope: evt.scope, db_name: evt.db_name, changeset: STANDARD.encode(&evt.changeset) };
                                                let _ = socket.send(AxumWsMessage::Text(serde_json::to_string(&out).unwrap().into())).await;
                                            }
                                        }
                                        info.tx = Some(tx.clone());
                                        info.scopes.extend(add_scopes);
                                        false
                                    } else {
                                        map.insert(replica_id.clone(), ReplicaInfo { id: replica_id.clone(), scopes: add_scopes.into_iter().collect(), buffer: vec![], last_seen: Instant::now(), tx: Some(tx.clone()) });
                                        true
                                    }
                                };
                                if require_full_sync {
                                    let _ = socket.send(AxumWsMessage::Text(serde_json::to_string(&WsReplMsg::FullSyncRequired).unwrap().into())).await;
                                }
                            }
                            WsReplMsg::Ping => {
                                if !current_replica_id.is_empty() {
                                    if let Some(info) = tracker.write().await.get_mut(&current_replica_id) { info.last_seen = Instant::now(); }
                                }
                                let _ = socket.send(AxumWsMessage::Text(serde_json::to_string(&WsReplMsg::Pong).unwrap().into())).await;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some(Ok(evt)) = rx.recv() => {
                use base64::{Engine as _, engine::general_purpose::STANDARD};
                let out = WsReplMsg::DbEvent { scope: evt.scope, db_name: evt.db_name, changeset: STANDARD.encode(&evt.changeset) };
                if socket.send(AxumWsMessage::Text(serde_json::to_string(&out).unwrap().into())).await.is_err() { break; }
            }
            else => break,
        }
    }
    
    if !current_replica_id.is_empty() {
        if let Some(info) = tracker.write().await.get_mut(&current_replica_id) {
            info.tx = None;
            info.last_seen = Instant::now();
        }
    }
}

// [UPDATED] MasterReplicationService now holds AppState
pub struct MasterReplicationService { 
    pub event_tx: tokio::sync::broadcast::Sender<ChangesetEvent>,
    pub state: AppState, 
}

#[tonic::async_trait]
impl Replication for MasterReplicationService {
    type FetchDbSnapshotStream = ReceiverStream<Result<pb::FileChunk, Status>>;
    type StreamEventsStream = ReceiverStream<Result<pb::DbChangeEvent, Status>>;

    async fn execute_write(&self, req: Request<pb::WriteRequest>) -> Result<Response<pb::WriteResponse>, Status> {
        let request = req.into_inner();
        
        match process_master_write(&self.event_tx, &request.db_path, &request.sql, &request.params).await {
            Ok((insert_id, error)) => {
                Ok(Response::new(pb::WriteResponse { success: true, insert_id, error }))
            },
            Err(e) => {
                Err(Status::internal(e))
            }
        }
    }

    async fn fetch_db_snapshot(&self, req: Request<pb::SnapshotRequest>) -> Result<Response<Self::FetchDbSnapshotStream>, Status> {
        let db_path = req.into_inner().db_path;
        if !db_path.starts_with("storage/") || db_path.contains("..") { return Err(Status::permission_denied("Invalid DB Path")); }
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
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
        let (tx, rx) = mpsc::channel(1024);
        
        let first_msg = in_stream.message().await.map_err(|_| Status::internal("Stream error"))?;
        let sub = first_msg.ok_or(Status::invalid_argument("Missing initial subscription"))?;
        
        let replica_id = sub.replica_id.clone();
        
        if let Err(e) = register_replica_on_master(&replica_id, &sub.add_scopes).await {
            tracing::error!("Failed to register replica in DB: {}", e);
        }

        let tracker = get_replica_tracker();
        
        // Register/Restore State
        let require_full_sync = {
            let mut map = tracker.write().await;
            if let Some(info) = map.get_mut(&replica_id) {
                let buffered_count = info.buffer.len();
                if buffered_count > 0 {
                    tracing::info!("🔄 Replaying {} missed changesets to Replica {}", buffered_count, replica_id);
                    for evt in info.buffer.drain(..) { 
                        let _ = tx.try_send(Ok(evt)); 
                    }
                } else {
                    tracing::info!("✅ Replica {} reconnected successfully (No missed changesets).", replica_id);
                }
                info.tx = Some(tx.clone());
                info.scopes.extend(sub.add_scopes.clone());
                false
            } else {
                tracing::info!("🌟 New Replica connected: {}", replica_id);
                map.insert(replica_id.clone(), ReplicaInfo {
                    id: replica_id.clone(),
                    scopes: sub.add_scopes.into_iter().collect(),
                    buffer: vec![],
                    last_seen: Instant::now(),
                    tx: Some(tx.clone()),
                });
                true
            }
        };

        if require_full_sync {
            let _ = tx.send(Ok(pb::DbChangeEvent { scope: "system".to_string(), db_name: "FULL_SYNC_REQUIRED".to_string(), changeset: vec![] })).await;
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

    // [NEW] Handles file replication from Replicas to Master via gRPC
    async fn sync_file(&self, req: Request<pb::SyncFileRequest>) -> Result<Response<pb::SyncFileResponse>, Status> {
        let request = req.into_inner();
        
        match process_master_sync_file(&self.state, &request.scope, &request.filename, &request.mime_type, &request.data).await {
            Ok(_) => Ok(Response::new(pb::SyncFileResponse { success: true, error: "".into() })),
            Err(e) => Ok(Response::new(pb::SyncFileResponse { success: false, error: e.to_string() }))
        }
    }
}

pub struct GrpcWriteForwarder { 
    pub master_url: String,
    pub channel: Arc<RwLock<Option<Channel>>>,
}

impl GrpcWriteForwarder {
    pub fn new(master_url: String) -> Self {
        Self { master_url, channel: Arc::new(RwLock::new(None)) }
    }
    
    async fn get_channel(&self) -> Result<Channel, String> {
        {
            let lock = self.channel.read().await;
            if let Some(ch) = &*lock {
                return Ok(ch.clone());
            }
        }
        let mut lock = self.channel.write().await;
        if let Some(ch) = &*lock {
            return Ok(ch.clone());
        }
        let ch = build_grpc_channel(&self.master_url).await?;
        *lock = Some(ch.clone());
        Ok(ch)
    }
}

#[async_trait::async_trait]
impl WriteForwarder for GrpcWriteForwarder {
    async fn forward_write(&self, db_path: String, sql: String, params: Vec<rusqlite::types::Value>) -> Result<(i64, u64), String> {
        if USE_HTTP_FALLBACK.load(Ordering::SeqCst) {
            let url = format!("{}/replication/write", self.master_url.trim_end_matches('/'));
            let master_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
            let client = reqwest::Client::new();
            
            let mut json_params = Vec::new();
            for p in params {
                match p {
                    rusqlite::types::Value::Null => json_params.push(serde_json::Value::Null),
                    rusqlite::types::Value::Integer(i) => json_params.push(serde_json::json!(i)),
                    rusqlite::types::Value::Real(f) => json_params.push(serde_json::json!(f)),
                    rusqlite::types::Value::Text(s) => json_params.push(serde_json::json!(s)),
                    rusqlite::types::Value::Blob(b) => {
                        use base64::{Engine as _, engine::general_purpose::STANDARD};
                        json_params.push(serde_json::json!({ "__type": "blob", "data": STANDARD.encode(&b) }));
                    },
                }
            }
            
            let req = FallbackWriteReq { sql, params: serde_json::to_vec(&json_params).unwrap(), db_path };
            let res: FallbackWriteRes = client.post(&url).header("Authorization", format!("Bearer {}", master_key)).json(&req).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
            if res.success { return Ok((res.insert_id, 1)); } else { return Err(res.error); }
        }

        let channel = self.get_channel().await?;
        // [FIX] Ensure message size is uncapped
        let mut client = pb::replication_client::ReplicationClient::with_interceptor(channel, client_auth_interceptor)
            .max_decoding_message_size(100 * 1024 * 1024)
            .max_encoding_message_size(100 * 1024 * 1024);
            
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

// [NEW] Helper to push a file up to the Master node via gRPC
pub async fn forward_file_to_master(master_url: &str, scope: &str, filename: &str, mime: &str, data: &[u8]) -> Result<(), String> {
    if USE_HTTP_FALLBACK.load(Ordering::SeqCst) {
        let url = format!("{}/replication/sync-file", master_url.trim_end_matches('/'));
        let master_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
        let client = reqwest::Client::new();
        
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let req = FallbackSyncFileReq {
            scope: scope.to_string(),
            filename: filename.to_string(),
            mime_type: mime.to_string(),
            data: STANDARD.encode(data),
        };
        
        let res: FallbackSyncFileRes = client.post(&url).header("Authorization", format!("Bearer {}", master_key)).json(&req).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
        if res.success { return Ok(()); } else { return Err(res.error); }
    }

    let channel = build_grpc_channel(master_url).await?;
    let mut client = pb::replication_client::ReplicationClient::with_interceptor(channel, client_auth_interceptor)
        .max_decoding_message_size(100 * 1024 * 1024) // 100 MB limit
        .max_encoding_message_size(100 * 1024 * 1024);
    
    let req = tonic::Request::new(pb::SyncFileRequest {
        scope: scope.to_string(),
        filename: filename.to_string(),
        mime_type: mime.to_string(),
        data: data.to_vec(),
    });

    let res = client.sync_file(req).await.map_err(|e| e.to_string())?.into_inner();
    
    if res.success {
        Ok(())
    } else {
        Err(res.error)
    }
}

pub async fn fetch_snapshot_from_master(master_url: &str, db_path: &str) -> Result<(), String> {
    if USE_HTTP_FALLBACK.load(Ordering::SeqCst) {
        let url = format!("{}/replication/snapshot?db_path={}", master_url.trim_end_matches('/'), db_path);
        let master_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
        let client = reqwest::Client::new();
        
        let mut response = client.get(&url).header("Authorization", format!("Bearer {}", master_key)).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() { return Err("HTTP Snapshot download failed".into()); }
        
        if let Some(parent) = std::path::Path::new(db_path).parent() { tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?; }
        
        let tmp_path = format!("{}.tmp", db_path);
        let mut file = tokio::fs::File::create(&tmp_path).await.map_err(|e| e.to_string())?;
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? { file.write_all(&chunk).await.map_err(|e| e.to_string())?; }
        file.sync_all().await.map_err(|e| e.to_string())?;
        
        tokio::fs::rename(&tmp_path, db_path).await.map_err(|e| e.to_string())?;
        let _ = tokio::fs::remove_file(format!("{}-wal", db_path)).await;
        let _ = tokio::fs::remove_file(format!("{}-shm", db_path)).await;
        
        return Ok(());
    }

    let channel = build_grpc_channel(master_url).await?;
    let mut client = pb::replication_client::ReplicationClient::with_interceptor(channel, client_auth_interceptor)
        .max_decoding_message_size(100 * 1024 * 1024)
        .max_encoding_message_size(100 * 1024 * 1024);
        
    let req = tonic::Request::new(pb::SnapshotRequest { db_path: db_path.to_string() });
    
    match client.fetch_db_snapshot(req).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
            }
            
            let tmp_path = format!("{}.tmp", db_path);
            let mut file = tokio::fs::File::create(&tmp_path).await.map_err(|e| e.to_string())?;
            
            while let Ok(Some(chunk)) = stream.message().await {
                file.write_all(&chunk.data).await.map_err(|e| e.to_string())?;
            }
            file.sync_all().await.map_err(|e| e.to_string())?;
            
            tokio::fs::rename(&tmp_path, db_path).await.map_err(|e| e.to_string())?;
            
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

// Ensure local files exist. Only run once at startup.
pub async fn ensure_replica_env(base_path: &str) {
    do_sync_env(base_path, false).await;
}

// Force a full sync from master (e.g. after prolonged downtime)
pub async fn force_replica_sync(base_path: &str) {
    do_sync_env(base_path, true).await;
}

async fn do_sync_env(base_path: &str, force: bool) {
    if let Ok(master_url) = std::env::var("APEX_MASTER_URL") {
        if !master_url.is_empty() {
            if force {
                tracing::warn!("🔄 [ReplicaEnv] FORCING DB snapshot sync for path: {}", base_path);
            } else {
                tracing::info!("🔄 [ReplicaEnv] Ensuring DB snapshot existence for path: {}", base_path);
            }
            
            let dbs = ["core.db", "data.db", "logs.db", "system.db", "vectors.db"];
            for db in dbs {
                let db_path = format!("{}/{}", base_path, db);
                if force || !std::path::Path::new(&db_path).exists() {
                    tracing::info!("📥 [ReplicaEnv] Fetching snapshot for {} from Master...", db_path);
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

pub fn add_replica_subscription(scope: &str) {
    if let Some(tx) = EVENT_SUB_TX.get() {
        let tx = tx.clone();
        let s = scope.to_string();
        if let Some(replica_id) = REPLICA_ID.get() {
            let r_id = replica_id.clone();
            tokio::spawn(async move {
                let _ = tx.send(pb::EventSubscription {
                    replica_id: r_id,
                    add_scopes: vec![s],
                }).await;
            });
        }
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

pub async fn start_event_streamer(master_url: String, state: Option<crate::AppState>) {
    let replica_id = init_replica_id().await;
    tracing::info!("📡 [EventStreamer] Connected as Replica ID: {}", replica_id);
    
    let channel = match build_grpc_channel(&master_url).await {
        Ok(c) => c,
        Err(e) => { 
            tracing::warn!("gRPC connection failed ({}). Falling back to HTTP/WS Replication.", e);
            USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
            start_ws_event_streamer(master_url, state, replica_id).await;
            return; 
        }
    };
    
    let mut client = pb::replication_client::ReplicationClient::with_interceptor(channel, client_auth_interceptor)
        .max_decoding_message_size(100 * 1024 * 1024)
        .max_encoding_message_size(100 * 1024 * 1024);
    
    let (sub_tx, sub_rx) = tokio::sync::mpsc::channel(32);
    let _ = EVENT_SUB_TX.set(sub_tx.clone());

    let initial_scopes = get_local_scopes();
    let _ = sub_tx.send(pb::EventSubscription {
        replica_id: replica_id.clone(),
        add_scopes: initial_scopes,
    }).await;

    let request_stream = tokio_stream::wrappers::ReceiverStream::new(sub_rx);

    match client.stream_events(tonic::Request::new(request_stream)).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            while let Ok(Some(event)) = stream.message().await {
                if event.db_name == "FULL_SYNC_REQUIRED" {
                    tracing::warn!("Master requested FULL_SYNC due to prolonged disconnection (> 5m).");
                    force_replica_sync("storage/system").await;
                    if let Some(s) = &state {
                        let _ = s.db.reload_connections().await;
                    }
                    continue;
                }
                let _ = get_db_sync_tx().send(event);
            }
        }
        Err(e) => {
            tracing::warn!("gRPC Stream encountered errors ({}). Switching to WS Replication Fallback.", e);
            USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
            start_ws_event_streamer(master_url, state, replica_id).await;
        }
    }
}

pub async fn start_ws_event_streamer(master_url: String, state: Option<crate::AppState>, replica_id: String) {
    let ws_url = format!("{}/replication/ws", master_url.replace("http://", "ws://").replace("https://", "wss://").trim_end_matches('/'));
    let master_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
    
    loop {
        tracing::info!("📡 [WS Streamer] Connecting to {}", ws_url);
        
        let request = axum::http::Request::builder()
            .uri(&ws_url)
            .header("Authorization", format!("Bearer {}", master_key))
            .body(())
            .unwrap();

        match tokio_tungstenite::connect_async(request).await {
            Ok((mut ws_stream, _)) => {
                tracing::info!("✅ [WS Streamer] Connected successfully");
                
                let sub_msg = WsReplMsg::Subscribe { replica_id: replica_id.clone(), add_scopes: get_local_scopes() };
                let _ = ws_stream.send(tokio_tungstenite::tungstenite::protocol::Message::Text(serde_json::to_string(&sub_msg).unwrap().into())).await;
                
                let mut ping_interval = tokio::time::interval(Duration::from_secs(25));
                
                loop {
                    tokio::select! {
                        _ = ping_interval.tick() => {
                            if ws_stream.send(tokio_tungstenite::tungstenite::protocol::Message::Text(serde_json::to_string(&WsReplMsg::Ping).unwrap().into())).await.is_err() { break; }
                        }
                        msg = ws_stream.next() => {
                            match msg {
                                Some(Ok(tokio_tungstenite::tungstenite::protocol::Message::Text(t))) => {
                                    if let Ok(ws_msg) = serde_json::from_str::<WsReplMsg>(&t) {
                                        match ws_msg {
                                            WsReplMsg::DbEvent { scope, db_name, changeset } => {
                                                use base64::{Engine as _, engine::general_purpose::STANDARD};
                                                if let Ok(bytes) = STANDARD.decode(&changeset) { 
                                                    let _ = get_db_sync_tx().send(pb::DbChangeEvent { scope, db_name, changeset: bytes }); 
                                                }
                                            }
                                            WsReplMsg::FullSyncRequired => {
                                                force_replica_sync("storage/system").await;
                                                if let Some(s) = &state { let _ = s.db.reload_connections().await; }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                Some(Err(e)) => { tracing::error!("WS Error: {}", e); break; }
                                None => break,
                                _ => {}
                            }
                        }
                    }
                }
            }
            Err(e) => tracing::error!("❌ [WS Streamer] Connection failed: {}", e),
        }
        
        tracing::warn!("🔄 [WS Streamer] Reconnecting in 5 seconds...");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}