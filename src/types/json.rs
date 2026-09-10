//! Canonical JSON wire format implementation for `cty` Types and Values (`cty/json` equivalent).

use crate::error::HclError;
use crate::number::Number;
use crate::types::ty::Type;
use crate::types::val::{Value, ValueData};
use serde_json::{Map, Value as JsonValue, json};
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

/// Encodes an HCL [`Type`] into its canonical `cty/json` representation.
///
/// # Arguments
/// * `ty` - The type to encode.
#[must_use]
pub fn encode_type_to_json(ty: &Type) -> JsonValue {
    match ty {
        Type::Dynamic => JsonValue::String("dynamic".to_string()),
        Type::String => JsonValue::String("string".to_string()),
        Type::Number => JsonValue::String("number".to_string()),
        Type::Bool => JsonValue::String("bool".to_string()),
        Type::List(elem) => json!(["list", encode_type_to_json(elem)]),
        Type::Set(elem) => json!(["set", encode_type_to_json(elem)]),
        Type::Map(elem) => json!(["map", encode_type_to_json(elem)]),
        Type::Tuple(elems) => {
            let encoded_elems: Vec<JsonValue> = elems.iter().map(encode_type_to_json).collect();
            json!(["tuple", encoded_elems])
        }
        Type::Object {
            attrs,
            optional_attrs: _,
        } => {
            let mut obj_map = Map::new();
            for (k, v) in attrs {
                obj_map.insert(k.clone(), encode_type_to_json(v));
            }
            json!(["object", obj_map])
        }
        Type::Capsule { name, .. } => json!(["capsule", name]),
    }
}

/// Decodes an HCL [`Type`] from its canonical `cty/json` representation.
///
/// # Arguments
/// * `val` - The JSON value representing the type.
///
/// # Errors
/// Returns [`HclError::CtyJson`] if the JSON structure does not match a valid `cty` type signature.
pub fn decode_type_from_json(val: &JsonValue) -> Result<Type, HclError> {
    match val {
        JsonValue::String(s) => match s.as_str() {
            "dynamic" => Ok(Type::Dynamic),
            "string" => Ok(Type::String),
            "number" => Ok(Type::Number),
            "bool" => Ok(Type::Bool),
            other => Err(HclError::CtyJson(format!(
                "Unknown primitive type '{other}'"
            ))),
        },
        JsonValue::Array(arr) => {
            if arr.len() < 2 {
                return Err(HclError::CtyJson(
                    "Type array must have at least 2 elements".to_string(),
                ));
            }
            let kind = arr[0]
                .as_str()
                .ok_or_else(|| HclError::CtyJson("Type kind must be a string".to_string()))?;
            match kind {
                "list" => {
                    let elem = decode_type_from_json(&arr[1])?;
                    Ok(Type::List(Box::new(elem)))
                }
                "set" => {
                    let elem = decode_type_from_json(&arr[1])?;
                    Ok(Type::Set(Box::new(elem)))
                }
                "map" => {
                    let elem = decode_type_from_json(&arr[1])?;
                    Ok(Type::Map(Box::new(elem)))
                }
                "tuple" => {
                    let elems_arr = arr[1].as_array().ok_or_else(|| {
                        HclError::CtyJson("Tuple element signature must be an array".to_string())
                    })?;
                    let mut elems = Vec::with_capacity(elems_arr.len());
                    for el in elems_arr {
                        elems.push(decode_type_from_json(el)?);
                    }
                    Ok(Type::Tuple(elems))
                }
                "object" => {
                    let obj_map = arr[1].as_object().ok_or_else(|| {
                        HclError::CtyJson("Object attribute signature must be a map".to_string())
                    })?;
                    let mut attrs = BTreeMap::new();
                    for (k, v) in obj_map {
                        attrs.insert(k.clone(), decode_type_from_json(v)?);
                    }
                    Ok(Type::Object {
                        attrs,
                        optional_attrs: BTreeSet::new(),
                    })
                }
                "capsule" => {
                    let name = arr[1].as_str().ok_or_else(|| {
                        HclError::CtyJson("Capsule name must be a string".to_string())
                    })?;
                    Ok(Type::capsule::<()>(Box::leak(
                        name.to_string().into_boxed_str(),
                    )))
                }
                other => Err(HclError::CtyJson(format!(
                    "Unsupported compound type constructor '{other}'"
                ))),
            }
        }
        _ => Err(HclError::CtyJson(format!(
            "Invalid type JSON value: {val:?}"
        ))),
    }
}

