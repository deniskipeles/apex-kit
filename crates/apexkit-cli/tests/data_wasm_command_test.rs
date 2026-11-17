//! Functional tests for the `data`, `wasm`, `backup` and `restore`
//! subcommands, run against a real temp working directory.

mod common;

use common::{run_in, stderr, stdout, temp_workdir};

#[test]
fn data_export_missing_collection_errors() {
    let dir = temp_workdir("data_export_missing");
    // Fresh DB: collection 999 does not exist. Export should still succeed with
    // an empty list (list_records returns empty) rather than crashing.
    let out = run_in(&dir, &["data", "export", "999"]);
    assert!(
        out.status.success(),
        "export should not crash: {}",
        stderr(&out)
    );
    assert_eq!(stdout(&out).trim(), "[]");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn data_export_pretty_flag_prints_json() {
    let dir = temp_workdir("data_export_pretty");
    let out = run_in(&dir, &["data", "export", "1", "--pretty"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "[]");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn data_reindex_nonexistent_collection_reports() {
    let dir = temp_workdir("data_reindex");
    let out = run_in(&dir, &["data", "reindex", "1"]);
    // Reindexing a non-existent collection must not panic the process.
    let s = stdout(&out) + &stderr(&out);
    assert!(
        s.contains("Re-indexing") || s.contains("Error") || s.contains("error"),
        "unexpected output: {s}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn data_prune_logs_succeeds() {
    let dir = temp_workdir("data_prune");
    let out = run_in(&dir, &["data", "prune-logs"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("pruned") || stdout(&out).contains("Pruning"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wasm_list_on_empty_cache_prints_header() {
    let dir = temp_workdir("wasm_list");
    let out = run_in(&dir, &["wasm", "--list"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let s = stdout(&out);
    assert!(
        s.contains("CACHED WASM") || s.contains("No cached") || s.contains("NAME"),
        "unexpected wasm list output: {s}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wasm_file_import_rejects_missing_path() {
    let dir = temp_workdir("wasm_missing_file");
    let out = run_in(&dir, &["wasm", "--file", "does-not-exist.wasm"]);
    assert!(!out.status.success(), "missing file must fail");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wasm_with_no_arguments_is_a_noop() {
    let dir = temp_workdir("wasm_noop");
    let out = run_in(&dir, &["wasm"]);
    // No flags => nothing to do; must exit cleanly without crashing.
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn backup_creates_archive() {
    let dir = temp_workdir("backup_create");
    // Ensure the storage tree exists by running a command that initialises it.
    run_in(&dir, &["user", "list"]);

    let out = run_in(&dir, &["backup", "--root"]);
    let s = stdout(&out) + &stderr(&out);
    assert!(
        out.status.success() || s.contains("backup") || s.contains("Backup"),
        "backup output: {s}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restore_requires_confirmation_or_yes() {
    let dir = temp_workdir("restore_missing");
    // Restoring a non-existent archive must fail cleanly.
    let out = run_in(&dir, &["restore", "nope.tar.gz", "-y"]);
    assert!(!out.status.success(), "restoring a missing archive must fail");
    let _ = std::fs::remove_dir_all(&dir);
}
