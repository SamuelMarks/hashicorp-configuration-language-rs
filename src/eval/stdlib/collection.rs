//! Collection standard library functions.

use crate::eval::func::Function;
use crate::number::Number;
use crate::types::unify::unify;
use crate::types::{Type, Value, ValueData};
use bigdecimal::BigDecimal;
use bigdecimal::num_traits::ToPrimitive;
use std::collections::{BTreeMap, BTreeSet};

use std::sync::Arc;

#[must_use]
/// Get all collection functions
pub fn functions() -> Vec<Function> {
    vec![
        alltrue_func(),
        anytrue_func(),
        chunklist_func(),
        coalesce_func(),
        coalescelist_func(),
        compact_func(),
        concat_func(),
        contains_func(),
        distinct_func(),
        element_func(),
        flatten_func(),
        index_func(),
        keys_func(),
        length_func(),
        list_func(),
        lookup_func(),
        map_func(),
        matchkeys_func(),
        merge_func(),
        one_func(),
        reverse_func(),
        setintersection_func(),
        setproduct_func(),
        setsubtract_func(),
        setsymmetricdifference_func(),
        setunion_func(),
        slice_func(),
        sort_func(),
        sum_func(),
        transpose_func(),
        values_func(),
        zipmap_func(),
    ]
}

fn alltrue_func() -> Function {
    Function {
        name: "alltrue".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("alltrue expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Bool));
            }

            let list = match &*args[0].data {
                ValueData::Array(arr) => arr,
                ValueData::Set(_set) => return Err("alltrue requires a list, got set".to_string()), // actually it works for sets but we can iterate easily
                _ => return Err("alltrue requires a list".to_string()),
            };

            for item in list {
                if item.is_unknown() {
                    return Ok(Value::unknown(Type::Bool));
                }
                let b = item
                    .clone()
                    .coerce(&Type::Bool)
                    .map_err(|_| "alltrue requires a list of booleans".to_string())?;
                if let ValueData::Bool(false) = *b.data {
                    return Ok(Value::new(Type::Bool, ValueData::Bool(false)));
                }
            }
            Ok(Value::new(Type::Bool, ValueData::Bool(true)))
        }),
        signature: None,
    }
}

fn anytrue_func() -> Function {
    Function {
        name: "anytrue".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("anytrue expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Bool));
            }

            let ValueData::Array(list) = &*args[0].data else {
                return Err("anytrue requires a list".to_string());
            };

            for item in list {
                if item.is_unknown() {
                    return Ok(Value::unknown(Type::Bool));
                }
                let b = item
                    .clone()
                    .coerce(&Type::Bool)
                    .map_err(|_| "anytrue requires a list of booleans".to_string())?;
                if let ValueData::Bool(true) = *b.data {
                    return Ok(Value::new(Type::Bool, ValueData::Bool(true)));
                }
            }
            Ok(Value::new(Type::Bool, ValueData::Bool(false)))
        }),
        signature: None,
    }
}

fn chunklist_func() -> Function {
    Function {
        name: "chunklist".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("chunklist expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::List(Box::new(args[0].ty().clone()))));
            }

            let ValueData::Array(list) = &*args[0].data else {
                return Err("chunklist requires a list".to_string());
            };

            let size_val = args[1]
                .clone()
                .coerce(&Type::Number)
                .map_err(|_| "chunklist size must be number".to_string())?;
            let size = match *size_val.data {
                ValueData::Number(n) => n.0.to_usize().ok_or("chunklist size too large")?,
                _ => return Err("chunklist size must be number".to_string()),
            };

            if size == 0 {
                return Err("chunklist size must be greater than 0".to_string());
            }

            let mut result = Vec::new();
            for chunk in list.chunks(size) {
                let inner_ty = if let Type::List(inner) = args[0].ty() {
                    inner.clone()
                } else {
                    Box::new(Type::Dynamic)
                };
                result.push(Value::new(
                    Type::List(inner_ty),
                    ValueData::Array(chunk.to_vec()),
                ));
            }
            let res_ty = if let Type::List(inner) = args[0].ty() {
                Type::List(Box::new(Type::List(inner.clone())))
            } else {
                Type::List(Box::new(Type::List(Box::new(Type::Dynamic))))
            };
            Ok(Value::new(res_ty, ValueData::Array(result)))
        }),
        signature: None,
    }
}

fn coalesce_func() -> Function {
    Function {
        name: "coalesce".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.is_empty() {
                return Err("coalesce expects at least 1 argument".to_string());
            }

            for arg in args {
                if arg.is_null() {
                    continue;
                }
                if let ValueData::String(s) = &*arg.data
                    && s.is_empty()
                {
                    continue;
                }
                if arg.is_unknown() {
                    return Ok(Value::unknown(arg.ty().clone()));
                }
                return Ok(arg.clone());
            }
            Err("no non-null, non-empty-string arguments".to_string())
        }),
        signature: None,
    }
}

fn coalescelist_func() -> Function {
    Function {
        name: "coalescelist".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.is_empty() {
                return Err("coalescelist expects at least 1 argument".to_string());
            }

            for arg in args {
                if arg.is_unknown() {
                    return Ok(Value::unknown(Type::List(Box::new(Type::Dynamic))));
                }
                if let ValueData::Array(arr) = &*arg.data
                    && !arr.is_empty()
                {
                    return Ok(arg.clone());
                }
            }
            Err("no non-empty list arguments".to_string())
        }),
        signature: None,
    }
}

fn compact_func() -> Function {
    Function {
        name: "compact".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("compact expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::List(Box::new(Type::String))));
            }

            let ValueData::Array(list) = &*args[0].data else {
                return Err("compact requires a list".to_string());
            };

            let mut result = Vec::new();
            for item in list {
                if item.is_null() {
                    continue;
                }
                let s = item
                    .clone()
                    .coerce(&Type::String)
                    .map_err(|_| "compact requires list of strings".to_string())?;
                match &*s.data {
                    ValueData::String(str_val) if !str_val.is_empty() => {
                        result.push(s);
                    }
                    _ => {}
                }
            }
            Ok(Value::new(
                Type::List(Box::new(Type::String)),
                ValueData::Array(result),
            ))
        }),
        signature: None,
    }
}

fn concat_func() -> Function {
    Function {
        name: "concat".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.is_empty() {
                return Err("concat expects at least 1 argument".to_string());
            }

            let mut res_ty = None;
            let mut result = Vec::new();

            for arg in args {
                if arg.is_unknown() {
                    return Ok(Value::unknown(Type::List(Box::new(Type::Dynamic))));
                }
                if let ValueData::Array(arr) = &*arg.data {
                    result.extend(arr.clone());
                    if let Type::List(inner) = arg.ty() {
                        match &res_ty {
                            None => res_ty = Some((**inner).clone()),
                            Some(current) => res_ty = unify(current, inner),
                        }
                    }
                } else {
                    return Err("concat arguments must be lists".to_string());
                }
            }

            let t = res_ty.unwrap_or(Type::Dynamic);
            Ok(Value::new(
                Type::List(Box::new(t)),
                ValueData::Array(result),
            ))
        }),
        signature: None,
    }
}
fn contains_func() -> Function {
    Function {
        name: "contains".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("contains expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Bool));
            }

            match &*args[0].data {
                ValueData::Array(arr) => {
                    for item in arr {
                        if item == &args[1] {
                            return Ok(Value::new(Type::Bool, ValueData::Bool(true)));
                        }
                    }
                    Ok(Value::new(Type::Bool, ValueData::Bool(false)))
                }
                ValueData::Set(set) => Ok(Value::new(
                    Type::Bool,
                    ValueData::Bool(set.contains(&args[1])),
                )),
                _ => Err("contains requires a list or set".to_string()),
            }
        }),
        signature: None,
    }
}

fn distinct_func() -> Function {
    Function {
        name: "distinct".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("distinct expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(args[0].ty().clone()));
            }

            let ValueData::Array(list) = &*args[0].data else {
                return Err("distinct requires a list".to_string());
            };

            let mut result = Vec::new();
            for item in list {
                if !result.contains(item) {
                    result.push(item.clone());
                }
            }
            Ok(Value::new(args[0].ty().clone(), ValueData::Array(result)))
        }),
        signature: None,
    }
}

fn element_func() -> Function {
    Function {
        name: "element".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("element expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                let inner_ty = if let Type::List(inner) = args[0].ty() {
                    (**inner).clone()
                } else {
                    Type::Dynamic
                };
                return Ok(Value::unknown(inner_ty));
            }

            let ValueData::Array(list) = &*args[0].data else {
                return Err("element requires a list".to_string());
            };

            if list.is_empty() {
                return Err("element cannot be used on an empty list".to_string());
            }

            let idx_val = args[1]
                .clone()
                .coerce(&Type::Number)
                .map_err(|_| "element index must be number".to_string())?;
            let idx = match *idx_val.data {
                ValueData::Number(n) => n.0.to_isize().ok_or("index too large")?,
                _ => return Err("element index must be number".to_string()),
            };

            let len = list.len() as isize;
            let mut mod_idx = idx % len;
            if mod_idx < 0 {
                mod_idx += len;
            } // though TF element doesn't typically accept negative, but modulo means this. Actually TF just wraps around natively.

            Ok(list[mod_idx as usize].clone())
        }),
        signature: None,
    }
}

