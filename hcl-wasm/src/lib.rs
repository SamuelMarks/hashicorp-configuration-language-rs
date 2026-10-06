//! WebAssembly bindings for HCL (`hcl-wasm`).
//!
//! Provides JavaScript- and WebAssembly-compatible functions for parsing,
//! formatting, and evaluating HCL configurations.

#![deny(clippy::all, clippy::pedantic)]
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
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::cst::format::format_str;
use hashicorp_configuration_language_rs::eval::context::Context;
use hashicorp_configuration_language_rs::eval::evaluator::Evaluator;
use hashicorp_configuration_language_rs::parse::parser::Parser;
use hashicorp_configuration_language_rs::types::json::{
    decode_value_from_json, encode_value_to_json,
};
use hashicorp_configuration_language_rs::types::{Type, Value};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Implementation of HCL parsing returning a generic JSON object.
///
/// # Arguments
/// * `source` - HCL source code string.
///
/// # Errors
/// Returns error string on parse failure.
pub fn parse_hcl_impl(source: &str) -> Result<serde_json::Value, String> {
    let mut parser = Parser::new(source);
    let body = parser.parse_body();
    if parser.errors().has_errors() {
        return Err(parser.errors().to_string());
    }

    let mut attrs_map = serde_json::Map::new();
    for (k, attr) in &body.attributes {
        attrs_map.insert(
            k.clone(),
            serde_json::Value::String(format!("{:?}", attr.expr)),
        );
    }

    Ok(serde_json::Value::Object(attrs_map))
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
    let json = parse_hcl_impl(source).map_err(|e| JsValue::from_str(&e))?;
    serde_wasm_bindgen::to_value(&json).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Implementation of HCL formatting.
///
/// # Arguments
/// * `source` - Unformatted HCL source string.
///
/// # Errors
/// Returns error string on format failure.
pub fn format_hcl_impl(source: &str) -> Result<String, String> {
    format_str(source).map_err(|e| e.to_string())
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
    format_hcl_impl(source).map_err(|e| JsValue::from_str(&e))
}

/// Implementation of HCL evaluation with JSON context.
///
/// # Arguments
/// * `source` - HCL source string.
/// * `context_json` - JSON string of variables.
///
/// # Errors
/// Returns error string on evaluation failure.
pub fn evaluate_hcl_impl(source: &str, context_json: &str) -> Result<serde_json::Value, String> {
    let mut parser = Parser::new(source);
    let body = parser.parse_body();
    if parser.errors().has_errors() {
        return Err(parser.errors().to_string());
    }

    let mut ctx = Context::with_stdlib();

    if !context_json.trim().is_empty() && context_json.trim() != "{}" {
        let vars: serde_json::Value =
            serde_json::from_str(context_json).map_err(|e| format!("Invalid context JSON: {e}"))?;
        if let serde_json::Value::Object(map) = vars {
            for (k, v) in map {
                let hcl_val = decode_value_from_json(&v, &Type::Dynamic)
                    .unwrap_or(Value::null(Type::Dynamic));
                ctx.set_variable(k, hcl_val);
            }
        }
    }

    let mut evaluated_map = serde_json::Map::new();
    for (k, attr) in &body.attributes {
        let evaluator = Evaluator::new(&ctx);
        let (val, _diags) = evaluator.evaluate(&attr.expr).map_err(|e| e.to_string())?;
        ctx.set_variable(k.clone(), val.clone());
        let json_val = encode_value_to_json(&val).unwrap_or(serde_json::Value::Null);
        evaluated_map.insert(k.clone(), json_val);
    }

    Ok(serde_json::Value::Object(evaluated_map))
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
    let json = evaluate_hcl_impl(source, context_json).map_err(|e| JsValue::from_str(&e))?;
    serde_wasm_bindgen::to_value(&json).map_err(|e| JsValue::from_str(&e.to_string()))
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
    fn test_format_hcl_native() {
        let res = format_hcl_impl("a=1\n").unwrap();
        assert_eq!(res, "a = 1\n");

        let err = format_hcl_impl("{ invalid syntax");
        assert!(err.is_err());
    }

    #[test]
    fn test_parse_and_evaluate_hcl_native() {
        let input = "port = 8080\nname = \"app\"\nhuge = 1e400\n";
        let parsed = parse_hcl_impl(input).unwrap();
        assert!(parsed.is_object());

        let bad_parse = parse_hcl_impl("{ invalid syntax");
        assert!(bad_parse.is_err());

        let eval_res = evaluate_hcl_impl(input, "{}").unwrap();
        assert!(eval_res.is_object());
        assert_eq!(eval_res["port"], 8080);
        assert_eq!(eval_res["name"], "app");
        assert_eq!(eval_res["huge"], "1e+400");

        let eval_with_ctx = evaluate_hcl_impl(
            "combined = \"${var_a}-${var_b}\"",
            r#"{"var_a": "alpha", "var_b": "beta"}"#,
        )
        .unwrap();
        assert_eq!(eval_with_ctx["combined"], "alpha-beta");

        let eval_with_non_obj_ctx = evaluate_hcl_impl("port = 8080", "[1, 2]").unwrap();
        assert_eq!(eval_with_non_obj_ctx["port"], 8080);

        let eval_empty_ctx = evaluate_hcl_impl("port = 8080", "").unwrap();
        assert_eq!(eval_empty_ctx["port"], 8080);

        let eval_ws_ctx = evaluate_hcl_impl("port = 8080", "   ").unwrap();
        assert_eq!(eval_ws_ctx["port"], 8080);

        let bad_eval = evaluate_hcl_impl("invalid syntax {", "{}");
        assert!(bad_eval.is_err());

        let bad_json_ctx = evaluate_hcl_impl("a = 1", "invalid json");
        assert!(bad_json_ctx.is_err());

        let eval_runtime_err = evaluate_hcl_impl("err = 1 / 0", "{}");
        assert!(eval_runtime_err.is_err());
    }
}
