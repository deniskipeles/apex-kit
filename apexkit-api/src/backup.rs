use std::path::Path;
use std::fs;
use std::sync::Arc;
use apexkit_core::{Db, security::Vault, security::EncryptedValue, realtime::EventScope};
use crate::settings::{StorageConfigDto, BackupConfigDto};
use chrono::Utc;
use tracing::info;
use apexkit_core::storage::StorageBackend;

pub async fn perform_backup(
    db: Arc<dyn Db>, 
    vault: Arc<Vault>,
    config: BackupConfigDto,
    scope: EventScope 
) -> Result<(), String> {
    if !config.enabled { return Ok(()); }

    info!("Starting backup for scope {:?}...", scope);

    let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let backup_filename = format!("backup_{}.tar.gz", timestamp);
    
    // 1. Resolve Paths
    let (source_dir, backup_dir_local, s3_prefix) = match &scope {
        EventScope::Root => (
            "storage/system".to_string(), 
            "storage/backups".to_string(), 
            "backups".to_string()
        ),
        EventScope::Tenant(id) => (
            format!("storage/tenants/{}", id),
            format!("storage/tenants/{}/backups", id),
            format!("tenants/{}/backups", id)
        ),
        EventScope::Sandbox(id) => (
            format!("storage/sandboxes/session_{}", id),
            format!("storage/sandboxes/session_{}/backups", id),
            format!("sandboxes/{}/backups", id)
        ),
        _ => return Err("Unsupported scope for backup".into()),
    };

    let temp_dir = format!("{}/backup_staging_{}", source_dir, timestamp); // Stage inside tenant dir to ensure same vol
    let archive_path = format!("{}/{}", source_dir, backup_filename); // Temp location

    // 2. Prepare Temp Directory
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    // 3. Copy DB Files
    let files_to_backup = vec!["core.db", "data.db", "system.db", "vectors.db"];

    for filename in files_to_backup {
        let src = Path::new(&source_dir).join(filename);
        if src.exists() {
            fs::copy(&src, Path::new(&temp_dir).join(filename))
                .map_err(|e| format!("Failed to copy {}: {}", filename, e))?;
        }
    }

    // 4. Create Archive
    let output = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&temp_dir)
        .arg(".")
        .output()
        .map_err(|e| format!("Tar execution failed: {}", e))?;

    if !output.status.success() {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!("Tar failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    // 5. Upload / Move
    match config.destination.as_str() {
        "s3" => {
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
                    // Upload to scoped prefix
                    s3.save(&format!("{}/{}", s3_prefix, backup_filename), &bytes, "application/gzip").await
                        .map_err(|e| e.to_string())?;
                        
                    info!("Backup uploaded to S3: {}/{}", s3_prefix, backup_filename);
                } else {
                    return Err("S3 not enabled".into());
                }
            }
        },
        "local" | _ => {
            fs::create_dir_all(&backup_dir_local).ok();
            fs::rename(&archive_path, format!("{}/{}", backup_dir_local, backup_filename))
                .map_err(|e| e.to_string())?;
            info!("Backup saved locally: {}/{}", backup_dir_local, backup_filename);
            
            prune_local_backups(&backup_dir_local, config.retention as u64)?;
        }
    }

    // 6. Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
    if config.destination == "s3" {
        let _ = fs::remove_file(&archive_path);
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
    file_path: &str, 
    is_s3: bool,
    db: Arc<dyn Db>,
    vault: Arc<Vault>,
    scope: EventScope
) -> Result<(), String> {
    info!("Starting restoration for scope {:?} from {}", scope, file_path);

    // Resolve Target Directory based on Scope
    let target_dir = match &scope {
        EventScope::Root => "storage/system".to_string(),
        EventScope::Tenant(id) => format!("storage/tenants/{}", id),
        EventScope::Sandbox(id) => format!("storage/sandboxes/session_{}", id),
        _ => return Err("Invalid scope".into()),
    };

    let temp_restore_dir = format!("{}/restore_staging", target_dir);
    
    // 1. Fetch File (S3 or Local)
    let local_archive_path = if is_s3 {
        // Fetch from S3 to a temp location
        let storage_settings = db.get_config("storage").await.map_err(|e| e.to_string())?;
        
        let s3_client = if let Some(val) = storage_settings {
            let storage_conf: StorageConfigDto = serde_json::from_value(val).unwrap_or_default();
            if storage_conf.s3.enabled {
                let secret = if let Some(enc_str) = storage_conf.s3.secret_key {
                     let enc: EncryptedValue = serde_json::from_str(&enc_str).unwrap();
                     vault.decrypt(&enc).unwrap_or_default()
                } else { String::new() };

                apexkit_core::storage::S3Storage::new_with_creds(
                    &storage_conf.s3.bucket,
                    &storage_conf.s3.region,
                    &storage_conf.s3.endpoint,
                    "",
                    &storage_conf.s3.access_key,
                    &secret
                ).await
            } else {
                return Err("S3 not enabled in config, cannot restore from S3".into());
            }
        } else {
            return Err("Storage config missing".into());
        };

        // Determine S3 key based on scope and filename
        // file_path passed here is just the filename "backup_xyz.tar.gz" if s3 is true
        let s3_prefix = match &scope {
             EventScope::Root => "backups".to_string(),
             EventScope::Tenant(id) => format!("tenants/{}/backups", id),
             EventScope::Sandbox(id) => format!("sandboxes/{}/backups", id),
             _ => return Err("Invalid scope".into()),
        };
        
        let s3_key = format!("{}/{}", s3_prefix, file_path);
        info!("Downloading backup from S3: {}", s3_key);

        let data = s3_client.get(&s3_key).await
             .map_err(|e: Box<dyn std::error::Error + Send + Sync>| e.to_string())?;
             
        let temp_download_path = format!("{}/restore_download_{}.tar.gz", target_dir, uuid::Uuid::new_v4());
        fs::write(&temp_download_path, data).map_err(|e| e.to_string())?;
        
        temp_download_path
    } else {
        // Local path passed directly
        file_path.to_string()
    };

    // 2. Clear Staging
    if Path::new(&temp_restore_dir).exists() {
        fs::remove_dir_all(&temp_restore_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&temp_restore_dir).map_err(|e| e.to_string())?;

    // 3. Extract Tarball
    let output = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&local_archive_path)
        .arg("-C")
        .arg(&temp_restore_dir)
        .output()
        .map_err(|e| format!("Tar extract failed: {}", e))?;

    if !output.status.success() {
        return Err(format!("Tar extract failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    // 4. Validate
    let critical_files = vec!["core.db", "data.db", "system.db"];
    for f in &critical_files {
        if !Path::new(&temp_restore_dir).join(f).exists() {
            return Err(format!("Invalid backup: Missing {}", f));
        }
    }

    // 5. Swap
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();

    for f in critical_files {
        let live_path = Path::new(&target_dir).join(f);
        let backup_path = Path::new(&target_dir).join(format!("{}.bak_{}", f, timestamp));
        let staged_path = Path::new(&temp_restore_dir).join(f);

        if live_path.exists() {
            fs::rename(&live_path, &backup_path).map_err(|e| format!("Failed to backup live DB {}: {}", f, e))?;
        }
        fs::rename(&staged_path, &live_path).map_err(|e| format!("Failed to restore DB {}: {}", f, e))?;
    }
    
    let vec_file = "vectors.db";
    let vec_staged = Path::new(&temp_restore_dir).join(vec_file);
    if vec_staged.exists() {
         let live_path = Path::new(&target_dir).join(vec_file);
         let backup_path = Path::new(&target_dir).join(format!("{}.bak_{}", vec_file, timestamp));
         if live_path.exists() { fs::rename(&live_path, &backup_path).ok(); }
         fs::rename(&vec_staged, &live_path).map_err(|e| e.to_string())?;
    }

    let _ = fs::remove_dir_all(&temp_restore_dir);
    
    // Cleanup downloaded S3 file if applicable
    if is_s3 {
        let _ = fs::remove_file(&local_archive_path);
    }

    info!("Restoration complete.");
    Ok(())
}