fn flatten_func() -> Function {
    Function {
        name: "flatten".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("flatten expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::List(Box::new(Type::Dynamic))));
            }

            let ValueData::Array(list) = &*args[0].data else {
                return Err("flatten requires a list".to_string());
            };

            fn flatten_recurse(arr: &[Value], result: &mut Vec<Value>) {
                for item in arr {
                    if let ValueData::Array(inner) = &*item.data {
                        flatten_recurse(inner, result);
                    } else if let ValueData::Set(inner) = &*item.data {
                        // sets converted to list implicitly?
                        let inner_arr: Vec<Value> = inner.iter().cloned().collect();
                        flatten_recurse(&inner_arr, result);
                    } else {
                        result.push(item.clone());
                    }
                }
            }

            let mut result = Vec::new();
            flatten_recurse(list, &mut result);

            let mut res_ty = None;
            for item in &result {
                match &res_ty {
                    None => res_ty = Some(item.ty().clone()),
                    Some(current) => res_ty = unify(current, item.ty()),
                }
            }
            let t = res_ty.unwrap_or(Type::Dynamic);
            Ok(Value::new(
                Type::List(Box::new(t)),
                ValueData::Array(result),
            ))
        }),
        signature: None,
    }
}

fn index_func() -> Function {
    Function {
        name: "index".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("index expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }

            match &*args[0].data {
                ValueData::Array(list) => {
                    for (i, item) in list.iter().enumerate() {
                        if item == &args[1] {
                            return Ok(Value::new(
                                Type::Number,
                                ValueData::Number(Number::new(BigDecimal::from(i as u64))),
                            ));
                        }
                    }
                    Err("item not found".to_string())
                }
                ValueData::Object(map) => {
                    let key = match &*args[1].data {
                        ValueData::String(s) => s.as_str(),
                        _ => {
                            return Err("index with a map requires a string key".to_string());
                        }
                    };
                    for (i, k) in map.keys().enumerate() {
                        if k == key {
                            return Ok(Value::new(
                                Type::Number,
                                ValueData::Number(Number::new(BigDecimal::from(i as u64))),
                            ));
                        }
                    }
                    Err(format!("key {key:?} not found in map"))
                }
                _ => Err("index requires a list, tuple, or map".to_string()),
            }
        }),
        signature: None,
    }
}

fn one_func() -> Function {
    Function {
        name: "one".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("one expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                let inner_ty = match args[0].ty() {
                    Type::List(inner) | Type::Set(inner) => *inner.clone(),
                    Type::Tuple(elems) if !elems.is_empty() => elems[0].clone(),
                    _ => Type::Dynamic,
                };
                return Ok(Value::unknown(inner_ty));
            }
            if args[0].is_null() {
                return Ok(Value::null(Type::Dynamic));
            }

            match &*args[0].data {
                ValueData::Array(arr) => {
                    if arr.is_empty() {
                        let inner_ty = match args[0].ty() {
                            Type::List(inner) => *inner.clone(),
                            _ => Type::Dynamic,
                        };
                        Ok(Value::null(inner_ty))
                    } else if arr.len() == 1 {
                        Ok(arr[0].clone())
                    } else {
                        Err(
                            "one() cannot be applied to a collection with more than one element"
                                .to_string(),
                        )
                    }
                }
                ValueData::Set(set) => {
                    if set.is_empty() {
                        let inner_ty = match args[0].ty() {
                            Type::Set(inner) => *inner.clone(),
                            _ => Type::Dynamic,
                        };
                        Ok(Value::null(inner_ty))
                    } else if set.len() == 1 {
                        let first = set
                            .iter()
                            .fold(Value::null(Type::Dynamic), |_, v| v.clone());
                        Ok(first)
                    } else {
                        Err(
                            "one() cannot be applied to a collection with more than one element"
                                .to_string(),
                        )
                    }
                }
                _ => Err("one() requires a list, set, or tuple".to_string()),
            }
        }),
        signature: None,
    }
}

fn keys_func() -> Function {
    Function {
        name: "keys".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("keys expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::List(Box::new(Type::String))));
            }

            let ValueData::Object(map) = &*args[0].data else {
                return Err("keys requires a map".to_string());
            };

            let mut result = Vec::new();
            // maps are sorted by key in BTreeMap, which is correct for HCL keys()
            for k in map.keys() {
                result.push(Value::new(Type::String, ValueData::String(k.clone())));
            }
            Ok(Value::new(
                Type::List(Box::new(Type::String)),
                ValueData::Array(result),
            ))
        }),
        signature: None,
    }
}
fn length_func() -> Function {
    Function {
        name: "length".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("length expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                if let Some(r) = args[0].refinement() {
                    if let Some(len) = r
                        .string_length_min
                        .filter(|&min| r.string_length_max == Some(min))
                    {
                        return Ok(Value::new(
                            Type::Number,
                            ValueData::Number(Number::new(BigDecimal::from(len as u64))),
                        ));
                    }
                    if let Some(len) = r
                        .collection_length_min
                        .filter(|&min| r.collection_length_max == Some(min))
                    {
                        return Ok(Value::new(
                            Type::Number,
                            ValueData::Number(Number::new(BigDecimal::from(len as u64))),
                        ));
                    }
                    let mut num_ref = crate::types::refinement::Refinement::not_null();
                    if let Some(min) = r.string_length_min.or(r.collection_length_min) {
                        num_ref.number_min = Some(Number::new(BigDecimal::from(min as u64)));
                    }
                    if let Some(max) = r.string_length_max.or(r.collection_length_max) {
                        num_ref.number_max = Some(Number::new(BigDecimal::from(max as u64)));
                    }
                    return Ok(Value::unknown_refined(Type::Number, num_ref));
                }
                return Ok(Value::unknown(Type::Number));
            }

            let len = match &*args[0].data {
                ValueData::String(s) => s.chars().count(), // Length of string is number of chars (unicode)
                ValueData::Array(arr) => arr.len(),
                ValueData::Set(set) => set.len(),
                ValueData::Object(obj) => obj.len(),
                _ => return Err("length requires a string, list, set, or map".to_string()),
            };

            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(BigDecimal::from(len as u64))),
            ))
        }),
        signature: None,
    }
}

fn list_func() -> Function {
    Function {
        name: "list".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            // list(args...) converts args to a tuple, which typically unifies to a list.
            let mut result = Vec::new();
            let mut res_ty = None;
            for arg in args {
                result.push(arg.clone());
                match &res_ty {
                    None => res_ty = Some(arg.ty().clone()),
                    Some(current) => res_ty = unify(current, arg.ty()),
                }
            }
            let t = res_ty.unwrap_or(Type::Dynamic);
            Ok(Value::new(
                Type::List(Box::new(t)),
                ValueData::Array(result),
            ))
        }),
        signature: None,
    }
}

fn lookup_func() -> Function {
    Function {
        name: "lookup".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 && args.len() != 3 {
                return Err("lookup expects 2 or 3 arguments".to_string());
            }

            let map = match &*args[0].data {
                ValueData::Object(obj) => obj,
                ValueData::Unknown(_) => return Ok(Value::unknown(Type::Dynamic)),
                _ => return Err("lookup requires a map as first argument".to_string()),
            };

            if args[1].is_unknown() {
                return Ok(Value::unknown(Type::Dynamic));
            }
            let key = args[1]
                .clone()
                .coerce(&Type::String)
                .map_err(|_| "lookup key must be string".to_string())?;
            let key_str = match &*key.data {
                ValueData::String(s) => s.as_str(),
                _ => return Err("lookup key must be string".to_string()),
            };

            if let Some(val) = map.get(key_str) {
                Ok(val.clone())
            } else if args.len() == 3 {
                Ok(args[2].clone())
            } else {
                Err(format!("key '{key_str}' not found in map"))
            }
        }),
        signature: None,
    }
}

fn map_func() -> Function {
    Function {
        name: "map".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if !args.len().is_multiple_of(2) {
                return Err("map expects an even number of arguments".to_string());
            }

            let mut map = BTreeMap::new();
            let mut res_ty = None;

            for i in (0..args.len()).step_by(2) {
                let key = args[i]
                    .clone()
                    .coerce(&Type::String)
                    .map_err(|_| "map keys must be strings".to_string())?;
                let key_str = match &*key.data {
                    ValueData::String(s) => s.clone(),
                    _ => return Err("map keys must be strings".to_string()),
                };

                let val = args[i + 1].clone();
                match &res_ty {
                    None => res_ty = Some(val.ty().clone()),
                    Some(current) => res_ty = unify(current, val.ty()),
                }

                map.insert(key_str, val);
            }

            let t = res_ty.unwrap_or(Type::Dynamic);
            Ok(Value::new(Type::Map(Box::new(t)), ValueData::Object(map)))
        }),
        signature: None,
    }
}

