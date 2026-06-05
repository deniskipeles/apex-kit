use crate::AppState;
use apexkit_core::batching::WriteForwarder;
use apexkit_core::models::ChangesetEvent;
use apexkit_core::realtime::EventScope;
use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use futures_util::{SinkExt, StreamExt};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt}; // [FIX] Trait imports now in scope
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::metadata::MetadataValue;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};
use tonic::{Request, Response, Status};

pub static USE_HTTP_FALLBACK: AtomicBool = AtomicBool::new(false);

pub fn is_http_fallback() -> bool {
    if std::env::var("APEX_FORCE_HTTP_REPLICATION").unwrap_or_default() == "true" {
        return true;
    }
    USE_HTTP_FALLBACK.load(Ordering::SeqCst)
}

pub mod pb {
    tonic::include_proto!("replication");
}
use pb::replication_server::Replication;

// --- GLOBALS ---
pub static DB_SYNC_TX: OnceLock<broadcast::Sender<pb::DbChangeEvent>> = OnceLock::new();
pub static EVENT_SUB_TX: OnceLock<broadcast::Sender<pb::EventSubscription>> = OnceLock::new();
pub static MASTER_CHANGESET_TX: OnceLock<broadcast::Sender<ChangesetEvent>> = OnceLock::new();

pub static WS_OUTBOUND_TX: OnceLock<RwLock<Option<mpsc::Sender<WsReplMsg>>>> = OnceLock::new();
pub static PENDING_WS_REQS: OnceLock<
    Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<FallbackWriteRes>>>>,
> = OnceLock::new();

pub fn get_db_sync_tx() -> broadcast::Sender<pb::DbChangeEvent> {
    DB_SYNC_TX
        .get_or_init(|| {
            let (tx, _) = broadcast::channel(100);
            tx
        })
        .clone()
}

pub fn get_event_sub_tx() -> broadcast::Sender<pb::EventSubscription> {
    EVENT_SUB_TX
        .get_or_init(|| {
            let (tx, _) = broadcast::channel(100);
            tx
        })
        .clone()
}

pub fn get_pending_reqs()
-> Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<FallbackWriteRes>>>> {
    PENDING_WS_REQS
        .get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
        .clone()
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

pub fn get_db_path_from_scope(scope: &str, db_name: &str) -> String {
    match scope {
        "root" => format!("storage/system/{}.db", db_name),
        tenant if tenant.starts_with("tenant:") => {
            let id = tenant.strip_prefix("tenant:").unwrap();
            format!("storage/tenants/{}/{}.db", id, db_name)
        }
        sandbox if sandbox.starts_with("sandbox:") => {
            let id = sandbox.strip_prefix("sandbox:").unwrap();
            format!("storage/sandboxes/session_{}/{}.db", id, db_name)
        }
        _ => "".to_string(),
    }
}

pub fn client_auth_interceptor(mut req: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
    let master_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
    let token = MetadataValue::try_from(&format!("Bearer {}", master_key))
        .map_err(|_| Status::internal("Invalid key"))?;
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
        }
        None => Err(Status::unauthenticated("Missing Master Key")),
    }
}

async fn build_grpc_channel(master_url: &str) -> Result<Channel, String> {
    let mut endpoint = Channel::from_shared(master_url.to_string())
        .map_err(|e| format!("Invalid Master URL: {}", e))?;

    if master_url.starts_with("https://") {
        let mut tls_config = ClientTlsConfig::new();
        if let Ok(ca_path) = std::env::var("APEX_TLS_CA_PATH") {
            if !ca_path.is_empty()
                && let Ok(ca_cert) = tokio::fs::read(&ca_path).await
            {
                let ca = Certificate::from_pem(ca_cert);
                tls_config = tls_config.ca_certificate(ca);
            }
        } else {
            tls_config = tls_config.with_native_roots();
        }
        endpoint = endpoint
            .tls_config(tls_config)
            .map_err(|e| format!("TLS Config Error: {}", e))?;
    }
    endpoint
        .connect()
        .await
        .map_err(|e| format!("Failed to connect: {}", e))
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
    conn.execute("INSERT OR REPLACE INTO _replicas (id, scopes, last_seen) VALUES (?1, ?2, CURRENT_TIMESTAMP)", params![id, scope_list]).map_err(|e| Status::internal(e.to_string()))?;
    Ok(())
}

