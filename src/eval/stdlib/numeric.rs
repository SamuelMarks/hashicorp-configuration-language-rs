//! Numeric standard library functions.

use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
use crate::number::Number;
use crate::types::{Type, Value, ValueData};
use bigdecimal::num_bigint::BigInt;
use bigdecimal::num_traits::Num;
use bigdecimal::num_traits::Signed;
use bigdecimal::num_traits::ToPrimitive;
use bigdecimal::{BigDecimal, RoundingMode};
use std::str::FromStr;
use std::sync::Arc;

/// Return all numeric standard library functions.
#[must_use]
pub fn functions() -> Vec<Function> {
    vec![
        abs_func(),
        ceil_func(),
        floor_func(),
        max_func(),
        min_func(),
        parseint_func(),
        pow_func(),
        signum_func(),
        log_func(),
        range_func(),
    ]
}

fn range_func() -> Function {
    Function {
        name: "range".to_string(),
        func: Arc::new(|args| {
            if args.is_empty() || args.len() > 3 {
                return Err("range() takes 1, 2, or 3 arguments".to_string());
            }

            for arg in args {
                if arg.is_unknown() {
                    return Ok(Value::unknown(Type::List(Box::new(Type::Number))));
                }
            }

            let (start, limit, step) = match args.len() {
                1 => {
                    let limit = coerce_to_number(&args[0], "range")?;
                    let zero = BigDecimal::from(0);
                    let step = if limit >= zero {
                        BigDecimal::from(1)
                    } else {
                        BigDecimal::from(-1)
                    };
                    (zero, limit, step)
                }
                2 => {
                    let start = coerce_to_number(&args[0], "range")?;
                    let limit = coerce_to_number(&args[1], "range")?;
                    let step = if start <= limit {
                        BigDecimal::from(1)
                    } else {
                        BigDecimal::from(-1)
                    };
                    (start, limit, step)
                }
                _ => {
                    let start = coerce_to_number(&args[0], "range")?;
                    let limit = coerce_to_number(&args[1], "range")?;
                    let step = coerce_to_number(&args[2], "range")?;
                    if step == 0 {
                        return Err("range() step cannot be zero".to_string());
                    }
                    (start, limit, step)
                }
            };

            let zero = BigDecimal::from(0);
            let mut current = start;
            let mut items = Vec::new();
            let mut count = 0;
            const MAX_RANGE_ELEMENTS: usize = 100_000;

            if step > zero {
                while current < limit {
                    if count >= MAX_RANGE_ELEMENTS {
                        return Err("range() generated too many elements".to_string());
                    }
                    items.push(Value::new(
                        Type::Number,
                        ValueData::Number(Number(current.clone())),
                    ));
                    current += &step;
                    count += 1;
                }
            } else {
                while current > limit {
                    if count >= MAX_RANGE_ELEMENTS {
                        return Err("range() generated too many elements".to_string());
                    }
                    items.push(Value::new(
                        Type::Number,
                        ValueData::Number(Number(current.clone())),
                    ));
                    current += &step;
                    count += 1;
                }
            }

            Ok(Value::new(
                Type::List(Box::new(Type::Number)),
                ValueData::Array(items),
            ))
        }),
        signature: Some(
            FunctionSignature::with_static_return_type(
                vec![FunctionParamSpec::new("args", Type::Number)],
                Type::List(Box::new(Type::Number)),
            )
            .with_variadic(FunctionParamSpec::new("rest", Type::Number)),
        ),
    }
}

fn coerce_to_number(arg: &Value, name: &str) -> Result<BigDecimal, String> {
    let coerced = arg
        .clone()
        .coerce(&Type::Number)
        .map_err(|_| format!("{} requires a number, got type {}", name, arg.ty()))?;
    if let ValueData::Number(num) = *coerced.data {
        Ok(num.0)
    } else {
        Err(format!("{name} requires a number"))
    }
}