fn matchkeys_func() -> Function {
    Function {
        name: "matchkeys".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 3 {
                return Err("matchkeys expects 3 arguments".to_string());
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::List(Box::new(Type::Dynamic))));
            }

            let ValueData::Array(values) = &*args[0].data else {
                return Err("matchkeys values must be list".to_string());
            };
            let ValueData::Array(keys) = &*args[1].data else {
                return Err("matchkeys keys must be list".to_string());
            };
            let ValueData::Array(searchset) = &*args[2].data else {
                return Err("matchkeys searchset must be list".to_string());
            };

            if values.len() != keys.len() {
                return Err("matchkeys values and keys must have same length".to_string());
            }

            let mut result = Vec::new();
            for (i, key) in keys.iter().enumerate() {
                if searchset.contains(key) {
                    result.push(values[i].clone());
                }
            }

            let inner_ty = if let Type::List(inner) = args[0].ty() {
                (**inner).clone()
            } else {
                Type::Dynamic
            };
            Ok(Value::new(
                Type::List(Box::new(inner_ty)),
                ValueData::Array(result),
            ))
        }),
        signature: None,
    }
}
fn merge_func() -> Function {
    Function {
        name: "merge".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.is_empty() {
                return Err("merge expects at least 1 argument".to_string());
            }

            let mut res_map = BTreeMap::new();
            let mut res_ty = None;

            for arg in args {
                if arg.is_unknown() {
                    return Ok(Value::unknown(Type::Map(Box::new(Type::Dynamic))));
                }

                if let ValueData::Object(obj) = &*arg.data {
                    for (k, v) in obj {
                        res_map.insert(k.clone(), v.clone());
                        match &res_ty {
                            None => res_ty = Some(v.ty().clone()),
                            Some(current) => res_ty = unify(current, v.ty()),
                        }
                    }
                } else {
                    return Err("merge arguments must be maps".to_string());
                }
            }

            let t = res_ty.unwrap_or(Type::Dynamic);
            Ok(Value::new(
                Type::Map(Box::new(t)),
                ValueData::Object(res_map),
            ))
        }),
        signature: None,
    }
}

fn reverse_func() -> Function {
    Function {
        name: "reverse".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("reverse expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(args[0].ty().clone()));
            }

            match &*args[0].data {
                ValueData::Array(arr) => {
                    let mut res = arr.clone();
                    res.reverse();
                    Ok(Value::new(args[0].ty().clone(), ValueData::Array(res)))
                }
                ValueData::String(s) => {
                    let res: String = s.chars().rev().collect();
                    Ok(Value::new(Type::String, ValueData::String(res)))
                }
                _ => Err("reverse requires a list or string".to_string()),
            }
        }),
        signature: None,
    }
}

fn setintersection_func() -> Function {
    Function {
        name: "setintersection".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.is_empty() {
                return Err("setintersection expects at least 1 argument".to_string());
            }

            let mut sets = Vec::new();
            for arg in args {
                if arg.is_unknown() {
                    return Ok(Value::unknown(Type::Set(Box::new(Type::Dynamic))));
                }
                match &*arg.data {
                    ValueData::Set(set) => sets.push(set.clone()),
                    ValueData::Array(arr) => {
                        let mut set = BTreeSet::new();
                        for item in arr {
                            set.insert(item.clone());
                        }
                        sets.push(set);
                    }
                    _ => return Err("setintersection arguments must be sets or lists".to_string()),
                }
            }

            let mut res_set = sets[0].clone();
            for set in sets.iter().skip(1) {
                res_set = res_set.intersection(set).cloned().collect();
            }

            let inner_ty = if let Type::Set(inner) = args[0].ty() {
                (**inner).clone()
            } else if let Type::List(inner) = args[0].ty() {
                (**inner).clone()
            } else {
                Type::Dynamic
            };
            Ok(Value::new(
                Type::Set(Box::new(inner_ty)),
                ValueData::Set(res_set),
            ))
        }),
        signature: None,
    }
}

fn setproduct_func() -> Function {
    Function {
        name: "setproduct".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() < 2 {
                return Err("setproduct expects at least 2 arguments".to_string());
            }

            let mut lists = Vec::new();
            for arg in args {
                if arg.is_unknown() {
                    return Ok(Value::unknown(Type::List(Box::new(Type::Tuple(vec![])))));
                }
                match &*arg.data {
                    ValueData::Set(set) => lists.push(set.iter().cloned().collect::<Vec<_>>()),
                    ValueData::Array(arr) => lists.push(arr.clone()),
                    _ => return Err("setproduct arguments must be sets or lists".to_string()),
                }
            }

            let mut result = Vec::new();
            let mut indices = vec![0; lists.len()];

            if lists.iter().all(|l| !l.is_empty()) {
                loop {
                    let mut tuple_els = Vec::new();
                    let mut ty_els = Vec::new();
                    for i in 0..lists.len() {
                        tuple_els.push(lists[i][indices[i]].clone());
                        ty_els.push(lists[i][indices[i]].ty().clone());
                    }
                    result.push(Value::new(Type::Tuple(ty_els), ValueData::Array(tuple_els)));

                    let mut pos = (lists.len() - 1) as isize;
                    while pos >= 0 {
                        indices[pos as usize] += 1;
                        if indices[pos as usize] < lists[pos as usize].len() {
                            break;
                        }
                        indices[pos as usize] = 0;
                        pos -= 1;
                    }
                    if pos < 0 {
                        break;
                    }
                }
            }

            let mut ty_els = Vec::new();
            for arg in args {
                if let Type::List(inner) = arg.ty() {
                    ty_els.push((**inner).clone());
                } else if let Type::Set(inner) = arg.ty() {
                    ty_els.push((**inner).clone());
                } else {
                    ty_els.push(Type::Dynamic);
                }
            }

            Ok(Value::new(
                Type::List(Box::new(Type::Tuple(ty_els))),
                ValueData::Array(result),
            ))
        }),
        signature: None,
    }
}
fn setsubtract_func() -> Function {
    Function {
        name: "setsubtract".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("setsubtract expects 2 arguments".to_string());
            }

            let mut sets = Vec::new();
            for arg in args {
                if arg.is_unknown() {
                    return Ok(Value::unknown(Type::Set(Box::new(Type::Dynamic))));
                }
                match &*arg.data {
                    ValueData::Set(set) => sets.push(set.clone()),
                    ValueData::Array(arr) => {
                        let mut set = BTreeSet::new();
                        for item in arr {
                            set.insert(item.clone());
                        }
                        sets.push(set);
                    }
                    _ => return Err("setsubtract arguments must be sets or lists".to_string()),
                }
            }

            let res_set: BTreeSet<_> = sets[0].difference(&sets[1]).cloned().collect();

            let inner_ty = if let Type::Set(inner) = args[0].ty() {
                (**inner).clone()
            } else if let Type::List(inner) = args[0].ty() {
                (**inner).clone()
            } else {
                Type::Dynamic
            };
            Ok(Value::new(
                Type::Set(Box::new(inner_ty)),
                ValueData::Set(res_set),
            ))
        }),
        signature: None,
    }
}

/// Built-in `setsymmetricdifference` function computing elements present in either set A or set B, but not both.
fn setsymmetricdifference_func() -> Function {
    Function {
        name: "setsymmetricdifference".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("setsymmetricdifference expects 2 arguments".to_string());
            }

            let mut combined_marks = BTreeSet::new();
            for arg in args {
                combined_marks.extend(arg.marks().clone());
            }

            let mut res_ty = None;
            for arg in args {
                let inner = if let Type::Set(inner) = arg.ty() {
                    (**inner).clone()
                } else if let Type::List(inner) = arg.ty() {
                    (**inner).clone()
                } else {
                    Type::Dynamic
                };
                match &res_ty {
                    None => res_ty = Some(inner),
                    Some(current) => res_ty = unify(current, &inner),
                }
            }
            let inner_ty = res_ty.unwrap_or(Type::Dynamic);

            for arg in args {
                if arg.is_unknown() {
                    return Ok(
                        Value::unknown(Type::Set(Box::new(inner_ty))).with_marks(combined_marks)
                    );
                }
            }

            let mut sets = Vec::new();
            for arg in args {
                match &*arg.data {
                    ValueData::Set(set) => sets.push(set.clone()),
                    ValueData::Array(arr) => {
                        let mut set = BTreeSet::new();
                        for item in arr {
                            set.insert(item.clone());
                        }
                        sets.push(set);
                    }
                    _ => {
                        return Err(
                            "setsymmetricdifference arguments must be sets or lists".to_string()
                        );
                    }
                }
            }

            let res_set: BTreeSet<_> = sets[0].symmetric_difference(&sets[1]).cloned().collect();

            Ok(Value::new_with_marks(
                Type::Set(Box::new(inner_ty)),
                ValueData::Set(res_set),
                combined_marks,
            ))
        }),
        signature: None,
    }
}

fn setunion_func() -> Function {
    Function {
        name: "setunion".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.is_empty() {
                return Err("setunion expects at least 1 argument".to_string());
            }

            let mut sets = Vec::new();
            for arg in args {
                if arg.is_unknown() {
                    return Ok(Value::unknown(Type::Set(Box::new(Type::Dynamic))));
                }
                match &*arg.data {
                    ValueData::Set(set) => sets.push(set.clone()),
                    ValueData::Array(arr) => {
                        let mut set = BTreeSet::new();
                        for item in arr {
                            set.insert(item.clone());
                        }
                        sets.push(set);
                    }
                    _ => return Err("setunion arguments must be sets or lists".to_string()),
                }
            }

            let mut res_set = sets[0].clone();
            for set in sets.iter().skip(1) {
                res_set = res_set.union(set).cloned().collect();
            }

            let mut res_ty = None;
            for arg in args {
                let inner = if let Type::Set(inner) = arg.ty() {
                    (**inner).clone()
                } else if let Type::List(inner) = arg.ty() {
                    (**inner).clone()
                } else {
                    Type::Dynamic
                };
                match &res_ty {
                    None => res_ty = Some(inner),
                    Some(current) => res_ty = unify(current, &inner),
                }
            }
            let inner_ty = res_ty.unwrap_or(Type::Dynamic);

            Ok(Value::new(
                Type::Set(Box::new(inner_ty)),
                ValueData::Set(res_set),
            ))
        }),
        signature: None,
    }
}

