#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]
#[cfg(test)]
mod tests {
    use crate::cst::builder::CstBody;
    use crate::decode::DecodeBody;
    use crate::diagnostic::Diagnostics;
    use crate::encode::{EncodeBody, EncodeValue, encode_to_string};
    use crate::number::Number;
    use crate::types::ty::Type;
    use crate::types::val::{Value, ValueData};
    use std::collections::{BTreeMap, HashMap};
    #[derive(Debug, Clone, PartialEq, DecodeBody, EncodeBody)]
    struct NestedConfig {
        #[hcl(label)]
        name: String,
        enabled: bool,
    }
    #[derive(Debug, Clone, PartialEq, DecodeBody, EncodeBody)]
    struct ServerConfig {
        #[hcl(label)]
        env: String,
        host: String,
        port: u16,
        #[hcl(name = "custom_timeout")]
        timeout: Option<u32>,
        #[hcl(block)]
        nested: NestedConfig,
        #[hcl(block)]
        services: Vec<NestedConfig>,
    }
    #[test]
    fn test_encode_value_primitives() {
        assert_eq!(true.encode_value().data.as_ref(), &ValueData::Bool(true));
        assert_eq!(
            "hello".encode_value().data.as_ref(),
            &ValueData::String("hello".into())
        );
        assert_eq!(
            String::from("world").encode_value().data.as_ref(),
            &ValueData::String("world".into())
        );
        let signed_8: i8 = 42;
        assert_eq!(
            signed_8.encode_value().data.as_ref(),
            &ValueData::Number(42_i32.into())
        );
        let signed_16: i16 = 16;
        assert_eq!(
            signed_16.encode_value().data.as_ref(),
            &ValueData::Number(16_i32.into())
        );
        let signed_32: i32 = 32;
        assert_eq!(
            signed_32.encode_value().data.as_ref(),
            &ValueData::Number(32_i32.into())
        );
        let signed_64: i64 = 64;
        assert_eq!(
            signed_64.encode_value().data.as_ref(),
            &ValueData::Number(64_i32.into())
        );
        let signed_size: isize = 100;
        assert_eq!(
            signed_size.encode_value().data.as_ref(),
            &ValueData::Number(100_i32.into())
        );
        let signed_128: i128 = 128;
        assert_eq!(
            signed_128.encode_value().data.as_ref(),
            &ValueData::Number(128_i32.into())
        );
        let unsigned_8: u8 = 8;
        assert_eq!(
            unsigned_8.encode_value().data.as_ref(),
            &ValueData::Number(8_i32.into())
        );
        let unsigned_16: u16 = 8080;
        assert_eq!(
            unsigned_16.encode_value().data.as_ref(),
            &ValueData::Number(8080_i32.into())
        );
        let unsigned_32: u32 = 320;
        assert_eq!(
            unsigned_32.encode_value().data.as_ref(),
            &ValueData::Number(320_i32.into())
        );
        let unsigned_64: u64 = 640;
        assert_eq!(
            unsigned_64.encode_value().data.as_ref(),
            &ValueData::Number(640_i32.into())
        );
        let unsigned_size: usize = 1000;
        assert_eq!(
            unsigned_size.encode_value().data.as_ref(),
            &ValueData::Number(1000_i32.into())
        );
        let unsigned_128: u128 = 1280;
        assert_eq!(
            unsigned_128.encode_value().data.as_ref(),
            &ValueData::Number(1280_i32.into())
        );
        let num = Number::from(123_i64);
        assert_eq!(
            num.encode_value().data.as_ref(),
            &ValueData::Number(num.clone())
        );
        let val_f32: f32 = 1.25;
        assert!(matches!(
            &*val_f32.encode_value().data,
            ValueData::Number(_)
        ));
        let val_f64: f64 = 2.75;
        assert!(matches!(
            &*val_f64.encode_value().data,
            ValueData::Number(_)
        ));
        let nan_f32: f32 = f32::NAN;
        assert_eq!(
            nan_f32.encode_value().data.as_ref(),
            &ValueData::Number(Number::from(0_i64))
        );
        let nan_f64: f64 = f64::NAN;
        assert_eq!(
            nan_f64.encode_value().data.as_ref(),
            &ValueData::Number(Number::from(0_i64))
        );
        let vec_i = vec![1, 2];
        let v_vec = vec_i.encode_value();
        assert!(matches!(&*v_vec.data, ValueData::Array(_)));
        let empty_vec: Vec<i32> = Vec::new();
        assert!(matches!(
            &*empty_vec.encode_value().data,
            ValueData::Array(_)
        ));
        let opt_some: Option<i32> = Some(10);
        assert_eq!(
            opt_some.encode_value().data.as_ref(),
            &ValueData::Number(10_i32.into())
        );
        let opt_none: Option<i32> = None;
        assert!(opt_none.encode_value().is_null());
        let mut hm = HashMap::new();
        hm.insert("k".to_string(), 1);
        assert!(matches!(&*hm.encode_value().data, ValueData::Object(_)));
        let hash_empty: HashMap<String, i32> = HashMap::new();
        assert!(matches!(
            &*hash_empty.encode_value().data,
            ValueData::Object(_)
        ));
        let mut bm = BTreeMap::new();
        bm.insert("k".to_string(), 2);
        assert!(matches!(&*bm.encode_value().data, ValueData::Object(_)));
        let btree_empty: BTreeMap<String, i32> = BTreeMap::new();
        assert!(matches!(
            &*btree_empty.encode_value().data,
            ValueData::Object(_)
        ));
        let val = Value::new(Type::Bool, ValueData::Bool(false));
        assert_eq!(val.encode_value(), val);
    }
    #[test]
    fn test_encode_body_and_roundtrip() {
        let original = ServerConfig {
            env: "production".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8080,
            timeout: Some(30),
            nested: NestedConfig {
                name: "primary".to_string(),
                enabled: true,
            },
            services: vec![
                NestedConfig {
                    name: "auth".to_string(),
                    enabled: true,
                },
                NestedConfig {
                    name: "metrics".to_string(),
                    enabled: false,
                },
            ],
        };
        let mut body = CstBody::new();
        original.encode_into_body(&mut body).unwrap();
        assert!(body.attributes().iter().any(|a| a.name == "host"));
        assert!(body.attributes().iter().any(|a| a.name == "port"));
        assert!(body.attributes().iter().any(|a| a.name == "custom_timeout"));
        assert!(
            body.blocks()
                .iter()
                .any(|b| b.block_type == "nested" && b.labels == vec!["primary"])
        );
        let s_blocks: Vec<_> = body
            .blocks()
            .iter()
            .filter(|b| b.block_type == "services")
            .collect();
        assert_eq!(s_blocks.len(), 2);
        assert_eq!(s_blocks[0].labels, vec!["auth"]);
        assert_eq!(s_blocks[1].labels, vec!["metrics"]);
        let mut hcl_str = String::new();
        body.render(&mut hcl_str);
        assert!(hcl_str.contains("host = \"0.0.0.0\""));
        assert!(hcl_str.contains("custom_timeout = 30"));
        assert!(hcl_str.contains("nested \"primary\" {"));
        assert!(hcl_str.contains("services \"auth\" {"));
        let parsed_body = crate::api::parse(&hcl_str).unwrap();
        let mut ctx = crate::eval::context::Context::new();
        let decoded =
            ServerConfig::decode_body(&parsed_body, &["production".to_string()], &mut ctx).unwrap();
        assert_eq!(decoded, original);
        let no_timeout = ServerConfig {
            timeout: None,
            ..original.clone()
        };
        let mut body_no_timeout = CstBody::new();
        no_timeout.encode_into_body(&mut body_no_timeout).unwrap();
        assert!(
            !body_no_timeout
                .attributes()
                .iter()
                .any(|a| a.name == "custom_timeout")
        );
    }
    struct MaybeFailingEncode(bool);
    impl EncodeBody for MaybeFailingEncode {
        fn encode_into_body(&self, _body: &mut CstBody) -> Result<(), Diagnostics> {
            if self.0 {
                Err(Diagnostics::from(crate::diagnostic::Diagnostic::error(
                    "Encode Error",
                    "simulated failure",
                    crate::span::Span::new(0, 0, 0, 0, 0, 0),
                )))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn test_extract_labels_default() {
        let success = MaybeFailingEncode(false);
        assert_eq!(success.extract_labels(), Vec::<String>::new());
        let mut body = CstBody::new();
        assert!(success.encode_into_body(&mut body).is_ok());
        let blk = success.encode_into_block("dummy", vec![]).unwrap();
        assert_eq!(blk.block_type, "dummy");
        assert!(encode_to_string(&success).is_ok());
        let failure = MaybeFailingEncode(true);
        assert!(failure.encode_into_block("fail", vec![]).is_err());
        assert!(encode_to_string(&failure).is_err());
    }
    #[derive(Debug, PartialEq, crate::decode::DecodeValue, crate::encode::EncodeValue)]
    enum TaskPriority {
        Low,
        #[hcl(name = "high_priority")]
        High,
    }
    #[derive(Debug, PartialEq, crate::decode::DecodeValue, crate::encode::EncodeValue)]
    struct TaskMeta {
        author: String,
        retries: u32,
    }
    #[derive(Debug, PartialEq, crate::decode::DecodeValue, crate::encode::EncodeValue)]
    struct TaskItem {
        #[hcl(name = "task_id")]
        id: String,
        priority: TaskPriority,
        description: Option<String>,
        meta: TaskMeta,
    }
    #[test]
    fn test_derive_encode_value_and_roundtrip() {
        let task = TaskItem {
            id: "task-123".to_string(),
            priority: TaskPriority::High,
            description: Some("Urgent database migration".to_string()),
            meta: TaskMeta {
                author: "ops-team".to_string(),
                retries: 3,
            },
        };
        let encoded = task.encode_value();
        assert!(matches!(encoded.ty(), Type::Object { .. }));
        let decoded: TaskItem = crate::decode::DecodeValue::decode_value(
            &encoded,
            crate::span::Span::new(0, 0, 0, 0, 0, 0),
        )
        .unwrap();
        assert_eq!(decoded, task);
        let task_no_desc = TaskItem {
            id: "task-456".to_string(),
            priority: TaskPriority::Low,
            description: None,
            meta: TaskMeta {
                author: "dev-team".to_string(),
                retries: 0,
            },
        };
        let encoded_no_desc = task_no_desc.encode_value();
        let decoded_no_desc: TaskItem = crate::decode::DecodeValue::decode_value(
            &encoded_no_desc,
            crate::span::Span::new(0, 0, 0, 0, 0, 0),
        )
        .unwrap();
        assert_eq!(decoded_no_desc, task_no_desc);
        let low_val = TaskPriority::Low.encode_value();
        assert_eq!(low_val.to_string(), "\"Low\"");
        let high_val = TaskPriority::High.encode_value();
        assert_eq!(high_val.to_string(), "\"high_priority\"");
    }
    /// A leaf service definition with attributes.
    #[derive(Debug, Clone, PartialEq, DecodeBody, EncodeBody, hcl_macros::ImpliedBodySchema)]
    struct ComplexLeafService {
        port: u16,
        enabled: bool,
    }
    /// An embedded common configuration flattened into the parent block.
    #[derive(Debug, Clone, PartialEq, DecodeBody, EncodeBody, hcl_macros::ImpliedBodySchema)]
    struct ComplexCommonMeta {
        owner: String,
        region: String,
    }
    /// A middle-tier cluster configuration containing blocks, maps, and flattened structs.
    #[derive(Debug, Clone, PartialEq, DecodeBody, EncodeBody, hcl_macros::ImpliedBodySchema)]
    struct ComplexClusterConfig {
        #[hcl(flatten)]
        meta: ComplexCommonMeta,
        #[hcl(block)]
        services: BTreeMap<String, ComplexLeafService>,
    }
    /// Root complex infrastructure definition testing complete round-trip fidelity.
    #[derive(Debug, Clone, PartialEq, DecodeBody, EncodeBody, hcl_macros::ImpliedBodySchema)]
    struct ComplexInfrastructure {
        project_name: String,
        version: u32,
        #[hcl(block, name = "cluster")]
        clusters: HashMap<String, ComplexClusterConfig>,
    }
    /// Tests round-trip decode -> encode -> decode fidelity on complex nested structures.
    #[test]
    fn test_complex_nested_structure_roundtrip() {
        let mut services = BTreeMap::new();
        services.insert(
            "auth".to_string(),
            ComplexLeafService {
                port: 8080,
                enabled: true,
            },
        );
        services.insert(
            "payments".to_string(),
            ComplexLeafService {
                port: 9000,
                enabled: false,
            },
        );
        let mut clusters = HashMap::new();
        clusters.insert(
            "prod".to_string(),
            ComplexClusterConfig {
                meta: ComplexCommonMeta {
                    owner: "infra-team".to_string(),
                    region: "us-west-2".to_string(),
                },
                services,
            },
        );
        let infra = ComplexInfrastructure {
            project_name: "global-cloud".to_string(),
            version: 2,
            clusters,
        };
        let rendered = encode_to_string(&infra).unwrap();
        assert!(rendered.contains("project_name = \"global-cloud\""));
        assert!(rendered.contains("version = 2"));
        assert!(rendered.contains("cluster \"prod\" {"));
        assert!(rendered.contains("owner = \"infra-team\""));
        assert!(rendered.contains("region = \"us-west-2\""));
        assert!(rendered.contains("services \"auth\" {"));
        assert!(rendered.contains("port = 8080"));
        assert!(rendered.contains("enabled = true"));
        let mut parser = crate::parse::parser::Parser::new(&rendered);
        let body = parser.parse_body();
        let mut ctx = crate::eval::context::Context::new();
        let decoded = ComplexInfrastructure::decode_body(&body, &[], &mut ctx).unwrap();
        assert_eq!(decoded.project_name, infra.project_name);
        assert_eq!(decoded.version, infra.version);
        assert_eq!(decoded.clusters.len(), infra.clusters.len());
        let prod = decoded.clusters.get("prod").unwrap();
        assert_eq!(
            prod.meta,
            ComplexCommonMeta {
                owner: "infra-team".to_string(),
                region: "us-west-2".to_string()
            }
        );
        assert_eq!(prod.services.len(), 2);
        assert_eq!(prod.services.get("auth").unwrap().port, 8080);
        assert_eq!(prod.services.get("payments").unwrap().port, 9000);
        let second_rendered = encode_to_string(&decoded).unwrap();
        let mut second_parser = crate::parse::parser::Parser::new(&second_rendered);
        let second_body = second_parser.parse_body();
        let mut second_ctx = crate::eval::context::Context::new();
        let second_decoded =
            ComplexInfrastructure::decode_body(&second_body, &[], &mut second_ctx).unwrap();
        assert_eq!(decoded.project_name, second_decoded.project_name);
        assert_eq!(decoded.version, second_decoded.version);
        assert_eq!(decoded.clusters, second_decoded.clusters);
    }
    #[test]
    fn test_encode_body_with_block_trailing_comment() {
        let span = crate::span::Span::default();
        let mut body = crate::ast::structure::Body::new(span.clone());
        let mut block = crate::ast::structure::Block::new(
            "server",
            vec!["web".to_string()],
            crate::ast::structure::Body::new(span.clone()),
            span.clone(),
        );
        block.trailing_comment = Some("# inline comment".to_string());
        block.leading_comments = vec!["# leading comment".to_string()];
        body.blocks.push(block);
        let block2 = crate::ast::structure::Block::new(
            "worker",
            vec!["node1".to_string()],
            crate::ast::structure::Body::new(span.clone()),
            span,
        );
        body.blocks.push(block2);
        let mut cst_body = CstBody::default();
        body.encode_into_body(&mut cst_body).unwrap();
        assert_eq!(cst_body.blocks.len(), 2);
        assert!(
            cst_body.blocks[0]
                .trailing
                .tokens
                .iter()
                .any(|t| t.text.contains("# inline comment"))
        );
        assert_eq!(cst_body.blocks[1].trailing.tokens.len(), 0);
    }
}
