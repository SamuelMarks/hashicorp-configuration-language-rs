//! Serde Serializer implementation for HCL.

use serde::ser::Serialize;
use serde_json::Value as JsonValue;

/// Serialize an instance of type `T` to a cleanly formatted HCL string.
///
/// # Errors
///
/// Returns an error if the value cannot be serialized.
pub fn to_string<T: Serialize>(value: &T) -> Result<String, String> {
    let json_val = serde_json::to_value(value).map_err(|e| format!("Serialize error: {e}"))?;

    // Convert json to formatted HCL string.
    let mut out = String::new();
    format_hcl(&json_val, &mut out, 0);
    Ok(out)
}

fn format_hcl(val: &JsonValue, out: &mut String, indent: usize) {
    let ind = "  ".repeat(indent);
    match val {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonValue::Number(n) => out.push_str(&n.to_string()),
        JsonValue::String(s) => {
            // Simplified string encoding
            out.push('"');
            out.push_str(&s.replace('"', "\\\""));
            out.push('"');
        }
        JsonValue::Array(arr) => {
            if arr.is_empty() {
                out.push_str("[]");
            } else {
                out.push_str("[\n");
                for (i, v) in arr.iter().enumerate() {
                    out.push_str(&format!("{ind}  "));
                    format_hcl(v, out, indent + 1);
                    if i < arr.len() - 1 {
                        out.push(',');
                    }
                    out.push('\n');
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
                    out.push_str(&format!("{ind}  {k} = "));
                    format_hcl(v, out, indent + 1);
                    out.push('\n');
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
    fn test_ser_all_json_primitives() {
        let val = serde_json::json!({
            "null_field": null,
            "bool_t": true,
            "bool_f": false,
            "num": 42,
            "str": "quotes \"and\" backslashes \\",
            "empty_list": [],
            "items": [1, 2],
            "empty_map": {},
            "nested": {
                "key": "val"
            }
        });
        let res = to_string(&val).expect("to_string succeeds");
        assert!(res.contains("null_field = null"));
        assert!(res.contains("bool_t = true"));
        assert!(res.contains("bool_f = false"));
        assert!(res.contains("num = 42"));
        assert!(res.contains("empty_list = []"));
        assert!(res.contains("empty_map = {}"));
        assert!(res.contains("key = \"val\""));

        assert!(to_string(&CustomFail).is_err());
        assert!(to_string(&ToggleFail(false)).is_ok());
        assert!(to_string(&ToggleFail(true)).is_err());
    }
}
