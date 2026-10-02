//! Type Conversion standard library functions.

use crate::eval::func::Function;
use crate::types::unify::unify;
use crate::types::{Type, Value, ValueData, ValueMark};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

#[must_use]
/// Get functions
pub fn functions() -> Vec<Function> {
    vec![
        can_func(),
        issensitive_func(),
        nonsensitive_func(),
        sensitive_func(),
        tobool_func(),
        tolist_func(),
        tomap_func(),
        tonumber_func(),
        toset_func(),
        tostring_func(),
        try_func(),
        type_func(),
    ]
}

// can, try, and type are special forms that require unevaluated ASTs.
// In our function mapping, they receive evaluated arguments, which means
// errors during evaluation will cause the entire expression to fail before
// these functions are called.
// We provide them here as identity/stub functions for API completeness,
// but true lazy evaluation would need AST walker integration.

fn can_func() -> Function {
    Function {
        name: "can".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("can expects 1 argument".to_string());
            }
            // If we reached here, the argument evaluated successfully.
            Ok(Value::new(Type::Bool, ValueData::Bool(true)))
        }),
        signature: None,
    }
}

fn try_func() -> Function {
    Function {
        name: "try".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.is_empty() {
                return Err("try expects at least 1 argument".to_string());
            }
            // If we reached here, all arguments evaluated successfully.
            // We just return the first one.
            Ok(args[0].clone())
        }),
        signature: None,
    }
}

fn type_func() -> Function {
    Function {
        name: "type".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("type expects 1 argument".to_string());
            }
            // Return string representation of the type
            Ok(Value::new(
                Type::String,
                ValueData::String(args[0].ty().to_string()),
            ))
        }),
        signature: None,
    }
}

fn tobool_func() -> Function {
    Function {
        name: "tobool".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("tobool expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Bool));
            }

            args[0]
                .clone()
                .coerce(&Type::Bool)
                .map_err(|_| format!("cannot convert {} to bool", args[0].ty()))
        }),
        signature: None,
    }
}

fn tolist_func() -> Function {
    Function {
        name: "tolist".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("tolist expects 1 argument".to_string());
            }

            let val = &args[0];
            if val.is_unknown() {
                return Ok(Value::unknown(Type::List(Box::new(Type::Dynamic))));
            }

            match &*val.data {
                ValueData::Array(_arr) => Ok(val.clone()),
                ValueData::Set(set) => {
                    let mut list = Vec::new();
                    let mut res_ty = None;
                    for item in set {
                        list.push(item.clone());
                        if res_ty.is_none() {
                            res_ty = Some(item.ty().clone());
                        }
                    }
                    let t = res_ty.unwrap_or(Type::Dynamic);
                    Ok(Value::new(Type::List(Box::new(t)), ValueData::Array(list)))
                }
                _ => {
                    let t = val.ty().clone();
                    Ok(Value::new(
                        Type::List(Box::new(t)),
                        ValueData::Array(vec![val.clone()]),
                    ))
                }
            }
        }),
        signature: None,
    }
}

fn tomap_func() -> Function {
    Function {
        name: "tomap".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("tomap expects 1 argument".to_string());
            }

            let val = &args[0];
            if val.is_unknown() {
                return Ok(Value::unknown(Type::Map(Box::new(Type::Dynamic))));
            }

            match &*val.data {
                ValueData::Object(obj) => {
                    if let Type::Object { attrs, .. } = val.ty() {
                        let mut res_ty = None;
                        for t in attrs.values() {
                            res_ty = match res_ty {
                                None => Some(t.clone()),
                                Some(ref current) => unify(current, t),
                            };
                        }
                        let t = res_ty.unwrap_or(Type::Dynamic);
                        let mut map = BTreeMap::new();
                        for (k, v) in obj {
                            if let Ok(coerced) = v.clone().coerce(&t) {
                                map.insert(k.clone(), coerced);
                            } else {
                                return Err(format!(
                                    "cannot convert object value {} to {}",
                                    v.ty(),
                                    t
                                ));
                            }
                        }
                        Ok(Value::new(Type::Map(Box::new(t)), ValueData::Object(map)))
                    } else {
                        Ok(val.clone())
                    }
                }
                _ => Err("cannot convert to map".to_string()),
            }
        }),
        signature: None,
    }
}

fn tonumber_func() -> Function {
    Function {
        name: "tonumber".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("tonumber expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }

            args[0]
                .clone()
                .coerce(&Type::Number)
                .map_err(|_| format!("cannot convert {} to number", args[0].ty()))
        }),
        signature: None,
    }
}

