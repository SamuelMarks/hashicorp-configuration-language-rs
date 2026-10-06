//! Date & Time standard library functions.
use crate::eval::func::Function;
use crate::types::{Type, Value, ValueData};
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
#[must_use]
/// Get functions
pub fn functions() -> Vec<Function> {
    vec![
        formatdate_func(),
        plantimestamp_func(),
        timeadd_func(),
        timecmp_func(),
        timestamp_func(),
    ]
}
static PLAN_TIMESTAMP: std::sync::OnceLock<String> = std::sync::OnceLock::new();
fn get_plan_timestamp() -> &'static str {
    PLAN_TIMESTAMP.get_or_init(|| Utc::now().to_rfc3339())
}
fn timestamp_func() -> Function {
    Function {
        name: "timestamp".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if !args.is_empty() {
                return Err(format!("timestamp expects 0 arguments, got {}", args.len()));
            }
            let ts = Utc::now().to_rfc3339();
            Ok(Value::new(Type::String, ValueData::String(ts)))
        }),
        signature: None,
    }
}
fn plantimestamp_func() -> Function {
    Function {
        name: "plantimestamp".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if !args.is_empty() {
                return Err(format!(
                    "plantimestamp expects 0 arguments, got {}",
                    args.len()
                ));
            }
            let ts = get_plan_timestamp().to_string();
            Ok(Value::new(Type::String, ValueData::String(ts)))
        }),
        signature: None,
    }
}
fn timecmp_func() -> Function {
    Function {
        name: "timecmp".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err(format!("timecmp expects 2 arguments, got {}", args.len()));
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Number));
            }
            let a_str = coerce_to_string(&args[0], "timecmp first argument")?;
            let b_str = coerce_to_string(&args[1], "timecmp second argument")?;
            let dt_a = DateTime::parse_from_rfc3339(&a_str)
                .map_err(|e| format!("timecmp: invalid RFC3339 timestamp {a_str:?}: {e}"))?;
            let dt_b = DateTime::parse_from_rfc3339(&b_str)
                .map_err(|e| format!("timecmp: invalid RFC3339 timestamp {b_str:?}: {e}"))?;
            let cmp_val = match dt_a.cmp(&dt_b) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            Ok(Value::new(
                Type::Number,
                ValueData::Number(crate::number::Number::from(cmp_val)),
            ))
        }),
        signature: None,
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
fn formatdate_func() -> Function {
    Function {
        name: "formatdate".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("formatdate expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let format_str = coerce_to_string(&args[0], "formatdate format")?;
            let time_str = coerce_to_string(&args[1], "formatdate time")?;
            let dt = DateTime::parse_from_rfc3339(&time_str)
                .map_err(|e| format!("invalid time: {e}"))?;
            let cformat = format_str
                .replace("YYYY", "%Y")
                .replace("MM", "%m")
                .replace("DD", "%d");
            let res = dt.format(&cformat).to_string();
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}
fn timeadd_func() -> Function {
    Function {
        name: "timeadd".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("timeadd expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let time_str = coerce_to_string(&args[0], "timeadd time")?;
            let dur_str = coerce_to_string(&args[1], "timeadd duration")?;
            let mut dt = DateTime::parse_from_rfc3339(&time_str)
                .map_err(|e| format!("invalid time: {e}"))?
                .with_timezone(&Utc);
            let mut total_secs = 0.0;
            let mut current_num = String::new();
            for c in dur_str.chars() {
                if c.is_ascii_digit() || c == '.' || c == '-' {
                    current_num.push(c);
                } else if c == 'h' {
                    let val: f64 = current_num
                        .parse()
                        .map_err(|_| "invalid duration".to_string())?;
                    total_secs += val * 3600.0;
                    current_num.clear();
                } else if c == 'm' {
                    let val: f64 = current_num
                        .parse()
                        .map_err(|_| "invalid duration".to_string())?;
                    total_secs += val * 60.0;
                    current_num.clear();
                } else if c == 's' {
                    let val: f64 = current_num
                        .parse()
                        .map_err(|_| "invalid duration".to_string())?;
                    total_secs += val;
                    current_num.clear();
                } else {
                    return Err("invalid duration unit".to_string());
                }
            }
            let dur = Duration::milliseconds((total_secs * 1000.0) as i64);
            dt += dur;
            let res = dt.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true);
            Ok(Value::new(Type::String, ValueData::String(res)))
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
    use crate::eval::stdlib::datetime::*;
    #[test]
    fn test_datetime_require_string_arg_err() {
        let val = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!(coerce_to_string(&val, "test").is_err());
    }
    use crate::types::{Type, Value, ValueData};
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
    fn test_datetime_errors() {
        use crate::types::{Type, Value, ValueData};
        let fmt = formatdate_func();
        let add = timeadd_func();
        let s = Value::new(Type::String, ValueData::String("string".to_string()));
        let ok_time = Value::new(
            Type::String,
            ValueData::String("2020-01-02T15:04:05Z".to_string()),
        );
        let bad_time = Value::new(Type::String, ValueData::String("2020".to_string()));
        let bad_dur = Value::new(Type::String, ValueData::String("1.5x".to_string()));
        let _bad_dur2 = Value::new(Type::String, ValueData::String("1.5".to_string()));
        let not_str = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!((fmt.func)(&[not_str.clone(), s.clone()]).is_err());
        assert!((add.func)(&[not_str, s.clone()]).is_err());
        assert!((fmt.func)(std::slice::from_ref(&s)).is_err());
        assert!((add.func)(std::slice::from_ref(&s)).is_err());
        assert!((timestamp_func().func)(std::slice::from_ref(&s)).is_err());
        assert!((plantimestamp_func().func)(std::slice::from_ref(&s)).is_err());
        assert!((fmt.func)(&[s.clone(), bad_time.clone()]).is_err());
        assert!((add.func)(&[bad_time, s.clone()]).is_err());
        assert!((add.func)(&[ok_time.clone(), bad_dur]).is_err());
    }
    #[test]
    fn test_datetime_duration_parse_errors() {
        use crate::types::{Type, Value, ValueData};
        let add = timeadd_func();
        let ok_time = Value::new(
            Type::String,
            ValueData::String("2020-01-02T15:04:05Z".to_string()),
        );
        let bad_dur_parse_h = Value::new(Type::String, ValueData::String("..h".to_string()));
        let bad_dur_parse_m = Value::new(Type::String, ValueData::String("..m".to_string()));
        let bad_dur_parse_s = Value::new(Type::String, ValueData::String("..s".to_string()));
        assert!((add.func)(&[ok_time.clone(), bad_dur_parse_h]).is_err());
        assert!((add.func)(&[ok_time.clone(), bad_dur_parse_m]).is_err());
        assert!((add.func)(&[ok_time, bad_dur_parse_s]).is_err());
    }
    #[test]
    fn test_datetime_unknown() {
        use crate::types::{Type, Value, ValueData};
        let fmt = formatdate_func();
        let add = timeadd_func();
        let ok_time = Value::new(
            Type::String,
            ValueData::String("2020-01-02T15:04:05Z".to_string()),
        );
        let unk = Value::unknown(Type::String);
        assert!(
            (fmt.func)(&[unk.clone(), ok_time.clone()])
                .unwrap()
                .is_unknown()
        );
        assert!(
            (fmt.func)(&[ok_time.clone(), unk.clone()])
                .unwrap()
                .is_unknown()
        );
        assert!(
            (add.func)(&[unk.clone(), ok_time.clone()])
                .unwrap()
                .is_unknown()
        );
        assert!((add.func)(&[ok_time, unk]).unwrap().is_unknown());
    }
    #[test]
    fn test_formatdate() {
        assert!(eval_func("formatdate", &[]).is_err());
        assert!(
            eval_func(
                "formatdate",
                &[Value::unknown(Type::String), Value::unknown(Type::String)]
            )
            .unwrap()
            .is_unknown()
        );
        let res = eval_func(
            "formatdate",
            &[
                Value::new(Type::String, ValueData::String("YYYY-MM-DD".to_string())),
                Value::new(
                    Type::String,
                    ValueData::String("2020-01-02T03:04:05Z".to_string()),
                ),
            ],
        )
        .unwrap();
        assert_eq!(*res.data, ValueData::String("2020-01-02".to_string()));
        assert!(
            eval_func(
                "formatdate",
                &[
                    Value::new(Type::String, ValueData::String("YYYY-MM-DD".to_string())),
                    Value::new(Type::String, ValueData::String("invalid-time".to_string()))
                ]
            )
            .is_err()
        );
    }
    #[test]
    fn test_timeadd() {
        assert!(eval_func("timeadd", &[]).is_err());
        assert!(
            eval_func(
                "timeadd",
                &[Value::unknown(Type::String), Value::unknown(Type::String)]
            )
            .unwrap()
            .is_unknown()
        );
        let res = eval_func(
            "timeadd",
            &[
                Value::new(
                    Type::String,
                    ValueData::String("2020-01-02T03:04:05Z".to_string()),
                ),
                Value::new(Type::String, ValueData::String("1h".to_string())),
            ],
        )
        .unwrap();
        assert_eq!(
            *res.data,
            ValueData::String("2020-01-02T04:04:05Z".to_string())
        );
        let res = eval_func(
            "timeadd",
            &[
                Value::new(
                    Type::String,
                    ValueData::String("2020-01-02T03:04:05Z".to_string()),
                ),
                Value::new(Type::String, ValueData::String("1m30s".to_string())),
            ],
        )
        .unwrap();
        assert_eq!(
            *res.data,
            ValueData::String("2020-01-02T03:05:35Z".to_string())
        );
        let res_neg = eval_func(
            "timeadd",
            &[
                Value::new(
                    Type::String,
                    ValueData::String("2020-01-02T03:04:05Z".to_string()),
                ),
                Value::new(Type::String, ValueData::String("-1h-30m".to_string())),
            ],
        )
        .unwrap();
        assert_eq!(
            *res_neg.data,
            ValueData::String("2020-01-02T01:34:05Z".to_string())
        );
        assert!(
            eval_func(
                "timeadd",
                &[
                    Value::new(Type::String, ValueData::String("invalid-time".to_string())),
                    Value::new(Type::String, ValueData::String("1h".to_string()))
                ]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "timeadd",
                &[
                    Value::new(
                        Type::String,
                        ValueData::String("2020-01-02T03:04:05Z".to_string())
                    ),
                    Value::new(
                        Type::String,
                        ValueData::String("invalid_duration".to_string())
                    )
                ]
            )
            .is_err()
        );
    }
    #[test]
    fn test_datetime_coverage() {
        let str_val = |s: &str| Value::new(Type::String, ValueData::String(s.to_string()));
        let non_str = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        let null_str = Value::new(Type::String, ValueData::Null);
        assert!(eval_func("formatdate", &[null_str, str_val("2020-01-02T00:00:00Z")]).is_err());
        assert!(eval_func("formatdate", &[str_val("YYYY-MM-DD"), non_str.clone()]).is_err());
        assert!(eval_func("timeadd", &[str_val("2020-01-02T00:00:00Z"), non_str]).is_err());
    }
    #[test]
    fn test_new_datetime_functions() {
        let str_val = |s: &str| Value::new(Type::String, ValueData::String(s.to_string()));
        let ts = eval_func("timestamp", &[]).unwrap();
        assert_eq!(ts.ty(), &Type::String);
        let check_str = |v: &Value| match &*v.data {
            ValueData::String(s) => DateTime::parse_from_rfc3339(s).is_ok(),
            _ => false,
        };
        assert!(check_str(&ts));
        assert!(!check_str(&Value::null(Type::Dynamic)));
        assert!(eval_func("timestamp", &[str_val("extra")]).is_err());
        let pts1 = eval_func("plantimestamp", &[]).unwrap();
        let pts2 = eval_func("plantimestamp", &[]).unwrap();
        assert_eq!(pts1, pts2);
        assert!(eval_func("plantimestamp", &[str_val("extra")]).is_err());
        let t1 = str_val("2026-01-01T00:00:00Z");
        let t2 = str_val("2026-06-01T00:00:00Z");
        let t3 = str_val("2026-01-01T00:00:00Z");
        let result_less = eval_func("timecmp", &[t1.clone(), t2.clone()]).unwrap();
        assert_eq!(
            result_less,
            Value::new(
                Type::Number,
                ValueData::Number(crate::number::Number::from(-1))
            )
        );
        let result_greater = eval_func("timecmp", &[t2, t1.clone()]).unwrap();
        assert_eq!(
            result_greater,
            Value::new(
                Type::Number,
                ValueData::Number(crate::number::Number::from(1))
            )
        );
        let result_equal = eval_func("timecmp", &[t1, t3]).unwrap();
        assert_eq!(
            result_equal,
            Value::new(
                Type::Number,
                ValueData::Number(crate::number::Number::from(0))
            )
        );
        let unk = Value::unknown(Type::String);
        let res_unk = eval_func("timecmp", &[unk, str_val("2026-01-01T00:00:00Z")]).unwrap();
        assert!(res_unk.is_unknown());
        let res_unk2 = eval_func(
            "timecmp",
            &[
                str_val("2026-01-01T00:00:00Z"),
                Value::unknown(Type::String),
            ],
        )
        .unwrap();
        assert!(res_unk2.is_unknown());
        assert!(
            eval_func(
                "timecmp",
                &[str_val("invalid"), str_val("2026-01-01T00:00:00Z")]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "timecmp",
                &[str_val("2026-01-01T00:00:00Z"), str_val("invalid")]
            )
            .is_err()
        );
        let non_coercible = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!(
            eval_func(
                "timecmp",
                &[non_coercible.clone(), str_val("2026-01-01T00:00:00Z")]
            )
            .is_err()
        );
        assert!(eval_func("timecmp", &[str_val("2026-01-01T00:00:00Z"), non_coercible]).is_err());
        assert!(eval_func("timecmp", &[]).is_err());
    }
    #[test]
    #[should_panic(expected = "Function notexist not found")]
    fn test_eval_func_not_found() {
        eval_func("notexist", &[]).unwrap();
    }
}
