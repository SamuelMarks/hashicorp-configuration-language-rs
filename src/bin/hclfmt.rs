//! Standalone command-line formatter for HCL files (`hclfmt`).
//!
//! Provides formatting, checking, listing, and diffing of `.hcl` and `.tf` files
//! according to canonical HCL style guidelines.

#![deny(missing_docs)]

use hashicorp_configuration_language_rs::cst::format::format_str;
use hashicorp_configuration_language_rs::diagnostic::Diagnostic;
use hashicorp_configuration_language_rs::error::HclError;
use hashicorp_configuration_language_rs::span::Span;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Command-line configuration options for `hclfmt`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Overwrite target files in-place with formatted output.
    pub write: bool,
    /// List paths of files whose formatting differs from canonical formatting.
    pub list: bool,
    /// Print unified diff of formatting changes to standard output.
    pub diff: bool,
    /// Check if files are canonically formatted, exiting with non-zero code if unformatted.
    pub check: bool,
    /// Emit diagnostics formatted as machine-readable JSON upon errors.
    pub json: bool,
    /// Target file or directory paths to process (or `"-"` for standard input).
    pub targets: Vec<String>,
}

/// Parses command-line arguments into strongly-typed [`Options`].
///
/// # Arguments
/// * `args` - The command-line argument strings excluding the program name.
///
/// # Errors
/// Returns an error message if unknown flags or invalid arguments are encountered.
pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Options, String> {
    let mut options = Options::default();

    for arg in args {
        match arg.as_str() {
            "-w" | "--write" => options.write = true,
            "-l" | "--list" => options.list = true,
            "-d" | "--diff" => options.diff = true,
            "-c" | "--check" => options.check = true,
            "-j" | "--json" => options.json = true,
            "-h" | "--help" => {
                return Err("Usage: hclfmt [-w] [-l] [-d] [-c] [-j] [file/dir ...]".to_string());
            }
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(format!("Unknown flag '{flag}'"));
            }
            target => options.targets.push(target.to_string()),
        }
    }

    if options.targets.is_empty() {
        options.targets.push("-".to_string());
    }

    Ok(options)
}

/// Recursively collects all target files from a path string.
///
/// If `target` is `"-"`, returns `["-"]`.
/// If `target` is a file, returns that single file.
/// If `target` is a directory, recursively walks and collects all `.hcl` and `.tf` files.
///
/// # Arguments
/// * `target` - File, directory, or stdin path identifier.
///
/// # Errors
/// Returns an error message if the path does not exist or cannot be read.
pub fn collect_target_files(target: &str) -> Result<Vec<PathBuf>, String> {
    let path = Path::new(target);
    if path == Path::new("-") {
        return Ok(vec![PathBuf::from("-")]);
    }
    if !path.exists() {
        return Err(format!("Path '{}' does not exist", target));
    }
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut files = Vec::new();
    walk_directory(path, &mut files)?;
    files.sort();
    Ok(files)
}

/// Recursively scans a directory for `.hcl` and `.tf` files.
fn walk_directory(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory '{}': {e}", dir.display()))?;

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            walk_directory(&entry_path, files)?;
        } else if entry_path.is_file()
            && let Some(ext) = entry_path.extension().and_then(|s| s.to_str())
            && (ext == "hcl" || ext == "tf")
        {
            files.push(entry_path);
        }
    }

    Ok(())
}

