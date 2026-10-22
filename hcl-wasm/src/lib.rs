//! WebAssembly bindings for HCL (`hcl-wasm`).
//!
//! Provides JavaScript- and WebAssembly-compatible functions for parsing,
//! formatting, and evaluating HCL configurations.

#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![deny(missing_docs)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::ast::structure::Body;
use hashicorp_configuration_language_rs::cst::format::format_str;
use hashicorp_configuration_language_rs::error::HclError;
use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::eval::dag::resolve_definitions;
use hashicorp_configuration_language_rs::eval::dynblock::expand_dynamic_blocks;
use hashicorp_configuration_language_rs::eval::evaluator::Evaluator;
use hashicorp_configuration_language_rs::parse::parser::Parser;
use hashicorp_configuration_language_rs::types::Type;
use hashicorp_configuration_language_rs::types::json::{
    decode_value_from_json, encode_value_to_json,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Recursively serializes an HCL AST [`Body`] into a structured JSON object.
fn body_to_json(body: &Body) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (k, attr) in &body.attributes {
        map.insert(
            k.clone(),
            serde_json::Value::String(format!("{:?}", attr.expr)),
        );
    }
    if !body.blocks.is_empty() {
        let mut blocks_arr = Vec::with_capacity(body.blocks.len());
        for blk in &body.blocks {
            let mut blk_obj = serde_json::Map::new();
            blk_obj.insert(
                "type".to_string(),
                serde_json::Value::String(blk.block_type.clone()),
            );
            blk_obj.insert(
                "labels".to_string(),
                serde_json::Value::Array(
                    blk.labels
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
            blk_obj.insert("body".to_string(), body_to_json(&blk.body));
            blocks_arr.push(serde_json::Value::Object(blk_obj));
        }
        map.insert("blocks".to_string(), serde_json::Value::Array(blocks_arr));
    }
    serde_json::Value::Object(map)
}

/// Implementation of HCL parsing returning a structured JSON object.
///
/// # Arguments
/// * `source` - HCL source code string.
///
/// # Errors
/// Returns [`HclError`] on parse failure.
pub fn parse_hcl_impl(source: &str) -> Result<serde_json::Value, HclError> {
    let mut parser = Parser::new(source);
    let body = parser.parse_body();
    if parser.errors().has_errors() {
        return Err(HclError::from(parser.errors().clone()));
    }
    Ok(body_to_json(&body))
}

/// Parses an HCL source string into a JavaScript-compatible JSON object representation.
///
/// # Arguments
/// * `source` - The HCL configuration source string.
///
/// # Errors
/// Returns a JavaScript error if parsing encounters syntax or structure errors.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn parse_hcl(source: &str) -> Result<JsValue, JsValue> {
    let json = parse_hcl_impl(source).map_err(|e| JsValue::from_str(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&json).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Implementation of HCL formatting.
///
/// # Arguments
/// * `source` - Unformatted HCL source string.
///
/// # Errors
/// Returns [`HclError`] on format failure.
pub fn format_hcl_impl(source: &str) -> Result<String, HclError> {
    format_str(source)
}

/// Formats an HCL source string according to canonical styling guidelines.
///
/// # Arguments
/// * `source` - The unformatted HCL source string.
///
/// # Errors
/// Returns a JavaScript error string if parsing or formatting fails.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn format_hcl(source: &str) -> Result<String, JsValue> {
    format_hcl_impl(source).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Evaluates an AST [`Body`] (attributes and nested blocks) into a JSON object map.
fn evaluate_body_to_json(
    body: &Body,
    ctx: &mut Context<'_>,
) -> Result<serde_json::Map<String, serde_json::Value>, HclError> {
    let mut evaluated_map = serde_json::Map::new();
    for (k, attr) in &body.attributes {
        let evaluator = Evaluator::new(ctx);
        let (val, _diags) = evaluator.evaluate(&attr.expr)?;
        ctx.set_variable(k.clone(), val.clone());
        let json_val = encode_value_to_json(&val)?;
        evaluated_map.insert(k.clone(), json_val);
    }
    for blk in &body.blocks {
        if blk.block_type == "locals" || blk.block_type == "variable" {
            continue;
        }
        let blk_map = evaluate_body_to_json(&blk.body, ctx)?;
        let blk_val = if let Some(first_label) = blk.labels.first() {
            let mut labeled = serde_json::Map::new();
            labeled.insert(first_label.clone(), serde_json::Value::Object(blk_map));
            serde_json::Value::Object(labeled)
        } else {
            serde_json::Value::Object(blk_map)
        };
        evaluated_map.insert(blk.block_type.clone(), blk_val);
    }
    Ok(evaluated_map)
}

/// Implementation of HCL evaluation with JSON context.
///
/// # Arguments
/// * `source` - HCL source string.
/// * `context_json` - JSON string of variables.
///
/// # Errors
/// Returns [`HclError`] on evaluation failure.
pub fn evaluate_hcl_impl(source: &str, context_json: &str) -> Result<serde_json::Value, HclError> {
    let mut parser = Parser::new(source);
    let parsed_body = parser.parse_body();
    if parser.errors().has_errors() {
        return Err(HclError::from(parser.errors().clone()));
    }

    let mut ctx = Context::with_stdlib();

    if !context_json.trim().is_empty() && context_json.trim() != "{}" {
        let vars: serde_json::Value = serde_json::from_str(context_json)?;
        if let serde_json::Value::Object(map) = vars {
            for (k, v) in map {
                let hcl_val = decode_value_from_json(&v, &Type::Dynamic)?;
                ctx.set_variable(k, hcl_val);
            }
        }
    }

    resolve_definitions(&parsed_body, &mut ctx)?;
    let body = expand_dynamic_blocks(&parsed_body, &mut ctx)?;
    let map = evaluate_body_to_json(&body, &mut ctx)?;
    Ok(serde_json::Value::Object(map))
}

/// Evaluates an HCL source string using variable bindings provided as a JSON string.
///
/// # Arguments
/// * `source` - The HCL configuration source string.
/// * `context_json` - JSON string of variables (e.g. `{"env": "prod", "count": 5}`).
///
/// # Errors
/// Returns a JavaScript error string if evaluation or parsing fails.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn evaluate_hcl(source: &str, context_json: &str) -> Result<JsValue, JsValue> {
    let json =
        evaluate_hcl_impl(source, context_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&json).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_hcl_native() -> Result<(), HclError> {
        let res = format_hcl_impl(
            "a=1
",
        )?;
        assert_eq!(
            res,
            "a = 1
"
        );

        let err = format_hcl_impl("{ invalid syntax");
        assert!(err.is_err());
        Ok(())
    }

    #[test]
    fn test_parse_and_evaluate_hcl_native() -> Result<(), HclError> {
        let input = r#"port = 8080
name = "app"
huge = 1e400
service "api" {
  enabled = true
}
config {
  timeout = 30
}
locals {
  loc = 1
}
variable "v" {
  default = 10
}
"#;
        let parsed = parse_hcl_impl(input)?;
        assert!(parsed.is_object());
        assert!(parsed["blocks"].is_array());

        let bad_parse = parse_hcl_impl("{ invalid syntax");
        assert!(bad_parse.is_err());

        let eval_res = evaluate_hcl_impl(input, "{}")?;
        assert!(eval_res.is_object());
        assert_eq!(eval_res["port"], 8080);
        assert_eq!(eval_res["name"], "app");
        assert_eq!(eval_res["huge"], "1e+400");
        assert_eq!(eval_res["service"]["api"]["enabled"], true);
        assert_eq!(eval_res["config"]["timeout"], 30);

        let eval_with_ctx = evaluate_hcl_impl(
            r#"combined = "${var_a}-${var_b}""#,
            r#"{"var_a": "alpha", "var_b": "beta"}"#,
        )?;
        assert_eq!(eval_with_ctx["combined"], "alpha-beta");

        let eval_with_non_obj_ctx = evaluate_hcl_impl("port = 8080", "[1, 2]")?;
        assert_eq!(eval_with_non_obj_ctx["port"], 8080);

        let eval_empty_ctx = evaluate_hcl_impl("port = 8080", "")?;
        assert_eq!(eval_empty_ctx["port"], 8080);

        let eval_ws_ctx = evaluate_hcl_impl("port = 8080", "   ")?;
        assert_eq!(eval_ws_ctx["port"], 8080);

        let bad_eval = evaluate_hcl_impl("invalid syntax {", "{}");
        assert!(bad_eval.is_err());

        let bad_json_ctx = evaluate_hcl_impl("a = 1", "invalid json");
        assert!(bad_json_ctx.is_err());

        let eval_runtime_err = evaluate_hcl_impl("err = 1 / 0", "{}");
        assert!(eval_runtime_err.is_err());
        Ok(())
    }
}
