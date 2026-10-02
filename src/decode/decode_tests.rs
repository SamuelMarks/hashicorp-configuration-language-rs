#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]
#[cfg(test)]
mod tests {

    use crate::decode::{DecodeBody, DecodeValue};
    use crate::diagnostic::{Diagnostic, Diagnostics};
    use crate::encode::{EncodeBody, EncodeValue};
    use crate::eval::context::Context;
    use crate::number::Number;
    use crate::parse::parser::Parser;
    use crate::span::Span;
    use crate::types::ty::Type;
    use crate::types::val::{Value, ValueData};
    use bigdecimal::BigDecimal;
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    use std::str::FromStr;

    #[derive(DecodeBody)]
    struct MyConfig {
        name: String,
        count: i64,
    }

    #[test]
    fn test_decode_body_macro() {
        let src = "name = \"test_name\"\ncount = 100";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();

        let mut ctx = Context::new();
        let config: MyConfig = DecodeBody::decode_body(&body, &[], &mut ctx).unwrap();

        assert!(config.name.contains("test_name"));
        assert_eq!(config.count, 100);
    }

    #[test]
    fn test_decode_body_for_body() {
        let src = "name = \"test\"";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let decoded =
            <crate::ast::structure::Body as DecodeBody>::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(decoded, body);
    }

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn make_val(data: ValueData) -> Value {
        Value::new(Type::Dynamic, data)
    }

    #[test]
    fn test_decode_value_int() {
        let val = make_val(ValueData::Number(Number(
            BigDecimal::from_str("42").unwrap(),
        )));
        assert_eq!(i8::decode_value(&val, dummy_span()).unwrap(), 42);
        assert_eq!(i16::decode_value(&val, dummy_span()).unwrap(), 42);
        assert_eq!(i32::decode_value(&val, dummy_span()).unwrap(), 42);
        assert_eq!(i64::decode_value(&val, dummy_span()).unwrap(), 42);
        assert_eq!(i128::decode_value(&val, dummy_span()).unwrap(), 42);
        assert_eq!(isize::decode_value(&val, dummy_span()).unwrap(), 42);

        assert_eq!(u8::decode_value(&val, dummy_span()).unwrap(), 42);
        assert_eq!(u16::decode_value(&val, dummy_span()).unwrap(), 42);
        assert_eq!(u32::decode_value(&val, dummy_span()).unwrap(), 42);
        assert_eq!(u64::decode_value(&val, dummy_span()).unwrap(), 42);
        assert_eq!(u128::decode_value(&val, dummy_span()).unwrap(), 42);
        assert_eq!(usize::decode_value(&val, dummy_span()).unwrap(), 42);

        // Error cases
        let err_val = make_val(ValueData::String("42".to_string()));
        assert!(i32::decode_value(&err_val, dummy_span()).is_err());
        assert!(u32::decode_value(&err_val, dummy_span()).is_err());

        let out_of_bounds = make_val(ValueData::Number(Number(
            BigDecimal::from_str("1e100").unwrap(),
        )));
        assert!(i32::decode_value(&out_of_bounds, dummy_span()).is_err());
    }

    #[test]
    fn test_decode_value_float() {
        let val = make_val(ValueData::Number(Number(
            BigDecimal::from_str("42.5").unwrap(),
        )));
        assert_eq!(f32::decode_value(&val, dummy_span()).unwrap(), 42.5);
        assert_eq!(f64::decode_value(&val, dummy_span()).unwrap(), 42.5);

        let err_val = make_val(ValueData::String("42.5".to_string()));
        assert!(f32::decode_value(&err_val, dummy_span()).is_err());
        assert!(f64::decode_value(&err_val, dummy_span()).is_err());
    }

    #[test]
    fn test_decode_value_bool() {
        let val = make_val(ValueData::Bool(true));
        assert!(bool::decode_value(&val, dummy_span()).unwrap());
        let err_val = make_val(ValueData::Number(Number(
            BigDecimal::from_str("1").unwrap(),
        )));
        assert!(bool::decode_value(&err_val, dummy_span()).is_err());
    }

    #[test]
    fn test_decode_value_string() {
        let val = make_val(ValueData::String("hello".to_string()));
        assert_eq!(String::decode_value(&val, dummy_span()).unwrap(), "hello");

        let err_val = make_val(ValueData::Bool(true));
        assert!(String::decode_value(&err_val, dummy_span()).is_err());
    }