/// Encodes an HCL [`Value`] into its JSON representation conforming to `cty/json`.
///
/// # Arguments
/// * `val` - The value to encode.
///
/// # Errors
/// Returns [`HclError::CtyJson`] if encoding fails.
pub fn encode_value_to_json(val: &Value) -> Result<JsonValue, HclError> {
    match &*val.data {
        ValueData::Null => Ok(JsonValue::Null),
        ValueData::Unknown(_) => Ok(json!({"__unknown": true})),
        ValueData::Bool(b) => Ok(JsonValue::Bool(*b)),
        ValueData::Number(n) => {
            let s = n.0.to_string();
            if let Ok(num) = serde_json::Number::from_str(&s) {
                Ok(JsonValue::Number(num))
            } else {
                Ok(JsonValue::String(s))
            }
        }
        ValueData::String(s) => Ok(JsonValue::String(s.clone())),
        ValueData::Array(arr) => {
            let mut list = Vec::with_capacity(arr.len());
            for item in arr {
                list.push(encode_value_to_json(item)?);
            }
            Ok(JsonValue::Array(list))
        }
        ValueData::Set(set) => {
            let mut list = Vec::with_capacity(set.len());
            for item in set {
                list.push(encode_value_to_json(item)?);
            }
            Ok(JsonValue::Array(list))
        }
        ValueData::Object(map) => {
            let mut obj = Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), encode_value_to_json(v)?);
            }
            Ok(JsonValue::Object(obj))
        }
        ValueData::Capsule(_) => {
            if let Ok(coerced) = val.coerce(&Type::String) {
                encode_value_to_json(&coerced)
            } else if val.ty().capsule_ops().is_some() {
                Err(HclError::CtyJson(format!(
                    "Cannot encode capsule '{}' to JSON without string conversion",
                    val.ty()
                )))
            } else {
                Ok(JsonValue::String(val.to_string()))
            }
        }
    }
}

/// Encodes an HCL [`Value`] into a typed JSON envelope `{ "type": ..., "value": ... }`.
///
/// # Arguments
/// * `val` - The value to encode.
///
/// # Errors
/// Returns [`HclError::CtyJson`] if serialization fails.
pub fn encode_typed_value(val: &Value) -> Result<JsonValue, HclError> {
    let ty_json = encode_type_to_json(val.ty());
    let val_json = encode_value_to_json(val)?;
    Ok(json!({
        "type": ty_json,
        "value": val_json,
    }))
}

/// Encodes an HCL [`Value`] into a JSON value, preserving sensitivity and custom marks in a `@cty.marks` envelope.
///
/// If the value has no marks, returns the plain JSON value representation.
///
/// # Arguments
/// * `val` - The value to encode.
///
/// # Errors
/// Returns [`HclError::CtyJson`] if serialization fails.
pub fn encode_value_to_json_with_marks(val: &Value) -> Result<JsonValue, HclError> {
    let raw = encode_value_to_json(val)?;
    if val.marks.is_empty() {
        Ok(raw)
    } else {
        let marks: Vec<JsonValue> = val
            .marks
            .iter()
            .map(|m| JsonValue::String(m.to_string()))
            .collect();
        let mut map = Map::new();
        map.insert("@cty.marks".to_string(), JsonValue::Array(marks));
        map.insert("value".to_string(), raw);
        Ok(JsonValue::Object(map))
    }
}

