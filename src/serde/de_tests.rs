#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]
#[cfg(test)]
mod tests {
    use super::super::de::from_str;
    use super::super::ser::to_string;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct TestConfig {
        name: String,
        count: i32,
    }
    #[test]
    fn test_deserialize_basic() {
        let hcl = "name = \"test\" \n count = 42";
        let config: TestConfig = from_str(hcl).unwrap();
        assert!(config.name.contains("test"));
        assert_eq!(config.count, 42);
    }
    #[test]
    fn test_deserialize_error() {
        let res: Result<TestConfig, _> = from_str("name");
        assert!(res.is_err());
        assert!(res.err().unwrap().contains("Parse error"));
        let hcl = "count = \"not an integer\"";
        let res: Result<TestConfig, _> = from_str(hcl);
        assert!(res.is_err());
        assert!(res.err().unwrap().contains("Deserialize error"));
    }
    #[test]
    fn test_deserialize_mixed_blocks() {
        let input = r#"
        my_block {}
        my_block "label" {}
        my_other "label" {}
        my_other {}
        "#;
        let res: serde_json::Value = crate::serde::de::from_str(input).unwrap();
        let map = res.as_object().unwrap();
        assert!(map.contains_key("my_block"));
        assert!(map.contains_key("my_other"));
    }
    #[test]
    fn test_expr_to_json_fallbacks() {
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
        let p = TemplatePart::Interpolation(Expression::Bool(true, span.clone()), span.clone());
        let e = Expression::Template(vec![p], span.clone());
        let json = crate::serde::de::expr_to_json(&e);
        assert_eq!(json.as_str().unwrap(), "${...}");
        let n = std::str::FromStr::from_str("1e400").unwrap();
        let e = Expression::Number(n, span);
        let json = crate::serde::de::expr_to_json(&e);
        assert!(json.is_null());
    }
    #[test]
    fn test_deserialize_complex() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct Complex {
            float_val: f64,
            flag: bool,
            arr: Vec<String>,
            obj: std::collections::BTreeMap<String, i32>,
            tmpl: String,
            null_val: Option<String>,
            blocks: Vec<Block>,
            named_block: std::collections::BTreeMap<String, Block>,
        }
        #[derive(Debug, PartialEq, Deserialize)]
        struct Block {
            inner: bool,
        }
        let hcl = r#"
            float_val = 42.42
            flag = true
            arr = ["a", "b"]
            obj = {
                key1 = 1
                "key2" = 2
                (1+1) = 3
            }
            tmpl = "hello ${world}"
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
        "#;
        let config: Complex = from_str(hcl).unwrap();
        assert_eq!(config.float_val, 42.42);
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
    }
    #[test]
    fn test_deserialize_nan() {
        let hcl = "val = 1e9999999999999999999999999999999999999";
        let res: Result<TestConfig, _> = from_str(hcl);
        assert!(res.is_err());
        use super::super::de::from_str;
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct Dummy {
            num: Option<f64>,
        }
        let hcl = "num = 1e4000";
        let res: Result<Dummy, _> = from_str(hcl);
        if let Ok(dummy) = res {
            assert_eq!(dummy.num, None);
        }
    }
    #[test]
    fn test_serialize_complex() {
        use std::collections::BTreeMap;
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
            string_val: "str\"ing".to_string(),
        };
        let hcl = to_string(&data).unwrap();
        assert!(hcl.contains("null_val = null"));
        assert!(hcl.contains("bool_true = true"));
        assert!(hcl.contains("bool_false = false"));
        assert!(hcl.contains("empty_arr = []"));
        assert!(hcl.contains("[\n    1,\n    2\n  ]"));
        assert!(hcl.contains("empty_obj = {}"));
        assert!(hcl.contains("a = 1"));
        assert!(hcl.contains("\"str\\\"ing\""));
    }
    #[test]
    fn test_serialize_error() {
        use std::collections::BTreeMap;
        let mut bad = BTreeMap::new();
        bad.insert(vec![1, 2], "value");
        let res = to_string(&bad);
        assert!(res.is_err());
        assert!(res.err().unwrap().contains("Serialize error"));
    }
}