    #[test]
    fn test_decode_value_vec() {
        let arr_val = make_val(ValueData::Array(vec![
            make_val(ValueData::Number(Number(
                BigDecimal::from_str("1").unwrap(),
            ))),
            make_val(ValueData::Number(Number(
                BigDecimal::from_str("2").unwrap(),
            ))),
        ]));
        let vec: Vec<i32> = Vec::decode_value(&arr_val, dummy_span()).unwrap();
        assert_eq!(vec, vec![1, 2]);

        let mut set = BTreeSet::new();
        set.insert(make_val(ValueData::Number(Number(
            BigDecimal::from_str("3").unwrap(),
        ))));
        let set_val = make_val(ValueData::Set(set));
        let vec_from_set: Vec<i32> = Vec::decode_value(&set_val, dummy_span()).unwrap();
        assert_eq!(vec_from_set, vec![3]);

        let err_val = make_val(ValueData::Bool(true));
        assert!(Vec::<i32>::decode_value(&err_val, dummy_span()).is_err());

        // Error inside array
        let arr_err_val = make_val(ValueData::Array(vec![
            make_val(ValueData::Number(Number(
                BigDecimal::from_str("1").unwrap(),
            ))),
            make_val(ValueData::String("error".to_string())),
        ]));
        assert!(Vec::<i32>::decode_value(&arr_err_val, dummy_span()).is_err());

        // Error inside set
        let mut set_err = BTreeSet::new();
        set_err.insert(make_val(ValueData::String("error".to_string())));
        let set_err_val = make_val(ValueData::Set(set_err));
        assert!(Vec::<i32>::decode_value(&set_err_val, dummy_span()).is_err());
    }

    #[test]
    fn test_decode_value_hashmap() {
        let mut map = BTreeMap::new();
        map.insert(
            "a".to_string(),
            make_val(ValueData::Number(Number(
                BigDecimal::from_str("1").unwrap(),
            ))),
        );
        let obj_val = make_val(ValueData::Object(map));
        let hashmap: HashMap<String, i32> = HashMap::decode_value(&obj_val, dummy_span()).unwrap();
        assert_eq!(hashmap.get("a"), Some(&1));

        let err_val = make_val(ValueData::Bool(true));
        assert!(HashMap::<String, i32>::decode_value(&err_val, dummy_span()).is_err());

        let mut err_map = BTreeMap::new();
        err_map.insert(
            "a".to_string(),
            make_val(ValueData::Number(Number(
                BigDecimal::from_str("1").unwrap(),
            ))),
        );
        err_map.insert(
            "b".to_string(),
            make_val(ValueData::String("error".to_string())),
        );
        let obj_err_val = make_val(ValueData::Object(err_map));
        assert!(HashMap::<String, i32>::decode_value(&obj_err_val, dummy_span()).is_err());
    }

    #[test]
    fn test_decode_value_option() {
        let null_val = make_val(ValueData::Null);
        assert_eq!(
            Option::<i32>::decode_value(&null_val, dummy_span()).unwrap(),
            None
        );

        let num_val = make_val(ValueData::Number(Number(
            BigDecimal::from_str("42").unwrap(),
        )));
        assert_eq!(
            Option::<i32>::decode_value(&num_val, dummy_span()).unwrap(),
            Some(42)
        );

        let unk_val = make_val(ValueData::Unknown(None));
        // We know Option::decode_value delegating to T for Unknown will return T's error since T expects Number/String/etc.
        assert!(Option::<i32>::decode_value(&unk_val, dummy_span()).is_err());
        let res_unk = Option::<Value>::decode_value(&unk_val, dummy_span()).unwrap();
        assert_eq!(res_unk, Some(unk_val));
    }

    #[test]
    fn test_decode_value_raw_value() {
        let val = make_val(ValueData::Bool(true));
        let decoded = Value::decode_value(&val, dummy_span()).unwrap();
        assert_eq!(decoded, val);
    }

    #[test]
    fn test_decode_value_btreemap() {
        let mut map = BTreeMap::new();
        map.insert(
            "key1".to_string(),
            make_val(ValueData::Number(Number(
                BigDecimal::from_str("100").unwrap(),
            ))),
        );
        let obj_val = make_val(ValueData::Object(map));
        let btreemap: BTreeMap<String, i32> =
            BTreeMap::decode_value(&obj_val, dummy_span()).unwrap();
        assert_eq!(btreemap.get("key1"), Some(&100));

        let non_obj = make_val(ValueData::Bool(true));
        assert!(BTreeMap::<String, i32>::decode_value(&non_obj, dummy_span()).is_err());

        let mut err_map = BTreeMap::new();
        err_map.insert(
            "k".to_string(),
            make_val(ValueData::String("nan".to_string())),
        );
        let err_obj = make_val(ValueData::Object(err_map));
        assert!(BTreeMap::<String, i32>::decode_value(&err_obj, dummy_span()).is_err());
    }

    #[derive(Debug, DecodeBody)]
    struct ConfigWithRemainMap {
        name: String,
        #[hcl(remain)]
        extra: HashMap<String, Value>,
    }

    #[derive(Debug, DecodeBody)]
    struct ConfigWithRemainAttrsAndBlocks {
        name: String,
        #[hcl(remain_attrs)]
        extra_attrs: BTreeMap<String, Value>,
        #[hcl(remain_blocks)]
        extra_blocks: Vec<crate::ast::structure::Block>,
    }

    #[derive(Debug, DecodeBody)]
    struct ConfigWithUnevaluatedExpr {
        name: String,
        #[hcl(expr)]
        raw: crate::ast::expr::Expression,
        #[hcl(expr)]
        opt_raw: Option<std::sync::Arc<crate::ast::expr::Expression>>,
    }