/// Decodes an HCL [`Value`] from JSON conforming to the given [`Type`].
///
/// # Arguments
/// * `val` - The JSON value.
/// * `ty` - The expected HCL type.
///
/// # Errors
/// Returns [`HclError::CtyJson`] if the JSON data cannot be coerced to the expected type.
pub fn decode_value_from_json(val: &JsonValue, ty: &Type) -> Result<Value, HclError> {
    if let Some(obj) = val.as_object() {
        if obj.contains_key("__unknown") {
            return Ok(Value::unknown(ty.clone()));
        }
        if let (Some(marks_val), Some(inner_val)) = (obj.get("@cty.marks"), obj.get("value")) {
            let mut decoded = decode_value_from_json(inner_val, ty)?;
            if let Some(arr) = marks_val.as_array() {
                for m in arr {
                    if let Some(s) = m.as_str() {
                        if s == "sensitive" {
                            decoded
                                .marks
                                .insert(crate::types::val::ValueMark::Sensitive);
                        } else {
                            decoded
                                .marks
                                .insert(crate::types::val::ValueMark::custom(s));
                        }
                    }
                }
            }
            return Ok(decoded);
        }
    }

    match (val, ty) {
        (JsonValue::Null, _) => Ok(Value::null(ty.clone())),
        (JsonValue::Bool(b), Type::Dynamic) => Ok(Value::new(Type::Bool, ValueData::Bool(*b))),
        (JsonValue::Number(_), Type::Dynamic) => decode_value_from_json(val, &Type::Number),
        (JsonValue::String(s), Type::Dynamic) => {
            Ok(Value::new(Type::String, ValueData::String(s.clone())))
        }
        (JsonValue::Array(arr), Type::Dynamic) => {
            let items = arr
                .iter()
                .flat_map(|el| decode_value_from_json(el, &Type::Dynamic))
                .collect();
            Ok(Value::new(
                Type::List(Box::new(Type::Dynamic)),
                ValueData::Array(items),
            ))
        }
        (JsonValue::Object(map), Type::Dynamic) => {
            let mut obj = BTreeMap::new();
            for (k, v) in map {
                obj.extend(decode_value_from_json(v, &Type::Dynamic).map(|val| (k.clone(), val)));
            }
            Ok(Value::new(
                Type::object(BTreeMap::new()),
                ValueData::Object(obj),
            ))
        }
        (_, Type::Bool) => {
            let b = val
                .as_bool()
                .ok_or_else(|| HclError::CtyJson(format!("Expected bool, got {val:?}")))?;
            Ok(Value::new(Type::Bool, ValueData::Bool(b)))
        }
        (_, Type::Number) => {
            let num_str = match val {
                JsonValue::Number(n) => n.to_string(),
                JsonValue::String(s) => s.clone(),
                _ => {
                    return Err(HclError::CtyJson(format!(
                        "Expected number representation, got {val:?}"
                    )));
                }
            };
            let num = Number::from_str(&num_str).map_err(|e| HclError::CtyJson(e.to_string()))?;
            Ok(Value::new(Type::Number, ValueData::Number(num)))
        }
        (_, Type::String) => {
            let s = val
                .as_str()
                .ok_or_else(|| HclError::CtyJson(format!("Expected string, got {val:?}")))?;
            Ok(Value::new(Type::String, ValueData::String(s.to_string())))
        }
        (_, Type::List(elem_ty)) => {
            let arr = val.as_array().ok_or_else(|| {
                HclError::CtyJson(format!("Expected array for list, got {val:?}"))
            })?;
            let mut list = Vec::with_capacity(arr.len());
            for item in arr {
                list.push(decode_value_from_json(item, elem_ty)?);
            }
            Ok(Value::new(ty.clone(), ValueData::Array(list)))
        }
        (_, Type::Set(elem_ty)) => {
            let arr = val
                .as_array()
                .ok_or_else(|| HclError::CtyJson(format!("Expected array for set, got {val:?}")))?;
            let mut set = std::collections::BTreeSet::new();
            for item in arr {
                set.insert(decode_value_from_json(item, elem_ty)?);
            }
            Ok(Value::new(ty.clone(), ValueData::Set(set)))
        }
        (_, Type::Map(elem_ty)) => {
            let obj = val.as_object().ok_or_else(|| {
                HclError::CtyJson(format!("Expected object for map, got {val:?}"))
            })?;
            let mut map = BTreeMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), decode_value_from_json(v, elem_ty)?);
            }
            Ok(Value::new(ty.clone(), ValueData::Object(map)))
        }
        (_, Type::Tuple(elem_types)) => {
            let arr = val.as_array().ok_or_else(|| {
                HclError::CtyJson(format!("Expected array for tuple, got {val:?}"))
            })?;
            if arr.len() != elem_types.len() {
                return Err(HclError::CtyJson(format!(
                    "Tuple length mismatch: expected {}, got {}",
                    elem_types.len(),
                    arr.len()
                )));
            }
            let mut tuple_vals = Vec::with_capacity(arr.len());
            for (item, item_ty) in arr.iter().zip(elem_types.iter()) {
                tuple_vals.push(decode_value_from_json(item, item_ty)?);
            }
            Ok(Value::new(ty.clone(), ValueData::Array(tuple_vals)))
        }
        (
            _,
            Type::Object {
                attrs,
                optional_attrs,
            },
        ) => {
            let obj = val
                .as_object()
                .ok_or_else(|| HclError::CtyJson(format!("Expected object, got {val:?}")))?;
            let mut res_map = BTreeMap::new();
            for (k, attr_ty) in attrs {
                if let Some(field_val) = obj.get(k) {
                    res_map.insert(k.clone(), decode_value_from_json(field_val, attr_ty)?);
                } else if optional_attrs.contains(k) {
                    res_map.insert(k.clone(), Value::null(attr_ty.clone()));
                } else {
                    return Err(HclError::CtyJson(format!(
                        "Missing required attribute '{k}' for object"
                    )));
                }
            }
            Ok(Value::new(ty.clone(), ValueData::Object(res_map)))
        }
        (_, Type::Capsule { .. }) => {
            if let Some(s) = val.as_str() {
                Value::new(Type::String, ValueData::String(s.to_string()))
                    .coerce(ty)
                    .map_err(HclError::CtyJson)
            } else {
                Err(HclError::CtyJson(format!(
                    "Cannot decode capsule value from {val:?}"
                )))
            }
        }
    }
}

