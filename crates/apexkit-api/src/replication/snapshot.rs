use super::{USE_HTTP_FALLBACK, build_grpc_channel, client_auth_interceptor, is_http_fallback, pb};
use std::sync::atomic::Ordering;
use tokio::io::AsyncWriteExt;

pub async fn fetch_snapshot_from_master(master_url: &str, db_path: &str) -> Result<(), String> {
    let do_http = || async {
        let url = format!(
            "{}/replication/snapshot?db_path={}",
            master_url.trim_end_matches('/'),
            db_path
        );
        let master_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
        let client = reqwest::Client::new();

        let mut response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", master_key))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "HTTP Snapshot download failed: {}",
                response.status()
            ));
        }
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }

        let tmp_path = format!("{}.tmp", db_path);
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| e.to_string())?;
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        }
        file.sync_all().await.map_err(|e| e.to_string())?;

        tokio::fs::rename(&tmp_path, db_path)
            .await
            .map_err(|e| e.to_string())?;
        let _ = tokio::fs::remove_file(format!("{}-wal", db_path)).await;
        let _ = tokio::fs::remove_file(format!("{}-shm", db_path)).await;
        Ok(())
    };

    if is_http_fallback() {
        return do_http().await;
    }

    let channel = match build_grpc_channel(master_url).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("gRPC channel error: {}. Switching to HTTP.", e);
            USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
            return do_http().await;
        }
    };

    let mut client = pb::replication_client::ReplicationClient::with_interceptor(
        channel,
        client_auth_interceptor,
    )
    .max_decoding_message_size(100 * 1024 * 1024)
    .max_encoding_message_size(100 * 1024 * 1024);

    let req = tonic::Request::new(pb::SnapshotRequest {
        db_path: db_path.to_string(),
    });

    match client.fetch_db_snapshot(req).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            let tmp_path = format!("{}.tmp", db_path);
            let mut file = tokio::fs::File::create(&tmp_path)
                .await
                .map_err(|e| e.to_string())?;
            let mut success = true;
            let mut total_bytes = 0;
            loop {
                match stream.message().await {
                    Ok(Some(chunk)) => {
                        file.write_all(&chunk.data)
                            .await
                            .map_err(|e| e.to_string())?;
                        total_bytes += chunk.data.len();
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!(
                            "gRPC stream failed mid-transfer: {}. Switching to HTTP.",
                            e
                        );
                        success = false;
                        break;
                    }
                }
            }
            if success && total_bytes > 0 {
                file.sync_all().await.map_err(|e| e.to_string())?;
                tokio::fs::rename(&tmp_path, db_path)
                    .await
                    .map_err(|e| e.to_string())?;
                let _ = tokio::fs::remove_file(format!("{}-wal", db_path)).await;
                let _ = tokio::fs::remove_file(format!("{}-shm", db_path)).await;
                Ok(())
            } else {
                tracing::warn!(
                    "gRPC snapshot transfer failed or returned 0 bytes. Falling back to HTTP."
                );
                USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
                return do_http().await;
            }
        }
        Err(e) => {
            tracing::warn!(
                "❌ Failed to fetch snapshot via gRPC for {}: {}. Falling back to HTTP.",
                db_path,
                e
            );
            USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
            return do_http().await;
        }
    }
}

pub async fn ensure_replica_env(base_path: &str) {
    do_sync_env(base_path, false).await;
}
pub async fn force_replica_sync(base_path: &str) {
    do_sync_env(base_path, true).await;
}

async fn do_sync_env(base_path: &str, force: bool) {
    if let Ok(master_url) = std::env::var("APEXKIT_MASTER_URL")
        && !master_url.is_empty()
    {
        if force {
            tracing::warn!(
                "🔄 [ReplicaEnv] FORCING DB snapshot sync for path: {}",
                base_path
            );
        } else {
            tracing::info!(
                "🔄 [ReplicaEnv] Ensuring DB snapshot existence for path: {}",
                base_path
            );
        }

        let dbs = ["core.db", "data.db", "system.db", "vectors.db"];
        for db in dbs {
            let db_path = format!("{}/{}", base_path, db);
            if force || !std::path::Path::new(&db_path).exists() {
                tracing::info!(
                    "📥 [ReplicaEnv] Fetching snapshot for {} from Master...",
                    db_path
                );
                let res = fetch_snapshot_from_master(&master_url, &db_path).await;
                if let Err(e) = res {
                    tracing::error!(
                        "❌ [ReplicaEnv] Failed to fetch snapshot for {}: {}",
                        db_path,
                        e
                    );
                } else {
                    tracing::info!(
                        "✅ [ReplicaEnv] Successfully fetched snapshot for {}",
                        db_path
                    );
                }
            }
        }
    }
}