    #[derive(Debug, DecodeBody)]
    struct LabeledResource {
        #[hcl(label)]
        type_name: String,
        #[hcl(label)]
        instance_name: String,
        enabled: bool,
    }

    #[test]
    fn test_macro_remain_map_and_separated_attrs_blocks() {
        let src = r#"
            name = "service"
            timeout = 30
            retries = 3

            nested "sub" {
                count = 1
            }
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        // 1. remain as HashMap<String, Value>
        let cfg_map: ConfigWithRemainMap = DecodeBody::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(cfg_map.name, "service");
        assert!(cfg_map.extra.contains_key("timeout"));
        assert!(cfg_map.extra.contains_key("retries"));

        // 2. remain_attrs and remain_blocks
        let cfg_sep: ConfigWithRemainAttrsAndBlocks =
            DecodeBody::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(cfg_sep.name, "service");
        assert!(cfg_sep.extra_attrs.contains_key("timeout"));
        assert!(cfg_sep.extra_attrs.contains_key("retries"));
        assert_eq!(cfg_sep.extra_blocks.len(), 1);
        assert_eq!(cfg_sep.extra_blocks[0].block_type, "nested");
    }

    #[test]
    fn test_macro_unevaluated_expr() {
        // Expressions with dynamic variables that aren't defined in Context
        let src = r#"
            name = "dynamic_service"
            raw = var.cluster.endpoint + "/v1"
            opt_raw = local.unresolved_attr
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new(); // Empty context, would fail if evaluated!

        let cfg: ConfigWithUnevaluatedExpr = DecodeBody::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(cfg.name, "dynamic_service");
        // Verify raw expression was captured without evaluating
        assert!(matches!(
            cfg.raw,
            crate::ast::expr::Expression::BinaryOp(..)
        ));
        assert!(cfg.opt_raw.is_some());
        if let Some(ref raw_expr) = cfg.opt_raw {
            assert!(matches!(
                **raw_expr,
                crate::ast::expr::Expression::Traversal(..)
            ));
        }
    }

    #[test]
    fn test_macro_label_count_validation() {
        let src = "enabled = true";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        // Exact match: 2 labels
        let labels_ok = vec!["aws_instance".to_string(), "web".to_string()];
        let res_ok: Result<LabeledResource, _> =
            DecodeBody::decode_body(&body, &labels_ok, &mut ctx);
        assert!(res_ok.is_ok());
        if let Ok(res) = res_ok {
            assert_eq!(res.type_name, "aws_instance");
            assert_eq!(res.instance_name, "web");
            assert!(res.enabled);
        }

        // Too few labels: 1 label
        let labels_few = vec!["aws_instance".to_string()];
        let res_few: Result<LabeledResource, _> =
            DecodeBody::decode_body(&body, &labels_few, &mut ctx);
        match res_few {
            Err(diags) => {
                assert!(
                    diags
                        .to_string()
                        .contains("Expected 2 block label(s), got 1")
                );
            }
            Ok(_) => panic!("expected label count error"),
        }

        // Too many labels: 3 labels
        let labels_many = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let res_many: Result<LabeledResource, _> =
            DecodeBody::decode_body(&body, &labels_many, &mut ctx);
        match res_many {
            Err(diags) => {
                assert!(
                    diags
                        .to_string()
                        .contains("Expected 2 block label(s), got 3")
                );
            }
            Ok(_) => panic!("expected label count error"),
        }
    }

    #[derive(Debug, PartialEq, crate::decode::DecodeValue)]
    enum ServerStatus {
        Running,
        #[hcl(name = "in_maintenance")]
        Maintenance,
        Stopped,
    }

    #[derive(Debug, PartialEq, crate::decode::DecodeValue)]
    struct NestedDetail {
        info: String,
        #[hcl(default)]
        retries: i64,
    }

    #[derive(Debug, PartialEq, crate::decode::DecodeValue)]
    struct ComplexRecord {
        #[hcl(name = "host_name")]
        host: String,
        status: ServerStatus,
        port: Option<i64>,
        detail: NestedDetail,
    }