pub async fn init_master_replica_tracker(tx: tokio::sync::broadcast::Sender<ChangesetEvent>) {
    let _ = MASTER_CHANGESET_TX.set(tx.clone());
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
        let mut cleanup_interval = tokio::time::interval(Duration::from_secs(10));
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
                        if info.tx.is_none() && now.duration_since(info.last_seen) > Duration::from_secs(300) {
                            tracing::warn!("Replica {} disconnected > 5m. Dropping from master.", id);
                            false
                        } else { true }
                    });
                }
            }
        }
    });
}

// --- SHARED MASTER LOGIC ---

pub async fn process_master_write(
    event_tx: &tokio::sync::broadcast::Sender<ChangesetEvent>,
    db_path: &str,
    sql: &str,
    params_bytes: &[u8],
) -> Result<(i64, String), String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| e.to_string())?; // [FIX] Removed unused mut
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .unwrap();

    let (scope, db_name) = parse_db_path(db_path);
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
        if session.changeset_strm(&mut changeset_bytes).is_ok() {
            if !changeset_bytes.is_empty() {
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

// --- WS & REST FALLBACK IMPLEMENTATIONS (SERVER) ---

pub async fn master_auth_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let expected_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
    if expected_key.is_empty() {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if auth_header != format!("Bearer {}", expected_key) {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(req).await)
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "payload")]
pub enum WsReplMsg {
    Subscribe {
        replica_id: String,
        add_scopes: Vec<String>,
    },
    Ping,
    Pong,
    DbEvent {
        scope: String,
        db_name: String,
        changeset: String,
    },
    FullSyncRequired,

    // [NEW] RPC over WebSocket for Bidirectional Database Writes
    WriteRequest {
        req_id: String,
        db_path: String,
        sql: String,
        params: Vec<u8>,
    },
    WriteResponse {
        req_id: String,
        success: bool,
        insert_id: i64,
        error: String,
    },
}

#[derive(Deserialize, Serialize)]
pub struct FallbackWriteReq {
    pub sql: String,
    pub params: Vec<u8>,
    pub db_path: String,
}
#[derive(Deserialize, Serialize)]
pub struct FallbackWriteRes {
    pub success: bool,
    pub insert_id: i64,
    pub error: String,
}

#[derive(Deserialize)]
pub struct FallbackSnapshotReq {
    pub db_path: String,
}

#[derive(Deserialize, Serialize)]
pub struct FallbackSyncFileReq {
    pub scope: String,
    pub filename: String,
    pub mime_type: String,
    pub data: String,
}
#[derive(Deserialize, Serialize)]
pub struct FallbackSyncFileRes {
    pub success: bool,
    pub error: String,
}

pub async fn fallback_write_handler(
    State(_state): State<AppState>,
    axum::extract::Json(req): axum::extract::Json<FallbackWriteReq>,
) -> Result<axum::Json<FallbackWriteRes>, axum::http::StatusCode> {
    let event_tx = MASTER_CHANGESET_TX
        .get()
        .ok_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    match process_master_write(event_tx, &req.db_path, &req.sql, &req.params).await {
        Ok((insert_id, error)) => Ok(axum::Json(FallbackWriteRes {
            success: true,
            insert_id,
            error,
        })),
        Err(e) => Ok(axum::Json(FallbackWriteRes {
            success: false,
            insert_id: 0,
            error: e,
        })),
    }
}

pub async fn fallback_snapshot_handler(
    Query(req): Query<FallbackSnapshotReq>,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    if !req.db_path.starts_with("storage/")
        || req.db_path.contains("..")
        || req.db_path.ends_with("logs.db")
    {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }
    if let Ok(conn) = rusqlite::Connection::open(&req.db_path) {
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
    }
    let file = tokio::fs::File::open(&req.db_path)
        .await
        .map_err(|_| axum::http::StatusCode::NOT_FOUND)?;
    let stream = tokio_util::io::ReaderStream::new(file);
    Ok(axum::response::Response::builder()
        .header("Content-Type", "application/octet-stream")
        .body(axum::body::Body::from_stream(stream))
        .unwrap())
}

pub async fn fallback_sync_file_handler(
    State(state): State<AppState>,
    axum::extract::Json(req): axum::extract::Json<FallbackSyncFileReq>,
) -> Result<axum::Json<FallbackSyncFileRes>, axum::http::StatusCode> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let data = STANDARD
        .decode(&req.data)
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    match process_master_sync_file(&state, &req.scope, &req.filename, &req.mime_type, &data).await {
        Ok(_) => Ok(axum::Json(FallbackSyncFileRes {
            success: true,
            error: "".into(),
        })),
        Err(e) => Ok(axum::Json(FallbackSyncFileRes {
            success: false,
            error: e,
        })),
    }
}

