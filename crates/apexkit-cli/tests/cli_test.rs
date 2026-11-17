//! Smoke tests for the compiled CLI binary: it exists, is executable, and
//! responds to the top-level flags without panicking.

mod common;

use common::{bin, run_in, stdout, temp_workdir};

#[test]
fn binary_exists_and_is_executable() {
    let path = std::path::Path::new(bin());
    assert!(path.exists(), "binary missing at {}", bin());
    let meta = std::fs::metadata(path).unwrap();
    assert!(meta.len() > 0, "binary is empty");
}

#[test]
fn help_mentions_entrypoint_description() {
    let dir = temp_workdir("smoke_help");
    let out = run_in(&dir, &["--help"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("ApexKit CLI & Server Entrypoint"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn version_exits_zero() {
    let dir = temp_workdir("smoke_version");
    let out = run_in(&dir, &["--version"]);
    assert!(out.status.success());
    let _ = std::fs::remove_dir_all(&dir);
}