/// Decodes an HCL [`Value`] from a typed JSON envelope `{ "type": ..., "value": ... }`.
///
/// # Arguments
/// * `json` - The typed JSON envelope.
///
/// # Errors
/// Returns [`HclError::CtyJson`] if the envelope is invalid or decoding fails.
pub fn decode_typed_value(json: &JsonValue) -> Result<Value, HclError> {
    let obj = json.as_object().ok_or_else(|| {
        HclError::CtyJson("Typed value envelope must be a JSON object".to_string())
    })?;
    let ty_json = obj.get("type").ok_or_else(|| {
        HclError::CtyJson("Typed value envelope missing 'type' property".to_string())
    })?;
    let val_json = obj.get("value").ok_or_else(|| {
        HclError::CtyJson("Typed value envelope missing 'value' property".to_string())
    })?;

    let ty = decode_type_from_json(ty_json)?;
    decode_value_from_json(val_json, &ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cty_json_type_roundtrip() {
        let types = vec![
            Type::Dynamic,
            Type::String,
            Type::Number,
            Type::Bool,
            Type::List(Box::new(Type::String)),
            Type::Set(Box::new(Type::Number)),
            Type::Map(Box::new(Type::Bool)),
            Type::Tuple(vec![Type::String, Type::Number]),
            Type::Object {
                attrs: BTreeMap::from([
                    ("foo".to_string(), Type::String),
                    ("bar".to_string(), Type::Number),
                ]),
                optional_attrs: BTreeSet::new(),
            },
            Type::capsule::<()>("my_capsule"),
        ];

        for ty in &types {
            let encoded = encode_type_to_json(ty);
            let decoded = decode_type_from_json(&encoded).expect("decoded type");
            assert_eq!(ty, &decoded);
        }

        // Test decode invalid types
        assert!(decode_type_from_json(&json!("unknown_type")).is_err());
        assert!(decode_type_from_json(&json!(["bad_kind"])).is_err());
        assert!(decode_type_from_json(&json!(123)).is_err());
        assert!(decode_type_from_json(&json!(["list"])).is_err());
        assert!(decode_type_from_json(&json!(["unsupported", "elem"])).is_err());
    }

    #[test]
    fn test_cty_json_value_roundtrip() {
        let mut obj = BTreeMap::new();
        obj.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("cluster".to_string())),
        );
        obj.insert(
            "count".to_string(),
            Value::new(Type::Number, ValueData::Number(Number::from(3_i64))),
        );

        let ty = Type::Object {
            attrs: BTreeMap::from([
                ("name".to_string(), Type::String),
                ("count".to_string(), Type::Number),
            ]),
            optional_attrs: BTreeSet::new(),
        };

        let val = Value::new(ty.clone(), ValueData::Object(obj));
        let encoded = encode_value_to_json(&val).expect("encoded value");
        let decoded = decode_value_from_json(&encoded, &ty).expect("decoded value");
        assert_eq!(val, decoded);

        // Test typed value envelope
        let envelope = encode_typed_value(&val).expect("envelope");
        let from_env = decode_typed_value(&envelope).expect("from envelope");
        assert_eq!(val, from_env);

        // Test null and unknown values
        let null_val = Value::null(Type::String);
        let null_env = encode_typed_value(&null_val).expect("null envelope");
        let from_null = decode_typed_value(&null_env).expect("from null");
        assert!(from_null.is_null());

        let unk_val = Value::unknown(Type::Number);
        let unk_env = encode_typed_value(&unk_val).expect("unknown envelope");
        let from_unk = decode_typed_value(&unk_env).expect("from unknown");
        assert!(from_unk.is_unknown());
    }

    #[test]
    fn test_cty_json_collections_and_error_paths() {
        use crate::encode::EncodeValue;

        // 1. List, Set, Map, Tuple roundtrips
        let list_val = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(vec!["a".encode_value(), "b".encode_value()]),
        );
        let enc_list = encode_value_to_json(&list_val).expect("enc list");
        let dec_list = decode_value_from_json(&enc_list, list_val.ty()).expect("dec list");
        assert_eq!(list_val, dec_list);

        let mut set = BTreeSet::new();
        set.insert(1_i64.encode_value());
        set.insert(2_i64.encode_value());
        let set_val = Value::new(Type::Set(Box::new(Type::Number)), ValueData::Set(set));
        let enc_set = encode_value_to_json(&set_val).expect("enc set");
        let dec_set = decode_value_from_json(&enc_set, set_val.ty()).expect("dec set");
        assert_eq!(set_val, dec_set);

        let mut map = BTreeMap::new();
        map.insert("k1".to_string(), true.encode_value());
        let map_val = Value::new(Type::Map(Box::new(Type::Bool)), ValueData::Object(map));
        let enc_map = encode_value_to_json(&map_val).expect("enc map");
        let dec_map = decode_value_from_json(&enc_map, map_val.ty()).expect("dec map");
        assert_eq!(map_val, dec_map);

        let tuple_val = Value::new(
            Type::Tuple(vec![Type::String, Type::Bool]),
            ValueData::Array(vec!["hello".encode_value(), false.encode_value()]),
        );
        let enc_tup = encode_value_to_json(&tuple_val).expect("enc tup");
        let dec_tup = decode_value_from_json(&enc_tup, tuple_val.ty()).expect("dec tup");
        assert_eq!(tuple_val, dec_tup);

        // 2. Dynamic decoding
        let dyn_bool = decode_value_from_json(&json!(true), &Type::Dynamic).expect("dyn bool");
        assert_eq!(dyn_bool, true.encode_value());
        let dyn_num = decode_value_from_json(&json!(123), &Type::Dynamic).expect("dyn num");
        assert_eq!(dyn_num, 123_i64.encode_value());
        let dyn_str = decode_value_from_json(&json!("dyn"), &Type::Dynamic).expect("dyn str");
        assert_eq!(dyn_str, "dyn".encode_value());
        let dyn_arr = decode_value_from_json(&json!([1, 2]), &Type::Dynamic).expect("dyn arr");
        assert!(matches!(dyn_arr.data.as_ref(), ValueData::Array(_)));
        let dyn_obj = decode_value_from_json(&json!({"a": true}), &Type::Dynamic).expect("dyn obj");
        assert!(matches!(dyn_obj.data.as_ref(), ValueData::Object(_)));

        // 3. Object with optional_attrs
        let obj_ty = Type::Object {
            attrs: BTreeMap::from([
                ("req".to_string(), Type::String),
                ("opt".to_string(), Type::Number),
            ]),
            optional_attrs: BTreeSet::from(["opt".to_string()]),
        };
        let partial_json = json!({"req": "present"});
        let dec_opt = decode_value_from_json(&partial_json, &obj_ty).expect("dec opt");
        assert!(
            dec_opt
                .get_path(
                    &crate::types::path::Path::empty()
                        .with_step(crate::types::path::PathStep::GetAttr("opt".to_string()))
                )
                .expect("opt")
                .is_null()
        );

        // Missing required attribute error
        let missing_json = json!({"opt": 10});
        assert!(decode_value_from_json(&missing_json, &obj_ty).is_err());

        // Tuple length mismatch error
        assert!(decode_value_from_json(&json!(["a"]), tuple_val.ty()).is_err());

        // Envelope errors
        assert!(decode_typed_value(&json!("not_an_obj")).is_err());
        assert!(decode_typed_value(&json!({"type": "string"})).is_err());
        assert!(decode_typed_value(&json!({"value": "hello"})).is_err());

        // Type errors
        assert!(decode_value_from_json(&json!("not_bool"), &Type::Bool).is_err());
        assert!(decode_value_from_json(&json!("not_num"), &Type::Number).is_err());
        assert!(decode_value_from_json(&json!(123), &Type::String).is_err());
        assert!(
            decode_value_from_json(&json!("not_arr"), &Type::List(Box::new(Type::String))).is_err()
        );
        assert!(
            decode_value_from_json(&json!("not_arr"), &Type::Set(Box::new(Type::String))).is_err()
        );
        assert!(
            decode_value_from_json(&json!("not_obj"), &Type::Map(Box::new(Type::String))).is_err()
        );
        assert!(decode_value_from_json(&json!("not_obj"), &obj_ty).is_err());

        // 4. Capsule encoding & decoding and edge cases
        let cap_val = Value::capsule::<()>("test_cap", ());
        let cap_json = encode_value_to_json(&cap_val).expect("enc capsule");
        assert!(cap_json.is_string());

        let cap_ty = Type::capsule::<()>("test_cap");
        assert!(decode_value_from_json(&json!(123), &cap_ty).is_err());

        // Decode type edge cases
        assert!(decode_type_from_json(&json!([123, "elem"])).is_err());
        assert!(decode_type_from_json(&json!(["tuple", 123])).is_err());
        assert!(decode_type_from_json(&json!(["object", 123])).is_err());
        assert!(decode_type_from_json(&json!(["capsule", 123])).is_err());

        // Dynamic null decoding
        let dyn_null = decode_value_from_json(&json!(null), &Type::Dynamic).expect("dyn null");
        assert!(dyn_null.is_null());
    }

    #[test]
    fn test_cty_json_coverage_exhaustive() {
        use crate::types::ty::CapsuleOps;
        use std::sync::Arc;

        // 1. Number that overflows serde_json::Number to string fallback
        let mut huge_num = Value::null(Type::Number);
        for s in ["1e400", "invalid"] {
            if let Ok(n) = Number::from_str(s) {
                huge_num = Value::new(Type::Number, ValueData::Number(n));
            }
        }
        let enc_huge = encode_value_to_json(&huge_num);
        assert!(enc_huge.as_ref().is_ok_and(JsonValue::is_string));
        assert_eq!(
            enc_huge.as_ref().ok().and_then(JsonValue::as_str),
            Some("1e+400")
        );

        // 2. Capsule with string coercion
        let cap_ops = Arc::new(
            CapsuleOps::new("custom_str_cap", Arc::new(|_a, _b| false), Arc::new(|_| 0))
                .with_conversion_to(Arc::new(|a, target| {
                    if target == &Type::String
                        && let Some(s) = a.downcast_ref::<String>()
                    {
                        Some(Value::new(Type::String, ValueData::String(s.clone())))
                    } else {
                        None
                    }
                }))
                .with_conversion_from(Arc::new(|val| {
                    if let ValueData::String(s) = &*val.data {
                        Some(Arc::new(s.clone()))
                    } else {
                        None
                    }
                })),
        );

        let cap_val =
            Value::capsule_with_ops("custom_str_cap", cap_ops.clone(), "hello_cap".to_string());
        let enc_cap = encode_value_to_json(&cap_val);
        assert_eq!(enc_cap.ok(), Some(json!("hello_cap")));

        // Fallback branches for capsule conversion closures
        let bad_any: Arc<dyn std::any::Any + Send + Sync> = Arc::new(123_i32);
        assert!(
            cap_ops
                .conversion_to
                .as_ref()
                .is_some_and(|conv| (conv)(&bad_any, &Type::Number).is_none())
        );
        assert!(
            cap_ops
                .conversion_to
                .as_ref()
                .is_some_and(|conv| (conv)(&bad_any, &Type::String).is_none())
        );
        assert!(cap_ops.conversion_from.as_ref().is_some_and(|conv| {
            (conv)(&Value::new(Type::Number, ValueData::Number(123_i64.into()))).is_none()
        }));

        let cap_ty = Type::capsule_with_ops::<String>("custom_str_cap", cap_ops);
        let dec_cap = decode_value_from_json(&json!("hello_cap"), &cap_ty);
        assert_eq!(
            dec_cap.as_ref().ok().and_then(|v| v.ty().capsule_name()),
            Some("custom_str_cap")
        );

        // Unencodable capsule with ops but no string conversion
        let no_str_ops = Arc::new(CapsuleOps::new(
            "unencodable_cap",
            Arc::new(|_a, _b| true),
            Arc::new(|_| 0),
        ));
        let unencodable_cap = Value::capsule_with_ops("unencodable_cap", no_str_ops, ());
        assert!(encode_value_to_json(&unencodable_cap).is_err());
        assert!(
            encode_value_to_json(&Value::new(
                Type::List(Box::new(unencodable_cap.ty().clone())),
                ValueData::Array(vec![unencodable_cap.clone()]),
            ))
            .is_err()
        );
        assert!(
            encode_value_to_json(&Value::new(
                Type::Set(Box::new(unencodable_cap.ty().clone())),
                ValueData::Set(std::collections::BTreeSet::from([unencodable_cap.clone()])),
            ))
            .is_err()
        );
        assert!(
            encode_value_to_json(&Value::new(
                Type::object(BTreeMap::from([("k".into(), unencodable_cap.ty().clone())])),
                ValueData::Object(BTreeMap::from([("k".into(), unencodable_cap.clone())])),
            ))
            .is_err()
        );
        assert!(encode_typed_value(&unencodable_cap).is_err());

        // decode_type_from_json nested error paths
        assert!(decode_type_from_json(&json!(["list", "bad"])).is_err());
        assert!(decode_type_from_json(&json!(["set", "bad"])).is_err());
        assert!(decode_type_from_json(&json!(["map", "bad"])).is_err());
        assert!(decode_type_from_json(&json!(["tuple", ["string", "bad"]])).is_err());
        assert!(decode_type_from_json(&json!(["object", {"k": "bad"}])).is_err());

        // 3. Number decoding from string and error paths
        let dec_num_str = decode_value_from_json(&json!("123.45"), &Type::Number);
        assert_eq!(
            dec_num_str.as_ref().ok().map(Value::ty),
            Some(&Type::Number)
        );
        assert!(decode_value_from_json(&json!(true), &Type::Number).is_err());
        assert!(decode_value_from_json(&json!([]), &Type::Number).is_err());

        // decode_value_from_json collection element error paths
        assert!(
            decode_value_from_json(&json!([1, "bad"]), &Type::List(Box::new(Type::Number)))
                .is_err()
        );
        assert!(
            decode_value_from_json(&json!([1, "bad"]), &Type::Set(Box::new(Type::Number))).is_err()
        );
        assert!(
            decode_value_from_json(
                &json!({"a": 1, "b": "bad"}),
                &Type::Map(Box::new(Type::Number))
            )
            .is_err()
        );
        assert!(
            decode_value_from_json(
                &json!([1, "bad"]),
                &Type::Tuple(vec![Type::Number, Type::Number])
            )
            .is_err()
        );
        let obj_spec_ty = Type::object(BTreeMap::from([("a".into(), Type::Number)]));
        assert!(decode_value_from_json(&json!({"a": "bad"}), &obj_spec_ty).is_err());

        // 4. Tuple decoding error path
        assert!(
            decode_value_from_json(&json!("not_array"), &Type::Tuple(vec![Type::String])).is_err()
        );

        // 5. decode_typed_value envelope error paths
        assert!(decode_typed_value(&json!("not_object")).is_err());
        assert!(decode_typed_value(&json!({"value": "something"})).is_err());
        assert!(decode_typed_value(&json!({"type": "string"})).is_err());
        assert!(decode_typed_value(&json!({"type": ["list", "bad"], "value": []})).is_err());
    }

    #[test]
    fn test_cty_json_marked_values_roundtrip() {
        use crate::types::val::ValueMark;

        // Plain value
        let val_plain = Value::new(Type::String, ValueData::String("unmarked".into()));
        let json_plain = encode_value_to_json_with_marks(&val_plain).expect("encode ok");
        assert_eq!(json_plain, json!("unmarked"));

        // Sensitive value
        let val_sens = val_plain.mark(ValueMark::Sensitive);
        let json_sens = encode_value_to_json_with_marks(&val_sens).expect("encode ok");
        assert_eq!(
            json_sens,
            json!({
                "@cty.marks": ["sensitive"],
                "value": "unmarked"
            })
        );

        let dec_sens = decode_value_from_json(&json_sens, &Type::String).expect("decode ok");
        assert_eq!(
            dec_sens.data.as_ref(),
            &ValueData::String("unmarked".into())
        );
        assert!(dec_sens.has_mark(&ValueMark::Sensitive));

        // Custom marked value
        let val_custom = val_plain.mark(ValueMark::custom("encrypted"));
        let json_custom = encode_value_to_json_with_marks(&val_custom).expect("encode ok");
        let dec_custom = decode_value_from_json(&json_custom, &Type::String).expect("decode ok");
        assert!(dec_custom.has_mark(&ValueMark::custom("encrypted")));

        // Envelope with non-array @cty.marks
        let json_invalid_marks = json!({
            "@cty.marks": "not_an_array",
            "value": "unmarked"
        });
        let dec_invalid_marks =
            decode_value_from_json(&json_invalid_marks, &Type::String).expect("decode ok");
        assert_eq!(
            dec_invalid_marks.data.as_ref(),
            &ValueData::String("unmarked".into())
        );
        assert!(dec_invalid_marks.marks.is_empty());

        // Envelope with array containing non-string items and empty array
        let json_mixed_marks = json!({
            "@cty.marks": [123, null, true, "sensitive", "custom_tag"],
            "value": "unmarked"
        });
        let dec_mixed_marks =
            decode_value_from_json(&json_mixed_marks, &Type::String).expect("decode ok");
        assert!(dec_mixed_marks.has_mark(&ValueMark::Sensitive));
        assert!(dec_mixed_marks.has_mark(&ValueMark::custom("custom_tag")));

        let json_empty_marks = json!({
            "@cty.marks": [],
            "value": "unmarked"
        });
        let dec_empty_marks =
            decode_value_from_json(&json_empty_marks, &Type::String).expect("decode ok");
        assert!(dec_empty_marks.marks.is_empty());

        // Inner value decoding failure inside marked envelope
        let json_err_inner = json!({
            "@cty.marks": ["sensitive"],
            "value": "not_a_boolean"
        });
        assert!(decode_value_from_json(&json_err_inner, &Type::Bool).is_err());

        // Encoding failure inside encode_value_to_json_with_marks
        let eq_fn: crate::types::ty::CapsuleEqualsFn = std::sync::Arc::new(|_, _| true);
        let hash_fn: crate::types::ty::CapsuleHashFn = std::sync::Arc::new(|_| 0);
        let ops = crate::types::ty::CapsuleOps::new("NonJsonCapsule", eq_fn, hash_fn);
        let cap_ty = Type::capsule_with_ops::<()>("NonJsonCapsule", std::sync::Arc::new(ops));
        let cap_val = Value::new(cap_ty, ValueData::Capsule(std::sync::Arc::new(())))
            .mark(ValueMark::Sensitive);
        assert!(encode_value_to_json_with_marks(&cap_val).is_err());
    }
}