// Master WebSocket Handler
pub async fn ws_replication_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> axum::response::Response {
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
                                        tracing::info!("🔄 Replica reconnected (WS): {}", replica_id);
                                        false
                                    } else {
                                        map.insert(replica_id.clone(), ReplicaInfo { id: replica_id.clone(), scopes: add_scopes.into_iter().collect(), buffer: vec![], last_seen: Instant::now(), tx: Some(tx.clone()) });
                                        tracing::info!("🌟 New Replica connected (WS): {}", replica_id);
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
                            // [NEW] Master handles writes received from Replica via WS
                            WsReplMsg::WriteRequest { req_id, db_path, sql, params } => {
                                if let Some(event_tx) = MASTER_CHANGESET_TX.get() {
                                    let response_msg = match process_master_write(event_tx, &db_path, &sql, &params).await {
                                        Ok((insert_id, error)) => WsReplMsg::WriteResponse { req_id, success: true, insert_id, error },
                                        Err(e) => WsReplMsg::WriteResponse { req_id, success: false, insert_id: 0, error: e },
                                    };
                                    let _ = socket.send(AxumWsMessage::Text(serde_json::to_string(&response_msg).unwrap().into())).await;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some(Ok(evt)) = rx.recv() => {
                if evt.db_name == "logs" { continue; }
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

pub struct GrpcWriteForwarder {
    pub master_url: String,
    pub channel: Arc<RwLock<Option<Channel>>>,
}

impl GrpcWriteForwarder {
    pub fn new(master_url: String) -> Self {
        Self {
            master_url,
            channel: Arc::new(RwLock::new(None)),
        }
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

    // [NEW] WebSocket RPC execution
    async fn fallback_ws_write(
        &self,
        db_path: &str,
        sql: &str,
        json_params: &Vec<serde_json::Value>,
    ) -> Result<(i64, u64), String> {
        let req_id = uuid::Uuid::new_v4().to_string();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        get_pending_reqs()
            .write()
            .await
            .insert(req_id.clone(), reply_tx);

        let msg = WsReplMsg::WriteRequest {
            req_id: req_id.clone(),
            db_path: db_path.to_string(),
            sql: sql.to_string(),
            params: serde_json::to_vec(json_params).unwrap(),
        };

        let sent = if let Some(lock) = WS_OUTBOUND_TX.get() {
            if let Some(tx) = lock.read().await.as_ref() {
                tx.send(msg).await.is_ok()
            } else {
                false
            }
        } else {
            false
        };

        if !sent {
            get_pending_reqs().write().await.remove(&req_id);
            return Err("WS disconnected or not initialized".into());
        }

        match tokio::time::timeout(Duration::from_secs(15), reply_rx).await {
            Ok(Ok(res)) => {
                if res.success {
                    Ok((res.insert_id, 1))
                } else {
                    Err(res.error)
                }
            }
            _ => {
                get_pending_reqs().write().await.remove(&req_id);
                Err("WS write request timed out".into())
            }
        }
    }
}

#[async_trait::async_trait]
impl WriteForwarder for GrpcWriteForwarder {
    async fn forward_write(
        &self,
        db_path: String,
        sql: String,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<(i64, u64), String> {
        let mut json_params = Vec::new();
        for p in params {
            match p {
                rusqlite::types::Value::Null => json_params.push(serde_json::Value::Null),
                rusqlite::types::Value::Integer(i) => json_params.push(serde_json::json!(i)),
                rusqlite::types::Value::Real(f) => json_params.push(serde_json::json!(f)),
                rusqlite::types::Value::Text(s) => json_params.push(serde_json::json!(s)),
                rusqlite::types::Value::Blob(b) => {
                    use base64::{Engine as _, engine::general_purpose::STANDARD};
                    json_params
                        .push(serde_json::json!({ "__type": "blob", "data": STANDARD.encode(&b) }));
                }
            }
        }

        if is_http_fallback() {
            return self.fallback_ws_write(&db_path, &sql, &json_params).await;
        }

        let channel = match self.get_channel().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("gRPC channel error: {}. Switching to WS.", e);
                USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
                return self.fallback_ws_write(&db_path, &sql, &json_params).await;
            }
        };

        let mut client = pb::replication_client::ReplicationClient::with_interceptor(
            channel,
            client_auth_interceptor,
        )
        .max_decoding_message_size(100 * 1024 * 1024)
        .max_encoding_message_size(100 * 1024 * 1024);

        let req = tonic::Request::new(pb::WriteRequest {
            sql: sql.clone(),
            params: serde_json::to_vec(&json_params).unwrap(),
            db_path: db_path.clone(),
        });

        match client.execute_write(req).await {
            Ok(res) => {
                let inner = res.into_inner();
                if inner.success {
                    Ok((inner.insert_id, 1))
                } else {
                    Err(inner.error)
                }
            }
            Err(e) => {
                tracing::warn!("gRPC Write failed ({}). Switching to WS fallback.", e);
                USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
                self.fallback_ws_write(&db_path, &sql, &json_params).await
            }
        }
    }
}

pub async fn forward_file_to_master(
    master_url: &str,
    scope: &str,
    filename: &str,
    mime: &str,
    data: &[u8],
) -> Result<(), String> {
    let do_http = || async {
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
        let res: FallbackSyncFileRes = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", master_key))
            .json(&req)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        if res.success { Ok(()) } else { Err(res.error) }
    };

    if is_http_fallback() {
        return do_http().await;
    }

    let channel = match build_grpc_channel(master_url).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("gRPC channel error: {}. Switching to HTTP.", e);
            USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
            return do_http().await;
        }
    };

    let mut client = pb::replication_client::ReplicationClient::with_interceptor(
        channel,
        client_auth_interceptor,
    )
    .max_decoding_message_size(100 * 1024 * 1024)
    .max_encoding_message_size(100 * 1024 * 1024);

    let req = tonic::Request::new(pb::SyncFileRequest {
        scope: scope.to_string(),
        filename: filename.to_string(),
        mime_type: mime.to_string(),
        data: data.to_vec(),
    });

    match client.sync_file(req).await {
        Ok(res) => {
            if res.into_inner().success {
                Ok(())
            } else {
                Err("File Sync Failed".into())
            }
        }
        Err(e) => {
            tracing::warn!("gRPC sync_file failed ({}). Switching to HTTP fallback.", e);
            USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
            do_http().await
        }
    }
}

pub async fn fetch_snapshot_from_master(master_url: &str, db_path: &str) -> Result<(), String> {
    let do_http = || async {
        let url = format!(
            "{}/replication/snapshot?db_path={}",
            master_url.trim_end_matches('/'),
            db_path
        );
        let master_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
        let client = reqwest::Client::new();

        let mut response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", master_key))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "HTTP Snapshot download failed: {}",
                response.status()
            ));
        }
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }

        let tmp_path = format!("{}.tmp", db_path);
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| e.to_string())?;
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        }
        file.sync_all().await.map_err(|e| e.to_string())?;

        tokio::fs::rename(&tmp_path, db_path)
            .await
            .map_err(|e| e.to_string())?;
        let _ = tokio::fs::remove_file(format!("{}-wal", db_path)).await;
        let _ = tokio::fs::remove_file(format!("{}-shm", db_path)).await;
        Ok(())
    };

    if is_http_fallback() {
        return do_http().await;
    }

    let channel = match build_grpc_channel(master_url).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("gRPC channel error: {}. Switching to HTTP.", e);
            USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
            return do_http().await;
        }
    };

    let mut client = pb::replication_client::ReplicationClient::with_interceptor(
        channel,
        client_auth_interceptor,
    )
    .max_decoding_message_size(100 * 1024 * 1024)
    .max_encoding_message_size(100 * 1024 * 1024);

    let req = tonic::Request::new(pb::SnapshotRequest {
        db_path: db_path.to_string(),
    });

    match client.fetch_db_snapshot(req).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            let tmp_path = format!("{}.tmp", db_path);
            let mut file = tokio::fs::File::create(&tmp_path)
                .await
                .map_err(|e| e.to_string())?;
            let mut success = true;
            let mut total_bytes = 0;
            loop {
                match stream.message().await {
                    Ok(Some(chunk)) => {
                        file.write_all(&chunk.data)
                            .await
                            .map_err(|e| e.to_string())?;
                        total_bytes += chunk.data.len();
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!(
                            "gRPC stream failed mid-transfer: {}. Switching to HTTP.",
                            e
                        );
                        success = false;
                        break;
                    }
                }
            }
            if success && total_bytes > 0 {
                file.sync_all().await.map_err(|e| e.to_string())?;
                tokio::fs::rename(&tmp_path, db_path)
                    .await
                    .map_err(|e| e.to_string())?;
                let _ = tokio::fs::remove_file(format!("{}-wal", db_path)).await;
                let _ = tokio::fs::remove_file(format!("{}-shm", db_path)).await;
                return Ok(());
            } else {
                tracing::warn!(
                    "gRPC snapshot transfer failed or returned 0 bytes. Falling back to HTTP."
                );
                USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
                return do_http().await;
            }
        }
        Err(e) => {
            tracing::warn!(
                "❌ Failed to fetch snapshot via gRPC for {}: {}. Falling back to HTTP.",
                db_path,
                e
            );
            USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
            return do_http().await;
        }
    }
}

