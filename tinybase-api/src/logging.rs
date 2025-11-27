use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};
use std::path::Path;

// Function to initialize logging. 
pub fn init_logging(log_dir: &str, _retention_days: usize) -> tracing_appender::non_blocking::WorkerGuard {
    // 1. File Appender (Daily Rotation)
    // We prefix _retention_days because tracing_appender handles rotation, 
    // but cleanup is handled separately by the cron job.
    
    let file_appender = RollingFileAppender::new(
        Rotation::DAILY,
        log_dir,
        "tinybase.log",
    );

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // 2. Console Layer (For Devs)
    let console_layer = fmt::layer()
        .with_target(false)
        .with_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()));

    // 3. File Layer (JSON format for easy parsing later)
    // Requires the "json" feature in Cargo.toml
    let file_layer = fmt::layer()
        .with_target(true)
        .with_writer(non_blocking)
        .json() 
        .with_filter(EnvFilter::new("info")); 

    // 4. Register
    tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .init();

    guard 
}

// Helper to clean up old logs (Call this from Cron Scheduler)
pub fn cleanup_logs(log_dir: &str, retention_days: u64) {
    let path = Path::new(log_dir);
    if let Ok(entries) = std::fs::read_dir(path) {
        // Calculate cutoff time safely
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(retention_days * 24 * 60 * 60))
            .unwrap_or_else(std::time::SystemTime::now);

        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(created) = metadata.created() {
                    if created < cutoff {
                        let _ = std::fs::remove_file(entry.path());
                        println!("Deleted old log: {:?}", entry.path());
                    }
                }
            }
        }
    }
}