fn coerce_to_number_list(args: &[Value], name: &str) -> Result<Vec<BigDecimal>, String> {
    let mut nums = Vec::new();
    for arg in args {
        if arg.ty().is_collection() {
            if let ValueData::Array(arr) = &*arg.data {
                for item in arr {
                    nums.push(coerce_to_number(item, name)?);
                }
            } else if let ValueData::Set(set) = &*arg.data {
                for item in set {
                    nums.push(coerce_to_number(item, name)?);
                }
            }
        } else {
            nums.push(coerce_to_number(arg, name)?);
        }
    }
    if nums.is_empty() {
        return Err(format!("{name} requires at least one number"));
    }
    Ok(nums)
}

fn abs_func() -> Function {
    Function {
        name: "abs".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("abs expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }
            if args[0].is_null() {
                return Err("abs cannot be called with null".to_string());
            }

            let num = coerce_to_number(&args[0], "abs")?;
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(num.abs())),
            ))
        }),
        signature: None,
    }
}

fn ceil_func() -> Function {
    Function {
        name: "ceil".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("ceil expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }
            if args[0].is_null() {
                return Err("ceil cannot be called with null".to_string());
            }

            let num = coerce_to_number(&args[0], "ceil")?;
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(num.with_scale_round(0, RoundingMode::Ceiling))),
            ))
        }),
        signature: None,
    }
}

fn floor_func() -> Function {
    Function {
        name: "floor".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("floor expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }
            if args[0].is_null() {
                return Err("floor cannot be called with null".to_string());
            }

            let num = coerce_to_number(&args[0], "floor")?;
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(num.with_scale_round(0, RoundingMode::Floor))),
            ))
        }),
        signature: None,
    }
}

fn max_func() -> Function {
    Function {
        name: "max".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::Number));
            }
            let nums = coerce_to_number_list(args, "max")?;
            let mut it = nums.into_iter();
            let mut max_val = it.next().unwrap_or_default();
            for n in it {
                if n > max_val {
                    max_val = n;
                }
            }
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(max_val)),
            ))
        }),
        signature: None,
    }
}

fn min_func() -> Function {
    Function {
        name: "min".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::Number));
            }
            let nums = coerce_to_number_list(args, "min")?;
            let mut it = nums.into_iter();
            let mut min_val = it.next().unwrap_or_default();
            for n in it {
                if n < min_val {
                    min_val = n;
                }
            }
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(min_val)),
            ))
        }),
        signature: None,
    }
}

fn signum_func() -> Function {
    Function {
        name: "signum".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err(format!("signum expects 1 argument, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }
            if args[0].is_null() {
                return Err("signum cannot be called with null".to_string());
            }

            let num = coerce_to_number(&args[0], "signum")?;
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(num.signum())),
            ))
        }),
        signature: None,
    }
}

fn parseint_func() -> Function {
    Function {
        name: "parseint".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("parseint expects 2 arguments, got {}", args.len()));
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }
            if args[0].is_null() || args[1].is_null() {
                return Err("parseint cannot be called with null".to_string());
            }

            let s = args[0]
                .clone()
                .coerce(&Type::String)
                .map_err(|_| "parseint requires a string for first argument".to_string())?;
            let s_val = match *s.data {
                ValueData::String(ref s_str) => s_str.clone(),
                _ => return Err("parseint requires a string for first argument".to_string()),
            };

            let base_num = coerce_to_number(&args[1], "parseint base")?;
            let base = base_num
                .to_u32()
                .ok_or_else(|| "parseint base must be a valid u32".to_string())?;
            if !(2..=62).contains(&base) {
                return Err("parseint base must be between 2 and 62".to_string());
            }

            let bi = BigInt::from_str_radix(&s_val, base)
                .map_err(|_| format!("cannot parse '{s_val}' with base {base}"))?;
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(BigDecimal::from(bi))),
            ))
        }),
        signature: None,
    }
}