    #[test]
    fn test_derive_decode_value_struct_and_enum() {
        use crate::encode::EncodeValue;
        let mut detail_map = BTreeMap::new();
        detail_map.insert("info".to_string(), "active-cluster".encode_value());
        // retries omitted -> will use default (0)

        let mut record_map = BTreeMap::new();
        record_map.insert("host_name".to_string(), "prod.internal".encode_value());
        record_map.insert("status".to_string(), "in_maintenance".encode_value());
        record_map.insert("port".to_string(), 8080_i64.encode_value());
        record_map.insert(
            "detail".to_string(),
            Value::new(Type::Dynamic, ValueData::Object(detail_map)),
        );

        let val = Value::new(Type::Dynamic, ValueData::Object(record_map));
        let decoded: ComplexRecord =
            crate::decode::DecodeValue::decode_value(&val, dummy_span()).unwrap();

        assert_eq!(decoded.host, "prod.internal");
        assert_eq!(decoded.status, ServerStatus::Maintenance);
        assert_eq!(decoded.port, Some(8080));
        assert_eq!(decoded.detail.info, "active-cluster");
        assert_eq!(decoded.detail.retries, 0);

        // Test missing required field
        let empty_obj = Value::new(Type::Dynamic, ValueData::Object(BTreeMap::new()));
        assert!(ComplexRecord::decode_value(&empty_obj, dummy_span()).is_err());

        // Test type mismatch (expected object, got string)
        let str_val = "not_an_object".encode_value();
        assert!(ComplexRecord::decode_value(&str_val, dummy_span()).is_err());

        // Test enum errors
        let unknown_variant = "unknown_status".encode_value();
        assert!(ServerStatus::decode_value(&unknown_variant, dummy_span()).is_err());
        let number_variant = 123_i64.encode_value();
        assert!(ServerStatus::decode_value(&number_variant, dummy_span()).is_err());
    }

    #[test]
    fn test_decode_expression_and_context() {
        use crate::encode::EncodeValue;
        let mut ctx = Context::new();
        ctx.set_variable("base_port", 9000_i64.encode_value());

        let mut parser = crate::parse::parser::Parser::new("base_port + 80");
        let expr = parser.parse_expression().unwrap();

        let decoded: i64 = crate::decode::decode_expression(&expr, &mut ctx).unwrap();
        assert_eq!(decoded, 9080);

        // Evaluation error branch
        let mut err_parser = crate::parse::parser::Parser::new("undefined_var + 1");
        let err_expr = err_parser.parse_expression().unwrap();
        assert!(crate::decode::decode_expression::<i64>(&err_expr, &mut ctx).is_err());

        // Type decoding error branch (evaluated to string, but expected i64)
        let mut type_err_parser = crate::parse::parser::Parser::new("\"not_a_number\"");
        let type_err_expr = type_err_parser.parse_expression().unwrap();
        assert!(crate::decode::decode_expression::<i64>(&type_err_expr, &mut ctx).is_err());

        // decode_value_with_context
        let val = "hello_world".encode_value();
        let decoded_str: String =
            crate::decode::decode_value_with_context(&val, dummy_span(), &ctx).unwrap();
        assert_eq!(decoded_str, "hello_world");
    }

    #[derive(DecodeBody)]
    struct ExplicitAttrConfig {
        #[hcl(attr)]
        timeout: i64,
        #[hcl(attr, optional)]
        description: Option<String>,
        #[hcl(attr, default)]
        retries: i64,
    }

    #[test]
    fn test_explicit_hcl_attr_annotations() {
        let src = "timeout = 30";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();

        let mut ctx = Context::new();
        let config: ExplicitAttrConfig = DecodeBody::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(config.timeout, 30);
        assert_eq!(config.description, None);
        assert_eq!(config.retries, 0);
    }

    #[derive(DecodeBody, crate::encode::EncodeBody, Debug, PartialEq)]
    struct ServerBlock {
        port: i64,
        host: String,
    }

    #[derive(DecodeBody, crate::encode::EncodeBody, Debug, PartialEq)]
    struct ServerMapConfig {
        #[hcl(block)]
        server: HashMap<String, ServerBlock>,
        #[hcl(block)]
        database: BTreeMap<String, ServerBlock>,
    }

    #[test]
    fn test_decode_and_encode_single_level_label_maps() {
        let src = r#"
            server "web" {
                port = 80
                host = "10.0.0.1"
            }
            server "api" {
                port = 8080
                host = "10.0.0.2"
            }
            database "primary" {
                port = 5432
                host = "db1.local"
            }
            database "replica" {
                port = 5433
                host = "db2.local"
            }
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let config: ServerMapConfig = DecodeBody::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(config.server.len(), 2);
        assert_eq!(config.server["web"].port, 80);
        assert_eq!(config.server["web"].host, "10.0.0.1");
        assert_eq!(config.server["api"].port, 8080);
        assert_eq!(config.database.len(), 2);
        assert_eq!(config.database["primary"].port, 5432);
        assert_eq!(config.database["replica"].port, 5433);

        // Test EncodeBody
        let encoded_str = crate::encode::encode_to_string(&config).unwrap();
        let mut rep_parser = Parser::new(&encoded_str);
        let rep_body = rep_parser.parse_body();
        let re_config: ServerMapConfig = DecodeBody::decode_body(&rep_body, &[], &mut ctx).unwrap();
        assert_eq!(config, re_config);
    }

    #[derive(DecodeBody, crate::encode::EncodeBody, Debug, PartialEq)]
    struct RepeatedMapConfig {
        #[hcl(block)]
        listener: HashMap<String, Vec<ServerBlock>>,
        #[hcl(block)]
        worker: BTreeMap<String, Vec<ServerBlock>>,
    }

