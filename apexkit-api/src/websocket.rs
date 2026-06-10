use crate::AppState;
use apexkit_core::{
    filter::FilterNode,
    realtime::{DbEvent, EventScope},
};
use axum::{
    Extension,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use tracing::warn;

#[derive(Deserialize, Debug, Clone, Default)]
pub struct SubscriptionFilter {
    pub collection_id: Option<i64>,
    pub record_id: Option<i64>,
    pub event_type: Option<String>,
    pub filter: Option<serde_json::Value>,
    pub custom_event: Option<String>,
    pub channel: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct SearchRequest {
    pub collection_id: i64,
    pub query: String,
    pub limit: Option<usize>,
    pub request_id: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct SignalRequest {
    pub channel: String,
    pub event: String,
    pub data: serde_json::Value,
}

#[derive(Deserialize, Debug)]
pub struct AuthRequest {
    pub token: String,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "payload")]
pub enum ClientMessage {
    Subscribe(SubscriptionFilter),
    Unsubscribe,
    Ping,
    Search(SearchRequest),
    Signal(SignalRequest),
    Auth(AuthRequest),
}

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
) -> impl IntoResponse {
    let client_scope = scope.map(|e| e.0).unwrap_or_default();
    ws.on_upgrade(move |socket| handle_socket(socket, state, client_scope))
}

fn namespaced_channel(scope: &EventScope, channel: &str) -> String {
    match scope {
        EventScope::Root => format!("root::{}", channel),
        EventScope::Tenant(id) => format!("tenant_{}::{}", id, channel),
        EventScope::Sandbox(id) => format!("sandbox_{}::{}", id, channel),
        _ => channel.to_string(),
    }
}

async fn handle_socket(socket: WebSocket, state: AppState, client_scope: EventScope) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let mut current_filter = SubscriptionFilter::default();
    let mut current_filter_node = FilterNode::Empty;
    let mut active_namespaced_channel: Option<String> = None;

    // [ADDED] Hold authenticated claims in memory for this connection
    let mut current_claims: Option<apexkit_core::auth::Claims> = None;

    loop {
        tokio::select! {
            client_msg = receiver.next() => {
                match client_msg {
                    Some(Ok(msg)) => {
                        match msg {
                            Message::Text(text) => {
                                match serde_json::from_str::<ClientMessage>(&text) {
                                    Ok(ClientMessage::Auth(req)) => {
                                        // [ADDED] Decode and save the token
                                        if let Ok(claims) = apexkit_core::auth::decode_jwt(&req.token) {
                                            current_claims = Some(claims);
                                            let _ = sender.send(Message::Text(serde_json::json!({ "type": "AuthSuccess" }).to_string().into())).await;
                                        } else {
                                            let _ = sender.send(Message::Text(serde_json::json!({ "type": "Error", "message": "Invalid token" }).to_string().into())).await;
                                        }
                                    },
                                    Ok(ClientMessage::Subscribe(filter)) => {
                                        if let Some(json) = &filter.filter {
                                            current_filter_node = FilterNode::parse(json);
                                        } else {
                                            current_filter_node = FilterNode::Empty;
                                        }

                                        if let Some(chan) = &filter.channel {
                                            active_namespaced_channel = Some(namespaced_channel(&client_scope, chan));
                                        } else {
                                            active_namespaced_channel = None;
                                        }

                                        current_filter = filter;
                                    },
                                    Ok(ClientMessage::Signal(req)) => {
                                        let scoped_channel = namespaced_channel(&client_scope, &req.channel);
                                        let event = DbEvent::Custom {
                                            event: req.event,
                                            data: req.data,
                                            scope: EventScope::Channel(scoped_channel)
                                        };
                                        let _ = state.tx.send(event);
                                    },
                                    Ok(ClientMessage::Unsubscribe) => {
                                        current_filter = SubscriptionFilter::default();
                                        current_filter_node = FilterNode::Empty;
                                        active_namespaced_channel = None;
                                    },
                                    Ok(ClientMessage::Ping) => {
                                        let _ = sender.send(Message::Text("Pong".into())).await;
                                    },
                                    Ok(ClientMessage::Search(req)) => {
                                        let limit = req.limit.unwrap_or(10).min(50);

                                        // [ADDED] Check Access Control before executing search over WS
                                        let mut allowed = false;
                                        if let Ok(Some(col)) = state.db.get_collection(req.collection_id).await {
                                            let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
                                            allowed = apexkit_core::policies::check_access(policy, current_claims.as_ref(), None);
                                        }

                                        if !allowed {
                                            let err_resp = serde_json::json!({ "type": "Error", "request_id": req.request_id, "message": "Access denied" });
                                            let _ = sender.send(Message::Text(err_resp.to_string().into())).await;
                                            continue;
                                        }

                                        match state.db.instant_search(req.collection_id, &req.query, limit).await {
                                            Ok(results) => {
                                                let response = serde_json::json!({
                                                    "type": "SearchResult",
                                                    "request_id": req.request_id,
                                                    "results": results
                                                });
                                                let _ = sender.send(Message::Text(response.to_string().into())).await;
                                            },
                                            Err(e) => {
                                                let err_resp = serde_json::json!({ "type": "Error", "request_id": req.request_id, "message": e.to_string() });
                                                let _ = sender.send(Message::Text(err_resp.to_string().into())).await;
                                            }
                                        }
                                    }
                                    Err(_) => {}
                                }
                            }
                            Message::Close(_) => break,
                            _ => {}
                        }
                    },
                    Some(Err(e)) => { warn!("WS Error: {}", e); break; },
                    None => break,
                }
            }
            Ok(event) = rx.recv() => {
                if matches_scope(&event, &client_scope, &active_namespaced_channel) && matches_filter(&event, &current_filter, &current_filter_node) {

                    // [ADDED] Row-Level Security checks for Streaming Realtime Broadcasts
                    let mut allowed = true;
                    match &event {
                        DbEvent::Insert { collection_id, data, .. } | DbEvent::Update { collection_id, data, .. } => {
                            if let Ok(Some(col)) = state.db.get_collection(*collection_id).await {
                                let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
                                allowed = apexkit_core::policies::check_access(policy, current_claims.as_ref(), Some(data));
                            }
                        }
                        DbEvent::Delete { collection_id, .. } => {
                            // On delete, we no longer have row data, check table-level base access
                            if let Ok(Some(col)) = state.db.get_collection(*collection_id).await {
                                let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
                                allowed = apexkit_core::policies::check_access(policy, current_claims.as_ref(), None);
                            }
                        }
                        _ => {}
                    }

                    if allowed {
                        if let Ok(json_msg) = serde_json::to_string(&event) {
                            if sender.send(Message::Text(json_msg.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn matches_scope(
    event: &DbEvent,
    client_scope: &EventScope,
    active_channel: &Option<String>,
) -> bool {
    match event {
        DbEvent::Insert { scope, .. }
        | DbEvent::Update { scope, .. }
        | DbEvent::Delete { scope, .. } => scope == client_scope,
        DbEvent::Custom { scope, .. } => {
            if let EventScope::Channel(evt_channel) = scope {
                if let Some(sub_channel) = active_channel {
                    return evt_channel == sub_channel;
                }
                return false;
            }
            scope == client_scope
        }
    }
}

fn matches_filter(event: &DbEvent, filter: &SubscriptionFilter, filter_node: &FilterNode) -> bool {
    match event {
        DbEvent::Custom {
            event: evt_name,
            data,
            ..
        } => {
            if let Some(req_evt) = &filter.custom_event {
                if req_evt != evt_name {
                    return false;
                }
            }
            if !matches!(filter_node, FilterNode::Empty) {
                return filter_node.matches(data);
            }
            true
        }
        DbEvent::Insert {
            collection_id,
            record_id,
            data,
            ..
        } => check_db_event(
            *collection_id,
            *record_id,
            "Insert",
            Some(data),
            filter,
            filter_node,
        ),
        DbEvent::Update {
            collection_id,
            record_id,
            data,
            ..
        } => check_db_event(
            *collection_id,
            *record_id,
            "Update",
            Some(data),
            filter,
            filter_node,
        ),
        DbEvent::Delete {
            collection_id,
            record_id,
            ..
        } => check_db_event(
            *collection_id,
            *record_id,
            "Delete",
            None,
            filter,
            filter_node,
        ),
    }
}

fn check_db_event(
    col_id: i64,
    rec_id: i64,
    type_str: &str,
    data: Option<&serde_json::Value>,
    filter: &SubscriptionFilter,
    node: &FilterNode,
) -> bool {
    if let Some(req_col) = filter.collection_id {
        if req_col != col_id {
            return false;
        }
    }
    if let Some(req_rec) = filter.record_id {
        if req_rec != rec_id {
            return false;
        }
    }
    if let Some(req_type) = &filter.event_type {
        if !req_type.eq_ignore_ascii_case(type_str) {
            return false;
        }
    }
    if !matches!(node, FilterNode::Empty) {
        if let Some(d) = data {
            if !node.matches(d) {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}
