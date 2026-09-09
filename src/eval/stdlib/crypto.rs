//! Crypto/Hash standard library functions.

use crate::eval::func::Function;
use crate::types::{Type, Value, ValueData};
use base64::Engine;
use bcrypt::{DEFAULT_COST, hash};
use md5;
use sha1::{Digest, Sha1};
use sha2::{Sha256, Sha512};
use std::sync::Arc;
use uuid::Uuid;

#[must_use]
/// Get functions
pub fn functions() -> Vec<Function> {
    vec![
        bcrypt_func(),
        md5_func(),
        rsadecrypt_func(),
        sha1_func(),
        sha256_func(),
        sha512_func(),
        uuidv4_func(),
        uuidv5_func(),
        base64sha256_func(),
        base64sha512_func(),
    ]
}

fn base64sha256_func() -> Function {
    Function {
        name: "base64sha256".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("base64sha256 expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let s = coerce_to_string(&args[0], "base64sha256")?;
            let mut hasher = Sha256::new();
            hasher.update(s.as_bytes());
            let result = hasher.finalize();
            let encoded = base64::engine::general_purpose::STANDARD.encode(result);
            Ok(Value::new(Type::String, ValueData::String(encoded)))
        }),
        signature: None,
    }
}

fn base64sha512_func() -> Function {
    Function {
        name: "base64sha512".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("base64sha512 expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let s = coerce_to_string(&args[0], "base64sha512")?;
            let mut hasher = Sha512::new();
            hasher.update(s.as_bytes());
            let result = hasher.finalize();
            let encoded = base64::engine::general_purpose::STANDARD.encode(result);
            Ok(Value::new(Type::String, ValueData::String(encoded)))
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

fn bcrypt_func() -> Function {
    Function {
        name: "bcrypt".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 && args.len() != 2 {
                return Err("bcrypt expects 1 or 2 arguments".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "bcrypt")?;
            let cost = if args.len() == 2 {
                if args[1].is_unknown() {
                    return Ok(Value::unknown(Type::String));
                }
                let cost_val = args[1]
                    .clone()
                    .coerce(&Type::Number)
                    .map_err(|_| "bcrypt cost must be number".to_string())?;
                let ValueData::Number(n) = *cost_val.data else {
                    return Err("bcrypt cost must be number".to_string());
                };
                use bigdecimal::num_traits::ToPrimitive;
                n.0.to_u32().unwrap_or(DEFAULT_COST)
            } else {
                DEFAULT_COST
            };

            let hashed = hash(&s, cost).map_err(|e| format!("bcrypt error: {e}"))?;
            Ok(Value::new(Type::String, ValueData::String(hashed)))
        }),
        signature: None,
    }
}

fn md5_func() -> Function {
    Function {
        name: "md5".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("md5 expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "md5")?;
            let digest = md5::compute(s.as_bytes());
            Ok(Value::new(
                Type::String,
                ValueData::String(format!("{digest:x}")),
            ))
        }),
        signature: None,
    }
}

fn sha1_func() -> Function {
    Function {
        name: "sha1".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("sha1 expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "sha1")?;
            let mut hasher = Sha1::new();
            hasher.update(s.as_bytes());
            let result = hasher.finalize();
            Ok(Value::new(
                Type::String,
                ValueData::String(result.iter().fold(
                    String::with_capacity(result.len() * 2),
                    |mut s, b| {
                        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
                        s
                    },
                )),
            ))
        }),
        signature: None,
    }
}

fn sha256_func() -> Function {
    Function {
        name: "sha256".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("sha256 expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "sha256")?;
            let mut hasher = Sha256::new();
            hasher.update(s.as_bytes());
            let result = hasher.finalize();
            Ok(Value::new(
                Type::String,
                ValueData::String(result.iter().fold(
                    String::with_capacity(result.len() * 2),
                    |mut s, b| {
                        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
                        s
                    },
                )),
            ))
        }),
        signature: None,
    }
}

