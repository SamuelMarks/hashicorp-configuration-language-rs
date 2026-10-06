//! Serde Deserializer implementation for HCL.
use crate::ast::structure::Body;
use crate::parse::parser::Parser;
use serde::de::DeserializeOwned;
use serde_json::{Map as JsonMap, Value as JsonValue};
/// Deserialize an instance of type `T` from a string of HCL text.
///
/// # Errors
///
/// Returns an error if the HCL cannot be parsed or deserialized.
pub fn from_str<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    let mut parser = Parser::new(s);
    let body = parser.parse_body();
    if parser.errors().has_errors() {
        return Err(format!("Parse error: {:?}", parser.errors().errors()));
    }
    let json_val = body_to_json(&body);
    serde_json::from_value(json_val).map_err(|e| format!("Deserialize error: {e}"))
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
                .or_insert(JsonValue::Array(Vec::new()));
            if let JsonValue::Array(arr) = current {
                arr.push(block_json);
            }
        } else {
            let current = map
                .entry(block.block_type.clone())
                .or_insert(JsonValue::Object(JsonMap::new()));
            if let JsonValue::Object(o) = current {
                o.insert(block.labels[0].clone(), block_json.clone());
            }
        }
    }
    JsonValue::Object(map)
}
pub(crate) fn expr_to_json(expr: &crate::ast::expr::Expression) -> JsonValue {
    use crate::ast::expr::Expression;
    use crate::ast::expr::TemplatePart;
    match expr {
        Expression::String(s, _) => JsonValue::String(s.clone()),
        Expression::Number(n, _) => {
            let f = n.as_f64().unwrap_or(0.0);
            if f.fract() == 0.0 {
                serde_json::Number::from(f as i64).into()
            } else {
                serde_json::Number::from_f64(f).map_or(JsonValue::Null, JsonValue::Number)
            }
        }
        Expression::Bool(b, _) => JsonValue::Bool(*b),
        Expression::Tuple(exprs, _) => {
            let mut arr = Vec::new();
            for e in exprs {
                arr.push(expr_to_json(e));
            }
            JsonValue::Array(arr)
        }
        Expression::Object(pairs, _) => {
            let mut obj = JsonMap::new();
            for (k, v) in pairs {
                let key_str = match k {
                    Expression::String(s, _) => s.clone(),
                    Expression::Variable(v_name, _) => v_name.clone(),
                    _ => "unknown_key".to_string(),
                };
                obj.insert(key_str, expr_to_json(v));
            }
            JsonValue::Object(obj)
        }
        Expression::Template(parts, _) => {
            let mut s = String::new();
            for p in parts {
                if let TemplatePart::Literal(lit, _) = p {
                    s.push_str(lit);
                } else {
                    s.push_str("${...}");
                }
            }
            JsonValue::String(s)
        }
        _ => JsonValue::Null,
    }
}
