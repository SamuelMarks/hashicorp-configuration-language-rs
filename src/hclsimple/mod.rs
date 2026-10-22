//! Single-Step Loader API (`hclsimple` Parity).
//!
//! Exposes straightforward functions to decode HCL and JSON configurations
//! from files and strings into strongly typed structures, mirroring the Go `hclsimple` package.

#![deny(missing_docs)]

use crate::ast::structure::{Block, Body};
use crate::decode::DecodeBody;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::error::HclError;
use crate::eval::{context::Context, evaluator::Evaluator};
use crate::parse::file_manager::FileManager;
use crate::span::Span;
use crate::types::{Type, Value, ValueData};
use std::collections::BTreeMap;
use std::path::Path;

fn parse_file_auto(fm: &mut FileManager, path: &Path) -> Result<Body, Diagnostics> {
    let filename = path.to_string_lossy().into_owned();
    if is_json_file(path) {
        fm.parse_json_file(&filename).cloned()
    } else {
        fm.parse_hcl_file(&filename).cloned()
    }
}

fn parse_str_auto(fm: &mut FileManager, filename: &str, src: &str) -> Result<Body, Diagnostics> {
    if is_json_filename(filename) || is_json_content(src) {
        fm.parse_json_string(filename, src).cloned()
    } else {
        fm.parse_hcl_string(filename, src).cloned()
    }
}

fn with_or_default_ctx<R>(ctx: Option<&mut Context>, f: impl FnOnce(&mut Context) -> R) -> R {
    if let Some(c) = ctx {
        f(c)
    } else {
        let mut default_ctx = Context::with_stdlib();
        f(&mut default_ctx)
    }
}

/// Decodes an HCL or JSON file into a strongly typed target struct.
///
/// Automatically detects syntax profile from file extensions (`.json`, etc.).
///
/// # Arguments
/// * `path` - The path to the file.
/// * `ctx` - Optional evaluation context for variables and functions.
/// * `target` - A mutable reference to the target struct to populate.
///
/// # Errors
/// Returns [`Diagnostics`] on I/O failure, parsing error, or decoding mismatch.
pub fn decode_file<T: DecodeBody>(
    path: impl AsRef<Path>,
    ctx: Option<&mut Context>,
    target: &mut T,
) -> Result<(), Diagnostics> {
    let mut fm = FileManager::new();
    let body = parse_file_auto(&mut fm, path.as_ref())?;
    *target = with_or_default_ctx(ctx, |eval_ctx| T::decode_body(&body, &[], eval_ctx))?;
    Ok(())
}

/// Decodes an HCL or JSON string into a strongly typed target struct.
///
/// # Arguments
/// * `filename` - Logical name of the source (used for diagnostics and format detection).
/// * `src` - The source string.
/// * `ctx` - Optional evaluation context for variables and functions.
/// * `target` - A mutable reference to the target struct to populate.
///
/// # Errors
/// Returns [`Diagnostics`] on parsing error or decoding mismatch.
pub fn decode<T: DecodeBody>(
    filename: &str,
    src: &str,
    ctx: Option<&mut Context>,
    target: &mut T,
) -> Result<(), Diagnostics> {
    let mut fm = FileManager::new();
    let body = parse_str_auto(&mut fm, filename, src)?;
    *target = with_or_default_ctx(ctx, |eval_ctx| T::decode_body(&body, &[], eval_ctx))?;
    Ok(())
}

/// Decodes HCL or JSON bytes into a strongly typed target struct.
///
/// # Arguments
/// * `filename` - Logical name of the source.
/// * `bytes` - The source bytes.
/// * `ctx` - Optional evaluation context.
/// * `target` - A mutable reference to the target struct to populate.
///
/// # Errors
/// Returns [`Diagnostics`] on non-UTF-8 bytes, parsing error, or decoding mismatch.
pub fn decode_bytes<T: DecodeBody>(
    filename: &str,
    bytes: &[u8],
    ctx: Option<&mut Context>,
    target: &mut T,
) -> Result<(), Diagnostics> {
    let src = std::str::from_utf8(bytes).map_err(|e| {
        let mut diags = Diagnostics::new();
        diags.push(
            Diagnostic::error(
                "Invalid UTF-8",
                format!("File contains invalid UTF-8 bytes: {e}"),
                Span::new(0, 0, 1, 1, 1, 1),
            )
            .with_error(HclError::Io(e.to_string())),
        );
        diags
    })?;
    decode(filename, src, ctx, target)
}

