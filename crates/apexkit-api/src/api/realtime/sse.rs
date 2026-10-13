use crate::AppState;
use crate::BaseUrl;
use crate::hooks::{trigger_filter_hook, trigger_void_hook};
use crate::utils::resolve_collection_by_id_or_name;
use apexkit_core::query::filter::FilterNode;
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
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{OnceLock, RwLock};

static SSE_CONNECTIONS: OnceLock<RwLock<HashMap<String, usize>>> = OnceLock::new();

fn get_sse_connections() -> &'static RwLock<HashMap<String, usize>> {
    SSE_CONNECTIONS.get_or_init(|| RwLock::new(HashMap::new()))
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

struct SseConnectionGuard {
    scope_key: String,
    state: AppState,
    client_scope: EventScope,
    base_url: String,
    claims: Option<apexkit_core::auth::Claims>,
    headers_map: HashMap<String, String>,
    ip: String,
}

impl Drop for SseConnectionGuard {
    fn drop(&mut self) {
        if let Some(map) = SSE_CONNECTIONS.get() {
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
            "protocol": "sse",
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

#[derive(Deserialize)]
pub struct SseQuery {
    pub channel: Option<String>,
    pub event: Option<String>,
    pub token: Option<String>,
    pub filter: Option<String>,
    pub collection: Option<String>,
}

fn namespaced_channel_sse(scope: &EventScope, channel: &str) -> String {
    match scope {
        EventScope::Root => format!("root::{}", channel),
        EventScope::Tenant(id) => format!("tenant_{}::{}", id, channel),
        EventScope::Sandbox(id) => format!("sandbox_{}::{}", id, channel),
        _ => channel.to_string(),
    }
}

#[utoipa::path(
    get,
    path = "/sse",
    params(
        ("channel" = Option<String>, Query, description = "Specific channel to listen to"),
        ("event" = Option<String>, Query, description = "Specific event name to filter"),
        ("token" = Option<String>, Query, description = "JWT Auth Token"),
        ("filter" = Option<String>, Query, description = "In-memory JSON filter query for events"),
        ("collection" = Option<String>, Query, description = "Collection name or ID")
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
    let scope_key = get_scope_key(&client_scope);
    let mut rx = state.tx.subscribe();

    let target_channel = params
        .channel
        .clone()
        .map(|c| namespaced_channel_sse(&client_scope, &c));
    let target_event = params.event.clone();

    let mut claims = auth.map(|c| c.0);
    if let Some(token) = &params.token {
        if let Ok(c) = apexkit_core::auth::decode_jwt(token) {
            claims = Some(c);
        }
    }

    let db = state.db.clone();

    // --- ENFORCE REALTIME CONNECTION QUOTA LIMITS ---
    let (max_sse, _) = resolve_realtime_limits(&state, &client_scope).await;
    {
        let map = get_sse_connections().read().unwrap();
        let current_count = map.get(&scope_key).copied().unwrap_or(0);
        if current_count >= max_sse {
            tracing::warn!(
                "SSE connection limit reached for scope {}: {}/{}",
                scope_key,
                current_count,
                max_sse
            );
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }
    {
        let mut map = get_sse_connections().write().unwrap();
        *map.entry(scope_key.clone()).or_insert(0) += 1;
    }

    let mut headers_map = HashMap::new();
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
        if let Some(map) = SSE_CONNECTIONS.get() {
            if let Ok(mut lock) = map.write() {
                if let Some(count) = lock.get_mut(&scope_key) {
                    *count = count.saturating_sub(1);
                }
            }
        }
        return Err(StatusCode::FORBIDDEN);
    }

    // --- PARSE IN-MEMORY FILTER FOR SSE ---
    let filter_node = if let Some(ref f_str) = params.filter {
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(f_str) {
            FilterNode::parse(&json_val)
        } else {
            FilterNode::Empty
        }
    } else {
        FilterNode::Empty
    };

    let target_collection_id = if let Some(ref col_str) = params.collection {
        resolve_collection_by_id_or_name(&db, col_str)
            .await
            .ok()
            .map(|c| c.id)
    } else {
        None
    };

    let guard = SseConnectionGuard {
        scope_key,
        state: state.clone(),
        client_scope: client_scope.clone(),
        base_url: base_url.clone(),
        claims: claims.clone(),
        headers_map: headers_map.clone(),
        ip: addr.ip().to_string(),
    };

    let state_clone = state.clone();
    let base_url_clone = base_url.clone();
    let client_scope_clone = client_scope.clone();
    let claims_clone = claims.clone();

    let stream = async_stream::stream! {
        let _guard = guard;

        while let Ok(msg) = rx.recv().await {
            let should_yield = match &msg {
                DbEvent::Custom { event, scope, .. } => {
                    if let Some(req_evt) = &target_event
                        && req_evt != event { false }
                    else if let EventScope::Channel(msg_channel) = scope {
                        if let Some(req_channel) = &target_channel {
                            msg_channel == req_channel
                        } else {
                            false
                        }
                    } else {
                        scope == &client_scope_clone
                    }
                },
                DbEvent::Insert { scope, collection_id, data, .. } |
                DbEvent::Update { scope, collection_id, data, .. } => {
                    if scope != &client_scope_clone {
                        false
                    } else if let Some(target_col) = target_collection_id {
                        if *collection_id != target_col {
                            false
                        } else if !matches!(filter_node, FilterNode::Empty) {
                            filter_node.matches(data)
                        } else {
                            true
                        }
                    } else if !matches!(filter_node, FilterNode::Empty) {
                        filter_node.matches(data)
                    } else {
                        true
                    }
                },
                DbEvent::Delete { scope, collection_id, .. } => {
                    if scope != &client_scope_clone {
                        false
                    } else if let Some(target_col) = target_collection_id {
                        *collection_id == target_col
                    } else {
                        true
                    }
                }
            };

            if should_yield {
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
                            yield Ok(Event::default().data("{}"));
                        }
                    }
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()))
}
