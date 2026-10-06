#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]
use crate::ast::expr::Expression;
use crate::ast::type_expr::TypeExpr;
use crate::ast::user_func::{FunctionBlock, FunctionParam};
use crate::eval::context::Context;
use crate::number::Number;
use crate::span::Span;
use crate::types::{Type, Value, ValueData};
use std::str::FromStr;
#[test]
fn test_register_and_call_user_func() {
    let span = Span::new(0, 0, 0, 0, 0, 0);
    let mut ctx = Context::new();
    let params = vec![FunctionParam {
        name: "x".to_string(),
        type_expr: Some(TypeExpr::Primitive(Type::Number, span.clone())),
        span: span.clone(),
    }];
    let body = Expression::Variable("x".to_string(), span.clone());
    let fb = FunctionBlock::new(
        "add_one".to_string(),
        params,
        Some(TypeExpr::Primitive(Type::Number, span.clone())),
        body,
        span,
    );
    ctx.register_function_block(&fb);
    let f = ctx.get_function("add_one").unwrap();
    let res = (f.func)(&[Value::new(
        Type::Number,
        ValueData::Number(Number::from_str("5").unwrap()),
    )])
    .unwrap();
    assert_eq!(*res.ty(), Type::Number);
    if let ValueData::Number(n) = &*res.data {
        assert_eq!(n.0.to_string(), "5");
    } else {
        panic!("expected number");
    }
}
#[test]
fn test_register_and_call_user_func_type_mismatch() {
    let span = Span::new(0, 0, 0, 0, 0, 0);
    let mut ctx = Context::new();
    let params = vec![FunctionParam {
        name: "x".to_string(),
        type_expr: Some(TypeExpr::Primitive(Type::Number, span.clone())),
        span: span.clone(),
    }];
    let body = Expression::Number(Number::from_str("1").unwrap(), span.clone());
    let fb = FunctionBlock::new("test".to_string(), params, None, body, span);
    ctx.register_function_block(&fb);
    let f = ctx.get_function("test").unwrap();
    let err = (f.func)(&[Value::new(
        Type::String,
        ValueData::String("hello".to_string()),
    )])
    .err()
    .unwrap();
    assert!(err.contains("type mismatch"));
}
#[test]
fn test_user_func_variadic_0_1_n_args() {
    let span = Span::new(0, 0, 0, 0, 0, 0);
    let mut ctx = Context::new();
    let params = vec![FunctionParam {
        name: "prefix".to_string(),
        type_expr: Some(TypeExpr::Primitive(Type::String, span.clone())),
        span: span.clone(),
    }];
    let variadic = FunctionParam {
        name: "rest".to_string(),
        type_expr: None,
        span: span.clone(),
    };
    let parsed_body = crate::api::parse("v = join(\"-\", concat([prefix], rest))").unwrap();
    let body = parsed_body.attributes["v"].expr.clone();
    let fb = FunctionBlock::new("custom_join".to_string(), params, None, body, span)
        .with_variadic_param(Some(variadic));
    ctx.register_function_block(&fb);
    let f = ctx.get_function("custom_join").unwrap();
    let res0 = (f.func)(&[Value::new(Type::String, ValueData::String("base".into()))]).unwrap();
    assert_eq!(res0.data.as_ref(), &ValueData::String("base".into()));
    let res1 = (f.func)(&[
        Value::new(Type::String, ValueData::String("base".into())),
        Value::new(Type::String, ValueData::String("one".into())),
    ])
    .unwrap();
    assert_eq!(res1.data.as_ref(), &ValueData::String("base-one".into()));
    let res3 = (f.func)(&[
        Value::new(Type::String, ValueData::String("base".into())),
        Value::new(Type::String, ValueData::String("one".into())),
        Value::new(Type::String, ValueData::String("two".into())),
        Value::new(Type::String, ValueData::String("three".into())),
    ])
    .unwrap();
    assert_eq!(
        res3.data.as_ref(),
        &ValueData::String("base-one-two-three".into())
    );
    assert!((f.func)(&[]).is_err());
}
#[test]
fn test_parse_and_execute_user_func_with_variadic() {
    let hcl_attr_syntax = r#"
        function "collect" {
            params = [first]
            variadic_param = more
            result = length(more)
        }
    "#;
    let body1 = crate::api::parse(hcl_attr_syntax).unwrap();
    assert_eq!(body1.functions.len(), 1);
    let func_block = &body1.functions[0];
    assert_eq!(func_block.name, "collect");
    assert_eq!(func_block.params.len(), 1);
    assert_eq!(
        func_block.variadic_param.as_ref().map(|v| v.name.as_str()),
        Some("more")
    );
    let mut ctx = Context::new();
    ctx.register_function_block(func_block);
    let f1 = ctx.get_function("collect").unwrap();
    let r0 = (f1.func)(&[Value::new(Type::String, ValueData::String("a".into()))]).unwrap();
    assert_eq!(r0.data.as_ref(), &ValueData::Number(0_i32.into()));
    let r2 = (f1.func)(&[
        Value::new(Type::String, ValueData::String("a".into())),
        Value::new(Type::String, ValueData::String("b".into())),
        Value::new(Type::String, ValueData::String("c".into())),
    ])
    .unwrap();
    assert_eq!(r2.data.as_ref(), &ValueData::Number(2_i32.into()));
    let hcl_block_syntax = r#"
        function "collect_block" {
            params = [first]
            variadic_param "rest" {
                type = "number"
            }
            result = sum(rest)
        }
    "#;
    let body2 = crate::api::parse(hcl_block_syntax).unwrap();
    assert_eq!(body2.functions.len(), 1);
    let fb2 = &body2.functions[0];
    assert_eq!(
        fb2.variadic_param.as_ref().map(|v| v.name.as_str()),
        Some("rest")
    );
    let mut ctx2 = Context::new();
    ctx2.register_function_block(fb2);
    let f2 = ctx2.get_function("collect_block").unwrap();
    let sum_res = (f2.func)(&[
        Value::new(Type::String, ValueData::String("unused".into())),
        Value::new(Type::Number, ValueData::Number(10_i32.into())),
        Value::new(Type::Number, ValueData::Number(20_i32.into())),
        Value::new(Type::Number, ValueData::Number(30_i32.into())),
    ])
    .unwrap();
    assert_eq!(sum_res.data.as_ref(), &ValueData::Number(60_i32.into()));
}
#[test]
fn test_misspelled_function_name_suggestions() {
    use crate::eval::evaluator::Evaluator;
    let mut ctx = Context::with_stdlib();
    let expr_uppr = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "uppr".into(),
            args: vec![Expression::String("test".to_string(), Span::default())],
            expand_final: false,
        }),
        Span::default(),
    );
    let err_uppr = Evaluator::new(&ctx).evaluate(&expr_uppr).err().unwrap();
    let diag_uppr = &err_uppr.errors()[0];
    assert_eq!(
        diag_uppr.detail.as_deref(),
        Some("Unknown function 'uppr'. Did you mean 'upper'?")
    );
    ctx.set_function(
        "my_transform",
        crate::eval::func::Function::new(
            "my_transform",
            std::sync::Arc::new(|args| Ok(args[0].clone())),
        ),
    );
    let expr_custom = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "my_transfrm".into(),
            args: vec![Expression::String("foo".to_string(), Span::default())],
            expand_final: false,
        }),
        Span::default(),
    );
    let err_custom = Evaluator::new(&ctx).evaluate(&expr_custom).err().unwrap();
    let diag_custom = &err_custom.errors()[0];
    assert_eq!(
        diag_custom.detail.as_deref(),
        Some("Unknown function 'my_transfrm'. Did you mean 'my_transform'?")
    );
    let expr_nomatch = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "completely_unknown_function_name".into(),
            args: vec![],
            expand_final: false,
        }),
        Span::default(),
    );
    let err_nomatch = Evaluator::new(&ctx).evaluate(&expr_nomatch).err().unwrap();
    let diag_nomatch = &err_nomatch.errors()[0];
    assert_eq!(diag_nomatch.detail, None);
}