fn pow_func() -> Function {
    Function {
        name: "pow".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("pow expects 2 arguments, got {}", args.len()));
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }
            if args[0].is_null() || args[1].is_null() {
                return Err("pow cannot be called with null".to_string());
            }

            let num1 = coerce_to_number(&args[0], "pow")?;
            let num2 = coerce_to_number(&args[1], "pow")?;

            let f1 = num1.to_f64().unwrap_or_default();
            let f2 = num2.to_f64().unwrap_or_default();

            let res = f1.powf(f2);
            if res.is_nan() || res.is_infinite() {
                return Err("pow resulted in infinity or NaN".to_string());
            }

            let res_bd = BigDecimal::from_str(&res.to_string()).unwrap_or_default();
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(res_bd)),
            ))
        }),
        signature: None,
    }
}

fn log_func() -> Function {
    Function {
        name: "log".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("log expects 2 arguments, got {}", args.len()));
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }
            if args[0].is_null() || args[1].is_null() {
                return Err("log cannot be called with null".to_string());
            }

            let num1 = coerce_to_number(&args[0], "log num")?;
            let num2 = coerce_to_number(&args[1], "log base")?;

            let f1 = num1.to_f64().unwrap_or_default();
            let f2 = num2.to_f64().unwrap_or_default();

            let res = f1.log(f2);
            if res.is_nan() || res.is_infinite() {
                return Err("log resulted in infinity or NaN".to_string());
            }

            let res_bd = BigDecimal::from_str(&res.to_string()).unwrap_or_default();
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(res_bd)),
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
    use bigdecimal::BigDecimal;
    use std::collections::BTreeSet;
    use std::str::FromStr;

    fn get_func(name: &str) -> Function {
        functions().into_iter().find(|f| f.name == name).unwrap()
    }

    fn eval_func(name: &str, args: &[Value]) -> Result<Value, String> {
        let f = get_func(name);
        (f.func)(args)
    }

    fn num_val(s: &str) -> Value {
        Value::new(
            Type::Number,
            ValueData::Number(Number::new(BigDecimal::from_str(s).unwrap())),
        )
    }

    fn str_val(s: &str) -> Value {
        Value::new(Type::String, ValueData::String(s.to_string()))
    }

    fn unk_val() -> Value {
        Value::unknown(Type::Number)
    }

    fn null_val() -> Value {
        Value::new(Type::Number, ValueData::Null)
    }

    #[test]
    fn test_abs() {
        assert_eq!(
            eval_func("abs", &[num_val("-5.5")]).unwrap().data.as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("5.5").unwrap()))
        );
        assert!(eval_func("abs", &[]).is_err());
        assert!(eval_func("abs", &[num_val("1"), num_val("2")]).is_err());
        assert!(eval_func("abs", &[unk_val()]).unwrap().is_unknown());
        assert!(eval_func("abs", &[null_val()]).is_err());
    }

    #[test]
    fn test_ceil_floor() {
        assert_eq!(
            eval_func("ceil", &[num_val("5.1")]).unwrap().data.as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("6").unwrap()))
        );
        assert!(eval_func("ceil", &[]).is_err());
        assert!(eval_func("ceil", &[unk_val()]).unwrap().is_unknown());
        assert!(eval_func("ceil", &[null_val()]).is_err());

        assert_eq!(
            eval_func("floor", &[num_val("5.9")]).unwrap().data.as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("5").unwrap()))
        );
        assert!(eval_func("floor", &[]).is_err());
        assert!(eval_func("floor", &[unk_val()]).unwrap().is_unknown());
        assert!(eval_func("floor", &[null_val()]).is_err());
    }

    #[test]
    fn test_max_min() {
        assert_eq!(
            eval_func("max", &[num_val("1"), num_val("10"), num_val("5")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("10").unwrap()))
        );
        assert!(
            eval_func("max", &[num_val("1"), unk_val()])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("max", &[]).is_err()); // empty args -> coerce_to_number_list empty
        assert!(eval_func("max", &[null_val()]).is_err()); // coerce fails

        assert_eq!(
            eval_func("min", &[num_val("1"), num_val("10"), num_val("5")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("1").unwrap()))
        );
        assert!(
            eval_func("min", &[num_val("1"), unk_val()])
                .unwrap()
                .is_unknown()
        );
    }

    #[test]
    fn test_coerce_list() {
        let arr = Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(vec![num_val("1"), num_val("2")]),
        );
        assert_eq!(
            eval_func("max", &[arr]).unwrap().data.as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("2").unwrap()))
        );

        let mut set = BTreeSet::new();
        set.insert(num_val("3"));
        let set_val = Value::new(Type::Set(Box::new(Type::Number)), ValueData::Set(set));
        assert_eq!(
            eval_func("max", &[set_val]).unwrap().data.as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("3").unwrap()))
        );

        let empty_arr = Value::new(Type::List(Box::new(Type::Number)), ValueData::Array(vec![]));
        assert!(eval_func("max", &[empty_arr]).is_err());
    }

    #[test]
    fn test_signum() {
        assert_eq!(
            eval_func("signum", &[num_val("-5.5")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("-1").unwrap()))
        );
        assert_eq!(
            eval_func("signum", &[num_val("5.5")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("1").unwrap()))
        );
        assert!(eval_func("signum", &[]).is_err());
        assert!(eval_func("signum", &[unk_val()]).unwrap().is_unknown());
        assert!(eval_func("signum", &[null_val()]).is_err());
    }

    #[test]
    fn test_parseint() {
        assert_eq!(
            eval_func("parseint", &[str_val("ff"), num_val("16")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("255").unwrap()))
        );
        assert!(eval_func("parseint", &[]).is_err());
        assert!(
            eval_func("parseint", &[unk_val(), num_val("10")])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("parseint", &[null_val(), num_val("10")]).is_err());
        assert!(eval_func("parseint", &[num_val("10"), num_val("10")]).is_ok());
        assert!(eval_func("parseint", &[str_val("10"), str_val("10")]).is_ok()); // second arg coerce to num ok
        assert!(eval_func("parseint", &[str_val("10"), num_val("1")]).is_err()); // base < 2
        assert!(eval_func("parseint", &[str_val("zz"), num_val("10")]).is_err()); // invalid parse
    }

    #[test]
    fn test_coerce_list_map() {
        let mut map = std::collections::BTreeMap::new();
        map.insert("a".to_string(), num_val("1"));
        let map_val = Value::new(Type::Map(Box::new(Type::Number)), ValueData::Object(map));
        // max should ignore map elements if they fall through lines 54-ish, or fail because nums is empty
        assert!(eval_func("max", &[map_val]).is_err());
    }

    #[test]
    fn test_pow_inf_nan() {
        assert!(eval_func("pow", &[num_val("0"), num_val("-1")]).is_err()); // inf
        assert!(eval_func("pow", &[num_val("-1"), num_val("0.5")]).is_err()); // NaN
    }

    #[test]
    fn test_log_inf_nan() {
        assert!(eval_func("log", &[num_val("0"), num_val("10")]).is_err()); // -inf
        assert!(eval_func("log", &[num_val("-1"), num_val("10")]).is_err()); // NaN
    }

    #[test]
    fn test_pow_log() {
        assert_eq!(
            eval_func("pow", &[num_val("3"), num_val("2")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("9").unwrap()))
        );
        assert!(eval_func("pow", &[]).is_err());
        assert!(
            eval_func("pow", &[unk_val(), num_val("2")])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("pow", &[null_val(), num_val("2")]).is_err());
        // To cover fallback error for float conversion
        // "1e1000000" or similar may cause to_f64 to return None but BigDecimal's to_f64 never returns None?
        // Wait, bigdecimal to_f64 returns None? I already checked it returns Some(inf) for 1e10000.
        // Wait, what if I pass string to coerce_to_number? It will fail coerce.

        assert_eq!(
            eval_func("log", &[num_val("100"), num_val("10")])
                .unwrap()
                .data
                .as_ref(),
            &ValueData::Number(Number::new(BigDecimal::from_str("2").unwrap()))
        );
        assert!(eval_func("log", &[]).is_err());
        assert!(
            eval_func("log", &[unk_val(), num_val("2")])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("log", &[null_val(), num_val("2")]).is_err());
    }
    #[test]
    #[should_panic]
    fn test_eval_func_not_found() {
        eval_func("notexist", &[]).unwrap();
    }

    #[test]
    fn test_numeric_coverage() {
        let non_val = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        let null_str = Value::new(Type::String, ValueData::Null);
        let null_num = Value::new(Type::Number, ValueData::Null);

        // parseint first arg coercion error and non-string data error
        assert!(eval_func("parseint", &[non_val.clone(), num_val("10")]).is_err());
        assert!(
            eval_func(
                "parseint",
                &[
                    Value::new(Type::Dynamic, ValueData::Bool(true)),
                    num_val("10")
                ]
            )
            .is_err()
        );
        assert!(eval_func("parseint", &[str_val("10"), null_num.clone()]).is_err());
        assert!(eval_func("pow", &[num_val("2"), null_num.clone()]).is_err());
        assert!(eval_func("log", &[num_val("10"), null_num.clone()]).is_err());
        assert!(eval_func("parseint", &[null_str, num_val("10")]).is_err());

        // parseint base error (not valid u32)
        assert!(eval_func("parseint", &[str_val("10"), num_val("-1")]).is_err());
        assert!(eval_func("parseint", &[str_val("10"), num_val("1e100")]).is_err());

        // coerce_to_number error with non-convertible type and null data
        assert!(eval_func("abs", std::slice::from_ref(&non_val)).is_err());
        assert!(eval_func("abs", std::slice::from_ref(&null_num)).is_err());

        assert!(
            eval_func("parseint", &[Value::unknown(Type::String), num_val("10")])
                .unwrap()
                .is_unknown()
        );
        assert!(
            eval_func("parseint", &[str_val("10"), Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("parseint", &[str_val("10"), num_val("1")]).is_err());
        assert!(eval_func("parseint", &[str_val("10"), num_val("63")]).is_err());
        assert!(eval_func("parseint", &[str_val("10"), str_val("bad")]).is_err());
        assert!(eval_func("parseint", &[str_val("10"), num_val("10")]).is_ok());

        assert!(
            eval_func("abs", &[Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("abs", &[str_val("bad")]).is_err());

        assert!(
            eval_func("ceil", &[Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("ceil", &[str_val("bad")]).is_err());

        assert!(
            eval_func("floor", &[Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("floor", &[str_val("bad")]).is_err());

        assert!(
            eval_func("log", &[Value::unknown(Type::Number), num_val("10")])
                .unwrap()
                .is_unknown()
        );
        assert!(
            eval_func("log", &[num_val("10"), Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("log", &[str_val("bad"), num_val("10")]).is_err());
        assert!(eval_func("log", &[num_val("10"), str_val("bad")]).is_err());

        assert!(
            eval_func("pow", &[Value::unknown(Type::Number), num_val("10")])
                .unwrap()
                .is_unknown()
        );
        assert!(
            eval_func("pow", &[num_val("10"), Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("pow", &[str_val("bad"), num_val("10")]).is_err());
        assert!(eval_func("pow", &[num_val("10"), str_val("bad")]).is_err());

        assert!(eval_func("max", &[]).is_err());
        assert!(
            eval_func("max", &[Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("max", &[str_val("bad")]).is_err());

        // Coerce error in single list argument and multi-argument for min/max
        let bad_list = Value::new(
            Type::List(Box::new(Type::Dynamic)),
            ValueData::Array(vec![num_val("1"), str_val("not_a_number")]),
        );
        assert!(eval_func("min", std::slice::from_ref(&bad_list)).is_err());
        assert!(eval_func("max", &[bad_list]).is_err());
        assert!(eval_func("min", &[num_val("1"), str_val("not_a_number")]).is_err());
        assert!(eval_func("max", &[num_val("1"), str_val("not_a_number")]).is_err());

        // Coerce error in Set argument for min/max
        let mut bad_set_items = std::collections::BTreeSet::new();
        bad_set_items.insert(Value::new(Type::Dynamic, ValueData::Null));
        let bad_set = Value::new(
            Type::Set(Box::new(Type::Dynamic)),
            ValueData::Set(bad_set_items),
        );
        assert!(eval_func("min", std::slice::from_ref(&bad_set)).is_err());
        assert!(eval_func("max", &[bad_set]).is_err());

        // max comparisons true/false
        assert_eq!(
            eval_func("max", &[num_val("1"), num_val("5"), num_val("2")])
                .unwrap()
                .data
                .as_ref(),
            num_val("5").data.as_ref()
        );

        assert!(eval_func("min", &[]).is_err());
        assert!(
            eval_func("min", &[Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("min", &[str_val("bad")]).is_err());
        // min comparisons true/false
        assert_eq!(
            eval_func("min", &[num_val("5"), num_val("1"), num_val("3")])
                .unwrap()
                .data
                .as_ref(),
            num_val("1").data.as_ref()
        );

        assert!(
            eval_func("signum", &[Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );
        assert!(eval_func("signum", &[str_val("bad")]).is_err());
        let res1 = eval_func("signum", &[num_val("0")]).unwrap();
        assert_eq!(res1.data.as_ref(), num_val("0").data.as_ref());
        let res2 = eval_func("signum", &[num_val("-5")]).unwrap();
        assert_eq!(res2.data.as_ref(), num_val("-1").data.as_ref());
    }

    #[test]
    fn test_range() {
        assert!(eval_func("range", &[]).is_err());
        assert!(
            eval_func(
                "range",
                &[num_val("1"), num_val("2"), num_val("3"), num_val("4")]
            )
            .is_err()
        );

        // line 41: unknown argument
        assert!(
            eval_func("range", &[Value::unknown(Type::Number)])
                .unwrap()
                .is_unknown()
        );

        // line 52: 1-arg negative limit
        let r_neg = eval_func("range", &[num_val("-3")]).unwrap();
        assert_eq!(r_neg.to_string(), "[0, -1, -2]");

        // 1-arg positive limit
        let r_pos = eval_func("range", &[num_val("3")]).unwrap();
        assert_eq!(r_pos.to_string(), "[0, 1, 2]");

        // line 62: 2-arg descending
        let r_desc = eval_func("range", &[num_val("5"), num_val("2")]).unwrap();
        assert_eq!(r_desc.to_string(), "[5, 4, 3]");

        // 2-arg ascending
        let r_asc = eval_func("range", &[num_val("1"), num_val("4")]).unwrap();
        assert_eq!(r_asc.to_string(), "[1, 2, 3]");

        // line 75: step cannot be zero
        assert!(eval_func("range", &[num_val("1"), num_val("5"), num_val("0")]).is_err());

        // line 87: positive step too many elements
        assert!(eval_func("range", &[num_val("0"), num_val("200000"), num_val("1")]).is_err());

        // line 99: negative step too many elements
        assert!(eval_func("range", &[num_val("200000"), num_val("0"), num_val("-1")]).is_err());

        // Coerce to number errors for 1, 2, and 3 arguments
        let bad = str_val("not_a_number");
        assert!(eval_func("range", std::slice::from_ref(&bad)).is_err());
        assert!(eval_func("range", &[bad.clone(), num_val("5")]).is_err());
        assert!(eval_func("range", &[num_val("0"), bad.clone()]).is_err());
        assert!(eval_func("range", &[bad.clone(), num_val("5"), num_val("1")]).is_err());
        assert!(eval_func("range", &[num_val("0"), bad.clone(), num_val("1")]).is_err());
        assert!(eval_func("range", &[num_val("0"), num_val("5"), bad]).is_err());
    }
}
