// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/logging.rs ===========================
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{Event, Subscriber};
use tracing_subscriber::{
    layer::{Context, SubscriberExt},
    util::SubscriberInitExt,
    EnvFilter, Layer,
};
use tinybase_core::Db;
use std::path::Path;
use std::fs;
use chrono::{Datelike, Utc};

// --- 1. Log Message Struct ---
#[derive(Debug)]
pub struct SystemLogEntry {
    pub level: String,
    pub target: String,
    pub message: String,
}

// --- 2. Tracing Layer (Captures logs and sends to channel) ---
pub struct DbLoggerLayer {
    sender: mpsc::UnboundedSender<SystemLogEntry>,
}

impl<S> Layer<S> for DbLoggerLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let target = event.metadata().target();

        // --- CRITICAL FIX: PREVENT FEEDBACK LOOP ---
        // Do not log database internal operations to the database.
        // Also filter out other noisy low-level crates.
        if target.starts_with("libsql") 
            || target.starts_with("rusqlite") 
            || target.starts_with("hyper") 
            || target.starts_with("h2") 
            || target.starts_with("tower") 
        {
            return;
        }

        let level = event.metadata().level().to_string();
        
        // Visitor to extract the message string
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        
        let entry = SystemLogEntry {
            level,
            target: target.to_string(),
            message: visitor.message,
        };

        // Fire and forget
        let _ = self.sender.send(entry);
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
         if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

// --- 3. Background Worker (Writes to DB) ---
pub async fn start_log_worker(mut rx: mpsc::UnboundedReceiver<SystemLogEntry>, db: Arc<dyn Db>) {
    let mut buffer = Vec::new();
    let batch_size = 50;
    let flush_interval = std::time::Duration::from_secs(2);
    
    loop {
        let timeout = tokio::time::sleep(flush_interval);
        tokio::pin!(timeout);

        tokio::select! {
            Some(entry) = rx.recv() => {
                buffer.push(entry);
                if buffer.len() >= batch_size {
                    flush_logs(&db, &mut buffer).await;
                }
            }
            _ = &mut timeout => {
                if !buffer.is_empty() {
                    flush_logs(&db, &mut buffer).await;
                }
            }
        }
    }
}

async fn flush_logs(db: &Arc<dyn Db>, buffer: &mut Vec<SystemLogEntry>) {
    for entry in buffer.drain(..) {
        let _ = db.log_system_event(&entry.level, &entry.target, &entry.message).await;
    }
}

// --- 4. Rotation Logic ---
pub fn rotate_logs_on_startup(db_path: &str, archive_dir: &str) {
    let path = Path::new(db_path);
    if !path.exists() { return; }

    let metadata = fs::metadata(path).unwrap();
    if let Ok(created) = metadata.created() {
        let created_datetime: chrono::DateTime<Utc> = created.into();
        let now = Utc::now();

        if created_datetime.month() != now.month() || created_datetime.year() != now.year() {
            println!("[Logs] Detected new month. Rotating logs database...");
            let _ = fs::create_dir_all(archive_dir);
            let archive_name = format!("{}/logs_{:04}_{:02}.db", archive_dir, created_datetime.year(), created_datetime.month());
            
            match fs::rename(path, &archive_name) {
                Ok(_) => println!("[Logs] Archived to {}", archive_name),
                Err(e) => eprintln!("[Logs] Failed to rotate logs: {}", e),
            }
        }
    }
}

pub fn cleanup_logs(log_dir: &str, retention_days: u64) {
    let path = Path::new(log_dir);
    if !path.exists() { return; }

    let cutoff = Utc::now()
        .checked_sub_signed(chrono::Duration::days(retention_days as i64))
        .unwrap_or_else(Utc::now);

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let modified_dt: chrono::DateTime<Utc> = modified.into();
                        if modified_dt < cutoff {
                            let _ = fs::remove_file(&path);
                            println!("[Logs] Pruned old archive: {:?}", path);
                        }
                    }
                }
            }
        }
    }
}

// --- 5. Initialization ---
pub fn init_logging_system() -> mpsc::UnboundedReceiver<SystemLogEntry> {
    let (tx, rx) = mpsc::unbounded_channel();

    let db_layer = DbLoggerLayer { sender: tx };
    
    // Configure default filter to be noisy for app, quiet for libs
    // info by default, but libsql/hyper/tower only show warnings/errors
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,libsql=warn,hyper=warn,tower=warn".into());

    let console_layer = tracing_subscriber::fmt::layer()
        .with_target(true);

    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(db_layer)
        .init();
        
    rx
}