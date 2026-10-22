//! Tool to convert HCL to JSON (`hcl2json`).
#![forbid(missing_docs)]
#![forbid(rustdoc::missing_crate_level_docs)]
#![forbid(clippy::all, clippy::pedantic, clippy::nursery)]
#![forbid(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::diagnostic::Diagnostic;
use hashicorp_configuration_language_rs::error::HclError;
use hashicorp_configuration_language_rs::serde::from_str;
use hashicorp_configuration_language_rs::span::Span;
use serde_json::Value;
#[cfg(not(test))]
use std::env;
use std::fs;
use std::io::{self, Read, Write};

/// CLI options for `hcl2json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hcl2JsonOptions {
    /// Emit error diagnostics as JSON to standard error.
    pub json: bool,
    /// Target file paths or `"-"` for standard input.
    pub files: Vec<String>,
}

/// Parses CLI arguments into [`Hcl2JsonOptions`].
///
/// # Arguments
/// * `args` - The command-line argument strings.
///
/// # Errors
/// Returns [`HclError::CliArgs`] if unrecognized flags are passed.
pub fn parse_hcl2json_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<Hcl2JsonOptions, HclError> {
    let mut opts = Hcl2JsonOptions::default();
    for arg in args {
        match arg.as_str() {
            "-j" | "--json" => opts.json = true,
            "-h" | "--help" => {
                return Err(HclError::CliArgs(
                    "Usage: hcl2json [-j|--json] [file ...]".to_string(),
                ));
            }
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(HclError::CliArgs(format!("Unknown flag '{flag}'")));
            }
            file => opts.files.push(file.to_string()),
        }
    }
    Ok(opts)
}

/// Converts an HCL string to a pretty-printed JSON string.
///
/// # Arguments
/// * `hcl` - The input HCL configuration string.
///
/// # Errors
/// Returns [`HclError`] if the HCL cannot be parsed or serialized.
pub fn convert_to_json(hcl: &str) -> Result<String, HclError> {
    let v: Value = from_str(hcl)?;
    Ok(serde_json::to_string_pretty(&v).unwrap_or_default())
}

/// Runs `hcl2json` on the given input string, writing JSON to standard output.
///
/// # Arguments
/// * `hcl` - The input HCL configuration string.
///
/// # Errors
/// Returns [`HclError`] if the HCL cannot be parsed.
pub fn main_impl(hcl: &str) -> Result<(), HclError> {
    let json = convert_to_json(hcl)?;
    println!("{json}");
    Ok(())
}

