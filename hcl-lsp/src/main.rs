//! Language server executable entry point for `hcl-lsp`.
#![deny(missing_docs)]
#![allow(clippy::use_self)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::suspicious_operation_groupings)]
#![allow(clippy::needless_collect)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::iter_on_single_items)]
#![allow(clippy::trivial_regex)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::similar_names)]
#![allow(clippy::if_not_else)]
#![allow(clippy::format_push_string)]
#![allow(clippy::unused_self)]
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::equatable_if_let)]
#![allow(clippy::branches_sharing_code)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::useless_let_if_seq)]
#![allow(clippy::collection_is_never_read)]
#![allow(clippy::literal_string_with_formatting_args)]
#![allow(clippy::string_lit_as_bytes)]
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
/// Returns an error message on failure.
pub fn run_lsp<R: Read, W: Write>(args: &[String], input: R, output: W) -> Result<(), String> {
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
                    return Err("Missing address argument for --tcp".to_string());
                }
            }
            "--stdio" => {}
            "-h" | "--help" => {
                let _ = writeln!(stderr(), "Usage: hcl-lsp [--stdio] [--tcp <ip:port>]");
                return Ok(());
            }
            other => {
                return Err(format!("Unknown argument '{other}'"));
            }
        }
        idx += 1;
    }
    if let Some(addr) = tcp_addr {
        server.run_tcp(&addr).map_err(|e| e.to_string())
    } else {
        server.run_stream(input, output).map_err(|e| e.to_string())
    }
}
/// Main entry point for `hcl-lsp`.
///
/// # Errors
/// Returns an error message if the server encounters an unrecoverable failure.
pub fn main() -> Result<(), String> {
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
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::pedantic,
        clippy::nursery,
        clippy::collection_is_never_read
    )]
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
        assert!(run_lsp(&unknown_args, io::empty(), &mut out_buf).is_err());
        let missing_tcp = vec!["--tcp".to_string()];
        assert!(run_lsp(&missing_tcp, io::empty(), &mut out_buf).is_err());
        let tcp_args = vec!["--tcp".to_string(), "invalid-addr:99999".to_string()];
        assert!(run_lsp(&tcp_args, io::empty(), &mut out_buf).is_err());
        let stdio_args = vec!["--stdio".to_string()];
        assert!(run_lsp(&stdio_args, io::empty(), &mut out_buf).is_ok());
        let bad_input = io::Cursor::new(b"Content-Length: not-a-number\r\n\r\n");
        assert!(run_lsp(&stdio_args, bad_input, &mut out_buf).is_err());
        assert!(main().is_ok());
    }
}
