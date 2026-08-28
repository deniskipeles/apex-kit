use ring::digest::{SHA256, digest};
use std::fs;
use std::path::{Path, PathBuf};
use wasmtime::*;

fn get_wasm_cache_dir() -> PathBuf {
    // 1. Check custom override or mounted storage
    if let Ok(custom) = std::env::var("APEXKIT_WASM_CACHE_DIR") {
        let dir = PathBuf::from(custom);
        let _ = fs::create_dir_all(&dir);
        return dir;
    }
    if let Ok(base_storage) = std::env::var("APEXKIT_MOUNTED_FILE_STORAGE") {
        let dir = PathBuf::from(base_storage).join(".cache").join("wasm");
        let _ = fs::create_dir_all(&dir);
        return dir;
    }

    // 2. Default to CWD (Current Working Directory)
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let dir = base.join(".cache").join("wasm");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn create_readable_symlink(cache_dir: &Path, readable_name: &str, target_filename: &str) {
    let clean_name = readable_name
        .trim_start_matches('/')
        .trim_start_matches("./");
    if clean_name.is_empty() {
        return;
    }

    let link_path = cache_dir.join(clean_name);
    if let Some(parent) = link_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let target_path = cache_dir.join(target_filename);

    let _ = fs::remove_file(&link_path);

    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(&target_path, &link_path);
    }
    #[cfg(windows)]
    {
        let _ = std::os::windows::fs::symlink_file(&target_path, &link_path);
    }
}

pub async fn execute(
    get_url: Option<String>,
    local_file: Option<String>,
    custom_name: Option<String>,
    list: bool,
) -> Result<(), String> {
    if list {
        return handle_list().await;
    }

    if let Some(path) = local_file {
        return handle_file(path, custom_name).await;
    }

    if let Some(url) = get_url {
        return handle_get(url, custom_name).await;
    }

    println!("⚠️ No action specified for WASM tool. Use --file <PATH>, --get <URL>, or --list.");
    Ok(())
}

async fn handle_file(file_path: String, custom_name: Option<String>) -> Result<(), String> {
    println!("🔍 Reading local WASM binary from: {}...", file_path);

    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let bytes = fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    if bytes.is_empty() {
        return Err("WASM file is empty (0 bytes).".to_string());
    }

    let readable_name = custom_name.unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("module.wasm")
            .to_string()
    });

    process_and_cache_wasm(bytes, readable_name).await
}

async fn handle_get(url: String, custom_name: Option<String>) -> Result<(), String> {
    println!("⏳ Fetching WASM binary from: {}...", url);

    let client = reqwest::Client::builder()
        .user_agent("ApexKit-CLI/1.0")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error fetching URL: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to download WASM file. Server returned HTTP {}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?
        .to_vec();

    if bytes.is_empty() {
        return Err("Downloaded file is empty (0 bytes).".to_string());
    }

    let readable_name = custom_name.unwrap_or_else(|| {
        url.split('/')
            .last()
            .unwrap_or("module.wasm")
            .split('?')
            .next()
            .unwrap_or("module.wasm")
            .to_string()
    });

    process_and_cache_wasm(bytes, readable_name).await
}

async fn process_and_cache_wasm(bytes: Vec<u8>, readable_name: String) -> Result<(), String> {
    println!("🔍 Sanitizing and validating WebAssembly module...");

    let mut config = Config::new();
    config.consume_fuel(true);

    let engine = Engine::new(&config).map_err(|e| format!("Wasmtime Engine init failed: {}", e))?;
    let module = Module::from_binary(&engine, &bytes).map_err(|e| {
        format!(
            "❌ Validation Failed: Provided file is not a valid WebAssembly module: {}",
            e
        )
    })?;

    // Calculate SHA-256 Hash
    let hash_bytes = digest(&SHA256, &bytes);
    let mut hash_hex = String::with_capacity(hash_bytes.as_ref().len() * 2);
    for b in hash_bytes.as_ref() {
        use std::fmt::Write;
        let _ = write!(&mut hash_hex, "{:02x}", b);
    }

    let cache_dir = get_wasm_cache_dir();
    let wasm_filename = format!("{}.wasm", hash_hex);
    let cwasm_filename = format!("{}.cwasm", hash_hex);

    let wasm_file_path = cache_dir.join(&wasm_filename);
    let cwasm_file_path = cache_dir.join(&cwasm_filename);

    // Save raw .wasm
    fs::write(&wasm_file_path, &bytes)
        .map_err(|e| format!("Failed to save raw .wasm to cache: {}", e))?;

    // Precompile and save .cwasm
    if let Ok(serialized) = module.serialize() {
        let _ = fs::write(&cwasm_file_path, serialized);
    }

    // Create readable symlinks
    create_readable_symlink(&cache_dir, &readable_name, &wasm_filename);

    let cwasm_readable_name = if readable_name.ends_with(".wasm") {
        format!("{}.cwasm", readable_name.trim_end_matches(".wasm"))
    } else {
        format!("{}.cwasm", readable_name)
    };
    create_readable_symlink(&cache_dir, &cwasm_readable_name, &cwasm_filename);

    println!("\n✅ WASM binary successfully validated and cached!");
    println!("  • Hash: {}", hash_hex);
    println!(
        "  • Size: {} bytes ({:.2} KB)",
        bytes.len(),
        bytes.len() as f64 / 1024.0
    );
    println!("  • Cached WASM: {}", wasm_file_path.display());
    println!("  • Precompiled CWASM: {}", cwasm_file_path.display());
    println!(
        "  • Symlink: {} -> {}",
        cache_dir.join(&readable_name).display(),
        wasm_filename
    );

    Ok(())
}

async fn handle_list() -> Result<(), String> {
    let cache_dir = get_wasm_cache_dir();
    println!("📦 Cached WASM binaries in {}:\n", cache_dir.display());

    if !cache_dir.exists() {
        println!("  (No cached WASM modules found)");
        return Ok(());
    }

    let entries = fs::read_dir(&cache_dir).map_err(|e| e.to_string())?;
    let mut count = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        let fname = entry.file_name().to_string_lossy().to_string();

        if let Ok(meta) = fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() {
                let target = fs::read_link(&path).unwrap_or_default();
                println!("  🔗 {} -> {}", fname, target.display());
                count += 1;
            } else if fname.ends_with(".wasm") || fname.ends_with(".cwasm") {
                let size_kb = meta.len() as f64 / 1024.0;
                println!("  📄 {} ({:.2} KB)", fname, size_kb);
                count += 1;
            }
        }
    }

    if count == 0 {
        println!("  (No cached WASM modules found)");
    }

    Ok(())
}