pub async fn ensure_replica_env(base_path: &str) {
    do_sync_env(base_path, false).await;
}
pub async fn force_replica_sync(base_path: &str) {
    do_sync_env(base_path, true).await;
}

async fn do_sync_env(base_path: &str, force: bool) {
    if let Ok(master_url) = std::env::var("APEX_MASTER_URL")
        && !master_url.is_empty()
    {
        if force {
            tracing::warn!(
                "🔄 [ReplicaEnv] FORCING DB snapshot sync for path: {}",
                base_path
            );
        } else {
            tracing::info!(
                "🔄 [ReplicaEnv] Ensuring DB snapshot existence for path: {}",
                base_path
            );
        }

        let dbs = ["core.db", "data.db", "system.db", "vectors.db"];
        for db in dbs {
            let db_path = format!("{}/{}", base_path, db);
            if force || !std::path::Path::new(&db_path).exists() {
                tracing::info!(
                    "📥 [ReplicaEnv] Fetching snapshot for {} from Master...",
                    db_path
                );
                let res = fetch_snapshot_from_master(&master_url, &db_path).await;
                if let Err(e) = res {
                    tracing::error!(
                        "❌ [ReplicaEnv] Failed to fetch snapshot for {}: {}",
                        db_path,
                        e
                    );
                } else {
                    tracing::info!(
                        "✅ [ReplicaEnv] Successfully fetched snapshot for {}",
                        db_path
                    );
                }
            }
        }
    }
}

