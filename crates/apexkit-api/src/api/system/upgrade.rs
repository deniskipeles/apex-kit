use crate::{AppError, AppState};
use apexkit_core::auth::Claims;
use axum::{
    Extension, Json,
    extract::{Multipart, Path, State},
    http::StatusCode,
};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path as StdPath, PathBuf};
use std::process::Command;
use std::time::Duration;

fn is_executable_binary(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    // ELF binary (Linux)
    if bytes.starts_with(&[0x7f, b'E', b'L', b'F']) {
        return true;
    }
    // Mach-O binary (macOS 32/64/universal)
    if bytes.starts_with(&[0xfe, 0xed, 0xfa, 0xce])
        || bytes.starts_with(&[0xfe, 0xed, 0xfa, 0xcf])
        || bytes.starts_with(&[0xce, 0xfa, 0xed, 0xfe])
        || bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
        || bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe])
    {
        return true;
    }
    // Windows PE binary (MZ)
    if bytes.starts_with(&[b'M', b'Z']) {
        return true;
    }
    false
}

fn extract_from_zip(bytes: &[u8], target_exe_name: &str) -> Result<Vec<u8>, AppError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| AppError::UnknownError(format!("Failed to parse ZIP archive: {}", e)))?;

    let mut candidate: Option<(String, Vec<u8>)> = None;
    let mut best_fallback: Option<(String, Vec<u8>)> = None;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| {
            AppError::UnknownError(format!("Failed reading ZIP entry {}: {}", i, e))
        })?;

        if file.is_dir() {
            continue;
        }

        let name = file.name().to_string();
        let file_stem = StdPath::new(&name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let lower = file_stem.to_lowercase();
        if lower.ends_with(".md")
            || lower.ends_with(".txt")
            || lower.ends_with(".json")
            || lower.ends_with(".yml")
            || lower.ends_with(".yaml")
            || lower.starts_with("license")
        {
            continue;
        }

        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| {
            AppError::UnknownError(format!("Failed to extract ZIP entry '{}': {}", name, e))
        })?;

        if file_stem == target_exe_name
            || file_stem.eq_ignore_ascii_case(target_exe_name)
            || file_stem.eq_ignore_ascii_case(&format!("{}.exe", target_exe_name))
            || file_stem == "apexkit"
            || file_stem == "apexkit-cli"
        {
            candidate = Some((name, buf));
            break;
        }

        if is_executable_binary(&buf) {
            if best_fallback
                .as_ref()
                .map(|(_, b)| buf.len() > b.len())
                .unwrap_or(true)
            {
                best_fallback = Some((name, buf));
            }
        }
    }

    if let Some((_, b)) = candidate.or(best_fallback) {
        Ok(b)
    } else {
        Err(AppError::NotFound(format!(
            "No executable binary matching '{}' found inside ZIP artifact",
            target_exe_name
        )))
    }
}

