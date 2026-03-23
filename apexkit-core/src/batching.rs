use tokio::sync::{mpsc, oneshot};
use rusqlite::Connection;
use std::sync::Arc;
use std::time::Duration;

pub enum WriteRequest {
    Execute {
        sql: String,
        params: Vec<rusqlite::types::Value>,
        reply: oneshot::Sender<Result<u64, String>>,
    },
    InsertReturningId {
        sql: String,
        params: Vec<rusqlite::types::Value>,
        reply: oneshot::Sender<Result<i64, String>>,
    },
    BulkInsert {
        sql: String,
        params_list: Vec<Vec<rusqlite::types::Value>>,
        reply: oneshot::Sender<Result<usize, String>>,
    }
}

enum PendingReply {
    Execute(oneshot::Sender<Result<u64, String>>, u64),
    Insert(oneshot::Sender<Result<i64, String>>, i64),
    Bulk(oneshot::Sender<Result<usize, String>>, usize),
}

#[async_trait::async_trait]
pub trait WriteForwarder: Send + Sync {
    async fn forward_write(&self, db_path: String, sql: String, params: Vec<rusqlite::types::Value>) -> Result<(i64, u64), String>;
}

#[derive(Clone)]
pub struct WriteManager {
    sender: mpsc::Sender<WriteRequest>,
}

impl WriteManager {
    pub fn new(db_path: String, db: Arc<tokio::sync::Mutex<Connection>>, forwarder: Option<Arc<dyn WriteForwarder>>) -> Self {
        let batch_size = std::env::var("DB_BATCH_SIZE")
            .unwrap_or("2000".to_string())
            .parse::<usize>()
            .unwrap_or(2000);

        let flush_ms = std::env::var("DB_FLUSH_MS")
            .unwrap_or("50".to_string())
            .parse::<u64>()
            .unwrap_or(50);

        let (tx, rx) = mpsc::channel(100_000); 
        
        tokio::spawn(async move {
            Self::background_task(db_path, db, rx, batch_size, flush_ms, forwarder).await;
        });

        Self { sender: tx }
    }

