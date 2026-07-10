use std::process::Command;

#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "apexkit-cli",
            "--",
            "--help"
        ])
        .output()
        .expect("failed to execute process");

    assert!(output.status.success(), "CLI --help should exit with success");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ApexKit CLI & Server Entrypoint"), "Help text should contain description");
}
