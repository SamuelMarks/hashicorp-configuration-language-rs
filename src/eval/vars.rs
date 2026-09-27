//! CLI Variable and Var-File Ingestion Subsystem.
//!
//! Provides parsing and ingestion for command-line variable flags (`-var 'key=value'`)
//! and variable files (`*.pkrvars.hcl`, `*.tfvars`, `*.pkrvars.json`, `*.tfvars.json`),
//! with automatic type inference (booleans, numbers, compound collections), sequential
//! merging, and precedence resolution.

use crate::ast::structure::Body;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::error::HclError;
use crate::eval::context::Context;
use crate::eval::evaluator::Evaluator;
use crate::number::Number;
use crate::parse::json::parse_json;
use crate::parse::parser::Parser;
use crate::span::Span;
use crate::types::ty::Type;
use crate::types::val::{Value, ValueData};
use bigdecimal::BigDecimal;
use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

/// Parses a single `-var 'key=value'` command-line flag argument into a key and typed [`Value`].
///
/// Automatic inference order:
/// 1. Booleans (`true` / `false`) and `null`.
/// 2. HCL expression evaluation (numbers, tuples `[1, 2]`, maps/objects `{a = 1}`).
/// 3. JSON compound literals (arrays `["a", "b"]`, objects `{"k": "v"}`).
/// 4. Arbitrary precision numeric literals (`BigDecimal`).
/// 5. Unquoted or fallback literal strings.
///
/// # Arguments
/// * `raw` - The raw string in `key=value` format.
///
/// # Errors
/// Returns [`HclError::Parse`] if the string does not contain an equals sign (`=`) or if the key is empty.
pub fn parse_var_flag(raw: &str) -> Result<(String, Value), HclError> {
    let Some(eq_idx) = raw.find('=') else {
        return Err(HclError::Parse(format!(
            "Invalid variable assignment '{raw}': missing '=' separator (expected 'key=value')"
        )));
    };

    let key = raw[..eq_idx].trim().to_string();
    if key.is_empty() {
        return Err(HclError::Parse(
            "Variable key cannot be empty in 'key=value'".to_string(),
        ));
    }

    let val_str = raw[eq_idx + 1..].trim();
    let val = parse_var_value(val_str);
    Ok((key, val))
}

/// Parses a raw variable value string into a typed [`Value`].
///
/// # Arguments
/// * `val_str` - The value string to parse.
#[must_use]
pub fn parse_var_value(val_str: &str) -> Value {
    // 1. Literal booleans & null
    if val_str == "true" {
        return Value::new(Type::Bool, ValueData::Bool(true));
    }
    if val_str == "false" {
        return Value::new(Type::Bool, ValueData::Bool(false));
    }
    if val_str == "null" {
        return Value::null(Type::Dynamic);
    }

    // 2. Try evaluating as an HCL expression if it looks like a compound expression
    if (val_str.starts_with('[') && val_str.ends_with(']'))
        || (val_str.starts_with('{') && val_str.ends_with('}'))
        || (val_str.starts_with('"') && val_str.ends_with('"'))
    {
        let ctx = Context::new();
        let mut parser = Parser::new(val_str);
        if let Some(expr) = parser.parse_expression()
            && !parser.errors().has_errors()
            && let Ok((v, _)) = Evaluator::new(&ctx).evaluate(&expr)
        {
            return v;
        }
    }

    // 3. Try parsing as a number
    if let Ok(num) = BigDecimal::from_str(val_str) {
        return Value::new(Type::Number, ValueData::Number(Number::new(num)));
    }

    // 4. Fallback: literal string
    Value::new(Type::String, ValueData::String(val_str.to_string()))
}

/// Parses a variable file in HCL (`*.pkrvars.hcl`, `*.tfvars`) or JSON (`*.pkrvars.json`, `*.tfvars.json`) format.
///
/// # Arguments
/// * `path` - The path to the variable file.
///
/// # Errors
/// Returns [`Diagnostics`] if reading or parsing the file fails.
pub fn parse_var_file<P: AsRef<Path>>(path: P) -> Result<BTreeMap<String, Value>, Diagnostics> {
    parse_var_file_impl(path.as_ref())
}

