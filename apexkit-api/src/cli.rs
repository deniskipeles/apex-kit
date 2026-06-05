use crate::AppState;
use apexkit_core::realtime::EventScope;
use apexkit_core::security::EncryptedValue;
use apexkit_core::{auth, security::MasterKey};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about = "ApexKit CLI Manager", long_about = None)]
pub struct Cli {
    /// Port to run the server on (if starting server)
    #[arg(short, long, default_value_t = 5000)]
    pub port: u16,

    /// Subcommands for system management (skips starting HTTP server if used)
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// User Management
    #[command(subcommand)]
    User(UserCmd),

    /// System Configuration (Secrets & Settings)
    #[command(subcommand)]
    Config(ConfigCmd),

    /// Data Operations (Import/Export/Maintenance)
    #[command(subcommand)]
    Data(DataCmd),

    /// Execute a server-side script directly
    RunScript {
        /// The name/slug of the script to execute
        name: String,
        /// JSON input payload string (e.g. '{"foo":"bar"}')
        #[arg(long)]
        input: Option<String>,
    },

    /// Create a full or partial backup of the system
    Backup {
        /// Backup root data. Optional items: vectors.db, logs.db, indexes, or *
        #[arg(long, default_missing_value = "default", num_args = 0..=1)]
        root: Option<String>,

        /// Backup specific tenants. Format: app-0,app-1(*),app-2(vectors.db)
        #[arg(long)]
        tenants: Option<String>,

        /// Custom output file path (e.g., my_backup.tar.gz)
        #[arg(short, long)]
        out: Option<String>,
    },

    /// Restore a backup from an archive
    Restore {
        /// Path to the archive (.tar.gz)
        file: String,

        /// Bypass the interactive confirmation prompt
        #[arg(short, long, action)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum UserCmd {
    /// Create a new user (or Admin)
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: Option<String>,
        /// Role: 'admin' or 'user'
        #[arg(long, default_value = "user")]
        role: String,
    },
    /// Reset a user's password manually
    ResetPassword {
        email: String,
        new_password: Option<String>,
    },
    /// List all users
    List,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Set a system secret (Encrypted in DB)
    Set { key: String, value: String },
    /// Get a system secret (Decrypted)
    Get { key: String },
}

#[derive(Subcommand, Debug)]
pub enum DataCmd {
    /// Force re-index of Tantivy search for a collection
    Reindex {
        /// Collection ID
        id: i64,
    },
    /// Export a collection to JSON (printed to stdout)
    Export {
        /// Collection ID
        id: i64,
        /// Pretty print JSON
        #[arg(long, action)]
        pretty: bool,
    },
    /// Clear all logs from the database
    PruneLogs,
}

/// Helper function to execute CLI commands
pub async fn execute_cli_command(state: AppState, command: Commands) -> Result<(), String> {
    match command {
        // --- USER COMMANDS ---
        Commands::User(cmd) => match cmd {
            UserCmd::Create {
                email,
                password,
                role,
            } => handle_create_user(state, email, password, role).await,
            UserCmd::ResetPassword {
                email,
                new_password,
            } => handle_reset_password(state, email, new_password).await,
            UserCmd::List => {
                let users = state
                    .db
                    .list_users(None, 1000, 0)
                    .await
                    .map_err(|e| e.to_string())?;

                println!("{:<5} {:<30} {:<10}", "ID", "EMAIL", "ROLE");
                println!("{:-<50}", "");
                for u in users {
                    println!("{:<5} {:<30} {:<10}", u.id, u.email, u.role);
                }
                Ok(())
            }
        },

        Commands::Config(cmd) => match cmd {
            ConfigCmd::Set { key, value } => {
                let encrypted = state.vault.encrypt(&value).map_err(|e| e.to_string())?;
                let json_val = serde_json::to_value(&encrypted).unwrap();

                state
                    .db
                    .set_config(&key, &json_val, true)
                    .await
                    .map_err(|e| e.to_string())?;
                println!("✅ Config '{}' set successfully (Encrypted).", key);
                Ok(())
            }
            ConfigCmd::Get { key } => {
                if let Some(json_val) =
                    state.db.get_config(&key).await.map_err(|e| e.to_string())?
                {
                    if let Ok(enc) = serde_json::from_value::<EncryptedValue>(json_val) {
                        let val = state.vault.decrypt(&enc).map_err(|e| e.to_string())?;
                        println!("{}", val);
                    } else {
                        println!("(Value is not encrypted or invalid format)");
                    }
                    Ok(())
                } else {
                    Err(format!("Config '{}' not found.", key))
                }
            }
        },

        // --- DATA COMMANDS ---
        Commands::Data(cmd) => match cmd {
            DataCmd::Reindex { id } => {
                println!("Re-indexing collection {}...", id);
                state
                    .db
                    .reindex_collection(id)
                    .await
                    .map_err(|e| e.to_string())?;
                println!("✅ Re-indexing complete.");
                Ok(())
            }
            DataCmd::Export { id, pretty } => {
                let opts = apexkit_core::query::QueryOptions {
                    limit: Some(100000),
                    ..Default::default()
                };
                let result = state
                    .db
                    .list_records(id, opts)
                    .await
                    .map_err(|e| e.to_string())?;

                let output = if pretty {
                    serde_json::to_string_pretty(&result.items).unwrap()
                } else {
                    serde_json::to_string(&result.items).unwrap()
                };

                std::io::stdout().write_all(output.as_bytes()).unwrap();
                Ok(())
            }
            DataCmd::PruneLogs => {
                crate::logging::cleanup_logs("logs", 0);
                println!("✅ Log files pruned.");
                Ok(())
            }
        },

        // --- SCRIPTING ---
        Commands::RunScript { name, input } => {
            println!("🚀 Running Script: {}", name);

            let payload = if let Some(s) = input {
                serde_json::from_str(&s).map_err(|e| format!("Invalid JSON input: {}", e))?
            } else {
                json!({})
            };

            let script = state
                .db
                .get_script_by_name(&name)
                .await
                .map_err(|e| e.to_string())?
                .ok_or(format!("Script '{}' not found.", name))?;

            let context = Arc::new(crate::ScopedScriptContext {
                state: state.clone(),
                scope: EventScope::Root,
            });

            let result = state
                .script_engine
                .run_script(&script.code, payload, context, None, None)
                .await
                .map_err(|e| format!("Script Error: {}", e))?;

            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }

        // --- BACKUP & RESTORE ---
        Commands::Backup { root, tenants, out } => handle_backup(root, tenants, out).await,
        Commands::Restore { file, yes } => handle_restore(file, yes).await,
    }
}