    #[test]
    fn test_decode_and_encode_repeated_blocks_by_label() {
        let src = r#"
            listener "http" {
                port = 80
                host = "0.0.0.0"
            }
            listener "http" {
                port = 8080
                host = "127.0.0.1"
            }
            listener "https" {
                port = 443
                host = "0.0.0.0"
            }
            worker "queue" {
                port = 1
                host = "q1"
            }
            worker "queue" {
                port = 2
                host = "q2"
            }
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let config: RepeatedMapConfig = DecodeBody::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(config.listener["http"].len(), 2);
        assert_eq!(config.listener["https"].len(), 1);
        assert_eq!(config.worker["queue"].len(), 2);

        // Encode and round-trip
        let encoded_str = crate::encode::encode_to_string(&config).unwrap();
        let mut rep_parser = Parser::new(&encoded_str);
        let rep_body = rep_parser.parse_body();
        let re_config: RepeatedMapConfig =
            DecodeBody::decode_body(&rep_body, &[], &mut ctx).unwrap();
        assert_eq!(config.worker, re_config.worker);
        assert_eq!(
            config.listener["http"].len(),
            re_config.listener["http"].len()
        );
    }

    #[derive(DecodeBody, crate::encode::EncodeBody, Debug, PartialEq)]
    struct ResourceBlock {
        ami: String,
    }

    #[derive(DecodeBody, crate::encode::EncodeBody, Debug, PartialEq)]
    struct NestedMultiLabelConfig {
        #[hcl(block)]
        resource: HashMap<String, HashMap<String, ResourceBlock>>,
    }

    #[derive(DecodeBody, crate::encode::EncodeBody, Debug, PartialEq)]
    struct ThreeLevelMultiLabelConfig {
        #[hcl(block)]
        target: HashMap<String, HashMap<String, BTreeMap<String, ResourceBlock>>>,
    }

    #[test]
    fn test_decode_and_encode_multi_level_label_maps() {
        let src = r#"
            resource "aws_instance" "web" {
                ami = "ami-web-123"
            }
            resource "aws_instance" "api" {
                ami = "ami-api-456"
            }
            resource "aws_security_group" "firewall" {
                ami = "sec-789"
            }
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let config: NestedMultiLabelConfig = DecodeBody::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(config.resource.len(), 2);
        assert_eq!(config.resource["aws_instance"]["web"].ami, "ami-web-123");
        assert_eq!(config.resource["aws_instance"]["api"].ami, "ami-api-456");
        assert_eq!(
            config.resource["aws_security_group"]["firewall"].ami,
            "sec-789"
        );

        // Round-trip
        let encoded_str = crate::encode::encode_to_string(&config).unwrap();
        let mut rep_parser = Parser::new(&encoded_str);
        let rep_body = rep_parser.parse_body();
        let re_config: NestedMultiLabelConfig =
            DecodeBody::decode_body(&rep_body, &[], &mut ctx).unwrap();
        assert_eq!(config, re_config);

        // 3-level map
        let src_3 = r#"
            target "cloud" "aws" "prod" {
                ami = "ami-prod"
            }
        "#;
        let mut p3 = Parser::new(src_3);
        let b3 = p3.parse_body();
        let c3: ThreeLevelMultiLabelConfig = DecodeBody::decode_body(&b3, &[], &mut ctx).unwrap();
        assert_eq!(c3.target["cloud"]["aws"]["prod"].ami, "ami-prod");
    }