/// Internal implementation of variable file parsing operating on a borrowed path reference.
fn parse_var_file_impl(p: &Path) -> Result<BTreeMap<String, Value>, Diagnostics> {
    let display_name = p.to_string_lossy().to_string();

    let content = match std::fs::read_to_string(p) {
        Ok(c) => c,
        Err(err) => {
            let mut diags = Diagnostics::new();
            diags.push(
                Diagnostic::error(
                    "Failed to read variable file",
                    format!("Could not read file '{display_name}': {err}"),
                    Span::new(0, 0, 1, 1, 1, 1),
                )
                .with_error(HclError::Io(err.to_string())),
            );
            return Err(diags);
        }
    };

    let is_json = p
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));

    let body: Body = if is_json {
        parse_json(&content)?
    } else {
        let file_arc = Some(Arc::from(display_name.as_str()));
        let mut parser = Parser::new_with_file(&content, file_arc);
        let b = parser.parse_body();
        if parser.errors().has_errors() {
            return Err(parser.errors().clone());
        }
        b
    };

    let mut result = BTreeMap::new();
    let mut diags = Diagnostics::new();
    let ctx = Context::with_stdlib();

    for (k, attr) in body.attributes {
        let eval = Evaluator::new(&ctx);
        match eval.evaluate(&attr.expr) {
            Ok((v, _)) => {
                result.insert(k, v);
            }
            Err(e) => {
                for diag in e.errors() {
                    diags.push(diag.clone());
                }
            }
        }
    }

    if diags.has_errors() {
        Err(diags)
    } else {
        Ok(result)
    }
}

/// Variable ingestion manager for CLI flags and variable files.
///
/// Ingests multiple `-var-file` configurations in sequence, applying later values
/// over earlier ones, and applies individual `-var` flag overrides with highest precedence.
#[derive(Debug, Clone, Default)]
pub struct VarManager {
    /// Ingested variables map.
    variables: BTreeMap<String, Value>,
}

impl VarManager {
    /// Creates a new, empty `VarManager`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ingests a variable file, merging its definitions.
    ///
    /// # Arguments
    /// * `path` - The path to the variable file.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if file reading or evaluation fails.
    pub fn add_var_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Diagnostics> {
        self.add_var_file_impl(path.as_ref())
    }

    /// Internal implementation of adding a variable file from a borrowed path reference.
    fn add_var_file_impl(&mut self, path: &Path) -> Result<(), Diagnostics> {
        let vars = parse_var_file_impl(path)?;
        self.variables.extend(vars);
        Ok(())
    }

    /// Ingests a single `-var 'key=value'` command-line flag argument.
    ///
    /// # Arguments
    /// * `raw` - The raw string in `key=value` format.
    ///
    /// # Errors
    /// Returns [`HclError::Parse`] if the format is invalid.
    pub fn add_var_flag(&mut self, raw: &str) -> Result<(), HclError> {
        let (key, val) = parse_var_flag(raw)?;
        self.variables.insert(key, val);
        Ok(())
    }

    /// Extends the manager with an iterator of `(key, value)` pairs.
    ///
    /// # Arguments
    /// * `iter` - Iterator yielding `(String, Value)` tuples.
    pub fn extend<I: IntoIterator<Item = (String, Value)>>(&mut self, iter: I) {
        self.variables.extend(iter);
    }

    /// Populates a [`Context`] with the resolved variables in the `"var"` namespace scope.
    ///
    /// # Arguments
    /// * `ctx` - The context to populate.
    pub fn populate_context(&self, ctx: &mut Context<'_>) {
        for (k, v) in &self.variables {
            ctx.set_var(k.clone(), v.clone());
        }
    }

    /// Returns a reference to the merged variables map.
    #[must_use]
    pub fn variables(&self) -> &BTreeMap<String, Value> {
        &self.variables
    }

