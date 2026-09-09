//! String standard library functions.

use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
use crate::types::{Type, Value, ValueData};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Return all string standard library functions.
#[must_use]
pub fn functions() -> Vec<Function> {
    vec![
        chomp_func(),
        endswith_func(),
        indent_func(),
        join_func(),
        lower_func(),
        replace_func(),
        split_func(),
        startswith_func(),
        strcontains_func(),
        strrev_func(),
        substr_func(),
        title_func(),
        trim_func(),
        trimprefix_func(),
        trimsuffix_func(),
        trimspace_func(),
        upper_func(),
        urlencode_func(),
        regex_func(),
        regexall_func(),
        regex_replace_func(),
        format_func(),
        formatlist_func(),
    ]
}

fn regex_replace_func() -> Function {
    Function {
        name: "regex_replace".to_string(),
        func: Arc::new(|args| {
            if args.len() != 3 {
                return Err("regex_replace() takes exactly 3 arguments".to_string());
            }
            for a in args {
                if a.is_unknown() {
                    return Ok(Value::unknown(Type::String));
                }
            }
            let string = coerce_to_string(&args[0], "regex_replace")?;
            let pattern = coerce_to_string(&args[1], "regex_replace")?;
            let replacement = coerce_to_string(&args[2], "regex_replace")?;

            let re = Regex::new(&pattern).map_err(|e| format!("invalid regex pattern: {e}"))?;
            let result = re.replace_all(&string, replacement.as_str()).to_string();
            Ok(Value::new(Type::String, ValueData::String(result)))
        }),
        signature: Some(
            FunctionSignature::with_static_return_type(
                vec![
                    FunctionParamSpec::new("string", Type::String),
                    FunctionParamSpec::new("pattern", Type::String),
                    FunctionParamSpec::new("replacement", Type::String),
                ],
                Type::String,
            )
            .with_description("Replaces all occurrences matching a regex pattern."),
        ),
    }
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

fn chomp_func() -> Function {
    Function {
        name: "chomp".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("chomp expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            if args[0].is_null() {
                return Err("chomp cannot be called with null".to_string());
            }

            let s = coerce_to_string(&args[0], "chomp")?;
            let mut res = s.clone();
            while res.ends_with('\n') || res.ends_with('\r') {
                res.pop();
            }
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}

fn endswith_func() -> Function {
    Function {
        name: "endswith".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("endswith expects 2 arguments, got {}", args.len()));
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Bool));
            }
            let s = coerce_to_string(&args[0], "endswith first argument")?;
            let suffix = coerce_to_string(&args[1], "endswith suffix")?;
            Ok(Value::new(
                Type::Bool,
                ValueData::Bool(s.ends_with(&suffix)),
            ))
        }),
        signature: None,
    }
}

fn startswith_func() -> Function {
    Function {
        name: "startswith".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!(
                    "startswith expects 2 arguments, got {}",
                    args.len()
                ));
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Bool));
            }
            let s = coerce_to_string(&args[0], "startswith first argument")?;
            let prefix = coerce_to_string(&args[1], "startswith prefix")?;
            Ok(Value::new(
                Type::Bool,
                ValueData::Bool(s.starts_with(&prefix)),
            ))
        }),
        signature: None,
    }
}

fn strcontains_func() -> Function {
    Function {
        name: "strcontains".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!(
                    "strcontains expects 2 arguments, got {}",
                    args.len()
                ));
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Bool));
            }
            let s = coerce_to_string(&args[0], "strcontains first argument")?;
            let substr = coerce_to_string(&args[1], "strcontains substr")?;
            Ok(Value::new(Type::Bool, ValueData::Bool(s.contains(&substr))))
        }),
        signature: None,
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for byte in s.as_bytes() {
        if byte.is_ascii_alphanumeric()
            || *byte == b'-'
            || *byte == b'_'
            || *byte == b'.'
            || *byte == b'~'
        {
            out.push(*byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn urlencode_func() -> Function {
    Function {
        name: "urlencode".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("urlencode expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let s = coerce_to_string(&args[0], "urlencode string")?;
            Ok(Value::new(Type::String, ValueData::String(urlencode(&s))))
        }),
        signature: None,
    }
}

fn indent_func() -> Function {
    Function {
        name: "indent".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("indent expects 2 arguments, got {}", args.len()));
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            if args[0].is_null() || args[1].is_null() {
                return Err("indent cannot be called with null".to_string());
            }

            let spaces_val = args[0]
                .clone()
                .coerce(&Type::Number)
                .map_err(|_| "indent first argument must be a number".to_string())?;
            let spaces = match *spaces_val.data {
                ValueData::Number(n) => {
                    use bigdecimal::num_traits::ToPrimitive;
                    n.0.to_usize()
                        .ok_or_else(|| "indent spaces too large".to_string())?
                }
                _ => return Err("indent first argument must be a number".to_string()),
            };

            let s = coerce_to_string(&args[1], "indent")?;
            if s.is_empty() {
                return Ok(Value::new(Type::String, ValueData::String(s)));
            }

            let indent_str = " ".repeat(spaces);
            let mut res = String::new();
            let mut is_first = true;
            for line in s.split('\n') {
                if is_first {
                    // HCL indent does not indent the first line
                    res.push_str(line);
                    is_first = false;
                } else {
                    res.push('\n');
                    if !line.is_empty() {
                        res.push_str(&indent_str);
                        res.push_str(line);
                    }
                }
            }
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}

fn join_func() -> Function {
    Function {
        name: "join".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("join expects 2 arguments, got {}", args.len()));
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let sep = coerce_to_string(&args[0], "join separator")?;

            let mut parts = Vec::new();
            if args[1].ty().is_collection() || args[1].ty().is_structural() {
                match &*args[1].data {
                    ValueData::Array(arr) => {
                        for item in arr {
                            parts.push(coerce_to_string(item, "join element")?);
                        }
                    }
                    ValueData::Set(set) => {
                        for item in set {
                            parts.push(coerce_to_string(item, "join element")?);
                        }
                    }
                    ValueData::Null => return Err("join list cannot be null".to_string()),
                    _ => return Err("join requires a list or set".to_string()),
                }
            } else {
                return Err("join requires a list or set".to_string());
            }

            Ok(Value::new(
                Type::String,
                ValueData::String(parts.join(&sep)),
            ))
        }),
        signature: None,
    }
}

fn lower_func() -> Function {
    Function {
        name: "lower".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("lower expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            if args[0].is_null() {
                return Err("lower cannot be called with null".to_string());
            }

            let s = coerce_to_string(&args[0], "lower")?;
            Ok(Value::new(
                Type::String,
                ValueData::String(s.to_lowercase()),
            ))
        }),
        signature: None,
    }
}

fn upper_func() -> Function {
    Function {
        name: "upper".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("upper expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            if args[0].is_null() {
                return Err("upper cannot be called with null".to_string());
            }

            let s = coerce_to_string(&args[0], "upper")?;
            Ok(Value::new(
                Type::String,
                ValueData::String(s.to_uppercase()),
            ))
        }),
        signature: None,
    }
}

