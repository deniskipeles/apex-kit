// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/websocket.rs start here ===========================
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use crate::AppState;

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    // Subscribe to the broadcast channel
    let mut rx = state.tx.subscribe();

    while let Ok(event) = rx.recv().await {
        if let Ok(json_msg) = serde_json::to_string(&event) {
            // FIX: Add .into() to convert String -> Utf8Bytes
            if socket.send(Message::Text(json_msg.into())).await.is_err() {
                break; 
            }
        }
    }
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/websocket.rs ends here ===========================