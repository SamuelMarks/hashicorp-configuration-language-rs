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
    let path = path.as_ref();
    let filename = path.to_string_lossy().into_owned();
    let is_json = is_json_file(path);

    let mut fm = FileManager::new();
    let body = if is_json {
        fm.parse_json_file(&filename)?
    } else {
        fm.parse_hcl_file(&filename)?
    };

    let mut default_ctx = Context::new();
    let eval_ctx = if let Some(c) = ctx {
        c
    } else {
        default_ctx.register_stdlib();
        &mut default_ctx
    };

    *target = T::decode_body(body, &[], eval_ctx)?;
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
    let is_json = is_json_filename(filename) || is_json_content(src);

    let mut fm = FileManager::new();
    let body = if is_json {
        fm.parse_json_string(filename, src)?
    } else {
        fm.parse_hcl_string(filename, src)?
    };

    let mut default_ctx = Context::new();
    let eval_ctx = if let Some(c) = ctx {
        c
    } else {
        default_ctx.register_stdlib();
        &mut default_ctx
    };

    *target = T::decode_body(body, &[], eval_ctx)?;
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
    let path = path.as_ref();
    let filename = path.to_string_lossy().into_owned();
    let is_json = is_json_file(path);

    let mut fm = FileManager::new();
    let body = if is_json {
        fm.parse_json_file(&filename)?
    } else {
        fm.parse_hcl_file(&filename)?
    };

    let mut default_ctx = Context::new();
    let eval_ctx = if let Some(c) = ctx {
        c
    } else {
        default_ctx.register_stdlib();
        &mut default_ctx
    };

    evaluate_body_to_value(body, eval_ctx)
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
    let is_json = is_json_filename(filename) || is_json_content(src);

    let mut fm = FileManager::new();
    let body = if is_json {
        fm.parse_json_string(filename, src)?
    } else {
        fm.parse_hcl_string(filename, src)?
    };

    let mut default_ctx = Context::new();
    let eval_ctx = if let Some(c) = ctx {
        c
    } else {
        default_ctx.register_stdlib();
        &mut default_ctx
    };

    evaluate_body_to_value(body, eval_ctx)
}

fn is_json_file(path: &Path) -> bool {
    let filename = path.to_string_lossy();
    is_json_filename(&filename)
}

