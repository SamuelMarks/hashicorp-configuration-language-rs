#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]
#[cfg(test)]
mod tests {
    use crate::api::{from_str, from_str_with_context, parse};
    use crate::eval::context::Context;
    use crate::eval::func::Function;
    use crate::types::ty::Type;
    use crate::types::val::{Value, ValueData};
    use hcl_macros::DecodeBody;
    #[derive(Debug, DecodeBody)]
    struct MockConfig {
        name: String,
        count: i64,
        features: Vec<String>,
        enabled: bool,
    }
    #[test]
    fn test_parse() {
        let input = r#"
            name = "my_app"
        "#;
        let body = parse(input).unwrap();
        assert_eq!(body.attributes.len(), 1);
        assert!(body.attributes.contains_key("name"));
    }
    #[test]
    fn test_from_str() {
        let input = r#"
            name = "my_app"
            count = 10
            features = ["auth"]
            enabled = true
        "#;
        let config: MockConfig = from_str(input).unwrap();
        assert!(config.name.contains("my_app"));
        assert_eq!(config.count, 10);
        assert_eq!(config.features, vec!["auth".to_string()]);
        assert!(config.enabled);
    }
    #[test]
    fn test_e2e_pipeline() {
        let input = r#"
            name = "my_app"
            count = 10
            features = ["auth", "logging"]
            enabled = true
        "#;
        let mut ctx = Context::new();
        let config: MockConfig = from_str_with_context(input, &mut ctx).unwrap();
        assert!(config.name.contains("my_app"));
        assert_eq!(config.count, 10);
        assert_eq!(
            config.features,
            vec!["auth".to_string(), "logging".to_string()]
        );
        assert!(config.enabled);
    }
    #[test]
    fn test_custom_functions() {
        let input = r#"
            name = get_app_name("test")
            count = 1
            features = []
            enabled = false
        "#;
        let mut ctx = Context::new();
        let get_app_name = Function {
            name: "get_app_name".to_string(),
            func: std::sync::Arc::new(|args: &[Value]| {
                if let ValueData::String(ref s) = *args[0].data {
                    Ok(Value::new(
                        Type::String,
                        ValueData::String(format!("{s}-app")),
                    ))
                } else {
                    panic!("Expected string argument")
                }
            }),
            signature: None,
        };
        ctx.set_function("get_app_name", get_app_name);
        let config: MockConfig = from_str_with_context(input, &mut ctx).unwrap();
        assert!(config.name.contains("test-app"));
    }
    #[test]
    fn test_unknown_values_plan_phase() {
        let input = r"
            name = unknown_var
            count = 100
            features = []
            enabled = true
        ";
        let mut ctx = Context::new();
        ctx.set_variable("unknown_var", Value::unknown(Type::String));
        let config_res: Result<MockConfig, _> = from_str_with_context(input, &mut ctx);
        let errs = config_res.err().unwrap();
        assert!(errs.has_errors());
        assert!(errs.errors()[0].error.to_string().contains("Type Mismatch"));
    }
    #[test]
    fn test_from_str_stdlib_out_of_the_box() {
        let input = r#"
            name = upper("production")
            count = abs(-50)
            features = [lower("FEATURE_A"), lower("FEATURE_B")]
            enabled = true
        "#;
        let config: MockConfig = from_str(input).unwrap();
        assert_eq!(config.name, "PRODUCTION");
        assert_eq!(config.count, 50);
        assert_eq!(config.features, vec!["feature_a", "feature_b"]);
        assert!(config.enabled);
    }
    #[test]
    fn test_from_str_without_stdlib() {
        let input_with_func = r#"
            name = upper("production")
            count = 10
            features = []
            enabled = true
        "#;
        let res: Result<MockConfig, _> = crate::api::from_str_without_stdlib(input_with_func);
        assert!(res.is_err());
        let input_plain = r#"
            name = "production"
            count = 10
            features = []
            enabled = true
        "#;
        let config: MockConfig = crate::api::from_str_without_stdlib(input_plain).unwrap();
        assert_eq!(config.name, "production");
    }
    #[test]
    fn test_evaluate_expr() {
        let val = crate::api::evaluate_expr("upper(\"hello\")", None).unwrap();
        assert_eq!(*val.data, ValueData::String("HELLO".to_string()));
        let val_num = crate::api::evaluate_expr("abs(-99)", None).unwrap();
        assert_eq!(*val_num.data, ValueData::Number(99.into()));
        let mut ctx = Context::new();
        ctx.set_variable(
            "my_var",
            Value::new(Type::String, ValueData::String("world".into())),
        );
        let val_custom = crate::api::evaluate_expr("my_var", Some(&ctx)).unwrap();
        assert_eq!(*val_custom.data, ValueData::String("world".into()));
        let err_empty = crate::api::evaluate_expr("", None);
        assert!(err_empty.is_err());
        let err_syntax = crate::api::evaluate_expr("+++", None);
        assert!(err_syntax.is_err());
    }
    #[test]
    fn test_evaluate_helper() {
        let input = r#"
            name = "my_app"
            dynamic "item" {
                for_each = ["a", "b"]
                content {
                    val = item.value
                }
            }
        "#;
        let body = crate::api::evaluate(input, None).unwrap();
        assert_eq!(body.blocks.len(), 2);
        assert_eq!(body.blocks[0].block_type, "item");
        assert_eq!(body.blocks[1].block_type, "item");
        let mut ctx = Context::with_stdlib();
        let body2 = crate::api::evaluate(input, Some(&mut ctx)).unwrap();
        assert_eq!(body2.blocks.len(), 2);
        let err = crate::api::evaluate("invalid syntax {{{", None);
        assert!(err.is_err());
        let dyn_err_input = r#"
            dynamic "item" {
                for_each = var.non_existent
                content {}
            }
        "#;
        let err_dyn = crate::api::evaluate(dyn_err_input, None);
        assert!(err_dyn.is_err());
    }
    #[test]
    fn test_api_error_branches() {
        let bad_syntax = "invalid syntax {{{";
        let bad_dyn = r#"
            name = "a"
            count = 1
            features = []
            enabled = true
            dynamic "d" {
                for_each = var.missing
                content {}
            }
        "#;
        assert!(from_str::<MockConfig>(bad_syntax).is_err());
        assert!(from_str::<MockConfig>(bad_dyn).is_err());
        assert!(crate::api::from_str_without_stdlib::<MockConfig>(bad_syntax).is_err());
        assert!(crate::api::from_str_without_stdlib::<MockConfig>(bad_dyn).is_err());
        let mut ctx = Context::new();
        assert!(from_str_with_context::<MockConfig>(bad_syntax, &mut ctx).is_err());
        assert!(from_str_with_context::<MockConfig>(bad_dyn, &mut ctx).is_err());
        assert!(crate::api::evaluate_expr("1 / 0", None).is_err());
    }
    #[test]
    fn test_from_file_and_evaluate_file() {
        use crate::api::{evaluate_file, from_file, from_file_with_context};
        use std::io::Write;
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_api_config.hcl");
        let content = r#"
            name = "file_app"
            count = 42
            features = ["network", "storage"]
            enabled = true
        "#;
        let mut f = std::fs::File::create(&file_path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let config: MockConfig = from_file(&file_path).unwrap();
        assert_eq!(config.name, "file_app");
        assert_eq!(config.count, 42);
        assert_eq!(
            config.features,
            vec!["network".to_string(), "storage".to_string()]
        );
        assert!(config.enabled);
        let mut ctx = Context::with_stdlib();
        let config2: MockConfig = from_file_with_context(&file_path, &mut ctx).unwrap();
        assert_eq!(config2.count, 42);
        let body = evaluate_file(&file_path, None).unwrap();
        assert_eq!(body.attributes.len(), 4);
        let mut eval_ctx = Context::with_stdlib();
        let body_with_ctx = evaluate_file(&file_path, Some(&mut eval_ctx)).unwrap();
        assert_eq!(body_with_ctx.attributes.len(), 4);
        let bad_path = dir.join("nonexistent_hcl_file_999.hcl");
        assert!(from_file::<MockConfig>(&bad_path).is_err());
        assert!(from_file_with_context::<MockConfig>(&bad_path, &mut ctx).is_err());
        assert!(evaluate_file(&bad_path, None).is_err());
        let bad_dyn_file = dir.join("test_api_bad_dyn.hcl");
        let bad_dyn_content = r#"
            dynamic "sub" {
                for_each = "not-a-collection"
                content {}
            }
        "#;
        let mut f_dyn = std::fs::File::create(&bad_dyn_file).unwrap();
        f_dyn.write_all(bad_dyn_content.as_bytes()).unwrap();
        assert!(from_file_with_context::<MockConfig>(&bad_dyn_file, &mut ctx).is_err());
        let _ = std::fs::remove_file(&bad_dyn_file);
        let _ = std::fs::remove_file(&file_path);
    }
}