fn slice_func() -> Function {
    Function {
        name: "slice".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 3 {
                return Err("slice expects 3 arguments".to_string());
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::List(Box::new(Type::Dynamic))));
            }

            let ValueData::Array(list) = &*args[0].data else {
                return Err("slice first argument must be a list".to_string());
            };

            let start_val = args[1]
                .clone()
                .coerce(&Type::Number)
                .map_err(|_| "slice start index must be number".to_string())?;
            let start = match *start_val.data {
                ValueData::Number(n) => n.0.to_isize().ok_or("start index too large")?,
                _ => return Err("slice start index must be number".to_string()),
            };

            let end_val = args[2]
                .clone()
                .coerce(&Type::Number)
                .map_err(|_| "slice end index must be number".to_string())?;
            let end = match *end_val.data {
                ValueData::Number(n) => n.0.to_isize().ok_or("end index too large")?,
                _ => return Err("slice end index must be number".to_string()),
            };

            if start < 0 || start > list.len() as isize {
                return Err("slice start index out of bounds".to_string());
            }
            if end < start || end > list.len() as isize {
                return Err("slice end index out of bounds".to_string());
            }

            let res = list[start as usize..end as usize].to_vec();
            Ok(Value::new(args[0].ty().clone(), ValueData::Array(res)))
        }),
        signature: None,
    }
}
fn sort_func() -> Function {
    Function {
        name: "sort".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("sort expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::List(Box::new(Type::String))));
            }

            let ValueData::Array(list) = &*args[0].data else {
                return Err("sort requires a list".to_string());
            };

            let mut strings = Vec::new();
            for item in list {
                let s = item
                    .clone()
                    .coerce(&Type::String)
                    .map_err(|_| "sort requires list of strings".to_string())?;
                match &*s.data {
                    ValueData::String(str_val) => strings.push(str_val.clone()),
                    _ => return Err("sort requires list of strings".to_string()),
                }
            }

            strings.sort();
            let res = strings
                .into_iter()
                .map(|s| Value::new(Type::String, ValueData::String(s)))
                .collect();
            Ok(Value::new(
                Type::List(Box::new(Type::String)),
                ValueData::Array(res),
            ))
        }),
        signature: None,
    }
}

fn sum_func() -> Function {
    Function {
        name: "sum".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("sum expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }

            let items: Vec<Value> = match &*args[0].data {
                ValueData::Array(arr) => arr.clone(),
                ValueData::Set(set) => set.iter().cloned().collect(),
                _ => return Err("sum requires a list or set".to_string()),
            };

            let mut total = BigDecimal::from(0);
            for item in items {
                let num_val = item
                    .coerce(&Type::Number)
                    .map_err(|_| "sum requires list/set of numbers".to_string())?;
                match *num_val.data {
                    ValueData::Number(n) => total += n.0,
                    _ => return Err("sum requires list/set of numbers".to_string()),
                }
            }

            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(total)),
            ))
        }),
        signature: None,
    }
}

fn transpose_func() -> Function {
    Function {
        name: "transpose".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("transpose expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Map(Box::new(Type::List(Box::new(
                    Type::String,
                ))))));
            }

            let ValueData::Object(map) = &*args[0].data else {
                return Err("transpose requires a map".to_string());
            };

            let mut trans_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

            for (key, val) in map {
                if let ValueData::Array(arr) = &*val.data {
                    for item in arr {
                        let s = item.clone().coerce(&Type::String).map_err(|_| {
                            "transpose requires map of lists of strings".to_string()
                        })?;
                        if let ValueData::String(str_val) = &*s.data {
                            trans_map
                                .entry(str_val.clone())
                                .or_default()
                                .push(key.clone());
                        }
                    }
                } else {
                    return Err("transpose requires map of lists".to_string());
                }
            }

            let mut res_map = BTreeMap::new();
            for (k, mut v) in trans_map {
                v.sort(); // transpose returns lexicographically sorted lists
                let val_list: Vec<Value> = v
                    .into_iter()
                    .map(|s| Value::new(Type::String, ValueData::String(s)))
                    .collect();
                res_map.insert(
                    k,
                    Value::new(
                        Type::List(Box::new(Type::String)),
                        ValueData::Array(val_list),
                    ),
                );
            }

            Ok(Value::new(
                Type::Map(Box::new(Type::List(Box::new(Type::String)))),
                ValueData::Object(res_map),
            ))
        }),
        signature: None,
    }
}

fn values_func() -> Function {
    Function {
        name: "values".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("values expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::List(Box::new(Type::Dynamic))));
            }

            let ValueData::Object(map) = &*args[0].data else {
                return Err("values requires a map".to_string());
            };

            let mut result = Vec::new();
            let mut res_ty = None;
            // maps are sorted by key in BTreeMap, which is correct for HCL values()
            for v in map.values() {
                result.push(v.clone());
                match &res_ty {
                    None => res_ty = Some(v.ty().clone()),
                    Some(current) => res_ty = unify(current, v.ty()),
                }
            }

            let t = res_ty.unwrap_or(Type::Dynamic);
            Ok(Value::new(
                Type::List(Box::new(t)),
                ValueData::Array(result),
            ))
        }),
        signature: None,
    }
}

