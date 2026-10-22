//! Serde Deserializer implementation for HCL.
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::ast::structure::Body;
use crate::error::HclError;
use crate::eval::context::Context;
use crate::eval::evaluator::Evaluator;
use crate::parse::parser::Parser;
use bigdecimal::ToPrimitive;
use serde::de::DeserializeOwned;
use serde_json::{Map as JsonMap, Value as JsonValue};

/// Deserialize an instance of type `T` from a string of HCL text.
///
/// # Arguments
/// * `s` - The HCL source string to parse and deserialize.
///
/// # Errors
/// Returns [`HclError::Parse`] if the HCL cannot be parsed or [`HclError::SerdeDeserialize`]
/// if deserialization into `T` fails.
pub fn from_str<T: DeserializeOwned>(s: &str) -> Result<T, HclError> {
    let mut parser = Parser::new(s);
    let body = parser.parse_body();
    if parser.errors().has_errors() {
        return Err(HclError::Parse(format!("{:?}", parser.errors().errors())));
    }
    let json_val = body_to_json(&body);
    serde_json::from_value(json_val).map_err(|e| HclError::SerdeDeserialize(e.to_string()))
}

fn insert_labeled_block(map: &mut JsonMap<String, JsonValue>, labels: &[String], val: JsonValue) {
    let first = &labels[0];
    let rest = &labels[1..];
    if rest.is_empty() {
        map.insert(first.clone(), val);
    } else {
        let mut nested_map = match map.remove(first) {
            Some(JsonValue::Object(existing)) => existing,
            _ => JsonMap::new(),
        };
        insert_labeled_block(&mut nested_map, rest, val);
        map.insert(first.clone(), JsonValue::Object(nested_map));
    }
}

fn body_to_json(body: &Body) -> JsonValue {
    let mut map = JsonMap::new();
    for (name, attr) in &body.attributes {
        map.insert(name.clone(), expr_to_json(&attr.expr));
    }
    for block in &body.blocks {
        let block_json = body_to_json(&block.body);
        if block.labels.is_empty() {
            let current = map
                .entry(block.block_type.clone())
                .or_insert_with(|| JsonValue::Array(Vec::new()));
            if let JsonValue::Array(arr) = current {
                arr.push(block_json);
            }
        } else {
            let current = map
                .entry(block.block_type.clone())
                .or_insert_with(|| JsonValue::Object(JsonMap::new()));
            if let JsonValue::Object(o) = current {
                insert_labeled_block(o, &block.labels, block_json);
            }
        }
    }
    JsonValue::Object(map)
}

pub(crate) fn expr_to_json(expr: &crate::ast::expr::Expression) -> JsonValue {
    match expr {
        crate::ast::expr::Expression::String(s, _) => JsonValue::String(s.clone()),
        crate::ast::expr::Expression::Number(n, _) => {
            if n.0.is_integer() {
                if let Some(i) = n.0.to_i64() {
                    return JsonValue::Number(serde_json::Number::from(i));
                }
                if let Some(u) = n.0.to_u64() {
                    return JsonValue::Number(serde_json::Number::from(u));
                }
            }
            n.as_f64()
                .and_then(serde_json::Number::from_f64)
                .map_or(JsonValue::Null, JsonValue::Number)
        }
        crate::ast::expr::Expression::Bool(b, _) => JsonValue::Bool(*b),
        crate::ast::expr::Expression::Tuple(exprs, _) => {
            let mut arr = Vec::with_capacity(exprs.len());
            for e in exprs {
                arr.push(expr_to_json(e));
            }
            JsonValue::Array(arr)
        }
        crate::ast::expr::Expression::Object(pairs, _) => {
            let mut obj = JsonMap::new();
            for (k, v) in pairs {
                let key_str = match k {
                    crate::ast::expr::Expression::String(s, _) => s.clone(),
                    crate::ast::expr::Expression::Variable(v_name, _) => v_name.clone(),
                    _ => "unknown_key".to_string(),
                };
                obj.insert(key_str, expr_to_json(v));
            }
            JsonValue::Object(obj)
        }
        crate::ast::expr::Expression::Template(parts, _) => {
            let ctx = Context::with_stdlib();
            let evaluator = Evaluator::new(&ctx);
            if let Ok((val, _)) = evaluator.evaluate(expr) {
                return crate::types::json::encode_value_to_json(&val).unwrap_or(JsonValue::Null);
            }
            let mut s = String::new();
            for p in parts {
                if let crate::ast::expr::TemplatePart::Literal(lit, _) = p {
                    s.push_str(lit);
                } else {
                    s.push_str("${...}");
                }
            }
            JsonValue::String(s)
        }
        crate::ast::expr::Expression::Null(_) => JsonValue::Null,
        other => {
            let ctx = Context::with_stdlib();
            let evaluator = Evaluator::new(&ctx);
            evaluator
                .evaluate(other)
                .ok()
                .and_then(|(val, _)| crate::types::json::encode_value_to_json(&val).ok())
                .unwrap_or(JsonValue::Null)
        }
    }
}