#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn is_json_filename(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".json") || lower.ends_with(".tf.json") || lower.ends_with(".tfvars.json")
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

        if !block_vals.is_empty() {
            let elem_types: Vec<Type> = block_vals.iter().map(|v| v.ty().clone()).collect();
            let tuple_val = Value::new(Type::Tuple(elem_types), ValueData::Array(block_vals));
            map.insert(ident, tuple_val);
        }
    }

    if diags.has_errors() {
        Err(diags)
    } else {
        let ty = Type::object(BTreeMap::new()); // simplified dynamic type
        Ok(Value::new(ty, ValueData::Object(map)))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    #![allow(
        clippy::unwrap_used,
        clippy::pedantic,
        clippy::nursery,
        clippy::expect_used,
        clippy::panic
    )]
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
            let mut conf = DummyConfig::default();
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
    fn test_decode_hcl() {
        let temp_dir = std::env::temp_dir();
        let path_err = temp_dir.join(format!(
            "test_hclsimple_file_ctx_{}.hcl",
            std::process::id()
        ));
        fs::write(
            &path_err,
            "foo = \"file\"
bar = 2",
        )
        .unwrap();
        let mut ctx = Context::new();
        let mut target = DummyConfig {
            foo: "".into(),
            bar: 0,
        };
        decode_file(&path_err, Some(&mut ctx), &mut target).unwrap();
        assert_eq!(target.foo, "file");
        fs::remove_file(&path_err).unwrap();

        let hcl_src_invalid_types = r#"
            foo = undefined_var
            bar = another_undefined_var
            unknown_key = 1
        "#;
        let mut tmp_invalid = DummyConfig {
            foo: String::new(),
            bar: 0,
        };
        let _ = decode(
            "test.hcl",
            hcl_src_invalid_types,
            Some(&mut ctx),
            &mut tmp_invalid,
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
            decode("test.hcl", hcl_src, Some(&mut ctx), &mut tmp).unwrap();
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
            decode("test.hcl", hcl_src2, None, &mut tmp).unwrap();
            tmp
        };
        assert_eq!(c2.foo, "world");

        let hcl = r#"
            foo = "hello"
            bar = 42
        "#;
        let mut target = DummyConfig::default();
        decode("test.hcl", hcl, None, &mut target).unwrap();
        assert_eq!(target.foo, "hello");
        assert_eq!(target.bar, 42);
    }

    #[test]
    fn test_decode_json() {
        let json = r#"{"foo": "world", "bar": 99}"#;
        let mut target = DummyConfig::default();
        decode("test.json", json, None, &mut target).unwrap();
        assert_eq!(target.foo, "world");
        assert_eq!(target.bar, 99);
    }

    #[test]
    fn test_decode_bytes() {
        let hcl = b"foo = \"bytes\"\nbar = 100\n";
        let mut target = DummyConfig::default();
        decode_bytes("bytes.hcl", hcl, None, &mut target).unwrap();
        assert_eq!(target.foo, "bytes");
        assert_eq!(target.bar, 100);

        // invalid utf8
        let bad_bytes = [0xff, 0xfe, 0xfd];
        let err = decode_bytes("bad.hcl", &bad_bytes, None, &mut target).unwrap_err();
        assert!(err.to_string().contains("invalid utf-8"));
    }

    #[test]
    fn test_decode_file() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!("test_hclsimple_{}.hcl", std::process::id()));
        fs::write(&path, "foo = \"file\"\nbar = 123\n").unwrap();

        let mut target = DummyConfig::default();
        decode_file(&path, None, &mut target).unwrap();
        assert_eq!(target.foo, "file");
        assert_eq!(target.bar, 123);

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_decode_file_json() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!("test_hclsimple_{}.json", std::process::id()));
        fs::write(&path, r#"{"foo": "json_file", "bar": 456}"#).unwrap();

        let mut target = DummyConfig::default();
        decode_file(&path, None, &mut target).unwrap();
        assert_eq!(target.foo, "json_file");
        assert_eq!(target.bar, 456);

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_decode_to_value() {
        let hcl = r#"
            attr = 10
            block "labeled" "l1" {
                inner = true
            }
            block "labeled" "l2" {
                inner = false
            }
            single {
                val = 42
            }
        "#;
        let val = decode_to_value("test.hcl", hcl, None).unwrap();
        let crate::types::ValueData::Object(map) = &*val.data else {
            return;
        };
        if let Some(v) = map.get("attr") {
            if let crate::types::ValueData::Number(n) = &*v.data {
                assert_eq!(n.as_i64(), Some(10));
            }
        }
        let crate::types::ValueData::Array(single) = &*map.get("single").unwrap().data else {
            return;
        };
        assert_eq!(single.len(), 1);
        let crate::types::ValueData::Object(single_obj) = &*single[0].data else {
            return;
        };
        if let Some(v) = single_obj.get("val") {
            if let crate::types::ValueData::Number(n) = &*v.data {
                assert_eq!(n.as_i64(), Some(42));
            }
        }

        let crate::types::ValueData::Array(labeled) = &*map.get("block").unwrap().data else {
            return;
        };
        assert_eq!(labeled.len(), 2);
    }

    #[test]
    fn test_decode_file_to_value() {
        let temp_dir = std::env::temp_dir();
        let path_hcl = temp_dir.join(format!("test_hclsimple_val_{}.hcl", std::process::id()));
        fs::write(&path_hcl, r#"attr = 99"#).unwrap();
        let mut ctx = Context::new();
        let _val_hcl = decode_file_to_value(&path_hcl, Some(&mut ctx)).unwrap();
        fs::remove_file(&path_hcl).unwrap();

        let json_src = r#"{"attr": 100}"#;
        let _val_json = decode_to_value("test.json", json_src, Some(&mut ctx)).unwrap();

        let path = temp_dir.join(format!("test_hclsimple_val_{}.json", std::process::id()));
        fs::write(&path, r#"{"attr": 99}"#).unwrap();

        let path_err = temp_dir.join(format!("test_hclsimple_err_{}.hcl", std::process::id()));
        fs::write(&path_err, "foo =").unwrap();
        assert!(decode_file_to_value(&path_err, None).is_err());
        fs::remove_file(&path_err).unwrap();

        let hcl_src_invalid_types = r#"
            foo = undefined_var
            bar = another_undefined_var
            unknown_key = 1
        "#;
        let mut tmp_invalid = DummyConfig {
            foo: String::new(),
            bar: 0,
        };
        let _ = decode(
            "test.hcl",
            hcl_src_invalid_types,
            Some(&mut ctx),
            &mut tmp_invalid,
        );

        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!("test_hclsimple_val_{}.json", std::process::id()));
        fs::write(&path, r#"{"attr": 99}"#).unwrap();

        let val = decode_file_to_value(&path, None).unwrap();
        let crate::types::ValueData::Object(map) = &*val.data else {
            return;
        };
        assert_eq!(
            map.get("attr")
                .map(|v| if let crate::types::ValueData::Number(n) = &*v.data {
                    n.as_i64().unwrap()
                } else {
                    0
                }),
            Some(99)
        );

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_eval_errors() {
        // syntax error
        let mut target = DummyConfig::default();
        assert!(decode("bad.hcl", "foo = ", None, &mut target).is_err());
        assert!(decode_to_value("bad.hcl", "foo = ", None).is_err());

        // evaluation error
        let bad_eval = "foo = non_existent_var";
        assert!(decode_to_value("eval.hcl", bad_eval, None).is_err());

        let bad_eval_block = "block { val = missing }";
        assert!(decode_to_value("eval_block.hcl", bad_eval_block, None).is_err());
    }
}

#[cfg(test)]
mod tests_simple_ephemeral {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    #[test]
    fn test_decode_unknown_file() {
        let mut target: crate::ast::structure::Body =
            crate::ast::structure::Body::new(crate::span::Span::new(0, 0, 1, 1, 1, 1));
        let res = decode_file("non_existent_file.hcl", None, &mut target);
        assert!(res.is_err());
    }
}