fn toset_func() -> Function {
    Function {
        name: "toset".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("toset expects 1 argument".to_string());
            }

            let val = &args[0];
            if val.is_unknown() {
                return Ok(Value::unknown(Type::Set(Box::new(Type::Dynamic))));
            }

            match &*val.data {
                ValueData::Set(_set) => Ok(val.clone()),
                ValueData::Array(arr) => {
                    let mut set = BTreeSet::new();
                    let mut res_ty = None;
                    for item in arr {
                        set.insert(item.clone());
                        if res_ty.is_none() {
                            res_ty = Some(item.ty().clone());
                        }
                    }
                    let t = res_ty.unwrap_or(Type::Dynamic);
                    Ok(Value::new(Type::Set(Box::new(t)), ValueData::Set(set)))
                }
                _ => {
                    let t = val.ty().clone();
                    let mut set = BTreeSet::new();
                    set.insert(val.clone());
                    Ok(Value::new(Type::Set(Box::new(t)), ValueData::Set(set)))
                }
            }
        }),
        signature: None,
    }
}

fn sensitive_func() -> Function {
    Function {
        name: "sensitive".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("sensitive expects 1 argument".to_string());
            }
            Ok(args[0].mark(ValueMark::Sensitive))
        }),
        signature: None,
    }
}

fn nonsensitive_func() -> Function {
    Function {
        name: "nonsensitive".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("nonsensitive expects 1 argument".to_string());
            }
            Ok(args[0].unmark_sensitive())
        }),
        signature: None,
    }
}

fn issensitive_func() -> Function {
    Function {
        name: "issensitive".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("issensitive expects 1 argument".to_string());
            }
            Ok(Value::new(
                Type::Bool,
                ValueData::Bool(args[0].is_sensitive()),
            ))
        }),
        signature: None,
    }
}

