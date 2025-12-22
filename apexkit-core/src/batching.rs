use tokio::sync::{mpsc, oneshot};
use libsql::Database;
use std::sync::Arc;
use std::time::Duration;

// Type of write operation
pub enum WriteRequest {
    Execute {
        sql: String,
        params: Vec<libsql::Value>,
        reply: oneshot::Sender<Result<u64, String>>, // Returns rows affected
    },
    InsertReturningId {
        sql: String,
        params: Vec<libsql::Value>,
        reply: oneshot::Sender<Result<i64, String>>, // Returns Last Insert ID
    }
}

#[derive(Clone)]
pub struct WriteManager {
    sender: mpsc::Sender<WriteRequest>,
}

impl WriteManager {
    pub fn new(db: Arc<Database>) -> Self {
        let (tx, rx) = mpsc::channel(1000); // Buffer up to 1000 pending writes
        
        // Spawn background worker
        tokio::spawn(async move {
            Self::background_task(db, rx).await;
        });

        Self { sender: tx }
    }

    async fn background_task(db: Arc<Database>, mut rx: mpsc::Receiver<WriteRequest>) {
        // Connect once
        let conn = match db.connect() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("CRITICAL: Batcher failed to connect to DB: {}", e);
                return;
            }
        };

        let mut buffer = Vec::with_capacity(100);
        let max_batch_size = 100;
        let flush_interval = Duration::from_millis(10); // Wait max 10ms for a batch

        loop {
            // 1. Collect Batch
            let _ = tokio::time::timeout(flush_interval, async {
                while buffer.len() < max_batch_size {
                    if let Some(req) = rx.recv().await {
                        buffer.push(req);
                    } else {
                        // Channel closed
                        return;
                    }
                }
            }).await;

            if buffer.is_empty() { continue; }

            // 2. Execute Batch inside ONE transaction
            let tx_result = conn.execute("BEGIN IMMEDIATE", ()).await;
            if tx_result.is_err() {
                // DB Locked or busy, retry buffer later or fail all
                for req in buffer.drain(..) {
                    match req {
                        WriteRequest::Execute { reply, .. } => { let _ = reply.send(Err("Database Busy".into())); },
                        WriteRequest::InsertReturningId { reply, .. } => { let _ = reply.send(Err("Database Busy".into())); },
                    }
                }
                continue;
            }

            // Execute commands
            for req in buffer.drain(..) {
                match req {
                    WriteRequest::Execute { sql, params, reply } => {
                        match conn.execute(&sql, params).await {
                            Ok(r) => { let _ = reply.send(Ok(r)); },
                            Err(e) => { let _ = reply.send(Err(e.to_string())); }
                        }
                    },
                    WriteRequest::InsertReturningId { sql, params, reply } => {
                        match conn.execute(&sql, params).await {
                            Ok(_) => { 
                                let id = conn.last_insert_rowid();
                                let _ = reply.send(Ok(id)); 
                            },
                            Err(e) => { let _ = reply.send(Err(e.to_string())); }
                        }
                    }
                }
            }

            // Commit
            if let Err(e) = conn.execute("COMMIT", ()).await {
                eprintln!("CRITICAL: Failed to commit batch: {}", e);
            }
        }
    }

    // Public API
    pub async fn execute(&self, sql: String, params: Vec<libsql::Value>) -> Result<u64, String> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(WriteRequest::Execute { sql, params, reply: tx }).await
            .map_err(|_| "Write manager closed".to_string())?;
        rx.await.map_err(|_| "Response dropped".to_string())?
    }

    pub async fn insert(&self, sql: String, params: Vec<libsql::Value>) -> Result<i64, String> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(WriteRequest::InsertReturningId { sql, params, reply: tx }).await
            .map_err(|_| "Write manager closed".to_string())?;
        rx.await.map_err(|_| "Response dropped".to_string())?
    }
}