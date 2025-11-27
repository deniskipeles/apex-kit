use axum::Router;
use std::sync::Arc;
use tinybase_api::app_router;
use tinybase_core::Db;
use tokio::sync::Mutex;

pub async fn setup_test_app() -> Router {
    let db = libsql::Builder::new_local(":memory:")
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();

    conn.execute_batch(
        "
        CREATE TABLE collections (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            schema JSON
        );

        CREATE TABLE records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            collection_id INTEGER NOT NULL,
            data JSON NOT NULL
        );
    ",
    )
    .await
    .unwrap();

    let db: Arc<dyn Db> = Arc::new(Mutex::new(conn));
    app_router(db)
}
