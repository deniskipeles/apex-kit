// apexkit-api/src/backup.rs
use std::path::Path;
use std::fs;
use std::sync::Arc;
use apexkit_core::{Db, storage::StorageBackend, security::Vault, security::EncryptedValue};
use crate::settings::{StorageConfigDto, BackupConfigDto};
use chrono::Utc;
use tracing::info;

// Since we removed 'zip' crate to save size, we use system 'tar'
// If 'zip' is absolutely required, we'd need to re-add it or use system 'zip' command.
// Using 'tar' is standard on Linux.

pub async fn perform_backup(
    db: Arc<dyn Db>, 
    vault: Arc<Vault>,
    config: BackupConfigDto
) -> Result<(), String> {
    if !config.enabled { return Ok(()); }

    info!("Starting scheduled backup...");

    // 1. Define Paths
    let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let backup_filename = format!("backup_{}.tar.gz", timestamp);
    let temp_dir = format!("storage/tmp/backup_{}", timestamp);
    let archive_path = format!("storage/tmp/{}", backup_filename);

    // 2. Prepare Temp Directory
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    // 3. Copy Database Files (Use VACUUM or simple copy if WAL mode allows)
    // Simple copy is risky on active DBs, but WAL helps. Best practice: "VACUUM INTO".
    // Since we use LibSQL locally, we can try file copy if we accept small risk, 
    // or use the 'sqlite3' CLI if available to vacuum.
    // For simplicity/portability in single-binary, we copy. 
    // Ideally: db.execute("VACUUM INTO 'backup.db'")
    
    // We backup the "storage/system" databases (Root)
    // TODO: Iterate tenants for full backup? For now, Root Backup.
    let files_to_backup = vec![
        "storage/system/core.db",
        "storage/system/data.db",
        "storage/system/system.db",
        "storage/system/vectors.db",
        // Skip logs.db to save space? Configurable.
    ];

    for file in files_to_backup {
        if Path::new(file).exists() {
            let file_name = Path::new(file).file_name().unwrap();
            fs::copy(file, Path::new(&temp_dir).join(file_name))
                .map_err(|e| format!("Failed to copy {}: {}", file, e))?;
        }
    }

    // 4. Create Archive (Tarball)
    // Command: tar -czf archive_path -C temp_dir .
    let output = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&temp_dir)
        .arg(".")
        .output()
        .map_err(|e| format!("Tar execution failed: {}", e))?;

    if !output.status.success() {
        return Err(format!("Tar failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    // 5. Upload to Destination
    match config.destination.as_str() {
        "s3" => {
            // Load S3 Config
            let storage_settings = db.get_config("storage").await.map_err(|e| e.to_string())?;
            if let Some(val) = storage_settings {
                let storage_conf: StorageConfigDto = serde_json::from_value(val).unwrap_or_default();
                if storage_conf.s3.enabled {
                    let secret = if let Some(enc_str) = storage_conf.s3.secret_key {
                        let enc: EncryptedValue = serde_json::from_str(&enc_str).unwrap();
                        vault.decrypt(&enc).unwrap_or_default()
                    } else { String::new() };

                    let s3 = apexkit_core::storage::S3Storage::new_with_creds(
                        &storage_conf.s3.bucket,
                        &storage_conf.s3.region,
                        &storage_conf.s3.endpoint,
                        "",
                        &storage_conf.s3.access_key,
                        &secret
                    ).await;

                    let bytes = fs::read(&archive_path).map_err(|e| e.to_string())?;
                    // Upload to 'backups/' folder in bucket
                    s3.save(&format!("backups/{}", backup_filename), &bytes, "application/gzip").await
                        .map_err(|e| e.to_string())?;
                        
                    info!("Backup uploaded to S3: backups/{}", backup_filename);
                } else {
                    return Err("S3 not enabled for backup destination".into());
                }
            }
        },
        "local" | _ => {
            // Move to storage/backups
            fs::create_dir_all("storage/backups").ok();
            fs::rename(&archive_path, format!("storage/backups/{}", backup_filename))
                .map_err(|e| e.to_string())?;
            info!("Backup saved locally: storage/backups/{}", backup_filename);
        }
    }

    // 6. Cleanup Temp
    let _ = fs::remove_dir_all(&temp_dir);
    if config.destination == "s3" {
        let _ = fs::remove_file(&archive_path); // Delete local tar if uploaded to S3
    }

    // 7. Retention Policy (Pruning)
    // This part is tricky without file metadata parsing or S3 list.
    // For local, we can implement pruning easily.
    if config.destination == "local" {
        prune_local_backups("storage/backups", config.retention as u64)?;
    }

    Ok(())
}

fn prune_local_backups(dir: &str, days: u64) -> Result<(), String> {
    if days == 0 { return Ok(()); }
    
    let cutoff = Utc::now() - chrono::Duration::days(days as i64);
    
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    let dt: chrono::DateTime<Utc> = modified.into();
                    if dt < cutoff {
                        let _ = fs::remove_file(entry.path());
                        info!("Pruned old backup: {:?}", entry.path());
                    }
                }
            }
        }
    }
    Ok(())
}


