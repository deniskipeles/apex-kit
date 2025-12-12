// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/websocket.rs ===========================
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use serde::Deserialize;
use tinybase_core::{realtime::DbEvent, filter::FilterNode};
use crate::AppState;
// These imports will now work because 'futures' is in Cargo.toml
use futures::{sink::SinkExt, stream::StreamExt}; 
use tracing::{info, warn};

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
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    // .split() comes from StreamExt trait
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let mut current_filter = SubscriptionFilter::default();
    let mut current_filter_node = FilterNode::Empty;

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
                                        info!("WS Subscribed: {:?}", filter);
                                        if let Some(json) = &filter.filter {
                                            current_filter_node = FilterNode::parse(json);
                                        } else {
                                            current_filter_node = FilterNode::Empty;
                                        }
                                        current_filter = filter;
                                    },
                                    Ok(ClientMessage::Unsubscribe) => {
                                        info!("WS Unsubscribed");
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
                if matches_filter(&event, &current_filter, &current_filter_node) {
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

fn matches_filter(
    event: &DbEvent, 
    filter: &SubscriptionFilter, 
    filter_node: &FilterNode
) -> bool {
    let (col_id, rec_id, type_str, event_data) = match event {
        DbEvent::Insert { collection_id, record_id, data } => (*collection_id, *record_id, "Insert", Some(data)),
        DbEvent::Update { collection_id, record_id, data } => (*collection_id, *record_id, "Update", Some(data)),
        DbEvent::Delete { collection_id, record_id } => (*collection_id, *record_id, "Delete", None),
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