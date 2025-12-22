use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    Extension,
};
use serde::Deserialize;
use apexkit_core::{realtime::{DbEvent, EventScope}, filter::FilterNode};
use crate::AppState;
use futures::{sink::SinkExt, stream::StreamExt}; 
use tracing::{warn};

#[derive(Deserialize, Debug, Clone, Default)]
pub struct SubscriptionFilter {
    pub collection_id: Option<i64>,
    pub record_id: Option<i64>,
    pub event_type: Option<String>, 
    pub filter: Option<serde_json::Value>, 
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "payload")]
pub enum ClientMessage {
    Subscribe(SubscriptionFilter),
    Unsubscribe,
    Ping,
}

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    // Extract scope injected by middleware (Tenant/Sandbox/Root)
    scope: Option<Extension<EventScope>>, 
) -> impl IntoResponse {
    let client_scope = scope.map(|e| e.0).unwrap_or_default();
    ws.on_upgrade(move |socket| handle_socket(socket, state, client_scope))
}

async fn handle_socket(socket: WebSocket, state: AppState, client_scope: EventScope) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let mut current_filter = SubscriptionFilter::default();
    let mut current_filter_node = FilterNode::Empty;
    
    // info!("WS Connected. Scope: {:?}", client_scope);

    loop {
        tokio::select! {
            // 1. Client Messages
            client_msg = receiver.next() => {
                match client_msg {
                    Some(Ok(msg)) => {
                        match msg {
                            Message::Text(text) => {
                                match serde_json::from_str::<ClientMessage>(&text) {
                                    Ok(ClientMessage::Subscribe(filter)) => {
                                        // info!("WS Subscribed: {:?}", filter);
                                        if let Some(json) = &filter.filter {
                                            current_filter_node = FilterNode::parse(json);
                                        } else {
                                            current_filter_node = FilterNode::Empty;
                                        }
                                        current_filter = filter;
                                    },
                                    Ok(ClientMessage::Unsubscribe) => {
                                        current_filter = SubscriptionFilter::default();
                                        current_filter_node = FilterNode::Empty;
                                    },
                                    Ok(ClientMessage::Ping) => {
                                        let _ = sender.send(Message::Text("Pong".into())).await;
                                    }
                                    Err(_) => {}
                                }
                            }
                            Message::Close(_) => break,
                            _ => {}
                        }
                    },
                    Some(Err(e)) => {
                        warn!("WS Error: {}", e);
                        break;
                    },
                    None => break, 
                }
            }

            // 2. Broadcast Events
            Ok(event) = rx.recv() => {
                // Critical: Check Isolation Scope FIRST
                if matches_scope(&event, &client_scope) && matches_filter(&event, &current_filter, &current_filter_node) {
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

fn matches_scope(event: &DbEvent, client_scope: &EventScope) -> bool {
    let event_scope = match event {
        DbEvent::Insert { scope, .. } => scope,
        DbEvent::Update { scope, .. } => scope,
        DbEvent::Delete { scope, .. } => scope,
    };
    event_scope == client_scope
}

fn matches_filter(
    event: &DbEvent, 
    filter: &SubscriptionFilter, 
    filter_node: &FilterNode
) -> bool {
    let (col_id, rec_id, type_str, event_data) = match event {
        DbEvent::Insert { collection_id, record_id, data, .. } => (*collection_id, *record_id, "Insert", Some(data)),
        DbEvent::Update { collection_id, record_id, data, .. } => (*collection_id, *record_id, "Update", Some(data)),
        DbEvent::Delete { collection_id, record_id, .. } => (*collection_id, *record_id, "Delete", None),
    };

    if let Some(req_col) = filter.collection_id {
        if req_col != col_id { return false; }
    }
    if let Some(req_rec) = filter.record_id {
        if req_rec != rec_id { return false; }
    }
    if let Some(req_type) = &filter.event_type {
        if !req_type.eq_ignore_ascii_case(type_str) { return false; }
    }

    if !matches!(filter_node, FilterNode::Empty) {
        if let Some(data) = event_data {
            if !filter_node.matches(data) {
                return false;
            }
        } else {
            return false;
        }
    }

    true
}