/// A line edit operation for diff generation.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffOp<'a> {
    /// Line is identical in original and formatted text.
    Equal(&'a str),
    /// Line was deleted from original text.
    Delete(&'a str),
    /// Line was inserted into formatted text.
    Insert(&'a str),
}

/// Generates a pure-Rust unified diff between `original` and `formatted` text.
///
/// Produces standard unified hunk headers (`@@ -line,count +line,count @@`).
/// If both inputs are identical, returns an empty string.
///
/// # Arguments
/// * `original` - The original unformatted text.
/// * `formatted` - The canonical reformatted text.
/// * `file_name` - The filename to display in the diff header.
#[must_use]
pub fn generate_unified_diff(original: &str, formatted: &str, file_name: &str) -> String {
    if original == formatted {
        return String::new();
    }

    let orig_lines: Vec<&str> = original.lines().collect();
    let form_lines: Vec<&str> = formatted.lines().collect();

    let m = orig_lines.len();
    let n = form_lines.len();

    let mut table = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..m {
        for j in 0..n {
            if orig_lines[i] == form_lines[j] {
                table[i + 1][j + 1] = table[i][j] + 1;
            } else {
                table[i + 1][j + 1] = std::cmp::max(table[i + 1][j], table[i][j + 1]);
            }
        }
    }

    let mut ops = Vec::new();
    let mut i = m;
    let mut j = n;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && orig_lines[i - 1] == form_lines[j - 1] {
            ops.push(DiffOp::Equal(orig_lines[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
            ops.push(DiffOp::Insert(form_lines[j - 1]));
            j -= 1;
        } else {
            ops.push(DiffOp::Delete(orig_lines[i - 1]));
            i -= 1;
        }
    }
    ops.reverse();

    format_unified_hunks(&ops, file_name)
}

/// Formats calculated diff operations into unified diff hunk format.
fn format_unified_hunks(ops: &[DiffOp<'_>], file_name: &str) -> String {
    let mut out = format!(
        "--- {file_name}	(original)
+++ {file_name}	(formatted)
"
    );

    let context_size = 3;
    let mut hunk_ranges: Vec<(usize, usize)> = Vec::new();

    let mut idx = 0;
    while idx < ops.len() {
        if !matches!(ops[idx], DiffOp::Equal(_)) {
            let start = idx.saturating_sub(context_size);
            let mut end = std::cmp::min(ops.len(), idx + context_size + 1);

            let mut lookahead = idx + 1;
            while lookahead < ops.len() {
                if !matches!(ops[lookahead], DiffOp::Equal(_)) {
                    if lookahead <= end + context_size {
                        end = std::cmp::min(ops.len(), lookahead + context_size + 1);
                        idx = lookahead;
                    } else {
                        break;
                    }
                }
                lookahead += 1;
            }

            hunk_ranges.push((start, end));
        }
        idx += 1;
    }

    for (hunk_start, hunk_end) in hunk_ranges {
        let mut orig_line_no = 1;
        let mut form_line_no = 1;

        for op in &ops[..hunk_start] {
            match op {
                DiffOp::Equal(_) => {
                    orig_line_no += 1;
                    form_line_no += 1;
                }
                DiffOp::Delete(_) => orig_line_no += 1,
                DiffOp::Insert(_) => form_line_no += 1,
            }
        }

        let mut orig_count = 0;
        let mut form_count = 0;
        let hunk_ops = &ops[hunk_start..hunk_end];

        for op in hunk_ops {
            match op {
                DiffOp::Equal(_) => {
                    orig_count += 1;
                    form_count += 1;
                }
                DiffOp::Delete(_) => orig_count += 1,
                DiffOp::Insert(_) => form_count += 1,
            }
        }

        out.push_str(&format!(
            "@@ -{orig_line_no},{orig_count} +{form_line_no},{form_count} @@
"
        ));

        for op in hunk_ops {
            match op {
                DiffOp::Equal(line) => {
                    out.push(' ');
                    out.push_str(line);
                    out.push('\n');
                }
                DiffOp::Delete(line) => {
                    out.push('-');
                    out.push_str(line);
                    out.push('\n');
                }
                DiffOp::Insert(line) => {
                    out.push('+');
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
    }

    out
}

/// Executes formatting operations with the given options and streams.
///
/// Returns the process exit code (`0` for success, non-zero for differences in check mode or errors).
///
/// # Arguments
/// * `options` - Parsed CLI options.
/// * `stdin` - Standard input reader.
/// * `stdout` - Standard output writer.
/// * `stderr` - Standard error writer.
///
/// # Errors
/// Returns an error message if an unrecoverable IO failure occurs.
pub fn run(
    options: &Options,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, String> {
    let mut files_to_process = Vec::new();

    for target in &options.targets {
        let collected = collect_target_files(target)?;
        files_to_process.extend(collected);
    }

    let mut had_unformatted = false;
    let mut had_errors = false;

    for path in files_to_process {
        let is_stdin = path == Path::new("-");
        let (content, display_name) = if is_stdin {
            let mut buffer = String::new();
            stdin
                .read_to_string(&mut buffer)
                .map_err(|e| format!("Failed to read standard input: {e}"))?;
            (buffer, "<stdin>".to_string())
        } else {
            let buffer = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read file '{}': {e}", path.display()))?;
            (buffer, path.display().to_string())
        };

        let formatted = match format_str(&content) {
            Ok(f) => f,
            Err(e) => {
                if options.json {
                    let span = Span::new_with_file(
                        0,
                        content.len(),
                        1,
                        1,
                        content.lines().count().max(1),
                        1,
                        Some(Arc::from(display_name.as_str())),
                    );
                    let mut diag = Diagnostic::new(HclError::Parse(e.to_string()), span);
                    diag.summary = Some(format!("Error parsing {display_name}"));
                    diag.detail = Some(e.to_string());
                    let json_str = diag.to_json(Some(&content)).unwrap_or_default();
                    let _ = writeln!(stderr, "{json_str}");
                } else {
                    let _ = writeln!(stderr, "Error parsing {display_name}: {e}");
                }
                had_errors = true;
                continue;
            }
        };

        let is_different = content != formatted;

        if is_different {
            had_unformatted = true;

            if options.list {
                let _ = writeln!(stdout, "{display_name}");
            }

            if options.diff {
                let diff_output = generate_unified_diff(&content, &formatted, &display_name);
                let _ = write!(stdout, "{diff_output}");
            }

            if options.write && !is_stdin {
                fs::write(&path, &formatted)
                    .map_err(|e| format!("Failed to write file '{}': {e}", path.display()))?;
            }
        }

        if !options.write && !options.list && !options.diff && !options.check {
            let _ = write!(stdout, "{formatted}");
        }
    }

    if had_errors || (options.check && had_unformatted) {
        Ok(1)
    } else {
        Ok(0)
    }
}

/// Helper entry point returning the exit code or command error.
///
/// # Arguments
/// * `args` - The command-line arguments excluding binary name.
///
/// # Errors
/// Returns an error message if argument parsing or unrecoverable IO fails.
pub fn main_impl(args: Vec<String>) -> Result<i32, String> {
    let options = parse_args(args)?;
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run(&options, &mut stdin, &mut stdout, &mut stderr)
}

/// Converts a formatting execution result into a process exit code.
fn exit_code_from_result(res: Result<i32, String>) -> std::process::ExitCode {
    match res {
        Ok(code) => {
            if let Ok(c) = u8::try_from(code) {
                std::process::ExitCode::from(c)
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Main entry point for the `hclfmt` CLI executable.
fn main() -> std::process::ExitCode {
    #[cfg(not(test))]
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(test)]
    let args = vec!["--help".to_string()];
    exit_code_from_result(main_impl(args))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args_all_flags() {
        let args = vec![
            "-w".to_string(),
            "-l".to_string(),
            "-d".to_string(),
            "-c".to_string(),
            "main.hcl".to_string(),
            "sub.tf".to_string(),
        ];
        let opts = parse_args(args).expect("parse ok");
        assert!(opts.write);
        assert!(opts.list);
        assert!(opts.diff);
        assert!(opts.check);
        assert_eq!(opts.targets, vec!["main.hcl", "sub.tf"]);
    }

    #[test]
    fn test_parse_args_long_flags() {
        let args = vec![
            "--write".to_string(),
            "--list".to_string(),
            "--diff".to_string(),
            "--check".to_string(),
        ];
        let opts = parse_args(args).expect("parse ok");
        assert!(opts.write);
        assert!(opts.list);
        assert!(opts.diff);
        assert!(opts.check);
        assert_eq!(opts.targets, vec!["-"]);
    }

    #[test]
    fn test_parse_args_errors() {
        assert!(parse_args(vec!["--unknown".to_string()]).is_err());
        assert!(parse_args(vec!["-h".to_string()]).is_err());
        assert!(parse_args(vec!["--help".to_string()]).is_err());
    }

    #[test]
    fn test_diff_identical() {
        let src = "a = 1
";
        assert_eq!(generate_unified_diff(src, src, "test.hcl"), "");
    }

    #[test]
    fn test_diff_modified() {
        let orig = "a = 1
b = 2
c = 3
";
        let form = "a = 1
b = 20
c = 3
";
        let diff = generate_unified_diff(orig, form, "test.hcl");
        assert!(diff.contains("--- test.hcl	(original)"));
        assert!(diff.contains("+++ test.hcl	(formatted)"));
        assert!(diff.contains("@@ -1,3 +1,3 @@"));
        assert!(diff.contains("-b = 2"));
        assert!(diff.contains("+b = 20"));

        let empty_to_new = generate_unified_diff("", "added\n", "add.hcl");
        assert!(empty_to_new.contains("+added"));

        let old_to_empty = generate_unified_diff("removed\n", "", "rem.hcl");
        assert!(old_to_empty.contains("-removed"));

        let del_from_start = generate_unified_diff("a\nb\nc\n", "b\nc\n", "del_start.hcl");
        assert!(del_from_start.contains("-a"));
    }

    #[test]
    fn test_run_stdin_default_and_check() {
        let unformatted = "a=1
";
        let opts_default = Options {
            targets: vec!["-".to_string()],
            ..Default::default()
        };

        let mut stdin = unformatted.as_bytes();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&opts_default, &mut stdin, &mut stdout, &mut stderr).expect("run ok");
        assert_eq!(code, 0);
        let out_str = String::from_utf8(stdout).expect("utf8");
        assert_eq!(
            out_str,
            "a = 1
"
        );

        let opts_stdin_write = Options {
            write: true,
            targets: vec!["-".to_string()],
            ..Default::default()
        };
        let mut stdin_w = unformatted.as_bytes();
        let mut stdout_w = Vec::new();
        let mut stderr_w = Vec::new();
        assert_eq!(
            run(
                &opts_stdin_write,
                &mut stdin_w,
                &mut stdout_w,
                &mut stderr_w
            ),
            Ok(0)
        );

        let opts_check = Options {
            check: true,
            targets: vec!["-".to_string()],
            ..Default::default()
        };
        let mut stdin_check = unformatted.as_bytes();
        let mut stdout_check = Vec::new();
        let mut stderr_check = Vec::new();
        let code_check = run(
            &opts_check,
            &mut stdin_check,
            &mut stdout_check,
            &mut stderr_check,
        )
        .expect("run ok");
        assert_eq!(code_check, 1);

        let mut stdin_formatted_check = "a = 1\n".as_bytes();
        let mut stdout_fc = Vec::new();
        let mut stderr_fc = Vec::new();
        let code_fc = run(
            &opts_check,
            &mut stdin_formatted_check,
            &mut stdout_fc,
            &mut stderr_fc,
        )
        .expect("run ok");
        assert_eq!(code_fc, 0);
    }

    #[test]
    fn test_collect_target_files_nonexistent() {
        assert!(collect_target_files("nonexistent_path_definitely_not_here.hcl").is_err());
    }

    #[test]
    fn test_run_file_with_parse_error() {
        let dir = std::env::temp_dir().join(format!("hclfmt_err_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let bad_file = dir.join("bad.hcl");
        let _ = fs::write(&bad_file, "{ invalid syntax");

        let opts = Options {
            targets: vec![bad_file.to_string_lossy().to_string()],
            ..Default::default()
        };
        let mut stdin = std::io::empty();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code =
            run(&opts, &mut stdin, &mut stdout, &mut stderr).expect("run should return status");
        assert_eq!(code, 1);
        let err_str = String::from_utf8(stderr).expect("utf8");
        assert!(err_str.contains("Error parsing"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_main_impl_error() {
        assert!(main_impl(vec!["--unknown-flag".to_string()]).is_err());
    }

    #[test]
    fn test_walk_directory_and_collect_files_comprehensive() {
        let dir = std::env::temp_dir().join(format!("hclfmt_walk_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let sub_dir = dir.join("nested");
        let _ = fs::create_dir_all(&sub_dir);

        let f_hcl = dir.join("top.hcl");
        let f_tf = dir.join("top.tf");
        let f_txt = dir.join("readme.txt");
        let f_noext = dir.join("LICENSE");
        let f_sub_hcl = sub_dir.join("nested.hcl");

        let _ = fs::write(&f_hcl, "a = 1\n");
        let _ = fs::write(&f_tf, "b = 2\n");
        let _ = fs::write(&f_txt, "ignore\n");
        let _ = fs::write(&f_noext, "ignore\n");
        let _ = fs::write(&f_sub_hcl, "c = 3\n");

        let mut collected = Vec::new();
        let walk_res = walk_directory(&dir, &mut collected);
        assert!(walk_res.is_ok());
        collected.sort();

        assert_eq!(collected.len(), 3);
        assert!(collected.contains(&f_hcl));
        assert!(collected.contains(&f_tf));
        assert!(collected.contains(&f_sub_hcl));

        // Test collecting a single file directly
        let f_hcl_str = f_hcl.to_string_lossy();
        let single_res = collect_target_files(&f_hcl_str);
        assert_eq!(single_res, Ok(vec![f_hcl.clone()]));

        // Test collecting a directory directly (covers files.sort() and Ok(files))
        let dir_str = dir.to_string_lossy();
        let dir_res = collect_target_files(&dir_str);
        assert_eq!(
            dir_res,
            Ok(vec![f_sub_hcl.clone(), f_hcl.clone(), f_tf.clone()])
        );

        // Test walk on nonexistent directory
        let bad_dir = dir.join("nonexistent_subdir");
        let mut bad_collected = Vec::new();
        assert!(walk_directory(&bad_dir, &mut bad_collected).is_err());

        #[cfg(unix)]
        {
            let broken_symlink = dir.join("broken_link");
            let _ = std::os::unix::fs::symlink(dir.join("nonexistent_target"), &broken_symlink);
            let mut coll_link = Vec::new();
            assert!(walk_directory(&dir, &mut coll_link).is_ok());
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_format_unified_hunks_complex_cases() {
        // Case 1: Two far-apart hunks with deletes and inserts to exercise
        // lookahead break, second hunk insertion, and prior hunk line count accumulation.
        let ops = vec![
            DiffOp::Delete("old_1"),
            DiffOp::Insert("new_1"),
            DiffOp::Equal("eq_1"),
            DiffOp::Equal("eq_2"),
            DiffOp::Equal("eq_3"),
            DiffOp::Equal("eq_4"),
            DiffOp::Equal("eq_5"),
            DiffOp::Equal("eq_6"),
            DiffOp::Equal("eq_7"),
            DiffOp::Equal("eq_8"),
            DiffOp::Equal("eq_9"),
            DiffOp::Delete("old_2"),
            DiffOp::Insert("new_2"),
        ];
        let diff_out = format_unified_hunks(&ops, "multi_hunk.hcl");
        assert!(diff_out.contains("--- multi_hunk.hcl"));
        assert!(diff_out.contains("-old_1"));
        assert!(diff_out.contains("+new_1"));
        assert!(diff_out.contains("-old_2"));
        assert!(diff_out.contains("+new_2"));

        // Case 2: Adjacent hunks within overlap range where start <= last.1
        let ops_overlap = vec![
            DiffOp::Delete("top"),
            DiffOp::Equal("mid_1"),
            DiffOp::Equal("mid_2"),
            DiffOp::Equal("mid_3"),
            DiffOp::Equal("mid_4"),
            DiffOp::Equal("mid_5"),
            DiffOp::Insert("bottom"),
        ];
        let diff_overlap = format_unified_hunks(&ops_overlap, "overlap.hcl");
        assert!(diff_overlap.contains("-top"));
        assert!(diff_overlap.contains("+bottom"));
    }

    /// Reader that simulates an IO error on standard input.
    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("simulated stdin read error"))
        }
    }

    #[test]
    fn test_run_stdin_read_error() {
        let opts = Options {
            targets: vec!["-".to_string()],
            ..Default::default()
        };
        let mut stdin = FailingReader;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = run(&opts, &mut stdin, &mut stdout, &mut stderr);
        assert!(res.is_err());
    }

    #[test]
    fn test_run_options_list_diff_write_success() {
        let dir = std::env::temp_dir().join(format!("hclfmt_run_opts_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let f_hcl = dir.join("unformatted.hcl");
        let _ = fs::write(&f_hcl, "a=1\n");

        let path_str = f_hcl.to_string_lossy();

        // Test list option
        let opts_list = Options {
            list: true,
            targets: vec![path_str.to_string()],
            ..Default::default()
        };
        let mut stdin = std::io::empty();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res_list = run(&opts_list, &mut stdin, &mut stdout, &mut stderr);
        assert_eq!(res_list, Ok(0));
        assert!(String::from_utf8_lossy(&stdout).contains(path_str.as_ref()));

        // Test diff option
        let opts_diff = Options {
            diff: true,
            targets: vec![path_str.to_string()],
            ..Default::default()
        };
        let mut stdin = std::io::empty();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res_diff = run(&opts_diff, &mut stdin, &mut stdout, &mut stderr);
        assert_eq!(res_diff, Ok(0));
        assert!(String::from_utf8_lossy(&stdout).contains("---"));

        // Test write option (success case)
        let opts_write = Options {
            write: true,
            targets: vec![path_str.to_string()],
            ..Default::default()
        };
        let mut stdin = std::io::empty();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res_write = run(&opts_write, &mut stdin, &mut stdout, &mut stderr);
        assert_eq!(res_write, Ok(0));
        let new_content = fs::read_to_string(&f_hcl).unwrap_or_default();
        assert_eq!(new_content, "a = 1\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_main_and_main_impl_comprehensive() {
        let dir = std::env::temp_dir().join(format!("hclfmt_main_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let f_hcl = dir.join("valid.hcl");
        let _ = fs::write(&f_hcl, "a = 1\n");

        let path_str = f_hcl.to_string_lossy();
        let res = main_impl(vec![path_str.to_string()]);
        assert_eq!(res, Ok(0));

        // Test main_impl target collection failure
        let res_err = main_impl(vec!["definitely_missing_target_file.hcl".to_string()]);
        assert!(res_err.is_err());

        // Test all branches of exit_code_from_result
        assert_eq!(
            exit_code_from_result(Ok(0)),
            std::process::ExitCode::SUCCESS
        );
        assert_eq!(
            exit_code_from_result(Ok(256)),
            std::process::ExitCode::FAILURE
        );
        assert_eq!(
            exit_code_from_result(Err("custom error".to_string())),
            std::process::ExitCode::FAILURE
        );

        let _ = main();

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn test_io_and_permission_failures_unix() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("hclfmt_perms_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);

        // 1. Unreadable file: triggers read error in run()
        let unreadable_file = dir.join("unreadable.hcl");
        let _ = fs::write(&unreadable_file, "a=1\n");
        let _ = fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o000));

        assert!(main_impl(vec![unreadable_file.to_string_lossy().to_string()]).is_err());

        let opts_unreadable = Options {
            targets: vec![unreadable_file.to_string_lossy().to_string()],
            ..Default::default()
        };
        let mut stdin = std::io::empty();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res_unreadable = run(&opts_unreadable, &mut stdin, &mut stdout, &mut stderr);
        assert!(res_unreadable.is_err());

        // 2. Read-only unformatted file with write mode: triggers write error in run()
        let readonly_file = dir.join("readonly.hcl");
        let _ = fs::write(&readonly_file, "a=1\n");
        let _ = fs::set_permissions(&readonly_file, fs::Permissions::from_mode(0o444));

        let opts_write = Options {
            write: true,
            targets: vec![readonly_file.to_string_lossy().to_string()],
            ..Default::default()
        };
        let mut stdin = std::io::empty();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res_write = run(&opts_write, &mut stdin, &mut stdout, &mut stderr);
        assert!(res_write.is_err());

        // 3. Unreadable directory: triggers walk_directory error in collect_target_files()
        let unreadable_dir = dir.join("unreadable_dir");
        let _ = fs::create_dir_all(&unreadable_dir);
        let _ = fs::set_permissions(&unreadable_dir, fs::Permissions::from_mode(0o000));
        let res_collect = collect_target_files(&unreadable_dir.to_string_lossy());
        assert!(res_collect.is_err());

        // 4. Nested unreadable directory: triggers recursive walk_directory error
        let parent_dir = dir.join("parent_walk");
        let child_dir = parent_dir.join("child_unreadable");
        let _ = fs::create_dir_all(&child_dir);
        let _ = fs::set_permissions(&child_dir, fs::Permissions::from_mode(0o000));
        let mut files = Vec::new();
        let res_nested = walk_directory(&parent_dir, &mut files);
        assert!(res_nested.is_err());

        // Restore permissions for cleanup
        let _ = fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o644));
        let _ = fs::set_permissions(&readonly_file, fs::Permissions::from_mode(0o644));
        let _ = fs::set_permissions(&unreadable_dir, fs::Permissions::from_mode(0o755));
        let _ = fs::set_permissions(&child_dir, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hclfmt_json_flag_and_output() {
        let opts = parse_args(vec!["--json".to_string(), "-".to_string()]).expect("parse json");
        assert!(opts.json);

        // Invalid HCL with json flag
        let mut stdin = "{ invalid syntax".as_bytes();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&opts, &mut stdin, &mut stdout, &mut stderr).expect("run json");
        let err_str = String::from_utf8_lossy(&stderr);
        assert_eq!(code, 1);
        assert!(err_str.contains("\"severity\":\"error\""));
        assert!(err_str.contains("<stdin>"));
    }
}
