use crate::AppState;
use crate::BaseUrl;
use crate::hooks::{trigger_filter_hook, trigger_void_hook};
use apexkit_core::realtime::DbEvent;
use apexkit_core::realtime::EventScope;
use axum::extract::ConnectInfo;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::{
    Extension,
    extract::{Query, State},
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::net::SocketAddr;

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
    auth: Option<Extension<apexkit_core::auth::Claims>>,
    BaseUrl(base_url): BaseUrl,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Query(params): Query<SseQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let client_scope = scope.clone().map(|e| e.0).unwrap_or(EventScope::Root);
    let mut rx = state.tx.subscribe();

    let target_channel = params
        .channel
        .clone()
        .map(|c| namespaced_channel_sse(&client_scope, &c));
    let target_event = params.event.clone();

    // Verify identity for stream authorization
    let mut claims = auth.map(|c| c.0);
    if let Some(token) = &params.token {
        if let Ok(c) = apexkit_core::auth::decode_jwt(token) {
            claims = Some(c);
        }
    }

    let mut headers_map = std::collections::HashMap::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            headers_map.insert(k.to_string(), val.to_string());
        }
    }

    // --- HOOK: before_realtime_connection_established ---
    let hook_payload = serde_json::json!({
        "ip": addr.ip().to_string(),
        "protocol": "sse",
        "headers": headers_map
    });

    if let Err(e) = trigger_void_hook(
        &state,
        "before_realtime_connection_established",
        hook_payload,
        claims.as_ref(),
        Some(&client_scope),
        Some(base_url.clone()),
    )
    .await
    {
        tracing::warn!("SSE connection blocked by hook: {:?}", e);
        return Err(StatusCode::FORBIDDEN); // Drops connection immediately
    }
    // ----------------------------------------------------

    let db = state.db.clone();
    let state_clone = state.clone();
    let base_url_clone = base_url.clone();
    let client_scope_clone = client_scope.clone();
    let claims_clone = claims.clone();

    let stream = async_stream::stream! {
        while let Ok(msg) = rx.recv().await {
            let should_yield = match &msg {
                DbEvent::Custom { event, scope, .. } => {
                    if let Some(req_evt) = &target_event
                        && req_evt != event { continue; }

                    if let EventScope::Channel(msg_channel) = scope {
                        if let Some(req_channel) = &target_channel {
                            msg_channel == req_channel
                        } else {
                            false
                        }
                    } else {
                        scope == &client_scope_clone
                    }
                },
                DbEvent::Insert { scope, .. } |
                DbEvent::Update { scope, .. } |
                DbEvent::Delete { scope, .. } => {
                    scope == &client_scope_clone
                }
            };

            if should_yield {
                // Row-Level Security checks for Streaming SSE Broadcasts
                let mut allowed = true;
                match &msg {
                    DbEvent::Insert { collection_id, data, .. } | DbEvent::Update { collection_id, data, .. } => {
                        if let Ok(Some(col)) = db.get_collection(*collection_id).await {
                            let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
                            allowed = apexkit_core::auth::policies::check_access(policy, claims_clone.as_ref(), Some(data), None, Some(db.clone())).await;
                        }
                    }
                    DbEvent::Delete { collection_id, .. } => {
                        if let Ok(Some(col)) = db.get_collection(*collection_id).await {
                            let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
                            allowed = apexkit_core::auth::policies::check_access(policy, claims_clone.as_ref(), None, None, Some(db.clone())).await;
                        }
                    }
                    _ => {}
                }

                if allowed {
                    let event_json = serde_json::to_value(&msg).unwrap_or(serde_json::json!({}));

                    // --- HOOK: before_realtime_data_pushed ---
                    match trigger_filter_hook(
                        &state_clone,
                        "before_realtime_data_pushed",
                        event_json,
                        claims_clone.as_ref(),
                        Some(&client_scope_clone),
                        Some(base_url_clone.clone())
                    ).await {
                        Ok(modified_data) => {
                            if let Ok(json_data) = serde_json::to_string(&modified_data) {
                                yield Ok(Event::default().data(json_data));
                            }
                        }
                        Err(_) => {
                            // Errored Hook returns an empty object
                            yield Ok(Event::default().data("{}"));
                        }
                    }
                    // -----------------------------------------
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()))
}
