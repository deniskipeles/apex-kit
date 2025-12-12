use rust_embed::RustEmbed;
use axum::{
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    body::Body,
};

// 1. EMBEDDING: This macro compiles everything in "../static/" into the binary.
#[derive(RustEmbed)]
#[folder = "../static/"] 
pub struct Assets;

// Helper to serve a specific file by path string
fn serve_file(path: &str) -> Response {
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                // Cache control: Cache for 1 hour
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(content.data))
                .unwrap()
        }
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

// 2. ROOT HANDLER: Serves index.html
pub async fn index_handler() -> impl IntoResponse {
    serve_file("index.html")
}

// 3. GENERIC STATIC ASSET HANDLER
// This will take any URL path (e.g. /favicon.ico, /js/microframe.js) 
// and try to find it in the embedded folder.
pub async fn serve_static_asset(uri: Uri) -> impl IntoResponse {
    // 1. Get the full path (e.g., "/static/microframe.js")
    let full_path = uri.path();
    
    // 2. Strip the "/static/" prefix to match the embedded file structure
    // If the URL is "/static/microframe.js", we want "microframe.js"
    let clean_path = full_path.strip_prefix("/static/")
        .unwrap_or(full_path.trim_start_matches('/'));

    serve_file(clean_path)
}

// 4. DASHBOARD HANDLER (React SPA Logic)
pub async fn dashboard_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/').to_string();

    // Explicitly handle the root dashboard request
    if path == "_dashboard" || path == "_dashboard/" {
        return serve_file("dashboard/index.html");
    }

    // Rewrite path: /_dashboard/assets/x.js -> dashboard/assets/x.js
    let relative_path = path.replace("_dashboard/", "dashboard/");

    // Try to serve the exact file
    if Assets::get(&relative_path).is_some() {
        return serve_file(&relative_path);
    }

    // Fallback for SPA routing (if file not found, serve index.html)
    serve_file("dashboard/index.html")
}