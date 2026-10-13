use crate::AppState;
use crate::BaseUrl;
use crate::hooks::{trigger_filter_hook, trigger_void_hook};
use crate::utils::{resolve_collection_by_id_or_name, resolve_db_from_scope};
use apexkit_core::{
    query::filter::FilterNode,
    realtime::{DbEvent, EventScope},
};
use axum::extract::ConnectInfo;
use axum::http::StatusCode;
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
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{OnceLock, RwLock};
use tracing::warn;

static WS_CONNECTIONS: OnceLock<RwLock<HashMap<String, usize>>> = OnceLock::new();

fn get_ws_connections() -> &'static RwLock<HashMap<String, usize>> {
    WS_CONNECTIONS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn get_scope_key(scope: &EventScope) -> String {
    match scope {
        EventScope::Root => "root".to_string(),
        EventScope::Tenant(id) => format!("tenant:{}", id),
        EventScope::Sandbox(id) => format!("sandbox:{}", id),
        EventScope::Channel(c) => format!("channel:{}", c),
    }
}

async fn resolve_realtime_limits(state: &AppState, scope: &EventScope) -> (usize, usize) {
    match scope {
        EventScope::Root => (usize::MAX, usize::MAX),
        EventScope::Tenant(id) => {
            let mut max_sse = 500;
            let mut max_ws = 300;
            if let Ok(tenants) = state.db.list_tenants().await {
                if let Some(t) = tenants.iter().find(|t| &t.id == id) {
                    max_sse = t.stats.max_sse as usize;
                    max_ws = t.stats.max_ws as usize;
                }
            }
            (max_sse, max_ws)
        }
        EventScope::Sandbox(_) => (50, 30),
        _ => (usize::MAX, usize::MAX),
    }
}

struct WsConnectionGuard {
    scope_key: String,
    state: AppState,
    client_scope: EventScope,
    base_url: String,
    claims: Option<apexkit_core::auth::Claims>,
    headers_map: HashMap<String, String>,
    ip: String,
}

