use tokio::sync::{mpsc, oneshot};
use libsql::Database;
use std::sync::Arc;
use std::time::Duration;

pub enum WriteRequest {
    Execute {
        sql: String,
        params: Vec<libsql::Value>,
        reply: oneshot::Sender<Result<u64, String>>,
    },
    InsertReturningId {
        sql: String,
        params: Vec<libsql::Value>,
        reply: oneshot::Sender<Result<i64, String>>,
    },
    BulkInsert {
        sql: String,
        params_list: Vec<Vec<libsql::Value>>,
        reply: oneshot::Sender<Result<usize, String>>,
    }
}

// [NEW] Helper enum to hold pending replies until commit
enum PendingReply {
    Execute(oneshot::Sender<Result<u64, String>>, u64),
    Insert(oneshot::Sender<Result<i64, String>>, i64),
    Bulk(oneshot::Sender<Result<usize, String>>, usize),
}

#[derive(Clone)]
pub struct WriteManager {
    sender: mpsc::Sender<WriteRequest>,
}

impl WriteManager {
    pub fn new(db: Arc<Database>) -> Self {
        // High-throughput tuning
        // Increase batch size to process more items per transaction under load
        let batch_size = std::env::var("DB_BATCH_SIZE")
            .unwrap_or("2000".to_string())
            .parse::<usize>()
            .unwrap_or(2000);

        let flush_ms = std::env::var("DB_FLUSH_MS")
            .unwrap_or("50".to_string())
            .parse::<u64>()
            .unwrap_or(50);

        // [FIX] Massive channel capacity to prevent backpressure on the API handlers
        let (tx, rx) = mpsc::channel(100_000); 
        
        tokio::spawn(async move {
            Self::background_task(db, rx, batch_size, flush_ms).await;
        });

        Self { sender: tx }
    }

    async fn background_task(
        db: Arc<Database>, 
        mut rx: mpsc::Receiver<WriteRequest>,
        max_batch_size: usize,
        flush_ms: u64
    ) {
        let conn = match db.connect() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("CRITICAL: Batcher failed to connect to DB: {}", e);
                return;
            }
        };

        if let Err(e) = conn.execute_batch("PRAGMA busy_timeout = 10000;").await {
            eprintln!("CRITICAL: Batcher failed to set busy_timeout: {}", e);
        }
        
        let mut buffer = Vec::with_capacity(max_batch_size);
        let flush_interval = Duration::from_millis(flush_ms);

        loop {
            // 1. Fetch first item (Block until we have work)
            let first_req = match rx.recv().await {
                Some(req) => req,
                None => break, // Channel closed
            };
            buffer.push(first_req);

            // 2. Aggressive Drain: Grab everything currently in the channel up to max_batch_size
            // We use a short timeout to allow a small window for more items to arrive
            // if the channel is temporarily empty but active.
            let _ = tokio::time::timeout(flush_interval, async {
                while buffer.len() < max_batch_size {
                    // try_recv is non-blocking. If empty, we wait on recv()
                    match rx.try_recv() {
                        Ok(req) => buffer.push(req),
                        Err(mpsc::error::TryRecvError::Empty) => {
                             // Channel empty, wait a bit for more or timeout
                             if let Some(req) = rx.recv().await {
                                 buffer.push(req);
                             } else {
                                 return; // Channel closed
                             }
                        },
                        Err(_) => return, // Closed
                    }
                }
            }).await;

            if buffer.is_empty() { continue; }

            // 3. Execute Batch in Transaction
            let tx_result = conn.execute("BEGIN IMMEDIATE", ()).await;
            if tx_result.is_err() {
                 // Fail batch to prevent deadlock if database is locked by external process
                 for req in buffer.drain(..) {
                    match req {
                        WriteRequest::Execute { reply, .. } => { let _ = reply.send(Err("Database Locked".into())); },
                        WriteRequest::InsertReturningId { reply, .. } => { let _ = reply.send(Err("Database Locked".into())); },
                        WriteRequest::BulkInsert { reply, .. } => { let _ = reply.send(Err("Database Locked".into())); },
                    }
                }
                continue;
            }

            // [FIX] Store pending replies instead of sending immediately
            let mut pending_replies: Vec<PendingReply> = Vec::with_capacity(buffer.len());

            for req in buffer.drain(..) {
                match req {
                    WriteRequest::Execute { sql, params, reply } => {
                        let res = conn.execute(&sql, params).await;
                        match res {
                            // Defer success reply
                            Ok(r) => pending_replies.push(PendingReply::Execute(reply, r)),
                            // Fail immediately on individual SQL error
                            Err(e) => { let _ = reply.send(Err(e.to_string())); }
                        }
                    },
                    WriteRequest::InsertReturningId { sql, params, reply } => {
                        let res = conn.execute(&sql, params).await;
                        match res {
                            Ok(_) => {
                                let id = conn.last_insert_rowid();
                                // Defer success reply
                                pending_replies.push(PendingReply::Insert(reply, id));
                            }
                            Err(e) => { let _ = reply.send(Err(e.to_string())); }
                        }
                    },
                    WriteRequest::BulkInsert { sql, params_list, reply } => {
                         let mut count = 0;
                         let mut err = None;
                         for p in params_list {
                             if let Err(e) = conn.execute(&sql, p).await {
                                 err = Some(e.to_string());
                                 break;
                             }
                             count += 1;
                         }
                         if let Some(e) = err { 
                             let _ = reply.send(Err(e)); 
                         } else { 
                             // Defer success reply
                             pending_replies.push(PendingReply::Bulk(reply, count)); 
                         }
                    }
                }
            }

            if let Err(e) = conn.execute("COMMIT", ()).await {
                eprintln!("CRITICAL: Failed to commit batch: {}", e);
                // [FIX] If commit fails, fail all pending successes
                for pr in pending_replies {
                    let err_msg = format!("Commit failed: {}", e);
                    match pr {
                        PendingReply::Execute(tx, _) => { let _ = tx.send(Err(err_msg)); },
                        PendingReply::Insert(tx, _) => { let _ = tx.send(Err(err_msg)); },
                        PendingReply::Bulk(tx, _) => { let _ = tx.send(Err(err_msg)); },
                    }
                }
            } else {
                // [FIX] Commit Success: Now it is safe to tell clients "OK"
                for pr in pending_replies {
                    match pr {
                        PendingReply::Execute(tx, res) => { let _ = tx.send(Ok(res)); },
                        PendingReply::Insert(tx, res) => { let _ = tx.send(Ok(res)); },
                        PendingReply::Bulk(tx, res) => { let _ = tx.send(Ok(res)); },
                    }
                }
            }
        }
    }

    pub async fn execute(&self, sql: String, params: Vec<libsql::Value>) -> Result<u64, String> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(WriteRequest::Execute { sql, params, reply: tx }).await.map_err(|_| "Write manager closed".to_string())?;
        rx.await.map_err(|_| "Response dropped".to_string())?
    }

    pub async fn insert(&self, sql: String, params: Vec<libsql::Value>) -> Result<i64, String> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(WriteRequest::InsertReturningId { sql, params, reply: tx }).await.map_err(|_| "Write manager closed".to_string())?;
        rx.await.map_err(|_| "Response dropped".to_string())?
    }
    
    pub async fn bulk_insert(&self, sql: String, params_list: Vec<Vec<libsql::Value>>) -> Result<usize, String> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(WriteRequest::BulkInsert { sql, params_list, reply: tx }).await.map_err(|_| "Write manager closed".to_string())?;
        rx.await.map_err(|_| "Response dropped".to_string())?
    }
}