fn zipmap_func() -> Function {
    Function {
        name: "zipmap".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("zipmap expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Map(Box::new(Type::Dynamic))));
            }

            let ValueData::Array(keys) = &*args[0].data else {
                return Err("zipmap keys must be list".to_string());
            };
            let ValueData::Array(values) = &*args[1].data else {
                return Err("zipmap values must be list".to_string());
            };

            if keys.len() != values.len() {
                return Err("zipmap keys and values must have same length".to_string());
            }

            let mut map = BTreeMap::new();
            let mut res_ty = None;
            for (i, key) in keys.iter().enumerate() {
                let key_val = key
                    .clone()
                    .coerce(&Type::String)
                    .map_err(|_| "zipmap keys must be strings".to_string())?;
                match &*key_val.data {
                    ValueData::String(key_str) => {
                        map.insert(key_str.clone(), values[i].clone());

                        match &res_ty {
                            None => res_ty = Some(values[i].ty().clone()),
                            Some(current) => res_ty = unify(current, values[i].ty()),
                        }
                    }
                    _ => return Err("zipmap keys must be strings".to_string()),
                }
            }

            let t = res_ty.unwrap_or(Type::Dynamic);
            Ok(Value::new(Type::Map(Box::new(t)), ValueData::Object(map)))
        }),
        signature: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::number::Number;
    use bigdecimal::BigDecimal;
    use std::str::FromStr;

    fn get_func(name: &str) -> Function {
        functions()
            .into_iter()
            .find(|f| f.name == name)
            .expect("expected value")
    }

    fn eval_func(name: &str, args: &[Value]) -> Result<Value, String> {
        let f = get_func(name);
        (f.func)(args)
    }

    fn str_val(s: &str) -> Value {
        Value::new(Type::String, ValueData::String(s.to_string()))
    }

    fn bool_val(b: bool) -> Value {
        Value::new(Type::Bool, ValueData::Bool(b))
    }

    fn num_val(s: &str) -> Value {
        Value::new(
            Type::Number,
            ValueData::Number(Number::new(
                BigDecimal::from_str(s).expect("expected value"),
            )),
        )
    }

    fn unk_val() -> Value {
        Value::unknown(Type::Dynamic)
    }

    fn null_val() -> Value {
        Value::new(Type::Dynamic, ValueData::Null)
    }

    fn list_val(vals: Vec<Value>) -> Value {
        Value::new(Type::List(Box::new(Type::Dynamic)), ValueData::Array(vals))
    }

    fn set_val(vals: Vec<Value>) -> Value {
        let mut set = BTreeSet::new();
        for v in vals {
            set.insert(v);
        }
        Value::new(Type::Set(Box::new(Type::Dynamic)), ValueData::Set(set))
    }

    fn map_val(vals: Vec<(&str, Value)>) -> Value {
        let mut map = BTreeMap::new();
        for (k, v) in vals {
            map.insert(k.to_string(), v);
        }
        Value::new(Type::Map(Box::new(Type::Dynamic)), ValueData::Object(map))
    }

    fn get_arr_len(val: &Value) -> usize {
        match &*val.data {
            ValueData::Array(arr) => arr.len(),
            _ => 0,
        }
    }

    fn get_obj_len(val: &Value) -> usize {
        match &*val.data {
            ValueData::Object(obj) => obj.len(),
            _ => 0,
        }
    }

    #[test]
    fn test_alltrue_anytrue() {
        assert_eq!(
            eval_func("alltrue", &[list_val(vec![bool_val(true), bool_val(true)])])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::Bool(true)
        );
        assert_eq!(
            eval_func(
                "alltrue",
                &[list_val(vec![bool_val(true), bool_val(false)])]
            )
            .expect("expected value")
            .data
            .as_ref(),
            &ValueData::Bool(false)
        );

        assert_eq!(
            eval_func(
                "anytrue",
                &[list_val(vec![bool_val(true), bool_val(false)])]
            )
            .expect("expected value")
            .data
            .as_ref(),
            &ValueData::Bool(true)
        );
        assert_eq!(
            eval_func(
                "anytrue",
                &[list_val(vec![bool_val(false), bool_val(false)])]
            )
            .expect("expected value")
            .data
            .as_ref(),
            &ValueData::Bool(false)
        );

        assert!(eval_func("alltrue", &[]).is_err());
        assert!(
            eval_func("alltrue", &[unk_val()])
                .expect("expected value")
                .is_unknown()
        );
        assert!(
            eval_func("alltrue", &[list_val(vec![unk_val()])])
                .expect("expected value")
                .is_unknown()
        );
        assert!(eval_func("alltrue", &[list_val(vec![str_val("bad")])]).is_err());
    }

    #[test]
    fn test_anytrue_errors() {
        assert!(eval_func("anytrue", &[]).is_err());
        assert!(
            eval_func("anytrue", &[unk_val()])
                .expect("expected value")
                .is_unknown()
        );
        assert!(
            eval_func("anytrue", &[list_val(vec![unk_val()])])
                .expect("expected value")
                .is_unknown()
        );
        assert!(eval_func("anytrue", &[list_val(vec![str_val("bad")])]).is_err());
    }

    #[test]
    fn test_contains_set() {
        let mut set = std::collections::BTreeSet::new();
        set.insert(str_val("a"));
        let set_val = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set));
        assert_eq!(
            eval_func("contains", &[set_val, str_val("a")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::Bool(true)
        );
    }

    #[test]
    fn test_chunklist_errors() {
        assert!(eval_func("chunklist", &[str_val("a"), num_val("2")]).is_err());
        assert!(
            eval_func("chunklist", &[list_val(vec![]), unk_val()])
                .expect("expected value")
                .is_unknown()
        );
        assert!(eval_func("chunklist", &[list_val(vec![]), str_val("bad")]).is_err());
    }

    #[test]
    fn test_chunklist() {
        let res = eval_func(
            "chunklist",
            &[
                list_val(vec![str_val("a"), str_val("b"), str_val("c")]),
                num_val("2"),
            ],
        )
        .expect("expected value");
        assert_eq!(get_arr_len(&res), 2);
        assert!(eval_func("chunklist", &[]).is_err());
        assert!(eval_func("chunklist", &[list_val(vec![]), num_val("2")]).is_ok());
        assert!(eval_func("chunklist", &[list_val(vec![str_val("a")]), num_val("-1")]).is_err());
        assert!(
            eval_func("chunklist", &[unk_val(), num_val("2")])
                .expect("expected value")
                .is_unknown()
        );
    }

    #[test]
    fn test_coalesce_coalescelist() {
        assert_eq!(
            eval_func("coalesce", &[null_val(), str_val("a")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("a".to_string())
        );
        assert!(eval_func("coalesce", &[null_val(), null_val()]).is_err());
        assert!(eval_func("coalesce", &[]).is_err());

        assert_eq!(
            eval_func(
                "coalescelist",
                &[list_val(vec![]), list_val(vec![str_val("a")])]
            )
            .expect("expected value")
            .data
            .as_ref(),
            &ValueData::Array(vec![str_val("a")])
        );
        assert!(eval_func("coalescelist", &[list_val(vec![]), list_val(vec![])]).is_err());
        assert!(eval_func("coalescelist", &[]).is_err());
    }

    #[test]
    fn test_compact() {
        let res = eval_func("compact", &[list_val(vec![null_val(), str_val("a")])])
            .expect("expected value");
        assert_eq!(get_arr_len(&res), 1);
        assert!(eval_func("compact", &[]).is_err());
        assert!(eval_func("compact", &[str_val("bad")]).is_err());
    }

    #[test]
    fn test_concat() {
        let res = eval_func(
            "concat",
            &[list_val(vec![str_val("a")]), list_val(vec![str_val("b")])],
        )
        .expect("expected value");
        assert_eq!(get_arr_len(&res), 2);
        assert!(eval_func("concat", &[list_val(vec![str_val("a")]), str_val("bad")]).is_err());
    }

    #[test]
    fn test_contains() {
        assert_eq!(
            eval_func("contains", &[list_val(vec![str_val("a")]), str_val("a")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::Bool(true)
        );
        assert_eq!(
            eval_func("contains", &[list_val(vec![str_val("a")]), str_val("b")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::Bool(false)
        );
        assert!(eval_func("contains", &[]).is_err());
        assert!(eval_func("contains", &[str_val("bad"), str_val("a")]).is_err());
    }

    #[test]
    fn test_distinct() {
        let res = eval_func("distinct", &[list_val(vec![str_val("a"), str_val("a")])])
            .expect("expected value");
        assert_eq!(get_arr_len(&res), 1);
        assert!(eval_func("distinct", &[]).is_err());
    }

    #[test]
    fn test_element() {
        assert_eq!(
            eval_func(
                "element",
                &[list_val(vec![str_val("a"), str_val("b")]), num_val("1")]
            )
            .expect("expected value")
            .data
            .as_ref(),
            &ValueData::String("b".to_string())
        );
        assert_eq!(
            eval_func("element", &[list_val(vec![str_val("a")]), num_val("1")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("a".to_string())
        ); // wraps around
        assert!(eval_func("element", &[]).is_err());
        assert!(eval_func("element", &[list_val(vec![]), num_val("0")]).is_err());
    }

    #[test]
    fn test_flatten() {
        let res = eval_func("flatten", &[list_val(vec![list_val(vec![str_val("a")])])])
            .expect("expected value");
        assert_eq!(get_arr_len(&res), 1);
        assert!(eval_func("flatten", &[]).is_err());
    }

    #[test]
    fn test_index() {
        assert_eq!(
            eval_func(
                "index",
                &[list_val(vec![str_val("a"), str_val("b")]), str_val("b")]
            )
            .expect("expected value")
            .data
            .as_ref(),
            &ValueData::Number(Number::new(
                BigDecimal::from_str("1").expect("expected value")
            ))
        );
        assert!(eval_func("index", &[list_val(vec![str_val("a")]), str_val("b")]).is_err());
        assert!(eval_func("index", &[]).is_err());
    }

    #[test]
    fn test_keys_values() {
        let map = map_val(vec![("a", str_val("1")), ("b", str_val("2"))]);
        let k = eval_func("keys", std::slice::from_ref(&map)).expect("expected value");
        assert_eq!(get_arr_len(&k), 2);
        let v = eval_func("values", &[map]).expect("expected value");
        assert_eq!(get_arr_len(&v), 2);
        assert!(eval_func("keys", &[]).is_err());
        assert!(eval_func("values", &[]).is_err());
    }

    #[test]
    fn test_length() {
        assert_eq!(
            eval_func("length", &[list_val(vec![str_val("a"), str_val("b")])])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::Number(Number::new(
                BigDecimal::from_str("2").expect("expected value")
            ))
        );
        assert_eq!(
            eval_func("length", &[str_val("abc")])
                .expect("expected value")
                .data
                .as_ref(),
            num_val("3").data.as_ref()
        );
        assert!(eval_func("length", &[]).is_err());
    }

    #[test]
    fn test_list_map() {
        assert!(eval_func("list", &[str_val("a"), str_val("b")]).is_ok());
        assert!(eval_func("map", &[str_val("k1"), str_val("v1")]).is_ok());
        assert!(eval_func("map", &[str_val("k1")]).is_err()); // uneven
    }

    #[test]
    fn test_lookup() {
        let map = map_val(vec![("a", str_val("1"))]);
        assert_eq!(
            eval_func("lookup", &[map.clone(), str_val("a")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("1".to_string())
        );
        assert_eq!(
            eval_func("lookup", &[map.clone(), str_val("b"), str_val("def")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("def".to_string())
        );
        assert!(eval_func("lookup", &[map, str_val("b")]).is_err());
        assert!(eval_func("lookup", &[]).is_err());
    }

    #[test]
    fn test_matchkeys() {
        let res = eval_func(
            "matchkeys",
            &[
                list_val(vec![str_val("v1"), str_val("v2")]),
                list_val(vec![str_val("k1"), str_val("k2")]),
                list_val(vec![str_val("k1")]),
            ],
        )
        .expect("expected value");
        assert_eq!(get_arr_len(&res), 1);
        assert!(eval_func("matchkeys", &[]).is_err());
    }

    #[test]
    fn test_merge() {
        let m1 = map_val(vec![("a", str_val("1"))]);
        let m2 = map_val(vec![("b", str_val("2"))]);
        let res = eval_func("merge", &[m1, m2]).expect("expected value");
        assert_eq!(get_obj_len(&res), 2);
        assert!(eval_func("merge", &[str_val("bad")]).is_err());
    }

    #[test]
    fn test_reverse() {
        let res = eval_func("reverse", &[list_val(vec![str_val("a"), str_val("b")])])
            .expect("expected value");
        assert_eq!(
            *res.data,
            ValueData::Array(vec![str_val("b"), str_val("a")])
        );
        assert!(eval_func("reverse", &[]).is_err());
    }

    #[test]
    fn test_set_math() {
        let s1 = set_val(vec![str_val("a"), str_val("b")]);
        let s2 = set_val(vec![str_val("b"), str_val("c")]);

        let union = eval_func("setunion", &[s1.clone(), s2.clone()]).expect("expected value");
        assert_eq!(
            std::mem::discriminant(&*union.data),
            std::mem::discriminant(&ValueData::Set(BTreeSet::new()))
        );

        let int = eval_func("setintersection", &[s1.clone(), s2.clone()]).expect("expected value");
        assert_eq!(
            std::mem::discriminant(&*int.data),
            std::mem::discriminant(&ValueData::Set(BTreeSet::new()))
        );

        let sub = eval_func("setsubtract", &[s1.clone(), s2.clone()]).expect("expected value");
        assert_eq!(
            std::mem::discriminant(&*sub.data),
            std::mem::discriminant(&ValueData::Set(BTreeSet::new()))
        );

        let sym_diff =
            eval_func("setsymmetricdifference", &[s1.clone(), s2.clone()]).expect("expected value");
        assert_eq!(
            std::mem::discriminant(&*sym_diff.data),
            std::mem::discriminant(&ValueData::Set(BTreeSet::new()))
        );

        let prod = eval_func("setproduct", &[s1, s2]).expect("expected value");
        assert_eq!(
            std::mem::discriminant(&*prod.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );
    }

    #[test]
    fn test_slice() {
        let arr = list_val(vec![str_val("a"), str_val("b"), str_val("c")]);
        let sl = eval_func("slice", &[arr, num_val("1"), num_val("2")]).expect("expected value");
        assert_eq!(get_arr_len(&sl), 1);
        assert!(eval_func("slice", &[]).is_err());
    }

    #[test]
    fn test_sort() {
        let res = eval_func("sort", &[list_val(vec![str_val("b"), str_val("a")])])
            .expect("expected value");
        assert_eq!(
            *res.data,
            ValueData::Array(vec![str_val("a"), str_val("b")])
        );
        assert!(eval_func("sort", &[]).is_err());
    }

    #[test]
    fn test_sum() {
        assert_eq!(
            eval_func("sum", &[list_val(vec![num_val("1"), num_val("2")])])
                .expect("expected value")
                .data
                .as_ref(),
            num_val("3").data.as_ref()
        );
        assert!(eval_func("sum", &[]).is_err());
    }

    #[test]
    fn test_transpose() {
        let map = map_val(vec![("a", list_val(vec![str_val("1")]))]);
        let res = eval_func("transpose", &[map]).expect("expected value");
        assert_eq!(get_obj_len(&res), 1);
        assert!(eval_func("transpose", &[]).is_err());
    }

    #[test]
    fn test_zipmap() {
        let res = eval_func(
            "zipmap",
            &[list_val(vec![str_val("a")]), list_val(vec![str_val("1")])],
        )
        .expect("expected value");
        assert_eq!(get_obj_len(&res), 1);
        assert!(eval_func("zipmap", &[]).is_err());
        assert!(eval_func("zipmap", &[list_val(vec![str_val("a")]), list_val(vec![])]).is_err()); // length mismatch
    }

    #[test]
    fn test_coverage_gaps_collection() {
        let unk = unk_val();
        let unk_bool = Value::unknown(Type::Bool);
        let unk_list = Value::unknown(Type::List(Box::new(Type::Bool)));
        let set_bool = Value::new(
            Type::Set(Box::new(Type::Bool)),
            ValueData::Set(
                vec![Value::new(Type::Bool, ValueData::Bool(true))]
                    .into_iter()
                    .collect(),
            ),
        );

        // alltrue
        assert!(
            eval_func("alltrue", std::slice::from_ref(&unk_bool)).is_ok_and(|v| v.is_unknown())
        );
        assert!(
            eval_func("alltrue", &[list_val(vec![unk_bool.clone()])]).is_ok_and(|v| v.is_unknown())
        );
        assert!(eval_func("alltrue", std::slice::from_ref(&set_bool)).is_err());
        assert!(eval_func("alltrue", &[str_val("notlist")]).is_err());

        // anytrue
        assert!(
            eval_func("anytrue", std::slice::from_ref(&unk_bool)).is_ok_and(|v| v.is_unknown())
        );
        assert!(
            eval_func("anytrue", &[list_val(vec![unk_bool.clone()])]).is_ok_and(|v| v.is_unknown())
        );
        assert!(eval_func("anytrue", &[str_val("notlist")]).is_err());

        // chunklist
        assert!(
            eval_func("chunklist", &[unk_list.clone(), num_val("2")]).is_ok_and(|v| v.is_unknown())
        );
        assert!(eval_func("chunklist", &[list_val(vec![]), num_val("0")]).is_err());
        assert!(eval_func("chunklist", &[unk.clone(), num_val("2")]).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("chunklist", &[list_val(vec![]), num_val("2")]).is_ok_and(|v| v.ty() == &Type::List(Box::new(Type::List(Box::new(Type::Dynamic))))));

        // coalesce
        assert!(eval_func("coalesce", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("coalesce", &[str_val("")]).is_err());

        // coalescelist
        assert!(
            eval_func("coalescelist", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown())
        );

        // compact
        assert!(eval_func("compact", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));

        // concat
        assert!(eval_func("concat", &[]).is_err());
        assert!(eval_func("concat", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(
            eval_func(
                "concat",
                &[list_val(vec![str_val("a")]), list_val(vec![num_val("1")])]
            )
            .is_ok()
        );

        // contains
        assert!(eval_func("contains", &[unk.clone(), str_val("a")]).is_ok_and(|v| v.is_unknown()));

        // distinct
        assert!(eval_func("distinct", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("distinct", &[str_val("a")]).is_err());

        // element
        assert!(
            eval_func("element", &[unk_list.clone(), num_val("0")]).is_ok_and(|v| v.is_unknown())
        );
        assert!(eval_func("element", &[unk.clone(), num_val("0")]).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("element", &[str_val("a"), num_val("0")]).is_err());
        assert!(
            eval_func("element", &[list_val(vec![str_val("a")]), num_val("-1")])
                .is_ok_and(|v| v.data.as_ref() == &ValueData::String("a".to_string()))
        );

        // flatten
        assert!(eval_func("flatten", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("flatten", &[str_val("a")]).is_err());
        assert!(eval_func("flatten", &[list_val(vec![set_val(vec![str_val("a")])])]).is_ok());
        assert!(
            eval_func(
                "flatten",
                &[list_val(vec![
                    list_val(vec![str_val("a")]),
                    list_val(vec![num_val("1")])
                ])]
            )
            .is_ok()
        );

        // index
        assert!(eval_func("index", &[unk.clone(), str_val("a")]).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("index", &[str_val("a"), str_val("a")]).is_err());

        // keys
        assert!(eval_func("keys", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("keys", &[str_val("a")]).is_err());

        // length
        assert!(eval_func("length", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(
            eval_func("length", &[set_val(vec![str_val("a")])])
                .is_ok_and(|v| v.data.as_ref() == num_val("1").data.as_ref())
        );
        assert!(
            eval_func("length", &[map_val(vec![("a", str_val("b"))])])
                .is_ok_and(|v| v.data.as_ref() == num_val("1").data.as_ref())
        );
        assert!(eval_func("length", &[num_val("1")]).is_err());

        // lookup
        assert!(eval_func("lookup", &[unk.clone(), str_val("a")]).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("lookup", &[str_val("a"), str_val("b")]).is_err());
        assert!(eval_func("lookup", &[map_val(vec![]), unk.clone()]).is_ok_and(|v| v.is_unknown()));

        // map
        assert!(
            eval_func(
                "map",
                &[str_val("a"), str_val("b"), str_val("c"), num_val("1")]
            )
            .is_ok()
        );

        // matchkeys
        assert!(
            eval_func("matchkeys", &[unk.clone(), unk.clone(), unk.clone()])
                .is_ok_and(|v| v.is_unknown())
        );
        assert!(
            eval_func(
                "matchkeys",
                &[str_val("a"), list_val(vec![]), list_val(vec![])]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "matchkeys",
                &[list_val(vec![]), str_val("a"), list_val(vec![])]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "matchkeys",
                &[list_val(vec![]), list_val(vec![]), str_val("a")]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "matchkeys",
                &[
                    list_val(vec![str_val("a")]),
                    list_val(vec![]),
                    list_val(vec![])
                ]
            )
            .is_err()
        );

        // merge
        assert!(eval_func("merge", &[]).is_err());
        assert!(eval_func("merge", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));

        // reverse
        assert!(eval_func("reverse", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(
            eval_func("reverse", &[str_val("abc")])
                .is_ok_and(|v| v.data.as_ref() == &ValueData::String("cba".to_string()))
        );
        assert!(eval_func("reverse", &[num_val("1")]).is_err());

        // setintersection
        assert!(eval_func("setintersection", &[]).is_err());
        assert!(
            eval_func("setintersection", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown())
        );
        assert!(eval_func("setintersection", &[str_val("a")]).is_err());
        assert!(eval_func("setintersection", &[list_val(vec![str_val("a")])]).is_ok());

        // setproduct
        assert!(eval_func("setproduct", &[list_val(vec![])]).is_err()); // less than 2 args
        assert!(eval_func("setproduct", &[unk.clone(), unk.clone()]).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("setproduct", &[str_val("a"), str_val("b")]).is_err());
        assert!(
            eval_func(
                "setproduct",
                &[list_val(vec![str_val("a")]), list_val(vec![str_val("b")])]
            )
            .is_ok()
        );
        assert!(
            eval_func(
                "setproduct",
                &[
                    Value::new(Type::Dynamic, ValueData::Array(vec![str_val("a")])),
                    Value::new(Type::Dynamic, ValueData::Array(vec![str_val("b")]))
                ]
            )
            .is_ok()
        );

        // setsubtract
        assert!(eval_func("setsubtract", &[]).is_err());
        assert!(
            eval_func("setsubtract", &[unk.clone(), unk.clone()]).is_ok_and(|v| v.is_unknown())
        );
        assert!(eval_func("setsubtract", &[str_val("a"), str_val("b")]).is_err());
        assert!(
            eval_func(
                "setsubtract",
                &[list_val(vec![str_val("a")]), list_val(vec![str_val("b")])]
            )
            .is_ok()
        );
        assert!(
            eval_func(
                "setsubtract",
                &[
                    Value::new(Type::Dynamic, ValueData::Array(vec![str_val("a")])),
                    Value::new(Type::Dynamic, ValueData::Array(vec![str_val("b")]))
                ]
            )
            .is_ok()
        );

        // setunion
        assert!(eval_func("setunion", &[]).is_err());
        assert!(eval_func("setunion", &[unk.clone(), unk.clone()]).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("setunion", &[str_val("a"), str_val("b")]).is_err());
        assert!(
            eval_func(
                "setunion",
                &[list_val(vec![str_val("a")]), list_val(vec![str_val("b")])]
            )
            .is_ok()
        );
        assert!(
            eval_func(
                "setunion",
                &[
                    Value::new(Type::Dynamic, ValueData::Array(vec![str_val("a")])),
                    Value::new(Type::Dynamic, ValueData::Array(vec![str_val("b")]))
                ]
            )
            .is_ok()
        );

        // slice
        assert!(
            eval_func("slice", &[unk.clone(), num_val("0"), num_val("1")])
                .is_ok_and(|v| v.is_unknown())
        );
        assert!(eval_func("slice", &[str_val("a"), num_val("0"), num_val("1")]).is_err());
        assert!(eval_func("slice", &[list_val(vec![]), num_val("-1"), num_val("1")]).is_err());
        assert!(eval_func("slice", &[list_val(vec![]), num_val("0"), num_val("1")]).is_err()); // end index out of bounds

        // sort
        assert!(eval_func("sort", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("sort", &[str_val("a")]).is_err());

        // sum
        assert!(eval_func("sum", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(
            eval_func("sum", &[set_val(vec![num_val("1")])])
                .is_ok_and(|v| v.data.as_ref() == num_val("1").data.as_ref())
        );
        assert!(eval_func("sum", &[str_val("a")]).is_err());

        // transpose
        assert!(eval_func("transpose", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("transpose", &[str_val("a")]).is_err());
        assert!(eval_func("transpose", &[map_val(vec![("a", str_val("b"))])]).is_err());
        assert!(
            eval_func(
                "transpose",
                &[map_val(vec![("a", list_val(vec![num_val("1")]))])]
            )
            .is_ok()
        );
        assert!(
            eval_func(
                "transpose",
                &[map_val(vec![("a", list_val(vec![list_val(vec![])]))])]
            )
            .is_err()
        );

        // values
        assert!(eval_func("values", std::slice::from_ref(&unk)).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("values", &[str_val("a")]).is_err());

        // zipmap
        assert!(eval_func("zipmap", &[unk.clone(), unk.clone()]).is_ok_and(|v| v.is_unknown()));
        assert!(eval_func("zipmap", &[str_val("a"), list_val(vec![])]).is_err());
        assert!(eval_func("zipmap", &[list_val(vec![]), str_val("a")]).is_err());
        assert!(
            eval_func(
                "zipmap",
                &[
                    list_val(vec![str_val("a"), str_val("b")]),
                    list_val(vec![str_val("a"), num_val("1")])
                ]
            )
            .is_ok()
        );
    }

    fn tuple_val(vals: Vec<Value>) -> Value {
        let tys = vals.iter().map(Value::ty).cloned().collect();
        Value::new(Type::Tuple(tys), ValueData::Array(vals))
    }

    #[test]
    fn test_collection_comprehensive_coverage() {
        let tup = tuple_val(vec![num_val("1"), num_val("2"), num_val("3")]);

        // 1. chunklist with tuple
        let cl = eval_func("chunklist", &[tup.clone(), num_val("2")]).expect("expected chunklist");
        assert_eq!(
            cl.ty(),
            &Type::List(Box::new(Type::List(Box::new(Type::Dynamic))))
        );

        // 2. element with negative index
        let el = eval_func(
            "element",
            &[
                list_val(vec![str_val("a"), str_val("b"), str_val("c")]),
                num_val("-1"),
            ],
        )
        .expect("expected element");
        assert_eq!(*el.data, ValueData::String("c".to_string()));

        // 3. matchkeys with tuple
        let mk = eval_func(
            "matchkeys",
            &[
                tup.clone(),
                list_val(vec![str_val("k1"), str_val("k2"), str_val("k3")]),
                list_val(vec![str_val("k2")]),
            ],
        )
        .expect("expected matchkeys");
        assert_eq!(mk.ty(), &Type::List(Box::new(Type::Dynamic)));

        // 4. setintersection with tuple
        let si = eval_func(
            "setintersection",
            &[tup, list_val(vec![num_val("2"), num_val("4")])],
        )
        .expect("expected setintersection");
        assert_eq!(si.ty(), &Type::Set(Box::new(Type::Dynamic)));

        // 5. setproduct with multi-element lists to trigger inner index increment
        let sp = eval_func(
            "setproduct",
            &[
                list_val(vec![str_val("a"), str_val("b")]),
                list_val(vec![num_val("1"), num_val("2")]),
            ],
        )
        .expect("expected setproduct");
        assert_eq!(get_arr_len(&sp), 4);

        // 6. reverse with empty list
        let rev = eval_func("reverse", &[list_val(vec![])]).expect("expected reverse");
        assert_eq!(get_arr_len(&rev), 0);

        // 7. lookup with default value
        let lk = eval_func(
            "lookup",
            &[
                map_val(vec![("a", str_val("1"))]),
                str_val("missing"),
                str_val("default_val"),
            ],
        )
        .expect("expected lookup");
        assert_eq!(*lk.data, ValueData::String("default_val".to_string()));

        // 8. Coercion failure tests for map_err closures
        let bad_type = tuple_val(vec![]);
        assert!(
            eval_func(
                "chunklist",
                &[list_val(vec![num_val("1")]), bad_type.clone()]
            )
            .is_err()
        );
        assert!(eval_func("element", &[list_val(vec![num_val("1")]), bad_type.clone()]).is_err());
        assert!(
            eval_func(
                "slice",
                &[list_val(vec![num_val("1")]), bad_type.clone(), num_val("1")]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "slice",
                &[list_val(vec![num_val("1")]), num_val("0"), bad_type.clone()]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "lookup",
                &[map_val(vec![("a", str_val("1"))]), bad_type.clone()]
            )
            .is_err()
        );
        assert!(eval_func("map", &[bad_type.clone(), str_val("val")]).is_err());
        assert!(eval_func("sort", &[list_val(vec![bad_type.clone()])]).is_err());
        assert!(eval_func("sum", &[list_val(vec![bad_type.clone()])]).is_err());
        assert!(
            eval_func(
                "zipmap",
                &[
                    list_val(vec![bad_type.clone()]),
                    list_val(vec![num_val("1")])
                ]
            )
            .is_err()
        );
        assert!(eval_func("compact", &[list_val(vec![bad_type.clone()])]).is_err());
        assert!(
            eval_func(
                "transpose",
                &[map_val(vec![("k", list_val(vec![bad_type]))])]
            )
            .is_err()
        );

        // 9. Huge number tests
        let huge = num_val("9999999999999999999999999999999999999999");
        assert!(eval_func("chunklist", &[list_val(vec![num_val("1")]), huge.clone()]).is_err());
        assert!(eval_func("element", &[list_val(vec![num_val("1")]), huge.clone()]).is_err());
        assert!(
            eval_func(
                "slice",
                &[list_val(vec![num_val("1")]), huge.clone(), num_val("1")]
            )
            .is_err()
        );
        assert!(eval_func("slice", &[list_val(vec![num_val("1")]), num_val("0"), huge]).is_err());

        // 10. Null typed data tests for _ => Err arms after coerce
        let null_num = Value::new(Type::Number, ValueData::Null);
        let null_str = Value::new(Type::String, ValueData::Null);
        assert!(
            eval_func(
                "chunklist",
                &[list_val(vec![num_val("1")]), null_num.clone()]
            )
            .is_err()
        );
        assert!(eval_func("element", &[list_val(vec![num_val("1")]), null_num.clone()]).is_err());
        assert!(
            eval_func(
                "slice",
                &[list_val(vec![num_val("1")]), null_num.clone(), num_val("1")]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "slice",
                &[list_val(vec![num_val("1")]), num_val("0"), null_num.clone()]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "lookup",
                &[map_val(vec![("a", str_val("1"))]), null_str.clone()]
            )
            .is_err()
        );
        assert!(eval_func("map", &[null_str.clone(), str_val("val")]).is_err());
        assert!(eval_func("sort", &[list_val(vec![null_str.clone()])]).is_err());
        assert!(eval_func("sum", &[list_val(vec![null_num])]).is_err());
        assert!(
            eval_func(
                "zipmap",
                &[
                    list_val(vec![null_str.clone()]),
                    list_val(vec![num_val("1")])
                ]
            )
            .is_err()
        );

        // 11. Concat with tuple and setproduct with empty list
        let cc = eval_func(
            "concat",
            &[list_val(vec![num_val("1")]), tuple_val(vec![num_val("2")])],
        )
        .expect("expected concat");
        assert_eq!(get_arr_len(&cc), 2);

        let spe = eval_func(
            "setproduct",
            &[list_val(vec![num_val("1")]), list_val(vec![])],
        )
        .expect("expected setproduct");
        assert_eq!(get_arr_len(&spe), 0);

        // 12. args[0] and args[1] unknown tests for contains, element, index, zipmap
        assert!(eval_func("contains", &[unk_val(), str_val("x")]).is_ok_and(|v| v.is_unknown()));
        assert!(
            eval_func("contains", &[list_val(vec![str_val("a")]), unk_val()])
                .is_ok_and(|v| v.is_unknown())
        );
        assert!(eval_func("element", &[unk_val(), num_val("0")]).is_ok_and(|v| v.is_unknown()));
        assert!(
            eval_func("element", &[list_val(vec![num_val("1")]), unk_val()])
                .is_ok_and(|v| v.is_unknown())
        );
        assert!(eval_func("index", &[unk_val(), str_val("x")]).is_ok_and(|v| v.is_unknown()));
        assert!(
            eval_func("index", &[list_val(vec![str_val("a")]), unk_val()])
                .is_ok_and(|v| v.is_unknown())
        );
        assert!(
            eval_func("zipmap", &[unk_val(), list_val(vec![str_val("x")])])
                .is_ok_and(|v| v.is_unknown())
        );
        assert!(
            eval_func("zipmap", &[list_val(vec![str_val("a")]), unk_val()])
                .is_ok_and(|v| v.is_unknown())
        );

        // 13. coalesce with empty strings, null_str, and non-string fallbacks
        let coal_res =
            eval_func("coalesce", &[str_val(""), num_val("10")]).expect("expected coalesce");
        assert_eq!(*coal_res.data, *num_val("10").data);
        let coal_res2 =
            eval_func("coalesce", &[str_val(""), str_val("hello")]).expect("expected coalesce");
        assert_eq!(*coal_res2.data, *str_val("hello").data);
        let coal_res3 = eval_func("coalesce", &[null_str.clone(), str_val("fallback")])
            .expect("expected coalesce");
        assert_eq!(*coal_res3.data, *str_val("fallback").data);

        // 14. coalescelist with empty list and non-list error
        let cl_res = eval_func(
            "coalescelist",
            &[list_val(vec![]), list_val(vec![num_val("1")])],
        )
        .expect("expected coalescelist");
        assert_eq!(get_arr_len(&cl_res), 1);
        assert!(eval_func("coalescelist", &[num_val("10")]).is_err());

        // 15. compact with empty string and null_str
        let comp_res = eval_func("compact", &[list_val(vec![str_val(""), str_val("a")])])
            .expect("expected compact");
        assert_eq!(get_arr_len(&comp_res), 1);
        let comp_null =
            eval_func("compact", &[list_val(vec![null_str.clone()])]).expect("expected compact");
        assert_eq!(get_arr_len(&comp_null), 0);

        // 16. slice with start < 0, start > len, end < start, and end > len
        assert!(
            eval_func(
                "slice",
                &[list_val(vec![num_val("1")]), num_val("-1"), num_val("1")]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "slice",
                &[list_val(vec![num_val("1")]), num_val("5"), num_val("5")]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "slice",
                &[
                    list_val(vec![num_val("1"), num_val("2")]),
                    num_val("1"),
                    num_val("0")
                ]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "slice",
                &[list_val(vec![num_val("1")]), num_val("0"), num_val("5")]
            )
            .is_err()
        );

        // 17. transpose with null_str and default get_len arms
        let trans_null = eval_func(
            "transpose",
            &[map_val(vec![("a", list_val(vec![null_str]))])],
        )
        .expect("expected transpose");
        assert_eq!(get_obj_len(&trans_null), 0);
        assert_eq!(get_arr_len(&num_val("1")), 0);
        assert_eq!(get_obj_len(&num_val("1")), 0);

        // 18. one() tests
        let single_list = list_val(vec![str_val("solo")]);
        assert_eq!(
            eval_func("one", &[single_list]).expect("ok"),
            str_val("solo")
        );

        let empty_list = Value::new(Type::List(Box::new(Type::String)), ValueData::Array(vec![]));
        let empty_res = eval_func("one", &[empty_list]).expect("ok");
        assert!(empty_res.is_null());

        let multi_list = list_val(vec![str_val("a"), str_val("b")]);
        assert!(eval_func("one", &[multi_list]).is_err());

        let single_set = set_val(vec![str_val("single_set_item")]);
        assert_eq!(
            eval_func("one", &[single_set]).expect("ok"),
            str_val("single_set_item")
        );

        let empty_set = Value::new(
            Type::Set(Box::new(Type::Number)),
            ValueData::Set(BTreeSet::new()),
        );
        assert!(eval_func("one", &[empty_set]).expect("ok").is_null());

        let multi_set = set_val(vec![str_val("a"), str_val("b")]);
        assert!(eval_func("one", &[multi_set]).is_err());

        let unk_list = Value::unknown(Type::List(Box::new(Type::String)));
        assert!(eval_func("one", &[unk_list]).expect("ok").is_unknown());
        let unk_set = Value::unknown(Type::Set(Box::new(Type::String)));
        assert!(eval_func("one", &[unk_set]).expect("ok").is_unknown());
        let unk_tuple = Value::unknown(Type::Tuple(vec![Type::String]));
        assert!(eval_func("one", &[unk_tuple]).expect("ok").is_unknown());
        let unk_empty_tuple = Value::unknown(Type::Tuple(vec![]));
        assert!(
            eval_func("one", &[unk_empty_tuple])
                .expect("ok")
                .is_unknown()
        );
        let unk_dyn = Value::unknown(Type::Dynamic);
        assert!(eval_func("one", &[unk_dyn]).expect("ok").is_unknown());

        let empty_tup = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!(eval_func("one", &[empty_tup]).expect("ok").is_null());
        let empty_set_dyn = Value::new(Type::Dynamic, ValueData::Set(BTreeSet::new()));
        assert!(eval_func("one", &[empty_set_dyn]).expect("ok").is_null());

        assert!(eval_func("one", &[str_val("not_a_collection")]).is_err());
        assert!(
            eval_func("one", &[Value::null(Type::Dynamic)])
                .expect("ok")
                .is_null()
        );
        assert!(eval_func("one", &[]).is_err());
        assert!(eval_func("one", &[num_val("123")]).is_err());

        // index() on invalid type & index on map with non-string key
        assert!(eval_func("index", &[num_val("42"), str_val("k")]).is_err());
        let test_map = map_val(vec![("k", str_val("v"))]);
        assert!(eval_func("index", &[test_map, num_val("123")]).is_err());

        // length() on unknown refined without length constraint
        let unk_ref = Value::unknown_refined(
            Type::String,
            crate::types::refinement::Refinement::not_null(),
        );
        assert!(eval_func("length", &[unk_ref]).expect("ok").is_unknown());

        // 19. coalesce() tests with nulls, empty strings, and unknowns
        assert_eq!(
            eval_func("coalesce", &[str_val(""), str_val("first")]).expect("ok"),
            str_val("first")
        );
        let null_v = Value::null(Type::String);
        assert_eq!(
            eval_func("coalesce", &[null_v.clone(), str_val(""), str_val("found")]).expect("ok"),
            str_val("found")
        );
        let unk_v = Value::unknown(Type::String);
        assert!(
            eval_func("coalesce", &[null_v, str_val(""), unk_v])
                .expect("ok")
                .is_unknown()
        );
        assert!(eval_func("coalesce", &[str_val(""), Value::null(Type::String)]).is_err());

        // 20. index() on maps/objects
        let sample_map = map_val(vec![("alpha", num_val("10")), ("beta", num_val("20"))]);
        let idx_alpha = eval_func("index", &[sample_map.clone(), str_val("alpha")]).expect("ok");
        assert_eq!(idx_alpha, num_val("0"));

        let idx_beta = eval_func("index", &[sample_map.clone(), str_val("beta")]).expect("ok");
        assert_eq!(idx_beta, num_val("1"));

        assert!(eval_func("index", &[sample_map, str_val("gamma")]).is_err());
        assert!(eval_func("index", &[num_val("1"), str_val("a")]).is_err());
    }

    #[test]
    fn test_set_functions_with_dynamic_type() {
        let mut set1 = BTreeSet::new();
        set1.insert(str_val("a"));
        let dyn_set1 = Value::new(Type::Dynamic, ValueData::Set(set1));

        let mut set2 = BTreeSet::new();
        set2.insert(str_val("b"));
        let dyn_set2 = Value::new(Type::Dynamic, ValueData::Set(set2));

        let si =
            eval_func("setintersection", &[dyn_set1.clone(), dyn_set2.clone()]).expect("si ok");
        assert_eq!(si.ty(), &Type::Set(Box::new(Type::Dynamic)));

        let ss = eval_func("setsubtract", &[dyn_set1.clone(), dyn_set2.clone()]).expect("ss ok");
        assert_eq!(ss.ty(), &Type::Set(Box::new(Type::Dynamic)));

        let sym = eval_func(
            "setsymmetricdifference",
            &[dyn_set1.clone(), dyn_set2.clone()],
        )
        .expect("sym ok");
        assert_eq!(sym.ty(), &Type::Set(Box::new(Type::Dynamic)));

        let sp = eval_func("setproduct", &[dyn_set1, dyn_set2]).expect("sp ok");
        assert_eq!(
            sp.ty(),
            &Type::List(Box::new(Type::Tuple(vec![Type::Dynamic, Type::Dynamic])))
        );
    }
}