    #[test]
    fn test_label_keyed_maps_error_diagnostics() {
        let mut ctx = Context::new();

        // 1. Missing label on 1-level map (0 labels provided)
        let src_missing_1 = r#"
            server {
                port = 80
                host = "localhost"
            }
        "#;
        let mut p1 = Parser::new(src_missing_1);
        let b1 = p1.parse_body();
        let err1 = ServerMapConfig::decode_body(&b1, &[], &mut ctx).unwrap_err();
        assert!(err1.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Missing Label"))
        }));

        // 2. Missing label on 2-level map (only 1 label provided when 2 required)
        let src_missing_2 = r#"
            resource "aws_instance" {
                ami = "ami-123"
            }
        "#;
        let mut p2 = Parser::new(src_missing_2);
        let b2 = p2.parse_body();
        let err2 = NestedMultiLabelConfig::decode_body(&b2, &[], &mut ctx).unwrap_err();
        assert!(err2.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Missing Label"))
        }));

        // 3. Duplicate block error on single-item map (same label defined twice)
        let src_duplicate = r#"
            server "web" {
                port = 80
                host = "10.0.0.1"
            }
            server "web" {
                port = 8080
                host = "10.0.0.2"
            }
        "#;
        let mut p3 = Parser::new(src_duplicate);
        let b3 = p3.parse_body();
        let err3 = ServerMapConfig::decode_body(&b3, &[], &mut ctx).unwrap_err();
        assert!(err3.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Duplicate Block"))
        }));

        // 4. Inner decoding failure (port is a string, expecting integer)
        let src_inner_err = r#"
            server "web" {
                port = "not_an_int"
                host = "10.0.0.1"
            }
        "#;
        let mut p4 = Parser::new(src_inner_err);
        let b4 = p4.parse_body();
        let err4 = ServerMapConfig::decode_body(&b4, &[], &mut ctx)
            .err()
            .unwrap();
        assert!(err4.has_errors());
    }

    /// Child database configuration struct.
    #[derive(
        DecodeBody,
        EncodeBody,
        hcl_macros::ImpliedBodySchema,
        PartialEq,
        Debug,
        DecodeValue,
        EncodeValue,
    )]
    struct FlatDatabaseConfig {
        #[hcl(attr)]
        engine: String,
        #[hcl(attr)]
        port: u16,
    }

    /// Child server node block representation.
    #[derive(DecodeBody, EncodeBody, hcl_macros::ImpliedBodySchema, PartialEq, Debug)]
    struct FlatServerNode {
        #[hcl(attr)]
        host: String,
        #[hcl(attr)]
        active: bool,
    }

    /// Intermediate configuration struct containing child attributes, blocks, and flattened child.
    #[derive(DecodeBody, EncodeBody, hcl_macros::ImpliedBodySchema, PartialEq, Debug)]
    struct FlatCoreConfig {
        #[hcl(attr)]
        name: String,
        #[hcl(flatten)]
        database: FlatDatabaseConfig,
        #[hcl(block)]
        servers: Vec<FlatServerNode>,
    }

    /// Empty struct for testing zero-field flattened structs.
    #[derive(DecodeBody, EncodeBody, hcl_macros::ImpliedBodySchema, PartialEq, Debug)]
    struct FlatEmptyConfig {}

    /// Optional extra configuration struct.
    #[derive(DecodeBody, EncodeBody, hcl_macros::ImpliedBodySchema, PartialEq, Debug)]
    struct FlatExtraConfig {
        #[hcl(attr)]
        extra_tag: Option<String>,
    }

    /// Root configuration struct testing multiple levels of flattening, squashing, empty structs, and remain.
    #[derive(DecodeBody, EncodeBody, hcl_macros::ImpliedBodySchema, PartialEq, Debug)]
    struct FlatRootConfig {
        #[hcl(attr)]
        env: String,
        #[hcl(squash)]
        core: FlatCoreConfig,
        #[hcl(flatten)]
        empty: FlatEmptyConfig,
        #[hcl(flatten)]
        opt_extra: Option<FlatExtraConfig>,
        #[hcl(remain_attrs)]
        remain: HashMap<String, Value>,
    }

    /// Tests multiple levels of flattened and squashed structs, empty structs, and remain capture.
    #[test]
    fn test_flatten_multi_level_and_empty_and_remain() {
        use crate::ast::schema::ImpliedBodySchema;

        let src = r#"
            env = "production"
            name = "payment-gateway"
            engine = "postgres"
            port = 5432
            servers {
                host = "node-1.internal"
                active = true
            }
            extra_token = "secure_abc"
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let decoded = FlatRootConfig::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(decoded.env, "production");
        assert_eq!(decoded.core.name, "payment-gateway");
        assert_eq!(decoded.core.database.engine, "postgres");
        assert_eq!(decoded.core.database.port, 5432);
        assert_eq!(decoded.core.servers.len(), 1);
        assert_eq!(decoded.core.servers[0].host, "node-1.internal");
        assert!(decoded.core.servers[0].active);
        assert_eq!(decoded.empty, FlatEmptyConfig {});
        assert_eq!(decoded.opt_extra, None);
        assert_eq!(decoded.remain.len(), 1);
        assert!(decoded.remain.contains_key("extra_token"));

        // Verify ImpliedBodySchema merges child schemas across multiple levels
        let schema = FlatRootConfig::implied_body_schema();
        assert!(schema.attributes.contains_key("env"));
        assert!(schema.attributes.contains_key("name"));
        assert!(schema.attributes.contains_key("engine"));
        assert!(schema.attributes.contains_key("port"));
        assert!(schema.blocks.contains_key("servers"));

        // Verify round-trip EncodeBody -> decode
        let mut cst = crate::cst::builder::CstBody::new();
        decoded.encode_into_body(&mut cst).unwrap();
        let mut rendered = String::new();
        cst.render(&mut rendered);

        let mut round_parser = Parser::new(&rendered);
        let round_body = round_parser.parse_body();
        let mut round_ctx = Context::new();
        let round_decoded = FlatRootConfig::decode_body(&round_body, &[], &mut round_ctx).unwrap();
        assert_eq!(decoded.env, round_decoded.env);
        assert_eq!(decoded.core.name, round_decoded.core.name);
        assert_eq!(
            decoded.core.database.engine,
            round_decoded.core.database.engine
        );
        assert_eq!(decoded.core.database.port, round_decoded.core.database.port);
        assert_eq!(decoded.core.servers.len(), round_decoded.core.servers.len());
    }

    /// Tests optional flattened structs when inner attributes are present.
    #[test]
    fn test_flatten_optional_present() {
        let src = r#"
            env = "staging"
            name = "auth-service"
            engine = "mysql"
            port = 3306
            extra_tag = "auth-v2"
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let decoded = FlatRootConfig::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(decoded.env, "staging");
        assert_eq!(decoded.core.name, "auth-service");
        assert_eq!(decoded.core.database.engine, "mysql");
        assert_eq!(decoded.core.database.port, 3306);
        assert_eq!(
            decoded.opt_extra,
            Some(FlatExtraConfig {
                extra_tag: Some("auth-v2".to_string())
            })
        );
    }

    /// Struct colliding with child attribute.
    #[allow(dead_code)]
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct DupAttrParent {
        #[hcl(attr)]
        engine: String,
        #[hcl(flatten)]
        child: FlatDatabaseConfig,
    }

    /// Struct colliding with child block.
    #[allow(dead_code)]
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct DupBlockParent {
        #[hcl(block)]
        servers: Vec<FlatServerNode>,
        #[hcl(flatten)]
        child: FlatCoreConfig,
    }

    /// Sibling child struct A.
    #[allow(dead_code)]
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct SiblingChildA {
        #[hcl(attr)]
        port: u16,
    }

    /// Sibling child struct B colliding with child A on attribute port.
    #[allow(dead_code)]
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct SiblingChildB {
        #[hcl(attr)]
        port: u16,
    }

    /// Parent containing two sibling flattened structs with conflicting attributes.
    #[allow(dead_code)]
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct SiblingConflictParent {
        #[hcl(flatten)]
        a: SiblingChildA,
        #[hcl(flatten)]
        b: SiblingChildB,
    }

    /// Tests runtime collision detection between parent and child attributes.
    #[test]
    fn test_flatten_collision_parent_child_attr() {
        let src = r#"
            engine = "postgres"
            port = 5432
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let err = DupAttrParent::decode_body(&body, &[], &mut ctx)
            .err()
            .unwrap();
        assert!(err.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Duplicate Attribute"))
        }));
    }

    /// Tests runtime collision detection between parent and child blocks.
    #[test]
    fn test_flatten_collision_parent_child_block() {
        let src = r#"
            name = "service"
            engine = "postgres"
            port = 5432
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let err = DupBlockParent::decode_body(&body, &[], &mut ctx)
            .err()
            .unwrap();
        assert!(err.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Duplicate Block"))
        }));
    }

    /// Tests runtime collision detection between two sibling flattened structs.
    #[test]
    fn test_flatten_collision_sibling_children() {
        let src = "port = 8080";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let err = SiblingConflictParent::decode_body(&body, &[], &mut ctx)
            .err()
            .unwrap();
        assert!(err.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Duplicate Attribute"))
        }));
    }

    /// Child with required attribute x.
    #[allow(dead_code)]
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct ChildRequiredA {
        #[hcl(attr)]
        attr_a: String,
    }

    /// Child with required attribute y.
    #[allow(dead_code)]
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct ChildRequiredB {
        #[hcl(attr)]
        attr_b: i64,
    }

    /// Parent containing both `ChildRequiredA` and `ChildRequiredB` for partial failure testing.
    #[allow(dead_code)]
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct MultiFailureParent {
        #[hcl(flatten)]
        child_a: ChildRequiredA,
        #[hcl(flatten)]
        child_b: ChildRequiredB,
    }

    /// Tests partial failure recovery: both child structs report missing attributes in a single pass.
    #[test]
    fn test_flatten_partial_failure_recovery() {
        // Body is completely empty: both attr_a and attr_b are missing
        let src = "# Empty configuration";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let err = MultiFailureParent::decode_body(&body, &[], &mut ctx)
            .err()
            .unwrap();
        let diags = err.errors();
        assert!(
            diags.len() >= 2,
            "Expected at least 2 diagnostics for partial failure recovery, got {}",
            diags.len()
        );
        let has_err_a = diags
            .iter()
            .any(|e| e.detail.as_deref().is_some_and(|d| d.contains("attr_a")));
        let has_err_b = diags
            .iter()
            .any(|e| e.detail.as_deref().is_some_and(|d| d.contains("attr_b")));
        assert!(has_err_a, "Expected missing attr_a diagnostic");
        assert!(has_err_b, "Expected missing attr_b diagnostic");
    }

    /// Inner value struct for object testing.
    #[derive(DecodeValue, EncodeValue, PartialEq, Debug)]
    struct ValueInner {
        host: String,
        port: u16,
    }

    /// Outer value struct flattening `ValueInner`.
    #[derive(DecodeValue, EncodeValue, PartialEq, Debug)]
    struct ValueOuter {
        name: String,
        #[hcl(flatten)]
        inner: ValueInner,
        #[hcl(squash)]
        opt_inner: Option<ValueInner>,
    }

    /// Tests `DecodeValue` and `EncodeValue` with flattened and squashed inner structs.
    #[test]
    fn test_flatten_value_roundtrip() {
        let mut obj = BTreeMap::new();
        obj.insert(
            "name".to_string(),
            make_val(ValueData::String("test-val".to_string())),
        );
        obj.insert(
            "host".to_string(),
            make_val(ValueData::String("localhost".to_string())),
        );
        obj.insert(
            "port".to_string(),
            make_val(ValueData::Number(Number(BigDecimal::from(8080)))),
        );

        let val = make_val(ValueData::Object(obj));
        let decoded = ValueOuter::decode_value(&val, dummy_span()).unwrap();
        assert_eq!(decoded.name, "test-val");
        assert_eq!(decoded.inner.host, "localhost");
        assert_eq!(decoded.inner.port, 8080);
        assert!(decoded.opt_inner.is_some());

        let encoded = decoded.encode_value();
        if let ValueData::Object(map) = &*encoded.data {
            assert!(map.contains_key("name"));
            assert!(map.contains_key("host"));
            assert!(map.contains_key("port"));
        } else {
            panic!("Expected object data");
        }
    }

    /// Validated hex color wrapper type.
    #[derive(PartialEq, Debug, Clone)]
    struct HexColor(String);

    impl crate::encode::EncodeValue for HexColor {
        fn encode_value(&self) -> Value {
            Value::new(
                crate::types::ty::Type::String,
                ValueData::String(self.0.clone()),
            )
        }
    }

    /// Custom decoder parsing hex color strings into `HexColor`.
    fn custom_hex_color_decoder(
        val: &Value,
        span: Span,
        _ctx: &Context,
    ) -> Result<HexColor, Diagnostics> {
        if let ValueData::String(s) = &*val.data {
            if s.starts_with('#') && s.len() == 7 {
                return Ok(HexColor(s.clone()));
            }
            return Err(Diagnostics::from(Diagnostic::error(
                "Invalid Hex Color",
                format!("Expected hex color format #RRGGBB, got '{s}'"),
                span,
            )));
        }
        Err(Diagnostics::from(Diagnostic::error(
            "Type Mismatch",
            format!("Expected string for hex color, got {}", val.ty()),
            span,
        )))
    }

    /// Factory function providing default cluster name.
    fn default_cluster_provider() -> String {
        "us-east-cluster".to_string()
    }

    /// Configuration struct testing raw body retention, custom decoder, `default_expr`, and `default_fn`.
    #[derive(DecodeBody, EncodeBody, hcl_macros::ImpliedBodySchema, PartialEq, Debug)]
    struct RawBodyAndHooksConfig {
        #[hcl(attr)]
        title: String,
        #[hcl(body)]
        raw_body: crate::ast::structure::Body,
        #[hcl(attr, with = "custom_hex_color_decoder")]
        bg_color: HexColor,
        #[hcl(attr, default_expr = "1000 + 24")]
        port: u16,
        #[hcl(attr, default_fn = "default_cluster_provider")]
        cluster: String,
        #[hcl(remain_attrs)]
        remain: HashMap<String, Value>,
    }

    /// Tests successful raw body retention, custom decoder invocation, default expression evaluation, and default fn instantiation.
    #[test]
    fn test_raw_body_retention_and_hooks_success() {
        let src = r##"
            title = "Dashboard"
            bg_color = "#FF00AA"
            extra_metric = "enabled"
        "##;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let decoded = RawBodyAndHooksConfig::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(decoded.title, "Dashboard");
        assert_eq!(decoded.bg_color, HexColor("#FF00AA".to_string()));
        assert_eq!(decoded.port, 1024);
        assert_eq!(decoded.cluster, "us-east-cluster");
        // Verify unparsed body captured all attributes
        assert!(decoded.raw_body.attributes.contains_key("title"));
        assert!(decoded.raw_body.attributes.contains_key("bg_color"));
        assert!(decoded.raw_body.attributes.contains_key("extra_metric"));
        // Verify raw_body did NOT remove attributes from remain
        assert!(decoded.remain.contains_key("extra_metric"));

        // Verify EncodeBody on struct with raw Body field
        let mut cst = crate::cst::builder::CstBody::new();
        decoded.encode_into_body(&mut cst).unwrap();
        let mut rendered = String::new();
        cst.render(&mut rendered);
        assert!(rendered.contains("title"));
        assert!(rendered.contains("extra_metric"));
    }

    /// Tests custom decoder failure diagnostic reporting.
    #[test]
    fn test_custom_decoder_failure() {
        let src = r#"
            title = "Bad Color"
            bg_color = "not-a-hex"
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let err = RawBodyAndHooksConfig::decode_body(&body, &[], &mut ctx)
            .err()
            .unwrap();
        assert!(err.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Invalid Hex Color"))
        }));
    }

    /// Struct testing fallback `default_expr` evaluation failure.
    #[allow(dead_code)]
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct FallbackFailureConfig {
        #[hcl(attr, default_expr = "undefined_var + 1")]
        num: i64,
    }

    /// Tests that failure during fallback `default_expr` evaluation emits proper diagnostics.
    #[test]
    fn test_default_expr_fallback_failure() {
        // Missing "num" attribute causes default_expr evaluation of undefined variable
        let src = "# Missing num";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();

        let err = FallbackFailureConfig::decode_body(&body, &[], &mut ctx)
            .err()
            .unwrap();
        assert!(err.has_errors());
    }
}