/// Decodes an HCL or JSON file to a runtime `Value`.
///
/// # Arguments
/// * `path` - The path to the file.
/// * `ctx` - Optional evaluation context.
///
/// # Errors
/// Returns [`Diagnostics`] on I/O failure, parsing error, or evaluation error.
pub fn decode_file_to_value(
    path: impl AsRef<Path>,
    ctx: Option<&mut Context>,
) -> Result<Value, Diagnostics> {
    let mut fm = FileManager::new();
    let body = parse_file_auto(&mut fm, path.as_ref())?;
    with_or_default_ctx(ctx, |eval_ctx| evaluate_body_to_value(&body, eval_ctx))
}

/// Decodes an HCL or JSON string to a runtime `Value`.
///
/// # Arguments
/// * `filename` - Logical name of the source.
/// * `src` - The source string.
/// * `ctx` - Optional evaluation context.
///
/// # Errors
/// Returns [`Diagnostics`] on parsing error or evaluation error.
pub fn decode_to_value(
    filename: &str,
    src: &str,
    ctx: Option<&mut Context>,
) -> Result<Value, Diagnostics> {
    let mut fm = FileManager::new();
    let body = parse_str_auto(&mut fm, filename, src)?;
    with_or_default_ctx(ctx, |eval_ctx| evaluate_body_to_value(&body, eval_ctx))
}

fn is_json_file(path: &Path) -> bool {
    let filename = path.to_string_lossy();
    is_json_filename(&filename)
}

