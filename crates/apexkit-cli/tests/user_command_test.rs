//! Functional tests for the `user` subcommand, exercising real user
//! persistence inside an isolated temp working directory.

mod common;

use common::{run_in, stderr, stdout, temp_workdir};

#[test]
fn user_create_then_list_shows_user() {
    let dir = temp_workdir("user_create_list");

    let out = run_in(
        &dir,
        &[
            "user",
            "create",
            "--email",
            "cli@apexkit.io",
            "--password",
            "secret123",
            "--role",
            "admin",
        ],
    );
    assert!(out.status.success(), "create failed: {}", stderr(&out));
    assert!(stdout(&out).contains("cli@apexkit.io"));

    let out = run_in(&dir, &["user", "list"]);
    assert!(out.status.success(), "list failed: {}", stderr(&out));
    let s = stdout(&out);
    assert!(s.contains("cli@apexkit.io"), "list output: {s}");
    assert!(s.contains("admin"), "role column missing: {s}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_create_rejects_duplicate_email() {
    let dir = temp_workdir("user_dup");
    let args = [
        "user",
        "create",
        "--email",
        "dup@apexkit.io",
        "--password",
        "secret123",
    ];
    assert!(run_in(&dir, &args).status.success());

    let out = run_in(&dir, &args);
    assert!(!out.status.success(), "duplicate create should fail");
    assert!(stderr(&out).contains("already exists"), "stderr: {}", stderr(&out));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_create_rejects_invalid_email() {
    let dir = temp_workdir("user_bademail");
    let out = run_in(
        &dir,
        &[
            "user",
            "create",
            "--email",
            "not-an-email",
            "--password",
            "secret123",
        ],
    );
    assert!(!out.status.success());
    assert!(
        stderr(&out).to_lowercase().contains("invalid email"),
        "stderr: {}",
        stderr(&out)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_create_generates_password_when_omitted() {
    let dir = temp_workdir("user_genpw");
    let out = run_in(
        &dir,
        &["user", "create", "--email", "gen@apexkit.io"],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let s = stdout(&out);
    assert!(
        s.contains("Generated") || s.contains("generated"),
        "expected generated-password notice: {s}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_reset_password_succeeds_for_existing_user() {
    let dir = temp_workdir("user_reset");
    run_in(
        &dir,
        &[
            "user",
            "create",
            "--email",
            "reset@apexkit.io",
            "--password",
            "oldpass123",
        ],
    );

    let out = run_in(
        &dir,
        &["user", "reset-password", "reset@apexkit.io", "newpass123"],
    );
    assert!(out.status.success(), "reset failed: {}", stderr(&out));
    assert!(stdout(&out).contains("reset"), "stdout: {}", stdout(&out));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_reset_password_unknown_user_fails() {
    let dir = temp_workdir("user_reset_missing");
    let out = run_in(
        &dir,
        &["user", "reset-password", "ghost@apexkit.io", "x"],
    );
    assert!(!out.status.success());
    assert!(stderr(&out).contains("not found"), "stderr: {}", stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_list_empty_prints_header_only() {
    let dir = temp_workdir("user_list_empty");
    let out = run_in(&dir, &["user", "list"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let s = stdout(&out);
    assert!(s.contains("EMAIL"), "header missing: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}