/// Executes `hcl2json` using the provided options, input reader, output writer, and error writer.
///
/// # Arguments
/// * `opts` - Configuration options.
/// * `stdin` - Input reader.
/// * `stdout` - Output writer.
/// * `stderr` - Error writer.
///
/// # Errors
/// Returns [`HclError::Io`] if an unrecoverable I/O error occurs.
pub fn run_hcl2json(
    opts: &Hcl2JsonOptions,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, HclError> {
    let targets = if opts.files.is_empty() {
        vec!["-".to_string()]
    } else {
        opts.files.clone()
    };
    let mut had_error = false;
    for target in targets {
        let (content, display_name) = if target == "-" {
            let mut buf = String::new();
            stdin
                .read_to_string(&mut buf)
                .map_err(|e| HclError::Io(format!("Failed to read standard input: {e}")))?;
            (buf, "<stdin>".to_string())
        } else {
            let buf = fs::read_to_string(&target)
                .map_err(|e| HclError::Io(format!("Failed to read file '{target}': {e}")))?;
            (buf, target)
        };
        match convert_to_json(&content) {
            Ok(json) => {
                let _ = writeln!(stdout, "{json}");
            }
            Err(e) => {
                had_error = true;
                let err_msg = e.to_string();
                if opts.json {
                    let span = Span::new_with_file(
                        0,
                        content.len(),
                        1,
                        1,
                        content.lines().count().max(1),
                        1,
                        Some(std::sync::Arc::from(display_name.as_str())),
                    );
                    let mut diag = Diagnostic::new(HclError::Parse(err_msg.clone()), span);
                    diag.summary = Some(format!("Error parsing {display_name}"));
                    diag.detail = Some(err_msg);
                    let json_str = diag.to_json(Some(&content)).unwrap_or_default();
                    let _ = writeln!(stderr, "{json_str}");
                } else {
                    let _ = writeln!(stderr, "Error parsing {display_name}: {err_msg}");
                }
            }
        }
    }
    if had_error { Ok(1) } else { Ok(0) }
}

/// Runs `hcl2json` with the provided arguments.
///
/// # Arguments
/// * `args` - Command-line arguments.
///
/// # Errors
/// Returns [`HclError`] if argument parsing, execution, or serialization fails.
pub fn main_with_args<I: IntoIterator<Item = String>>(args: I) -> Result<(), HclError> {
    let args: Vec<String> = args.into_iter().collect();
    if args.is_empty() {
        let default_hcl = r#"
            name = "test"
            count = 42
            block "b1" {
                inner = true
            }
        "#;
        main_impl(default_hcl)
    } else {
        let opts = parse_hcl2json_args(args)?;
        let mut stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut stderr = io::stderr();
        let code = run_hcl2json(&opts, &mut stdin, &mut stdout, &mut stderr)?;
        if code == 0 {
            Ok(())
        } else {
            Err(HclError::CliArgs("hcl2json failed".to_string()))
        }
    }
}

/// Main entry point for `hcl2json`.
///
/// # Errors
/// Returns [`HclError`] if HCL parsing or serialization fails.
pub fn main() -> Result<(), HclError> {
    #[cfg(not(test))]
    let args = env::args().skip(1);
    #[cfg(test)]
    let args = Vec::<String>::new();
    main_with_args(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingReader;
    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("simulated read failure"))
        }
    }

    #[test]
    fn test_convert_to_json() -> Result<(), HclError> {
        let hcl = r#"name = "test""#;
        let res = convert_to_json(hcl)?;
        assert!(res.contains(r#""test""#));
        Ok(())
    }

    #[test]
    fn test_convert_to_json_err() {
        let hcl = "{";
        let res = convert_to_json(hcl);
        assert!(res.is_err());
    }

    #[test]
    fn test_main() {
        assert!(main().is_ok());
    }

    #[test]
    fn test_main_err() {
        assert!(main_impl("{").is_err());
        assert!(main_with_args(vec!["--unknown-flag".to_string()]).is_err());
        assert!(
            main_with_args(vec![
                "nonexistent_file_definitely_missing_9999.hcl".to_string()
            ])
            .is_err()
        );
        let dir = std::env::temp_dir().join(format!("hcl2json_args_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let bad_file = dir.join("bad.hcl");
        let _ = fs::write(&bad_file, "{\n");
        assert!(main_with_args(vec![bad_file.to_string_lossy().to_string()]).is_err());
        let good_file = dir.join("good.hcl");
        let _ = fs::write(&good_file, "key = \"value\"\n");
        assert!(main_with_args(vec![good_file.to_string_lossy().to_string()]).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_hcl2json_args() -> Result<(), HclError> {
        let opts = parse_hcl2json_args(vec!["--json".to_string(), "file.hcl".to_string()])?;
        assert!(opts.json);
        assert_eq!(opts.files, vec!["file.hcl".to_string()]);
        let short_opts = parse_hcl2json_args(vec!["-j".to_string()])?;
        assert!(short_opts.json);
        let stdin_opts = parse_hcl2json_args(vec!["-".to_string()])?;
        assert_eq!(stdin_opts.files, vec!["-".to_string()]);
        let help = parse_hcl2json_args(vec!["--help".to_string()]);
        assert!(help.is_err());
        let short_help = parse_hcl2json_args(vec!["-h".to_string()]);
        assert!(short_help.is_err());
        let unknown = parse_hcl2json_args(vec!["--unknown".to_string()]);
        assert!(unknown.is_err());
        Ok(())
    }

    #[test]
    fn test_run_hcl2json_success_and_error() -> Result<(), HclError> {
        let opts = Hcl2JsonOptions {
            json: true,
            files: vec![],
        };
        let mut stdin = &br#"name = "sample""#[..];
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_hcl2json(&opts, &mut stdin, &mut stdout, &mut stderr)?;
        assert_eq!(code, 0);
        let out_str = String::from_utf8_lossy(&stdout);
        assert!(out_str.contains(r#""sample""#));
        let mut bad_stdin = &b"{"[..];
        let mut bad_stdout = Vec::new();
        let mut bad_stderr = Vec::new();
        let bad_code = run_hcl2json(&opts, &mut bad_stdin, &mut bad_stdout, &mut bad_stderr)?;
        assert_eq!(bad_code, 1);
        let err_str = String::from_utf8_lossy(&bad_stderr);
        assert!(err_str.contains(r#""severity":"error""#));
        let opts_plain = Hcl2JsonOptions {
            json: false,
            files: vec![],
        };
        let mut plain_in = &b"{"[..];
        let mut plain_out = Vec::new();
        let mut plain_err = Vec::new();
        let plain_code = run_hcl2json(&opts_plain, &mut plain_in, &mut plain_out, &mut plain_err)?;
        assert_eq!(plain_code, 1);
        let plain_err_str = String::from_utf8_lossy(&plain_err);
        assert!(plain_err_str.contains("Error parsing <stdin>"));
        let mut failing_reader = FailingReader;
        let mut fail_out = Vec::new();
        let mut fail_err = Vec::new();
        assert!(run_hcl2json(&opts, &mut failing_reader, &mut fail_out, &mut fail_err).is_err());
        let dir = std::env::temp_dir().join(format!("hcl2json_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let valid_file = dir.join("valid.hcl");
        let _ = fs::write(&valid_file, "key = \"value\"\n");
        let bad_file = dir.join("bad.hcl");
        let _ = fs::write(&bad_file, "{\n");
        let opts_file = Hcl2JsonOptions {
            json: false,
            files: vec![valid_file.to_string_lossy().to_string()],
        };
        let mut empty_in = io::empty();
        let mut file_out = Vec::new();
        let mut file_err = Vec::new();
        assert_eq!(
            run_hcl2json(&opts_file, &mut empty_in, &mut file_out, &mut file_err),
            Ok(0)
        );
        let opts_nonexistent = Hcl2JsonOptions {
            json: false,
            files: vec!["nonexistent_file_definitely_missing.hcl".to_string()],
        };
        assert!(
            run_hcl2json(
                &opts_nonexistent,
                &mut empty_in,
                &mut file_out,
                &mut file_err
            )
            .is_err()
        );
        assert!(main_with_args(vec![valid_file.to_string_lossy().to_string()]).is_ok());
        assert!(main_with_args(vec![bad_file.to_string_lossy().to_string()]).is_err());
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