pub async fn restore_backup(
    file_path: &str, // Path to the uploaded .tar.gz
    is_s3: bool,
    db: Arc<dyn Db>,
    _vault: Arc<Vault>
) -> Result<(), String> {
    info!("Starting restoration from {}", file_path);

    let temp_restore_dir = "storage/tmp/restore_staging";
    
    // 1. Fetch File (if S3)
    let local_archive_path = if is_s3 {
         // Load S3 settings, download file to tmp
         let _storage_settings = db.get_config("storage").await.map_err(|e| e.to_string())?;
         // ... (S3 download logic similar to backup but reverse) ...
         // For brevity, assuming file_path is already local for now (Upload handler handles S3 download if needed)
         file_path.to_string() 
    } else {
        file_path.to_string()
    };

    // 2. Clear Staging
    if Path::new(temp_restore_dir).exists() {
        fs::remove_dir_all(temp_restore_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(temp_restore_dir).map_err(|e| e.to_string())?;

    // 3. Extract Tarball
    let output = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&local_archive_path)
        .arg("-C")
        .arg(temp_restore_dir)
        .output()
        .map_err(|e| format!("Tar extract failed: {}", e))?;

    if !output.status.success() {
        return Err(format!("Tar extract failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    // 4. Validate Backup Integrity
    // Check if critical files exist in staging
    let critical_files = vec!["core.db", "data.db", "system.db"];
    for f in &critical_files {
        if !Path::new(temp_restore_dir).join(f).exists() {
            return Err(format!("Invalid backup: Missing {}", f));
        }
    }

    // 5. SWAP DATABASES (Critical Section)
    // We rename current live DBs to .bak_{timestamp}
    // Then move staged DBs to live location
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let target_dir = "storage/system";

    for f in critical_files {
        let live_path = Path::new(target_dir).join(f);
        let backup_path = Path::new(target_dir).join(format!("{}.bak_{}", f, timestamp));
        let staged_path = Path::new(temp_restore_dir).join(f);

        // Rename Live -> Backup
        if live_path.exists() {
            fs::rename(&live_path, &backup_path).map_err(|e| format!("Failed to backup live DB {}: {}", f, e))?;
        }

        // Move Staged -> Live
        fs::rename(&staged_path, &live_path).map_err(|e| format!("Failed to restore DB {}: {}", f, e))?;
    }
    
    // Also handle vectors.db if present
    let vec_file = "vectors.db";
    let vec_staged = Path::new(temp_restore_dir).join(vec_file);
    if vec_staged.exists() {
         let live_path = Path::new(target_dir).join(vec_file);
         let backup_path = Path::new(target_dir).join(format!("{}.bak_{}", vec_file, timestamp));
         if live_path.exists() { fs::rename(&live_path, &backup_path).ok(); }
         fs::rename(&vec_staged, &live_path).map_err(|e| e.to_string())?;
    }

    // 6. Cleanup
    let _ = fs::remove_dir_all(temp_restore_dir);

    info!("Restoration complete. Restarting system...");
    
    Ok(())
}