use crate::AppError;
use crate::AppState;
use crate::DatabaseConnection;
use axum::{extract::State, response::Response};

#[utoipa::path(
    get,
    path = "/styles.css",
    responses((status = 200, description = "Purged Tailwind CSS", content_type = "text/css"))
)]
pub async fn serve_styles(
    State(state): State<AppState>,
    DatabaseConnection(db): DatabaseConnection,
) -> Result<Response, AppError> {
    {
        let cache = state.css_cache.read().await;
        if !cache.is_empty() {
            return Ok(Response::builder()
                .header("Content-Type", "text/css")
                .header("Cache-Control", "public, max-age=60")
                .body(axum::body::Body::from(cache.clone()))
                .unwrap());
        }
    }

    let css = super::compile_css::compile_styles(db.clone())
        .await
        .map_err(AppError::UnknownError)?;

    {
        let mut cache = state.css_cache.write().await;
        *cache = css.clone();
    }

    Ok(Response::builder()
        .header("Content-Type", "text/css")
        .body(axum::body::Body::from(css))
        .unwrap())
}