    async fn background_task(
        db_path: String,
        db: Arc<tokio::sync::Mutex<Connection>>, 
        mut rx: mpsc::Receiver<WriteRequest>,
        max_batch_size: usize,
        flush_ms: u64,
        forwarder: Option<Arc<dyn WriteForwarder>>
    ) {
        let mut buffer = Vec::with_capacity(max_batch_size);
        let flush_interval = Duration::from_millis(flush_ms);

        loop {
            let first_req = match rx.recv().await {
                Some(req) => req,
                None => break, 
            };
            buffer.push(first_req);

            let _ = tokio::time::timeout(flush_interval, async {
                while buffer.len() < max_batch_size {
                    match rx.try_recv() {
                        Ok(req) => buffer.push(req),
                        Err(mpsc::error::TryRecvError::Empty) => {
                             if let Some(req) = rx.recv().await {
                                 buffer.push(req);
                             } else {
                                 return; 
                             }
                        },
                        Err(_) => return, 
                    }
                }
            }).await;

            if buffer.is_empty() { continue; }

            // REPLICA MODE: Forward over gRPC
            if let Some(fwd) = &forwarder {
                for req in buffer.drain(..) {
                    match req {
                        WriteRequest::Execute { sql, params, reply } => {
                            match fwd.forward_write(db_path.clone(), sql, params).await {
                                Ok((_, affected)) => { let _ = reply.send(Ok(affected)); },
                                Err(e) => { let _ = reply.send(Err(e)); }
                            }
                        },
                        WriteRequest::InsertReturningId { sql, params, reply } => {
                            match fwd.forward_write(db_path.clone(), sql, params).await {
                                Ok((id, _)) => { let _ = reply.send(Ok(id)); },
                                Err(e) => { let _ = reply.send(Err(e)); }
                            }
                        },
                        WriteRequest::BulkInsert { sql, params_list, reply } => {
                            let mut count = 0;
                            let mut err = None;
                            for p in params_list {
                                match fwd.forward_write(db_path.clone(), sql.clone(), p).await {
                                    Ok(_) => count += 1,
                                    Err(e) => { err = Some(e); break; }
                                }
                            }
                            if let Some(e) = err { let _ = reply.send(Err(e)); }
                            else { let _ = reply.send(Ok(count)); }
                        }
                    }
                }
                continue;
            }

            // MASTER MODE: Execute locally via SQLite
            let conn = db.lock().await;

            let tx_result = conn.execute_batch("BEGIN IMMEDIATE");
            if tx_result.is_err() {
                 for req in buffer.drain(..) {
                    match req {
                        WriteRequest::Execute { reply, .. } => { let _ = reply.send(Err("Database Locked".into())); },
                        WriteRequest::InsertReturningId { reply, .. } => { let _ = reply.send(Err("Database Locked".into())); },
                        WriteRequest::BulkInsert { reply, .. } => { let _ = reply.send(Err("Database Locked".into())); },
                    }
                }
                continue;
            }

            let mut pending_replies: Vec<PendingReply> = Vec::with_capacity(buffer.len());

            for req in buffer.drain(..) {
                match req {
                    WriteRequest::Execute { sql, params, reply } => {
                        match conn.execute(&sql, rusqlite::params_from_iter(params)) {
                            Ok(r) => pending_replies.push(PendingReply::Execute(reply, r as u64)),
                            Err(e) => { let _ = reply.send(Err(e.to_string())); }
                        }
                    },
                    WriteRequest::InsertReturningId { sql, params, reply } => {
                        match conn.execute(&sql, rusqlite::params_from_iter(params)) {
                            Ok(_) => {
                                let id = conn.last_insert_rowid();
                                pending_replies.push(PendingReply::Insert(reply, id));
                            }
                            Err(e) => { let _ = reply.send(Err(e.to_string())); }
                        }
                    },
                    WriteRequest::BulkInsert { sql, params_list, reply } => {
                         let mut count = 0;
                         let mut err = None;
                         if let Ok(mut stmt) = conn.prepare(&sql) {
                             for p in params_list {
                                 if let Err(e) = stmt.execute(rusqlite::params_from_iter(p)) {
                                     err = Some(e.to_string());
                                     break;
                                 }
                                 count += 1;
                             }
                         } else {
                             err = Some("Prepare failed".to_string());
                         }
                         if let Some(e) = err { 
                             let _ = reply.send(Err(e)); 
                         } else { 
                             pending_replies.push(PendingReply::Bulk(reply, count)); 
                         }
                    }
                }
            }

            if let Err(e) = conn.execute_batch("COMMIT") {
                eprintln!("CRITICAL: Failed to commit batch: {}", e);
                for pr in pending_replies {
                    let err_msg = format!("Commit failed: {}", e);
                    match pr {
                        PendingReply::Execute(tx, _) => { let _ = tx.send(Err(err_msg)); },
                        PendingReply::Insert(tx, _) => { let _ = tx.send(Err(err_msg)); },
                        PendingReply::Bulk(tx, _) => { let _ = tx.send(Err(err_msg)); },
                    }
                }
            } else {
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

    pub async fn execute(&self, sql: String, params: Vec<rusqlite::types::Value>) -> Result<u64, String> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(WriteRequest::Execute { sql, params, reply: tx }).await.map_err(|_| "Write manager closed".to_string())?;
        rx.await.map_err(|_| "Response dropped".to_string())?
    }

    pub async fn insert(&self, sql: String, params: Vec<rusqlite::types::Value>) -> Result<i64, String> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(WriteRequest::InsertReturningId { sql, params, reply: tx }).await.map_err(|_| "Write manager closed".to_string())?;
        rx.await.map_err(|_| "Response dropped".to_string())?
    }
    
    pub async fn bulk_insert(&self, sql: String, params_list: Vec<Vec<rusqlite::types::Value>>) -> Result<usize, String> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(WriteRequest::BulkInsert { sql, params_list, reply: tx }).await.map_err(|_| "Write manager closed".to_string())?;
        rx.await.map_err(|_| "Response dropped".to_string())?
    }
}