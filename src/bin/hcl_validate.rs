//! Continuous integration and batch validation CLI tool (`hcl-validate`).
#![deny(missing_docs)]
use hashicorp_configuration_language_rs::analysis::Linter;
use hashicorp_configuration_language_rs::analysis::type_check::TypeChecker;
use hashicorp_configuration_language_rs::ast::schema::{
    AttributeSchema, BlockHeaderSchema, BodySchema,
};
use hashicorp_configuration_language_rs::diagnostic::{Diagnostics, Severity};
use hashicorp_configuration_language_rs::parse::parser::Parser;
use hashicorp_configuration_language_rs::types::Type;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(not(test))]
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
/// Command-line configuration options for `hcl-validate`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidateOptions {
    /// Path to a declarative JSON schema file.
    pub schema_path: Option<String>,
    /// Treat warnings as errors, exiting with code 2 if warnings are detected.
    pub strict: bool,
    /// Emit diagnostics formatted as machine-readable JSON.
    pub json: bool,
    /// Target files or directories to validate.
    pub targets: Vec<String>,
}
/// JSON schema representation for loading declarative validation schemas.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaSpecJson {
    /// Expected attributes mapped by name.
    #[serde(default)]
    pub attributes: HashMap<String, AttrSpecJson>,
    /// Expected blocks mapped by block type.
    #[serde(default)]
    pub blocks: HashMap<String, BlockSpecJson>,
}
impl SchemaSpecJson {
    /// Converts this JSON schema specification into a strongly-typed [`BodySchema`].
    #[must_use]
    pub fn to_body_schema(&self) -> BodySchema {
        let mut schema = BodySchema::new();
        for (name, attr_spec) in &self.attributes {
            let mut aschem = AttributeSchema::new(name, attr_spec.required);
            if let Some(ref d) = attr_spec.description {
                aschem = aschem.with_description(d.clone());
            }
            if let Some(ref t_str) = attr_spec.r#type {
                aschem = aschem.with_type(parse_type_str(t_str));
            }
            schema = schema.with_attribute(aschem);
        }
        for (b_name, b_spec) in &self.blocks {
            let mut bschem = BlockHeaderSchema::new(b_name, b_spec.labels.clone());
            if let Some(ref d) = b_spec.description {
                bschem = bschem.with_description(d.clone());
            }
            if let Some(ref inner_json) = b_spec.body {
                bschem = bschem.with_body_schema(inner_json.to_body_schema());
            }
            schema = schema.with_block(bschem);
        }
        schema
    }
}
/// Parses a type name string into an HCL [`Type`].
///
/// # Arguments
/// * `s` - The type name string.
fn parse_type_str(s: &str) -> Type {
    match s.trim().to_lowercase().as_str() {
        "string" => Type::String,
        "number" => Type::Number,
        "bool" | "boolean" => Type::Bool,
        "list" => Type::List(Box::new(Type::Dynamic)),
        "map" => Type::Map(Box::new(Type::Dynamic)),
        "set" => Type::Set(Box::new(Type::Dynamic)),
        _ => Type::Dynamic,
    }
}
/// Attribute specification in JSON schema format.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AttrSpecJson {
    /// Whether the attribute is required.
    #[serde(default)]
    pub required: bool,
    /// Attribute documentation description.
    #[serde(default)]
    pub description: Option<String>,
    /// Attribute type name (`"string"`, `"number"`, `"bool"`, etc.).
    #[serde(default)]
    pub r#type: Option<String>,
}
/// Block specification in JSON schema format.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockSpecJson {
    /// List of expected label names.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Block documentation description.
    #[serde(default)]
    pub description: Option<String>,
    /// Nested body schema specification.
    #[serde(default)]
    pub body: Option<SchemaSpecJson>,
}
/// Parses CLI arguments into [`ValidateOptions`].
///
/// # Arguments
/// * `args` - The command-line argument strings.
///
/// # Errors
/// Returns an error message if unknown flags or missing arguments are encountered.
pub fn parse_validate_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<ValidateOptions, String> {
    let mut opts = ValidateOptions::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--strict" => opts.strict = true,
            "--json" => opts.json = true,
            "--schema" => {
                let schema_file = iter
                    .next()
                    .ok_or_else(|| "Missing argument for --schema".to_string())?;
                opts.schema_path = Some(schema_file);
            }
            "-h" | "--help" => {
                return Err(
                    "Usage: hcl-validate [--schema <schema.json>] [--strict] [--json] [file/dir ...]"
                        .to_string(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("Unknown flag '{flag}'"));
            }
            target => opts.targets.push(target.to_string()),
        }
    }
    if opts.targets.is_empty() {
        opts.targets.push(".".to_string());
    }
    Ok(opts)
}
/// Recursively discovers target `.hcl` and `.tf` files from a path string.
///
/// # Arguments
/// * `target` - File or directory path.
///
/// # Errors
/// Returns error string on I/O failure.
pub fn collect_validate_files(target: &str) -> Result<Vec<PathBuf>, String> {
    let path = Path::new(target);
    if !path.exists() {
        return Err(format!("Path '{}' does not exist", path.display()));
    }
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut files = Vec::new();
    walk_directory(path, &mut files)?;
    files.sort();
    Ok(files)
}
/// Recursively walks a directory looking for `.hcl` and `.tf` files.
///
/// # Arguments
/// * `dir` - The directory path to inspect.
/// * `files` - Output vector collecting matched file paths.
///
/// # Errors
/// Returns an error string if reading the directory fails.
fn walk_directory(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory '{}': {e}", dir.display()))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_directory(&p, files)?;
        } else if p.is_file() {
            if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                if ext == "hcl" || ext == "tf" {
                    files.push(p);
                }
            }
        }
    }
    Ok(())
}
/// Runs validation on target files, writing diagnostic outputs and returning an exit code.
///
/// Exit codes:
/// * `0` - Clean: no errors, and no warnings in strict mode.
/// * `1` - Error: syntax, type, or schema errors detected.
/// * `2` - Warning: lint warnings detected in strict mode.
///
/// # Arguments
/// * `opts` - Validation options.
/// * `stdout` - Output stream.
/// * `stderr` - Error stream.
///
/// # Errors
/// Returns an error string on unrecoverable I/O or configuration failure.
pub fn run_validate<O: Write, E: Write>(
    opts: &ValidateOptions,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<i32, String> {
    let schema = if let Some(ref sp) = opts.schema_path {
        let content = fs::read_to_string(sp)
            .map_err(|e| format!("Failed to read schema file '{sp}': {e}"))?;
        let spec: SchemaSpecJson = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse schema JSON '{sp}': {e}"))?;
        Some(spec.to_body_schema())
    } else {
        None
    };
    let mut all_files = Vec::new();
    for target in &opts.targets {
        let files = collect_validate_files(target)?;
        all_files.extend(files);
    }
    all_files.sort();
    all_files.dedup();
    let mut had_errors = false;
    let mut had_warnings = false;
    let checker = TypeChecker::new();
    let linter = Linter::new();
    for file_path in all_files {
        let display_name = file_path.to_string_lossy().to_string();
        let content = fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read file '{display_name}': {e}"))?;
        let mut file_diags = Diagnostics::new();
        let mut parser = Parser::new(&content);
        let body = parser.parse_body();
        file_diags.extend(parser.errors().clone());
        if let Some(ref s) = schema {
            if let Err(schema_diags) = checker.check_body(&body, s) {
                file_diags.extend(schema_diags);
            }
        }
        let lint_diags = linter.lint_body(&body);
        file_diags.extend(lint_diags);
        if file_diags.has_errors() {
            had_errors = true;
        }
        if file_diags.iter().any(|d| d.severity == Severity::Warning) {
            had_warnings = true;
        }
        if opts.json {
            if !file_diags.errors().is_empty() {
                let json_val = file_diags.to_json_value(Some(&content));
                let _ = writeln!(stderr, "{json_val:#}");
            }
        } else {
            for diag in file_diags.errors() {
                let level = match diag.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                };
                let _ = writeln!(
                    stderr,
                    "{}: {}: {}",
                    display_name,
                    level,
                    diag.summary_str()
                );
            }
        }
    }
    if had_errors {
        Ok(1)
    } else if opts.strict {
        if had_warnings {
            Ok(2)
        } else {
            if !opts.json {
                let _ = writeln!(stdout, "Success! The configuration is valid.");
            }
            Ok(0)
        }
    } else {
        if !opts.json {
            let _ = writeln!(stdout, "Success! The configuration is valid.");
        }
        Ok(0)
    }
}
#[cfg(test)]
thread_local! {
    static TEST_ARGS : std::cell::RefCell < Option < Vec < String >>> = const {
    std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn set_test_args(args: Vec<String>) {
    TEST_ARGS.with(|a| *a.borrow_mut() = Some(args));
}
#[cfg(test)]
fn get_test_args() -> Vec<String> {
    match TEST_ARGS.with(|a| a.borrow_mut().take()) {
        Some(args) => args,
        None => vec!["--help".to_string()],
    }
}
/// Main entry point for `hcl-validate`.
///
/// # Errors
/// Returns error string on validation failure.
pub fn main() -> Result<(), String> {
    #[cfg(not(test))]
    let args: Vec<String> = env::args().skip(1).collect();
    #[cfg(test)]
    let args: Vec<String> = get_test_args();
    let opts = parse_validate_args(args)?;
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let exit_code = run_validate(&opts, &mut stdout, &mut stderr)?;
    if exit_code != 0 {
        Err(format!("Validation failed with exit code {exit_code}"))
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_type_str_variants() {
        assert_eq!(parse_type_str("string"), Type::String);
        assert_eq!(parse_type_str("STRING"), Type::String);
        assert_eq!(parse_type_str("number"), Type::Number);
        assert_eq!(parse_type_str("bool"), Type::Bool);
        assert_eq!(parse_type_str("boolean"), Type::Bool);
        assert_eq!(parse_type_str("list"), Type::List(Box::new(Type::Dynamic)));
        assert_eq!(parse_type_str("map"), Type::Map(Box::new(Type::Dynamic)));
        assert_eq!(parse_type_str("set"), Type::Set(Box::new(Type::Dynamic)));
        assert_eq!(parse_type_str("custom_unknown"), Type::Dynamic);
    }
    #[test]
    fn test_parse_validate_args_variations() {
        let args = vec![
            "--schema".to_string(),
            "schema.json".to_string(),
            "--strict".to_string(),
            "--json".to_string(),
            "config.hcl".to_string(),
        ];
        let opts = parse_validate_args(args).expect("args ok");
        assert_eq!(opts.schema_path, Some("schema.json".to_string()));
        assert!(opts.strict);
        assert!(opts.json);
        assert_eq!(opts.targets, vec!["config.hcl".to_string()]);
        let opts_default =
            parse_validate_args(vec!["--strict".to_string()]).expect("default target ok");
        assert_eq!(opts_default.targets, vec![".".to_string()]);
        let err_schema = parse_validate_args(vec!["--schema".to_string()]);
        assert!(err_schema.is_err());
        assert_eq!(
            err_schema.err().as_deref(),
            Some("Missing argument for --schema")
        );
        let err_h = parse_validate_args(vec!["-h".to_string()]);
        assert!(err_h.expect_err("expected help").contains("Usage:"));
        let err_help = parse_validate_args(vec!["--help".to_string()]);
        assert!(err_help.expect_err("expected help").contains("Usage:"));
        let err_unknown = parse_validate_args(vec!["--unknown-flag".to_string()]);
        assert!(err_unknown.is_err());
        assert_eq!(
            err_unknown.err().as_deref(),
            Some("Unknown flag '--unknown-flag'")
        );
    }
    #[test]
    fn test_schema_spec_json_to_body_schema_exhaustive() {
        let json = r#"
        {
            "attributes": {
                "name": { "required": true, "type": "string", "description": "Resource name" },
                "port": { "required": false, "type": "number" },
                "tag": { "required": false }
            },
            "blocks": {
                "server": {
                    "labels": ["id"],
                    "description": "Server block",
                    "body": {
                        "attributes": {
                            "ip": { "required": true, "type": "string" }
                        }
                    }
                },
                "cluster": {
                    "labels": []
                }
            }
        }
        "#;
        let spec: SchemaSpecJson = serde_json::from_str(json).expect("deserialize schema json");
        let body_schema = spec.to_body_schema();
        assert_eq!(body_schema.attributes.len(), 3);
        assert!(body_schema.attributes["name"].required);
        assert_eq!(
            body_schema.attributes["name"].expected_type,
            Some(Type::String)
        );
        assert_eq!(
            body_schema.attributes["name"].description.as_deref(),
            Some("Resource name")
        );
        assert_eq!(body_schema.attributes["tag"].expected_type, None);
        assert_eq!(body_schema.blocks.len(), 2);
        let srv = &body_schema.blocks["server"];
        assert_eq!(srv.label_names, vec!["id".to_string()]);
        assert_eq!(srv.description.as_deref(), Some("Server block"));
        assert!(srv.body_schema.is_some());
        let cluster = &body_schema.blocks["cluster"];
        assert!(cluster.label_names.is_empty());
        assert_eq!(cluster.description, None);
        assert!(cluster.body_schema.is_none());
    }
    #[test]
    fn test_collect_validate_files_and_walk() {
        assert!(collect_validate_files("nonexistent_path_definitely_absent_123").is_err());
        let dir = std::env::temp_dir().join(format!("hcl_validate_walk_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let file_path = dir.join("solo.hcl");
        fs::write(&file_path, "a = 1\n").expect("write solo");
        let mut err_files = Vec::new();
        assert!(walk_directory(&file_path, &mut err_files).is_err());
        let single =
            collect_validate_files(file_path.to_str().expect("to_str")).expect("collect single");
        assert_eq!(single, vec![file_path.clone()]);
        let sub_dir = dir.join("sub");
        let _ = fs::create_dir_all(&sub_dir);
        let tf_file = sub_dir.join("main.tf");
        fs::write(&tf_file, "b = 2\n").expect("write tf");
        let txt_file = sub_dir.join("notes.txt");
        fs::write(&txt_file, "text").expect("write txt");
        let no_ext = sub_dir.join("LICENSE");
        fs::write(&no_ext, "MIT").expect("write no ext");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let broken_symlink = sub_dir.join("broken_symlink");
            let _ = std::os::unix::fs::symlink("does_not_exist", broken_symlink);
            let unreadable_dir = dir.join("unreadable_dir");
            let _ = fs::create_dir_all(&unreadable_dir);
            let _ = fs::set_permissions(&unreadable_dir, fs::Permissions::from_mode(0o000));
            assert!(collect_validate_files(&unreadable_dir.to_string_lossy()).is_err());
            let parent_walk = dir.join("parent_walk");
            let child_unreadable = parent_walk.join("child_unreadable");
            let _ = fs::create_dir_all(&child_unreadable);
            let _ = fs::set_permissions(&child_unreadable, fs::Permissions::from_mode(0o000));
            let mut files = Vec::new();
            assert!(walk_directory(&parent_walk, &mut files).is_err());
            let _ = fs::set_permissions(&unreadable_dir, fs::Permissions::from_mode(0o755));
            let _ = fs::set_permissions(&child_unreadable, fs::Permissions::from_mode(0o755));
        }
        let collected = collect_validate_files(dir.to_str().expect("to_str")).expect("collect dir");
        assert!(collected.contains(&file_path));
        assert!(collected.contains(&tf_file));
        assert!(!collected.contains(&txt_file));
        assert!(!collected.contains(&no_ext));
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn test_run_validate_clean_and_errors() {
        let dir =
            std::env::temp_dir().join(format!("hcl_validate_clean_err_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let valid_file = dir.join("valid.hcl");
        fs::write(&valid_file, "name = \"app\"\n").expect("write valid");
        let schema_file = dir.join("schema.json");
        fs::write(
            &schema_file,
            r#"{ "attributes": { "name": { "required": true, "type": "string" } } }"#,
        )
        .expect("write schema");
        let opts_clean = ValidateOptions {
            schema_path: Some(schema_file.to_string_lossy().to_string()),
            strict: false,
            json: false,
            targets: vec![valid_file.to_string_lossy().to_string()],
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code_clean = run_validate(&opts_clean, &mut stdout, &mut stderr).expect("validate ok");
        assert_eq!(code_clean, 0);
        let stdout_str = String::from_utf8_lossy(&stdout);
        assert!(stdout_str.contains("Success! The configuration is valid."));
        let opts_missing = ValidateOptions {
            schema_path: None,
            strict: false,
            json: false,
            targets: vec!["nonexistent_target_path_definitely_absent_12345.hcl".to_string()],
        };
        assert!(run_validate(&opts_missing, &mut stdout, &mut stderr).is_err());
        let opts_clean_json = ValidateOptions {
            schema_path: None,
            strict: false,
            json: true,
            targets: vec![valid_file.to_string_lossy().to_string()],
        };
        let mut stdout_cj = Vec::new();
        let mut stderr_cj = Vec::new();
        let code_cj = run_validate(&opts_clean_json, &mut stdout_cj, &mut stderr_cj)
            .expect("validate clean json");
        assert_eq!(code_cj, 0);
        assert!(stdout_cj.is_empty());
        let bad_file = dir.join("bad.hcl");
        fs::write(&bad_file, "{ invalid syntax").expect("write bad");
        let opts_err = ValidateOptions {
            schema_path: None,
            strict: false,
            json: true,
            targets: vec![bad_file.to_string_lossy().to_string()],
        };
        let mut stdout_err = Vec::new();
        let mut stderr_err = Vec::new();
        let code_err =
            run_validate(&opts_err, &mut stdout_err, &mut stderr_err).expect("validate err ok");
        assert_eq!(code_err, 1);
        assert!(!stderr_err.is_empty());
        let mut stdout_err_txt = Vec::new();
        let mut stderr_err_txt = Vec::new();
        let opts_err_txt = ValidateOptions {
            schema_path: None,
            strict: false,
            json: false,
            targets: vec![bad_file.to_string_lossy().to_string()],
        };
        let code_err_txt = run_validate(&opts_err_txt, &mut stdout_err_txt, &mut stderr_err_txt)
            .expect("validate err txt");
        assert_eq!(code_err_txt, 1);
        let err_txt_str = String::from_utf8_lossy(&stderr_err_txt);
        assert!(err_txt_str.contains("error:"));
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn test_run_validate_schema_checking_and_read_errors() {
        let dir =
            std::env::temp_dir().join(format!("hcl_validate_schema_err_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let opts_missing_schema = ValidateOptions {
            schema_path: Some(dir.join("nonexistent.json").to_string_lossy().to_string()),
            strict: false,
            json: false,
            targets: vec![],
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert!(run_validate(&opts_missing_schema, &mut stdout, &mut stderr).is_err());
        let bad_json_file = dir.join("invalid_schema.json");
        fs::write(&bad_json_file, "invalid json {").expect("write bad json");
        let opts_bad_json = ValidateOptions {
            schema_path: Some(bad_json_file.to_string_lossy().to_string()),
            strict: false,
            json: false,
            targets: vec![],
        };
        assert!(run_validate(&opts_bad_json, &mut stdout, &mut stderr).is_err());
        let bad_utf8_file = dir.join("bad_utf8.hcl");
        fs::write(&bad_utf8_file, [0xFF, 0xFE, 0xFD]).expect("write non utf8");
        let opts_bad_utf8 = ValidateOptions {
            schema_path: None,
            strict: false,
            json: false,
            targets: vec![bad_utf8_file.to_string_lossy().to_string()],
        };
        assert!(run_validate(&opts_bad_utf8, &mut stdout, &mut stderr).is_err());
        let schema_file = dir.join("strict_schema.json");
        fs::write(
            &schema_file,
            r#"{ "attributes": { "req_key": { "required": true, "type": "string" } } }"#,
        )
        .expect("write schema");
        let incomplete_hcl = dir.join("incomplete.hcl");
        fs::write(&incomplete_hcl, "other_key = 123\n").expect("write incomplete");
        let opts_schema_mismatch = ValidateOptions {
            schema_path: Some(schema_file.to_string_lossy().to_string()),
            strict: false,
            json: false,
            targets: vec![incomplete_hcl.to_string_lossy().to_string()],
        };
        let code_mismatch =
            run_validate(&opts_schema_mismatch, &mut stdout, &mut stderr).expect("run ok");
        assert_eq!(code_mismatch, 1);
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn test_run_validate_strict_warnings() {
        let dir = std::env::temp_dir().join(format!("hcl_validate_strict_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let warn_file = dir.join("warn.hcl");
        fs::write(&warn_file, "val = true ? 1 : 2\n").expect("write warn");
        let opts_strict = ValidateOptions {
            schema_path: None,
            strict: true,
            json: false,
            targets: vec![warn_file.to_string_lossy().to_string()],
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code_strict =
            run_validate(&opts_strict, &mut stdout, &mut stderr).expect("strict validate");
        assert_eq!(code_strict, 2);
        let stderr_str = String::from_utf8_lossy(&stderr);
        assert!(stderr_str.contains("warning:"));
        let opts_non_strict = ValidateOptions {
            schema_path: None,
            strict: false,
            json: false,
            targets: vec![warn_file.to_string_lossy().to_string()],
        };
        let mut stdout_ns = Vec::new();
        let mut stderr_ns = Vec::new();
        let code_ns =
            run_validate(&opts_non_strict, &mut stdout_ns, &mut stderr_ns).expect("non strict");
        assert_eq!(code_ns, 0);
        let clean_file = dir.join("clean.hcl");
        fs::write(&clean_file, "a = 10\n").expect("write clean");
        let opts_clean_strict = ValidateOptions {
            schema_path: None,
            strict: true,
            json: false,
            targets: vec![clean_file.to_string_lossy().to_string()],
        };
        let mut stdout_cs = Vec::new();
        let mut stderr_cs = Vec::new();
        let code_cs =
            run_validate(&opts_clean_strict, &mut stdout_cs, &mut stderr_cs).expect("clean strict");
        assert_eq!(code_cs, 0);
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn test_validate_options_and_specs_derives() {
        let opts = ValidateOptions::default();
        let opts_clone = opts.clone();
        assert_eq!(opts, opts_clone);
        assert!(format!("{opts:?}").contains("ValidateOptions"));
        let attr = AttrSpecJson::default();
        let attr_clone = attr.clone();
        assert!(format!("{attr_clone:?}").contains("AttrSpecJson"));
        let block = BlockSpecJson::default();
        let block_clone = block.clone();
        assert!(format!("{block_clone:?}").contains("BlockSpecJson"));
        let schema = SchemaSpecJson::default();
        let schema_clone = schema.clone();
        assert!(format!("{schema_clone:?}").contains("SchemaSpecJson"));
    }
    #[test]
    fn test_main_function() {
        let dir = std::env::temp_dir().join(format!("hcl_validate_main_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let valid_file = dir.join("valid.hcl");
        fs::write(&valid_file, "foo = \"bar\"\n").expect("write valid");
        set_test_args(vec![valid_file.to_string_lossy().to_string()]);
        assert!(main().is_ok());
        let bad_file = dir.join("bad.hcl");
        fs::write(&bad_file, "{ syntax error").expect("write bad");
        set_test_args(vec![bad_file.to_string_lossy().to_string()]);
        let res_err = main();
        assert!(
            res_err
                .expect_err("expected failure")
                .contains("Validation failed with exit code 1")
        );
        set_test_args(vec!["--unknown".to_string()]);
        let res_arg_err = main();
        assert!(
            res_arg_err
                .expect_err("expected parse error")
                .contains("Unknown flag")
        );
        let res_default = main();
        assert!(
            res_default
                .expect_err("expected help error")
                .contains("Usage:")
        );
        set_test_args(vec![
            "--schema".to_string(),
            dir.join("nonexistent_schema.json")
                .to_string_lossy()
                .to_string(),
            valid_file.to_string_lossy().to_string(),
        ]);
        assert!(main().is_err());
        let bad_schema = dir.join("bad_schema.json");
        fs::write(&bad_schema, "not json {").expect("write bad schema");
        set_test_args(vec![
            "--schema".to_string(),
            bad_schema.to_string_lossy().to_string(),
            valid_file.to_string_lossy().to_string(),
        ]);
        assert!(main().is_err());
        let bad_utf8 = dir.join("bad_utf8.hcl");
        fs::write(&bad_utf8, [0xFF, 0xFE]).expect("write bad utf8");
        set_test_args(vec![bad_utf8.to_string_lossy().to_string()]);
        assert!(main().is_err());
        set_test_args(vec![
            "--strict".to_string(),
            valid_file.to_string_lossy().to_string(),
        ]);
        assert!(main().is_ok());
        set_test_args(vec![
            "--json".to_string(),
            valid_file.to_string_lossy().to_string(),
        ]);
        assert!(main().is_ok());
        let warn_file = dir.join("warn.hcl");
        fs::write(&warn_file, "val = true ? 1 : 2\n").expect("write warn");
        set_test_args(vec![
            "--strict".to_string(),
            warn_file.to_string_lossy().to_string(),
        ]);
        let res_warn_strict = main();
        assert!(
            res_warn_strict
                .expect_err("expected warn exit")
                .contains("Validation failed with exit code 2")
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