// ---------------------------------------------------------
// USER HELPERS
// ---------------------------------------------------------

async fn handle_create_user(
    state: AppState,
    email: String,
    password: Option<String>,
    role: String,
) -> Result<(), String> {
    if !email.contains('@') {
        return Err("Invalid email format.".into());
    }

    if state
        .db
        .get_user_by_email(&email)
        .await
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err(format!("User '{}' already exists.", email));
    }

    let raw_password = password.unwrap_or_else(|| {
        let p = MasterKey::generate_random_password();
        println!("⚠️  No password provided. Generated: {}", p);
        p
    });

    let hash = auth::hash_password(&raw_password).map_err(|e| e.to_string())?;
    let user = state
        .db
        .create_user(&email, &hash, &role, None)
        .await
        .map_err(|e| e.to_string())?;

    println!("✅ User created: {} (ID: {})", user.email, user.id);
    Ok(())
}

async fn handle_reset_password(
    state: AppState,
    email: String,
    new_password: Option<String>,
) -> Result<(), String> {
    let _user = state
        .db
        .get_user_by_email(&email)
        .await
        .map_err(|e| e.to_string())?
        .ok_or(format!("User '{}' not found", email))?;

    let raw_password = new_password.unwrap_or_else(|| {
        let p = MasterKey::generate_random_password();
        println!("⚠️  Generated new password: {}", p);
        p
    });

    let hash = auth::hash_password(&raw_password).map_err(|e| e.to_string())?;

    println!("❌ DB Trait update required to change password directly via CLI without SQL access.");
    println!("   Hash to set manually in DB: {}", hash);

    Ok(())
}

// ---------------------------------------------------------
// BACKUP & RESTORE LOGIC
// ---------------------------------------------------------

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

    // [FIX] Included WAL and SHM files to ensure no data loss during live backups
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
    let optionals = vec![
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
                // Match base name for optionals (e.g. "vectors.db" includes wal and shm)
                if optionals.iter().any(|opt| opt.starts_with(i)) {
                    // Push all related files
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
            // Wildcard: Backup ALL tenants
            println!("⏳ Backing up ALL tenants...");
            if let Ok(entries) = fs::read_dir("storage/tenants") {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        let t_id = entry.file_name().to_string_lossy().to_string();
                        println!("  - Backing up tenant: {}", t_id);
                        let items = parse_items("*"); // Full backup for wildcard
                        copy_items(
                            &format!("storage/tenants/{}", t_id),
                            &format!("{}/tenants/{}", tmp_dir, t_id),
                            &items,
                        );
                    }
                }
            }
        } else {
            // Specific tenants: name OR name(args)
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

    // Cleanup Staging
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

    let mut to_restore = Vec::new(); // (Extracted Source, Live Destination, Human Label)

    // Discover Root
    let tmp_sys = Path::new(&tmp_dir).join("system");
    if tmp_sys.exists() {
        to_restore.push((
            tmp_sys,
            Path::new("storage/system").to_path_buf(),
            "Root (System Data)".to_string(),
        ));
    }

    // Discover Tenants
    let tmp_tenants = Path::new(&tmp_dir).join("tenants");
    if tmp_tenants.exists() {
        if let Ok(entries) = fs::read_dir(&tmp_tenants) {
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
    }

    if to_restore.is_empty() {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err("No recognizable ApexKit data found in archive. Ensure it contains a 'system' or 'tenants' directory.".to_string());
    }

    println!("\nArchive contains the following scopes:");
    for (_, _, label) in &to_restore {
        println!("  - {}", label);
    }

    // Interactive Prompt
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
        } else {
            if let Some(p) = dest.parent() {
                fs::create_dir_all(p).ok();
            }
        }

        fs::rename(&src, &dest).map_err(|e| e.to_string())?;
        println!("  ✅ Restored {}", label);
    }

    // Clean up temporary extraction directory
    let _ = fs::remove_dir_all(&tmp_dir);

    // [FIX] Clean up .bak files to avoid eating up server disk space over multiple deploys
    println!("🧹 Cleaning up temporary backup files...");
    for bak in bak_dirs {
        let _ = fs::remove_dir_all(bak);
    }

    println!(
        "\n🎉 Restoration complete! Please restart the ApexKit server to reload databases into memory."
    );
    Ok(())
}
