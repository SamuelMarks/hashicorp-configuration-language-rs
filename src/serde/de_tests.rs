//! Unit tests for Serde HCL serialization and deserialization.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[cfg(test)]
mod tests {
    use super::super::de::from_str;
    use super::super::ser::to_string;
    use crate::error::HclError;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct TestConfig {
        name: String,
        count: i32,
    }

    #[test]
    fn test_deserialize_basic() -> Result<(), HclError> {
        let hcl = r#"name = "test" 
 count = 42"#;
        let config: TestConfig = from_str(hcl)?;
        assert_eq!(config.name, "test");
        assert_eq!(config.count, 42);
        Ok(())
    }

    #[test]
    fn test_deserialize_error() {
        let res: Result<TestConfig, _> = from_str("name");
        assert!(matches!(res, Err(HclError::Parse(_))));
        let hcl = r#"count = "not an integer""#;
        let res: Result<TestConfig, _> = from_str(hcl);
        assert!(matches!(res, Err(HclError::SerdeDeserialize(_))));
    }

    #[test]
    fn test_deserialize_mixed_and_multi_label_blocks() -> Result<(), HclError> {
        let input = r#"
        my_block {}
        my_block "label" {}
        my_other "label" {}
        my_other {}
        resource "aws_instance" "web" {
            ami = "ami-123"
        }
        resource "aws_instance" "db" {
            ami = "ami-456"
        }
        "#;
        let res: serde_json::Value = crate::serde::de::from_str(input)?;
        let map = res
            .as_object()
            .ok_or_else(|| HclError::Eval("expected object".to_string()))?;
        assert!(map.contains_key("my_block"));
        assert!(map.contains_key("my_other"));
        assert_eq!(res["resource"]["aws_instance"]["web"]["ami"], "ami-123");
        assert_eq!(res["resource"]["aws_instance"]["db"]["ami"], "ami-456");
        Ok(())
    }

    #[test]
    fn test_expr_to_json_fallbacks_and_u64_precision() -> Result<(), HclError> {
        use crate::ast::expr::{Expression, TemplatePart};
        use crate::span::Span;
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let e = Expression::Variable("foo".to_string(), span.clone());
        let pairs = vec![(e, Expression::Bool(true, span.clone()))];
        let obj = Expression::Object(pairs, span.clone());
        let json = crate::serde::de::expr_to_json(&obj);
        assert_eq!(json["foo"], true);
        let other = Expression::Null(span.clone());
        let pairs = vec![(other, Expression::Bool(true, span.clone()))];
        let obj = Expression::Object(pairs, span.clone());
        let json = crate::serde::de::expr_to_json(&obj);
        assert_eq!(json["unknown_key"], true);

        // Template with known expression evaluates cleanly
        let p_known =
            TemplatePart::Interpolation(Expression::Bool(true, span.clone()), span.clone());
        let e_known = Expression::Template(vec![p_known], span.clone());
        let json_known = crate::serde::de::expr_to_json(&e_known);
        assert_eq!(json_known.as_str(), Some("true"));

        // Template with unresolvable variable falls back to "${...}"
        let p_lit = TemplatePart::Literal("prefix_".to_string(), span.clone());
        let p_unk = TemplatePart::Interpolation(
            Expression::Variable("unresolved_var".to_string(), span.clone()),
            span.clone(),
        );
        let e_unk = Expression::Template(vec![p_lit, p_unk], span.clone());
        let json_unk = crate::serde::de::expr_to_json(&e_unk);
        assert_eq!(json_unk.as_str(), Some("prefix_${...}"));

        // Large u64 integer preserves exact precision without f64 truncation
        let u64_max_num: crate::number::Number = "18446744073709551615"
            .parse()
            .map_err(|_| HclError::Parse("parse u64".to_string()))?;
        let e_u64 = Expression::Number(u64_max_num, span.clone());
        let json_u64 = crate::serde::de::expr_to_json(&e_u64);
        assert_eq!(json_u64.as_u64(), Some(u64::MAX));

        // Huge integer outside u64 range falls back to f64 or Null if overflowed
        let n: crate::number::Number = "1e400"
            .parse()
            .map_err(|_| HclError::Parse("parse 1e400".to_string()))?;
        let e = Expression::Number(n, span);
        let json = crate::serde::de::expr_to_json(&e);
        assert!(json.is_null());

        // Evaluated binary op expression
        let eval_res: serde_json::Value = from_str(
            "sum = 10 + 32
undef = unknown_var + 1
",
        )?;
        assert_eq!(eval_res["sum"], 42);
        assert!(eval_res["undef"].is_null());
        Ok(())
    }