impl Drop for WsConnectionGuard {
    fn drop(&mut self) {
        if let Some(map) = WS_CONNECTIONS.get() {
            if let Ok(mut lock) = map.write() {
                if let Some(count) = lock.get_mut(&self.scope_key) {
                    *count = count.saturating_sub(1);
                }
            }
        }

        let state = self.state.clone();
        let client_scope = self.client_scope.clone();
        let base_url = self.base_url.clone();
        let claims = self.claims.clone();
        let payload = serde_json::json!({
            "ip": self.ip,
            "protocol": "websocket",
            "headers": self.headers_map,
        });

        tokio::spawn(async move {
            let _ = trigger_void_hook(
                &state,
                "after_realtime_connection_disconnected",
                payload.clone(),
                claims.as_ref(),
                Some(&client_scope),
                Some(base_url.clone()),
            )
            .await;
            let _ = trigger_void_hook(
                &state,
                "after_realtime_connection_desconnected",
                payload,
                claims.as_ref(),
                Some(&client_scope),
                Some(base_url),
            )
            .await;
        });
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct SubscriptionFilter {
    pub collection_id: Option<serde_json::Value>,
    pub record_id: Option<i64>,
    pub event_type: Option<String>,
    pub filter: Option<serde_json::Value>,
    pub custom_event: Option<String>,
    pub channel: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct SearchRequest {
    pub collection_id: serde_json::Value,
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
    auth: Option<Extension<apexkit_core::auth::Claims>>,
    BaseUrl(base_url): BaseUrl,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    let client_scope = scope.clone().map(|e| e.0).unwrap_or_default();
    let scope_key = get_scope_key(&client_scope);
    let claims = auth.map(|c| c.0);

    let mut headers_map = HashMap::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            headers_map.insert(k.to_string(), val.to_string());
        }
    }

    // --- ENFORCE REALTIME CONNECTION QUOTA LIMITS ---
    let (_, max_ws) = resolve_realtime_limits(&state, &client_scope).await;
    {
        let map = get_ws_connections().read().unwrap();
        let current_count = map.get(&scope_key).copied().unwrap_or(0);
        if current_count >= max_ws {
            tracing::warn!(
                "WebSocket connection limit reached for scope {}: {}/{}",
                scope_key,
                current_count,
                max_ws
            );
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }
    {
        let mut map = get_ws_connections().write().unwrap();
        *map.entry(scope_key.clone()).or_insert(0) += 1;
    }

    // --- HOOK: before_realtime_connection_established ---
    let hook_payload = serde_json::json!({
        "ip": addr.ip().to_string(),
        "protocol": "websocket",
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
        tracing::warn!("WebSocket connection blocked by hook: {:?}", e);
        if let Some(map) = WS_CONNECTIONS.get() {
            if let Ok(mut lock) = map.write() {
                if let Some(count) = lock.get_mut(&scope_key) {
                    *count = count.saturating_sub(1);
                }
            }
        }
        return Err(StatusCode::FORBIDDEN);
    }

    let guard = WsConnectionGuard {
        scope_key,
        state: state.clone(),
        client_scope: client_scope.clone(),
        base_url: base_url.clone(),
        claims: claims.clone(),
        headers_map,
        ip: addr.ip().to_string(),
    };

    Ok(ws.on_upgrade(move |socket| {
        handle_socket(socket, state, client_scope, base_url, claims, guard)
    }))
}

fn namespaced_channel(scope: &EventScope, channel: &str) -> String {
    match scope {
        EventScope::Root => format!("root::{}", channel),
        EventScope::Tenant(id) => format!("tenant_{}::{}", id, channel),
        EventScope::Sandbox(id) => format!("sandbox_{}::{}", id, channel),
        _ => channel.to_string(),
    }
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    client_scope: EventScope,
    base_url: String,
    claims: Option<apexkit_core::auth::Claims>,
    guard: WsConnectionGuard,
) {
    let _guard = guard;

    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let mut current_filter = SubscriptionFilter::default();
    let mut current_filter_node = FilterNode::Empty;
    let mut active_namespaced_channel: Option<String> = None;
    let mut current_resolved_col_id: Option<i64> = None;

    let mut current_claims: Option<apexkit_core::auth::Claims> = claims.clone();

    loop {
        tokio::select! {
            client_msg = receiver.next() => {
                match client_msg {
                    Some(Ok(msg)) => {
                        match msg {
                            Message::Text(text) => {
                                match serde_json::from_str::<ClientMessage>(&text) {
                                    Ok(ClientMessage::Auth(req)) => {
                                        if let Ok(c) = apexkit_core::auth::decode_jwt(&req.token) {
                                            current_claims = Some(c);
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

                                        current_resolved_col_id = None;
                                        if let Some(ref col_val) = filter.collection_id
                                            && let Ok(db) = resolve_db_from_scope(&state, &client_scope).await {
                                                let identifier = match col_val {
                                                    serde_json::Value::String(s) => s.clone(),
                                                    serde_json::Value::Number(n) => n.to_string(),
                                                    _ => "".to_string(),
                                                };
                                                if !identifier.is_empty()
                                                    && let Ok(col) = resolve_collection_by_id_or_name(&db, &identifier).await {
                                                        current_resolved_col_id = Some(col.id);
                                                    }
                                            }

                                        current_filter = filter;
                                    },
                                    Ok(ClientMessage::Signal(req)) => {
                                        let scoped_channel = namespaced_channel(&client_scope, &req.channel);
                                        let event = DbEvent::Custom {
                                            event: req.event,
                                            data: req.data,
                                            node_id: Some(apexkit_core::realtime::types::get_node_id().to_string()),
                                            is_local: true,
                                            scope: EventScope::Channel(scoped_channel)
                                        };
                                        let _ = state.tx.send(event);
                                    },
                                    Ok(ClientMessage::Unsubscribe) => {
                                        current_filter = SubscriptionFilter::default();
                                        current_filter_node = FilterNode::Empty;
                                        active_namespaced_channel = None;
                                        current_resolved_col_id = None;
                                    },
                                    Ok(ClientMessage::Ping) => {
                                        let _ = sender.send(Message::Text("Pong".into())).await;
                                    },
                                    Ok(ClientMessage::Search(req)) => {
                                        let limit = req.limit.unwrap_or(10).min(50);

                                        let mut resolved_search_id = None;
                                        let mut current_db = state.db.clone();

                                        if let Ok(db) = resolve_db_from_scope(&state, &client_scope).await {
                                            current_db = db.clone();
                                            let identifier = match &req.collection_id {
                                                serde_json::Value::String(s) => s.clone(),
                                                serde_json::Value::Number(n) => n.to_string(),
                                                _ => "".to_string(),
                                            };
                                            if !identifier.is_empty()
                                                && let Ok(col) = resolve_collection_by_id_or_name(&db, &identifier).await {
                                                    resolved_search_id = Some(col.id);
                                                }
                                        }

                                        let mut allowed = false;
                                        if let Some(col_id) = resolved_search_id
                                            && let Ok(Some(col)) = current_db.get_collection(col_id).await {
                                                let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
                                                allowed = apexkit_core::auth::policies::check_access(
                                                    policy,
                                                    current_claims.as_ref(),
                                                    None,
                                                    None,
                                                    Some(current_db.clone())
                                                ).await;
                                            }

                                        if !allowed || resolved_search_id.is_none() {
                                            let err_resp = serde_json::json!({ "type": "Error", "request_id": req.request_id, "message": "Access denied" });
                                            let _ = sender.send(Message::Text(err_resp.to_string().into())).await;
                                            continue;
                                        }

                                        let col_id = resolved_search_id.unwrap();
                                        match current_db.instant_search(col_id, &req.query, limit).await {
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
                if matches_scope(&event, &client_scope, &active_namespaced_channel) && matches_filter(&event, &current_filter, &current_filter_node, current_resolved_col_id) {

                    let mut allowed = true;
                    match &event {
                        DbEvent::Insert { collection_id, data, .. } | DbEvent::Update { collection_id, data, .. } => {
                            if let Ok(Some(col)) = state.db.get_collection(*collection_id).await {
                                let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
                                allowed = apexkit_core::auth::policies::check_access(policy, current_claims.as_ref().or(claims.as_ref()), Some(data), None, Some(state.db.clone())).await;
                            }
                        }
                        DbEvent::Delete { collection_id, .. } => {
                            if let Ok(Some(col)) = state.db.get_collection(*collection_id).await {
                                let policy = col.schema.as_ref().map(|s| s.policies.read.as_str()).unwrap_or("public");
                                allowed = apexkit_core::auth::policies::check_access(policy, current_claims.as_ref().or(claims.as_ref()), None, None, Some(state.db.clone())).await;
                            }
                        }
                        _ => {}
                    }

                    if allowed {
                        let event_json = serde_json::to_value(&event).unwrap_or(serde_json::json!({}));

                        // --- HOOK: before_realtime_data_pushed ---
                        match trigger_filter_hook(
                            &state,
                            "before_realtime_data_pushed",
                            event_json,
                            current_claims.as_ref(),
                            Some(&client_scope),
                            Some(base_url.clone())
                        ).await {
                            Ok(modified_data) => {
                                if let Ok(json_msg) = serde_json::to_string(&modified_data) {
                                    if sender.send(Message::Text(json_msg.into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(_) => {
                                let _ = sender.send(Message::Text(serde_json::json!({}).to_string().into())).await;
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

fn matches_filter(
    event: &DbEvent,
    filter: &SubscriptionFilter,
    filter_node: &FilterNode,
    resolved_col_id: Option<i64>,
) -> bool {
    match event {
        DbEvent::Custom {
            event: evt_name,
            data,
            ..
        } => {
            if let Some(req_evt) = &filter.custom_event
                && req_evt != evt_name
            {
                return false;
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
            resolved_col_id,
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
            resolved_col_id,
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
            resolved_col_id,
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
    resolved_col_id: Option<i64>,
) -> bool {
    if let Some(req_col_id) = resolved_col_id
        && req_col_id != col_id
    {
        return false;
    }
    if let Some(req_rec) = filter.record_id
        && req_rec != rec_id
    {
        return false;
    }
    if let Some(req_type) = &filter.event_type
        && !req_type.eq_ignore_ascii_case(type_str)
    {
        return false;
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
