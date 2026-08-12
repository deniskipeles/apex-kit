use crate::{AppError, AppState, DatabaseConnection};
use apexkit_core::scripting::module_loader::WorkspaceManager;
use axum::{
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct SyncQuery {
    pub api_key: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type", content = "payload")]
pub enum SyncMessage {
    Auth {
        token: String,
    },
    PullWorkspace,
    PushFile {
        path: String,
        content: String,
        commit_to_db: bool,
    },
    CommitWorkspace,
    Ping,
    Pong,
    WorkspaceData {
        zip_b64: String,
    },
    SyncAck {
        message: String,
    },
    Error {
        message: String,
    },
}

pub async fn vscode_sync_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    DatabaseConnection(db): DatabaseConnection,
    Query(query): Query<SyncQuery>,
) -> Result<impl IntoResponse, AppError> {
    let key = query
        .api_key
        .ok_or_else(|| AppError::Unauthorized("Missing API Key".into()))?;

    let mut authenticated = false;
    let mut resolved_scope = "root".to_string();

    // 1. Check against local .env APEXKIT_API_KEY (Root Admin)
    let local_env_key = std::env::var("APEXKIT_API_KEY").unwrap_or_default();
    if !local_env_key.is_empty() && key == local_env_key {
        authenticated = true;
        resolved_scope = "root".to_string();
    }

    // 2. Verify Scoped API Key against Database (Supports tenant:app-1)
    if !authenticated {
        if let Some(parsed) = apexkit_core::security::api_keys::parse_and_validate_key(&key) {
            if let Ok(Some(api_key)) = db
                .verify_api_key(&parsed.tenant_id, &parsed.key_id, &parsed.secret)
                .await
            {
                if api_key.roles.contains(&"admin".to_string()) {
                    authenticated = true;
                    resolved_scope = if api_key.tenant_id == "root" {
                        "root".to_string()
                    } else {
                        format!("tenant:{}", api_key.tenant_id)
                    };
                }
            }
        }
    }

    if !authenticated {
        return Err(AppError::Unauthorized("Invalid API Key".into()));
    }

    Ok(ws.on_upgrade(move |socket| handle_vscode_socket(socket, state, db, resolved_scope)))
}

async fn handle_vscode_socket(
    socket: WebSocket,
    state: AppState,
    db: std::sync::Arc<dyn apexkit_core::Db>,
    scope_id: String,
) {
    let (mut sender, mut receiver) = socket.split();

    // Keep-alive ping interval (15 seconds)
    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                let ping_msg = serde_json::to_string(&SyncMessage::Ping).unwrap();
                if sender.send(Message::Text(ping_msg.into())).await.is_err() {
                    break;
                }
            }
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(sync_msg) = serde_json::from_str::<SyncMessage>(&text) {
                            match sync_msg {
                                SyncMessage::PullWorkspace => {
                                    use base64::{Engine as _, engine::general_purpose::STANDARD};
                                    match WorkspaceManager::export_workspace_zip(&db, &scope_id).await {
                                        Ok(bytes) => {
                                            let resp = SyncMessage::WorkspaceData { zip_b64: STANDARD.encode(bytes) };
                                            let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap().into())).await;
                                        }
                                        Err(e) => {
                                            let err = SyncMessage::Error { message: e };
                                            let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                                        }
                                    }
                                }
                                SyncMessage::PushFile { path, content, commit_to_db } => {
                                    // 1. Update Live Engine VFS under the isolated SCOPE ID
                                    state.vfs.set_file(&scope_id, &path, &content);

                                    // 2. Commit to the specific Tenant/Root SQLite DB
                                    if commit_to_db {
                                        match WorkspaceManager::commit_file_to_db(&db, &path, &content).await {
                                            Ok(msg) => {
                                                let ack = SyncMessage::SyncAck { message: msg };
                                                let _ = sender.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await;
                                            }
                                            Err(e) => {
                                                let err = SyncMessage::Error { message: e };
                                                let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                                            }
                                        }
                                    }
                                }
                                SyncMessage::Ping => {
                                    let pong = serde_json::to_string(&SyncMessage::Pong).unwrap();
                                    let _ = sender.send(Message::Text(pong.into())).await;
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