fn extract_from_tar_archive(
    bytes: &[u8],
    target_exe_name: &str,
    is_gz: bool,
) -> Result<Vec<u8>, AppError> {
    let rand_id = uuid::Uuid::new_v4().to_string();
    let tmp_archive = crate::utils::get_temp_path(&format!("upgrade_archive_{}.tmp", rand_id));
    let tmp_extract_dir = crate::utils::get_temp_path(&format!("upgrade_extracted_{}", rand_id));

    std::fs::create_dir_all(&tmp_extract_dir).map_err(|e| {
        AppError::UnknownError(format!("Failed to create temporary directory: {}", e))
    })?;
    std::fs::write(&tmp_archive, bytes)
        .map_err(|e| AppError::UnknownError(format!("Failed to write temporary archive: {}", e)))?;

    let mut cmd = Command::new("tar");
    if is_gz {
        cmd.arg("-xzf");
    } else {
        cmd.arg("-xf");
    }
    cmd.arg(&tmp_archive).arg("-C").arg(&tmp_extract_dir);

    let output = cmd.output();
    let tar_succeeded = output.as_ref().map(|o| o.status.success()).unwrap_or(false);

    if !tar_succeeded && is_gz {
        // Fallback for raw single-file .gz (not a tarball)
        if let Ok(gunzip_out) = Command::new("gzip").arg("-dc").arg(&tmp_archive).output() {
            if gunzip_out.status.success() && is_executable_binary(&gunzip_out.stdout) {
                let _ = std::fs::remove_file(&tmp_archive);
                let _ = std::fs::remove_dir_all(&tmp_extract_dir);
                return Ok(gunzip_out.stdout);
            }
        }
    }

    if !tar_succeeded {
        let _ = std::fs::remove_file(&tmp_archive);
        let _ = std::fs::remove_dir_all(&tmp_extract_dir);
        let err_msg = output
            .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
            .unwrap_or_else(|e| e.to_string());
        return Err(AppError::UnknownError(format!(
            "Tar extraction failed: {}",
            err_msg
        )));
    }

    let mut candidate_bytes: Option<Vec<u8>> = None;
    let mut best_fallback: Option<Vec<u8>> = None;

    for entry in walkdir::WalkDir::new(&tmp_extract_dir)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let lower = file_name.to_lowercase();
            if lower.ends_with(".md")
                || lower.ends_with(".txt")
                || lower.ends_with(".json")
                || lower.ends_with(".yml")
                || lower.ends_with(".yaml")
                || lower.starts_with("license")
            {
                continue;
            }

            if let Ok(file_bytes) = std::fs::read(entry.path()) {
                if file_name == target_exe_name
                    || file_name.eq_ignore_ascii_case(target_exe_name)
                    || file_name.eq_ignore_ascii_case(&format!("{}.exe", target_exe_name))
                    || file_name == "apexkit"
                    || file_name == "apexkit-cli"
                {
                    candidate_bytes = Some(file_bytes);
                    break;
                }

                if is_executable_binary(&file_bytes) {
                    if best_fallback
                        .as_ref()
                        .map(|b| file_bytes.len() > b.len())
                        .unwrap_or(true)
                    {
                        best_fallback = Some(file_bytes);
                    }
                }
            }
        }
    }

    let _ = std::fs::remove_file(&tmp_archive);
    let _ = std::fs::remove_dir_all(&tmp_extract_dir);

    if let Some(b) = candidate_bytes.or(best_fallback) {
        Ok(b)
    } else {
        Err(AppError::NotFound(format!(
            "No executable binary matching '{}' found inside tar archive",
            target_exe_name
        )))
    }
}

fn unpack_binary_if_compressed(bytes: Vec<u8>, current_exe: &StdPath) -> Result<Vec<u8>, AppError> {
    if bytes.len() < 4 {
        return Ok(bytes);
    }

    let exe_name = current_exe
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("apexkit");

    // 1. Detect ZIP archive magic bytes: PK\x03\x04
    if bytes.starts_with(&[0x50, 0x4b, 0x03, 0x04]) {
        tracing::info!("📦 Detected ZIP compressed artifact. Extracting binary...");
        return extract_from_zip(&bytes, exe_name);
    }

    // 2. Detect GZIP / TAR.GZ magic bytes: \x1f\x8b
    if bytes.starts_with(&[0x1f, 0x8b]) {
        tracing::info!("📦 Detected GZIP / TAR.GZ compressed artifact. Extracting binary...");
        return extract_from_tar_archive(&bytes, exe_name, true);
    }

    // 3. Detect uncompressed TAR: "ustar" header at offset 257
    if bytes.len() > 262 && &bytes[257..262] == b"ustar" {
        tracing::info!("📦 Detected uncompressed TAR artifact. Extracting binary...");
        return extract_from_tar_archive(&bytes, exe_name, false);
    }

    // 4. Raw uncompressed binary
    Ok(bytes)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RemoteUpgradeReq {
    pub url: String,
    pub sha256: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UpgradeResponse {
    pub success: bool,
    pub message: String,
    pub backup_name: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RollbackItem {
    pub name: String,
    pub size: u64,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct RollbackPath {
    pub name: String,
}

fn get_next_rollback_path(current_exe: &StdPath) -> (PathBuf, String) {
    let parent = current_exe.parent().unwrap_or_else(|| StdPath::new("."));
    let exe_name = current_exe
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("apexkit");

    let prefix = format!("{}__old-", exe_name);
    let mut highest_index = 0u64;

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if let Some(suffix) = name.strip_prefix(&prefix) {
                    if let Ok(num) = suffix.parse::<u64>() {
                        if num > highest_index {
                            highest_index = num;
                        }
                    }
                }
            }
        }
    }

    let next_index = highest_index + 1;
    let backup_name = format!("{}{}", prefix, next_index);
    (parent.join(&backup_name), backup_name)
}

async fn download_binary(url: &str) -> Result<Vec<u8>, AppError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| AppError::UnknownError(e.to_string()))?;

    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::UnknownError(format!("Failed to download artifact: {}", e)))?;

    if !res.status().is_success() {
        return Err(AppError::UnknownError(format!(
            "Remote artifact download returned HTTP {}",
            res.status()
        )));
    }

    let bytes = res
        .bytes()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
        .to_vec();

    Ok(bytes)
}