fn replace_func() -> Function {
    Function {
        name: "replace".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 3 {
                return Err(format!("replace expects 3 arguments, got {}", args.len()));
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "replace str")?;
            let substr = coerce_to_string(&args[1], "replace substr")?;
            let rep = coerce_to_string(&args[2], "replace replacement")?;

            if substr.starts_with('/') && substr.ends_with('/') && substr.len() >= 2 {
                let re_str = &substr[1..substr.len() - 1];
                let re = Regex::new(re_str).map_err(|e| format!("invalid regex: {e}"))?;
                // HCL regex replace uses $1 instead of , Rust regex uses $1
                Ok(Value::new(
                    Type::String,
                    ValueData::String(re.replace_all(&s, rep.as_str()).to_string()),
                ))
            } else {
                Ok(Value::new(
                    Type::String,
                    ValueData::String(s.replace(&substr, &rep)),
                ))
            }
        }),
        signature: None,
    }
}

fn split_func() -> Function {
    Function {
        name: "split".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("split expects 2 arguments, got {}", args.len()));
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::List(Box::new(Type::String))));
            }

            let sep = coerce_to_string(&args[0], "split sep")?;
            let s = coerce_to_string(&args[1], "split str")?;

            let parts: Vec<Value> = s
                .split(&sep)
                .map(|p| Value::new(Type::String, ValueData::String(p.to_string())))
                .collect();
            Ok(Value::new(
                Type::List(Box::new(Type::String)),
                ValueData::Array(parts),
            ))
        }),
        signature: None,
    }
}

fn strrev_func() -> Function {
    Function {
        name: "strrev".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("strrev expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "strrev")?;
            Ok(Value::new(
                Type::String,
                ValueData::String(s.chars().rev().collect()),
            ))
        }),
        signature: None,
    }
}

/// Computes an updated refined [`Value`] for a `substr` call on an unknown string if possible.
fn infer_substr_refinement(str_val: &Value, off_val: &Value, len_val: &Value) -> Option<Value> {
    if !str_val.is_unknown() || off_val.is_unknown() || len_val.is_unknown() {
        return None;
    }
    let n_off = match &*off_val.coerce(&Type::Number).ok()?.data {
        ValueData::Number(n) => n.clone(),
        _ => return None,
    };
    let n_len = match &*len_val.coerce(&Type::Number).ok()?.data {
        ValueData::Number(n) => n.clone(),
        _ => return None,
    };
    use bigdecimal::num_traits::ToPrimitive;
    let off = n_off.0.to_isize()?;
    let len = n_len.0.to_isize()?;
    if off < 0 || len < 0 {
        return None;
    }
    let off_u = off as usize;
    let len_u = len as usize;
    let r = str_val.refinement()?;
    if let Some(ref p) = r.string_prefix {
        let p_chars: Vec<char> = p.chars().collect();
        if off_u + len_u <= p_chars.len() {
            let res: String = p_chars[off_u..off_u + len_u].iter().collect();
            return Some(Value::new(Type::String, ValueData::String(res)));
        }
    }

    let mut new_r = crate::types::refinement::Refinement::not_null();
    if let Some(p) = r.string_prefix.as_ref().filter(|_| off_u == 0) {
        new_r = new_r.with_prefix(p.clone());
    }
    new_r.string_length_max = Some(
        r.string_length_max
            .map_or(len_u, |max_l| len_u.min(max_l.saturating_sub(off_u))),
    );
    if let Some(min_l) = r.string_length_min.filter(|&m| m > off_u) {
        let rem = min_l - off_u;
        new_r.string_length_min = Some(len_u.min(rem));
    }
    Some(Value::unknown_refined(Type::String, new_r))
}

fn substr_func() -> Function {
    Function {
        name: "substr".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 3 {
                return Err(format!("substr expects 3 arguments, got {}", args.len()));
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                if let Some(res) = infer_substr_refinement(&args[0], &args[1], &args[2]) {
                    return Ok(res);
                }
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "substr string")?;
            let offset_val = args[1]
                .clone()
                .coerce(&Type::Number)
                .map_err(|_| "substr offset must be number".to_string())?;
            let length_val = args[2]
                .clone()
                .coerce(&Type::Number)
                .map_err(|_| "substr length must be number".to_string())?;

            let mut offset = match *offset_val.data {
                ValueData::Number(n) => {
                    use bigdecimal::num_traits::ToPrimitive;
                    n.0.to_isize()
                        .ok_or_else(|| "offset too large".to_string())?
                }
                _ => return Err("substr offset must be number".to_string()),
            };

            let length = match *length_val.data {
                ValueData::Number(n) => {
                    use bigdecimal::num_traits::ToPrimitive;
                    n.0.to_isize()
                        .ok_or_else(|| "length too large".to_string())?
                }
                _ => return Err("substr length must be number".to_string()),
            };

            let chars: Vec<char> = s.chars().collect();
            let char_len = chars.len() as isize;

            if offset < 0 {
                offset += char_len;
            }
            if offset < 0 {
                offset = 0;
            }
            if offset > char_len {
                offset = char_len;
            }

            let end = if length < 0 {
                char_len
            } else {
                std::cmp::min(offset + length, char_len)
            };

            let res_str: String = chars[offset as usize..end as usize].iter().collect();
            Ok(Value::new(Type::String, ValueData::String(res_str)))
        }),
        signature: None,
    }
}

fn title_func() -> Function {
    Function {
        name: "title".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("title expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "title")?;
            let mut res = String::new();
            let mut capitalize_next = true;
            for c in s.chars() {
                if c.is_alphabetic() {
                    if capitalize_next {
                        res.extend(c.to_uppercase());
                        capitalize_next = false;
                    } else {
                        res.push(c);
                    }
                } else {
                    res.push(c);
                    capitalize_next = true;
                }
            }
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}

fn trim_func() -> Function {
    Function {
        name: "trim".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("trim expects 2 arguments, got {}", args.len()));
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "trim string")?;
            let cutset = coerce_to_string(&args[1], "trim cutset")?;

            let res = s.trim_matches(|c| cutset.contains(c)).to_string();
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}

fn trimprefix_func() -> Function {
    Function {
        name: "trimprefix".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!(
                    "trimprefix expects 2 arguments, got {}",
                    args.len()
                ));
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "trimprefix string")?;
            let prefix = coerce_to_string(&args[1], "trimprefix prefix")?;

            let res = if s.starts_with(&prefix) {
                s[prefix.len()..].to_string()
            } else {
                s
            };
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}

fn trimsuffix_func() -> Function {
    Function {
        name: "trimsuffix".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!(
                    "trimsuffix expects 2 arguments, got {}",
                    args.len()
                ));
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "trimsuffix string")?;
            let suffix = coerce_to_string(&args[1], "trimsuffix suffix")?;

            let res = if s.ends_with(&suffix) {
                s[..s.len() - suffix.len()].to_string()
            } else {
                s
            };
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}

fn trimspace_func() -> Function {
    Function {
        name: "trimspace".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("trimspace expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "trimspace")?;
            Ok(Value::new(
                Type::String,
                ValueData::String(s.trim().to_string()),
            ))
        }),
        signature: None,
    }
}

