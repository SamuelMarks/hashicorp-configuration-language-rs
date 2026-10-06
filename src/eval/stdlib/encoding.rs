//! Encoding standard library functions.
use crate::eval::func::Function;
use crate::number::Number;
use crate::types::{Type, Value, ValueData};
use base64::prelude::*;
use bigdecimal::BigDecimal;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::str::FromStr;
use std::sync::Arc;
#[must_use]
/// Return all encoding standard library functions.
pub fn functions() -> Vec<Function> {
    vec![
        base64decode_func(),
        base64encode_func(),
        base64gunzip_func(),
        base64gzip_func(),
        csvdecode_func(),
        jsondecode_func(),
        jsonencode_func(),
        textdecodebase64_func(),
        textencodebase64_func(),
        urlbase64decode_func(),
        urlbase64encode_func(),
        yamldecode_func(),
        yamlencode_func(),
    ]
}
fn coerce_to_string(arg: &Value, name: &str) -> Result<String, String> {
    let coerced = arg
        .clone()
        .coerce(&Type::String)
        .map_err(|_| format!("{} requires a string, got type {}", name, arg.ty()))?;
    if let ValueData::String(s) = *coerced.data {
        Ok(s)
    } else {
        Err(format!("{name} requires a string"))
    }
}
fn base64decode_func() -> Function {
    Function {
        name: "base64decode".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("base64decode expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let s = coerce_to_string(&args[0], "base64decode")?;
            let bytes = BASE64_STANDARD
                .decode(&s)
                .map_err(|e| format!("invalid base64: {e}"))?;
            let res = String::from_utf8(bytes)
                .map_err(|e| format!("base64decode result is not valid UTF-8: {e}"))?;
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}
fn base64encode_func() -> Function {
    Function {
        name: "base64encode".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("base64encode expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let s = coerce_to_string(&args[0], "base64encode")?;
            let res = BASE64_STANDARD.encode(s.as_bytes());
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}
fn textdecodebase64_func() -> Function {
    Function {
        name: "textdecodebase64".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("textdecodebase64 expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let s = coerce_to_string(&args[0], "textdecodebase64 data")?;
            let encoding = coerce_to_string(&args[1], "textdecodebase64 encoding")?;
            let enc = encoding_rs::Encoding::for_label(encoding.as_bytes())
                .ok_or_else(|| format!("unsupported encoding: {encoding}"))?;
            let bytes = BASE64_STANDARD
                .decode(&s)
                .map_err(|e| format!("invalid base64: {e}"))?;
            let (res, had_errors) = if enc == encoding_rs::UTF_16LE || enc == encoding_rs::UTF_16BE
            {
                enc.decode_without_bom_handling(&bytes)
            } else {
                let (cow, _, had_err) = enc.decode(&bytes);
                (cow, had_err)
            };
            if had_errors {
                return Err(format!(
                    "textdecodebase64 result is not valid for encoding {encoding}"
                ));
            }
            Ok(Value::new(
                Type::String,
                ValueData::String(res.into_owned()),
            ))
        }),
        signature: None,
    }
}
fn textencodebase64_func() -> Function {
    Function {
        name: "textencodebase64".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("textencodebase64 expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let s = coerce_to_string(&args[0], "textencodebase64 data")?;
            let encoding = coerce_to_string(&args[1], "textencodebase64 encoding")?;
            let enc = encoding_rs::Encoding::for_label(encoding.as_bytes())
                .ok_or_else(|| format!("unsupported encoding: {encoding}"))?;
            let encoded_bytes = if enc == encoding_rs::UTF_16LE {
                let mut b = Vec::with_capacity(s.len() * 2);
                for u in s.encode_utf16() {
                    b.extend_from_slice(&u.to_le_bytes());
                }
                b
            } else if enc == encoding_rs::UTF_16BE {
                let mut b = Vec::with_capacity(s.len() * 2);
                for u in s.encode_utf16() {
                    b.extend_from_slice(&u.to_be_bytes());
                }
                b
            } else {
                let (bytes, _, had_errors) = enc.encode(&s);
                if had_errors {
                    return Err(format!(
                        "character in input string cannot be represented in encoding {encoding}"
                    ));
                }
                bytes.into_owned()
            };
            let res = BASE64_STANDARD.encode(&encoded_bytes);
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}
fn base64gzip_func() -> Function {
    Function {
        name: "base64gzip".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("base64gzip expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let s = coerce_to_string(&args[0], "base64gzip")?;
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            let _ = encoder.write_all(s.as_bytes());
            let compressed = encoder.finish().unwrap_or_default();
            let res = BASE64_STANDARD.encode(compressed);
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}
/// Built-in `base64gunzip` function decoding base64 data and decompressing gzip byte stream.
fn base64gunzip_func() -> Function {
    Function {
        name: "base64gunzip".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("base64gunzip expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String).with_marks(args[0].marks().clone()));
            }
            let s = coerce_to_string(&args[0], "base64gunzip")?;
            let bytes = BASE64_STANDARD
                .decode(&s)
                .or_else(|_| BASE64_STANDARD_NO_PAD.decode(&s))
                .or_else(|_| BASE64_URL_SAFE.decode(&s))
                .or_else(|_| BASE64_URL_SAFE_NO_PAD.decode(&s))
                .map_err(|e| format!("invalid base64: {e}"))?;
            let mut decoder = GzDecoder::new(&bytes[..]);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| format!("corrupted gzip: {e}"))?;
            let res = String::from_utf8(decompressed)
                .map_err(|e| format!("base64gunzip result is not valid UTF-8: {e}"))?;
            Ok(Value::new_with_marks(
                Type::String,
                ValueData::String(res),
                args[0].marks().clone(),
            ))
        }),
        signature: None,
    }
}
fn urlbase64decode_func() -> Function {
    Function {
        name: "urlbase64decode".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("urlbase64decode expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let s = coerce_to_string(&args[0], "urlbase64decode")?;
            let bytes = BASE64_URL_SAFE_NO_PAD
                .decode(&s)
                .map_err(|_| "invalid urlbase64".to_string())?;
            let res = String::from_utf8(bytes)
                .map_err(|_| "urlbase64decode result is not valid UTF-8".to_string())?;
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}
fn urlbase64encode_func() -> Function {
    Function {
        name: "urlbase64encode".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("urlbase64encode expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let s = coerce_to_string(&args[0], "urlbase64encode")?;
            let res = BASE64_URL_SAFE_NO_PAD.encode(s.as_bytes());
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}
fn val_to_json(v: &Value) -> serde_json::Value {
    match &*v.data {
        ValueData::Null | ValueData::Unknown(_) | ValueData::Capsule(_) => serde_json::Value::Null,
        ValueData::Bool(b) => serde_json::Value::Bool(*b),
        ValueData::String(s) => serde_json::Value::String(s.clone()),
        ValueData::Number(n) => {
            let f = bigdecimal::num_traits::ToPrimitive::to_f64(&n.0).unwrap_or(0.0);
            serde_json::Number::from_f64(f)
                .map_or(serde_json::Value::Null, serde_json::Value::Number)
        }
        ValueData::Array(arr) => serde_json::Value::Array(arr.iter().map(val_to_json).collect()),
        ValueData::Set(set) => serde_json::Value::Array(set.iter().map(val_to_json).collect()),
        ValueData::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj {
                map.insert(k.clone(), val_to_json(v));
            }
            serde_json::Value::Object(map)
        }
    }
}
fn jsonencode_func() -> Function {
    Function {
        name: "jsonencode".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("jsonencode expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let json = val_to_json(&args[0]);
            let res = json.to_string();
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}
fn json_to_val(j: &serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::null(Type::Dynamic),
        serde_json::Value::Bool(b) => Value::new(Type::Bool, ValueData::Bool(*b)),
        serde_json::Value::String(s) => Value::new(Type::String, ValueData::String(s.clone())),
        serde_json::Value::Number(n) => {
            let bd = BigDecimal::from_str(&n.to_string()).unwrap_or_default();
            Value::new(Type::Number, ValueData::Number(Number::new(bd)))
        }
        serde_json::Value::Array(arr) => {
            let mut vals = Vec::new();
            let mut ty = None;
            for item in arr {
                let v = json_to_val(item);
                if ty.is_none() {
                    ty = Some(v.ty().clone());
                }
                vals.push(v);
            }
            Value::new(
                Type::List(Box::new(ty.unwrap_or(Type::Dynamic))),
                ValueData::Array(vals),
            )
        }
        serde_json::Value::Object(obj) => {
            let mut map = BTreeMap::new();
            let mut ty = None;
            for (k, v) in obj {
                let val = json_to_val(v);
                if ty.is_none() {
                    ty = Some(val.ty().clone());
                }
                map.insert(k.clone(), val);
            }
            Value::new(
                Type::Map(Box::new(ty.unwrap_or(Type::Dynamic))),
                ValueData::Object(map),
            )
        }
    }
}
fn jsondecode_func() -> Function {
    Function {
        name: "jsondecode".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("jsondecode expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Dynamic));
            }
            let s = coerce_to_string(&args[0], "jsondecode")?;
            let json: serde_json::Value =
                serde_json::from_str(&s).map_err(|e| format!("jsondecode error: {e}"))?;
            Ok(json_to_val(&json))
        }),
        signature: None,
    }
}
fn val_to_yaml(v: &Value) -> serde_yaml::Value {
    match &*v.data {
        ValueData::Null | ValueData::Unknown(_) | ValueData::Capsule(_) => serde_yaml::Value::Null,
        ValueData::Bool(b) => serde_yaml::Value::Bool(*b),
        ValueData::String(s) => serde_yaml::Value::String(s.clone()),
        ValueData::Number(n) => {
            let f = bigdecimal::num_traits::ToPrimitive::to_f64(&n.0).unwrap_or(0.0);
            serde_yaml::Value::Number(serde_yaml::Number::from(f))
        }
        ValueData::Array(arr) => serde_yaml::Value::Sequence(arr.iter().map(val_to_yaml).collect()),
        ValueData::Set(set) => serde_yaml::Value::Sequence(set.iter().map(val_to_yaml).collect()),
        ValueData::Object(obj) => {
            let mut map = serde_yaml::Mapping::new();
            for (k, v) in obj {
                map.insert(serde_yaml::Value::String(k.clone()), val_to_yaml(v));
            }
            serde_yaml::Value::Mapping(map)
        }
    }
}
fn yamlencode_func() -> Function {
    Function {
        name: "yamlencode".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("yamlencode expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let yaml = val_to_yaml(&args[0]);
            let res = serde_yaml::to_string(&yaml).unwrap_or_default();
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}
fn yaml_to_val(j: &serde_yaml::Value) -> Value {
    match j {
        serde_yaml::Value::Null => Value::null(Type::Dynamic),
        serde_yaml::Value::Bool(b) => Value::new(Type::Bool, ValueData::Bool(*b)),
        serde_yaml::Value::String(s) => Value::new(Type::String, ValueData::String(s.clone())),
        serde_yaml::Value::Number(n) => {
            let bd = BigDecimal::from_str(&n.to_string()).unwrap_or_default();
            Value::new(Type::Number, ValueData::Number(Number::new(bd)))
        }
        serde_yaml::Value::Sequence(arr) => {
            let mut vals = Vec::new();
            let mut ty = None;
            for item in arr {
                let v = yaml_to_val(item);
                if ty.is_none() {
                    ty = Some(v.ty().clone());
                }
                vals.push(v);
            }
            Value::new(
                Type::List(Box::new(ty.unwrap_or(Type::Dynamic))),
                ValueData::Array(vals),
            )
        }
        serde_yaml::Value::Mapping(obj) => {
            let mut map = BTreeMap::new();
            let mut ty = None;
            for (k, v) in obj {
                if let serde_yaml::Value::String(k_str) = k {
                    let val = yaml_to_val(v);
                    if ty.is_none() {
                        ty = Some(val.ty().clone());
                    }
                    map.insert(k_str.clone(), val);
                }
            }
            Value::new(
                Type::Map(Box::new(ty.unwrap_or(Type::Dynamic))),
                ValueData::Object(map),
            )
        }
        serde_yaml::Value::Tagged(t) => yaml_to_val(&t.value),
    }
}
fn yamldecode_func() -> Function {
    Function {
        name: "yamldecode".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("yamldecode expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Dynamic));
            }
            let s = coerce_to_string(&args[0], "yamldecode")?;
            let yaml: serde_yaml::Value =
                serde_yaml::from_str(&s).map_err(|e| format!("yamldecode error: {e}"))?;
            Ok(yaml_to_val(&yaml))
        }),
        signature: None,
    }
}
fn csvdecode_func() -> Function {
    Function {
        name: "csvdecode".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("csvdecode expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::List(Box::new(Type::Map(Box::new(
                    Type::String,
                ))))));
            }
            let s = coerce_to_string(&args[0], "csvdecode")?;
            let mut rdr = csv::Reader::from_reader(s.as_bytes());
            let headers = rdr.headers().cloned().unwrap_or_default();
            let mut result = Vec::new();
            for result_row in rdr.records() {
                let record =
                    result_row.map_err(|e| format!("csvdecode error reading record: {e}"))?;
                let mut map = BTreeMap::new();
                for (header, field) in headers.iter().zip(record.iter()) {
                    map.insert(
                        header.to_string(),
                        Value::new(Type::String, ValueData::String(field.to_string())),
                    );
                }
                result.push(Value::new(
                    Type::Map(Box::new(Type::String)),
                    ValueData::Object(map),
                ));
            }
            Ok(Value::new(
                Type::List(Box::new(Type::Map(Box::new(Type::String)))),
                ValueData::Array(result),
            ))
        }),
        signature: None,
    }
}
#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::pedantic,
        clippy::nursery
    )]
    use super::*;
    use crate::eval::func::Function;
    use std::str::FromStr;
    fn get_func(name: &str) -> Function {
        functions().into_iter().find(|f| f.name == name).unwrap()
    }
    fn eval_func(name: &str, args: &[Value]) -> Result<Value, String> {
        let f = get_func(name);
        (f.func)(args)
    }
    fn str_val(s: &str) -> Value {
        Value::new(Type::String, ValueData::String(s.to_string()))
    }
    fn unk_val() -> Value {
        Value::unknown(Type::String)
    }
    fn null_val() -> Value {
        Value::new(Type::Dynamic, ValueData::Null)
    }
    fn num_val(s: &str) -> Value {
        Value::new(
            Type::Number,
            ValueData::Number(Number::new(BigDecimal::from_str(s).unwrap())),
        )
    }
    #[test]
    fn test_coerce_to_string() {
        assert!(coerce_to_string(&num_val("123"), "test").is_ok());
        let unk = Value::unknown(Type::object(BTreeMap::new()));
        assert!(coerce_to_string(&unk, "test").is_err());
        let non_str = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!(coerce_to_string(&non_str, "test").is_err());
        let null_str = Value::new(Type::String, ValueData::Null);
        assert!(coerce_to_string(&null_str, "test").is_err());
    }
    #[test]
    fn test_base64decode() {
        assert_eq!(
            eval_func("base64decode", &[str_val("aGVsbG8=")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::String("hello".to_string())
        );
        assert!(eval_func("base64decode", &[]).is_err());
        assert!(
            eval_func("base64decode", &[unk_val()])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("base64decode", &[str_val("invalid!")]).is_err());
        assert!(eval_func("base64decode", &[str_val("/w==")]).is_err());
    }
    #[test]
    fn test_encoding_unknown_fallbacks() {
        use crate::types::{Type, Value, ValueData};
        let b64dec = base64decode_func();
        let b64enc = base64encode_func();
        let txtdec = textdecodebase64_func();
        let txtenc = textencodebase64_func();
        let gunz = base64gunzip_func();
        let gz = base64gzip_func();
        let urlbase64dec = urlbase64decode_func();
        let urlbase64enc = urlbase64encode_func();
        let jenc = jsonencode_func();
        let jdec = jsondecode_func();
        let yenc = yamlencode_func();
        let ydec = yamldecode_func();
        let csv = csvdecode_func();
        let unk = Value::unknown(Type::String);
        let s = Value::new(Type::String, ValueData::String("string".to_string()));
        assert!(
            (b64dec.func)(std::slice::from_ref(&unk))
                .unwrap()
                .is_unknown()
        );
        assert!(
            (b64enc.func)(std::slice::from_ref(&unk))
                .unwrap()
                .is_unknown()
        );
        assert!(
            (txtdec.func)(&[unk.clone(), s.clone()])
                .unwrap()
                .is_unknown()
        );
        assert!(
            (txtdec.func)(&[s.clone(), unk.clone()])
                .unwrap()
                .is_unknown()
        );
        assert!(
            (txtenc.func)(&[unk.clone(), s.clone()])
                .unwrap()
                .is_unknown()
        );
        assert!(
            (txtenc.func)(&[s.clone(), unk.clone()])
                .unwrap()
                .is_unknown()
        );
        assert!(
            (gunz.func)(std::slice::from_ref(&unk))
                .unwrap()
                .is_unknown()
        );
        assert!((gz.func)(std::slice::from_ref(&unk)).unwrap().is_unknown());
        assert!(
            (urlbase64dec.func)(std::slice::from_ref(&unk))
                .unwrap()
                .is_unknown()
        );
        assert!(
            (urlbase64enc.func)(std::slice::from_ref(&unk))
                .unwrap()
                .is_unknown()
        );
        assert!(
            (jenc.func)(std::slice::from_ref(&unk))
                .unwrap()
                .is_unknown()
        );
        assert!(
            (jdec.func)(std::slice::from_ref(&unk))
                .unwrap()
                .is_unknown()
        );
        assert!(
            (yenc.func)(std::slice::from_ref(&unk))
                .unwrap()
                .is_unknown()
        );
        assert!(
            (ydec.func)(std::slice::from_ref(&unk))
                .unwrap()
                .is_unknown()
        );
        assert!((csv.func)(std::slice::from_ref(&unk)).unwrap().is_unknown());
    }
    #[test]
    fn test_base64encode() {
        assert_eq!(
            eval_func("base64encode", &[str_val("hello")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::String("aGVsbG8=".to_string())
        );
        assert!(eval_func("base64encode", &[]).is_err());
        assert!(
            eval_func("base64encode", &[unk_val()])
                .unwrap()
                .is_unknown()
        );
    }
    #[test]
    fn test_textdecodebase64() {
        let non_str = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert_eq!(
            eval_func("textdecodebase64", &[str_val("aGVsbG8="), str_val("UTF-8")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::String("hello".to_string())
        );
        let utf16le_enc =
            eval_func("textencodebase64", &[str_val("hello"), str_val("UTF-16LE")]).unwrap();
        let utf16le_dec =
            eval_func("textdecodebase64", &[utf16le_enc, str_val("UTF-16LE")]).unwrap();
        assert_eq!(
            utf16le_dec.data.as_ref(),
            &ValueData::String("hello".to_string())
        );
        let be_encoded =
            eval_func("textencodebase64", &[str_val("hello"), str_val("UTF-16BE")]).unwrap();
        let be_decoded = eval_func("textdecodebase64", &[be_encoded, str_val("UTF-16BE")]).unwrap();
        assert_eq!(
            be_decoded.data.as_ref(),
            &ValueData::String("hello".to_string())
        );
        let w1252_enc = eval_func(
            "textencodebase64",
            &[str_val("Café"), str_val("windows-1252")],
        )
        .unwrap();
        let w1252_dec =
            eval_func("textdecodebase64", &[w1252_enc, str_val("windows-1252")]).unwrap();
        assert_eq!(
            w1252_dec.data.as_ref(),
            &ValueData::String("Café".to_string())
        );
        assert!(eval_func("textdecodebase64", &[]).is_err());
        assert!(
            eval_func("textdecodebase64", &[unk_val(), str_val("utf-8")])
                .unwrap()
                .is_unknown()
        );
        assert!(
            eval_func("textdecodebase64", &[str_val("aGVsbG8="), unk_val()])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("textdecodebase64", &[str_val("aGVsbG8="), non_str.clone()]).is_err());
        assert!(eval_func("textdecodebase64", &[non_str.clone(), str_val("utf-8")]).is_err());
        assert!(
            eval_func(
                "textdecodebase64",
                &[str_val("aGVsbG8="), str_val("unsupported-charset-xyz")]
            )
            .is_err()
        );
        assert!(eval_func("textdecodebase64", &[str_val("invalid!"), str_val("utf-8")]).is_err());
        assert!(eval_func("textdecodebase64", &[str_val("AQ=="), str_val("UTF-16LE")]).is_err());
        assert!(eval_func("textdecodebase64", &[str_val("AQ=="), str_val("UTF-16BE")]).is_err());
    }
    #[test]
    fn test_textencodebase64() {
        let non_str = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert_eq!(
            eval_func("textencodebase64", &[str_val("hello"), str_val("UTF-8")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::String("aGVsbG8=".to_string())
        );
        let sjis_enc = eval_func(
            "textencodebase64",
            &[str_val("こんにちは"), str_val("Shift_JIS")],
        )
        .unwrap();
        let sjis_dec = eval_func("textdecodebase64", &[sjis_enc, str_val("Shift_JIS")]).unwrap();
        assert_eq!(
            sjis_dec.data.as_ref(),
            &ValueData::String("こんにちは".to_string())
        );
        assert!(
            eval_func(
                "textencodebase64",
                &[str_val("hello 🚀"), str_val("ISO-8859-1")]
            )
            .is_err()
        );
        assert!(eval_func("textencodebase64", &[]).is_err());
        assert!(
            eval_func("textencodebase64", &[unk_val(), str_val("utf-8")])
                .unwrap()
                .is_unknown()
        );
        assert!(
            eval_func("textencodebase64", &[str_val("hello"), unk_val()])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("textencodebase64", &[str_val("hello"), non_str.clone()]).is_err());
        assert!(eval_func("textencodebase64", &[non_str, str_val("utf-8")]).is_err());
        assert!(
            eval_func(
                "textencodebase64",
                &[str_val("hello"), str_val("unsupported-charset-xyz")]
            )
            .is_err()
        );
    }
    #[test]
    fn test_base64gzip() {
        let res = eval_func("base64gzip", &[str_val("hello")]).unwrap();
        assert_eq!(
            std::mem::discriminant(&*res.data),
            std::mem::discriminant(&ValueData::String(String::new()))
        );
        assert!(eval_func("base64gzip", &[]).is_err());
        assert!(eval_func("base64gzip", &[unk_val()]).unwrap().is_unknown());
    }
    #[test]
    fn test_base64gunzip() {
        let original = "Hello, HCL v2 gzip compression!";
        let gzipped = eval_func("base64gzip", &[str_val(original)]).unwrap();
        let gunzipped = eval_func("base64gunzip", &[gzipped]).unwrap();
        assert_eq!(*gunzipped.data, ValueData::String(original.to_string()));
        assert!(eval_func("base64gunzip", &[]).is_err());
        assert!(
            eval_func("base64gunzip", &[unk_val()])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("base64gunzip", &[str_val("invalid base64!")]).is_err());
        assert!(eval_func("base64gunzip", &[str_val("aGVsbG8=")]).is_err());
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&[0xFF, 0xFE, 0xFD]).unwrap();
        let invalid_utf8_gz = encoder.finish().unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&invalid_utf8_gz);
        assert!(eval_func("base64gunzip", &[str_val(&b64)]).is_err());
    }
    #[test]
    fn test_urlbase64decode() {
        assert_eq!(
            eval_func("urlbase64decode", &[str_val("aGVsbG8")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::String("hello".to_string())
        );
        assert!(eval_func("urlbase64decode", &[]).is_err());
        assert!(
            eval_func("urlbase64decode", &[unk_val()])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("urlbase64decode", &[str_val("invalid!")]).is_err());
        assert!(eval_func("urlbase64decode", &[str_val("_w")]).is_err());
    }
    #[test]
    fn test_urlbase64encode() {
        assert_eq!(
            eval_func("urlbase64encode", &[str_val("hello")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::String("aGVsbG8".to_string())
        );
        assert!(eval_func("urlbase64encode", &[]).is_err());
        assert!(
            eval_func("urlbase64encode", &[unk_val()])
                .unwrap()
                .is_unknown()
        );
    }
    #[test]
    fn test_csvdecode() {
        let csv = "a,b\n1,2";
        let res = eval_func("csvdecode", &[str_val(csv)]).unwrap();
        assert_eq!(
            std::mem::discriminant(&*res.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );
        assert!(eval_func("csvdecode", &[]).is_err());
        assert!(eval_func("csvdecode", &[unk_val()]).unwrap().is_unknown());
        assert!(eval_func("csvdecode", &[str_val("a,b\n\"")]).is_err());
        assert!(eval_func("csvdecode", &[str_val("a,b\n1,2,3")]).is_err());
    }
    #[test]
    fn test_json_yaml_exhaustive() {
        let mut map = BTreeMap::new();
        map.insert("null".to_string(), null_val());
        map.insert(
            "bool".to_string(),
            Value::new(Type::Bool, ValueData::Bool(true)),
        );
        map.insert("string".to_string(), str_val("str"));
        map.insert("number".to_string(), num_val("42"));
        map.insert(
            "arr".to_string(),
            Value::new(
                Type::List(Box::new(Type::Number)),
                ValueData::Array(vec![num_val("1")]),
            ),
        );
        let mut set = std::collections::BTreeSet::new();
        set.insert(num_val("2"));
        map.insert(
            "set".to_string(),
            Value::new(Type::Set(Box::new(Type::Number)), ValueData::Set(set)),
        );
        let complex = Value::new(Type::Map(Box::new(Type::Dynamic)), ValueData::Object(map));
        let json_str = eval_func("jsonencode", std::slice::from_ref(&complex)).unwrap();
        let yaml_str = eval_func("yamlencode", std::slice::from_ref(&complex)).unwrap();
        let json_decoded = eval_func("jsondecode", &[json_str]).unwrap();
        let yaml_decoded = eval_func("yamldecode", &[yaml_str]).unwrap();
        assert_eq!(
            std::mem::discriminant(&*json_decoded.data),
            std::mem::discriminant(&ValueData::Object(BTreeMap::new()))
        );
        assert_eq!(
            std::mem::discriminant(&*yaml_decoded.data),
            std::mem::discriminant(&ValueData::Object(BTreeMap::new()))
        );
        let json_arr =
            eval_func("jsondecode", &[str_val("[null, true, \"s\", 42, [], {}]")]).unwrap();
        assert_eq!(
            std::mem::discriminant(&*json_arr.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );
        let yaml_arr = eval_func(
            "yamldecode",
            &[str_val("- null\n- true\n- s\n- 42\n- []\n- {}")],
        )
        .unwrap();
        assert_eq!(
            std::mem::discriminant(&*yaml_arr.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );
        let res = eval_func("yamldecode", &[str_val("a: !custom tagged\n1: a")]).unwrap();
        assert_eq!(
            std::mem::discriminant(&*res.data),
            std::mem::discriminant(&ValueData::Object(BTreeMap::new()))
        );
        assert!(eval_func("jsondecode", &[str_val("{\"a\": 1e100000}")]).is_err());
        let _ = eval_func("yamldecode", &[str_val("a: 1e100000")]);
    }
    #[test]
    fn test_jsonencode() {
        let obj = Value::new(
            Type::Map(Box::new(Type::Number)),
            ValueData::Object(BTreeMap::new()),
        );
        assert!(eval_func("jsonencode", &[obj]).is_ok());
        assert!(eval_func("jsonencode", &[]).is_err());
        let res = eval_func("jsonencode", &[unk_val()]).unwrap();
        assert!(res.is_unknown());
    }
    #[test]
    fn test_jsondecode() {
        assert!(eval_func("jsondecode", &[str_val("{}")]).is_ok());
        assert!(eval_func("jsondecode", &[]).is_err());
        assert!(eval_func("jsondecode", &[unk_val()]).unwrap().is_unknown());
        assert!(eval_func("jsondecode", &[str_val("{")]).is_err());
    }
    #[test]
    fn test_yamlencode() {
        let obj = Value::new(
            Type::Map(Box::new(Type::Number)),
            ValueData::Object(BTreeMap::new()),
        );
        assert!(eval_func("yamlencode", &[obj]).is_ok());
        assert!(eval_func("yamlencode", &[]).is_err());
        let res = eval_func("yamlencode", &[unk_val()]).unwrap();
        assert!(res.is_unknown());
    }
    #[test]
    fn test_yamldecode() {
        assert!(eval_func("yamldecode", &[str_val("a: 1")]).is_ok());
        assert!(eval_func("yamldecode", &[]).is_err());
        assert!(eval_func("yamldecode", &[unk_val()]).unwrap().is_unknown());
        assert!(eval_func("yamldecode", &[str_val("{invalid")]).is_err());
        assert!(eval_func("yamldecode", &[str_val("")]).is_ok());
    }
    #[test]
    fn test_encoding_coercion_errors() {
        let non_str = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!(eval_func("base64decode", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("base64encode", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("base64gunzip", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("base64gzip", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("urlbase64decode", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("urlbase64encode", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("jsondecode", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("yamldecode", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("csvdecode", std::slice::from_ref(&non_str)).is_err());
    }
}