pub async fn upgrade_handler(
    Extension(claims): Extension<Claims>,
    State(_state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<UpgradeResponse>, AppError> {
    if claims.role != "admin" || claims.scope != "root" {
        return Err(AppError::Forbidden(
            "Only Root Admins can upgrade the host binary".into(),
        ));
    }

    let mut binary_bytes = Vec::new();
    let mut expected_checksum = None;
    let mut artifact_url = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" || name == "binary" {
            binary_bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::UnknownError(e.to_string()))?
                .to_vec();
        } else if name == "url" || name == "link" {
            artifact_url = field.text().await.ok();
        } else if name == "sha256" {
            expected_checksum = field.text().await.ok();
        }
    }

    if binary_bytes.is_empty() {
        if let Some(url) = artifact_url {
            binary_bytes = download_binary(&url).await?;
        } else {
            return Err(AppError::JsonError(
                "Must provide either a binary file upload or an artifact URL link".into(),
            ));
        }
    }

    perform_upgrade(binary_bytes, expected_checksum).await
}

pub async fn remote_upgrade_handler(
    Extension(claims): Extension<Claims>,
    State(_state): State<AppState>,
    Json(payload): Json<RemoteUpgradeReq>,
) -> Result<Json<UpgradeResponse>, AppError> {
    if claims.role != "admin" || claims.scope != "root" {
        return Err(AppError::Forbidden(
            "Only Root Admins can upgrade the host binary".into(),
        ));
    }

    let binary_bytes = download_binary(&payload.url).await?;
    perform_upgrade(binary_bytes, payload.sha256).await
}