fn sha512_func() -> Function {
    Function {
        name: "sha512".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 1 {
                return Err("sha512 expects 1 argument".to_string());
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let s = coerce_to_string(&args[0], "sha512")?;
            let mut hasher = Sha512::new();
            hasher.update(s.as_bytes());
            let result = hasher.finalize();
            Ok(Value::new(
                Type::String,
                ValueData::String(result.iter().fold(
                    String::with_capacity(result.len() * 2),
                    |mut s, b| {
                        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
                        s
                    },
                )),
            ))
        }),
        signature: None,
    }
}

fn uuidv4_func() -> Function {
    Function {
        name: "uuidv4".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if !args.is_empty() {
                return Err("uuidv4 expects 0 arguments".to_string());
            }
            let id = Uuid::new_v4();
            Ok(Value::new(Type::String, ValueData::String(id.to_string())))
        }),
        signature: None,
    }
}

fn uuidv5_func() -> Function {
    Function {
        name: "uuidv5".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            if args.len() != 2 {
                return Err("uuidv5 expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let ns_str = coerce_to_string(&args[0], "uuidv5 namespace")?;
            let name_str = coerce_to_string(&args[1], "uuidv5 name")?;

            let ns =
                Uuid::parse_str(&ns_str).map_err(|e| format!("invalid namespace uuid: {e}"))?;
            let id = Uuid::new_v5(&ns, name_str.as_bytes());
            Ok(Value::new(Type::String, ValueData::String(id.to_string())))
        }),
        signature: None,
    }
}

fn rsadecrypt_func() -> Function {
    Function {
        name: "rsadecrypt".to_string(),
        func: Arc::new(|args: &[Value]| -> Result<Value, String> {
            use base64::prelude::*;
            use rsa::RsaPrivateKey;
            use rsa::pkcs1::DecodeRsaPrivateKey;
            use rsa::pkcs8::DecodePrivateKey;

            if args.len() != 2 {
                return Err("rsadecrypt expects 2 arguments".to_string());
            }
            if args[0].is_unknown() || args[1].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }

            let ciphertext = coerce_to_string(&args[0], "rsadecrypt ciphertext")?;
            let private_key = coerce_to_string(&args[1], "rsadecrypt private_key")?;

            let decoded = BASE64_STANDARD
                .decode(&ciphertext)
                .map_err(|e| format!("invalid base64: {e}"))?;

            let priv_key = if let Ok(k) = RsaPrivateKey::from_pkcs1_pem(&private_key) {
                k
            } else if let Ok(k) = RsaPrivateKey::from_pkcs8_pem(&private_key) {
                k
            } else {
                return Err("invalid RSA private key".to_string());
            };

            // Decrypt - HCL uses PKCS1v15 padding for rsadecrypt usually.
            let dec_data = priv_key
                .decrypt(rsa::Pkcs1v15Encrypt, &decoded)
                .map_err(|e| format!("decrypt error: {e}"))?;

            let res =
                String::from_utf8(dec_data).map_err(|_| "decrypted data not UTF-8".to_string())?;
            Ok(Value::new(Type::String, ValueData::String(res)))
        }),
        signature: None,
    }
}

