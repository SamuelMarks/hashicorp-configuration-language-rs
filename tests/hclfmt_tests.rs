//! Integration tests for the `hclfmt` command-line binary.

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn temp_test_dir(prefix: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("hclfmt_test_{prefix}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir
}

#[test]
fn test_hclfmt_write_modifies_file_in_place() -> Result<(), Box<dyn std::error::Error>> {
    let dir = temp_test_dir("write");
    let file_path = dir.join("test.hcl");
    let unformatted = "a=1
b=2
";
    fs::write(&file_path, unformatted)?;

    let status = Command::new(env!("CARGO_BIN_EXE_hclfmt"))
        .arg("-w")
        .arg(&file_path)
        .status()?;

    assert!(status.success());
    let formatted = fs::read_to_string(&file_path)?;
    assert_ne!(formatted, unformatted);
    assert!(formatted.contains("a = 1"));

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_hclfmt_list_reports_unformatted_without_modifying() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = temp_test_dir("list");
    let file_path = dir.join("test.hcl");
    let unformatted = "a=1
b=2
";
    fs::write(&file_path, unformatted)?;

    let output = Command::new(env!("CARGO_BIN_EXE_hclfmt"))
        .arg("-l")
        .arg(&file_path)
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains(file_path.to_str().ok_or("path error")?));

    // File content should remain unformatted
    let content = fs::read_to_string(&file_path)?;
    assert_eq!(content, unformatted);

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_hclfmt_diff_outputs_unified_diff() -> Result<(), Box<dyn std::error::Error>> {
    let dir = temp_test_dir("diff");
    let file_path = dir.join("test.hcl");
    let unformatted = "a=1
b=2
";
    fs::write(&file_path, unformatted)?;

    let output = Command::new(env!("CARGO_BIN_EXE_hclfmt"))
        .arg("-d")
        .arg(&file_path)
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("--- "));
    assert!(stdout.contains("+++ "));
    assert!(stdout.contains("@@ -1,"));

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_hclfmt_check_exit_code() -> Result<(), Box<dyn std::error::Error>> {
    let dir = temp_test_dir("check");
    let file_path = dir.join("test.hcl");
    let unformatted = "a=1
b=2
";
    fs::write(&file_path, unformatted)?;

    // Check on unformatted file should return exit code 1
    let status_fail = Command::new(env!("CARGO_BIN_EXE_hclfmt"))
        .arg("-c")
        .arg(&file_path)
        .status()?;
    assert_eq!(status_fail.code(), Some(1));

    // Format the file
    let status_write = Command::new(env!("CARGO_BIN_EXE_hclfmt"))
        .arg("-w")
        .arg(&file_path)
        .status()?;
    assert!(status_write.success());

    // Check on formatted file should return exit code 0
    let status_pass = Command::new(env!("CARGO_BIN_EXE_hclfmt"))
        .arg("-c")
        .arg(&file_path)
        .status()?;
    assert_eq!(status_pass.code(), Some(0));

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_hclfmt_stdin_stdout_streaming() -> Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hclfmt"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let unformatted = "foo=true
";
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(unformatted.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(
        stdout,
        "foo = true
"
    );

    Ok(())
}

#[test]
fn test_hclfmt_directory_traversal_filtering() -> Result<(), Box<dyn std::error::Error>> {
    let dir = temp_test_dir("dir_walk");
    let hcl_file = dir.join("a.hcl");
    let tf_file = dir.join("b.tf");
    let txt_file = dir.join("c.txt");

    let unformatted = "x=10
";
    fs::write(&hcl_file, unformatted)?;
    fs::write(&tf_file, unformatted)?;
    fs::write(&txt_file, unformatted)?;

    let status = Command::new(env!("CARGO_BIN_EXE_hclfmt"))
        .arg("-w")
        .arg(&dir)
        .status()?;
    assert!(status.success());

    // .hcl and .tf should be formatted
    assert_eq!(
        fs::read_to_string(&hcl_file)?,
        "x = 10
"
    );
    assert_eq!(
        fs::read_to_string(&tf_file)?,
        "x = 10
"
    );
    // .txt should be untouched
    assert_eq!(fs::read_to_string(&txt_file)?, unformatted);

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_hclfmt_invalid_flag_exits_one() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_hclfmt"))
        .arg("--invalid-flag-nonexistent")
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    let err_msg = String::from_utf8(output.stderr)?;
    assert!(err_msg.contains("Error:"));
    Ok(())
}