async fn perform_upgrade(
    raw_bytes: Vec<u8>,
    expected_sha256: Option<String>,
) -> Result<Json<UpgradeResponse>, AppError> {
    let current_exe = std::env::current_exe()
        .map_err(|e| AppError::UnknownError(format!("Failed to determine current_exe: {}", e)))?;

    // 1. Unpack compressed artifact (if applicable) and evaluate checksum
    let binary_bytes = if let Some(expected) = expected_sha256 {
        let raw_hash = apexkit_core::utils::to_hex(digest(&SHA256, &raw_bytes).as_ref());
        let expected_clean = expected.trim().to_lowercase();

        if raw_hash.to_lowercase() == expected_clean {
            // Checksum matched the compressed artifact directly
            unpack_binary_if_compressed(raw_bytes, &current_exe)?
        } else {
            // Check if checksum was generated against the uncompressed executable
            let unpacked = unpack_binary_if_compressed(raw_bytes, &current_exe)?;
            let unpacked_hash = apexkit_core::utils::to_hex(digest(&SHA256, &unpacked).as_ref());
            if unpacked_hash.to_lowercase() != expected_clean {
                return Err(AppError::Forbidden(format!(
                    "Checksum mismatch! Expected: {}, archive SHA-256: {}, binary SHA-256: {}",
                    expected, raw_hash, unpacked_hash
                )));
            }
            unpacked
        }
    } else {
        unpack_binary_if_compressed(raw_bytes, &current_exe)?
    };

    // 2. Identify staging path
    let staging_path = current_exe.with_extension("staging_upgrade");

    // 3. Write binary to temporary staging file
    tokio::fs::write(&staging_path, &binary_bytes)
        .await
        .map_err(|e| AppError::UnknownError(format!("Failed to write staging binary: {}", e)))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&staging_path)
            .map_err(|e| AppError::UnknownError(e.to_string()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&staging_path, perms)
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    // 4. Smoke-test candidate binary before swapping
    let test_output = Command::new(&staging_path)
        .arg("--help")
        .output()
        .map_err(|e| {
            let _ = std::fs::remove_file(&staging_path);
            AppError::UnknownError(format!(
                "Candidate binary failed smoke-test execution: {}",
                e
            ))
        })?;

    if !test_output.status.success() {
        let _ = std::fs::remove_file(&staging_path);
        return Err(AppError::UnknownError(
            "Candidate binary exited with an error on smoke-test".into(),
        ));
    }

    // 5. Compute next rollback archive name (e.g. apexkit__old-1)
    let (backup_path, backup_name) = get_next_rollback_path(&current_exe);

    // 6. Inode swap: rename current executable into rollback name, then staging over current_exe
    tokio::fs::rename(&current_exe, &backup_path)
        .await
        .map_err(|e| AppError::UnknownError(format!("Failed to archive running binary: {}", e)))?;

    if let Err(e) = tokio::fs::rename(&staging_path, &current_exe).await {
        // Rollback on rename failure
        let _ = tokio::fs::rename(&backup_path, &current_exe).await;
        return Err(AppError::UnknownError(format!(
            "Atomic binary swap failed: {}",
            e
        )));
    }

    // 7. Spawn the upgraded process and drain connections on the old instance
    let exe_to_spawn = current_exe.clone();
    let current_args: Vec<String> = std::env::args().skip(1).collect();

    tokio::spawn(async move {
        tracing::info!(
            "🚀 Spawning updated ApexKit process ({}) ...",
            exe_to_spawn.display()
        );
        let mut child = Command::new(&exe_to_spawn);
        child.args(&current_args);

        match child.spawn() {
            Ok(_) => {
                tracing::info!("✅ New ApexKit instance spawned. Waiting for initialization...");
                tokio::time::sleep(Duration::from_secs(3)).await;
                crate::trigger_graceful_shutdown();
            }
            Err(e) => {
                tracing::error!("❌ Failed to spawn upgraded process: {}", e);
            }
        }
    });

    Ok(Json(UpgradeResponse {
        success: true,
        message: format!(
            "Binary upgraded successfully. Archived previous version as '{}'.",
            backup_name
        ),
        backup_name: Some(backup_name),
    }))
}

pub async fn list_rollbacks_handler(
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<RollbackItem>>, AppError> {
    if claims.role != "admin" || claims.scope != "root" {
        return Err(AppError::Forbidden("Root Admins only".into()));
    }

    let current_exe = std::env::current_exe().map_err(|e| AppError::UnknownError(e.to_string()))?;
    let parent = current_exe.parent().unwrap_or_else(|| StdPath::new("."));
    let exe_name = current_exe
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("apexkit");

    let prefix = format!("{}__old-", exe_name);
    let mut items = Vec::new();

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if name.starts_with(&prefix) {
                    if let Ok(meta) = entry.metadata() {
                        let created_at = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
                            .flatten()
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_default();

                        items.push(RollbackItem {
                            name,
                            size: meta.len(),
                            created_at,
                        });
                    }
                }
            }
        }
    }

    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(Json(items))
}

