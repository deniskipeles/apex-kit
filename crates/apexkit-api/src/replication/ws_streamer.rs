use super::{
    FallbackSnapshotReq, FallbackSyncFileReq, FallbackSyncFileRes, FallbackWriteReq,
    FallbackWriteRes, MASTER_CHANGESET_TX, REPLICA_ID, ReplicaInfo, USE_HTTP_FALLBACK,
    WS_OUTBOUND_TX, WsReplMsg, build_grpc_channel, client_auth_interceptor, get_db_sync_tx,
    get_event_sub_tx, get_pending_reqs, get_replica_tracker, is_http_fallback, pb,
    process_master_write, register_replica_on_master,
};
use crate::AppState;
use axum::extract::{
    Query, State,
    ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade},
};
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc};

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
    match super::grpc_server::process_master_sync_file(
        &state,
        &req.scope,
        &req.filename,
        &req.mime_type,
        &data,
    )
    .await
    {
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
                if let AxumWsMessage::Text(t) = msg
                    && let Ok(ws_msg) = serde_json::from_str::<WsReplMsg>(&t) {
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
                                if !current_replica_id.is_empty()
                                    && let Some(info) = tracker.write().await.get_mut(&current_replica_id) { info.last_seen = Instant::now(); }
                                let _ = socket.send(AxumWsMessage::Text(serde_json::to_string(&WsReplMsg::Pong).unwrap().into())).await;
                            }
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
            Some(Ok(evt)) = rx.recv() => {
                if evt.db_name == "logs" { continue; }
                use base64::{Engine as _, engine::general_purpose::STANDARD};
                let out = WsReplMsg::DbEvent { scope: evt.scope, db_name: evt.db_name, changeset: STANDARD.encode(&evt.changeset) };
                if socket.send(AxumWsMessage::Text(serde_json::to_string(&out).unwrap().into())).await.is_err() { break; }
            }
            else => break,
        }
    }

    if !current_replica_id.is_empty()
        && let Some(info) = tracker.write().await.get_mut(&current_replica_id)
    {
        info.tx = None;
        info.last_seen = Instant::now();
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

pub fn get_local_scopes() -> Vec<String> {
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
    let replica_id = super::tracker::init_replica_id().await;
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

    let mut global_sub_rx = get_event_sub_tx().subscribe();
    let (mpsc_tx, mpsc_rx) = tokio::sync::mpsc::channel(32);

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
                                    super::snapshot::force_replica_sync("storage/system").await;
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

                    let sub_msg = WsReplMsg::Subscribe {
                        replica_id: replica_id.clone(),
                        add_scopes: get_local_scopes(),
                    };
                    let _ = ws_stream
                        .send(tokio_tungstenite::tungstenite::protocol::Message::Text(
                            serde_json::to_string(&sub_msg).unwrap(),
                        ))
                        .await;

                    let mut ping_interval = tokio::time::interval(Duration::from_secs(25));
                    let mut global_sub_rx = get_event_sub_tx().subscribe();

                    let (ws_out_tx, mut ws_out_rx) = mpsc::channel(1000);
                    let lock = WS_OUTBOUND_TX.get_or_init(|| RwLock::new(None));
                    *lock.write().await = Some(ws_out_tx);

                    loop {
                        tokio::select! {
                            _ = ping_interval.tick() => {
                                if ws_stream.send(tokio_tungstenite::tungstenite::protocol::Message::Text(serde_json::to_string(&WsReplMsg::Ping).unwrap())).await.is_err() { break; }
                            }
                            Ok(new_sub) = global_sub_rx.recv() => {
                                let dyn_msg = WsReplMsg::Subscribe { replica_id: new_sub.replica_id, add_scopes: new_sub.add_scopes };
                                let _ = ws_stream.send(tokio_tungstenite::tungstenite::protocol::Message::Text(serde_json::to_string(&dyn_msg).unwrap())).await;
                            }
                            Some(out_msg) = ws_out_rx.recv() => {
                                let _ = ws_stream.send(tokio_tungstenite::tungstenite::protocol::Message::Text(serde_json::to_string(&out_msg).unwrap())).await;
                            }
                            msg = ws_stream.next() => {
                                match msg {
                                    Some(Ok(tokio_tungstenite::tungstenite::protocol::Message::Text(t))) => {
                                        if let Ok(ws_msg) = serde_json::from_str::<WsReplMsg>(&t) {
                                            match ws_msg {
                                                WsReplMsg::DbEvent { scope, db_name, changeset } => {
                                                    if db_name == "logs" { continue; }
                                                    use base64::{Engine as _, engine::general_purpose::STANDARD};
                                                    if let Ok(bytes) = STANDARD.decode(&changeset) {
                                                        tracing::debug!("📥 [Replica WS] Received DB changes for {}/{}", scope, db_name);
                                                        let _ = get_db_sync_tx().send(pb::DbChangeEvent { scope, db_name, changeset: bytes });
                                                    }
                                                }
                                                WsReplMsg::FullSyncRequired => {
                                                    super::snapshot::force_replica_sync("storage/system").await;
                                                    if let Some(s) = &state { let _ = s.db.reload_connections().await; }
                                                }
                                                WsReplMsg::WriteResponse { req_id, success, insert_id, error } => {
                                                    let pending_reqs = get_pending_reqs();
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
