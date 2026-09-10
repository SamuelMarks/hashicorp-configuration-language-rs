#[cfg(test)]
mod tests {
    use crate::api::parse;
    use crate::eval::context::Context;
    use crate::eval::evaluator::Evaluator;
    use crate::eval::stdlib::{all_functions, get_stdlib_function, stdlib_map};
    use crate::types::ValueData;

    #[test]
    fn test_all_functions_and_registry() {
        let funcs = all_functions();
        assert!(funcs.len() >= 70);

        let map = stdlib_map();
        assert_eq!(map.len(), funcs.len());

        for f in &funcs {
            assert!(map.contains_key(&f.name));
            let retrieved = get_stdlib_function(&f.name);
            assert!(retrieved.is_some());
            assert_eq!(
                retrieved.map(|func| func.name.as_str()),
                Some(f.name.as_str())
            );
        }

        assert!(get_stdlib_function("nonexistent_func_12345").is_none());
    }

    #[test]
    fn test_all_stdlib_evaluation() {
        let hcl = r#"
            conv = tostring(1)
            crypt = md5("hello")
            dt = formatdate("YYYY-MM-DD", "2020-01-01T00:00:00Z")
            net = cidrsubnet("10.0.0.0/16", 4, 2)
            num = abs(-42)
            str = upper("hello")
            enc = base64encode("test")
            col = length([1, 2, 3])
        "#;
        let body = parse(hcl).expect("expected parse success");
        let ctx = Context::with_stdlib();

        let eval_attr = |name: &str| -> crate::types::Value {
            let attr = body.attributes.get(name).expect("attribute found");
            let eval = Evaluator::new(&ctx);
            let (val, diags) = eval.evaluate(&attr.expr).expect("evaluation success");
            assert!(!diags.has_errors());
            val
        };

        let conv_val = eval_attr("conv");
        assert_eq!(*conv_val.data, ValueData::String("1".to_string()));

        let crypt_val = eval_attr("crypt");
        assert_eq!(
            *crypt_val.data,
            ValueData::String("5d41402abc4b2a76b9719d911017c592".to_string())
        );

        let dt_val = eval_attr("dt");
        assert_eq!(*dt_val.data, ValueData::String("2020-01-01".to_string()));

        let net_val = eval_attr("net");
        assert_eq!(
            *net_val.data,
            ValueData::String("subnet_stub_10.0.0.0/16".to_string())
        );

        let num_val = eval_attr("num");
        assert_eq!(*num_val.data, ValueData::Number(42.into()));

        let str_val = eval_attr("str");
        assert_eq!(*str_val.data, ValueData::String("HELLO".to_string()));

        let enc_val = eval_attr("enc");
        assert_eq!(*enc_val.data, ValueData::String("dGVzdA==".to_string()));

        let col_val = eval_attr("col");
        assert_eq!(*col_val.data, ValueData::Number(3.into()));
    }