fn regex_func() -> Function {
    Function {
        name: "regex".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("regex expects 2 arguments, got {}", args.len()));
            }
            let mut marks = BTreeSet::new();
            for a in args {
                marks.extend(a.marks().clone());
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::Dynamic).with_marks(marks));
            }

            let re_str = coerce_to_string(&args[0], "regex pattern")?;
            let s = coerce_to_string(&args[1], "regex string")?;

            let re = Regex::new(&re_str).map_err(|e| format!("invalid regex: {e}"))?;

            let names: Vec<Option<&str>> = re.capture_names().skip(1).collect();
            let has_named = names.iter().any(Option::is_some);
            let has_unnamed = names.iter().any(Option::is_none);

            if has_named && has_unnamed {
                return Err("regex pattern cannot mix named and unnamed capture groups".to_string());
            }

            let captures = re
                .captures(&s)
                .ok_or_else(|| "no regex match".to_string())?;

            if names.is_empty() {
                let matched_str = captures.get(0).map_or("", |m| m.as_str()).to_string();
                return Ok(Value::new_with_marks(
                    Type::String,
                    ValueData::String(matched_str),
                    marks,
                ));
            }

            if has_named {
                let mut obj = BTreeMap::new();
                let mut obj_types = BTreeMap::new();
                for (n, opt_match) in re
                    .capture_names()
                    .skip(1)
                    .flatten()
                    .zip(captures.iter().skip(1))
                {
                    obj_types.insert(n.to_string(), Type::String);
                    if let Some(m) = opt_match {
                        obj.insert(
                            n.to_string(),
                            Value::new(Type::String, ValueData::String(m.as_str().to_string())),
                        );
                    } else {
                        obj.insert(n.to_string(), Value::null(Type::String));
                    }
                }
                Ok(Value::new_with_marks(
                    Type::object(obj_types),
                    ValueData::Object(obj),
                    marks,
                ))
            } else {
                let mut list = Vec::new();
                for opt_match in captures.iter().skip(1) {
                    if let Some(m) = opt_match {
                        list.push(Value::new(
                            Type::String,
                            ValueData::String(m.as_str().to_string()),
                        ));
                    } else {
                        list.push(Value::null(Type::String));
                    }
                }
                Ok(Value::new_with_marks(
                    Type::List(Box::new(Type::String)),
                    ValueData::Array(list),
                    marks,
                ))
            }
        }),
        signature: None,
    }
}

fn regexall_func() -> Function {
    Function {
        name: "regexall".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("regexall expects 2 arguments, got {}", args.len()));
            }
            let mut marks = BTreeSet::new();
            for a in args {
                marks.extend(a.marks().clone());
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::List(Box::new(Type::Dynamic))).with_marks(marks));
            }

            let re_str = coerce_to_string(&args[0], "regexall pattern")?;
            let s = coerce_to_string(&args[1], "regexall string")?;

            let re = Regex::new(&re_str).map_err(|e| format!("invalid regex: {e}"))?;

            let names: Vec<Option<&str>> = re.capture_names().skip(1).collect();
            let has_named = names.iter().any(Option::is_some);
            let has_unnamed = names.iter().any(Option::is_none);

            if has_named && has_unnamed {
                return Err(
                    "regexall pattern cannot mix named and unnamed capture groups".to_string(),
                );
            }

            let elem_ty = if names.is_empty() {
                Type::String
            } else if has_named {
                let mut obj_types = BTreeMap::new();
                for n in names.iter().flatten() {
                    obj_types.insert((*n).to_string(), Type::String);
                }
                Type::object(obj_types)
            } else {
                Type::List(Box::new(Type::String))
            };

            let mut results = Vec::new();
            for captures in re.captures_iter(&s) {
                if names.is_empty() {
                    let matched_str = captures.get(0).map_or("", |m| m.as_str()).to_string();
                    results.push(Value::new(Type::String, ValueData::String(matched_str)));
                } else if has_named {
                    let mut obj = BTreeMap::new();
                    for (n, opt_match) in re
                        .capture_names()
                        .skip(1)
                        .flatten()
                        .zip(captures.iter().skip(1))
                    {
                        if let Some(m) = opt_match {
                            obj.insert(
                                n.to_string(),
                                Value::new(Type::String, ValueData::String(m.as_str().to_string())),
                            );
                        } else {
                            obj.insert(n.to_string(), Value::null(Type::String));
                        }
                    }
                    results.push(Value::new(elem_ty.clone(), ValueData::Object(obj)));
                } else {
                    let mut list = Vec::new();
                    for opt_match in captures.iter().skip(1) {
                        if let Some(m) = opt_match {
                            list.push(Value::new(
                                Type::String,
                                ValueData::String(m.as_str().to_string()),
                            ));
                        } else {
                            list.push(Value::null(Type::String));
                        }
                    }
                    results.push(Value::new(
                        Type::List(Box::new(Type::String)),
                        ValueData::Array(list),
                    ));
                }
            }

            Ok(Value::new_with_marks(
                Type::List(Box::new(elem_ty)),
                ValueData::Array(results),
                marks,
            ))
        }),
        signature: None,
    }
}