    #[test]
    fn test_deserialize_complex() -> Result<(), HclError> {
        #[derive(Debug, PartialEq, Deserialize)]
        struct Complex {
            float_val: f64,
            flag: bool,
            arr: Vec<String>,
            obj: BTreeMap<String, i32>,
            tmpl: String,
            null_val: Option<String>,
            blocks: Vec<Block>,
            named_block: BTreeMap<String, Block>,
        }
        #[derive(Debug, PartialEq, Deserialize)]
        struct Block {
            inner: bool,
        }
        let hcl = [
            r#"
            float_val = 42.42
            flag = true
            arr = ["a", "b"]
            obj = {
                key1 = 1
                "key2" = 2
                (1+1) = 3
            }
            tmpl = "hello "#,
            "${",
            r#"world}"
            null_val = null
            
            blocks {
                inner = true
            }
            blocks {
                inner = false
            }

            named_block "label1" {
                inner = true
            }
        "#,
        ]
        .concat();
        let config: Complex = from_str(&hcl)?;
        assert!((config.float_val - 42.42).abs() < f64::EPSILON);
        assert!(config.flag);
        assert_eq!(config.arr, vec!["a", "b"]);
        assert_eq!(config.obj.get("key1"), Some(&1));
        assert_eq!(config.obj.get("key2"), Some(&2));
        assert_eq!(config.obj.get("unknown_key"), Some(&3));
        assert!(config.tmpl.contains("hello ${...}"));
        assert_eq!(config.null_val, None);
        assert_eq!(config.blocks.len(), 2);
        assert!(config.blocks[0].inner);
        assert!(!config.blocks[1].inner);
        assert!(config.named_block["label1"].inner);
        Ok(())
    }

    #[test]
    fn test_deserialize_nan() -> Result<(), HclError> {
        #[derive(Deserialize)]
        struct Dummy {
            num: Option<f64>,
        }
        let hcl = "val = 1e9999999999999999999999999999999999999";
        let res: Result<TestConfig, _> = from_str(hcl);
        assert!(res.is_err());

        let hcl = "num = 1e4000";
        let dummy: Dummy = from_str(hcl)?;
        assert_eq!(dummy.num, None);
        Ok(())
    }

    #[test]
    fn test_serialize_complex() -> Result<(), HclError> {
        #[derive(Serialize)]
        struct ComplexSer {
            null_val: Option<String>,
            bool_true: bool,
            bool_false: bool,
            empty_arr: Vec<i32>,
            arr: Vec<i32>,
            empty_obj: BTreeMap<String, i32>,
            obj: BTreeMap<String, i32>,
            string_val: String,
        }
        let mut obj = BTreeMap::new();
        obj.insert("a".to_string(), 1);
        let data = ComplexSer {
            null_val: None,
            bool_true: true,
            bool_false: false,
            empty_arr: vec![],
            arr: vec![1, 2],
            empty_obj: BTreeMap::new(),
            obj,
            string_val: r#"str"ing"#.to_string(),
        };
        let hcl = to_string(&data)?;
        assert!(hcl.contains("null_val = null"));
        assert!(hcl.contains("bool_true = true"));
        assert!(hcl.contains("bool_false = false"));
        assert!(hcl.contains("empty_arr = []"));
        assert!(hcl.contains("[\n    1,\n    2\n  ]"));
        assert!(hcl.contains("empty_obj = {}"));
        assert!(hcl.contains("a = 1"));
        assert!(hcl.contains("str") && hcl.contains("ing"));
        Ok(())
    }

    #[test]
    fn test_serialize_error() {
        let mut bad = BTreeMap::new();
        bad.insert(vec![1, 2], "value");
        let res = to_string(&bad);
        assert!(matches!(res, Err(HclError::SerdeSerialize(_))));

        let hcl_two_labeled = r#"
            resource "aws_instance" "web1" {
                x = 1
            }
            resource "aws_instance" "web2" {
                x = 2
            }
        "#;
        let val: serde_json::Value = from_str(hcl_two_labeled).unwrap_or_default();
        assert_eq!(val["resource"]["aws_instance"]["web1"]["x"], 1);
        assert_eq!(val["resource"]["aws_instance"]["web2"]["x"], 2);
    }
}
