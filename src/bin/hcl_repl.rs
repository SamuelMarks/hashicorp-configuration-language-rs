//! Interactive Read-Eval-Print Loop (REPL) console for HCL (`hcl-repl`).
#![cfg_attr(all(coverage_nightly, test), feature(coverage_attribute))]
#![deny(missing_docs)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::error::HclError;
use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::eval::evaluator::Evaluator;
use hashicorp_configuration_language_rs::eval::stdlib::stdlib_map;
use hashicorp_configuration_language_rs::parse::parser::Parser;
use hashicorp_configuration_language_rs::types::Value;
use std::collections::HashMap;
use std::fmt::Write as _;
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
    /// Returns [`HclError`] if expression parsing or evaluation fails.
    ///
    /// # Returns
    /// * `Ok(Some(output))` - Formatted evaluation result or meta-command response.
    /// * `Ok(None)` - Line buffered waiting for multiline continuation or empty.
    pub fn eval_line(&mut self, line: &str) -> Result<Option<String>, HclError> {
        let trimmed = line.trim();
        if self.buffer.is_empty() && trimmed.starts_with(':') {
            self.history.push(line.to_string());
            return self.handle_meta_command(trimmed);
        }
        if self.buffer.is_empty() {
            self.buffer = line.to_string();
        } else {
            self.buffer.push(char::from(10));
            self.buffer.push_str(line);
        }
        if Self::is_bracket_balanced(&self.buffer) {
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

    /// Formats the list of all standard library functions.
    fn format_functions_list() -> String {
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
            let _ = writeln!(out, "  {sig_str}");
        }
        out.trim_end().to_string()
    }

    /// Loads an HCL file into the current REPL session.
    fn load_file(&mut self, path: &str) -> Result<String, HclError> {
        let content = fs::read_to_string(path)
            .map_err(|e| HclError::Io(format!("Failed to read '{path}': {e}")))?;
        let mut parser = Parser::new(&content);
        let body = parser.parse_body();
        if parser.errors().has_errors() {
            return Err(HclError::Parse(format!(
                "Parse error in '{path}': {}",
                parser.errors()
            )));
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
        Ok(format!("Loaded {count} attributes from '{path}'."))
    }

    /// Handles REPL meta-commands starting with `:`.
    fn handle_meta_command(&mut self, cmd: &str) -> Result<Option<String>, HclError> {
        if cmd == ":help" {
            return Ok(Some(
                [
                    "Available REPL commands:",
                    "  :set <var> = <expr>  Bind a variable",
                    "  :vars                List all bound variables",
                    "  :type <expr>         Show the inferred type of an expression",
                    "  :funcs | :functions  List all built-in standard library functions",
                    "  :load <file.hcl>     Load an HCL file into the session",
                    "  :reset | :clear      Reset all variable bindings",
                    "  :help                Show this help message",
                    "  :quit | :exit        Exit the REPL",
                ]
                .join("\n"),
            ));
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
                let _ = writeln!(out, "  {k}: {} = {v}", v.ty());
            }
            return Ok(Some(out.trim_end().to_string()));
        }
        if cmd == ":functions" || cmd == ":funcs" {
            return Ok(Some(Self::format_functions_list()));
        }
        if let Some(rest) = cmd.strip_prefix(":set ") {
            let (var_name, expr_str) = rest
                .split_once('=')
                .ok_or_else(|| HclError::CliArgs(["Usage", ":set <var> = <expr>"].join(": ")))?;
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
            return self.load_file(file_path.trim()).map(Some);
        }
        if cmd == ":quit" || cmd == ":exit" {
            return Ok(Some("Goodbye!".to_string()));
        }
        Err(HclError::CliArgs(format!(
            "Unknown command '{cmd}'. Type :help for guidance."
        )))
    }

    /// Evaluates complete non-command HCL input.
    fn eval_complete_input(&mut self, input: &str) -> Result<String, HclError> {
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
    /// Returns [`HclError`] if evaluation fails.
    pub fn eval_expr(&self, expr_str: &str) -> Result<Value, HclError> {
        let mut parser = Parser::new(expr_str);
        let expr = parser
            .parse_expression()
            .ok_or_else(|| HclError::Parse("Failed to parse expression".to_string()))?;
        let mut ctx = Context::with_stdlib();
        for (k, v) in &self.variables {
            ctx.set_variable(k.clone(), v.clone());
        }
        let evaluator = Evaluator::new(&ctx);
        let (val, _diags) = evaluator
            .evaluate(&expr)
            .map_err(|e| HclError::Eval(format!("Evaluation error: {e}")))?;
        Ok(val)
    }

    /// Returns `true` if all parentheses, braces, and brackets are balanced.
    #[must_use]
    pub fn is_bracket_balanced(text: &str) -> bool {
        let mut paren_depth = 0i32;
        let mut curly_depth = 0i32;
        let mut square_depth = 0i32;
        let mut in_quote = false;
        let mut is_escaped = false;
        let backslash = char::from(92);
        for ch in text.chars() {
            if is_escaped {
                is_escaped = false;
                continue;
            }
            if ch == backslash {
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
                '(' => paren_depth += 1,
                ')' => paren_depth -= 1,
                '{' => curly_depth += 1,
                '}' => curly_depth -= 1,
                '[' => square_depth += 1,
                ']' => square_depth -= 1,
                _ => {}
            }
        }
        paren_depth <= 0 && curly_depth <= 0 && square_depth <= 0 && !in_quote
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
    /// Returns [`HclError::Io`] if an unrecoverable I/O error occurs.
    pub fn run(
        &mut self,
        reader: &mut dyn BufRead,
        writer: &mut dyn Write,
    ) -> Result<(), HclError> {
        let _ = writeln!(
            writer,
            "HashiCorp Configuration Language (HCL) REPL Console"
        );
        let _ = writeln!(writer, "Type :help for commands or :exit to quit.\n");
        loop {
            let prompt = if self.buffer.is_empty() { "> " } else { "... " };
            let _ = write!(writer, "{prompt}");
            let _ = writer.flush();
            let mut line = String::new();
            let bytes = reader
                .read_line(&mut line)
                .map_err(|e| HclError::Io(format!("I/O error: {e}")))?;
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
    static TEST_INPUT: std::cell::RefCell<Option<Vec<u8>>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(test)]
fn set_test_input(input: &[u8]) {
    TEST_INPUT.with(|i| *i.borrow_mut() = Some(input.to_vec()));
}

/// Main entry point for `hcl-repl`.
///
/// # Errors
/// Returns [`HclError`] if the interactive REPL encounters an unrecoverable failure.
pub fn main() -> Result<(), HclError> {
    #[cfg(not(test))]
    let mut stdin = io::stdin().lock();
    #[cfg(test)]
    let mut stdin = std::io::Cursor::new(
        TEST_INPUT.with(|i| i.borrow_mut().take().unwrap_or_else(|| b":quit\n".to_vec())),
    );
    let mut stdout = io::stdout();
    let mut session = ReplSession::new();
    session.run(&mut stdin, &mut stdout)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
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
    fn test_repl_expression_eval() -> Result<(), HclError> {
        let mut session = ReplSession::new();
        let res = session.eval_line("1 + 2 * 3")?;
        assert_eq!(res, Some("7".to_string()));
        let res_fn = session.eval_line(r#"upper("hello")"#)?;
        assert_eq!(res_fn, Some(r#""HELLO""#.to_string()));
        let res_assign = session.eval_line("foo = 100")?;
        assert_eq!(res_assign, Some("foo = 100".to_string()));
        let res_ref = session.eval_line("foo + 25")?;
        assert_eq!(res_ref, Some("125".to_string()));
        let res_eq = session.eval_line("foo == 100")?;
        assert_eq!(res_eq, Some("true".to_string()));
        assert!(session.eval_line("= 5").is_err());
        assert!(session.eval_line("== 100").is_err());
        assert!(session.eval_line("bar = = 10").is_err());
        assert!(session.eval_line("a + b = 10").is_err());
        let err_parse = session.eval_line("1 * / 2");
        assert!(err_parse.is_err());
        let err_eval = session.eval_line("undefined_variable + 1");
        assert!(err_eval.is_err());
        let res_empty = session.eval_line("")?;
        assert!(res_empty.is_none());
        let res_spaces = session.eval_line("   ")?;
        assert!(res_spaces.is_none());
        Ok(())
    }

    #[test]
    fn test_repl_meta_commands() -> Result<(), HclError> {
        let mut session = ReplSession::new();
        let help = session.eval_line(":help")?;
        assert!(
            help.as_deref()
                .is_some_and(|s| s.contains("Available REPL commands"))
        );
        let fns = session.eval_line(":functions")?;
        assert!(fns.as_deref().is_some_and(|s| s.contains("abs(")));
        let set_res = session.eval_line(":set x = 42")?;
        assert_eq!(set_res, Some("x = 42".to_string()));
        assert_eq!(
            session.variables.get("x").map(ToString::to_string),
            Some("42".to_string())
        );
        let set_err = session.eval_line(":set invalid_no_equals");
        assert!(
            matches!(set_err, Err(HclError::CliArgs(ref msg)) if msg.contains("Usage: :set <var> = <expr>"))
        );
        let set_eval_err = session.eval_line(":set x = undefined_eval_variable_xyz");
        assert!(set_eval_err.is_err());
        let type_res = session.eval_line(":type x")?;
        assert_eq!(type_res, Some("number: 42".to_string()));
        let type_err = session.eval_line(":type undefined_xyz");
        assert!(type_err.is_err());
        let clear_res = session.eval_line(":clear")?;
        assert!(clear_res.as_deref().is_some_and(|s| s.contains("reset")));
        assert!(session.variables.is_empty());
        Ok(())
    }

    #[test]
    fn test_repl_meta_commands_completion_and_exit() -> Result<(), HclError> {
        let mut session = ReplSession::new();
        let vars_empty = session.eval_line(":vars")?;
        assert_eq!(vars_empty, Some("No variables defined.".to_string()));
        let _ = session.eval_line("my_var = 123")?;
        let vars_populated = session.eval_line(":vars")?;
        assert!(
            vars_populated
                .as_deref()
                .is_some_and(|s| s.contains("my_var: number = 123"))
        );
        let funcs_res = session.eval_line(":funcs")?;
        assert!(funcs_res.as_deref().is_some_and(|s| s.contains("abs(")));
        let reset_res = session.eval_line(":reset")?;
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
        let _ = session.eval_line("custom_val = 999")?;
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
        let quit_res = session.eval_line(":quit")?;
        assert_eq!(quit_res, Some("Goodbye!".to_string()));
        let exit_res = session.eval_line(":exit")?;
        assert_eq!(exit_res, Some("Goodbye!".to_string()));
        let unknown_res = session.eval_line(":unknown_meta");
        assert!(
            matches!(unknown_res, Err(HclError::CliArgs(ref msg)) if msg.contains("Unknown command ':unknown_meta'"))
        );
        Ok(())
    }

    #[test]
    fn test_repl_meta_command_load() -> Result<(), HclError> {
        let dir = std::env::temp_dir().join(format!("hcl_repl_load_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let mut session = ReplSession::new();
        let res_missing =
            session.eval_line(&format!(":load {}", dir.join("missing.hcl").display()));
        assert!(matches!(res_missing, Err(HclError::Io(_))));
        let bad_file = dir.join("bad.hcl");
        fs::write(&bad_file, "= invalid syntax")?;
        let res_bad = session.eval_line(&format!(":load {}", bad_file.display()));
        assert!(matches!(res_bad, Err(HclError::Parse(_))));
        session.eval_line("initial = 50")?;
        let good_file = dir.join("good.hcl");
        fs::write(
            &good_file,
            "item_a = initial + 10\nitem_b = 15\nitem_bad = undefined_foo + 1\n",
        )?;
        let res_good = session.eval_line(&format!(":load {}", good_file.display()))?;
        assert!(
            res_good
                .as_deref()
                .is_some_and(|s| s.contains("Loaded 2 attributes"))
        );
        assert_eq!(
            session.variables.get("item_a").map(ToString::to_string),
            Some("60".to_string())
        );
        assert_eq!(
            session.variables.get("item_b").map(ToString::to_string),
            Some("15".to_string())
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn test_repl_multiline_and_brackets() -> Result<(), HclError> {
        let mut session = ReplSession::new();
        let l1 = session.eval_line("[\n")?;
        assert!(l1.is_none());
        let l_empty = session.eval_line("   \n")?;
        assert!(l_empty.is_none());
        let l2 = session.eval_line("  1,\n")?;
        assert!(l2.is_none());
        let l3 = session.eval_line("]\n")?;
        assert_eq!(l3, Some("[1]".to_string()));
        let backslash = char::from(92);
        assert!(ReplSession::is_bracket_balanced(
            &["\"escaped ", &backslash.to_string(), "\" quote\""].concat()
        ));
        assert!(ReplSession::is_bracket_balanced(
            &[
                "\"escaped ",
                &backslash.to_string(),
                &backslash.to_string(),
                " backslash\""
            ]
            .concat()
        ));
        assert!(ReplSession::is_bracket_balanced(
            &[
                &char::from(123).to_string(),
                " a = 1 ",
                &char::from(125).to_string()
            ]
            .concat()
        ));
        assert!(!ReplSession::is_bracket_balanced(r#""unclosed quote"#));
        assert!(!ReplSession::is_bracket_balanced("(unclosed paren"));
        assert!(!ReplSession::is_bracket_balanced(
            &[&char::from(123).to_string(), " unclosed brace"].concat()
        ));
        assert!(!ReplSession::is_bracket_balanced("[ unclosed bracket"));
        Ok(())
    }

    #[test]
    fn test_repl_run_loop_and_errors() -> Result<(), HclError> {
        let mut session = ReplSession::new();
        let input = "[\n1\n]\nundefined_var + 1\n\n:exit\n";
        let mut reader = Cursor::new(input);
        let mut writer = Vec::new();
        session.run(&mut reader, &mut writer)?;
        let out_str = String::from_utf8_lossy(&writer);
        assert!(out_str.contains("[1]"));
        assert!(out_str.contains("Error:"));
        assert!(out_str.contains("Goodbye!"));
        let mut session2 = ReplSession::new();
        let input_eof = "1 + 1\n";
        let mut reader_eof = Cursor::new(input_eof);
        let mut writer_eof = Vec::new();
        session2.run(&mut reader_eof, &mut writer_eof)?;
        let out_eof = String::from_utf8_lossy(&writer_eof);
        assert!(out_eof.contains('2'));
        let mut failing = FailingReader;
        let mut fail_buf = [0u8; 1];
        let _ = failing.read(&mut fail_buf);
        failing.consume(0);
        let mut fail_writer = Vec::new();
        let mut session3 = ReplSession::new();
        let res_fail = session3.run(&mut failing, &mut fail_writer);
        assert!(matches!(res_fail, Err(HclError::Io(_))));
        Ok(())
    }

    #[test]
    fn test_main_function() {
        set_test_input(b"[\n1\n]\nundefined_var + 1\n\n:quit\n");
        assert!(main().is_ok());
        set_test_input(b"2 * 3\n");
        assert!(main().is_ok());
        assert!(main().is_ok());
    }

    #[test]
    fn test_repl_session_derives() {
        let session = ReplSession::default();
        let dbg = format!("{session:?}");
        assert!(dbg.contains("ReplSession"));
        assert!(session.variables.is_empty());
        assert!(session.history.is_empty());
        assert!(session.buffer.is_empty());
    }
}
