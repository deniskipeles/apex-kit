use rust_embed::RustEmbed;
use axum::{
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    body::Body,
};

// This tells Rust to compile everything inside "../static" into the binary
#[derive(RustEmbed)]
#[folder = "../static/"] 
pub struct Assets;

pub struct StaticFile<T>(pub T);

impl<T> IntoResponse for StaticFile<T>
where
    T: Into<String>,
{
    fn into_response(self) -> Response {
        let path = self.0.into();
        
        match Assets::get(path.as_str()) {
            Some(content) => {
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                (
                    [(header::CONTENT_TYPE, mime.as_ref())],
                    Body::from(content.data),
                )
                    .into_response()
            }
            None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
        }
    }
}

// Handler for the Root Landing Page
pub async fn index_handler() -> impl IntoResponse {
    StaticFile("index.html")
}

// Handler for the React Dashboard (supports SPA routing)
pub async fn dashboard_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/').to_string();

    // If accessing /_dashboard/, we want to look inside the "dashboard" folder in static
    if path == "_dashboard" || path == "_dashboard/" {
        return StaticFile("dashboard/index.html").into_response();
    }

    // Determine the file path relative to the embedded root
    // Example: /_dashboard/assets/index.js -> dashboard/assets/index.js
    // We strip the "_dashboard/" prefix to find it in the embedded "dashboard/" folder
    let relative_path = path.replace("_dashboard/", "dashboard/");

    // Check if file exists
    if Assets::get(&relative_path).is_some() {
        return StaticFile(relative_path).into_response();
    }

    // Fallback for React Router: If file not found (and it's inside dashboard), serve dashboard/index.html
    StaticFile("dashboard/index.html").into_response()
}