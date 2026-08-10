pub mod grpc_client;
pub mod grpc_server;
pub mod snapshot;
pub mod tracker;
pub mod ws_streamer;

use apexkit_core::models::ChangesetEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::{RwLock, broadcast, mpsc};
use tonic::Status;
use tonic::metadata::MetadataValue;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};

pub use tracker::REPLICA_ID;
pub static USE_HTTP_FALLBACK: AtomicBool = AtomicBool::new(false);

pub fn is_http_fallback() -> bool {
    if std::env::var("APEXKIT_FORCE_HTTP_REPLICATION").unwrap_or_default() == "true" {
        return true;
    }
    USE_HTTP_FALLBACK.load(Ordering::SeqCst)
}

pub mod pb {
    tonic::include_proto!("replication");
}

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

pub async fn build_grpc_channel(master_url: &str) -> Result<Channel, String> {
    let mut endpoint = Channel::from_shared(master_url.to_string())
        .map_err(|e| format!("Invalid Master URL: {}", e))?;

    if master_url.starts_with("https://") {
        let mut tls_config = ClientTlsConfig::new();
        if let Ok(ca_path) = std::env::var("APEXKIT_TLS_CA_PATH") {
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

// --- DTOs ---

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

// --- RE-EXPORTS for main.rs & routing ---
pub use grpc_client::*;
pub use grpc_server::*;
pub use snapshot::*;
pub use tracker::*;
pub use ws_streamer::*;
