use super::pb;
use super::{
    FallbackSyncFileReq, FallbackSyncFileRes, USE_HTTP_FALLBACK, WS_OUTBOUND_TX, WsReplMsg,
    build_grpc_channel, client_auth_interceptor, get_pending_reqs, is_http_fallback,
};
use apexkit_core::batching::WriteForwarder;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::RwLock;
use tonic::transport::Channel;

pub struct GrpcWriteForwarder {
    pub master_url: String,
    pub channel: Arc<RwLock<Option<Channel>>>,
}

impl GrpcWriteForwarder {
    pub fn new(master_url: String) -> Self {
        Self {
            master_url,
            channel: Arc::new(RwLock::new(None)),
        }
    }

    async fn get_channel(&self) -> Result<Channel, String> {
        {
            let lock = self.channel.read().await;
            if let Some(ch) = &*lock {
                return Ok(ch.clone());
            }
        }
        let mut lock = self.channel.write().await;
        if let Some(ch) = &*lock {
            return Ok(ch.clone());
        }

        let ch = build_grpc_channel(&self.master_url).await?;
        *lock = Some(ch.clone());
        Ok(ch)
    }

    async fn fallback_ws_write(
        &self,
        db_path: &str,
        sql: &str,
        json_params: &Vec<serde_json::Value>,
    ) -> Result<(i64, u64), String> {
        let req_id = uuid::Uuid::new_v4().to_string();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        get_pending_reqs()
            .write()
            .await
            .insert(req_id.clone(), reply_tx);

        let msg = WsReplMsg::WriteRequest {
            req_id: req_id.clone(),
            db_path: db_path.to_string(),
            sql: sql.to_string(),
            params: serde_json::to_vec(json_params).unwrap(),
        };

        let sent = if let Some(lock) = WS_OUTBOUND_TX.get() {
            if let Some(tx) = lock.read().await.as_ref() {
                tx.send(msg).await.is_ok()
            } else {
                false
            }
        } else {
            false
        };

        if !sent {
            get_pending_reqs().write().await.remove(&req_id);
            return Err("WS disconnected or not initialized".into());
        }

        match tokio::time::timeout(Duration::from_secs(15), reply_rx).await {
            Ok(Ok(res)) => {
                if res.success {
                    Ok((res.insert_id, 1))
                } else {
                    Err(res.error)
                }
            }
            _ => {
                get_pending_reqs().write().await.remove(&req_id);
                Err("WS write request timed out".into())
            }
        }
    }
}

#[async_trait::async_trait]
impl WriteForwarder for GrpcWriteForwarder {
    async fn forward_write(
        &self,
        db_path: String,
        sql: String,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<(i64, u64), String> {
        let mut json_params = Vec::new();
        for p in params {
            match p {
                rusqlite::types::Value::Null => json_params.push(serde_json::Value::Null),
                rusqlite::types::Value::Integer(i) => json_params.push(serde_json::json!(i)),
                rusqlite::types::Value::Real(f) => json_params.push(serde_json::json!(f)),
                rusqlite::types::Value::Text(s) => json_params.push(serde_json::json!(s)),
                rusqlite::types::Value::Blob(b) => {
                    use base64::{Engine as _, engine::general_purpose::STANDARD};
                    json_params
                        .push(serde_json::json!({ "__type": "blob", "data": STANDARD.encode(&b) }));
                }
            }
        }

        if is_http_fallback() {
            return self.fallback_ws_write(&db_path, &sql, &json_params).await;
        }

        let channel = match self.get_channel().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("gRPC channel error: {}. Switching to WS.", e);
                USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
                return self.fallback_ws_write(&db_path, &sql, &json_params).await;
            }
        };

        let mut client = pb::replication_client::ReplicationClient::with_interceptor(
            channel,
            client_auth_interceptor,
        )
        .max_decoding_message_size(100 * 1024 * 1024)
        .max_encoding_message_size(100 * 1024 * 1024);

        let req = tonic::Request::new(pb::WriteRequest {
            sql: sql.clone(),
            params: serde_json::to_vec(&json_params).unwrap(),
            db_path: db_path.clone(),
        });

        match client.execute_write(req).await {
            Ok(res) => {
                let inner = res.into_inner();
                if inner.success {
                    Ok((inner.insert_id, 1))
                } else {
                    Err(inner.error)
                }
            }
            Err(e) => {
                tracing::warn!("gRPC Write failed ({}). Switching to WS fallback.", e);
                USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
                self.fallback_ws_write(&db_path, &sql, &json_params).await
            }
        }
    }
}

pub async fn forward_file_to_master(
    master_url: &str,
    scope: &str,
    filename: &str,
    mime: &str,
    data: &[u8],
) -> Result<(), String> {
    let do_http = || async {
        let url = format!("{}/replication/sync-file", master_url.trim_end_matches('/'));
        let master_key = std::env::var("APEXKIT_MASTER_KEY").unwrap_or_default();
        let client = reqwest::Client::new();

        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let req = FallbackSyncFileReq {
            scope: scope.to_string(),
            filename: filename.to_string(),
            mime_type: mime.to_string(),
            data: STANDARD.encode(data),
        };
        let res: FallbackSyncFileRes = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", master_key))
            .json(&req)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        if res.success { Ok(()) } else { Err(res.error) }
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

    let req = tonic::Request::new(pb::SyncFileRequest {
        scope: scope.to_string(),
        filename: filename.to_string(),
        mime_type: mime.to_string(),
        data: data.to_vec(),
    });

    match client.sync_file(req).await {
        Ok(res) => {
            if res.into_inner().success {
                Ok(())
            } else {
                Err("File Sync Failed".into())
            }
        }
        Err(e) => {
            tracing::warn!("gRPC sync_file failed ({}). Switching to HTTP fallback.", e);
            USE_HTTP_FALLBACK.store(true, Ordering::SeqCst);
            do_http().await
        }
    }
}