fn is_json_filename(filename: &str) -> bool {
    Path::new(filename)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

fn is_json_content(src: &str) -> bool {
    let trimmed = src.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn evaluate_body_to_value(body: &Body, ctx: &mut Context) -> Result<Value, Diagnostics> {
    let mut map = BTreeMap::new();
    let mut diags = Diagnostics::new();

    for (k, attr) in &body.attributes {
        match Evaluator::new(&*ctx).evaluate(&attr.expr) {
            Ok((val, d)) => {
                map.insert(k.clone(), val);
                diags.extend(d);
            }
            Err(d) => diags.extend(d),
        }
    }

    let mut block_groups: BTreeMap<String, Vec<&Block>> = BTreeMap::new();
    for block in &body.blocks {
        block_groups
            .entry(block.block_type.clone())
            .or_default()
            .push(block);
    }

    for (ident, blocks) in block_groups {
        let mut block_vals = Vec::new();
        for block in blocks {
            let b_val = match evaluate_body_to_value(&block.body, ctx) {
                Ok(b) => b,
                Err(d) => {
                    diags.extend(d);
                    Value::new(
                        Type::object(BTreeMap::new()),
                        ValueData::Object(BTreeMap::new()),
                    )
                }
            };
            if block.labels.is_empty() {
                block_vals.push(b_val);
            } else {
                let mut current_val = b_val;
                for label in block.labels.iter().rev() {
                    let mut obj_map = BTreeMap::new();
                    obj_map.insert(label.clone(), current_val);
                    current_val =
                        Value::new(Type::object(BTreeMap::new()), ValueData::Object(obj_map));
                }
                block_vals.push(current_val);
            }
        }

        let elem_types: Vec<Type> = block_vals.iter().map(|v| v.ty().clone()).collect();
        let tuple_val = Value::new(Type::Tuple(elem_types), ValueData::Array(block_vals));
        map.insert(ident, tuple_val);
    }

    if diags.has_errors() {
        Err(diags)
    } else {
        let ty = Type::object(BTreeMap::new()); // simplified dynamic type
        Ok(Value::new(ty, ValueData::Object(map)))
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::ast::structure::Body;
    use crate::decode::DecodeBody;
    use crate::diagnostic::Diagnostics;
    use crate::eval::context::Context;
    use std::fs;

    #[derive(Debug, PartialEq, Default)]
    struct DummyConfig {
        pub foo: String,
        pub bar: i64,
    }

    impl DecodeBody for DummyConfig {
        fn decode_body(
            body: &Body,
            _labels: &[String],
            ctx: &mut Context,
        ) -> Result<Self, Diagnostics> {
            let mut conf = Self::default();
            for (k, attr) in &body.attributes {
                if k == "foo" {
                    if let Ok((crate::types::Value { data, .. }, _)) =
                        crate::eval::evaluator::Evaluator::new(&*ctx).evaluate(&attr.expr)
                    {
                        if let crate::types::ValueData::String(s) = *data {
                            conf.foo = s;
                        }
                    }
                }
                if k == "bar" {
                    if let Ok((crate::types::Value { data, .. }, _)) =
                        crate::eval::evaluator::Evaluator::new(&*ctx).evaluate(&attr.expr)
                    {
                        if let crate::types::ValueData::Number(n) = *data {
                            conf.bar = n.as_i64().unwrap_or(0);
                        }
                    }
                }
            }
            Ok(conf)
        }
    }

    #[test]
    fn test_decode_hcl() -> Result<(), HclError> {
        let temp_dir = std::env::temp_dir();
        let path_err = temp_dir.join(format!(
            "test_hclsimple_file_ctx_{}.hcl",
            std::process::id()
        ));
        fs::write(
            &path_err,
            "foo = \"file\"
bar = 2",
        )?;
        let mut ctx = Context::new();
        let mut target = DummyConfig {
            foo: String::new(),
            bar: 0,
        };
        decode_file(&path_err, Some(&mut ctx), &mut target)?;
        assert_eq!(target.foo, "file");
        fs::remove_file(&path_err)?;

        let hcl_src_invalid_types = r"
            foo = undefined_var
            bar = another_undefined_var
            unknown_key = 1
        ";
        let mut tmp_invalid = DummyConfig {
            foo: String::new(),
            bar: 0,
        };
        assert!(
            decode(
                "test.hcl",
                hcl_src_invalid_types,
                Some(&mut ctx),
                &mut tmp_invalid,
            )
            .is_ok()
        );

        let mut ctx = Context::new();
        let hcl_src = r#"
            foo = "hello"
            bar = 42
        "#;
        let c: DummyConfig = {
            let mut tmp = DummyConfig {
                foo: String::new(),
                bar: 0,
            };
            decode("test.hcl", hcl_src, Some(&mut ctx), &mut tmp)?;
            tmp
        };
        assert_eq!(c.foo, "hello");
        assert_eq!(c.bar, 42);

        let hcl_src2 = r#"foo = "world""#;
        let c2: DummyConfig = {
            let mut tmp = DummyConfig {
                foo: String::new(),
                bar: 0,
            };
            decode("test.hcl", hcl_src2, None, &mut tmp)?;
            tmp
        };
        assert_eq!(c2.foo, "world");

        let hcl = r#"
            foo = "hello"
            bar = 42
        "#;
        let mut target = DummyConfig::default();
        decode("test.hcl", hcl, None, &mut target)?;
        assert_eq!(target.foo, "hello");
        assert_eq!(target.bar, 42);
        Ok(())
    }

    #[test]
    fn test_decode_json() -> Result<(), HclError> {
        let json = r#"{"foo": "world", "bar": 99}"#;
        let mut target = DummyConfig::default();
        decode("test.json", json, None, &mut target)?;
        assert_eq!(target.foo, "world");
        assert_eq!(target.bar, 99);
        Ok(())
    }

    #[test]
    fn test_decode_bytes() -> Result<(), HclError> {
        let hcl = b"foo = \"bytes\"\nbar = 100\n";
        let mut target = DummyConfig::default();
        decode_bytes("bytes.hcl", hcl, None, &mut target)?;
        assert_eq!(target.foo, "bytes");
        assert_eq!(target.bar, 100);

        // invalid utf8
        let bad_bytes = [0xff, 0xfe, 0xfd];
        let res = decode_bytes("bad.hcl", &bad_bytes, None, &mut target);
        assert!(res.is_err());
        if let Err(err) = res {
            assert!(err.to_string().contains("invalid utf-8"));
        }
        Ok(())
    }

    #[test]
    fn test_decode_file() -> Result<(), HclError> {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!("test_hclsimple_{}.hcl", std::process::id()));
        fs::write(&path, "foo = \"file\"\nbar = 123\n")?;

        let mut target = DummyConfig::default();
        decode_file(&path, None, &mut target)?;
        assert_eq!(target.foo, "file");
        assert_eq!(target.bar, 123);

        fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn test_decode_file_json() -> Result<(), HclError> {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!("test_hclsimple_{}.json", std::process::id()));
        fs::write(&path, r#"{"foo": "json_file", "bar": 456}"#)?;

        let mut target = DummyConfig::default();
        decode_file(&path, None, &mut target)?;
        assert_eq!(target.foo, "json_file");
        assert_eq!(target.bar, 456);

        fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn test_decode_to_value() -> Result<(), HclError> {
        let hcl = r#"
            attr = 10
            single {
                val = 42
            }
            block "labeled" "l1" {
                val = 42
            }
            block "labeled" "l2" {
                val = 42
            }
        "#;
        let mut ctx = Context::new();
        let val = decode_to_value("test.hcl", hcl, Some(&mut ctx))?;
        let dbg = format!("{val:?}");
        assert!(dbg.contains("attr"));
        assert!(dbg.contains("single"));
        assert!(dbg.contains("labeled"));

        let json_src = r#"{"attr": 10}"#;
        let val_json = decode_to_value("test.json", json_src, None)?;
        let dbg_json = format!("{val_json:?}");
        assert!(dbg_json.contains("attr"));

        // Error paths
        let error_hcl = r#"
            attr = undefined_var
            err_block {
                val = undefined_var
            }
            err_block "label" {
                val = undefined_var
            }
        "#;
        let err_res = decode_to_value("test_err.hcl", error_hcl, None);
        assert!(err_res.is_err());
        Ok(())
    }

    #[test]
    fn test_decode_file_to_value() -> Result<(), HclError> {
        let temp_dir = std::env::temp_dir();
        let path_hcl = temp_dir.join(format!("test_hclsimple_val_{}.hcl", std::process::id()));
        fs::write(&path_hcl, "attr = 99")?;
        let mut ctx = Context::new();
        let val_hcl = decode_file_to_value(&path_hcl, Some(&mut ctx))?;
        fs::remove_file(&path_hcl)?;

        let json_src = r#"{"attr": 100}"#;
        let path_json = temp_dir.join(format!("test_hclsimple_val_{}.json", std::process::id()));
        fs::write(&path_json, json_src)?;
        let val_json = decode_file_to_value(&path_json, None)?;
        fs::remove_file(&path_json)?;

        let dbg_hcl = format!("{val_hcl:?}");
        assert!(dbg_hcl.contains("attr"));

        let dbg_json = format!("{val_json:?}");
        assert!(dbg_json.contains("attr"));
        let val_json_with_hcl_ext = decode_to_value("inline.hcl", r#"{"attr": 101}"#, None)?;
        let dbg_inline = format!("{val_json_with_hcl_ext:?}");
        assert!(dbg_inline.contains("attr"));
        assert!(is_json_content("[1, 2, 3]"));
        Ok(())
    }
    #[test]
    fn test_decode_unknown_file() {
        let mut target: crate::ast::structure::Body =
            crate::ast::structure::Body::new(crate::span::Span::new(0, 0, 1, 1, 1, 1));
        let res = decode_file("non_existent_file.hcl", None, &mut target);
        assert!(res.is_err());
    }
}
