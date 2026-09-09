//! Interactive Read-Eval-Print Loop (REPL) console for HCL (`hcl-repl`).

#![deny(missing_docs)]

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

        // 1. Meta-commands (only evaluated when buffer is empty)
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
            return Ok(Some(
                "Available REPL commands:
  :set <var> = <expr>  Bind a variable
  :type <expr>         Show the inferred type of an expression
  :functions           List all built-in standard library functions
  :load <file.hcl>     Load an HCL file into the session
  :clear               Reset all variable bindings
  :help                Show this help message
  :quit | :exit        Exit the REPL"
                    .to_string(),
            ));
        }

        if cmd == ":clear" {
            self.variables.clear();
            return Ok(Some("Session context reset.".to_string()));
        }

        if cmd == ":functions" {
            let mut fn_names: Vec<&String> = stdlib_map().keys().collect();
            fn_names.sort();
            let mut out = format!(
                "Available functions ({}):
",
                fn_names.len()
            );
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
                out.push_str(&format!(
                    "  {sig_str}
"
                ));
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
        // Check if input is an assignment: `var_name = expr`
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

        // Otherwise evaluate as expression
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
                break; // Clean EOF
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
    static TEST_INPUT: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) };
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

        // Simple arithmetic
        let res = session.eval_line("1 + 2 * 3").expect("eval ok");
        assert_eq!(res, Some("7".to_string()));

        // Function call
        let res_fn = session.eval_line("upper(\"hello\")").expect("eval fn ok");
        assert_eq!(res_fn, Some("\"HELLO\"".to_string()));

        // Assignment
        let res_assign = session.eval_line("foo = 100").expect("assign ok");
        assert_eq!(res_assign, Some("foo = 100".to_string()));

        // Reference assigned var
        let res_ref = session.eval_line("foo + 25").expect("ref ok");
        assert_eq!(res_ref, Some("125".to_string()));

        // Equality check with == falls back to eval_expr
        let res_eq = session.eval_line("foo == 100").expect("eq ok");
        assert_eq!(res_eq, Some("true".to_string()));

        // Assignment starting with = falls back to eval_expr
        assert!(session.eval_line("= 5").is_err());

        // Assignment starting with == falls back to eval_expr
        assert!(session.eval_line("== 100").is_err());

        // Assignment with rhs starting with =
        assert!(session.eval_line("bar = = 10").is_err());

        // Invalid lhs with operators
        assert!(session.eval_line("a + b = 10").is_err());

        // Parse error in expression (balanced brackets but invalid syntax)
        let err_parse = session.eval_line("1 * / 2");
        assert!(err_parse.is_err());

        // Evaluation error (unknown var)
        let err_eval = session.eval_line("undefined_variable + 1");
        assert!(err_eval.is_err());

        // Empty line
        let res_empty = session.eval_line("").expect("empty ok");
        assert!(res_empty.is_none());

        let res_spaces = session.eval_line("   ").expect("spaces ok");
        assert!(res_spaces.is_none());
    }

    #[test]
    fn test_repl_meta_commands() {
        let mut session = ReplSession::new();

        // :help
        let help = session.eval_line(":help").expect("help ok");
        assert!(
            help.as_deref()
                .is_some_and(|s| s.contains("Available REPL commands"))
        );

        // :functions
        let fns = session.eval_line(":functions").expect("fns ok");
        assert!(fns.as_deref().is_some_and(|s| s.contains("abs(")));

        // :set
        let set_res = session.eval_line(":set x = 42").expect("set ok");
        assert_eq!(set_res, Some("x = 42".to_string()));
        assert_eq!(
            session.variables.get("x").map(|v| v.to_string()),
            Some("42".to_string())
        );

        // :set invalid format (no =)
        let set_err = session.eval_line(":set invalid_no_equals");
        assert!(set_err.is_err());
        assert!(
            set_err
                .expect_err("expected set usage err")
                .contains("Usage: :set <var> = <expr>")
        );

        // :type
        let type_res = session.eval_line(":type x").expect("type ok");
        assert_eq!(type_res, Some("number: 42".to_string()));

        // :type error
        let type_err = session.eval_line(":type undefined_xyz");
        assert!(type_err.is_err());

        // :clear
        let clear_res = session.eval_line(":clear").expect("clear ok");
        assert!(clear_res.as_deref().is_some_and(|s| s.contains("reset")));
        assert!(session.variables.is_empty());

        // :quit
        let quit_res = session.eval_line(":quit").expect("quit ok");
        assert_eq!(quit_res, Some("Goodbye!".to_string()));

        // :exit
        let exit_res = session.eval_line(":exit").expect("exit ok");
        assert_eq!(exit_res, Some("Goodbye!".to_string()));

        // Unknown meta command
        let unknown_res = session.eval_line(":unknown_meta");
        assert!(unknown_res.is_err());
        assert!(
            unknown_res
                .expect_err("expected unknown cmd err")
                .contains("Unknown command ':unknown_meta'")
        );
    }

    #[test]
    fn test_repl_meta_command_load() {
        let dir = std::env::temp_dir().join(format!("hcl_repl_load_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);

        let mut session = ReplSession::new();

        // 1. Nonexistent file
        let res_missing =
            session.eval_line(&format!(":load {}", dir.join("missing.hcl").display()));
        assert!(res_missing.is_err());
        assert!(
            res_missing
                .expect_err("expected read err")
                .contains("Failed to read")
        );

        // 2. File with syntax error
        let bad_file = dir.join("bad.hcl");
        fs::write(&bad_file, "{ invalid syntax").expect("write bad");
        let res_bad = session.eval_line(&format!(":load {}", bad_file.display()));
        assert!(res_bad.is_err());
        assert!(
            res_bad
                .expect_err("expected parse err")
                .contains("Parse error in")
        );

        // 3. Valid file with attributes and pre-existing session variables
        session.eval_line("initial = 50").expect("set initial");
        let good_file = dir.join("good.hcl");
        fs::write(
            &good_file,
            "item_a = initial + 10\nitem_b = 15\nitem_bad = undefined_foo + 1\n",
        )
        .expect("write good");
        let res_good = session
            .eval_line(&format!(":load {}", good_file.display()))
            .expect("load ok");
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

        let l1 = session.eval_line("[\n").expect("l1 ok");
        assert!(l1.is_none());

        // Empty line within buffer
        let l_empty = session.eval_line("   \n").expect("empty ok");
        assert!(l_empty.is_none());

        let l2 = session.eval_line("  1,\n").expect("l2 ok");
        assert!(l2.is_none());

        let l3 = session.eval_line("]\n").expect("l3 ok");
        assert_eq!(l3, Some("[1]".to_string()));

        // Balanced and escaped strings
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

        session.run(&mut reader, &mut writer).expect("run ok");
        let out_str = String::from_utf8_lossy(&writer);
        assert!(out_str.contains("[1]"));
        assert!(out_str.contains("Error:"));
        assert!(out_str.contains("Goodbye!"));

        // Clean EOF without :exit
        let mut session2 = ReplSession::new();
        let input_eof = "1 + 1\n";
        let mut reader_eof = Cursor::new(input_eof);
        let mut writer_eof = Vec::new();
        session2
            .run(&mut reader_eof, &mut writer_eof)
            .expect("run eof ok");
        let out_eof = String::from_utf8_lossy(&writer_eof);
        assert!(out_eof.contains("2"));

        // Failing reader returns error
        let mut failing = FailingReader;
        let mut fail_buf = [0u8; 1];
        let _ = failing.read(&mut fail_buf);
        failing.consume(0);
        let mut fail_writer = Vec::new();
        let mut session3 = ReplSession::new();
        let res_fail = session3.run(&mut failing, &mut fail_writer);
        assert!(res_fail.is_err());
        assert!(
            res_fail
                .expect_err("expected io err")
                .contains("I/O error: mock io error")
        );
    }

    #[test]
    fn test_main_function() {
        // 1. With multiline input, empty lines, and errors
        set_test_input(b"[\n1\n]\nundefined_var + 1\n\n:exit\n");
        assert!(main().is_ok());

        // 2. Clean EOF without :exit
        set_test_input(b"2 * 3\n");
        assert!(main().is_ok());

        // 3. Default input
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