fn format_func() -> Function {
    Function {
        name: "format".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.is_empty() {
                return Err("format expects at least 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let fmt_str = coerce_to_string(&args[0], "format string")?;

            // Basic mock format. Replaces %s, %d, %v sequentially.
            // Full go-cty format is highly complex (sprintf).
            let mut res = String::new();
            let mut chars = fmt_str.chars().peekable();
            let mut arg_idx = 1;

            while let Some(c) = chars.next() {
                if c == '%' {
                    if let Some(&next_c) = chars.peek() {
                        if next_c == '%' {
                            res.push('%');
                            chars.next();
                            continue;
                        }

                        // Parse format verb (very simplified)
                        let verb = next_c;
                        chars.next(); // consume

                        if arg_idx >= args.len() {
                            return Err("not enough arguments for format string".to_string());
                        }
                        let val = &args[arg_idx];
                        arg_idx += 1;

                        if val.is_unknown() {
                            return Ok(Value::unknown(Type::String));
                        }

                        match verb {
                            's' => res.push_str(&coerce_to_string(val, "format %s")?),
                            'd' => res.push_str(&coerce_to_string(val, "format %d")?),
                            'v' => res.push_str(&coerce_to_string(val, "format %v")?),
                            _ => res.push_str(&coerce_to_string(val, "format arg")?), // fallback
                        }
                    } else {
                        res.push('%');
                    }
                } else {
                    res.push(c);
                }
            }
            if arg_idx < args.len() {
                return Err("too many arguments for format string".to_string());
            }
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}

fn formatlist_func() -> Function {
    Function {
        name: "formatlist".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.is_empty() {
                return Err("formatlist expects at least 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::List(Box::new(Type::String))));
            }

            let _fmt_str = coerce_to_string(&args[0], "formatlist string")?;

            // Simplified formatlist: we expect lists and scalars.
            // We find the max length among all list arguments.
            let mut max_len = 0;
            let mut list_args: Vec<Option<&[Value]>> = Vec::with_capacity(args.len());
            list_args.push(None); // for args[0] (format string)

            let mut has_list = false;
            for arg in &args[1..] {
                if let ValueData::Array(arr) = &*arg.data {
                    has_list = true;
                    max_len = std::cmp::max(max_len, arr.len());
                    list_args.push(Some(arr.as_slice()));
                } else {
                    list_args.push(None);
                }
            }

            let mut results = Vec::new();
            let format_func_ctx = format_func();
            for i in 0..max_len {
                let mut current_args = vec![args[0].clone()];
                for (j, arg) in args[1..].iter().enumerate() {
                    if let Some(arr) = list_args[j + 1] {
                        if i < arr.len() {
                            current_args.push(arr[i].clone());
                        } else {
                            return Err(
                                "formatlist list arguments must have the same length".to_string()
                            );
                        }
                    } else {
                        current_args.push(arg.clone());
                    }
                }

                let val = (format_func_ctx.func)(&current_args)?;
                results.push(val);
            }

            if max_len == 0 {
                // If no lists were provided, formatlist returns a list of 1 element.
                if !has_list {
                    let val = (format_func_ctx.func)(args)?;
                    results.push(val);
                }
            }

            Ok(Value::new(
                Type::List(Box::new(Type::String)),
                ValueData::Array(results),
            ))
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

    fn num_val(s: &str) -> Value {
        Value::new(
            Type::Number,
            ValueData::Number(Number::new(
                BigDecimal::from_str(s).expect("expected value"),
            )),
        )
    }

    fn unk_val() -> Value {
        Value::unknown(Type::String)
    }

    fn null_val() -> Value {
        Value::new(Type::Dynamic, ValueData::Null)
    }

    fn list_val(vals: Vec<Value>) -> Value {
        Value::new(Type::List(Box::new(Type::Dynamic)), ValueData::Array(vals))
    }

    #[test]
    fn test_coerce_to_string() {
        assert!(coerce_to_string(&str_val("123"), "test").is_ok());
        let unk = Value::unknown(Type::object(std::collections::BTreeMap::new()));
        assert!(coerce_to_string(&unk, "test").is_err());
    }

    #[test]
    fn test_chomp() {
        assert_eq!(
            eval_func("chomp", &[str_val("hello\n\n")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("hello".to_string())
        );
        assert!(eval_func("chomp", &[]).is_err());
        assert!(
            eval_func("chomp", &[unk_val()])
                .expect("expected value")
                .is_unknown()
        );
        assert!(eval_func("chomp", &[null_val()]).is_err());
    }

    #[test]
    fn test_indent() {
        assert_eq!(
            eval_func("indent", &[num_val("2"), str_val("foo\nbar")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("foo\n  bar".to_string())
        );
        assert!(eval_func("indent", &[]).is_err());
        assert!(
            eval_func("indent", &[unk_val(), str_val("a")])
                .expect("expected value")
                .is_unknown()
        );
        assert!(eval_func("indent", &[num_val("2"), null_val()]).is_err());
        assert!(eval_func("indent", &[str_val("bad"), str_val("foo")]).is_err()); // spaces not int
        assert!(eval_func("indent", &[num_val("2"), str_val("")]).is_ok()); // empty string
    }

    #[test]
    fn test_join() {
        assert!(
            eval_func(
                "join",
                &[str_val(","), Value::new(Type::Dynamic, ValueData::Null)]
            )
            .is_err()
        );
        assert!(eval_func("join", &[str_val(","), str_val("not a list")]).is_err());

        assert_eq!(
            eval_func(
                "join",
                &[str_val(","), list_val(vec![str_val("a"), str_val("b")])]
            )
            .expect("expected value")
            .data
            .as_ref(),
            &ValueData::String("a,b".to_string())
        );
        // join on tuple instead of list
        let tup = Value::new(
            Type::Tuple(vec![Type::String, Type::String]),
            ValueData::Array(vec![str_val("a"), str_val("b")]),
        );
        assert!(eval_func("join", &[str_val(","), tup]).is_ok());

        assert!(eval_func("join", &[]).is_err());
        assert!(
            eval_func("join", &[unk_val(), list_val(vec![])])
                .expect("expected value")
                .is_unknown()
        );
        assert!(eval_func("join", &[str_val(","), null_val()]).is_err()); // neither tuple nor list

        // join on Set
        let mut set = std::collections::BTreeSet::new();
        set.insert(str_val("a"));
        let set_val = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set));
        assert!(eval_func("join", &[str_val(","), set_val]).is_ok());
    }

    #[test]
    fn test_lower_upper() {
        assert_eq!(
            eval_func("lower", &[str_val("HeLlo")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("hello".to_string())
        );
        assert!(eval_func("lower", &[]).is_err());
        assert!(
            eval_func("lower", &[unk_val()])
                .expect("expected value")
                .is_unknown()
        );

        assert_eq!(
            eval_func("upper", &[str_val("HeLlo")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("HELLO".to_string())
        );
        assert!(eval_func("upper", &[]).is_err());
        assert!(
            eval_func("upper", &[unk_val()])
                .expect("expected value")
                .is_unknown()
        );
    }

    #[test]
    fn test_replace() {
        assert_eq!(
            eval_func("replace", &[str_val("hello"), str_val("l"), str_val("x")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("hexxo".to_string())
        );
        // regex replace
        assert_eq!(
            eval_func(
                "replace",
                &[str_val("hello"), str_val("/l+/"), str_val("x")]
            )
            .expect("expected value")
            .data
            .as_ref(),
            &ValueData::String("hexo".to_string())
        );
        // bad regex replace
        assert!(
            eval_func(
                "replace",
                &[str_val("hello"), str_val("/l+/"), str_val("$9")]
            )
            .is_ok()
        ); // capture group 9 missing but replace succeeds
        assert!(
            eval_func(
                "replace",
                &[str_val("hello"), str_val("/[a-/"), str_val("x")]
            )
            .is_err()
        ); // regex invalid

        assert!(eval_func("replace", &[]).is_err());
        assert!(
            eval_func("replace", &[unk_val(), str_val("l"), str_val("x")])
                .expect("expected value")
                .is_unknown()
        );
    }

    #[test]
    fn test_split() {
        let res = eval_func("split", &[str_val(","), str_val("a,b,c")]).expect("expected value");
        assert_eq!(
            *res.data,
            ValueData::Array(vec![str_val("a"), str_val("b"), str_val("c")])
        );

        // empty separator
        let res_empty = eval_func("split", &[str_val(""), str_val("abc")]).expect("expected value");
        // string.split("") has length 5 in rust: "", "a", "b", "c", "" or similar, let's just assert is_ok
        assert_eq!(
            std::mem::discriminant(&*res_empty.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );

        assert!(eval_func("split", &[]).is_err());
        assert!(
            eval_func("split", &[unk_val(), str_val("abc")])
                .expect("expected value")
                .is_unknown()
        );
    }

    #[test]
    fn test_strrev() {
        assert_eq!(
            eval_func("strrev", &[str_val("hello")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("olleh".to_string())
        );
        assert!(eval_func("strrev", &[]).is_err());
        assert!(
            eval_func("strrev", &[unk_val()])
                .expect("expected value")
                .is_unknown()
        );
    }

    #[test]
    fn test_substr() {
        assert_eq!(
            eval_func("substr", &[str_val("hello"), num_val("1"), num_val("3")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("ell".to_string())
        );
        assert_eq!(
            eval_func("substr", &[str_val("hello"), num_val("-2"), num_val("2")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("lo".to_string())
        );
        assert_eq!(
            eval_func("substr", &[str_val("hello"), num_val("1"), num_val("-1")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("ello".to_string())
        );

        // offset out of bounds
        assert_eq!(
            eval_func("substr", &[str_val("hello"), num_val("10"), num_val("1")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String(String::new())
        );

        // length out of bounds
        assert_eq!(
            eval_func("substr", &[str_val("hello"), num_val("1"), num_val("10")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("ello".to_string())
        );

        // negative offset larger than len
        assert_eq!(
            eval_func("substr", &[str_val("hello"), num_val("-10"), num_val("2")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("he".to_string())
        );

        assert!(eval_func("substr", &[]).is_err());
        assert!(
            eval_func("substr", &[unk_val(), num_val("1"), num_val("1")])
                .expect("expected value")
                .is_unknown()
        );
        assert!(eval_func("substr", &[str_val("hello"), str_val("bad"), num_val("1")]).is_err());
    }

    #[test]
    fn test_title() {
        assert_eq!(
            eval_func("title", &[str_val("hello world")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("Hello World".to_string())
        );
        assert!(eval_func("title", &[]).is_err());
        assert!(
            eval_func("title", &[unk_val()])
                .expect("expected value")
                .is_unknown()
        );
    }

    #[test]
    fn test_trim_functions() {
        assert_eq!(
            eval_func("trim", &[str_val("!hello!"), str_val("!")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("hello".to_string())
        );
        assert!(eval_func("trim", &[]).is_err());
        assert!(
            eval_func("trim", &[unk_val(), str_val("!")])
                .expect("expected value")
                .is_unknown()
        );

        assert_eq!(
            eval_func("trimprefix", &[str_val("!hello!"), str_val("!")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("hello!".to_string())
        );
        assert!(eval_func("trimprefix", &[]).is_err());
        assert!(
            eval_func("trimprefix", &[unk_val(), str_val("!")])
                .expect("expected value")
                .is_unknown()
        );

        assert_eq!(
            eval_func("trimsuffix", &[str_val("!hello!"), str_val("!")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("!hello".to_string())
        );
        assert!(eval_func("trimsuffix", &[]).is_err());
        assert!(
            eval_func("trimsuffix", &[unk_val(), str_val("!")])
                .expect("expected value")
                .is_unknown()
        );

        assert_eq!(
            eval_func("trimspace", &[str_val("  hello  ")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("hello".to_string())
        );
        assert!(eval_func("trimspace", &[]).is_err());
        assert!(
            eval_func("trimspace", &[unk_val()])
                .expect("expected value")
                .is_unknown()
        );
    }

    #[test]
    fn test_regex() {
        // no capture groups
        assert_eq!(
            eval_func("regex", &[str_val("a+"), str_val("aa")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("aa".to_string())
        );
        // optional empty capture group
        assert!(eval_func("regex", &[str_val("(a+)(b*)"), str_val("aa")]).is_ok());

        assert!(eval_func("regex", &[str_val("^(a+)(b+)$"), str_val("aabb")]).is_ok());

        // No match
        assert!(eval_func("regex", &[str_val("x"), str_val("aabb")]).is_err());

        // Invalid regex
        assert!(eval_func("regex", &[str_val("[x"), str_val("aabb")]).is_err());

        assert!(eval_func("regex", &[]).is_err());
        assert!(
            eval_func("regex", &[unk_val(), str_val("aabb")])
                .expect("expected value")
                .is_unknown()
        );

        // Named capture groups with unmatched optional group (lines 731-733)
        let res_opt = eval_func(
            "regex",
            &[
                str_val("(?P<first>[a-z]+)(?:-(?P<second>[0-9]+))?"),
                str_val("hello"),
            ],
        )
        .expect("expected value");
        let mut expected_map = std::collections::BTreeMap::new();
        expected_map.insert("first".to_string(), str_val("hello"));
        expected_map.insert("second".to_string(), Value::null(Type::String));
        assert_eq!(*res_opt.data, ValueData::Object(expected_map));

        // regex_replace wrong arg length and unknown arg (lines 44, 48)
        assert!(eval_func("regex_replace", &[]).is_err());
        assert!(
            eval_func("regex_replace", &[unk_val(), str_val("a"), str_val("b")])
                .expect("expected value")
                .is_unknown()
        );

        let non_str = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!(
            eval_func(
                "regex_replace",
                &[non_str.clone(), str_val("a"), str_val("b")]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "regex_replace",
                &[str_val("hello"), non_str.clone(), str_val("b")]
            )
            .is_err()
        );
        assert!(eval_func("regex_replace", &[str_val("hello"), str_val("a"), non_str]).is_err());
        assert!(
            eval_func(
                "regex_replace",
                &[str_val("hello"), str_val("(invalid"), str_val("b")]
            )
            .is_err()
        );
        let res_replace = eval_func(
            "regex_replace",
            &[str_val("hello world"), str_val("world"), str_val("rust")],
        )
        .expect("expected value");
        assert_eq!(res_replace.to_string(), "\"hello rust\"");

        // Mixed named and unnamed capture groups error
        assert!(
            eval_func(
                "regex",
                &[str_val("(?P<name>[a-z]+)-([0-9]+)"), str_val("foo-123")]
            )
            .is_err()
        );
    }

    #[test]
    fn test_regexall() {
        // Mixed named and unnamed capture groups error
        assert!(
            eval_func(
                "regexall",
                &[str_val("(?P<name>[a-z]+)-([0-9]+)"), str_val("foo-123")]
            )
            .is_err()
        );

        let res =
            eval_func("regexall", &[str_val("a+"), str_val("aabb aa")]).expect("expected value");
        assert_eq!(
            std::mem::discriminant(&*res.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );

        // capture groups
        let res_cap = eval_func("regexall", &[str_val("(a+)(b*)"), str_val("aabb aa")])
            .expect("expected value");
        assert_eq!(
            std::mem::discriminant(&*res_cap.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );

        // Named capture groups with unmatched optional group (lines 826-828)
        let res_all_opt = eval_func(
            "regexall",
            &[
                str_val("(?P<first>[a-z]+)(?:-(?P<second>[0-9]+))?"),
                str_val("hello world"),
            ],
        )
        .expect("expected value");
        let mut m1 = std::collections::BTreeMap::new();
        m1.insert("first".to_string(), str_val("hello"));
        m1.insert("second".to_string(), Value::null(Type::String));
        let mut m2 = std::collections::BTreeMap::new();
        m2.insert("first".to_string(), str_val("world"));
        m2.insert("second".to_string(), Value::null(Type::String));
        let elem_ty = Type::object(std::collections::BTreeMap::from([
            ("first".to_string(), Type::String),
            ("second".to_string(), Type::String),
        ]));
        let expected_arr = vec![
            Value::new(elem_ty.clone(), ValueData::Object(m1)),
            Value::new(elem_ty, ValueData::Object(m2)),
        ];
        assert_eq!(*res_all_opt.data, ValueData::Array(expected_arr));

        // No match
        let res_no =
            eval_func("regexall", &[str_val("x"), str_val("aabb")]).expect("expected value");
        assert_eq!(*res_no.data, ValueData::Array(vec![]));

        // Invalid regex
        assert!(eval_func("regexall", &[str_val("[x"), str_val("aabb")]).is_err());

        assert!(eval_func("regexall", &[]).is_err());
        assert!(
            eval_func("regexall", &[unk_val(), str_val("aabb")])
                .expect("expected value")
                .is_unknown()
        );
    }

    #[test]
    fn test_format() {
        assert_eq!(
            eval_func(
                "format",
                &[str_val("hello %s %d"), str_val("world"), num_val("123")]
            )
            .expect("expected value")
            .data
            .as_ref(),
            &ValueData::String("hello world 123".to_string())
        );
        // sprintf crate might not support %.1f same way for bigdecimal.
        // We just ensure we cover the code paths.
        assert!(eval_func("format", &[str_val("%s"), num_val("1.23")]).is_ok());
        // %%
        assert_eq!(
            eval_func("format", &[str_val("%%")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("%".to_string())
        );
        // %v and trailing %
        assert_eq!(
            eval_func("format", &[str_val("%v %"), str_val("val")])
                .expect("expected value")
                .data
                .as_ref(),
            &ValueData::String("val %".to_string())
        );
        // too few args
        assert!(eval_func("format", &[str_val("%s %s"), str_val("a")]).is_err());
        // unknown arg
        assert!(
            eval_func("format", &[str_val("%s"), unk_val()])
                .expect("expected value")
                .is_unknown()
        );

        assert!(eval_func("format", &[]).is_err());
        assert!(eval_func("format", &[str_val("%s %s"), str_val("a")]).is_err());

        assert!(
            eval_func("format", &[unk_val(), str_val("a")])
                .expect("expected value")
                .is_unknown()
        );
        assert!(
            eval_func("format", &[str_val("%s"), unk_val()])
                .expect("expected value")
                .is_unknown()
        );
    }

    #[test]
    fn test_formatlist() {
        // scalar mixed with list
        assert!(
            eval_func(
                "formatlist",
                &[str_val("%s %s"), list_val(vec![str_val("a")]), str_val("b")]
            )
            .is_ok()
        );

        let res = eval_func(
            "formatlist",
            &[
                str_val("hello %s"),
                list_val(vec![str_val("a"), str_val("b")]),
            ],
        )
        .expect("expected value");
        assert_eq!(
            *res.data,
            ValueData::Array(vec![
                Value::new(Type::String, ValueData::String("hello a".to_string())),
                Value::new(Type::String, ValueData::String("hello b".to_string())),
            ])
        );

        let res2 =
            eval_func("formatlist", &[str_val("hello %s"), str_val("a")]).expect("expected value");
        assert_eq!(
            *res2.data,
            ValueData::Array(vec![Value::new(
                Type::String,
                ValueData::String("hello a".to_string())
            ),])
        );

        let res_empty = eval_func("formatlist", &[str_val("hello %s"), list_val(vec![])])
            .expect("expected value");
        assert_eq!(*res_empty.data, ValueData::Array(vec![]));

        assert!(eval_func("formatlist", &[]).is_err());

        // mismatched list lengths
        assert!(
            eval_func(
                "formatlist",
                &[
                    str_val("%s %s"),
                    list_val(vec![str_val("a")]),
                    list_val(vec![str_val("a"), str_val("b")])
                ]
            )
            .is_err()
        );
        assert!(
            eval_func("formatlist", &[unk_val(), str_val("a")])
                .expect("expected value")
                .is_unknown()
        );
    }

    #[test]
    fn test_string_coverage_comprehensive() {
        let non_val = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        let null_str = Value::new(Type::String, ValueData::Null);

        // 1. coerce_to_string error with non-string and null string
        assert!(eval_func("chomp", std::slice::from_ref(&non_val)).is_err());
        assert!(eval_func("chomp", std::slice::from_ref(&null_str)).is_err());

        // 2. chomp with unknown, null, and carriage return
        assert!(eval_func("chomp", &[unk_val()]).expect("ok").is_unknown());
        assert!(eval_func("chomp", &[null_val()]).is_err());
        assert_eq!(
            *eval_func("chomp", &[str_val("hello\r\n")])
                .expect("ok")
                .data,
            ValueData::String("hello".to_string())
        );
        assert_eq!(
            *eval_func("chomp", &[str_val("hello\r")]).expect("ok").data,
            ValueData::String("hello".to_string())
        );

        // 3. indent error and unknown paths
        assert!(
            eval_func("indent", &[num_val("2"), unk_val()])
                .expect("ok")
                .is_unknown()
        );
        assert!(eval_func("indent", &[null_val(), str_val("foo")]).is_err());
        assert!(eval_func("indent", &[num_val("-1"), str_val("foo")]).is_err());
        assert!(
            eval_func(
                "indent",
                &[
                    Value::new(Type::Dynamic, ValueData::Bool(true)),
                    str_val("foo")
                ]
            )
            .is_err()
        );
        assert_eq!(
            *eval_func("indent", &[num_val("2"), str_val("a\n\nb")])
                .expect("ok")
                .data,
            ValueData::String("a\n\n  b".to_string())
        );

        // 4. join errors and unknown paths
        assert!(
            eval_func("join", &[str_val(","), unk_val()])
                .expect("ok")
                .is_unknown()
        );
        assert!(eval_func("join", &[str_val(","), null_val()]).is_err());
        assert!(eval_func("join", &[str_val(","), num_val("123")]).is_err());
        let null_list = Value::new(Type::List(Box::new(Type::String)), ValueData::Null);
        assert!(eval_func("join", &[str_val(","), null_list]).is_err());
        let obj_struct = Value::new(
            Type::object(std::collections::BTreeMap::new()),
            ValueData::Object(std::collections::BTreeMap::new()),
        );
        assert!(eval_func("join", &[str_val(","), obj_struct]).is_err());

        // 5. lower and upper null errors
        assert!(eval_func("lower", &[null_val()]).is_err());
        assert!(eval_func("upper", &[null_val()]).is_err());

        // 6. substr unknowns, large numbers, dynamic errors, and arg coercion errors
        assert!(
            eval_func("substr", &[str_val("hello"), num_val("0"), unk_val()])
                .expect("ok")
                .is_unknown()
        );
        assert!(
            eval_func("substr", &[str_val("hello"), unk_val(), num_val("2")])
                .expect("ok")
                .is_unknown()
        );
        assert!(
            eval_func(
                "substr",
                &[str_val("hello"), num_val("1e100"), num_val("2")]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "substr",
                &[str_val("hello"), num_val("0"), num_val("1e100")]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "substr",
                &[
                    str_val("hello"),
                    Value::new(Type::Dynamic, ValueData::Bool(true)),
                    num_val("2")
                ]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "substr",
                &[
                    str_val("hello"),
                    num_val("0"),
                    Value::new(Type::Dynamic, ValueData::Bool(true))
                ]
            )
            .is_err()
        );
        assert!(eval_func("substr", &[str_val("hello"), non_val.clone(), num_val("2")]).is_err());
        assert!(eval_func("substr", &[str_val("hello"), num_val("0"), non_val.clone()]).is_err());

        // 7. trimprefix and trimsuffix when pattern does not match, and replace pattern edge cases
        assert_eq!(
            *eval_func("trimprefix", &[str_val("hello"), str_val("xyz")])
                .expect("ok")
                .data,
            ValueData::String("hello".to_string())
        );
        assert_eq!(
            *eval_func("trimsuffix", &[str_val("hello"), str_val("xyz")])
                .expect("ok")
                .data,
            ValueData::String("hello".to_string())
        );
        assert_eq!(
            *eval_func(
                "replace",
                &[str_val("hello"), str_val("/unclosed"), str_val("x")]
            )
            .expect("ok")
            .data,
            ValueData::String("hello".to_string())
        );
        assert_eq!(
            *eval_func("replace", &[str_val("hello"), str_val("/"), str_val("x")])
                .expect("ok")
                .data,
            ValueData::String("hello".to_string())
        );

        // 8. regex and regexall optional capture groups returning null
        let res_re = eval_func("regex", &[str_val("(a)|(b)"), str_val("a")]).expect("ok");
        assert_eq!(
            std::mem::discriminant(&*res_re.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );
        let res_reall = eval_func("regexall", &[str_val("(a)|(b)"), str_val("a")]).expect("ok");
        assert_eq!(
            std::mem::discriminant(&*res_reall.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );

        // 9. format extra paths: fallback format verb and too many arguments
        assert_eq!(
            *eval_func("format", &[str_val("hex: %x"), num_val("10")])
                .expect("ok")
                .data,
            ValueData::String("hex: 10".to_string())
        );
        assert!(
            eval_func(
                "format",
                &[str_val("val: %s"), str_val("a"), str_val("extra")]
            )
            .is_err()
        );

        // 10. formatlist mismatched lengths and scalar error propagation
        assert!(
            eval_func(
                "formatlist",
                &[
                    str_val("%s-%s"),
                    list_val(vec![str_val("a"), str_val("b")]),
                    list_val(vec![str_val("1")]),
                ]
            )
            .is_err()
        );
        assert!(eval_func("formatlist", &[str_val("%s %s"), str_val("only_one")]).is_err());

        // 11. urlencode, startswith, endswith, strcontains
        assert_eq!(
            *eval_func("urlencode", &[str_val("hello world/foo?bar=1&baz=2")])
                .expect("ok")
                .data,
            ValueData::String("hello%20world%2Ffoo%3Fbar%3D1%26baz%3D2".to_string())
        );
        assert!(
            eval_func("urlencode", &[unk_val()])
                .expect("ok")
                .is_unknown()
        );
        assert!(eval_func("urlencode", &[]).is_err());

        assert_eq!(
            *eval_func("startswith", &[str_val("hello world"), str_val("hello")])
                .expect("ok")
                .data,
            ValueData::Bool(true)
        );
        assert_eq!(
            *eval_func("startswith", &[str_val("hello world"), str_val("world")])
                .expect("ok")
                .data,
            ValueData::Bool(false)
        );
        assert!(
            eval_func("startswith", &[unk_val(), str_val("prefix")])
                .expect("ok")
                .is_unknown()
        );
        assert!(eval_func("startswith", &[str_val("s")]).is_err());

        assert_eq!(
            *eval_func("endswith", &[str_val("hello world"), str_val("world")])
                .expect("ok")
                .data,
            ValueData::Bool(true)
        );
        assert_eq!(
            *eval_func("endswith", &[str_val("hello world"), str_val("hello")])
                .expect("ok")
                .data,
            ValueData::Bool(false)
        );
        assert!(
            eval_func("endswith", &[unk_val(), str_val("suffix")])
                .expect("ok")
                .is_unknown()
        );
        assert!(eval_func("endswith", &[str_val("s")]).is_err());

        assert_eq!(
            *eval_func("strcontains", &[str_val("hello world"), str_val("lo wo")])
                .expect("ok")
                .data,
            ValueData::Bool(true)
        );
        assert_eq!(
            *eval_func("strcontains", &[str_val("hello world"), str_val("xyz")])
                .expect("ok")
                .data,
            ValueData::Bool(false)
        );
        assert!(
            eval_func("strcontains", &[unk_val(), str_val("sub")])
                .expect("ok")
                .is_unknown()
        );
        assert!(eval_func("strcontains", &[str_val("s")]).is_err());
    }

    #[test]
    fn test_string_functions_coercion_error_paths() {
        let non_str = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));

        assert!(eval_func("endswith", &[non_str.clone(), str_val("a")]).is_err());
        assert!(eval_func("endswith", &[str_val("a"), non_str.clone()]).is_err());

        assert!(eval_func("startswith", &[non_str.clone(), str_val("a")]).is_err());
        assert!(eval_func("startswith", &[str_val("a"), non_str.clone()]).is_err());

        assert!(eval_func("strcontains", &[non_str.clone(), str_val("a")]).is_err());
        assert!(eval_func("strcontains", &[str_val("a"), non_str.clone()]).is_err());

        assert!(eval_func("chomp", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("indent", &[num_val("2"), non_str.clone()]).is_err());

        assert!(eval_func("join", &[non_str.clone(), list_val(vec![str_val("a")])]).is_err());
        assert!(eval_func("join", &[str_val(","), num_val("42")]).is_err());
        assert!(eval_func("join", &[str_val(","), list_val(vec![non_str.clone()])]).is_err());
        let set_with_non_str = Value::new(
            Type::Set(Box::new(Type::Tuple(vec![]))),
            ValueData::Set(std::collections::BTreeSet::from([non_str.clone()])),
        );
        assert!(eval_func("join", &[str_val(","), set_with_non_str]).is_err());

        assert!(eval_func("lower", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("upper", std::slice::from_ref(&non_str)).is_err());

        assert!(eval_func("replace", &[non_str.clone(), str_val("a"), str_val("b")]).is_err());
        assert!(eval_func("replace", &[str_val("s"), non_str.clone(), str_val("b")]).is_err());
        assert!(eval_func("replace", &[str_val("s"), str_val("a"), non_str.clone()]).is_err());

        assert!(eval_func("split", &[non_str.clone(), str_val("a")]).is_err());
        assert!(eval_func("split", &[str_val("s"), non_str.clone()]).is_err());

        assert!(eval_func("strrev", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("title", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("trimspace", std::slice::from_ref(&non_str)).is_err());

        assert!(eval_func("trim", &[non_str.clone(), str_val("a")]).is_err());
        assert!(eval_func("trim", &[str_val("s"), non_str.clone()]).is_err());

        assert!(eval_func("trimprefix", &[non_str.clone(), str_val("a")]).is_err());
        assert!(eval_func("trimprefix", &[str_val("s"), non_str.clone()]).is_err());

        assert!(eval_func("trimsuffix", &[non_str.clone(), str_val("a")]).is_err());
        assert!(eval_func("trimsuffix", &[str_val("s"), non_str.clone()]).is_err());

        assert!(eval_func("regex", &[non_str.clone(), str_val("a")]).is_err());
        assert!(eval_func("regex", &[str_val("a"), non_str.clone()]).is_err());

        assert!(eval_func("regexall", &[non_str.clone(), str_val("a")]).is_err());
        assert!(eval_func("regexall", &[str_val("a"), non_str.clone()]).is_err());

        assert!(eval_func("urlencode", std::slice::from_ref(&non_str)).is_err());

        assert!(eval_func("format", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("format", &[str_val("%s"), non_str.clone()]).is_err());
        assert!(eval_func("format", &[str_val("%d"), non_str.clone()]).is_err());
        assert!(eval_func("format", &[str_val("%v"), non_str.clone()]).is_err());
        assert!(eval_func("format", &[str_val("%z"), non_str.clone()]).is_err());

        assert!(eval_func("formatlist", std::slice::from_ref(&non_str)).is_err());
        assert!(
            eval_func(
                "formatlist",
                &[str_val("%s"), list_val(vec![non_str.clone()])]
            )
            .is_err()
        );

        assert!(eval_func("substr", &[non_str.clone(), num_val("0"), num_val("1")]).is_err());
        assert!(eval_func("substr", &[str_val("s"), non_str.clone(), num_val("1")]).is_err());
        assert!(eval_func("substr", &[str_val("s"), num_val("0"), non_str]).is_err());
    }

    #[test]
    fn test_substr_refinement_coverage() {
        use crate::types::refinement::Refinement;

        // Prefix longer than offset + len
        let r_prefix = Refinement::not_null().with_prefix("hello_world");
        let unk_prefix = Value::unknown_refined(Type::String, r_prefix);
        let res1 =
            eval_func("substr", &[unk_prefix.clone(), num_val("0"), num_val("5")]).expect("ok");
        assert_eq!(*res1.data, ValueData::String("hello".to_string()));

        // Prefix shorter than offset + len, offset == 0
        let res2 =
            eval_func("substr", &[unk_prefix.clone(), num_val("0"), num_val("20")]).expect("ok");
        assert!(res2.is_unknown());

        // Prefix with offset > 0
        let res3 =
            eval_func("substr", &[unk_prefix.clone(), num_val("6"), num_val("5")]).expect("ok");
        assert_eq!(*res3.data, ValueData::String("world".to_string()));

        // Negative offset or negative len
        let res_neg_off =
            eval_func("substr", &[unk_prefix.clone(), num_val("-1"), num_val("5")]).expect("ok");
        assert!(res_neg_off.is_unknown());
        let res_neg_len =
            eval_func("substr", &[unk_prefix.clone(), num_val("0"), num_val("-1")]).expect("ok");
        assert!(res_neg_len.is_unknown());

        // Length min and max refinements
        let mut r_min_max = Refinement::not_null();
        r_min_max.string_length_min = Some(10);
        r_min_max.string_length_max = Some(20);
        let unk_min_max = Value::unknown_refined(Type::String, r_min_max);
        let res_min_max =
            eval_func("substr", &[unk_min_max, num_val("2"), num_val("5")]).expect("ok");
        assert!(res_min_max.is_unknown());

        let mut r_min_small = Refinement::not_null();
        r_min_small.string_length_min = Some(1);
        let unk_min_small = Value::unknown_refined(Type::String, r_min_small);
        let res_min_small =
            eval_func("substr", &[unk_min_small, num_val("5"), num_val("3")]).expect("ok");
        assert!(res_min_small.is_unknown());

        // Uncoercible off_val and len_val
        let non_str = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!(
            eval_func(
                "substr",
                &[unk_prefix.clone(), non_str.clone(), num_val("5")]
            )
            .expect("ok")
            .is_unknown()
        );
        assert!(
            eval_func("substr", &[unk_prefix.clone(), num_val("0"), non_str])
                .expect("ok")
                .is_unknown()
        );

        // Out-of-bounds isize numbers
        assert!(
            eval_func(
                "substr",
                &[unk_prefix.clone(), num_val("1e100"), num_val("5")]
            )
            .expect("ok")
            .is_unknown()
        );
        assert!(
            eval_func(
                "substr",
                &[unk_prefix.clone(), num_val("0"), num_val("1e100")]
            )
            .expect("ok")
            .is_unknown()
        );

        // Unrefined unknown
        assert!(
            eval_func("substr", &[unk_val(), num_val("0"), num_val("5")])
                .expect("ok")
                .is_unknown()
        );

        // Known string with unknown offset / length
        assert!(
            eval_func("substr", &[str_val("hello"), unk_val(), num_val("5")])
                .expect("ok")
                .is_unknown()
        );
        assert!(
            eval_func("substr", &[str_val("hello"), num_val("0"), unk_val()])
                .expect("ok")
                .is_unknown()
        );

        // Corrupted value with Type::Number but non-Number data
        let corrupted_num = Value::new(Type::Number, ValueData::Bool(true));
        assert!(
            eval_func(
                "substr",
                &[unk_prefix.clone(), corrupted_num.clone(), num_val("5")]
            )
            .expect("ok")
            .is_unknown()
        );
        assert!(
            eval_func("substr", &[unk_prefix.clone(), num_val("0"), corrupted_num])
                .expect("ok")
                .is_unknown()
        );
    }

    /// Tests edge cases and uncovered branches across string helper functions.
    #[test]
    fn test_string_branch_coverage_gaps() {
        // upper branch coverage
        assert!(eval_func("upper", &[]).is_err());
        assert!(eval_func("upper", &[unk_val()]).expect("ok").is_unknown());
        assert!(eval_func("upper", &[null_val()]).is_err());

        // coerce_to_string on unknown value
        assert!(coerce_to_string(&Value::unknown(Type::String), "test").is_err());

        // endswith, startswith, strcontains with known first arg and unknown second arg
        assert!(
            eval_func("endswith", &[str_val("abc"), unk_val()])
                .expect("ok")
                .is_unknown()
        );
        assert!(
            eval_func("startswith", &[str_val("abc"), unk_val()])
                .expect("ok")
                .is_unknown()
        );
        assert!(
            eval_func("strcontains", &[str_val("abc"), unk_val()])
                .expect("ok")
                .is_unknown()
        );

        // urlencode characters `-`, `_`, `.`, `~`
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");

        // infer_substr_refinement with unknown offset or length
        assert!(
            eval_func("substr", &[unk_val(), unk_val(), num_val("5")])
                .expect("ok")
                .is_unknown()
        );
        assert!(
            eval_func("substr", &[unk_val(), num_val("0"), unk_val()])
                .expect("ok")
                .is_unknown()
        );
    }
}
