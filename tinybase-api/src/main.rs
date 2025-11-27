use axum::serve;
use std::sync::Arc;
use tinybase_api::app_router;
use tinybase_core::a_new_database_connection;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let db = match a_new_database_connection().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Failed to connect to database: {}", e);
            return;
        }
    };
    let app = app_router(Arc::new(db));

    let listener = match TcpListener::bind("0.0.0.0:3000").await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("Failed to bind to port 3000: {}", e);
            return;
        }
    };
    println!("listening on {}", listener.local_addr().unwrap());
    if let Err(e) = serve(listener, app).await {
        eprintln!("Server error: {}", e);
    }
}
