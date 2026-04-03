use std::process::Command;
use tempfile::TempDir;

fn autovault_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_autovault"))
}

#[test]
fn init_creates_vault_structure() {
    let dir = TempDir::new().unwrap();
    let vault_path = dir.path().to_str().unwrap();

    let output = autovault_cmd()
        .args(["--vault", vault_path, "init"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    assert!(dir.path().join("raw").exists());
    assert!(dir.path().join("wiki").exists());
    assert!(dir.path().join("state").exists());
    assert!(dir.path().join("logs").exists());
    assert!(dir.path().join("state/manifest.json").exists());
}

#[test]
fn collect_on_empty_vault() {
    let dir = TempDir::new().unwrap();
    let vault_path = dir.path().to_str().unwrap();

    // Init first
    autovault_cmd()
        .args(["--vault", vault_path, "init"])
        .output()
        .unwrap();

    // Collect with no files
    let output = autovault_cmd()
        .args(["--vault", vault_path, "collect"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0 new"));
}

#[test]
fn collect_finds_new_md_files() {
    let dir = TempDir::new().unwrap();
    let vault_path = dir.path().to_str().unwrap();

    autovault_cmd()
        .args(["--vault", vault_path, "init"])
        .output()
        .unwrap();

    // Add a raw note
    std::fs::write(dir.path().join("raw/test.md"), "# Test Note\nContent").unwrap();

    let output = autovault_cmd()
        .args(["--vault", vault_path, "collect"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 new"));
}

#[test]
fn collect_json_output() {
    let dir = TempDir::new().unwrap();
    let vault_path = dir.path().to_str().unwrap();

    autovault_cmd()
        .args(["--vault", vault_path, "init"])
        .output()
        .unwrap();

    std::fs::write(dir.path().join("raw/note.md"), "# Note").unwrap();

    let output = autovault_cmd()
        .args(["--vault", vault_path, "--json", "collect"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["new"].as_array().unwrap().len(), 1);
}

#[test]
fn status_on_fresh_vault() {
    let dir = TempDir::new().unwrap();
    let vault_path = dir.path().to_str().unwrap();

    autovault_cmd()
        .args(["--vault", vault_path, "init"])
        .output()
        .unwrap();

    let output = autovault_cmd()
        .args(["--vault", vault_path, "status"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Pending: 0"));
}

#[test]
fn help_output() {
    let output = autovault_cmd()
        .args(["--help"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("autovault"));
    assert!(stdout.contains("init"));
    assert!(stdout.contains("collect"));
    assert!(stdout.contains("compile"));
    assert!(stdout.contains("run"));
}

#[test]
fn uninit_vault_returns_error() {
    let dir = TempDir::new().unwrap();
    let vault_path = dir.path().to_str().unwrap();

    // Don't init, try to collect
    let output = autovault_cmd()
        .args(["--vault", vault_path, "collect"])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success());
}

#[test]
fn init_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let vault_path = dir.path().to_str().unwrap();

    autovault_cmd()
        .args(["--vault", vault_path, "init"])
        .output()
        .unwrap();

    // Second init should succeed
    let output = autovault_cmd()
        .args(["--vault", vault_path, "init"])
        .output()
        .unwrap();

    assert!(output.status.success());
}

#[test]
fn lint_on_empty_vault() {
    let dir = TempDir::new().unwrap();
    let vault_path = dir.path().to_str().unwrap();

    autovault_cmd()
        .args(["--vault", vault_path, "init"])
        .output()
        .unwrap();

    let output = autovault_cmd()
        .args(["--vault", vault_path, "lint"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0 issues"));
}
