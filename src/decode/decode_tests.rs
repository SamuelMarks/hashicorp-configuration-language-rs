//! Unit tests for HCL structural and value decoding.
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
    fn test_decode_body_macro() -> Result<(), Box<dyn std::error::Error>> {
        let src = r#"name = "test_name"
count = 100"#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let config: MyConfig = DecodeBody::decode_body(&body, &[], &mut ctx)?;
        assert!(config.name.contains("test_name"));
        assert_eq!(config.count, 100);
        Ok(())
    }
    #[test]
    fn test_decode_body_for_body() -> Result<(), Box<dyn std::error::Error>> {
        let src = r#"name = "test""#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let decoded =
            <crate::ast::structure::Body as DecodeBody>::decode_body(&body, &[], &mut ctx)?;
        assert_eq!(decoded, body);
        Ok(())
    }
    const fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }
    fn make_val(data: ValueData) -> Value {
        Value::new(Type::Dynamic, data)
    }
    #[test]
    fn test_decode_value_int() -> Result<(), crate::error::HclError> {
        let val = make_val(ValueData::Number(Number(BigDecimal::from_str("42")?)));
        assert_eq!(i8::decode_value(&val, dummy_span())?, 42);
        assert_eq!(i16::decode_value(&val, dummy_span())?, 42);
        assert_eq!(i32::decode_value(&val, dummy_span())?, 42);
        assert_eq!(i64::decode_value(&val, dummy_span())?, 42);
        assert_eq!(i128::decode_value(&val, dummy_span())?, 42);
        assert_eq!(isize::decode_value(&val, dummy_span())?, 42);
        assert_eq!(u8::decode_value(&val, dummy_span())?, 42);
        assert_eq!(u16::decode_value(&val, dummy_span())?, 42);
        assert_eq!(u32::decode_value(&val, dummy_span())?, 42);
        assert_eq!(u64::decode_value(&val, dummy_span())?, 42);
        assert_eq!(u128::decode_value(&val, dummy_span())?, 42);
        assert_eq!(usize::decode_value(&val, dummy_span())?, 42);
        let err_val = make_val(ValueData::String("42".to_string()));
        assert!(i32::decode_value(&err_val, dummy_span()).is_err());
        assert!(u32::decode_value(&err_val, dummy_span()).is_err());
        let out_of_bounds = make_val(ValueData::Number(Number(BigDecimal::from_str("1e100")?)));
        assert!(i32::decode_value(&out_of_bounds, dummy_span()).is_err());
        Ok(())
    }
    #[test]
    fn test_decode_value_float() -> Result<(), crate::error::HclError> {
        let val = make_val(ValueData::Number(Number(BigDecimal::from_str("42.5")?)));
        assert!((f32::decode_value(&val, dummy_span())? - 42.5).abs() < f32::EPSILON);
        assert!((f64::decode_value(&val, dummy_span())? - 42.5).abs() < f64::EPSILON);
        let err_val = make_val(ValueData::String("42.5".to_string()));
        assert!(f32::decode_value(&err_val, dummy_span()).is_err());
        assert!(f64::decode_value(&err_val, dummy_span()).is_err());
        let overflow_val = make_val(ValueData::Number(Number(BigDecimal::from_str("1e1000")?)));
        assert!(f32::decode_value(&overflow_val, dummy_span()).is_err());
        assert!(f64::decode_value(&overflow_val, dummy_span()).is_err());
        Ok(())
    }
    #[test]
    fn test_decode_value_bool() -> Result<(), crate::error::HclError> {
        let val = make_val(ValueData::Bool(true));
        assert!(bool::decode_value(&val, dummy_span())?);
        let err_val = make_val(ValueData::Number(Number(BigDecimal::from_str("1")?)));
        assert!(bool::decode_value(&err_val, dummy_span()).is_err());
        Ok(())
    }
    #[test]
    fn test_decode_value_string() -> Result<(), crate::error::HclError> {
        let val = make_val(ValueData::String("hello".to_string()));
        assert_eq!(String::decode_value(&val, dummy_span())?, "hello");
        let err_val = make_val(ValueData::Bool(true));
        assert!(String::decode_value(&err_val, dummy_span()).is_err());
        Ok(())
    }
    #[test]
    fn test_decode_value_vec() -> Result<(), crate::error::HclError> {
        let arr_val = make_val(ValueData::Array(vec![
            make_val(ValueData::Number(Number(BigDecimal::from_str("1")?))),
            make_val(ValueData::Number(Number(BigDecimal::from_str("2")?))),
        ]));
        let vec: Vec<i32> = Vec::decode_value(&arr_val, dummy_span())?;
        assert_eq!(vec, vec![1, 2]);
        let mut set = BTreeSet::new();
        set.insert(make_val(ValueData::Number(Number(BigDecimal::from_str(
            "3",
        )?))));
        let set_val = make_val(ValueData::Set(set));
        let vec_from_set: Vec<i32> = Vec::decode_value(&set_val, dummy_span())?;
        assert_eq!(vec_from_set, vec![3]);
        let err_val = make_val(ValueData::Bool(true));
        assert!(Vec::<i32>::decode_value(&err_val, dummy_span()).is_err());
        let arr_err_val = make_val(ValueData::Array(vec![
            make_val(ValueData::Number(Number(BigDecimal::from_str("1")?))),
            make_val(ValueData::String("error".to_string())),
        ]));
        assert!(Vec::<i32>::decode_value(&arr_err_val, dummy_span()).is_err());
        let mut set_err = BTreeSet::new();
        set_err.insert(make_val(ValueData::String("error".to_string())));
        let set_err_val = make_val(ValueData::Set(set_err));
        assert!(Vec::<i32>::decode_value(&set_err_val, dummy_span()).is_err());
        Ok(())
    }
    #[test]
    fn test_decode_value_hashmap() -> Result<(), crate::error::HclError> {
        let mut map = BTreeMap::new();
        map.insert(
            "a".to_string(),
            make_val(ValueData::Number(Number(BigDecimal::from_str("1")?))),
        );
        let obj_val = make_val(ValueData::Object(map));
        let hashmap: HashMap<String, i32> = HashMap::decode_value(&obj_val, dummy_span())?;
        assert_eq!(hashmap.get("a"), Some(&1));
        let err_val = make_val(ValueData::Bool(true));
        assert!(HashMap::<String, i32>::decode_value(&err_val, dummy_span()).is_err());
        let mut err_map = BTreeMap::new();
        err_map.insert(
            "a".to_string(),
            make_val(ValueData::Number(Number(BigDecimal::from_str("1")?))),
        );
        err_map.insert(
            "b".to_string(),
            make_val(ValueData::String("error".to_string())),
        );
        let obj_err_val = make_val(ValueData::Object(err_map));
        assert!(HashMap::<String, i32>::decode_value(&obj_err_val, dummy_span()).is_err());
        Ok(())
    }
    #[test]
    fn test_decode_value_option() -> Result<(), crate::error::HclError> {
        let null_val = make_val(ValueData::Null);
        assert_eq!(Option::<i32>::decode_value(&null_val, dummy_span())?, None);
        let num_val = make_val(ValueData::Number(Number(BigDecimal::from_str("42")?)));
        assert_eq!(
            Option::<i32>::decode_value(&num_val, dummy_span())?,
            Some(42)
        );
        let unk_val = make_val(ValueData::Unknown(None));
        assert!(Option::<i32>::decode_value(&unk_val, dummy_span()).is_err());
        let res_unk = Option::<Value>::decode_value(&unk_val, dummy_span())?;
        assert_eq!(res_unk, Some(unk_val));
        Ok(())
    }
    #[test]
    fn test_decode_value_raw_value() -> Result<(), crate::error::HclError> {
        let val = make_val(ValueData::Bool(true));
        let decoded = Value::decode_value(&val, dummy_span())?;
        assert_eq!(decoded, val);
        Ok(())
    }
    #[test]
    fn test_decode_value_btreemap() -> Result<(), crate::error::HclError> {
        let mut map = BTreeMap::new();
        map.insert(
            "key1".to_string(),
            make_val(ValueData::Number(Number(BigDecimal::from_str("100")?))),
        );
        let obj_val = make_val(ValueData::Object(map));
        let btreemap: BTreeMap<String, i32> = BTreeMap::decode_value(&obj_val, dummy_span())?;
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
        Ok(())
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
    fn test_macro_remain_map_and_separated_attrs_blocks() -> Result<(), Box<dyn std::error::Error>>
    {
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
        let cfg_map: ConfigWithRemainMap = DecodeBody::decode_body(&body, &[], &mut ctx)?;
        assert_eq!(cfg_map.name, "service");
        assert!(cfg_map.extra.contains_key("timeout"));
        assert!(cfg_map.extra.contains_key("retries"));
        let cfg_sep: ConfigWithRemainAttrsAndBlocks =
            DecodeBody::decode_body(&body, &[], &mut ctx)?;
        assert_eq!(cfg_sep.name, "service");
        assert!(cfg_sep.extra_attrs.contains_key("timeout"));
        assert!(cfg_sep.extra_attrs.contains_key("retries"));
        assert_eq!(cfg_sep.extra_blocks.len(), 1);
        assert_eq!(cfg_sep.extra_blocks[0].block_type, "nested");
        Ok(())
    }
    #[test]
    fn test_macro_unevaluated_expr() -> Result<(), Box<dyn std::error::Error>> {
        let src = r#"
            name = "dynamic_service"
            raw = var.cluster.endpoint + "/v1"
            opt_raw = local.unresolved_attr
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let cfg: ConfigWithUnevaluatedExpr = DecodeBody::decode_body(&body, &[], &mut ctx)?;
        assert_eq!(cfg.name, "dynamic_service");
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
        Ok(())
    }
    #[test]
    fn test_macro_label_count_validation() -> Result<(), Box<dyn std::error::Error>> {
        let src = "enabled = true";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let labels_ok = vec!["aws_instance".to_string(), "web".to_string()];
        let res_ok: LabeledResource = DecodeBody::decode_body(&body, &labels_ok, &mut ctx)?;
        assert_eq!(res_ok.type_name, "aws_instance");
        assert_eq!(res_ok.instance_name, "web");
        assert!(res_ok.enabled);
        let labels_few = vec!["aws_instance".to_string()];
        let res_few: Result<LabeledResource, _> =
            DecodeBody::decode_body(&body, &labels_few, &mut ctx);
        let diags_few = res_few.err().ok_or("expected label count error")?;
        assert!(
            diags_few
                .to_string()
                .contains("Expected 2 block label(s), got 1")
        );
        let labels_many = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let res_many: Result<LabeledResource, _> =
            DecodeBody::decode_body(&body, &labels_many, &mut ctx);
        let diags_many = res_many.err().ok_or("expected label count error")?;
        assert!(
            diags_many
                .to_string()
                .contains("Expected 2 block label(s), got 3")
        );
        Ok(())
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
    fn test_derive_decode_value_struct_and_enum() -> Result<(), Box<dyn std::error::Error>> {
        use crate::encode::EncodeValue;
        let mut detail_map = BTreeMap::new();
        detail_map.insert("info".to_string(), "active-cluster".encode_value());
        let mut record_map = BTreeMap::new();
        record_map.insert("host_name".to_string(), "prod.internal".encode_value());
        record_map.insert("status".to_string(), "in_maintenance".encode_value());
        record_map.insert("port".to_string(), 8080_i64.encode_value());
        record_map.insert(
            "detail".to_string(),
            Value::new(Type::Dynamic, ValueData::Object(detail_map)),
        );
        let val = Value::new(Type::Dynamic, ValueData::Object(record_map));
        let decoded: ComplexRecord = crate::decode::DecodeValue::decode_value(&val, dummy_span())?;
        assert_eq!(decoded.host, "prod.internal");
        assert_eq!(decoded.status, ServerStatus::Maintenance);
        assert_eq!(decoded.port, Some(8080));
        assert_eq!(decoded.detail.info, "active-cluster");
        assert_eq!(decoded.detail.retries, 0);
        let empty_obj = Value::new(Type::Dynamic, ValueData::Object(BTreeMap::new()));
        assert!(ComplexRecord::decode_value(&empty_obj, dummy_span()).is_err());
        let str_val = "not_an_object".encode_value();
        assert!(ComplexRecord::decode_value(&str_val, dummy_span()).is_err());
        let unknown_variant = "unknown_status".encode_value();
        assert!(ServerStatus::decode_value(&unknown_variant, dummy_span()).is_err());
        let number_variant = 123_i64.encode_value();
        assert!(ServerStatus::decode_value(&number_variant, dummy_span()).is_err());
        Ok(())
    }
    #[test]
    fn test_decode_expression_and_context() -> Result<(), Box<dyn std::error::Error>> {
        use crate::encode::EncodeValue;
        let mut ctx = Context::new();
        ctx.set_variable("base_port", 9000_i64.encode_value());
        let mut parser = crate::parse::parser::Parser::new("base_port + 80");
        let expr = parser.parse_expression().ok_or("missing expr")?;
        let decoded: i64 = crate::decode::decode_expression(&expr, &mut ctx)?;
        assert_eq!(decoded, 9080);
        let mut err_parser = crate::parse::parser::Parser::new("undefined_var + 1");
        let err_expr = err_parser.parse_expression().ok_or("missing expr")?;
        assert!(crate::decode::decode_expression::<i64>(&err_expr, &mut ctx).is_err());
        let mut type_err_parser = crate::parse::parser::Parser::new(r#""not_a_number""#);
        let type_err_expr = type_err_parser.parse_expression().ok_or("missing expr")?;
        assert!(crate::decode::decode_expression::<i64>(&type_err_expr, &mut ctx).is_err());
        let val = "hello_world".encode_value();
        let decoded_str: String =
            crate::decode::decode_value_with_context(&val, dummy_span(), &ctx)?;
        assert_eq!(decoded_str, "hello_world");
        Ok(())
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
    fn test_explicit_hcl_attr_annotations() -> Result<(), Box<dyn std::error::Error>> {
        let src = "timeout = 30";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let config: ExplicitAttrConfig = DecodeBody::decode_body(&body, &[], &mut ctx)?;
        assert_eq!(config.timeout, 30);
        assert_eq!(config.description, None);
        assert_eq!(config.retries, 0);
        Ok(())
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
    fn test_decode_and_encode_single_level_label_maps() -> Result<(), Box<dyn std::error::Error>> {
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
        let config: ServerMapConfig = DecodeBody::decode_body(&body, &[], &mut ctx)?;
        assert_eq!(config.server.len(), 2);
        assert_eq!(config.server["web"].port, 80);
        assert_eq!(config.server["web"].host, "10.0.0.1");
        assert_eq!(config.server["api"].port, 8080);
        assert_eq!(config.database.len(), 2);
        assert_eq!(config.database["primary"].port, 5432);
        assert_eq!(config.database["replica"].port, 5433);
        let encoded_str = crate::encode::encode_to_string(&config)?;
        let mut rep_parser = Parser::new(&encoded_str);
        let rep_body = rep_parser.parse_body();
        let re_config: ServerMapConfig = DecodeBody::decode_body(&rep_body, &[], &mut ctx)?;
        assert_eq!(config, re_config);
        Ok(())
    }
    #[derive(DecodeBody, crate::encode::EncodeBody, Debug, PartialEq)]
    struct RepeatedMapConfig {
        #[hcl(block)]
        listener: HashMap<String, Vec<ServerBlock>>,
        #[hcl(block)]
        worker: BTreeMap<String, Vec<ServerBlock>>,
    }
    #[test]
    fn test_decode_and_encode_repeated_blocks_by_label() -> Result<(), Box<dyn std::error::Error>> {
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
        let config: RepeatedMapConfig = DecodeBody::decode_body(&body, &[], &mut ctx)?;
        assert_eq!(config.listener["http"].len(), 2);
        assert_eq!(config.listener["https"].len(), 1);
        assert_eq!(config.worker["queue"].len(), 2);
        let encoded_str = crate::encode::encode_to_string(&config)?;
        let mut rep_parser = Parser::new(&encoded_str);
        let rep_body = rep_parser.parse_body();
        let re_config: RepeatedMapConfig = DecodeBody::decode_body(&rep_body, &[], &mut ctx)?;
        assert_eq!(config.worker, re_config.worker);
        assert_eq!(
            config.listener["http"].len(),
            re_config.listener["http"].len()
        );
        Ok(())
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
    fn test_decode_and_encode_multi_level_label_maps() -> Result<(), Box<dyn std::error::Error>> {
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
        let config: NestedMultiLabelConfig = DecodeBody::decode_body(&body, &[], &mut ctx)?;
        assert_eq!(config.resource.len(), 2);
        assert_eq!(config.resource["aws_instance"]["web"].ami, "ami-web-123");
        assert_eq!(config.resource["aws_instance"]["api"].ami, "ami-api-456");
        assert_eq!(
            config.resource["aws_security_group"]["firewall"].ami,
            "sec-789"
        );
        let encoded_str = crate::encode::encode_to_string(&config)?;
        let mut rep_parser = Parser::new(&encoded_str);
        let rep_body = rep_parser.parse_body();
        let re_config: NestedMultiLabelConfig = DecodeBody::decode_body(&rep_body, &[], &mut ctx)?;
        assert_eq!(config, re_config);
        let src_3 = r#"
            target "cloud" "aws" "prod" {
                ami = "ami-prod"
            }
        "#;
        let mut p3 = Parser::new(src_3);
        let b3 = p3.parse_body();
        let c3: ThreeLevelMultiLabelConfig = DecodeBody::decode_body(&b3, &[], &mut ctx)?;
        assert_eq!(c3.target["cloud"]["aws"]["prod"].ami, "ami-prod");
        Ok(())
    }
    #[test]
    fn test_label_keyed_maps_error_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = Context::new();
        let src_missing_1 = r#"
            server {
                port = 80
                host = "localhost"
            }
        "#;
        let mut p1 = Parser::new(src_missing_1);
        let b1 = p1.parse_body();
        let err1 = ServerMapConfig::decode_body(&b1, &[], &mut ctx)
            .err()
            .ok_or("expected missing label error")?;
        assert!(err1.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Missing Label"))
        }));
        let src_missing_2 = r#"
            resource "aws_instance" {
                ami = "ami-123"
            }
        "#;
        let mut p2 = Parser::new(src_missing_2);
        let b2 = p2.parse_body();
        let err2 = NestedMultiLabelConfig::decode_body(&b2, &[], &mut ctx)
            .err()
            .ok_or("expected missing label error")?;
        assert!(err2.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Missing Label"))
        }));
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
        let err3 = ServerMapConfig::decode_body(&b3, &[], &mut ctx)
            .err()
            .ok_or("expected duplicate block error")?;
        assert!(err3.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Duplicate Block"))
        }));
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
            .ok_or("expected inner decode error")?;
        assert!(err4.has_errors());
        Ok(())
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
    fn test_flatten_multi_level_and_empty_and_remain() -> Result<(), Box<dyn std::error::Error>> {
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
        let decoded = FlatRootConfig::decode_body(&body, &[], &mut ctx)?;
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
        let schema = FlatRootConfig::implied_body_schema();
        assert!(schema.attributes.contains_key("env"));
        assert!(schema.attributes.contains_key("name"));
        assert!(schema.attributes.contains_key("engine"));
        assert!(schema.attributes.contains_key("port"));
        assert!(schema.blocks.contains_key("servers"));
        let mut cst = crate::cst::builder::CstBody::new();
        decoded.encode_into_body(&mut cst)?;
        let mut rendered = String::new();
        cst.render(&mut rendered);
        let mut round_parser = Parser::new(&rendered);
        let round_body = round_parser.parse_body();
        let mut round_ctx = Context::new();
        let round_decoded = FlatRootConfig::decode_body(&round_body, &[], &mut round_ctx)?;
        assert_eq!(decoded.env, round_decoded.env);
        assert_eq!(decoded.core.name, round_decoded.core.name);
        assert_eq!(
            decoded.core.database.engine,
            round_decoded.core.database.engine
        );
        assert_eq!(decoded.core.database.port, round_decoded.core.database.port);
        assert_eq!(decoded.core.servers.len(), round_decoded.core.servers.len());
        Ok(())
    }
    /// Tests optional flattened structs when inner attributes are present.
    #[test]
    fn test_flatten_optional_present() -> Result<(), Box<dyn std::error::Error>> {
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
        let decoded = FlatRootConfig::decode_body(&body, &[], &mut ctx)?;
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
        Ok(())
    }
    /// Struct colliding with child attribute.
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct DupAttrParent {
        #[hcl(attr)]
        pub engine: String,
        #[hcl(flatten)]
        pub child: FlatDatabaseConfig,
    }
    /// Struct colliding with child block.
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug)]
    struct DupBlockParent {
        #[hcl(block)]
        pub servers: Vec<FlatServerNode>,
        #[hcl(flatten)]
        pub child: FlatCoreConfig,
    }
    /// Sibling child struct A.
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug, PartialEq, Eq)]
    struct SiblingChildA {
        #[hcl(attr)]
        pub port: u16,
    }
    /// Sibling child struct B colliding with child A on attribute port.
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug, PartialEq, Eq)]
    struct SiblingChildB {
        #[hcl(attr)]
        pub port: u16,
    }
    /// Parent containing two sibling flattened structs with conflicting attributes.
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug, PartialEq, Eq)]
    struct SiblingConflictParent {
        #[hcl(flatten)]
        pub a: SiblingChildA,
        #[hcl(flatten)]
        pub b: SiblingChildB,
    }
    /// Tests runtime collision detection between parent and child attributes.
    #[test]
    fn test_flatten_collision_parent_child_attr() -> Result<(), Box<dyn std::error::Error>> {
        let src = r#"
            engine = "postgres"
            port = 5432
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let err = DupAttrParent::decode_body(&body, &[], &mut ctx)
            .map(|v| (v.engine, v.child))
            .err()
            .ok_or("expected duplicate attribute error")?;
        assert!(err.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Duplicate Attribute"))
        }));
        Ok(())
    }
    /// Tests runtime collision detection between parent and child blocks.
    #[test]
    fn test_flatten_collision_parent_child_block() -> Result<(), Box<dyn std::error::Error>> {
        let src = r#"
            name = "service"
            engine = "postgres"
            port = 5432
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let err = DupBlockParent::decode_body(&body, &[], &mut ctx)
            .map(|v| (v.servers, v.child))
            .err()
            .ok_or("expected duplicate block error")?;
        assert!(err.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Duplicate Block"))
        }));
        Ok(())
    }
    /// Tests runtime collision detection between two sibling flattened structs.
    #[test]
    fn test_flatten_collision_sibling_children() -> Result<(), Box<dyn std::error::Error>> {
        let src = "port = 8080";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let err = SiblingConflictParent::decode_body(&body, &[], &mut ctx)
            .map(|v| (v.a.port, v.b.port))
            .err()
            .ok_or("expected duplicate attribute error")?;
        assert!(err.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Duplicate Attribute"))
        }));
        Ok(())
    }
    /// Child with required attribute x.
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug, PartialEq, Eq)]
    struct ChildRequiredA {
        #[hcl(attr)]
        pub attr_a: String,
    }
    /// Child with required attribute y.
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug, PartialEq, Eq)]
    struct ChildRequiredB {
        #[hcl(attr)]
        pub attr_b: i64,
    }
    /// Parent containing both `ChildRequiredA` and `ChildRequiredB` for partial failure testing.
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug, PartialEq, Eq)]
    struct MultiFailureParent {
        #[hcl(flatten)]
        pub child_a: ChildRequiredA,
        #[hcl(flatten)]
        pub child_b: ChildRequiredB,
    }
    /// Tests partial failure recovery: both child structs report missing attributes in a single pass.
    #[test]
    fn test_flatten_partial_failure_recovery() -> Result<(), Box<dyn std::error::Error>> {
        let src = "# Empty configuration";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let err = MultiFailureParent::decode_body(&body, &[], &mut ctx)
            .map(|v| (v.child_a.attr_a, v.child_b.attr_b))
            .err()
            .ok_or("expected missing attributes error")?;
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
        Ok(())
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
    fn test_flatten_value_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
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
        let decoded = ValueOuter::decode_value(&val, dummy_span())?;
        assert_eq!(decoded.name, "test-val");
        assert_eq!(decoded.inner.host, "localhost");
        assert_eq!(decoded.inner.port, 8080);
        assert!(decoded.opt_inner.is_some());
        let encoded = decoded.encode_value();
        assert!(matches!(
            &*encoded.data,
            ValueData::Object(map)
                if map.contains_key("name")
                    && map.contains_key("host")
                    && map.contains_key("port")
        ));
        Ok(())
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
    fn test_raw_body_retention_and_hooks_success() -> Result<(), Box<dyn std::error::Error>> {
        let src = r##"
            title = "Dashboard"
            bg_color = "#FF00AA"
            extra_metric = "enabled"
        "##;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let decoded = RawBodyAndHooksConfig::decode_body(&body, &[], &mut ctx)?;
        assert_eq!(decoded.title, "Dashboard");
        assert_eq!(decoded.bg_color, HexColor("#FF00AA".to_string()));
        assert_eq!(decoded.port, 1024);
        assert_eq!(decoded.cluster, "us-east-cluster");
        assert!(decoded.raw_body.attributes.contains_key("title"));
        assert!(decoded.raw_body.attributes.contains_key("bg_color"));
        assert!(decoded.raw_body.attributes.contains_key("extra_metric"));
        assert!(decoded.remain.contains_key("extra_metric"));
        let mut cst = crate::cst::builder::CstBody::new();
        decoded.encode_into_body(&mut cst)?;
        let mut rendered = String::new();
        cst.render(&mut rendered);
        assert!(rendered.contains("title"));
        assert!(rendered.contains("extra_metric"));
        Ok(())
    }
    /// Tests custom decoder failure diagnostic reporting.
    #[test]
    fn test_custom_decoder_failure() -> Result<(), Box<dyn std::error::Error>> {
        let src = r#"
            title = "Bad Color"
            bg_color = "not-a-hex"
        "#;
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let err = RawBodyAndHooksConfig::decode_body(&body, &[], &mut ctx)
            .err()
            .ok_or("expected custom decoder error")?;
        assert!(err.errors().iter().any(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("Invalid Hex Color"))
        }));
        Ok(())
    }
    /// Struct testing fallback `default_expr` evaluation failure.
    #[derive(DecodeBody, hcl_macros::ImpliedBodySchema, Debug, PartialEq, Eq)]
    struct FallbackFailureConfig {
        #[hcl(attr, default_expr = "undefined_var + 1")]
        pub num: i64,
    }
    /// Tests that failure during fallback `default_expr` evaluation emits proper diagnostics.
    #[test]
    fn test_default_expr_fallback_failure() -> Result<(), Box<dyn std::error::Error>> {
        let src = "# Missing num";
        let mut parser = Parser::new(src);
        let body = parser.parse_body();
        let mut ctx = Context::new();
        let err = FallbackFailureConfig::decode_body(&body, &[], &mut ctx)
            .map(|v| v.num)
            .err()
            .ok_or("expected fallback evaluation error")?;
        assert!(err.has_errors());
        Ok(())
    }
}