pub async fn rollback_handler(
    Extension(claims): Extension<Claims>,
    Path(params): Path<RollbackPath>,
) -> Result<Json<UpgradeResponse>, AppError> {
    if claims.role != "admin" || claims.scope != "root" {
        return Err(AppError::Forbidden("Root Admins only".into()));
    }

    let current_exe = std::env::current_exe().map_err(|e| AppError::UnknownError(e.to_string()))?;
    let parent = current_exe.parent().unwrap_or_else(|| StdPath::new("."));

    let safe_name = params.name.replace("..", "").replace('/', "");
    let rollback_file = parent.join(&safe_name);

    if !rollback_file.exists() || !rollback_file.is_file() {
        return Err(AppError::NotFound(format!(
            "Rollback archive '{}' not found",
            safe_name
        )));
    }

    // Smoke-test the rollback binary
    let test_output = Command::new(&rollback_file)
        .arg("--help")
        .output()
        .map_err(|e| {
            AppError::UnknownError(format!("Rollback binary failed execution test: {}", e))
        })?;

    if !test_output.status.success() {
        return Err(AppError::UnknownError(
            "Rollback binary exited with an error".into(),
        ));
    }

    // Archive current binary before rolling back
    let (backup_path, backup_name) = get_next_rollback_path(&current_exe);
    tokio::fs::rename(&current_exe, &backup_path)
        .await
        .map_err(|e| AppError::UnknownError(format!("Failed to archive current binary: {}", e)))?;

    // Copy the selected rollback file to current_exe
    tokio::fs::copy(&rollback_file, &current_exe)
        .await
        .map_err(|e| AppError::UnknownError(format!("Failed to restore rollback binary: {}", e)))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&current_exe)
            .map_err(|e| AppError::UnknownError(e.to_string()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&current_exe, perms)
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    }

    let exe_to_spawn = current_exe.clone();
    let current_args: Vec<String> = std::env::args().skip(1).collect();

    tokio::spawn(async move {
        tracing::info!(
            "🚀 Spawning rollback ApexKit process ({}) ...",
            exe_to_spawn.display()
        );
        let mut child = Command::new(&exe_to_spawn);
        child.args(&current_args);

        match child.spawn() {
            Ok(_) => {
                tracing::info!("✅ Rollback instance spawned. Handover in progress...");
                tokio::time::sleep(Duration::from_secs(3)).await;
                crate::trigger_graceful_shutdown();
            }
            Err(e) => {
                tracing::error!("❌ Failed to spawn rollback process: {}", e);
            }
        }
    });

    Ok(Json(UpgradeResponse {
        success: true,
        message: format!(
            "Rolled back to '{}'. Current binary backed up as '{}'.",
            safe_name, backup_name
        ),
        backup_name: Some(backup_name),
    }))
}

pub async fn delete_rollback_handler(
    Extension(claims): Extension<Claims>,
    Path(params): Path<RollbackPath>,
) -> Result<StatusCode, AppError> {
    if claims.role != "admin" || claims.scope != "root" {
        return Err(AppError::Forbidden("Root Admins only".into()));
    }

    let current_exe = std::env::current_exe().map_err(|e| AppError::UnknownError(e.to_string()))?;
    let parent = current_exe.parent().unwrap_or_else(|| StdPath::new("."));

    let safe_name = params.name.replace("..", "").replace('/', "");
    if !safe_name.contains("__old-") {
        return Err(AppError::Forbidden(
            "Can only delete rollback archives".into(),
        ));
    }

    let target_file = parent.join(&safe_name);
    if target_file.exists() && target_file.is_file() {
        tokio::fs::remove_file(target_file)
            .await
            .map_err(|e| AppError::UnknownError(e.to_string()))?;
    } else {
        return Err(AppError::NotFound("Rollback file not found".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}
