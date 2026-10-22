//! Serde Serializer implementation for HCL.
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::error::HclError;
use serde::ser::Serialize;
use serde_json::Value as JsonValue;
use std::fmt::Write;

/// Escapes an arbitrary Rust string into a valid HCL string literal payload.
fn escape_hcl_string(s: &str, out: &mut String) {
    let backslash = char::from(92);
    for ch in s.chars() {
        if ch == '"' {
            out.push(backslash);
            out.push('"');
        } else if ch == backslash {
            out.push(backslash);
            out.push(backslash);
        } else if ch == '\n' {
            out.push(backslash);
            out.push('n');
        } else if ch == '\r' {
            out.push(backslash);
            out.push('r');
        } else if ch == '\t' {
            out.push(backslash);
            out.push('t');
        } else {
            out.push(ch);
        }
    }
}

/// Serialize an instance of type `T` to a cleanly formatted HCL string.
///
/// # Arguments
/// * `value` - The serializable Rust value to convert into HCL text.
///
/// # Errors
/// Returns [`HclError::SerdeSerialize`] if the value cannot be serialized.
pub fn to_string<T: Serialize>(value: &T) -> Result<String, HclError> {
    let json_val =
        serde_json::to_value(value).map_err(|e| HclError::SerdeSerialize(e.to_string()))?;
    let mut out = String::new();
    format_hcl(&json_val, &mut out, 0);
    Ok(out)
}

fn format_hcl(val: &JsonValue, out: &mut String, indent: usize) {
    let ind = "  ".repeat(indent);
    let newline = char::from(10);
    match val {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonValue::Number(n) => out.push_str(&n.to_string()),
        JsonValue::String(s) => {
            out.push('"');
            escape_hcl_string(s, out);
            out.push('"');
        }
        JsonValue::Array(arr) => {
            if arr.is_empty() {
                out.push_str("[]");
            } else {
                out.push_str("[\n");
                for (i, v) in arr.iter().enumerate() {
                    let _ = write!(out, "{ind}  ");
                    format_hcl(v, out, indent + 1);
                    if i < arr.len() - 1 {
                        out.push(',');
                    }
                    out.push(newline);
                }
                out.push_str(&ind);
                out.push(']');
            }
        }
        JsonValue::Object(obj) => {
            if obj.is_empty() {
                out.push_str("{}");
            } else {
                out.push_str("{\n");
                for (k, v) in obj {
                    let _ = write!(out, "{ind}  {k} = ");
                    format_hcl(v, out, indent + 1);
                    out.push(newline);
                }
                out.push_str(&ind);
                out.push('}');
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    struct CustomFail;
    impl serde::Serialize for CustomFail {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("forced failure"))
        }
    }

    struct ToggleFail(bool);
    impl serde::Serialize for ToggleFail {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            if self.0 {
                Err(serde::ser::Error::custom("forced toggle error"))
            } else {
                "success".serialize(serializer)
            }
        }
    }

    #[test]
    fn test_ser_all_json_primitives() -> Result<(), HclError> {
        let backslash = char::from(92);
        let input_str = format!("quotes \"and\" backslashes {backslash}\n\r\t");
        let val = serde_json::json!(
            {
                "null_field": null,
                "bool_t": true,
                "bool_f": false,
                "num": 42,
                "str": input_str,
                "empty_list": [],
                "items": [1, 2],
                "empty_map": {},
                "nested": { "key": "val" }
            }
        );
        let res = to_string(&val)?;
        assert!(res.contains("null_field = null"));
        assert!(res.contains("bool_t = true"));
        assert!(res.contains("bool_f = false"));
        assert!(res.contains("num = 42"));
        let expected_escaped = format!(
            "quotes {backslash}\"and{backslash}\" backslashes {backslash}{backslash}{backslash}n{backslash}r{backslash}t"
        );
        assert!(res.contains(&expected_escaped));
        assert!(res.contains("empty_list = []"));
        assert!(res.contains("empty_map = {}"));
        assert!(res.contains(r#"key = "val""#));
        assert!(to_string(&CustomFail).is_err());
        assert!(to_string(&ToggleFail(false)).is_ok());
        assert!(to_string(&ToggleFail(true)).is_err());
        Ok(())
    }
}