    #[test]
    fn test_refinement_propagation_and_stdlib() {
        use crate::ast::expr::{BinaryOp, Expression, TemplatePart};
        use crate::number::Number;
        use crate::span::Span;
        use crate::types::refinement::Refinement;
        use crate::types::{Type, Value};

        let span = Span::new(0, 1, 1, 1, 1, 2);
        let mut ctx = Context::with_stdlib();

        let r_left = Refinement::not_null()
            .with_prefix("app-")
            .with_string_length(4, 10)
            .expect("ok");
        let r_right = Refinement::not_null()
            .with_suffix("-prod")
            .with_string_length(5, 15)
            .expect("ok");

        let left_val = Value::unknown_refined(Type::String, r_left);
        let right_val = Value::unknown_refined(Type::String, r_right);

        ctx.set_variable("left", left_val.clone());
        ctx.set_variable("right", right_val.clone());
        ctx.set_variable(
            "known_str",
            Value::new(Type::String, ValueData::String("env-".to_string())),
        );

        let mut evaluator = Evaluator::new(&ctx);

        // 1. Binary + between unknown string with prefix and unknown string with suffix
        let add_expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("left".to_string(), span.clone())),
            Box::new(Expression::Variable("right".to_string(), span.clone())),
            span.clone(),
        );
        let result_add = evaluator.eval_expr(&add_expr);
        assert!(result_add.is_unknown());
        let ref_add = result_add.refinement().expect("refinement propagated");
        assert!(ref_add.not_null);
        assert_eq!(ref_add.string_prefix.as_deref(), Some("app-"));
        assert_eq!(ref_add.string_suffix.as_deref(), Some("-prod"));
        assert_eq!(ref_add.string_length_min, Some(9));
        assert_eq!(ref_add.string_length_max, Some(25));

        // 2. Binary + with known string prefix
        let add_known_expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("known_str".to_string(), span.clone())),
            Box::new(Expression::Variable("right".to_string(), span.clone())),
            span.clone(),
        );
        let result_known = evaluator.eval_expr(&add_known_expr);
        let ref_known = result_known.refinement().expect("refinement exists");
        assert_eq!(ref_known.string_prefix.as_deref(), Some("env-"));
        assert_eq!(ref_known.string_suffix.as_deref(), Some("-prod"));

        // 3. Template interpolation refinement inference
        let template_expr = Expression::Template(
            vec![
                TemplatePart::Literal("prefix-".to_string(), span.clone()),
                TemplatePart::Interpolation(
                    Expression::Variable("left".to_string(), span.clone()),
                    span.clone(),
                ),
                TemplatePart::Literal("-suffix".to_string(), span.clone()),
            ],
            span,
        );
        let result_tmpl = evaluator.eval_expr(&template_expr);
        assert!(result_tmpl.is_unknown());
        let ref_tmpl = result_tmpl.refinement().expect("template refinement");
        assert_eq!(ref_tmpl.string_prefix.as_deref(), Some("prefix-"));
        assert_eq!(ref_tmpl.string_suffix.as_deref(), Some("-suffix"));

        // 4. stdlib length() with exact string length
        let r_exact_str = Refinement::not_null().with_string_length(8, 8).expect("ok");
        let exact_str_val = Value::unknown_refined(Type::String, r_exact_str);
        let length_fn = crate::eval::stdlib::get_stdlib_function("length").expect("length exists");
        let len_res = (length_fn.func)(&[exact_str_val]).expect("call succeeds");
        assert!(!len_res.is_unknown());
        assert_eq!(
            len_res,
            Value::new(Type::Number, ValueData::Number(Number::from(8)))
        );

        // 5. stdlib length() with bounded string length
        let r_bound_str = Refinement::not_null()
            .with_string_length(3, 12)
            .expect("ok");
        let bound_str_val = Value::unknown_refined(Type::String, r_bound_str);
        let len_bound_res = (length_fn.func)(&[bound_str_val]).expect("call succeeds");
        assert!(len_bound_res.is_unknown());
        let len_num_ref = len_bound_res.refinement().expect("number bounds");
        assert_eq!(len_num_ref.number_min, Some(Number::from(3)));
        assert_eq!(len_num_ref.number_max, Some(Number::from(12)));

        // 6. stdlib length() with exact collection length
        let r_exact_col = Refinement::not_null()
            .with_collection_length(4, 4)
            .expect("ok");
        let exact_col_val = Value::unknown_refined(Type::List(Box::new(Type::String)), r_exact_col);
        let len_col_res = (length_fn.func)(&[exact_col_val]).expect("call succeeds");
        assert!(!len_col_res.is_unknown());
        assert_eq!(
            len_col_res,
            Value::new(Type::Number, ValueData::Number(Number::from(4)))
        );

        // 7. stdlib length() with bounded collection length
        let r_bound_col = Refinement::not_null()
            .with_collection_length(2, 6)
            .expect("ok");
        let bound_col_val = Value::unknown_refined(Type::List(Box::new(Type::String)), r_bound_col);
        let len_col_bound_res = (length_fn.func)(&[bound_col_val]).expect("call succeeds");
        assert!(len_col_bound_res.is_unknown());
        let len_col_num_ref = len_col_bound_res.refinement().expect("col number bounds");
        assert_eq!(len_col_num_ref.number_min, Some(Number::from(2)));
        assert_eq!(len_col_num_ref.number_max, Some(Number::from(6)));

        // 8. stdlib substr() when substring falls entirely within known prefix
        let substr_fn = crate::eval::stdlib::get_stdlib_function("substr").expect("substr exists");
        let r_sub = Refinement::not_null()
            .with_prefix("server-node-42")
            .with_string_length(14, 30)
            .expect("ok");
        let sub_val = Value::unknown_refined(Type::String, r_sub);
        let substr_exact = (substr_fn.func)(&[
            sub_val.clone(),
            Value::new(Type::Number, ValueData::Number(Number::from(0))),
            Value::new(Type::Number, ValueData::Number(Number::from(6))),
        ])
        .expect("substr succeeds");
        assert!(!substr_exact.is_unknown());
        assert_eq!(
            substr_exact,
            Value::new(Type::String, ValueData::String("server".to_string()))
        );

        // 9. stdlib substr() starting at 0 with length greater than prefix
        let substr_partial = (substr_fn.func)(&[
            sub_val,
            Value::new(Type::Number, ValueData::Number(Number::from(0))),
            Value::new(Type::Number, ValueData::Number(Number::from(20))),
        ])
        .expect("substr succeeds");
        assert!(substr_partial.is_unknown());
        let sub_partial_ref = substr_partial.refinement().expect("partial ref");
        assert_eq!(
            sub_partial_ref.string_prefix.as_deref(),
            Some("server-node-42")
        );
        assert_eq!(sub_partial_ref.string_length_max, Some(20));
    }

    #[test]
    fn test_stdlib_expansion_functions() {
        use crate::encode::EncodeValue;
        use crate::eval::stdlib::get_stdlib_function;

        // 1. range()
        let range_fn = get_stdlib_function("range").expect("range exists");
        let r1 = (range_fn.func)(&[3_i64.encode_value()]).expect("range(3)");
        assert_eq!(r1.to_string(), "[0, 1, 2]");

        let r2 =
            (range_fn.func)(&[1_i64.encode_value(), 4_i64.encode_value()]).expect("range(1, 4)");
        assert_eq!(r2.to_string(), "[1, 2, 3]");

        let r3 = (range_fn.func)(&[
            1_i64.encode_value(),
            8_i64.encode_value(),
            2_i64.encode_value(),
        ])
        .expect("range(1, 8, 2)");
        assert_eq!(r3.to_string(), "[1, 3, 5, 7]");

        let r_rev = (range_fn.func)(&[
            5_i64.encode_value(),
            1_i64.encode_value(),
            (-1_i64).encode_value(),
        ])
        .expect("range(5, 1, -1)");
        assert_eq!(r_rev.to_string(), "[5, 4, 3, 2]");

        assert!(
            (range_fn.func)(&[
                1_i64.encode_value(),
                5_i64.encode_value(),
                0_i64.encode_value()
            ])
            .is_err()
        );
        assert!((range_fn.func)(&[]).is_err());
        assert!(
            (range_fn.func)(&[
                1_i64.encode_value(),
                2_i64.encode_value(),
                3_i64.encode_value(),
                4_i64.encode_value()
            ])
            .is_err()
        );

        // 2. regex_replace()
        let regex_replace_fn = get_stdlib_function("regex_replace").expect("regex_replace exists");
        let rep = (regex_replace_fn.func)(&[
            "hello-123-world".encode_value(),
            "[0-9]+".encode_value(),
            "456".encode_value(),
        ])
        .expect("regex_replace ok");
        assert_eq!(rep.to_string(), "\"hello-456-world\"");

        assert!(
            (regex_replace_fn.func)(&[
                "test".encode_value(),
                "(".encode_value(),
                "x".encode_value(),
            ])
            .is_err()
        );

        // 3. base64sha256() & base64sha512()
        let b64_256_fn = get_stdlib_function("base64sha256").expect("base64sha256 exists");
        let b256 = (b64_256_fn.func)(&["hello world".encode_value()]).expect("b64 sha256");
        assert_eq!(
            b256.to_string(),
            "\"uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek=\""
        );

        let b64_512_fn = get_stdlib_function("base64sha512").expect("base64sha512 exists");
        let b512 = (b64_512_fn.func)(&["hello world".encode_value()]).expect("b64 sha512");
        assert!(b512.to_string().starts_with("\"MJ7MSJwS1"));

        // 4. cidrcontains()
        let cidrcontains_fn = get_stdlib_function("cidrcontains").expect("cidrcontains exists");
        let c1 = (cidrcontains_fn.func)(&[
            "192.168.1.0/24".encode_value(),
            "192.168.1.50".encode_value(),
        ])
        .expect("contains v4 ok");
        assert_eq!(c1.to_string(), "true");

        let c2 =
            (cidrcontains_fn.func)(&["192.168.1.0/24".encode_value(), "10.0.0.1".encode_value()])
                .expect("not contains v4 ok");
        assert_eq!(c2.to_string(), "false");

        let c3 =
            (cidrcontains_fn.func)(&["2001:db8::/32".encode_value(), "2001:db8::1".encode_value()])
                .expect("contains v6 ok");
        assert_eq!(c3.to_string(), "true");

        assert!(
            (cidrcontains_fn.func)(&["invalid_cidr".encode_value(), "10.0.0.1".encode_value()])
                .is_err()
        );
        assert!(
            (cidrcontains_fn.func)(&["10.0.0.0/8".encode_value(), "invalid_ip".encode_value()])
                .is_err()
        );
    }

    #[test]
    fn test_all_filesystem_functions_with_mem_fs() {
        use crate::eval::fs::MemFileSystem;
        use sha2::Digest;
        use std::sync::Arc;

        let mut mem_fs = MemFileSystem::new();
        mem_fs.insert_file("configs/app.hcl", "port = 8080\nenv = \"prod\"\n");
        mem_fs.insert_file("configs/db.hcl", "host = \"localhost\"\n");
        mem_fs.insert_file(
            "templates/greeting.tftpl",
            "Hello, ${name}! Port is ${port}.",
        );
        mem_fs.insert_file("data/bin.dat", vec![0xDE, 0xAD, 0xBE, 0xEF]);

        let ctx = Context::with_stdlib().with_filesystem(Arc::new(mem_fs));

        let eval_str = |expr_src: &str| -> crate::types::Value {
            let hcl = format!("result = {expr_src}");
            let body = parse(&hcl).expect("parse ok");
            let attr = body.attributes.get("result").expect("attribute ok");
            let (val, diags) = Evaluator::new(&ctx).evaluate(&attr.expr).expect("eval ok");
            assert!(!diags.has_errors());
            val
        };

        // 1. file()
        let file_val = eval_str("file(\"configs/app.hcl\")");
        assert_eq!(
            *file_val.data,
            ValueData::String("port = 8080\nenv = \"prod\"\n".to_string())
        );

        // 2. fileexists()
        let exists_true = eval_str("fileexists(\"configs/app.hcl\")");
        assert_eq!(*exists_true.data, ValueData::Bool(true));
        let exists_false = eval_str("fileexists(\"nonexistent.hcl\")");
        assert_eq!(*exists_false.data, ValueData::Bool(false));

        // 3. filebase64()
        let b64_val = eval_str("filebase64(\"data/bin.dat\")");
        assert_eq!(*b64_val.data, ValueData::String("3q2+7w==".to_string()));

        // 4. filemd5()
        let md5_val = eval_str("filemd5(\"configs/db.hcl\")");
        assert_eq!(
            *md5_val.data,
            ValueData::String(format!("{:x}", md5::compute(b"host = \"localhost\"\n")))
        );

        let to_hex_str = |bytes: &[u8]| -> String {
            let mut s = String::with_capacity(bytes.len() * 2);
            for b in bytes {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
            }
            s
        };

        // 5. filesha1()
        let sha1_val = eval_str("filesha1(\"configs/db.hcl\")");
        let mut hasher1 = sha1::Sha1::new();
        hasher1.update(b"host = \"localhost\"\n");
        let sha1_expected = to_hex_str(&hasher1.finalize());
        assert_eq!(*sha1_val.data, ValueData::String(sha1_expected));

        // 6. filesha256()
        let sha256_val = eval_str("filesha256(\"configs/db.hcl\")");
        let mut hasher256 = sha2::Sha256::new();
        hasher256.update(b"host = \"localhost\"\n");
        let sha256_expected = to_hex_str(&hasher256.finalize());
        assert_eq!(*sha256_val.data, ValueData::String(sha256_expected));

        // 7. filesha512()
        let sha512_val = eval_str("filesha512(\"configs/db.hcl\")");
        let mut hasher512 = sha2::Sha512::new();
        hasher512.update(b"host = \"localhost\"\n");
        let sha512_expected = to_hex_str(&hasher512.finalize());
        assert_eq!(*sha512_val.data, ValueData::String(sha512_expected));

        // 8. filebase64sha256()
        let b64sha256_val = eval_str("filebase64sha256(\"configs/db.hcl\")");
        let mut hasher_b64 = sha2::Sha256::new();
        hasher_b64.update(b"host = \"localhost\"\n");
        use base64::prelude::*;
        let expected_b64 = BASE64_STANDARD.encode(hasher_b64.finalize());
        assert_eq!(*b64sha256_val.data, ValueData::String(expected_b64));

        // 9. fileset()
        let fileset_val = eval_str("fileset(\"configs\", \"*.hcl\")");
        if let ValueData::Set(set) = &*fileset_val.data {
            let files: Vec<String> = set.iter().map(|v| v.to_string().replace('"', "")).collect();
            assert!(files.contains(&"app.hcl".to_string()));
            assert!(files.contains(&"db.hcl".to_string()));
        } else {
            panic!("expected set from fileset");
        }

        // 10. abspath()
        let abspath_val = eval_str("abspath(\"configs/app.hcl\")");
        assert_eq!(
            *abspath_val.data,
            ValueData::String("configs/app.hcl".to_string())
        );

        // 11. pathexpand()
        let pathexpand_val = eval_str("pathexpand(\"configs/app.hcl\")");
        assert_eq!(
            *pathexpand_val.data,
            ValueData::String("configs/app.hcl".to_string())
        );

        // 12. templatefile()
        let template_val = eval_str(
            "templatefile(\"templates/greeting.tftpl\", { \"name\" = \"World\", \"port\" = 8080 })",
        );
        assert_eq!(
            *template_val.data,
            ValueData::String("Hello, World! Port is 8080.".to_string())
        );
    }

    #[test]
    fn test_stdlib_expansion_parity() {
        use crate::types::{Type, Value, ValueMark};
        use std::collections::BTreeSet;

        let diff_fn =
            get_stdlib_function("setsymmetricdifference").expect("setsymmetricdifference exists");
        let gunzip_fn = get_stdlib_function("base64gunzip").expect("base64gunzip exists");
        let gzip_fn = get_stdlib_function("base64gzip").expect("base64gzip exists");
        let regex_fn = get_stdlib_function("regex").expect("regex exists");
        let regexall_fn = get_stdlib_function("regexall").expect("regexall exists");

        let make_set = |items: Vec<&str>| -> Value {
            let mut s = BTreeSet::new();
            for item in items {
                s.insert(Value::new(
                    Type::String,
                    ValueData::String(item.to_string()),
                ));
            }
            Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(s))
        };

        let make_list = |items: Vec<&str>| -> Value {
            let mut l = Vec::new();
            for item in items {
                l.push(Value::new(
                    Type::String,
                    ValueData::String(item.to_string()),
                ));
            }
            Value::new(Type::List(Box::new(Type::String)), ValueData::Array(l))
        };

        // 1. setsymmetricdifference
        // Empty sets
        let empty_a = make_set(vec![]);
        let empty_b = make_set(vec![]);
        let res_empty = (diff_fn.func)(&[empty_a, empty_b]).expect("eval ok");
        if let ValueData::Set(s) = &*res_empty.data {
            assert!(s.is_empty());
        } else {
            panic!("expected set");
        }

        // Disjoint sets
        let set_a = make_set(vec!["a", "b"]);
        let set_b = make_set(vec!["c", "d"]);
        let res_disjoint = (diff_fn.func)(&[set_a, set_b]).expect("eval ok");
        if let ValueData::Set(s) = &*res_disjoint.data {
            assert_eq!(s.len(), 4);
        } else {
            panic!("expected set");
        }

        // Overlapping sets
        let set_1 = make_set(vec!["a", "b", "c"]);
        let set_2 = make_set(vec!["b", "c", "d"]);
        let res_overlap = (diff_fn.func)(&[set_1, set_2]).expect("eval ok");
        if let ValueData::Set(s) = &*res_overlap.data {
            assert_eq!(s.len(), 2);
            let strs: Vec<String> = s.iter().map(|v| v.to_string().replace('"', "")).collect();
            assert!(strs.contains(&"a".to_string()));
            assert!(strs.contains(&"d".to_string()));
        } else {
            panic!("expected set");
        }

        // Using lists convertible to sets
        let list_1 = make_list(vec!["x", "y"]);
        let list_2 = make_list(vec!["y", "z"]);
        let res_lists = (diff_fn.func)(&[list_1, list_2]).expect("eval ok");
        if let ValueData::Set(s) = &*res_lists.data {
            assert_eq!(s.len(), 2);
        } else {
            panic!("expected set");
        }

        // Marks and unknown propagation
        let marked_set = make_set(vec!["m"]).mark(ValueMark::Sensitive);
        let plain_set = make_set(vec!["n"]);
        let res_marked = (diff_fn.func)(&[marked_set, plain_set]).expect("eval ok");
        assert!(res_marked.has_mark(&ValueMark::Sensitive));

        let unk_set = Value::unknown(Type::Set(Box::new(Type::String))).mark(ValueMark::Sensitive);
        let res_unk = (diff_fn.func)(&[unk_set, make_set(vec![])]).expect("eval ok");
        assert!(res_unk.is_unknown());
        assert!(res_unk.has_mark(&ValueMark::Sensitive));

        // Error cases
        assert!((diff_fn.func)(&[]).is_err());
        assert!((diff_fn.func)(&[make_set(vec![])]).is_err());
        assert!(
            (diff_fn.func)(&[
                Value::new(Type::Number, ValueData::String("42".into())),
                make_set(vec![])
            ])
            .is_err()
        );

        // 2. base64gunzip
        // Roundtrip with base64gzip
        let input_text = Value::new(
            Type::String,
            ValueData::String("test_data_to_compress".to_string()),
        );
        let gzipped = (gzip_fn.func)(std::slice::from_ref(&input_text)).expect("gzip ok");
        let gunzipped = (gunzip_fn.func)(&[gzipped]).expect("gunzip ok");
        assert_eq!(*gunzipped.data, *input_text.data);

        // Empty string roundtrip
        let empty_text = Value::new(Type::String, ValueData::String(String::new()));
        let gzipped_empty =
            (gzip_fn.func)(std::slice::from_ref(&empty_text)).expect("gzip empty ok");
        let gunzipped_empty = (gunzip_fn.func)(&[gzipped_empty]).expect("gunzip empty ok");
        assert_eq!(*gunzipped_empty.data, *empty_text.data);

        // Marks and unknown propagation
        let unk_arg = Value::unknown(Type::String).mark(ValueMark::Sensitive);
        let res_unk_gunzip = (gunzip_fn.func)(&[unk_arg]).expect("gunzip unk ok");
        assert!(res_unk_gunzip.is_unknown());
        assert!(res_unk_gunzip.has_mark(&ValueMark::Sensitive));

        // Invalid base64 and corrupted gzip
        let bad_b64 = Value::new(Type::String, ValueData::String("!@#$notb64".to_string()));
        assert!((gunzip_fn.func)(&[bad_b64]).is_err());
        let bad_gzip = Value::new(Type::String, ValueData::String("aGVsbG8=".to_string())); // "hello" in base64, not gzip
        assert!((gunzip_fn.func)(&[bad_gzip]).is_err());
        assert!((gunzip_fn.func)(&[]).is_err());

        // 3. regex and regexall
        let str_val = |s: &str| Value::new(Type::String, ValueData::String(s.to_string()));

        // Named capture groups
        let res_named = (regex_fn.func)(&[
            str_val("(?P<proto>https?)://(?P<host>[^/]+)"),
            str_val("https://example.com/index.html"),
        ])
        .expect("named regex ok");
        if let ValueData::Object(map) = &*res_named.data {
            assert_eq!(
                map.get("proto").map(std::string::ToString::to_string),
                Some("\"https\"".to_string())
            );
            assert_eq!(
                map.get("host").map(std::string::ToString::to_string),
                Some("\"example.com\"".to_string())
            );
        } else {
            panic!("expected Object");
        }

        // Unnamed capture groups
        let res_unnamed = (regex_fn.func)(&[str_val("(\\d+)-(\\d+)"), str_val("12-34")])
            .expect("unnamed regex ok");
        if let ValueData::Array(arr) = &*res_unnamed.data {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0].to_string(), "\"12\"");
            assert_eq!(arr[1].to_string(), "\"34\"");
        } else {
            panic!("expected Array");
        }

        // No capture groups
        let res_no_cap = (regex_fn.func)(&[str_val("\\d+"), str_val("prefix123suffix")])
            .expect("no cap regex ok");
        assert_eq!(*res_no_cap.data, ValueData::String("123".to_string()));

        // Mixed named and unnamed capture groups -> Error
        assert!((regex_fn.func)(&[str_val("(?P<first>\\d+)-(\\d+)"), str_val("12-34"),]).is_err());

        // No match -> Error
        assert!((regex_fn.func)(&[str_val("nomatch"), str_val("hello"),]).is_err());

        // regexall: named groups
        let res_all_named = (regexall_fn.func)(&[
            str_val("(?P<word>[a-z]+):(?P<num>\\d+)"),
            str_val("foo:1 bar:2"),
        ])
        .expect("regexall named ok");
        if let ValueData::Array(arr) = &*res_all_named.data {
            assert_eq!(arr.len(), 2);
            if let ValueData::Object(first) = &*arr[0].data {
                assert_eq!(
                    first.get("word").map(std::string::ToString::to_string),
                    Some("\"foo\"".to_string())
                );
                assert_eq!(
                    first.get("num").map(std::string::ToString::to_string),
                    Some("\"1\"".to_string())
                );
            } else {
                panic!("expected Object");
            }
        } else {
            panic!("expected Array");
        }

        // regexall: unnamed groups
        let res_all_unnamed =
            (regexall_fn.func)(&[str_val("([a-z]+):(\\d+)"), str_val("foo:1 bar:2")])
                .expect("regexall unnamed ok");
        if let ValueData::Array(arr) = &*res_all_unnamed.data {
            assert_eq!(arr.len(), 2);
            if let ValueData::Array(first) = &*arr[0].data {
                assert_eq!(first.len(), 2);
                assert_eq!(first[0].to_string(), "\"foo\"");
                assert_eq!(first[1].to_string(), "\"1\"");
            } else {
                panic!("expected Array");
            }
        } else {
            panic!("expected Array");
        }

        // regexall: no capture groups
        let res_all_no_cap = (regexall_fn.func)(&[str_val("\\d+"), str_val("10 20 30")])
            .expect("regexall no cap ok");
        if let ValueData::Array(arr) = &*res_all_no_cap.data {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0].to_string(), "\"10\"");
            assert_eq!(arr[1].to_string(), "\"20\"");
            assert_eq!(arr[2].to_string(), "\"30\"");
        } else {
            panic!("expected Array");
        }

        // regexall: no match -> empty list
        let res_all_empty =
            (regexall_fn.func)(&[str_val("nomatch"), str_val("hello")]).expect("regexall empty ok");
        if let ValueData::Array(arr) = &*res_all_empty.data {
            assert_eq!(arr.as_slice(), []);
        } else {
            panic!("expected Array");
        }

        // regexall: mixed -> Error
        assert!(
            (regexall_fn.func)(&[str_val("(?P<first>\\d+)-(\\d+)"), str_val("12-34"),]).is_err()
        );
    }

    #[test]
    fn test_unicode_uax29_grapheme_parity() {
        let length_fn = get_stdlib_function("length").expect("length exists");
        let substr_fn = get_stdlib_function("substr").expect("substr exists");
        let strrev_fn = get_stdlib_function("strrev").expect("strrev exists");

        let str_val = |s: &str| {
            crate::types::Value::new(crate::types::Type::String, ValueData::String(s.to_string()))
        };
        let num_val = |n: i64| {
            crate::types::Value::new(
                crate::types::Type::Number,
                ValueData::Number(crate::number::Number::from(n)),
            )
        };

        // 1. Regional indicator flag emoji: "🇺🇸" (2 codepoints, 1 grapheme cluster)
        let flag = "🇺🇸";
        let len_res = (length_fn.func)(&[str_val(flag)]).expect("length ok");
        assert_eq!(len_res.to_string(), "1");

        let sub_res =
            (substr_fn.func)(&[str_val(flag), num_val(0), num_val(1)]).expect("substr ok");
        assert_eq!(sub_res.to_string(), "\"🇺🇸\"");

        let mixed_flag = "flag: 🇺🇸!";
        let mixed_len = (length_fn.func)(&[str_val(mixed_flag)]).expect("length ok");
        assert_eq!(mixed_len.to_string(), "8"); // "f","l","a","g",":"," ","🇺🇸","!"

        let sub_flag =
            (substr_fn.func)(&[str_val(mixed_flag), num_val(6), num_val(1)]).expect("substr ok");
        assert_eq!(sub_flag.to_string(), "\"🇺🇸\"");

        let rev_flag = (strrev_fn.func)(&[str_val("A🇺🇸B")]).expect("strrev ok");
        assert_eq!(rev_flag.to_string(), "\"B🇺🇸A\"");

        // 2. Zero-Width Joiner (ZWJ) family emoji: "👨‍👩‍👧‍👦" (7 codepoints, 1 grapheme cluster)
        let family = "👨‍👩‍👧‍👦";
        let fam_len = (length_fn.func)(&[str_val(family)]).expect("length ok");
        assert_eq!(fam_len.to_string(), "1");

        let fam_sub =
            (substr_fn.func)(&[str_val(family), num_val(0), num_val(1)]).expect("substr ok");
        assert_eq!(fam_sub.to_string(), "\"👨‍👩‍👧‍👦\"");

        let rev_fam = (strrev_fn.func)(&[str_val("X👨‍👩‍👧‍👦Y")]).expect("strrev ok");
        assert_eq!(rev_fam.to_string(), "\"Y👨‍👩‍👧‍👦X\"");

        // 3. Combining accent: "e\u{0301}" (e + combining acute accent -> 1 cluster "é")
        let accented = "e\u{0301}";
        let acc_len = (length_fn.func)(&[str_val(accented)]).expect("length ok");
        assert_eq!(acc_len.to_string(), "1");

        let acc_rev = (strrev_fn.func)(&[str_val(accented)]).expect("strrev ok");
        assert_eq!(acc_rev.to_string(), "\"e\u{0301}\"");

        let acc_word = "re\u{0301}sume\u{0301}";
        let word_len = (length_fn.func)(&[str_val(acc_word)]).expect("length ok");
        assert_eq!(word_len.to_string(), "6"); // r, é, s, u, m, é

        let word_sub =
            (substr_fn.func)(&[str_val(acc_word), num_val(1), num_val(2)]).expect("substr ok");
        assert_eq!(word_sub.to_string(), "\"e\u{0301}s\"");

        // 4. Skin tone modifier: "👍🏽" (1 cluster)
        let thumbs_up = "👍🏽";
        let thumb_len = (length_fn.func)(&[str_val(thumbs_up)]).expect("length ok");
        assert_eq!(thumb_len.to_string(), "1");

        let thumb_rev = (strrev_fn.func)(&[str_val(thumbs_up)]).expect("strrev ok");
        assert_eq!(thumb_rev.to_string(), "\"👍🏽\"");

        // 5. Negative offsets with grapheme clusters
        let neg_sub =
            (substr_fn.func)(&[str_val(mixed_flag), num_val(-2), num_val(1)]).expect("substr ok");
        assert_eq!(neg_sub.to_string(), "\"🇺🇸\"");
    }
}
