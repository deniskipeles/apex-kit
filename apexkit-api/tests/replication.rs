use axum::{
    body::{Body, to_bytes},
    http::StatusCode,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tower::ServiceExt;

mod common;
use common::{admin_token, setup_test_context_with_forwarder, test_request};

#[tokio::test]
async fn test_full_replication_loop() {
    // 1. Set environment variables to force HTTP fallback (simulates HF Spaces)
    unsafe {
        std::env::set_var("APEXKIT_MASTER_KEY", "TEST_MASTER_KEY_12345");
        std::env::set_var("APEX_FORCE_HTTP_REPLICATION", "true");
    }

    // 2. Setup MASTER
    let (sqlite_event_tx, mut _sqlite_event_rx) = tokio::sync::broadcast::channel(100);

    // Initialize Master Tracker
    apexkit_api::replication::init_master_replica_tracker(sqlite_event_tx.clone()).await;

    let master_ctx = setup_test_context_with_forwarder(None, Some(sqlite_event_tx.clone())).await;

    // Start Master server on a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let master_url = format!("http://127.0.0.1:{}", port);

    let master_app = master_ctx.app.clone();
    tokio::spawn(async move {
        axum::serve(
            listener,
            master_app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    // 3. Setup REPLICA
    // Replica gets a GrpcWriteForwarder pointing to the Master's URL
    let forwarder: Arc<dyn apexkit_core::batching::WriteForwarder> = Arc::new(
        apexkit_api::replication::GrpcWriteForwarder::new(master_url.clone()),
    );

    let replica_ctx = setup_test_context_with_forwarder(Some(forwarder), None).await;
    let replica_state = replica_ctx.state.clone();
    let replica_app = replica_ctx.app.clone();

    // Start Replica WebSocket Streamer in the background
    let replica_id = uuid::Uuid::new_v4().to_string();
    tokio::spawn(async move {
        apexkit_api::replication::start_ws_event_streamer(
            master_url,
            Some(replica_state),
            replica_id,
        )
        .await;
    });

    // Wait a moment for WS connection to establish
    tokio::time::sleep(Duration::from_millis(500)).await;

    // --- EXECUTE THE TEST ---

    // 4. Send a Create Collection request to the REPLICA
    let response = replica_app
        .oneshot(
            test_request()
                .method("POST")
                .uri("/api/v1/collections")
                .header("authorization", format!("Bearer {}", admin_token()))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "ReplicaTest",
                        "schema": { "fields": { "title": { "type": "string", "required": true } } }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // The replica should forward the write over HTTP and return success
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Replica failed to forward write to Master"
    );

    let body_bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let collection: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(collection["name"], "ReplicaTest");

    // 5. Verify Master created it
    let master_cols = master_ctx.state.db.list_collections().await.unwrap();
    assert!(
        master_cols.iter().any(|c| c.name == "ReplicaTest"),
        "Master did not receive the forwarded write"
    );

    // 6. Simulate the WebSocket sync (Usually `start_ws_event_streamer` handles this, but since
    // SQLite session extension relies on physical WAL file syncs which can be tricky in fast tests,
    // we explicitly verify the Replica DB sync event loop works by verifying the DB object directly).

    // Give the WebSocket 1 second to pull down the changeset and apply it to the Replica's SQLite file
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify Replica applied the changeset locally
    let replica_cols = replica_ctx.state.db.list_collections().await.unwrap();
    assert!(
        replica_cols.iter().any(|c| c.name == "ReplicaTest"),
        "Replica failed to apply the broadcasted changeset via WebSocket"
    );
}