fn tostring_func() -> Function {
    Function {
        name: "tostring".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("tostring expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            args[0]
                .clone()
                .coerce(&Type::String)
                .map_err(|_| format!("cannot convert {} to string", args[0].ty()))
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

    use crate::eval::stdlib::conversion::*;
    use crate::number::Number;
    use crate::types::{Type, Value, ValueData};
    use std::collections::{BTreeMap, BTreeSet};
    use std::str::FromStr;

    fn eval_func(name: &str, args: &[Value]) -> Result<Value, String> {
        let funcs = functions();
        for f in funcs {
            if f.name == name {
                return (f.func)(args);
            }
        }
        panic!("Function {name} not found");
    }

    #[test]
    fn test_can() {
        assert!(
            eval_func(
                "can",
                &[Value::new(Type::String, ValueData::String(String::new()))]
            )
            .is_ok()
        );
        assert!(eval_func("can", &[]).is_err());
    }

    #[test]
    fn test_try() {
        assert!(eval_func("try", &[]).is_err());
        let val = Value::new(Type::String, ValueData::String("hello".to_string()));
        let res = eval_func("try", std::slice::from_ref(&val)).unwrap();
        assert_eq!(res, val);
    }

    #[test]
    fn test_type() {
        assert!(eval_func("type", &[]).is_err());
        let val = Value::new(Type::String, ValueData::String("hello".to_string()));
        let res = eval_func("type", &[val]).unwrap();
        assert_eq!(
            res,
            Value::new(Type::String, ValueData::String("string".to_string()))
        );
    }

    #[test]
    fn test_tobool() {
        assert!(eval_func("tobool", &[]).is_err());
        assert!(
            eval_func("tobool", &[Value::unknown(Type::String)])
                .unwrap()
                .is_unknown()
        );

        let val = Value::new(Type::String, ValueData::String("true".to_string()));
        let res = eval_func("tobool", &[val]).unwrap();
        assert_eq!(res, Value::new(Type::Bool, ValueData::Bool(true)));

        let val = Value::new(Type::String, ValueData::String("abc".to_string()));
        assert!(eval_func("tobool", &[val]).is_err());
    }

    #[test]
    fn test_tolist() {
        assert!(eval_func("tolist", &[]).is_err());
        assert!(
            eval_func("tolist", &[Value::unknown(Type::String)])
                .unwrap()
                .is_unknown()
        );

        let arr = vec![Value::new(Type::String, ValueData::String("a".to_string()))];
        let val = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(arr.clone()),
        );
        let res = eval_func("tolist", &[val]).unwrap();
        assert_eq!(res.ty(), &Type::List(Box::new(Type::String)));

        let mut set = BTreeSet::new();
        set.insert(Value::new(Type::String, ValueData::String("a".to_string())));
        let val = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set));
        let res = eval_func("tolist", &[val]).unwrap();
        assert_eq!(res.ty(), &Type::List(Box::new(Type::String)));

        let mut set_multi = BTreeSet::new();
        set_multi.insert(Value::new(Type::String, ValueData::String("a".to_string())));
        set_multi.insert(Value::new(Type::Dynamic, ValueData::Null));
        assert_eq!(set_multi.len(), 2);
        let val_multi = Value::new(
            Type::Set(Box::new(Type::Dynamic)),
            ValueData::Set(set_multi),
        );
        let res_multi = eval_func("tolist", &[val_multi]).unwrap();
        assert_eq!(res_multi.ty(), &Type::List(Box::new(Type::String)));

        let val = Value::new(Type::String, ValueData::String("a".to_string()));
        let res = eval_func("tolist", &[val]).unwrap();
        assert_eq!(res.ty(), &Type::List(Box::new(Type::String)));
    }

    #[test]
    fn test_tomap_no_conversion_needed() {
        use crate::types::{Type, Value, ValueData};
        let f = tomap_func();

        let mut obj = std::collections::BTreeMap::new();
        obj.insert(
            "a".to_string(),
            Value::new(Type::String, ValueData::String("b".to_string())),
        );
        let m = Value::new(Type::Map(Box::new(Type::String)), ValueData::Object(obj));

        let res = (f.func)(&[m]);
        assert!(res.is_ok());
    }

    #[test]
    fn test_tomap() {
        assert!(eval_func("tomap", &[]).is_err());
        assert!(
            eval_func("tomap", &[Value::unknown(Type::String)])
                .unwrap()
                .is_unknown()
        );

        let mut obj = BTreeMap::new();
        obj.insert(
            "k".to_string(),
            Value::new(Type::String, ValueData::String("v".to_string())),
        );

        let mut type_obj = BTreeMap::new();
        type_obj.insert("k".to_string(), Type::String);

        let val = Value::new(Type::object(type_obj), ValueData::Object(obj));
        let res = eval_func("tomap", &[val]).unwrap();
        assert_eq!(res.ty(), &Type::Map(Box::new(Type::String)));

        let val = Value::new(Type::String, ValueData::String("a".to_string()));
        assert!(eval_func("tomap", &[val]).is_err());
    }

    #[test]
    fn test_tomap_coercion_error() {
        let mut obj = BTreeMap::new();
        obj.insert(
            "k".to_string(),
            Value::new(Type::String, ValueData::String("v".to_string())),
        );
        obj.insert(
            "k2".to_string(),
            Value::new(
                Type::Number,
                ValueData::Number(Number::from_str("1").unwrap()),
            ),
        );

        let mut type_obj = BTreeMap::new();
        type_obj.insert("k".to_string(), Type::String);
        type_obj.insert("k2".to_string(), Type::Number);

        let val = Value::new(Type::object(type_obj), ValueData::Object(obj));
        let res = eval_func("tomap", &[val]).unwrap();
        assert_eq!(res.ty(), &Type::Map(Box::new(Type::String)));

        let mut obj2 = BTreeMap::new();
        obj2.insert(
            "k".to_string(),
            Value::new(Type::String, ValueData::String("v".to_string())),
        );
        obj2.insert(
            "k2".to_string(),
            Value::new(
                Type::Tuple(vec![Type::String]),
                ValueData::Array(vec![Value::new(
                    Type::String,
                    ValueData::String("v".to_string()),
                )]),
            ),
        );

        let mut type_obj2 = BTreeMap::new();
        type_obj2.insert("k".to_string(), Type::String);
        type_obj2.insert("k2".to_string(), Type::Tuple(vec![Type::String]));

        let val2 = Value::new(Type::object(type_obj2), ValueData::Object(obj2));
        assert!(eval_func("tomap", &[val2]).is_ok());
    }

    #[test]
    fn test_tonumber() {
        assert!(eval_func("tonumber", &[]).is_err());
        assert!(
            eval_func("tonumber", &[Value::unknown(Type::String)])
                .unwrap()
                .is_unknown()
        );

        let val = Value::new(Type::String, ValueData::String("1".to_string()));
        let res = eval_func("tonumber", &[val]).unwrap();
        assert_eq!(res.ty(), &Type::Number);

        let val = Value::new(Type::String, ValueData::String("abc".to_string()));
        assert!(eval_func("tonumber", &[val]).is_err());
    }

    #[test]
    fn test_toset() {
        assert!(eval_func("toset", &[]).is_err());
        assert!(
            eval_func("toset", &[Value::unknown(Type::String)])
                .unwrap()
                .is_unknown()
        );

        let mut set = BTreeSet::new();
        set.insert(Value::new(Type::String, ValueData::String("a".to_string())));
        let val = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set));
        let res = eval_func("toset", &[val]).unwrap();
        assert_eq!(res.ty(), &Type::Set(Box::new(Type::String)));

        let arr = vec![Value::new(Type::String, ValueData::String("a".to_string()))];
        let val = Value::new(Type::List(Box::new(Type::String)), ValueData::Array(arr));
        let res = eval_func("toset", &[val]).unwrap();
        assert_eq!(res.ty(), &Type::Set(Box::new(Type::String)));

        let arr_multi = vec![
            Value::new(Type::String, ValueData::String("a".to_string())),
            Value::new(Type::String, ValueData::String("b".to_string())),
        ];
        let val_multi = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(arr_multi),
        );
        let res_multi = eval_func("toset", &[val_multi]).unwrap();
        assert_eq!(res_multi.ty(), &Type::Set(Box::new(Type::String)));

        let val = Value::new(Type::String, ValueData::String("a".to_string()));
        let res = eval_func("toset", &[val]).unwrap();
        assert_eq!(res.ty(), &Type::Set(Box::new(Type::String)));
    }

    #[test]
    fn test_tostring() {
        assert!(eval_func("tostring", &[]).is_err());
        assert!(
            eval_func("tostring", &[Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );

        let val = Value::new(
            Type::Number,
            ValueData::Number(Number::from_str("1").unwrap()),
        );
        let res = eval_func("tostring", &[val]).unwrap();
        assert_eq!(res.ty(), &Type::String);

        let val = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!(eval_func("tostring", &[val]).is_err());
    }
    #[test]
    #[should_panic(expected = "Function notexist not found")]
    fn test_eval_func_not_found() {
        eval_func("notexist", &[]).unwrap();
    }

    #[test]
    fn test_tomap_errors() {
        use crate::number::Number;
        use bigdecimal::BigDecimal;
        use std::collections::BTreeMap;
        use std::str::FromStr;

        let num_val = |s: &str| -> Value {
            Value::new(
                Type::Number,
                ValueData::Number(Number::new(BigDecimal::from_str(s).unwrap())),
            )
        };
        let list_val = |vals: Vec<Value>| -> Value {
            Value::new(Type::List(Box::new(Type::Dynamic)), ValueData::Array(vals))
        };

        let mut attrs = BTreeMap::new();
        attrs.insert("a".to_string(), Type::List(Box::new(Type::Number)));
        attrs.insert("b".to_string(), Type::Tuple(vec![Type::Number]));

        let mut obj = BTreeMap::new();
        obj.insert("a".to_string(), list_val(vec![]));
        obj.insert(
            "b".to_string(),
            Value::new(
                Type::Tuple(vec![Type::Number]),
                ValueData::Array(vec![num_val("1")]),
            ),
        );

        let val = Value::new(Type::object(attrs), ValueData::Object(obj));

        let res = eval_func("tomap", &[val]);
        assert!(res.is_err());

        let unk_obj = Value::unknown(Type::object(BTreeMap::new()));
        let res2 = eval_func("tomap", std::slice::from_ref(&unk_obj)).unwrap();
        assert!(res2.is_unknown());
    }

    #[test]
    fn test_sensitivity_functions() {
        let val = Value::new(Type::String, ValueData::String("secret".to_string()));

        // issensitive on non-sensitive value
        let is_sens = eval_func("issensitive", std::slice::from_ref(&val)).unwrap();
        assert_eq!(is_sens, Value::new(Type::Bool, ValueData::Bool(false)));

        // sensitive(val)
        let sens_val = eval_func("sensitive", std::slice::from_ref(&val)).unwrap();
        assert!(sens_val.has_mark(&ValueMark::Sensitive));
        assert!(sens_val.is_sensitive());

        // issensitive on sensitive value
        let is_sens2 = eval_func("issensitive", std::slice::from_ref(&sens_val)).unwrap();
        assert_eq!(is_sens2, Value::new(Type::Bool, ValueData::Bool(true)));

        // nonsensitive(sens_val)
        let non_sens = eval_func("nonsensitive", std::slice::from_ref(&sens_val)).unwrap();
        assert!(!non_sens.has_mark(&ValueMark::Sensitive));
        assert!(!non_sens.is_sensitive());
        assert_eq!(non_sens, val);

        // Error paths: wrong arg counts
        assert!(eval_func("sensitive", &[]).is_err());
        assert!(eval_func("sensitive", &[val.clone(), val.clone()]).is_err());
        assert!(eval_func("nonsensitive", &[]).is_err());
        assert!(eval_func("nonsensitive", &[val.clone(), val.clone()]).is_err());
        assert!(eval_func("issensitive", &[]).is_err());
        assert!(eval_func("issensitive", &[val.clone(), val.clone()]).is_err());
    }
}
