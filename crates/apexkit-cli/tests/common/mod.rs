//! Shared helpers for apexkit-cli integration tests.
//!
//! The CLI has no library target, so tests drive the compiled binary directly
//! via `CARGO_BIN_EXE_apexkit-cli` inside an isolated temporary working
//! directory.

#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Command, Output};

/// Absolute path to the compiled CLI binary under test.
pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_apexkit-cli")
}

/// Creates a fresh temp directory to use as the CLI working directory.
pub fn temp_workdir(tag: &str) -> PathBuf {
    let id = uuid::Uuid::new_v4();
    let path = std::env::temp_dir().join(format!("apexkit_cli_{tag}_{id}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// Runs the CLI with `args` inside `cwd`, returning the raw output.
pub fn run_in(cwd: &std::path::Path, args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .env("APEXKIT_MASTER_KEY", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
        .env("PORT", "0")
        .output()
        .expect("failed to spawn apexkit-cli")
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}
