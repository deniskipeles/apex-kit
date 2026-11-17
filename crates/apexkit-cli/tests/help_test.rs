//! CLI argument-parsing tests: help text, version, and subcommand structure.
//!
//! These exercise the compiled binary's clap layer end to end.

mod common;

use common::{run_in, stderr, stdout, temp_workdir};

#[test]
fn top_level_help_lists_all_commands() {
    let dir = temp_workdir("help_top");
    let out = run_in(&dir, &["--help"]);
    assert!(out.status.success());
    let s = stdout(&out);
    for cmd in ["user", "data", "wasm", "backup", "restore"] {
        assert!(s.contains(cmd), "help missing `{cmd}`:\n{s}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn version_flag_prints_version() {
    let dir = temp_workdir("help_version");
    let out = run_in(&dir, &["--version"]);
    assert!(out.status.success());
    let s = stdout(&out);
    assert!(s.contains("apexkit"), "version output: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_help_lists_subcommands() {
    let dir = temp_workdir("help_user");
    let out = run_in(&dir, &["user", "--help"]);
    assert!(out.status.success());
    let s = stdout(&out);
    for sub in ["create", "reset-password", "list"] {
        assert!(s.contains(sub), "user help missing `{sub}`:\n{s}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn data_help_lists_subcommands() {
    let dir = temp_workdir("help_data");
    let out = run_in(&dir, &["data", "--help"]);
    assert!(out.status.success());
    let s = stdout(&out);
    for sub in ["reindex", "export", "prune-logs"] {
        assert!(s.contains(sub), "data help missing `{sub}`:\n{s}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_create_requires_email() {
    let dir = temp_workdir("help_email");
    let out = run_in(&dir, &["user", "create"]);
    assert!(!out.status.success(), "missing --email must fail");
    let s = stderr(&out);
    assert!(s.contains("email") || s.contains("EMAIL"), "stderr: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn data_export_requires_id() {
    let dir = temp_workdir("help_export_id");
    let out = run_in(&dir, &["data", "export"]);
    assert!(!out.status.success(), "missing <ID> must fail");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_subcommand_fails() {
    let dir = temp_workdir("help_unknown");
    let out = run_in(&dir, &["definitely-not-a-command"]);
    assert!(!out.status.success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wasm_help_documents_flags() {
    let dir = temp_workdir("help_wasm");
    let out = run_in(&dir, &["wasm", "--help"]);
    assert!(out.status.success());
    let s = stdout(&out);
    for flag in ["--get", "--file", "--name", "--list"] {
        assert!(s.contains(flag), "wasm help missing `{flag}`:\n{s}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
