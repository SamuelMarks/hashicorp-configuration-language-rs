//! Language server executable entry point for `hcl-lsp`.
#![deny(missing_docs)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::error::HclError;
use hcl_lsp::server::LspServer;
#[cfg(not(test))]
use std::env;
use std::io::{self, Read, Write, stderr};

/// Runs the `hcl-lsp` binary logic with explicit argument, input, and output streams.
///
/// # Arguments
/// * `args` - Command-line arguments.
/// * `input` - Stdio input stream.
/// * `output` - Stdio output stream.
///
/// # Errors
/// Returns [`HclError`] if CLI arguments are invalid or server transport fails.
pub fn run_lsp<R: Read, W: Write>(args: &[String], input: R, output: W) -> Result<(), HclError> {
    let mut server = LspServer::new();
    let mut tcp_addr: Option<String> = None;
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--tcp" => {
                if idx + 1 < args.len() {
                    tcp_addr = Some(args[idx + 1].clone());
                    idx += 1;
                } else {
                    return Err(HclError::CliArgs(
                        "Missing address argument for --tcp".to_string(),
                    ));
                }
            }
            "--stdio" => {}
            "-h" | "--help" => {
                let _ = writeln!(stderr(), "Usage: hcl-lsp [--stdio] [--tcp <ip:port>]");
                return Ok(());
            }
            other => {
                return Err(HclError::CliArgs(format!("Unknown argument '{other}'")));
            }
        }
        idx += 1;
    }
    if let Some(addr) = tcp_addr {
        server.run_tcp(&addr)
    } else {
        server.run_stream(input, output)
    }
}

/// Main entry point for `hcl-lsp`.
///
/// # Errors
/// Returns [`HclError`] if the server encounters an unrecoverable failure.
pub fn main() -> Result<(), HclError> {
    #[cfg(not(test))]
    let args: Vec<String> = env::args().skip(1).collect();
    #[cfg(test)]
    let args = vec!["-h".to_string()];
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_lsp(&args, stdin.lock(), stdout.lock())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_help_and_args() {
        let help_args = vec!["--help".to_string()];
        let in_buf = io::empty();
        let mut out_buf = Vec::new();
        assert!(run_lsp(&help_args, in_buf, &mut out_buf).is_ok());
        let short_help_args = vec!["-h".to_string()];
        assert!(run_lsp(&short_help_args, io::empty(), &mut out_buf).is_ok());
        let unknown_args = vec!["--unknown".to_string()];
        assert!(matches!(
            run_lsp(&unknown_args, io::empty(), &mut out_buf),
            Err(HclError::CliArgs(_))
        ));
        let missing_tcp = vec!["--tcp".to_string()];
        assert!(matches!(
            run_lsp(&missing_tcp, io::empty(), &mut out_buf),
            Err(HclError::CliArgs(_))
        ));
        let tcp_args = vec!["--tcp".to_string(), "invalid-addr:99999".to_string()];
        assert!(run_lsp(&tcp_args, io::empty(), &mut out_buf).is_err());
        let stdio_args = vec!["--stdio".to_string()];
        assert!(run_lsp(&stdio_args, io::empty(), &mut out_buf).is_ok());
        let bad_input = io::Cursor::new(
            b"Content-Length: not-a-number

",
        );
        assert!(matches!(
            run_lsp(&stdio_args, bad_input, &mut out_buf),
            Err(HclError::LspProtocol(_))
        ));
        assert!(main().is_ok());
    }
}