    /// Consumes the manager and returns the merged variables map.
    #[must_use]
    pub fn into_variables(self) -> BTreeMap<String, Value> {
        self.variables
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_var_flag_primitives_and_compounds() {
        // Boolean
        let (k1, v1) = parse_var_flag("enabled=true").expect("parse ok");
        assert_eq!(k1, "enabled");
        assert_eq!(v1.data.as_ref(), &ValueData::Bool(true));

        let (k2, v2) = parse_var_flag("disabled=false").expect("parse ok");
        assert_eq!(k2, "disabled");
        assert_eq!(v2.data.as_ref(), &ValueData::Bool(false));

        // Null
        let (kn, vn) = parse_var_flag("empty=null").expect("parse ok");
        assert_eq!(kn, "empty");
        assert!(vn.is_null());

        // Number
        let (k3, v3) = parse_var_flag("port=8080").expect("parse ok");
        assert_eq!(k3, "port");
        assert_eq!(v3.ty(), &Type::Number);

        // String
        let (k4, v4) = parse_var_flag("env=\"production\"").expect("parse ok");
        assert_eq!(k4, "env");
        assert_eq!(v4.data.as_ref(), &ValueData::String("production".into()));

        let (k5, v5) = parse_var_flag("raw_str=staging").expect("parse ok");
        assert_eq!(k5, "raw_str");
        assert_eq!(v5.data.as_ref(), &ValueData::String("staging".into()));

        // Tuple/List
        let (k6, v6) = parse_var_flag("ports=[80, 443]").expect("parse ok");
        assert_eq!(k6, "ports");
        assert!(matches!(v6.ty(), Type::Tuple(_) | Type::List(_)));

        // Object
        let (k7, v7) = parse_var_flag("tags={\"env\": \"prod\"}").expect("parse ok");
        assert_eq!(k7, "tags");
        assert!(matches!(v7.data.as_ref(), ValueData::Object(_)));

        // Compound evaluation failure fallback to string
        let (k8, v8) = parse_var_flag("bad_eval=[1 / 0]").expect("parse ok");
        assert_eq!(k8, "bad_eval");
        assert_eq!(v8.data.as_ref(), &ValueData::String("[1 / 0]".into()));

        // Errors
        assert!(parse_var_flag("invalid_no_equals").is_err());
        assert!(parse_var_flag("=missing_key").is_err());
    }

    #[test]
    fn test_var_manager_and_var_files() {
        let temp_dir = std::env::temp_dir().join(format!("test_vm_dir_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("create tempdir");

        let f_hcl = temp_dir.join("test.pkrvars.hcl");
        std::fs::write(&f_hcl, "env = \"dev\"\ncount = 1\n").expect("write hcl");

        let f_json = temp_dir.join("test.pkrvars.json");
        std::fs::write(
            &f_json,
            "{\n  \"env\": \"staging\",\n  \"active\": true\n}\n",
        )
        .expect("write json");

        let mut mgr = VarManager::new();
        mgr.add_var_file(&f_hcl).expect("add hcl file");
        assert_eq!(mgr.variables().len(), 2);

        // JSON overrides env="dev" with env="staging" and adds active=true
        mgr.add_var_file(&f_json).expect("add json file");
        assert_eq!(mgr.variables().len(), 3);
        assert_eq!(
            mgr.variables()
                .get("env")
                .expect("env exists")
                .data
                .as_ref(),
            &ValueData::String("staging".into())
        );

        // CLI flag override takes highest precedence
        mgr.add_var_flag("env=prod").expect("add var flag");
        assert_eq!(
            mgr.variables()
                .get("env")
                .expect("env exists")
                .data
                .as_ref(),
            &ValueData::String("prod".into())
        );

        // Populate context
        let mut ctx = Context::new();
        mgr.populate_context(&mut ctx);
        let var_scope = ctx.get_variable("var").expect("var scope");
        assert!(var_scope.to_string().contains("prod"));

        // Extension method
        let mut extra = BTreeMap::new();
        extra.insert(
            "custom".to_string(),
            Value::new(Type::Bool, ValueData::Bool(true)),
        );
        mgr.extend(extra);
        assert!(mgr.variables().contains_key("custom"));

        let final_vars = mgr.into_variables();
        assert_eq!(final_vars.len(), 4);

        // Test missing file error
        let bad_file = temp_dir.join("missing.pkrvars.hcl");
        assert!(parse_var_file(bad_file).is_err());

        // Test syntax error in HCL var file
        let syntax_err_file = temp_dir.join("syntax_err.pkrvars.hcl");
        std::fs::write(&syntax_err_file, "bad = = syntax").expect("write syntax err");
        assert!(parse_var_file(&syntax_err_file).is_err());

        // Test evaluation error in HCL var file
        let eval_err_file = temp_dir.join("eval_err.pkrvars.hcl");
        std::fs::write(&eval_err_file, "bad_num = 10 / 0\n").expect("write eval err");
        assert!(parse_var_file(&eval_err_file).is_err());

        // Test invalid JSON var file
        let bad_json_file = temp_dir.join("bad.pkrvars.json");
        std::fs::write(&bad_json_file, "{ invalid json }").expect("write bad json");
        assert!(parse_var_file(&bad_json_file).is_err());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_var_manager_coverage_gaps() {
        // 1. Unclosed compound prefixes (starts_with is true, but ends_with is false)
        let val_bracket = parse_var_value("[unclosed_bracket");
        assert_eq!(
            val_bracket.data.as_ref(),
            &ValueData::String("[unclosed_bracket".into())
        );

        let val_brace = parse_var_value("{unclosed_brace");
        assert_eq!(
            val_brace.data.as_ref(),
            &ValueData::String("{unclosed_brace".into())
        );

        let val_quote = parse_var_value("\"unclosed_quote");
        assert_eq!(
            val_quote.data.as_ref(),
            &ValueData::String("\"unclosed_quote".into())
        );

        // 2. Syntax errors inside compound expressions
        let val_bad_syntax_list = parse_var_value("[ 1, + ]");
        assert_eq!(
            val_bad_syntax_list.data.as_ref(),
            &ValueData::String("[ 1, + ]".into())
        );

        let val_bad_syntax_map = parse_var_value("{ key = = val }");
        assert_eq!(
            val_bad_syntax_map.data.as_ref(),
            &ValueData::String("{ key = = val }".into())
        );

        // 2b. Parser returns None for invalid prefix token after [
        let val_bad_prefix = parse_var_value("[ = ]");
        assert_eq!(
            val_bad_prefix.data.as_ref(),
            &ValueData::String("[ = ]".into())
        );

        // 2c. Parser returns Some but parser.errors().has_errors() is true (e.g. namespaced ident without call)
        let val_ns_ident = parse_var_value("[ ns::var ]");
        assert_eq!(
            val_ns_ident.data.as_ref(),
            &ValueData::String("[ ns::var ]".into())
        );

        // 3. Evaluation error inside compound expression (e.g. division by zero in map)
        let val_bad_eval_map = parse_var_value("{ div_zero = 10 / 0 }");
        assert_eq!(
            val_bad_eval_map.data.as_ref(),
            &ValueData::String("{ div_zero = 10 / 0 }".into())
        );

        // 4. VarManager error early returns via `?` operator
        let mut mgr = VarManager::new();
        let add_flag_res = mgr.add_var_flag("missing_equals_sign");
        assert!(add_flag_res.is_err());
        assert!(mgr.add_var_flag("flag_ok=42").is_ok());

        assert!(parse_var_flag("   =value").is_err());

        let temp_dir = std::env::temp_dir().join(format!("test_vm_gaps_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);

        let missing_file = temp_dir.join("nonexistent_vars.tfvars");
        let add_file_res = mgr.add_var_file(&missing_file);
        assert!(add_file_res.is_err());

        // File with no extension parsed as HCL body
        let no_ext_file = temp_dir.join("vars_without_ext");
        assert!(std::fs::write(&no_ext_file, "flag = true\n").is_ok());
        let parsed_no_ext = parse_var_file(&no_ext_file).expect("parsed ok");
        assert!(parsed_no_ext.contains_key("flag"));

        // File with uppercase .JSON extension parsed as JSON
        let upper_json_file = temp_dir.join("vars.JSON");
        assert!(std::fs::write(&upper_json_file, "{\"active\": false}").is_ok());
        let parsed_upper = parse_var_file(&upper_json_file).expect("parsed ok");
        assert!(parsed_upper.contains_key("active"));

        // 5. Clone and Debug on VarManager
        let mgr_cloned = mgr.clone();
        let debug_str = format!("{mgr_cloned:?}");
        assert!(debug_str.contains("VarManager"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
