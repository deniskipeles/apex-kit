use crate::AppError;
use crate::AppState;
use crate::DatabaseConnection;
use apexkit_core::realtime::EventScope;
use axum::{Extension, extract::State, response::Response};

#[utoipa::path(
    get,
    path = "/styles.css",
    responses((status = 200, description = "Purged Tailwind CSS", content_type = "text/css"))
)]
pub async fn serve_styles(
    State(state): State<AppState>,
    DatabaseConnection(db): DatabaseConnection,
    scope: Option<Extension<EventScope>>, // <--- EXTRACT CURRENT EVENT SCOPE
) -> Result<Response, AppError> {
    let event_scope = scope.map(|e| e.0).unwrap_or(EventScope::Root);
    let cache_key = match &event_scope {
        EventScope::Root => "root".to_string(),
        EventScope::Tenant(id) => format!("tenant:{}", id),
        EventScope::Sandbox(id) => format!("sandbox:{}", id),
        _ => "root".to_string(),
    };

    {
        let cache = state.css_cache.read().await;
        if let Some(css) = cache.get(&cache_key) {
            return Ok(Response::builder()
                .header("Content-Type", "text/css")
                .header("Cache-Control", "public, max-age=60")
                .body(axum::body::Body::from(css.clone()))
                .unwrap());
        }
    }

    let css = super::compile_css::compile_styles(db.clone())
        .await
        .map_err(AppError::UnknownError)?;

    {
        let mut cache = state.css_cache.write().await;
        cache.insert(cache_key, css.clone());
    }

    Ok(Response::builder()
        .header("Content-Type", "text/css")
        .body(axum::body::Body::from(css))
        .unwrap())
}
