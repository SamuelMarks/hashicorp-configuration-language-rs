//! Interactive Read-Eval-Print Loop (REPL) console for HCL (`hcl-repl`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]
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
use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::eval::evaluator::Evaluator;
use hashicorp_configuration_language_rs::eval::stdlib::stdlib_map;
use hashicorp_configuration_language_rs::parse::parser::Parser;
use hashicorp_configuration_language_rs::types::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
/// An active interactive REPL session maintaining bound variables and command history.
#[derive(Debug, Default)]
pub struct ReplSession {
    /// In-memory variables bound during the session.
    pub variables: HashMap<String, Value>,
    /// Command history log.
    pub history: Vec<String>,
    /// Multiline input buffer for unclosed brackets or multiline expressions.
    pub buffer: String,
}
impl ReplSession {
    /// Creates a new `ReplSession`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Evaluates a line of input or accumulates it if multiline brackets are unclosed.
    ///
    /// # Arguments
    /// * `line` - The raw input line.
    ///
    /// # Errors
    /// Returns an error string if expression parsing or evaluation fails.
    ///
    /// # Returns
    /// * `Ok(Some(output))` - Formatted evaluation result or meta-command response.
    /// * `Ok(None)` - Line buffered waiting for multiline continuation or empty.
    pub fn eval_line(&mut self, line: &str) -> Result<Option<String>, String> {
        let trimmed = line.trim();
        if self.buffer.is_empty() && trimmed.starts_with(':') {
            self.history.push(line.to_string());
            return self.handle_meta_command(trimmed);
        }
        if !self.buffer.is_empty() {
            self.buffer.push('\n');
            self.buffer.push_str(line);
        } else {
            self.buffer = line.to_string();
        }
        if self.is_bracket_balanced(&self.buffer) {
            let complete_input = self.buffer.clone();
            self.buffer.clear();
            let trimmed_complete = complete_input.trim();
            if trimmed_complete.is_empty() {
                return Ok(None);
            }
            self.history.push(trimmed_complete.to_string());
            self.eval_complete_input(trimmed_complete).map(Some)
        } else {
            Ok(None)
        }
    }
    /// Handles REPL meta-commands starting with `:`.
    fn handle_meta_command(&mut self, cmd: &str) -> Result<Option<String>, String> {
        if cmd == ":help" {
            return Ok(
                Some(
                    "Available REPL commands:\n  :set <var> = <expr>  Bind a variable\n  :vars                List all bound variables\n  :type <expr>         Show the inferred type of an expression\n  :funcs | :functions  List all built-in standard library functions\n  :load <file.hcl>     Load an HCL file into the session\n  :reset | :clear      Reset all variable bindings\n  :help                Show this help message\n  :quit | :exit        Exit the REPL"
                        .to_string(),
                ),
            );
        }
        if cmd == ":clear" || cmd == ":reset" {
            self.variables.clear();
            return Ok(Some("Session context reset.".to_string()));
        }
        if cmd == ":vars" {
            if self.variables.is_empty() {
                return Ok(Some("No variables defined.".to_string()));
            }
            let mut keys: Vec<&String> = self.variables.keys().collect();
            keys.sort();
            let mut out = format!("Bound variables ({}):\n", keys.len());
            for k in keys {
                let v = &self.variables[k];
                out.push_str(&format!("  {k}: {} = {v}\n", v.ty()));
            }
            return Ok(Some(out.trim_end().to_string()));
        }
        if cmd == ":functions" || cmd == ":funcs" {
            let mut fn_names: Vec<&String> = stdlib_map().keys().collect();
            fn_names.sort();
            let mut out = format!("Available functions ({}):\n", fn_names.len());
            for name in fn_names {
                let fn_obj = &stdlib_map()[name];
                let sig_str = fn_obj.signature.as_ref().map_or_else(
                    || format!("{name}()"),
                    |s| {
                        let params = s
                            .params
                            .iter()
                            .map(|p| format!("{}: {}", p.name, p.param_type))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{name}({params})")
                    },
                );
                out.push_str(&format!("  {sig_str}\n"));
            }
            return Ok(Some(out.trim_end().to_string()));
        }
        if let Some(rest) = cmd.strip_prefix(":set ") {
            let (var_name, expr_str) = rest
                .split_once('=')
                .ok_or_else(|| "Usage: :set <var> = <expr>".to_string())?;
            let var_name = var_name.trim();
            let val = self.eval_expr(expr_str.trim())?;
            self.variables.insert(var_name.to_string(), val.clone());
            return Ok(Some(format!("{var_name} = {val}")));
        }
        if let Some(expr_str) = cmd.strip_prefix(":type ") {
            let val = self.eval_expr(expr_str.trim())?;
            return Ok(Some(format!("{}: {}", val.ty(), val)));
        }
        if let Some(file_path) = cmd.strip_prefix(":load ") {
            let path = file_path.trim();
            let content =
                fs::read_to_string(path).map_err(|e| format!("Failed to read '{path}': {e}"))?;
            let mut parser = Parser::new(&content);
            let body = parser.parse_body();
            if parser.errors().has_errors() {
                return Err(format!("Parse error in '{path}': {}", parser.errors()));
            }
            let mut ctx = Context::with_stdlib();
            for (k, v) in &self.variables {
                ctx.set_variable(k.clone(), v.clone());
            }
            let mut count = 0;
            for (k, attr) in &body.attributes {
                let eval = Evaluator::new(&ctx);
                if let Ok((val, _)) = eval.evaluate(&attr.expr) {
                    ctx.set_variable(k.clone(), val.clone());
                    self.variables.insert(k.clone(), val);
                    count += 1;
                }
            }
            return Ok(Some(format!("Loaded {count} attributes from '{path}'.")));
        }
        if cmd == ":quit" || cmd == ":exit" {
            return Ok(Some("Goodbye!".to_string()));
        }
        Err(format!("Unknown command '{cmd}'. Type :help for guidance."))
    }
    /// Evaluates complete non-command HCL input.
    fn eval_complete_input(&mut self, input: &str) -> Result<String, String> {
        if let Some((lhs, rhs)) = input.split_once('=') {
            let lhs_trimmed = lhs.trim();
            if !lhs_trimmed.is_empty()
                && lhs_trimmed.chars().all(|c| c.is_alphanumeric() || c == '_')
                && !rhs.starts_with('=')
            {
                let val = self.eval_expr(rhs.trim())?;
                self.variables.insert(lhs_trimmed.to_string(), val.clone());
                return Ok(format!("{lhs_trimmed} = {val}"));
            }
        }
        let val = self.eval_expr(input)?;
        Ok(format!("{val}"))
    }
    /// Evaluates an expression string in the current session context.
    ///
    /// # Arguments
    /// * `expr_str` - Expression text.
    ///
    /// # Errors
    /// Returns error string if evaluation fails.
    pub fn eval_expr(&self, expr_str: &str) -> Result<Value, String> {
        let mut parser = Parser::new(expr_str);
        let expr = parser
            .parse_expression()
            .ok_or_else(|| "Failed to parse expression".to_string())?;
        let mut ctx = Context::with_stdlib();
        for (k, v) in &self.variables {
            ctx.set_variable(k.clone(), v.clone());
        }
        let evaluator = Evaluator::new(&ctx);
        let (val, _diags) = evaluator
            .evaluate(&expr)
            .map_err(|e| format!("Evaluation error: {e}"))?;
        Ok(val)
    }
    /// Returns `true` if all parentheses, braces, and brackets are balanced.
    fn is_bracket_balanced(&self, text: &str) -> bool {
        let mut paren_count = 0i32;
        let mut brace_count = 0i32;
        let mut brack_count = 0i32;
        let mut in_quote = false;
        let mut is_escaped = false;
        for ch in text.chars() {
            if is_escaped {
                is_escaped = false;
                continue;
            }
            if ch == '\\' {
                is_escaped = true;
                continue;
            }
            if ch == '"' {
                in_quote = !in_quote;
                continue;
            }
            if in_quote {
                continue;
            }
            match ch {
                '(' => paren_count += 1,
                ')' => paren_count -= 1,
                '{' => brace_count += 1,
                '}' => brace_count -= 1,
                '[' => brack_count += 1,
                ']' => brack_count -= 1,
                _ => {}
            }
        }
        paren_count <= 0 && brace_count <= 0 && brack_count <= 0 && !in_quote
    }
    /// Provides auto-completion candidates for a given prefix string.
    ///
    /// Matches against meta-commands, language keywords, bound variables, and standard library functions.
    ///
    /// # Arguments
    /// * `prefix` - The text prefix to match.
    #[must_use]
    pub fn complete_token(&self, prefix: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        if prefix.starts_with(':') {
            let commands = [
                ":clear",
                ":exit",
                ":funcs",
                ":functions",
                ":help",
                ":load",
                ":quit",
                ":reset",
                ":set",
                ":type",
                ":vars",
            ];
            for cmd in commands {
                if cmd.starts_with(prefix) {
                    candidates.push((*cmd).to_string());
                }
            }
            return candidates;
        }
        for kw in ["false", "null", "true"] {
            if kw.starts_with(prefix) {
                candidates.push((*kw).to_string());
            }
        }
        for var in self.variables.keys() {
            if var.starts_with(prefix) {
                candidates.push(var.clone());
            }
        }
        for fn_name in stdlib_map().keys() {
            if fn_name.starts_with(prefix) {
                candidates.push(fn_name.clone());
            }
        }
        candidates.sort();
        candidates.dedup();
        candidates
    }
    /// Runs the interactive loop with given reader and writer streams.
    ///
    /// # Arguments
    /// * `reader` - Input reader.
    /// * `writer` - Output writer.
    ///
    /// # Errors
    /// Returns error string if an unrecoverable I/O error occurs.
    pub fn run(&mut self, reader: &mut dyn BufRead, writer: &mut dyn Write) -> Result<(), String> {
        let _ = writeln!(
            writer,
            "HashiCorp Configuration Language (HCL) REPL Console"
        );
        let _ = writeln!(
            writer,
            "Type :help for commands or :exit to quit.
"
        );
        loop {
            let prompt = if self.buffer.is_empty() { "> " } else { "... " };
            let _ = write!(writer, "{prompt}");
            let _ = writer.flush();
            let mut line = String::new();
            let bytes = reader
                .read_line(&mut line)
                .map_err(|e| format!("I/O error: {e}"))?;
            if bytes == 0 {
                break;
            }
            match self.eval_line(&line) {
                Ok(Some(out)) => {
                    if out == "Goodbye!" {
                        let _ = writeln!(writer, "{out}");
                        break;
                    }
                    let _ = writeln!(writer, "{out}");
                }
                Ok(None) => {}
                Err(err) => {
                    let _ = writeln!(writer, "Error: {err}");
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
thread_local! {
    static TEST_INPUT : std::cell::RefCell < Option < Vec < u8 >>> = const {
    std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn set_test_input(input: &[u8]) {
    TEST_INPUT.with(|i| *i.borrow_mut() = Some(input.to_vec()));
}
/// Main entry point for `hcl-repl`.
///
/// # Errors
/// Returns an error message if the interactive REPL encounters an unrecoverable failure.
pub fn main() -> Result<(), String> {
    #[cfg(not(test))]
    let mut stdin = io::stdin().lock();
    #[cfg(test)]
    let mut stdin = std::io::Cursor::new(TEST_INPUT.with(|i| match i.borrow_mut().take() {
        Some(bytes) => bytes,
        None => b":exit\n".to_vec(),
    }));
    let mut stdout = io::stdout();
    let mut session = ReplSession::new();
    session.run(&mut stdin, &mut stdout)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};
    /// Mock reader that always fails on read operations.
    struct FailingReader;
    impl std::io::Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("mock io error"))
        }
    }
    impl BufRead for FailingReader {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            Err(std::io::Error::other("mock io error"))
        }
        fn consume(&mut self, _amt: usize) {}
    }
    #[test]
    fn test_repl_expression_eval() {
        let mut session = ReplSession::new();
        let res = session.eval_line("1 + 2 * 3").unwrap();
        assert_eq!(res, Some("7".to_string()));
        let res_fn = session.eval_line("upper(\"hello\")").unwrap();
        assert_eq!(res_fn, Some("\"HELLO\"".to_string()));
        let res_assign = session.eval_line("foo = 100").unwrap();
        assert_eq!(res_assign, Some("foo = 100".to_string()));
        let res_ref = session.eval_line("foo + 25").unwrap();
        assert_eq!(res_ref, Some("125".to_string()));
        let res_eq = session.eval_line("foo == 100").unwrap();
        assert_eq!(res_eq, Some("true".to_string()));
        assert!(session.eval_line("= 5").is_err());
        assert!(session.eval_line("== 100").is_err());
        assert!(session.eval_line("bar = = 10").is_err());
        assert!(session.eval_line("a + b = 10").is_err());
        let err_parse = session.eval_line("1 * / 2");
        assert!(err_parse.is_err());
        let err_eval = session.eval_line("undefined_variable + 1");
        assert!(err_eval.is_err());
        let res_empty = session.eval_line("").unwrap();
        assert!(res_empty.is_none());
        let res_spaces = session.eval_line("   ").unwrap();
        assert!(res_spaces.is_none());
    }
    #[test]
    fn test_repl_meta_commands() {
        let mut session = ReplSession::new();
        let help = session.eval_line(":help").unwrap();
        assert!(
            help.as_deref()
                .is_some_and(|s| s.contains("Available REPL commands"))
        );
        let fns = session.eval_line(":functions").unwrap();
        assert!(fns.as_deref().is_some_and(|s| s.contains("abs(")));
        let set_res = session.eval_line(":set x = 42").unwrap();
        assert_eq!(set_res, Some("x = 42".to_string()));
        assert_eq!(
            session.variables.get("x").map(|v| v.to_string()),
            Some("42".to_string())
        );
        let set_err = session.eval_line(":set invalid_no_equals");
        assert!(set_err.is_err());
        assert!(
            set_err
                .err()
                .unwrap()
                .contains("Usage: :set <var> = <expr>")
        );
        let set_eval_err = session.eval_line(":set x = undefined_eval_variable_xyz");
        assert!(set_eval_err.is_err());
        let type_res = session.eval_line(":type x").unwrap();
        assert_eq!(type_res, Some("number: 42".to_string()));
        let type_err = session.eval_line(":type undefined_xyz");
        assert!(type_err.is_err());
        let clear_res = session.eval_line(":clear").unwrap();
        assert!(clear_res.as_deref().is_some_and(|s| s.contains("reset")));
        assert!(session.variables.is_empty());
        let vars_empty = session.eval_line(":vars").unwrap();
        assert_eq!(vars_empty, Some("No variables defined.".to_string()));
        let _ = session.eval_line("my_var = 123").unwrap();
        let vars_populated = session.eval_line(":vars").unwrap();
        assert!(
            vars_populated
                .as_deref()
                .is_some_and(|s| s.contains("my_var: number = 123"))
        );
        let funcs_res = session.eval_line(":funcs").unwrap();
        assert!(funcs_res.as_deref().is_some_and(|s| s.contains("abs(")));
        let reset_res = session.eval_line(":reset").unwrap();
        assert!(reset_res.as_deref().is_some_and(|s| s.contains("reset")));
        assert!(session.variables.is_empty());
        assert!(session.complete_token(":v").contains(&":vars".to_string()));
        assert!(session.complete_token(":f").contains(&":funcs".to_string()));
        assert!(session.complete_token("tr").contains(&"true".to_string()));
        assert!(
            session
                .complete_token("sub")
                .contains(&"substr".to_string())
        );
        let _ = session.eval_line("custom_val = 999").unwrap();
        assert!(
            session
                .complete_token("custom")
                .contains(&"custom_val".to_string())
        );
        assert!(
            !session
                .complete_token("nonexistent")
                .contains(&"custom_val".to_string())
        );
        let quit_res = session.eval_line(":quit").unwrap();
        assert_eq!(quit_res, Some("Goodbye!".to_string()));
        let exit_res = session.eval_line(":exit").unwrap();
        assert_eq!(exit_res, Some("Goodbye!".to_string()));
        let unknown_res = session.eval_line(":unknown_meta");
        assert!(unknown_res.is_err());
        assert!(
            unknown_res
                .err()
                .unwrap()
                .contains("Unknown command ':unknown_meta'")
        );
    }
    #[test]
    fn test_repl_meta_command_load() {
        let dir = std::env::temp_dir().join(format!("hcl_repl_load_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let mut session = ReplSession::new();
        let res_missing =
            session.eval_line(&format!(":load {}", dir.join("missing.hcl").display()));
        assert!(res_missing.is_err());
        assert!(res_missing.err().unwrap().contains("Failed to read"));
        let bad_file = dir.join("bad.hcl");
        fs::write(&bad_file, "{ invalid syntax").unwrap();
        let res_bad = session.eval_line(&format!(":load {}", bad_file.display()));
        assert!(res_bad.is_err());
        assert!(res_bad.err().unwrap().contains("Parse error in"));
        session.eval_line("initial = 50").unwrap();
        let good_file = dir.join("good.hcl");
        fs::write(
            &good_file,
            "item_a = initial + 10\nitem_b = 15\nitem_bad = undefined_foo + 1\n",
        )
        .unwrap();
        let res_good = session
            .eval_line(&format!(":load {}", good_file.display()))
            .unwrap();
        assert!(
            res_good
                .as_deref()
                .is_some_and(|s| s.contains("Loaded 2 attributes"))
        );
        assert_eq!(
            session.variables.get("item_a").map(|v| v.to_string()),
            Some("60".to_string())
        );
        assert_eq!(
            session.variables.get("item_b").map(|v| v.to_string()),
            Some("15".to_string())
        );
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn test_repl_multiline_and_brackets() {
        let mut session = ReplSession::new();
        let l1 = session.eval_line("[\n").unwrap();
        assert!(l1.is_none());
        let l_empty = session.eval_line("   \n").unwrap();
        assert!(l_empty.is_none());
        let l2 = session.eval_line("  1,\n").unwrap();
        assert!(l2.is_none());
        let l3 = session.eval_line("]\n").unwrap();
        assert_eq!(l3, Some("[1]".to_string()));
        assert!(session.is_bracket_balanced("\"escaped \\\" quote\""));
        assert!(session.is_bracket_balanced("\"escaped \\\\ backslash\""));
        assert!(session.is_bracket_balanced("{ a = 1 }"));
        assert!(!session.is_bracket_balanced("\"unclosed quote"));
        assert!(!session.is_bracket_balanced("(unclosed paren"));
        assert!(!session.is_bracket_balanced("{ unclosed brace"));
        assert!(!session.is_bracket_balanced("[ unclosed bracket"));
    }
    #[test]
    fn test_repl_run_loop_and_errors() {
        let mut session = ReplSession::new();
        let input = "[\n1\n]\nundefined_var + 1\n\n:exit\n";
        let mut reader = Cursor::new(input);
        let mut writer = Vec::new();
        session.run(&mut reader, &mut writer).unwrap();
        let out_str = String::from_utf8_lossy(&writer);
        assert!(out_str.contains("[1]"));
        assert!(out_str.contains("Error:"));
        assert!(out_str.contains("Goodbye!"));
        let mut session2 = ReplSession::new();
        let input_eof = "1 + 1\n";
        let mut reader_eof = Cursor::new(input_eof);
        let mut writer_eof = Vec::new();
        session2.run(&mut reader_eof, &mut writer_eof).unwrap();
        let out_eof = String::from_utf8_lossy(&writer_eof);
        assert!(out_eof.contains("2"));
        let mut failing = FailingReader;
        let mut fail_buf = [0u8; 1];
        let _ = failing.read(&mut fail_buf);
        failing.consume(0);
        let mut fail_writer = Vec::new();
        let mut session3 = ReplSession::new();
        let res_fail = session3.run(&mut failing, &mut fail_writer);
        assert!(res_fail.is_err());
        assert!(res_fail.err().unwrap().contains("I/O error: mock io error"));
    }
    #[test]
    fn test_main_function() {
        set_test_input(b"[\n1\n]\nundefined_var + 1\n\n:exit\n");
        assert!(main().is_ok());
        set_test_input(b"2 * 3\n");
        assert!(main().is_ok());
        assert!(main().is_ok());
    }
    #[test]
    fn test_repl_session_derives() {
        let session = ReplSession::default();
        assert!(format!("{session:?}").contains("ReplSession"));
        assert!(session.variables.is_empty());
        assert!(session.history.is_empty());
        assert!(session.buffer.is_empty());
    }
}