pub fn add_replica_subscription(scope: &str) {
    let s = scope.to_string();
    if let Some(replica_id) = REPLICA_ID.get() {
        let _ = get_event_sub_tx().send(pb::EventSubscription {
            replica_id: replica_id.clone(),
            add_scopes: vec![s],
        });
    }
}

fn get_local_scopes() -> Vec<String> {
    let mut scopes = vec!["root".to_string()];
    if let Ok(entries) = std::fs::read_dir("storage/tenants") {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type()
                && file_type.is_dir()
                && let Ok(name) = entry.file_name().into_string()
            {
                scopes.push(format!("tenant:{}", name));
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir("storage/sandboxes") {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type()
                && file_type.is_dir()
                && let Ok(name) = entry.file_name().into_string()
                && name.starts_with("session_")
            {
                let sid = name.strip_prefix("session_").unwrap();
                scopes.push(format!("sandbox:{}", sid));
            }
        }
    }
    scopes
}

pub async fn start_event_streamer(master_url: String, state: Option<crate::AppState>) {
    let replica_id = init_replica_id().await;
    tracing::info!("📡 [EventStreamer] Connected as Replica ID: {}", replica_id);

    if is_http_fallback() {
        start_ws_event_streamer(master_url, state, replica_id).await;
        return;
    }

    let channel = match build_grpc_channel(&master_url).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                "gRPC connection failed ({}). Falling back to HTTP/WS Replication.",
                e
            );
            USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
            start_ws_event_streamer(master_url, state, replica_id).await;
            return;
        }
    };

    let mut client = pb::replication_client::ReplicationClient::with_interceptor(
        channel,
        client_auth_interceptor,
    )
    .max_decoding_message_size(100 * 1024 * 1024)
    .max_encoding_message_size(100 * 1024 * 1024);

    // Bridge the global broadcast channel to the MPSC stream required by Tonic
    let mut global_sub_rx = get_event_sub_tx().subscribe();
    let (mpsc_tx, mpsc_rx) = tokio::sync::mpsc::channel(32);

    // Send initial scopes immediately
    let initial_scopes = get_local_scopes();
    let _ = mpsc_tx
        .send(pb::EventSubscription {
            replica_id: replica_id.clone(),
            add_scopes: initial_scopes,
        })
        .await;

    tokio::spawn(async move {
        while let Ok(msg) = global_sub_rx.recv().await {
            if mpsc_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let request_stream = tokio_stream::wrappers::ReceiverStream::new(mpsc_rx);

    match client
        .stream_events(tonic::Request::new(request_stream))
        .await
    {
        Ok(response) => {
            let mut stream = response.into_inner();
            loop {
                tokio::select! {
                    msg = stream.message() => {
                        match msg {
                            Ok(Some(event)) => {
                                if event.db_name == "FULL_SYNC_REQUIRED" {
                                    tracing::warn!("Master requested FULL_SYNC due to prolonged disconnection (> 5m).");
                                    force_replica_sync("storage/system").await;
                                    if let Some(s) = &state { let _ = s.db.reload_connections().await; }
                                    continue;
                                }
                                if event.db_name == "logs" { continue; }
                                tracing::debug!("📥 [Replica] Received DB changes for {}/{}", event.scope, event.db_name);
                                let _ = get_db_sync_tx().send(event);
                            }
                            Ok(None) => break,
                            Err(e) => { tracing::warn!("gRPC stream error: {}", e); break; }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {
                        if is_http_fallback() {
                            tracing::warn!("HTTP Fallback triggered by another component. Abandoning gRPC event stream.");
                            break;
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!(
                "gRPC Stream encountered errors ({}). Switching to WS Replication Fallback.",
                e
            );
        }
    }

    USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
    start_ws_event_streamer(master_url, state, replica_id).await;
}

pub async fn start_ws_event_streamer(
    master_url: String,
    state: Option<crate::AppState>,
    replica_id: String,
) {
    let ws_url = format!(
        "{}/replication/ws",
        master_url
            .replace("http://", "ws://")
            .replace("https://", "wss://")
            .trim_end_matches('/')
    );
    let master_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();

    loop {
        tracing::info!("📡 [WS Streamer] Connecting to {}", ws_url);
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let request_result = ws_url.clone().into_client_request();

        if let Ok(mut request) = request_result {
            if let Ok(header_value) = format!("Bearer {}", master_key).parse() {
                request.headers_mut().insert("Authorization", header_value);
            }

            match tokio_tungstenite::connect_async(request).await {
                Ok((mut ws_stream, _)) => {
                    tracing::info!("✅ [WS Streamer] Connected successfully");

                    // 1. Send Initial Subscriptions
                    let sub_msg = WsReplMsg::Subscribe {
                        replica_id: replica_id.clone(),
                        add_scopes: get_local_scopes(),
                    };
                    let _ = ws_stream
                        .send(tokio_tungstenite::tungstenite::protocol::Message::Text(
                            serde_json::to_string(&sub_msg).unwrap().into(),
                        ))
                        .await;

                    // 2. Setup Multiplexing Channels
                    let mut ping_interval = tokio::time::interval(Duration::from_secs(25));
                    let mut global_sub_rx = get_event_sub_tx().subscribe();

                    let (ws_out_tx, mut ws_out_rx) = mpsc::channel(1000);
                    let lock = WS_OUTBOUND_TX.get_or_init(|| RwLock::new(None));
                    *lock.write().await = Some(ws_out_tx);

                    // 3. Multiplexing Loop
                    loop {
                        tokio::select! {
                            // A. Ping
                            _ = ping_interval.tick() => {
                                if ws_stream.send(tokio_tungstenite::tungstenite::protocol::Message::Text(serde_json::to_string(&WsReplMsg::Ping).unwrap().into())).await.is_err() { break; }
                            }

                            // B. Dynamic Subscriptions (New Tenants)
                            Ok(new_sub) = global_sub_rx.recv() => {
                                let dyn_msg = WsReplMsg::Subscribe { replica_id: new_sub.replica_id, add_scopes: new_sub.add_scopes };
                                let _ = ws_stream.send(tokio_tungstenite::tungstenite::protocol::Message::Text(serde_json::to_string(&dyn_msg).unwrap().into())).await;
                            }

                            // C. Outbound WS Requests (Database Writes from Replica -> Master)
                            Some(out_msg) = ws_out_rx.recv() => {
                                let _ = ws_stream.send(tokio_tungstenite::tungstenite::protocol::Message::Text(serde_json::to_string(&out_msg).unwrap().into())).await;
                            }

                            // D. Inbound WS Messages (Master -> Replica)
                            msg = ws_stream.next() => {
                                match msg {
                                    Some(Ok(tokio_tungstenite::tungstenite::protocol::Message::Text(t))) => {
                                        if let Ok(ws_msg) = serde_json::from_str::<WsReplMsg>(&t) {
                                            match ws_msg {
                                                // Handle DB Sync Events
                                                WsReplMsg::DbEvent { scope, db_name, changeset } => {
                                                    if db_name == "logs" { continue; }
                                                    use base64::{Engine as _, engine::general_purpose::STANDARD};
                                                    if let Ok(bytes) = STANDARD.decode(&changeset) {
                                                        tracing::debug!("📥 [Replica WS] Received DB changes for {}/{}", scope, db_name);
                                                        let _ = get_db_sync_tx().send(pb::DbChangeEvent { scope, db_name, changeset: bytes });
                                                    }
                                                }
                                                // Handle Full Sync Requests
                                                WsReplMsg::FullSyncRequired => {
                                                    force_replica_sync("storage/system").await;
                                                    if let Some(s) = &state { let _ = s.db.reload_connections().await; }
                                                }
                                                // [NEW] Handle Write Responses mapping back to the waiting HTTP thread
                                                WsReplMsg::WriteResponse { req_id, success, insert_id, error } => {
                                                    let pending_reqs = get_pending_reqs(); // 💡 Bind Arc first to keep it alive
                                                    let mut pending = pending_reqs.write().await;
                                                    if let Some(reply_tx) = pending.remove(&req_id) {
                                                        let _ = reply_tx.send(FallbackWriteRes { success, insert_id, error });
                                                    }
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
        }

        tracing::warn!("🔄 [WS Streamer] Reconnecting in 5 seconds...");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
