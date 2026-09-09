//! Integration tests for CLI binary executables (`hcl2json`, `hcl-repl`, `hcl-validate`).

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

/// Creates a unique temporary directory for test file isolation.
fn temp_test_dir(prefix: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("hcl_cli_test_{prefix}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    if let Err(e) = fs::create_dir_all(&dir) {
        panic!("failed to create temp dir: {e}");
    }
    dir
}

/// Tests executing the compiled `hcl2json` binary target via CLI arguments and standard input.
#[test]
fn test_hcl2json_cli_binary() -> Result<(), Box<dyn std::error::Error>> {
    // 1. --help flag prints usage and exits with error code 1
    let output = Command::new(env!("CARGO_BIN_EXE_hcl2json"))
        .arg("--help")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: hcl2json"));

    // 2. Default execution with no args converts default HCL
    let def_output = Command::new(env!("CARGO_BIN_EXE_hcl2json")).output()?;
    assert!(def_output.status.success());
    let def_stdout = String::from_utf8_lossy(&def_output.stdout);
    assert!(def_stdout.contains(r#""name": "test""#));

    // 3. Stream JSON from stdin using "-"
    let mut child = Command::new(env!("CARGO_BIN_EXE_hcl2json"))
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(b"key = \"value\"\n")?;
    }
    let output = child.wait_with_output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""key": "value""#));

    // 4. Invalid flag should fail
    let bad_output = Command::new(env!("CARGO_BIN_EXE_hcl2json"))
        .arg("--unrecognized-flag")
        .output()?;
    assert!(!bad_output.status.success());

    Ok(())
}

/// Tests executing the compiled `hcl-repl` binary target with piped interactive input.
#[test]
fn test_hcl_repl_cli_binary() -> Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hcl-repl"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(
            b"1 + 2
:exit
",
        )?;
    }
    let output = child.wait_with_output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("HashiCorp Configuration Language (HCL) REPL"));
    assert!(stdout.contains("3"));
    assert!(stdout.contains("Goodbye!"));

    Ok(())
}

/// Tests executing the compiled `hcl-validate` binary target via CLI arguments and files.
#[test]
fn test_hcl_validate_cli_binary() -> Result<(), Box<dyn std::error::Error>> {
    // 1. --help flag prints usage and exits with error code 1
    let output = Command::new(env!("CARGO_BIN_EXE_hcl-validate"))
        .arg("--help")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: hcl-validate"));

    // 2. Validate valid file
    let dir = temp_test_dir("validate");
    let valid_path = dir.join("valid.hcl");
    fs::write(
        &valid_path,
        r#"service = "web"
port = 8080
"#,
    )?;

    let valid_output = Command::new(env!("CARGO_BIN_EXE_hcl-validate"))
        .arg(&valid_path)
        .output()?;
    assert!(valid_output.status.success());
    let stdout = String::from_utf8_lossy(&valid_output.stdout);
    assert!(stdout.contains("Success! The configuration is valid."));

    // 3. Validate invalid file returns failure
    let invalid_path = dir.join("invalid.hcl");
    fs::write(&invalid_path, "service =\n")?;

    let invalid_output = Command::new(env!("CARGO_BIN_EXE_hcl-validate"))
        .arg(&invalid_path)
        .output()?;
    assert!(!invalid_output.status.success());

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}
