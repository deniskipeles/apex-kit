use crate::AppState;
use apexkit_core::realtime::DbEvent;
use apexkit_core::realtime::EventScope;
use axum::response::sse::{Event, Sse};
use axum::{
    Extension,
    extract::{Query, State},
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;

// [NEW] DTO for SSE Query Params
#[derive(Deserialize)]
pub struct SseQuery {
    pub channel: Option<String>,
    pub event: Option<String>,
    pub token: Option<String>,
}

// Helper to namespace channels (Same logic as websocket.rs to ensure security)
fn namespaced_channel_sse(scope: &EventScope, channel: &str) -> String {
    match scope {
        EventScope::Root => format!("root::{}", channel),
        EventScope::Tenant(id) => format!("tenant_{}::{}", id, channel),
        EventScope::Sandbox(id) => format!("sandbox_{}::{}", id, channel),
        _ => channel.to_string(), // Should not happen for channels
    }
}

#[utoipa::path(
    get,
    path = "/sse",
    params(
        ("channel" = Option<String>, Query, description = "Specific channel to listen to"),
        ("event" = Option<String>, Query, description = "Specific event name to filter"),
        ("token" = Option<String>, Query, description = "JWT Auth Token")
    ),
    responses((status = 200, description = "SSE Stream"))
)]
pub async fn sse_handler(
    State(state): State<AppState>,
    scope: Option<Extension<EventScope>>,
    Query(params): Query<SseQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let client_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let mut rx = state.tx.subscribe();

    let target_channel = params
        .channel
        .clone()
        .map(|c| namespaced_channel_sse(&client_scope, &c));
    let target_event = params.event.clone();

    // [ADDED] Verify identity for stream authorization
    let claims = if let Some(token) = &params.token {
        apexkit_core::auth::decode_jwt(token).ok()
    } else {
        None
    };

    let db = state.db.clone();

    let stream = async_stream::stream! {
        while let Ok(msg) = rx.recv().await {
            let should_yield = match &msg {
                DbEvent::Custom { event, scope, data: _ } => {
                    if let Some(req_evt) = &target_event
                        && req_evt != event { continue; }

                    if let EventScope::Channel(msg_channel) = scope {
                        if let Some(req_channel) = &target_channel {
                            msg_channel == req_channel
                        } else {
                            false
                        }
                    } else {
                        scope == &client_scope
                    }
                },
                DbEvent::Insert { scope, .. } |
                DbEvent::Update { scope, .. } |
                DbEvent::Delete { scope, .. } => {
                    scope == &client_scope
                }
            };

            if should_yield {
                // [ADDED] Row-Level Security checks for Streaming SSE Broadcasts
                let mut allowed = true;
                match &msg {
                    DbEvent::Insert { collection_id, data, .. } | DbEvent::Update { collection_id, data, .. } => {
                        if let Ok(Some(col)) = db.get_collection(*collection_id).await {
                            let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
                            allowed = apexkit_core::auth::policies::check_access(policy, claims.as_ref(), Some(data), None, Some(db.clone())).await;
                        }
                    }
                    DbEvent::Delete { collection_id, .. } => {
                        if let Ok(Some(col)) = db.get_collection(*collection_id).await {
                            let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
                            allowed = apexkit_core::auth::policies::check_access(policy, claims.as_ref(), None, None, Some(db.clone())).await;
                        }
                    }
                    _ => {}
                }

                if allowed
                    && let Ok(json_data) = serde_json::to_string(&msg) {
                        yield Ok(Event::default().data(json_data));
                    }
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}