#[cfg(test)]
mod tests {
    use crate::eval::stdlib::crypto::*;
    use crate::number::Number;
    use crate::types::{Type, Value, ValueData};
    use base64::prelude::*;
    use rand::rngs::OsRng;
    use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey};
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
    fn test_bcrypt_cost_fallback() {
        use crate::types::{Type, Value, ValueData};
        let f = bcrypt_func();

        let s = Value::new(Type::String, ValueData::String("hello".to_string()));
        let c = Value::unknown(Type::Number);

        let res = (f.func)(&[s, c]);
        assert!(res.is_ok());
    }

    #[test]
    fn test_bcrypt_cost_huge() {
        use crate::types::{Type, Value, ValueData};
        let f = bcrypt_func();

        let s = Value::new(Type::String, ValueData::String("hello".to_string()));
        let c = Value::new(
            Type::Number,
            ValueData::Number(std::str::FromStr::from_str("1e100").expect("expected value")),
        );

        let res = (f.func)(&[s, c]);
        assert!(res.is_ok());
    }

    #[test]
    fn test_bcrypt_cost_mismatch() {
        use crate::types::{Type, Value, ValueData};
        let f = bcrypt_func();

        let s = Value::new(Type::String, ValueData::String("hello".to_string()));
        // coerce() returns `self.clone()` if `self.ty()` matches the target type!
        // So if we make a Value with Type::Number but ValueData::Bool, `coerce` will return it unmodified!
        let c = Value::new(Type::Number, ValueData::Bool(true));

        let res = (f.func)(&[s, c]);
        assert!(res.is_err());
    }

    #[test]
    fn test_bcrypt() {
        assert!(eval_func("bcrypt", &[]).is_err());
        assert!(
            eval_func("bcrypt", &[Value::unknown(Type::String)])
                .expect("expected value")
                .is_unknown()
        );

        let res = eval_func(
            "bcrypt",
            &[Value::new(
                Type::String,
                ValueData::String("password".to_string()),
            )],
        )
        .expect("expected value");
        assert_eq!(res.ty(), &Type::String);

        let res = eval_func(
            "bcrypt",
            &[
                Value::new(Type::String, ValueData::String("password".to_string())),
                Value::new(
                    Type::Number,
                    ValueData::Number(Number::from_str("4").expect("expected value")),
                ),
            ],
        )
        .expect("expected value");
        assert_eq!(res.ty(), &Type::String);

        assert!(
            eval_func(
                "bcrypt",
                &[
                    Value::new(Type::String, ValueData::String("password".to_string())),
                    Value::new(Type::String, ValueData::String("invalid_cost".to_string()))
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn test_md5() {
        assert!(eval_func("md5", &[]).is_err());
        assert!(
            eval_func("md5", &[Value::unknown(Type::String)])
                .expect("expected value")
                .is_unknown()
        );

        let res = eval_func(
            "md5",
            &[Value::new(
                Type::String,
                ValueData::String("hello".to_string()),
            )],
        )
        .expect("expected value");
        assert_eq!(
            *res.data,
            ValueData::String("5d41402abc4b2a76b9719d911017c592".to_string())
        );
    }

    #[test]
    fn test_sha1() {
        assert!(eval_func("sha1", &[]).is_err());
        assert!(
            eval_func("sha1", &[Value::unknown(Type::String)])
                .expect("expected value")
                .is_unknown()
        );

        let res = eval_func(
            "sha1",
            &[Value::new(
                Type::String,
                ValueData::String("hello".to_string()),
            )],
        )
        .expect("expected value");
        assert_eq!(
            *res.data,
            ValueData::String("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".to_string())
        );
    }

    #[test]
    fn test_sha256() {
        assert!(eval_func("sha256", &[]).is_err());
        assert!(
            eval_func("sha256", &[Value::unknown(Type::String)])
                .expect("expected value")
                .is_unknown()
        );

        let res = eval_func(
            "sha256",
            &[Value::new(
                Type::String,
                ValueData::String("hello".to_string()),
            )],
        )
        .expect("expected value");
        assert_eq!(
            *res.data,
            ValueData::String(
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string()
            )
        );
    }

    #[test]
    fn test_sha512() {
        assert!(eval_func("sha512", &[]).is_err());
        assert!(
            eval_func("sha512", &[Value::unknown(Type::String)])
                .expect("expected value")
                .is_unknown()
        );

        let res = eval_func(
            "sha512",
            &[Value::new(
                Type::String,
                ValueData::String("hello".to_string()),
            )],
        )
        .expect("expected value");
        assert_eq!(
            *res.data,
            ValueData::String(
                "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043".to_string()
            )
        );
    }

    #[test]
    fn test_uuidv4() {
        assert!(eval_func("uuidv4", &[Value::unknown(Type::String)]).is_err());
        let res = eval_func("uuidv4", &[]).expect("expected value");
        assert_eq!(res.ty(), &Type::String);
    }

    #[test]
    fn test_uuidv5() {
        assert!(eval_func("uuidv5", &[]).is_err());
        assert!(
            eval_func(
                "uuidv5",
                &[Value::unknown(Type::String), Value::unknown(Type::String)]
            )
            .expect("expected value")
            .is_unknown()
        );

        let res = eval_func(
            "uuidv5",
            &[
                Value::new(
                    Type::String,
                    ValueData::String("1b671a64-40d5-491e-99b0-da01ff1f3341".to_string()),
                ),
                Value::new(Type::String, ValueData::String("test".to_string())),
            ],
        )
        .expect("expected value");
        assert_eq!(res.ty(), &Type::String);

        assert!(
            eval_func(
                "uuidv5",
                &[
                    Value::new(Type::String, ValueData::String("invalid-uuid".to_string())),
                    Value::new(Type::String, ValueData::String("test".to_string()))
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn test_rsadecrypt() {
        assert!(eval_func("rsadecrypt", &[]).is_err());
        assert!(
            eval_func(
                "rsadecrypt",
                &[Value::unknown(Type::String), Value::unknown(Type::String)]
            )
            .expect("expected value")
            .is_unknown()
        );

        let mut rng = OsRng;
        let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("failed to generate a key");

        let priv_key_pem = priv_key
            .to_pkcs1_pem(rsa::pkcs8::LineEnding::LF)
            .expect("expected value")
            .to_string();

        let data = b"hello world";
        let enc_data = priv_key
            .to_public_key()
            .encrypt(&mut rng, rsa::Pkcs1v15Encrypt, data)
            .expect("failed to encrypt");
        let b64_enc_data = BASE64_STANDARD.encode(enc_data);

        let res = eval_func(
            "rsadecrypt",
            &[
                Value::new(Type::String, ValueData::String(b64_enc_data)),
                Value::new(Type::String, ValueData::String(priv_key_pem)),
            ],
        )
        .expect("expected value");
        assert_eq!(*res.data, ValueData::String("hello world".to_string()));
    }

    #[test]
    #[should_panic(expected = "Function notexist not found")]
    fn test_eval_func_not_found() {
        eval_func("notexist", &[]).expect("expected value");
    }

    #[test]
    fn test_crypto_coverage() {
        let str_val = |s: &str| Value::new(Type::String, ValueData::String(s.to_string()));
        let non_str = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        let num_val = |s: &str| {
            Value::new(
                Type::Number,
                ValueData::Number(std::str::FromStr::from_str(s).expect("expected value")),
            )
        };

        // Line 31: coerce_to_string error when coercion fails
        assert!(eval_func("md5", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("sha1", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("sha256", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("sha512", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("base64sha256", std::slice::from_ref(&non_str)).is_err());
        assert!(eval_func("base64sha512", std::slice::from_ref(&non_str)).is_err());

        // base64sha256 & base64sha512: arg length error, unknown arg, and success
        assert!(eval_func("base64sha256", &[]).is_err());
        assert!(
            eval_func("base64sha256", &[Value::unknown(Type::String)])
                .expect("expected value")
                .is_unknown()
        );
        assert!(eval_func("base64sha256", &[str_val("hello")]).is_ok());
        assert!(eval_func("base64sha512", &[]).is_err());
        assert!(
            eval_func("base64sha512", &[Value::unknown(Type::String)])
                .expect("expected value")
                .is_unknown()
        );
        assert!(eval_func("base64sha512", &[str_val("hello")]).is_ok());

        // Line 35: coerce_to_string when coerced is not String (e.g. Null)
        assert!(eval_func("bcrypt", &[Value::new(Type::String, ValueData::Null)]).is_err());

        // Line 53
        assert!(
            eval_func("bcrypt", &[str_val("hello"), Value::unknown(Type::Number),])
                .expect("expected value")
                .is_unknown()
        );

        // Line 68: bcrypt error when cost is invalid (e.g. 50, bcrypt only allows 4..=31)
        assert!(eval_func("bcrypt", &[str_val("hello"), num_val("50")]).is_err());

        // Line 202: uuidv5 with known arg0 and unknown arg1
        assert!(
            eval_func(
                "uuidv5",
                &[
                    str_val("1b671a64-40d5-491e-99b0-da01ff1f3341"),
                    Value::unknown(Type::String),
                ]
            )
            .expect("expected value")
            .is_unknown()
        );

        // uuidv5 second arg coercion error
        assert!(
            eval_func(
                "uuidv5",
                &[
                    str_val("1b671a64-40d5-491e-99b0-da01ff1f3341"),
                    non_str.clone(),
                ]
            )
            .is_err()
        );

        // Line 229: rsadecrypt with known arg0 and unknown arg1
        assert!(
            eval_func(
                "rsadecrypt",
                &[str_val("ciphertext"), Value::unknown(Type::String)]
            )
            .expect("expected value")
            .is_unknown()
        );

        // rsadecrypt second arg coercion error
        assert!(eval_func("rsadecrypt", &[str_val("ciphertext"), non_str]).is_err());

        // Line 238: rsadecrypt with invalid base64
        assert!(
            eval_func(
                "rsadecrypt",
                &[str_val("invalid-base64!@#$%^"), str_val("key"),]
            )
            .is_err()
        );

        // RSA setup for decryption tests
        use base64::prelude::*;
        use rand::rngs::OsRng;
        use rsa::{RsaPrivateKey, pkcs8::EncodePrivateKey};
        let mut rng = OsRng;
        let priv_key = RsaPrivateKey::new(&mut rng, 512).expect("failed to generate a key");
        let priv_key_pem = priv_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("expected value")
            .to_string();

        // Line 246: rsadecrypt with invalid RSA key
        assert!(
            eval_func(
                "rsadecrypt",
                &[
                    str_val(&BASE64_STANDARD.encode(b"ciphertext")),
                    str_val("invalid key"),
                ]
            )
            .is_err()
        );

        // Line 251: rsadecrypt with valid base64 but invalid RSA ciphertext
        let bad_ciphertext = BASE64_STANDARD.encode(b"short-invalid-ciphertext");
        assert!(
            eval_func(
                "rsadecrypt",
                &[str_val(&bad_ciphertext), str_val(&priv_key_pem),]
            )
            .is_err()
        );

        // Line 254: rsadecrypt with valid RSA ciphertext containing non-UTF8 bytes
        let non_utf8_data = vec![0xFF, 0xFE, 0xFD];
        let enc_non_utf8 = priv_key
            .to_public_key()
            .encrypt(&mut rng, rsa::Pkcs1v15Encrypt, &non_utf8_data)
            .expect("failed to encrypt");
        let b64_non_utf8 = BASE64_STANDARD.encode(enc_non_utf8);
        assert!(
            eval_func(
                "rsadecrypt",
                &[str_val(&b64_non_utf8), str_val(&priv_key_pem),]
            )
            .is_err()
        );

        // Valid PKCS#8 decryption
        let enc_data = priv_key
            .to_public_key()
            .encrypt(&mut rng, rsa::Pkcs1v15Encrypt, b"test")
            .expect("failed to encrypt");
        let b64_enc_data = BASE64_STANDARD.encode(enc_data);

        let res = eval_func(
            "rsadecrypt",
            &[str_val(&b64_enc_data), str_val(&priv_key_pem)],
        )
        .expect("expected value");
        assert_eq!(*res.data, ValueData::String("test".to_string()));

        // Error handling for non-string first arguments
        let non_coercible = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        assert!(eval_func("uuidv5", &[non_coercible.clone(), str_val("name")]).is_err());
        assert!(eval_func("rsadecrypt", &[non_coercible, str_val(&priv_key_pem)]).is_err());
    }
}
