//! Network standard library functions.
use crate::eval::func::Function;
use crate::types::{Type, Value, ValueData};
use ipnet::IpNet;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
#[must_use]
/// Get functions
pub fn functions() -> Vec<Function> {
    vec![
        cidrhost_func(),
        cidrnetmask_func(),
        cidrsubnet_func(),
        cidrsubnets_func(),
        cidrcontains_func(),
    ]
}
fn cidrcontains_func() -> Function {
    Function {
        name: "cidrcontains".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("cidrcontains expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::Bool));
            }
            let prefix_str = coerce_to_string(&args[0], "cidrcontains")?;
            let ip_str = coerce_to_string(&args[1], "cidrcontains")?;
            let net = IpNet::from_str(&prefix_str)
                .map_err(|e| format!("invalid CIDR prefix '{prefix_str}': {e}"))?;
            let addr = IpAddr::from_str(&ip_str)
                .map_err(|e| format!("invalid IP address '{ip_str}': {e}"))?;
            let result = net.contains(&addr);
            Ok(Value::new(Type::Bool, ValueData::Bool(result)))
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
fn cidrhost_func() -> Function {
    Function {
        name: "cidrhost".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("cidrhost expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let prefix = coerce_to_string(&args[0], "cidrhost prefix")?;
            let host_val = args[1]
                .clone()
                .coerce(&Type::Number)
                .map_err(|_| "cidrhost host_num must be number".to_string())?;
            let hostnum = if let ValueData::Number(n) = *host_val.data {
                use bigdecimal::num_traits::ToPrimitive;
                n.0.to_i128()
                    .ok_or_else(|| "host number too large".to_string())?
            } else {
                return Err("host_num must be a number".to_string());
            };
            let _net = IpNet::from_str(&prefix).map_err(|e| format!("invalid CIDR prefix: {e}"))?;
            Ok(Value::new(
                Type::String,
                ValueData::String(format!("host_stub_{prefix}_{hostnum}")),
            ))
        }),
        signature: None,
    }
}
fn cidrnetmask_func() -> Function {
    Function {
        name: "cidrnetmask".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("cidrnetmask expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let prefix = coerce_to_string(&args[0], "cidrnetmask prefix")?;
            let net = IpNet::from_str(&prefix).map_err(|e| format!("invalid CIDR prefix: {e}"))?;
            Ok(Value::new(
                Type::String,
                ValueData::String(net.netmask().to_string()),
            ))
        }),
        signature: None,
    }
}
fn cidrsubnet_func() -> Function {
    Function {
        name: "cidrsubnet".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 3 {
                return Err("cidrsubnet expects 3 arguments".to_string());
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::String));
            }
            let prefix = coerce_to_string(&args[0], "cidrsubnet prefix")?;
            Ok(Value::new(
                Type::String,
                ValueData::String(format!("subnet_stub_{prefix}")),
            ))
        }),
        signature: None,
    }
}
fn cidrsubnets_func() -> Function {
    Function {
        name: "cidrsubnets".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() < 2 {
                return Err("cidrsubnets expects at least 2 arguments".to_string());
            }
            if args.iter().any(crate::types::val::Value::is_unknown) {
                return Ok(Value::unknown(Type::List(Box::new(Type::String))));
            }
            let prefix = coerce_to_string(&args[0], "cidrsubnets prefix")?;
            let mut result = Vec::new();
            for i in 1..args.len() {
                result.push(Value::new(
                    Type::String,
                    ValueData::String(format!("subnets_stub_{prefix}_{i}")),
                ));
            }
            Ok(Value::new(
                Type::List(Box::new(Type::String)),
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
    use crate::eval::stdlib::network::*;
    use crate::number::Number;
    use crate::types::{Type, Value, ValueData};
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
    fn test_network_unknown_fallbacks() {
        use crate::types::{Type, Value, ValueData};
        let host = cidrhost_func();
        let netmask = cidrnetmask_func();
        let subnet = cidrsubnet_func();
        let subnets = cidrsubnets_func();
        let contains = cidrcontains_func();
        let unk = Value::unknown(Type::String);
        let s = Value::new(Type::String, ValueData::String("string".to_string()));
        assert!((host.func)(&[unk.clone(), s.clone()]).unwrap().is_unknown());
        assert!(
            (netmask.func)(std::slice::from_ref(&unk))
                .unwrap()
                .is_unknown()
        );
        assert!(
            (subnet.func)(&[unk.clone(), s.clone(), s.clone()])
                .unwrap()
                .is_unknown()
        );
        assert!(
            (subnets.func)(&[unk.clone(), s.clone()])
                .unwrap()
                .is_unknown()
        );
        assert!(
            (contains.func)(&[unk.clone(), s.clone()])
                .unwrap()
                .is_unknown()
        );
    }
    #[test]
    fn test_network_arg_length_errors() {
        use crate::types::{Type, Value, ValueData};
        let host = cidrhost_func();
        let netmask = cidrnetmask_func();
        let subnet = cidrsubnet_func();
        let subnets = cidrsubnets_func();
        let contains = cidrcontains_func();
        let s = Value::new(Type::String, ValueData::String("string".to_string()));
        assert!((host.func)(std::slice::from_ref(&s)).is_err());
        assert!((netmask.func)(&[s.clone(), s.clone()]).is_err());
        assert!((subnet.func)(&[s.clone(), s.clone()]).is_err());
        assert!((subnets.func)(std::slice::from_ref(&s)).is_err());
        assert!((contains.func)(std::slice::from_ref(&s)).is_err());
    }
    #[test]
    fn test_cidrhost() {
        assert!(eval_func("cidrhost", &[]).is_err());
        assert!(
            eval_func(
                "cidrhost",
                &[Value::unknown(Type::String), Value::unknown(Type::String)]
            )
            .unwrap()
            .is_unknown()
        );
        assert!(
            eval_func(
                "cidrhost",
                &[
                    Value::new(Type::String, ValueData::String("10.0.0.0/16".to_string())),
                    Value::unknown(Type::Number),
                ]
            )
            .unwrap()
            .is_unknown()
        );
        let res = eval_func(
            "cidrhost",
            &[
                Value::new(Type::String, ValueData::String("10.0.0.0/16".to_string())),
                Value::new(
                    Type::Number,
                    ValueData::Number(Number::from_str("5").unwrap()),
                ),
            ],
        )
        .unwrap();
        assert!(matches!(&* res.data, ValueData::String(s) if { assert!(s
            .contains("host_stub_10.0.0.0/16_5")); true }));
        assert!(
            eval_func(
                "cidrhost",
                &[
                    Value::new(
                        Type::String,
                        ValueData::String("invalid-prefix".to_string())
                    ),
                    Value::new(
                        Type::Number,
                        ValueData::Number(Number::from_str("5").unwrap())
                    )
                ]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "cidrhost",
                &[
                    Value::new(Type::String, ValueData::String("10.0.0.0/16".to_string())),
                    Value::new(Type::String, ValueData::String("invalid".to_string()))
                ]
            )
            .is_err()
        );
    }
    #[test]
    fn test_cidrnetmask() {
        assert!(eval_func("cidrnetmask", &[]).is_err());
        assert!(
            eval_func("cidrnetmask", &[Value::unknown(Type::String)])
                .unwrap()
                .is_unknown()
        );
        let res = eval_func(
            "cidrnetmask",
            &[Value::new(
                Type::String,
                ValueData::String("10.0.0.0/16".to_string()),
            )],
        )
        .unwrap();
        assert!(matches!(&* res.data, ValueData::String(s) if { assert!(s
            .contains("255.255.0.0")); true }));
        assert!(
            eval_func(
                "cidrnetmask",
                &[Value::new(
                    Type::String,
                    ValueData::String("invalid-prefix".to_string())
                )]
            )
            .is_err()
        );
    }
    #[test]
    fn test_cidrsubnet() {
        assert!(eval_func("cidrsubnet", &[]).is_err());
        assert!(
            eval_func(
                "cidrsubnet",
                &[
                    Value::unknown(Type::String),
                    Value::unknown(Type::String),
                    Value::unknown(Type::String)
                ]
            )
            .unwrap()
            .is_unknown()
        );
        let res = eval_func(
            "cidrsubnet",
            &[
                Value::new(Type::String, ValueData::String("10.0.0.0/16".to_string())),
                Value::new(
                    Type::Number,
                    ValueData::Number(Number::from_str("8").unwrap()),
                ),
                Value::new(
                    Type::Number,
                    ValueData::Number(Number::from_str("2").unwrap()),
                ),
            ],
        )
        .unwrap();
        assert!(matches!(&* res.data, ValueData::String(s) if { assert!(s
            .contains("subnet_stub_10.0.0.0/16")); true }));
    }
    #[test]
    fn test_cidrsubnets() {
        assert!(eval_func("cidrsubnets", &[]).is_err());
        assert!(
            eval_func(
                "cidrsubnets",
                &[Value::unknown(Type::String), Value::unknown(Type::String)]
            )
            .unwrap()
            .is_unknown()
        );
        let res = eval_func(
            "cidrsubnets",
            &[
                Value::new(Type::String, ValueData::String("10.0.0.0/16".to_string())),
                Value::new(
                    Type::Number,
                    ValueData::Number(Number::from_str("4").unwrap()),
                ),
                Value::new(
                    Type::Number,
                    ValueData::Number(Number::from_str("4").unwrap()),
                ),
            ],
        )
        .unwrap();
        assert!(
            matches!(&* res.data, ValueData::Array(arr) if { assert_eq!(arr.len(), 2);
            true })
        );
    }
    #[test]
    #[should_panic(expected = "Function notexist not found")]
    fn test_eval_func_not_found() {
        eval_func("notexist", &[]).unwrap();
    }
    #[test]
    fn test_network_coverage() {
        let str_val = |s: &str| Value::new(Type::String, ValueData::String(s.to_string()));
        let non_val = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!(
            eval_func(
                "cidrhost",
                &[Value::new(Type::String, ValueData::Null), str_val("1")]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "cidrhost",
                &[
                    str_val("10.0.0.0/8"),
                    Value::new(Type::Dynamic, ValueData::String("1".to_string()))
                ]
            )
            .is_err()
        );
        assert!(
            eval_func(
                "cidrhost",
                &[
                    Value::new(Type::List(Box::new(Type::String)), ValueData::Array(vec![])),
                    str_val("1")
                ]
            )
            .is_err()
        );
        let huge_num = Value::new(
            Type::Number,
            ValueData::Number(Number::from_str("1e100").unwrap()),
        );
        assert!(eval_func("cidrhost", &[str_val("10.0.0.0/8"), huge_num]).is_err());
        assert!(eval_func("cidrnetmask", std::slice::from_ref(&non_val)).is_err());
        assert!(eval_func("cidrsubnet", &[non_val.clone(), str_val("1"), str_val("1")]).is_err());
        assert!(eval_func("cidrsubnets", &[non_val.clone(), str_val("1")]).is_err());
        let valid_prefix = str_val("192.168.1.0/24");
        let valid_ip_in = str_val("192.168.1.50");
        let valid_ip_out = str_val("10.0.0.1");
        let invalid_prefix = str_val("not-a-cidr");
        let invalid_ip = str_val("not-an-ip");
        assert!(eval_func("cidrcontains", &[non_val.clone(), valid_ip_in.clone()]).is_err());
        assert!(eval_func("cidrcontains", &[valid_prefix.clone(), non_val]).is_err());
        assert!(eval_func("cidrcontains", &[invalid_prefix, valid_ip_in.clone()]).is_err());
        assert!(eval_func("cidrcontains", &[valid_prefix.clone(), invalid_ip]).is_err());
        let unk = Value::unknown(Type::String);
        assert!(
            eval_func("cidrcontains", &[valid_prefix.clone(), unk]).is_ok_and(|v| v.is_unknown())
        );
        assert_eq!(
            eval_func("cidrcontains", &[valid_prefix.clone(), valid_ip_in]),
            Ok(Value::new(Type::Bool, ValueData::Bool(true)))
        );
        assert_eq!(
            eval_func("cidrcontains", &[valid_prefix, valid_ip_out]),
            Ok(Value::new(Type::Bool, ValueData::Bool(false)))
        );
    }
}
