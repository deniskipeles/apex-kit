use std::fs;
use std::io::Write;
use std::path::Path;

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub async fn handle_backup(
    root: Option<String>,
    tenants: Option<String>,
    out: Option<String>,
) -> Result<(), String> {
    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let out_file = out.unwrap_or_else(|| format!("storage/backups/backup_{}.tar.gz", ts));
    let tmp_dir = format!("storage/tmp/cli_backup_{}", ts);

    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all("storage/backups").ok();

    let defaults = vec![
        "core.db",
        "core.db-wal",
        "core.db-shm",
        "data.db",
        "data.db-wal",
        "data.db-shm",
        "system.db",
        "system.db-wal",
        "system.db-shm",
        "public",
        "uploads",
    ];
    let optionals = [
        "vectors.db",
        "vectors.db-wal",
        "vectors.db-shm",
        "logs.db",
        "logs.db-wal",
        "logs.db-shm",
        "indexes",
    ];

    let parse_items = |config: &str| -> Vec<String> {
        let mut items = defaults.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        if config == "*" {
            items.extend(optionals.iter().map(|s| s.to_string()));
        } else if config != "default" && !config.is_empty() {
            for item in config.split(',') {
                let i = item.trim();
                if optionals.iter().any(|opt| opt.starts_with(i)) {
                    items.extend(
                        optionals
                            .iter()
                            .filter(|opt| opt.starts_with(i))
                            .map(|s| s.to_string()),
                    );
                }
            }
        }
        items
    };

    let copy_items = |src_base: &str, dest_base: &str, items: &[String]| {
        fs::create_dir_all(dest_base).ok();
        for item in items {
            let src = Path::new(src_base).join(item);
            let dest = Path::new(dest_base).join(item);
            if src.exists() {
                if src.is_dir() {
                    let _ = copy_dir_all(&src, &dest);
                } else {
                    let _ = fs::copy(&src, &dest);
                }
            }
        }
    };

    // 1. Process Root Backup
    if let Some(r_conf) = root {
        println!("⏳ Backing up root environment...");
        let items = parse_items(&r_conf);
        copy_items("storage/system", &format!("{}/system", tmp_dir), &items);
    }

    // 2. Process Tenant Backups
    if let Some(t_str) = tenants {
        if t_str == "*" {
            println!("⏳ Backing up ALL tenants...");
            if let Ok(entries) = fs::read_dir("storage/tenants") {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        let t_id = entry.file_name().to_string_lossy().to_string();
                        println!("  - Backing up tenant: {}", t_id);
                        let items = parse_items("*");
                        copy_items(
                            &format!("storage/tenants/{}", t_id),
                            &format!("{}/tenants/{}", tmp_dir, t_id),
                            &items,
                        );
                    }
                }
            }
        } else {
            let re = regex::Regex::new(r"([\w-]+)(?:\(([^)]+)\))?").unwrap();
            for caps in re.captures_iter(&t_str) {
                let t_id = &caps[1];
                let t_conf = caps.get(2).map(|m| m.as_str()).unwrap_or("default");

                println!("⏳ Backing up tenant: {}...", t_id);
                let items = parse_items(t_conf);
                copy_items(
                    &format!("storage/tenants/{}", t_id),
                    &format!("{}/tenants/{}", tmp_dir, t_id),
                    &items,
                );
            }
        }
    }

    // 3. Compress Archive
    println!("📦 Compressing archive (This may take a moment)...");
    let output = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&out_file)
        .arg("-C")
        .arg(&tmp_dir)
        .arg(".")
        .output()
        .map_err(|e| format!("Tar failed: {}", e))?;

    if !output.status.success() {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(format!(
            "Tar execution failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let _ = fs::remove_dir_all(&tmp_dir);
    println!("✅ Backup successfully created at: {}", out_file);
    Ok(())
}

pub async fn handle_restore(file: String, force_yes: bool) -> Result<(), String> {
    let path = Path::new(&file);
    if !path.exists() {
        return Err(format!("File not found: {}", file));
    }

    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let tmp_dir = format!("storage/tmp/cli_restore_{}", ts);
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;

    println!("📦 Extracting archive...");
    let output = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&file)
        .arg("-C")
        .arg(&tmp_dir)
        .output()
        .map_err(|e| format!("Tar failed: {}", e))?;

    if !output.status.success() {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(format!(
            "Extraction failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mut to_restore = Vec::new();

    let tmp_sys = Path::new(&tmp_dir).join("system");
    if tmp_sys.exists() {
        to_restore.push((
            tmp_sys,
            Path::new("storage/system").to_path_buf(),
            "Root (System Data)".to_string(),
        ));
    }

    let tmp_tenants = Path::new(&tmp_dir).join("tenants");
    if tmp_tenants.exists()
        && let Ok(entries) = fs::read_dir(&tmp_tenants)
    {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let t_id = entry.file_name().to_string_lossy().to_string();
                to_restore.push((
                    entry.path(),
                    Path::new("storage/tenants").join(&t_id),
                    format!("Tenant: {}", t_id),
                ));
            }
        }
    }

    if to_restore.is_empty() {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err("No recognizable ApexKit data found in archive.".to_string());
    }

    println!("\nArchive contains the following scopes:");
    for (_, _, label) in &to_restore {
        println!("  - {}", label);
    }

    let prompt = || -> bool {
        if force_yes {
            return true;
        }
        print!(
            "\n⚠️  WARNING: Proceeding will OVERWRITE live data. Existing folders will be moved to a .bak suffix. Continue? [y/N]: "
        );
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        input.trim().eq_ignore_ascii_case("y")
    };

    if !prompt() {
        println!("Restoration aborted by user.");
        let _ = fs::remove_dir_all(&tmp_dir);
        return Ok(());
    }

    println!("\n🚀 Restoring data...");
    let mut bak_dirs = Vec::new();

    for (src, dest, label) in to_restore {
        if dest.exists() {
            let bak = format!("{}_bak_{}", dest.display(), ts);
            println!("  [Backing up] {} -> {}", label, bak);
            fs::rename(&dest, &bak).map_err(|e| e.to_string())?;
            bak_dirs.push(bak);
        } else if let Some(p) = dest.parent() {
            fs::create_dir_all(p).ok();
        }

        fs::rename(&src, &dest).map_err(|e| e.to_string())?;
        println!("  ✅ Restored {}", label);
    }

    let _ = fs::remove_dir_all(&tmp_dir);

    println!("🧹 Cleaning up temporary backup files...");
    for bak in bak_dirs {
        let _ = fs::remove_dir_all(bak);
    }

    println!("\n🎉 Restoration complete! Restart the ApexKit server to reload databases.");
    Ok(())
}
