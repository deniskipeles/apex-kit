use super::spa::get_public_dir;
use apexkit_core::realtime::EventScope;
use axum::{
    body::Body,
    extract::{Extension, Path}, // Add Path extractor
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;
use std::path::Path as StdPath;
use tokio::fs; // Avoid conflict with axum Path

// 1. EMBEDDING
#[derive(RustEmbed)]
#[folder = "../../static/"]
pub struct Assets;

// Helper to serve embedded file
fn serve_embedded(path: &str) -> Response {
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let cache_control = get_cache_header(path); // <--- USE HELPER

            // Generate simple ETag based on hash of content
            let etag = format!("\"{:x}\"", md5::compute(&content.data));

            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, cache_control)
                .header(header::ETAG, etag) // <--- ADD ETAG
                .body(Body::from(content.data))
                .unwrap()
        }
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

// Helper to serve from disk (async)
async fn serve_from_disk(full_path: &StdPath) -> Option<Response> {
    if full_path.exists()
        && full_path.is_file()
        && let Ok(content) = fs::read(full_path).await
    {
        let mime = mime_guess::from_path(full_path).first_or_octet_stream();
        let filename = full_path.file_name().unwrap().to_str().unwrap();
        let cache_control = get_cache_header(filename); // <--- USE HELPER

        // Simple ETag from file size + modified time (cheap)
        let meta = full_path.metadata().unwrap();
        let etag = format!(
            "\"{}_{}\"",
            meta.len(),
            meta.modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );

        return Some(
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, cache_control)
                .header(header::ETAG, etag) // <--- ADD ETAG
                .body(Body::from(content))
                .unwrap(),
        );
    }
    None
}

// Helper to determine cache headers based on path
fn get_cache_header(path: &str) -> String {
    if path.ends_with("index.html") || !path.contains('.') {
        // HTML/Roots: Don't cache, or cache very briefly to ensure updates are seen
        // "no-cache" means "check with server before using cached copy" (304 Not Modified)
        "public, no-cache".to_string()
    } else {
        // Assets (JS, CSS, PNG): Cache for 1 year, immutable
        // Vite handles hashing filenames (e.g. index-XyZ.js), so we can cache aggressively
        "public, max-age=31536000, immutable".to_string()
    }
}

// 2. ROOT INDEX HANDLER: Serves /
pub async fn index_handler(scope: Option<Extension<EventScope>>) -> Response {
    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let public_dir = get_public_dir(&event_scope);
    let custom_index = public_dir.join("index.html");

    // 1. Try Custom Site
    if let Some(res) = serve_from_disk(&custom_index).await {
        return res;
    }

    // 2. Fallback to Embedded
    serve_embedded("index.html")
}

// 3. TENANT/SANDBOX ROOT HANDLER
// Matches /tenant/{id} or /sandbox/{id}
pub async fn scoped_index_handler(
    scope: Option<Extension<EventScope>>,
    // We accept path params just to match the route, but rely on scope from middleware
    _params: Option<Path<String>>,
) -> Response {
    // Re-use index logic (scope is already set by middleware)
    index_handler(scope).await
}

// 4. GENERIC ASSET HANDLER / SPA ROUTER
pub async fn serve_static_asset(scope: Option<Extension<EventScope>>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if path.starts_with("api/") || path.starts_with("_dashboard/") {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }

    let event_scope = scope.map(|s| s.0).unwrap_or(EventScope::Root);
    let public_dir = get_public_dir(&event_scope);

    // Clean up path
    let relative_path = if path.starts_with("tenant/") {
        let parts: Vec<&str> = path.splitn(3, '/').collect();
        if parts.len() == 3 { parts[2] } else { "" }
    } else if path.starts_with("sandbox/") {
        let parts: Vec<&str> = path.splitn(3, '/').collect();
        if parts.len() == 3 { parts[2] } else { "" }
    } else {
        path
    };

    if relative_path.is_empty() {
        return index_handler(Some(Extension(event_scope))).await;
    }

    // A. Direct File Match (e.g. /css/style.css)
    let direct_path = public_dir.join(relative_path);
    if let Some(res) = serve_from_disk(&direct_path).await {
        return res;
    }

    // B. SSG HTML Match (e.g. /about -> /about.html)
    // This is crucial for Next.js Static Exports
    if !relative_path.contains('.') {
        let html_path = public_dir.join(format!("{}.html", relative_path));
        if let Some(res) = serve_from_disk(&html_path).await {
            return res;
        }

        let dir_index_path = public_dir.join(relative_path).join("index.html");
        if let Some(res) = serve_from_disk(&dir_index_path).await {
            return res;
        }
    }

    // C. Check Embedded Static
    if relative_path.starts_with("static/") {
        let clean_path = relative_path
            .strip_prefix("static/")
            .unwrap_or(relative_path);
        return serve_embedded(clean_path);
    }

    // D. SPA Fallback (Client-Side Routing)
    // If it's not a file extension we recognize as binary/asset, default to index.html
    // allowing React/Next Router to handle the 404 UI.
    if !relative_path.contains('.') {
        let custom_index = public_dir.join("index.html");
        if custom_index.exists()
            && let Some(res) = serve_from_disk(&custom_index).await
        {
            return res;
        }
    }

    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
}

// 5. DASHBOARD HANDLER (React SPA Logic)
pub async fn dashboard_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/').to_string();

    if path == "_dashboard" || path == "_dashboard/" {
        return serve_embedded("dashboard/index.html");
    }

    let relative_path = path.replace("_dashboard/", "dashboard/");

    if Assets::get(&relative_path).is_some() {
        return serve_embedded(&relative_path);
    }

    serve_embedded("dashboard/index.html")
}
