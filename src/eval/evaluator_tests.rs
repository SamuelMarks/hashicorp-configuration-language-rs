//! Unit tests for the HCL expression and template evaluator.
pub fn dummy_hash(_: &dyn std::any::Any) -> u64 {
    0
}
pub fn dummy_eq(_: &dyn std::any::Any, _: &dyn std::any::Any) -> bool {
    false
}

#[test]
fn test_dummy_helpers() {
    assert_eq!(dummy_hash(&()), 0);
    assert!(!dummy_eq(&(), &()));
}

#[test]
fn test_ephemeral_propagation() {
    let mut ctx = crate::eval::context::Context::new();
    ctx.set_variable(
        "var1",
        crate::types::val::Value::new(
            crate::types::ty::Type::Number,
            crate::types::val::ValueData::Number(crate::number::Number::from(10i64)),
        ),
    );
    ctx.set_variable(
        "var2",
        crate::types::val::Value::new(
            crate::types::ty::Type::Number,
            crate::types::val::ValueData::Number(crate::number::Number::from(20i64)),
        )
        .mark_ephemeral(),
    );

    let mut evaluator = Evaluator::new(&ctx);

    let span = crate::span::Span::new(0, 0, 1, 1, 1, 1);
    let expr = crate::ast::expr::Expression::BinaryOp(
        crate::ast::expr::BinaryOp::Add,
        Box::new(crate::ast::expr::Expression::Variable(
            "var1".to_string(),
            span.clone(),
        )),
        Box::new(crate::ast::expr::Expression::Variable(
            "var2".to_string(),
            span.clone(),
        )),
        span,
    );

    let val = evaluator.eval_expr(&expr);
    assert!(val.is_ephemeral());
}

#[test]
fn test_eval_binary_logical_right_errors() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::Number(Number::from_str("1")?, empty_span())),
        empty_span(),
    );
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("must be a boolean")
    );
    let evaluator = Evaluator::new(&ctx);
    let expr2 = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::Number(Number::from_str("1")?, empty_span())),
        empty_span(),
    );
    let errs = evaluator
        .evaluate(&expr2)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("must be a boolean")
    );
    Ok(())
}
#[test]
fn test_eval_binary_math_unknown_rhs_all_ops() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Dynamic));
    let ops = vec![
        BinaryOp::NotEq,
        BinaryOp::Less,
        BinaryOp::LessEq,
        BinaryOp::Greater,
        BinaryOp::GreaterEq,
        BinaryOp::Sub,
        BinaryOp::Mul,
        BinaryOp::Div,
        BinaryOp::Mod,
    ];
    for op in ops {
        let expr = Expression::BinaryOp(
            op,
            Box::new(Expression::Number(Number::from_str("1")?, empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let val = Evaluator::new(&ctx).evaluate(&expr)?.0;
        assert!(val.is_unknown());
    }
    Ok(())
}
#[test]
fn test_eval_binary_logical_right_unknown() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Dynamic));
    let expr = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let val = Evaluator::new(&ctx).evaluate(&expr)?.0;
    assert_eq!(val.ty(), &Type::Bool);
    assert!(val.is_unknown());
    let expr2 = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let val2 = Evaluator::new(&ctx).evaluate(&expr2)?.0;
    assert_eq!(val2.ty(), &Type::Bool);
    assert!(val2.is_unknown());
    Ok(())
}
#[test]
fn test_eval_binary_math_unknown_rhs2() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Dynamic));
    let expr3 = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Number(Number::from_str("1")?, empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let val3 = Evaluator::new(&ctx).evaluate(&expr3)?.0;
    assert_eq!(val3.ty(), &Type::Number);
    assert!(val3.is_unknown());
    let expr4 = Expression::BinaryOp(
        BinaryOp::Eq,
        Box::new(Expression::Number(Number::from_str("1")?, empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let val4 = Evaluator::new(&ctx).evaluate(&expr4)?.0;
    assert_eq!(val4.ty(), &Type::Bool);
    assert!(val4.is_unknown());
    Ok(())
}
use super::*;
use crate::number::Number;
use crate::span::Span;
use bigdecimal::BigDecimal;
use std::collections::BTreeSet;
use std::str::FromStr;
const fn empty_span() -> Span {
    Span::new(0, 0, 0, 0, 0, 0)
}
#[test]
fn test_eval_literal() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::Null(empty_span());
    let (val, _) = evaluator.evaluate(&expr)?;
    assert!(val.is_null());
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::Bool(true, empty_span());
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));
    let evaluator = Evaluator::new(&ctx);
    let n = Number::from_str("42")?;
    let expr = Expression::Number(n.clone(), empty_span());
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Number, ValueData::Number(n)));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::String("hello".to_string(), empty_span());
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(
        val,
        Value::new(Type::String, ValueData::String("hello".to_string()))
    );
    Ok(())
}
#[test]
fn test_eval_variable() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("foo", Value::new(Type::Bool, ValueData::Bool(true)));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::Variable("foo".to_string(), empty_span());
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));
    ctx.set_variable(
        "local.bar",
        Value::new(
            Type::Number,
            ValueData::Number(crate::number::Number::from(42)),
        ),
    );
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::Variable("local.bar".to_string(), empty_span());
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(
        val,
        Value::new(
            Type::Number,
            ValueData::Number(crate::number::Number::from(42))
        )
    );
    let _evaluator = Evaluator::new(&ctx);
    let expr = Expression::Variable("bar".to_string(), empty_span());
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert_eq!(errs.errors().len(), 1);
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Unknown variable 'bar'")
    );
    Ok(())
}
#[test]
fn test_eval_tuple() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::Tuple(
        vec![
            Expression::Bool(true, empty_span()),
            Expression::String("x".to_string(), empty_span()),
        ],
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val.ty(), &Type::Tuple(vec![Type::Bool, Type::String]));
    Ok(())
}
#[test]
fn test_eval_object() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let k1 = Expression::String("k1".to_string(), empty_span());
    let v1 = Expression::Bool(true, empty_span());
    let expr = Expression::Object(vec![(k1, v1)], empty_span());
    let (val, _) = evaluator.evaluate(&expr)?;
    let mut expected_map = BTreeMap::new();
    expected_map.insert("k1".to_string(), Type::Bool);
    assert_eq!(val.ty(), &Type::object(expected_map));
    Ok(())
}
#[test]
fn test_eval_unary() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::UnaryOp(
        UnaryOp::Not,
        Box::new(Expression::Bool(true, empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(false)));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Number(Number::from_str("10")?, empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(
        val,
        Value::new(Type::Number, ValueData::Number(Number::from_str("-10")?))
    );
    Ok(())
}
#[test]
fn test_eval_binary() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::Eq,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::Bool(false, empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(false)));
    let evaluator = Evaluator::new(&ctx);
    let expr2 = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::Variable("missing".to_string(), empty_span())),
        empty_span(),
    );
    let (val2, _) = evaluator.evaluate(&expr2)?;
    assert_eq!(val2, Value::new(Type::Bool, ValueData::Bool(false)));
    Ok(())
}
#[test]
fn test_eval_conditional() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let cond = Conditional {
        cond_expr: Expression::Bool(true, empty_span()),
        true_expr: Expression::Number(Number::from_str("1")?, empty_span()),
        false_expr: Expression::Number(Number::from_str("2")?, empty_span()),
    };
    let expr = Expression::Conditional(Box::new(cond), empty_span());
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(
        val,
        Value::new(Type::Number, ValueData::Number(Number::from_str("1")?))
    );
    Ok(())
}
#[test]
fn test_eval_binary_math_logic_unknown() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::String));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::Eq,
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val.ty(), &Type::Bool);
    let evaluator = Evaluator::new(&ctx);
    let expr2 = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (val2, _) = evaluator.evaluate(&expr2)?;
    assert_eq!(val2.ty(), &Type::Number);
    assert!(val2.is_unknown());
    Ok(())
}
#[test]
fn test_eval_binary_unsupported() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let expr = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::Bool(false, empty_span())),
        empty_span(),
    );
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert_eq!(errs.errors().len(), 1);
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Cannot coerce operands of Add to numbers")
    );
    Ok(())
}
#[test]
fn test_eval_logical_errors() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::Number(Number::from_str("1")?, empty_span())),
        empty_span(),
    );
    let _errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    let evaluator = Evaluator::new(&ctx);
    let expr2 = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::Number(Number::from_str("1")?, empty_span())),
        empty_span(),
    );
    let _errs = evaluator
        .evaluate(&expr2)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    Ok(())
}
#[test]
fn test_eval_logical_unknown() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Bool));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        Box::new(Expression::Bool(true, empty_span())),
        empty_span(),
    );
    let (_val, _) = evaluator.evaluate(&expr)?;
    let evaluator = Evaluator::new(&ctx);
    let expr2 = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (_val, _) = evaluator.evaluate(&expr2)?;
    let evaluator = Evaluator::new(&ctx);
    let expr3 = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        Box::new(Expression::Bool(false, empty_span())),
        empty_span(),
    );
    let (_val, _) = evaluator.evaluate(&expr3)?;
    let evaluator = Evaluator::new(&ctx);
    let expr4 = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (_val, _) = evaluator.evaluate(&expr4)?;
    Ok(())
}
#[test]
fn test_eval_unary_errors_and_unknown() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Bool));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::UnaryOp(
        UnaryOp::Not,
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val.ty(), &Type::Bool);
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val.ty(), &Type::Number);
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::UnaryOp(
        UnaryOp::Not,
        Box::new(Expression::Number(Number::from_str("1")?, empty_span())),
        empty_span(),
    );
    let _errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    let _evaluator = Evaluator::new(&ctx);
    let expr = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Bool(true, empty_span())),
        empty_span(),
    );
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("must be a number")
    );
    Ok(())
}
#[test]
fn test_eval_cond_unknown_and_errors() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Bool));
    let evaluator = Evaluator::new(&ctx);
    let cond = Conditional {
        cond_expr: Expression::Variable("unk".to_string(), empty_span()),
        true_expr: Expression::Number(Number::from_str("1")?, empty_span()),
        false_expr: Expression::Number(Number::from_str("2")?, empty_span()),
    };
    let expr = Expression::Conditional(Box::new(cond), empty_span());
    let (_val, _) = evaluator.evaluate(&expr)?;
    let _evaluator = Evaluator::new(&ctx);
    let cond2 = Conditional {
        cond_expr: Expression::Variable("unk".to_string(), empty_span()),
        true_expr: Expression::Number(Number::from_str("1")?, empty_span()),
        false_expr: Expression::Bool(false, empty_span()),
    };
    let expr2 = Expression::Conditional(Box::new(cond2), empty_span());
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr2)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Incompatible types")
    );
    let _evaluator = Evaluator::new(&ctx);
    let cond3 = Conditional {
        cond_expr: Expression::Number(Number::from_str("1")?, empty_span()),
        true_expr: Expression::Number(Number::from_str("1")?, empty_span()),
        false_expr: Expression::Number(Number::from_str("2")?, empty_span()),
    };
    let expr3 = Expression::Conditional(Box::new(cond3), empty_span());
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr3)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("must be a boolean")
    );
    Ok(())
}
#[test]
fn test_eval_stubs() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let exprs = vec![Expression::Template(vec![], empty_span())];
    for expr in exprs {
        let evaluator = Evaluator::new(&ctx);
        let val = evaluator.evaluate(&expr)?.0;
        assert_eq!(
            val,
            Value::new(Type::String, ValueData::String(String::new()))
        );
    }
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "foo".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        ))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Unknown function")
    );
    Ok(())
}
#[test]
fn test_eval_object_errors() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::String));
    let _evaluator = Evaluator::new(&ctx);
    let expr = Expression::Object(
        vec![(
            Expression::Variable("unk".to_string(), empty_span()),
            Expression::Bool(true, empty_span()),
        )],
        empty_span(),
    );
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Object key cannot be unknown")
    );
    let _evaluator = Evaluator::new(&ctx);
    let expr2 = Expression::Object(
        vec![(
            Expression::Number(Number::from_str("1")?, empty_span()),
            Expression::Bool(true, empty_span()),
        )],
        empty_span(),
    );
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr2)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Object key must be a string")
    );
    Ok(())
}
#[test]
fn test_eval_binary_math_logic_ops() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let cases = vec![
        (BinaryOp::Less, "1", "2", true),
        (BinaryOp::LessEq, "2", "2", true),
        (BinaryOp::Greater, "3", "2", true),
        (BinaryOp::GreaterEq, "2", "2", true),
    ];
    for (op, lhs, rhs, expected) in cases {
        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::BinaryOp(
            op,
            Box::new(Expression::Number(Number::from_str(lhs)?, empty_span())),
            Box::new(Expression::Number(Number::from_str(rhs)?, empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr)?;
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(expected)));
    }
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::Eq,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::String("true".to_string(), empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(false)));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::NotEq,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::String("true".to_string(), empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));
    Ok(())
}
#[test]
fn test_eval_logical_right_operand_unknown() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Bool));
    ctx.set_variable("unk_num", Value::unknown(Type::Number));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::Variable("unk_num".to_string(), empty_span())),
        empty_span(),
    );
    let (_val, _) = evaluator.evaluate(&expr)?;
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::Variable("unk_num".to_string(), empty_span())),
        empty_span(),
    );
    let (_val, _) = evaluator.evaluate(&expr)?;
    Ok(())
}
#[test]
fn test_eval_object_key_unknown_or_bad() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::String));
    let _evaluator = Evaluator::new(&ctx);
    let expr = Expression::Object(
        vec![(
            Expression::Variable("unk".to_string(), empty_span()),
            Expression::Bool(true, empty_span()),
        )],
        empty_span(),
    );
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Object key cannot be unknown")
    );
    Ok(())
}
#[test]
fn test_eval_splats() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let t1 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Bool(true, empty_span())),
        operators: vec![TraversalOperator::AttrSplat(empty_span())],
    };
    let (val, _) =
        Evaluator::new(&ctx).evaluate(&Expression::Traversal(Box::new(t1), empty_span()))?;
    assert_eq!(
        *val.data,
        ValueData::Array(vec![Value::new(Type::Bool, ValueData::Bool(true))])
    );
    let t2 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Bool(true, empty_span())),
        operators: vec![TraversalOperator::FullSplat(empty_span())],
    };
    let errs = Evaluator::new(&ctx)
        .evaluate(&Expression::Traversal(Box::new(t2), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Cannot apply full splat operator to non-sequence value")
    );
    let mut obj1 = BTreeMap::new();
    obj1.insert(
        "name".to_string(),
        Value::new(Type::String, ValueData::String("server1".to_string())),
    );
    obj1.insert(
        "port".to_string(),
        Value::new(
            Type::Number,
            ValueData::Number(Number::new(BigDecimal::from(80))),
        ),
    );
    let mut obj2 = BTreeMap::new();
    obj2.insert(
        "name".to_string(),
        Value::new(Type::String, ValueData::String("server2".to_string())),
    );
    obj2.insert(
        "port".to_string(),
        Value::new(
            Type::Number,
            ValueData::Number(Number::new(BigDecimal::from(443))),
        ),
    );
    let list_val = Value::new(
        Type::List(Box::new(Type::Dynamic)),
        ValueData::Array(vec![
            Value::new(Type::Dynamic, ValueData::Object(obj1)),
            Value::new(Type::Dynamic, ValueData::Object(obj2)),
        ]),
    );
    ctx.set_variable("servers", list_val);
    let t3 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("servers".to_string(), empty_span())),
        operators: vec![
            TraversalOperator::AttrSplat(empty_span()),
            TraversalOperator::GetAttr("name".to_string(), empty_span()),
        ],
    };
    let (val_names, _) =
        Evaluator::new(&ctx).evaluate(&Expression::Traversal(Box::new(t3), empty_span()))?;
    assert_eq!(
        *val_names.data,
        ValueData::Array(vec![
            Value::new(Type::String, ValueData::String("server1".to_string())),
            Value::new(Type::String, ValueData::String("server2".to_string())),
        ])
    );
    let t_null = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Null(empty_span())),
        operators: vec![
            TraversalOperator::AttrSplat(empty_span()),
            TraversalOperator::GetAttr("any".to_string(), empty_span()),
        ],
    };
    let (val_null, _) =
        Evaluator::new(&ctx).evaluate(&Expression::Traversal(Box::new(t_null), empty_span()))?;
    assert_eq!(*val_null.data, ValueData::Array(Vec::new()));
    let t_empty = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Tuple(vec![], empty_span())),
        operators: vec![
            TraversalOperator::AttrSplat(empty_span()),
            TraversalOperator::GetAttr("foo".to_string(), empty_span()),
        ],
    };
    let (val_empty, _) =
        Evaluator::new(&ctx).evaluate(&Expression::Traversal(Box::new(t_empty), empty_span()))?;
    assert_eq!(*val_empty.data, ValueData::Array(Vec::new()));
    Ok(())
}

#[test]
fn test_eval_splats_chained_and_marks() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let mut node0 = BTreeMap::new();
    node0.insert(
        "ip".to_string(),
        Value::new(Type::String, ValueData::String("10.0.0.1".to_string())),
    );
    let mut cluster0 = BTreeMap::new();
    cluster0.insert(
        "nodes".to_string(),
        Value::new(
            Type::List(Box::new(Type::Dynamic)),
            ValueData::Array(vec![Value::new(Type::Dynamic, ValueData::Object(node0))]),
        ),
    );
    let mut node1 = BTreeMap::new();
    node1.insert(
        "ip".to_string(),
        Value::new(Type::String, ValueData::String("10.0.0.2".to_string())),
    );
    let mut cluster1 = BTreeMap::new();
    cluster1.insert(
        "nodes".to_string(),
        Value::new(
            Type::List(Box::new(Type::Dynamic)),
            ValueData::Array(vec![Value::new(Type::Dynamic, ValueData::Object(node1))]),
        ),
    );
    ctx.set_variable(
        "clusters",
        Value::new(
            Type::List(Box::new(Type::Dynamic)),
            ValueData::Array(vec![
                Value::new(Type::Dynamic, ValueData::Object(cluster0)),
                Value::new(Type::Dynamic, ValueData::Object(cluster1)),
            ]),
        ),
    );
    let eval_clusters = Evaluator::new(&ctx);
    let t_chained_full = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("clusters".to_string(), empty_span())),
        operators: vec![
            TraversalOperator::FullSplat(empty_span()),
            TraversalOperator::GetAttr("nodes".to_string(), empty_span()),
            TraversalOperator::Index(
                Expression::Number(Number::new(BigDecimal::from(0)), empty_span()),
                empty_span(),
            ),
            TraversalOperator::GetAttr("ip".to_string(), empty_span()),
        ],
    };
    let (val_ips, _) = eval_clusters.evaluate(&Expression::Traversal(
        Box::new(t_chained_full),
        empty_span(),
    ))?;
    assert_eq!(
        *val_ips.data,
        ValueData::Array(vec![
            Value::new(Type::String, ValueData::String("10.0.0.1".to_string())),
            Value::new(Type::String, ValueData::String("10.0.0.2".to_string())),
        ])
    );
    Ok(())
}

#[test]
fn test_eval_splats_marks_and_unknown() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let mut marked_list = Value::new(
        Type::List(Box::new(Type::Dynamic)),
        ValueData::Array(vec![Value::new(
            Type::String,
            ValueData::String("m1".to_string()),
        )]),
    );
    marked_list
        .marks
        .insert(crate::types::val::ValueMark::Sensitive);
    ctx.set_variable("marked_list", marked_list);
    let eval_marked = Evaluator::new(&ctx);
    let t_marked = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable(
            "marked_list".to_string(),
            empty_span(),
        )),
        operators: vec![TraversalOperator::AttrSplat(empty_span())],
    };
    let (val_marked, _) =
        eval_marked.evaluate(&Expression::Traversal(Box::new(t_marked), empty_span()))?;
    assert!(
        val_marked
            .marks
            .contains(&crate::types::val::ValueMark::Sensitive)
    );
    ctx.set_variable(
        "unknown_seq",
        Value::unknown(Type::List(Box::new(Type::String))),
    );
    let eval_unk = Evaluator::new(&ctx);
    let t_unk = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable(
            "unknown_seq".to_string(),
            empty_span(),
        )),
        operators: vec![TraversalOperator::FullSplat(empty_span())],
    };
    let (val_unk, _) = eval_unk.evaluate(&Expression::Traversal(Box::new(t_unk), empty_span()))?;
    assert!(val_unk.is_unknown());
    Ok(())
}
#[test]
fn test_eval_func_call_expand_final() -> Result<(), crate::error::HclError> {
    let ctx = Context::new().enable_stdlib();
    let expr = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "format".into(),
            args: vec![
                Expression::String("%s-%d".to_string(), empty_span()),
                Expression::Tuple(
                    vec![
                        Expression::String("host".to_string(), empty_span()),
                        Expression::Number(Number::new(BigDecimal::from(1)), empty_span()),
                    ],
                    empty_span(),
                ),
            ],
            expand_final: true,
        }),
        empty_span(),
    );
    let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
    assert_eq!(*val.data, ValueData::String("host-1".to_string()));
    let expr_concat = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "concat".into(),
            args: vec![Expression::Tuple(
                vec![
                    Expression::Tuple(
                        vec![Expression::String("a".to_string(), empty_span())],
                        empty_span(),
                    ),
                    Expression::Tuple(
                        vec![
                            Expression::String("b".to_string(), empty_span()),
                            Expression::String("c".to_string(), empty_span()),
                        ],
                        empty_span(),
                    ),
                ],
                empty_span(),
            )],
            expand_final: true,
        }),
        empty_span(),
    );
    let (val_concat, _) = Evaluator::new(&ctx).evaluate(&expr_concat)?;
    assert_eq!(
        *val_concat.data,
        ValueData::Array(vec![
            Value::new(Type::String, ValueData::String("a".to_string())),
            Value::new(Type::String, ValueData::String("b".to_string())),
            Value::new(Type::String, ValueData::String("c".to_string())),
        ])
    );
    Ok(())
}

#[test]
fn test_eval_func_call_expand_final_marks() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new().enable_stdlib();
    let mut sensitive_list = Value::new(
        Type::List(Box::new(Type::String)),
        ValueData::Array(vec![Value::new(
            Type::String,
            ValueData::String("val".to_string()),
        )]),
    );
    sensitive_list
        .marks
        .insert(crate::types::val::ValueMark::Sensitive);
    ctx.set_variable("sec_args", sensitive_list);
    ctx.set_function(
        "check_mark",
        crate::eval::func::Function {
            name: "check_mark".to_string(),
            func: std::sync::Arc::new(|args| {
                if args.first().is_some_and(|first| {
                    first
                        .marks
                        .contains(&crate::types::val::ValueMark::Sensitive)
                }) {
                    return Ok(Value::new(Type::Bool, ValueData::Bool(true)));
                }
                Ok(Value::new(Type::Bool, ValueData::Bool(false)))
            }),
            signature: None,
        },
    );
    let expr_mark = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "check_mark".into(),
            args: vec![Expression::Variable("sec_args".to_string(), empty_span())],
            expand_final: true,
        }),
        empty_span(),
    );
    let (val_mark, _) = Evaluator::new(&ctx).evaluate(&expr_mark)?;
    assert_eq!(*val_mark.data, ValueData::Bool(true));
    let expr_unmark = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "check_mark".into(),
            args: vec![Expression::String("plain".to_string(), empty_span())],
            expand_final: false,
        }),
        empty_span(),
    );
    let (val_unmark, _) = Evaluator::new(&ctx).evaluate(&expr_unmark)?;
    assert_eq!(*val_unmark.data, ValueData::Bool(false));
    Ok(())
}

#[test]
fn test_eval_func_call_expand_final_sets_and_errors() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new().enable_stdlib();
    let mut set_data = BTreeSet::new();
    set_data.insert(Value::new(
        Type::String,
        ValueData::String("s1".to_string()),
    ));
    ctx.set_variable(
        "my_set",
        Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_data)),
    );
    ctx.set_function(
        "count_args",
        crate::eval::func::Function {
            name: "count_args".to_string(),
            func: std::sync::Arc::new(|args| {
                let len = u64::try_from(args.len()).unwrap_or_default();
                Ok(Value::new(
                    Type::Number,
                    ValueData::Number(Number::new(BigDecimal::from(len))),
                ))
            }),
            signature: None,
        },
    );
    let expr_set = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "count_args".into(),
            args: vec![Expression::Variable("my_set".to_string(), empty_span())],
            expand_final: true,
        }),
        empty_span(),
    );
    let (val_count, _) = Evaluator::new(&ctx).evaluate(&expr_set)?;
    assert_eq!(
        *val_count.data,
        ValueData::Number(Number::new(BigDecimal::from(1)))
    );
    ctx.set_variable(
        "unk_args",
        Value::unknown(Type::List(Box::new(Type::String))),
    );
    let expr_unk = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "format".into(),
            args: vec![
                Expression::String("%s".to_string(), empty_span()),
                Expression::Variable("unk_args".to_string(), empty_span()),
            ],
            expand_final: true,
        }),
        empty_span(),
    );
    let (val_unk, _) = Evaluator::new(&ctx).evaluate(&expr_unk)?;
    assert!(val_unk.is_unknown());
    let expr_bad = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "format".into(),
            args: vec![Expression::Number(
                Number::new(BigDecimal::from(42)),
                empty_span(),
            )],
            expand_final: true,
        }),
        empty_span(),
    );
    let errs = Evaluator::new(&ctx)
        .evaluate(&expr_bad)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Cannot expand non-sequence")
    );
    Ok(())
}
#[test]
fn test_eval_lazy_try_and_can() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let mut map = BTreeMap::new();
    map.insert(
        "existing".to_string(),
        Value::new(Type::String, ValueData::String("found".to_string())),
    );
    ctx.set_variable(
        "obj",
        Value::new(Type::object(BTreeMap::new()), ValueData::Object(map)),
    );
    ctx.set_variable(
        "arr",
        Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(vec![Value::new(
                Type::Number,
                ValueData::Number(Number::new(BigDecimal::from(10))),
            )]),
        ),
    );
    let expr_try_fallback = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "try".into(),
            args: vec![
                Expression::Traversal(
                    Box::new(crate::ast::expr::Traversal {
                        expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                        operators: vec![TraversalOperator::GetAttr(
                            "missing".to_string(),
                            empty_span(),
                        )],
                    }),
                    empty_span(),
                ),
                Expression::String("fallback".to_string(), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let (val_try1, _) = Evaluator::new(&ctx).evaluate(&expr_try_fallback)?;
    assert_eq!(*val_try1.data, ValueData::String("fallback".to_string()));
    let expr_try_found = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "try".into(),
            args: vec![
                Expression::Traversal(
                    Box::new(crate::ast::expr::Traversal {
                        expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                        operators: vec![TraversalOperator::GetAttr(
                            "existing".to_string(),
                            empty_span(),
                        )],
                    }),
                    empty_span(),
                ),
                Expression::String("fallback".to_string(), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let (val_try2, _) = Evaluator::new(&ctx).evaluate(&expr_try_found)?;
    assert_eq!(*val_try2.data, ValueData::String("found".to_string()));
    Ok(())
}

#[test]
fn test_eval_lazy_try_cascade() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let mut map = BTreeMap::new();
    map.insert(
        "existing".to_string(),
        Value::new(Type::String, ValueData::String("found".to_string())),
    );
    ctx.set_variable(
        "obj",
        Value::new(Type::object(BTreeMap::new()), ValueData::Object(map)),
    );
    ctx.set_variable(
        "arr",
        Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(vec![Value::new(
                Type::Number,
                ValueData::Number(Number::new(BigDecimal::from(10))),
            )]),
        ),
    );
    let expr_try_cascade = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "try".into(),
            args: vec![
                Expression::Traversal(
                    Box::new(crate::ast::expr::Traversal {
                        expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
                        operators: vec![TraversalOperator::Index(
                            Expression::Number(Number::new(BigDecimal::from(100)), empty_span()),
                            empty_span(),
                        )],
                    }),
                    empty_span(),
                ),
                Expression::Traversal(
                    Box::new(crate::ast::expr::Traversal {
                        expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                        operators: vec![TraversalOperator::GetAttr(
                            "missing".to_string(),
                            empty_span(),
                        )],
                    }),
                    empty_span(),
                ),
                Expression::Number(Number::new(BigDecimal::from(42)), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let (val_try3, _) = Evaluator::new(&ctx).evaluate(&expr_try_cascade)?;
    assert_eq!(
        *val_try3.data,
        ValueData::Number(Number::new(BigDecimal::from(42)))
    );
    Ok(())
}

#[test]
fn test_eval_lazy_try_errors() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let mut map = BTreeMap::new();
    map.insert(
        "existing".to_string(),
        Value::new(Type::String, ValueData::String("found".to_string())),
    );
    ctx.set_variable(
        "obj",
        Value::new(Type::object(BTreeMap::new()), ValueData::Object(map)),
    );
    let expr_try_zero = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "try".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    let err_try_zero = Evaluator::new(&ctx)
        .evaluate(&expr_try_zero)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_try_zero.errors()[0]
            .error
            .to_string()
            .contains("requires at least one argument")
    );
    let expr_try_all_fail = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "try".into(),
            args: vec![
                Expression::Traversal(
                    Box::new(crate::ast::expr::Traversal {
                        expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                        operators: vec![TraversalOperator::GetAttr(
                            "missing1".to_string(),
                            empty_span(),
                        )],
                    }),
                    empty_span(),
                ),
                Expression::Traversal(
                    Box::new(crate::ast::expr::Traversal {
                        expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                        operators: vec![TraversalOperator::GetAttr(
                            "missing2".to_string(),
                            empty_span(),
                        )],
                    }),
                    empty_span(),
                ),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let err_try_fail = Evaluator::new(&ctx)
        .evaluate(&expr_try_all_fail)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_try_fail.errors()[0]
            .error
            .to_string()
            .contains("No alternative expression succeeded in 'try'")
    );
    Ok(())
}

#[test]
fn test_eval_lazy_can() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let mut map = BTreeMap::new();
    map.insert(
        "existing".to_string(),
        Value::new(Type::String, ValueData::String("found".to_string())),
    );
    ctx.set_variable(
        "obj",
        Value::new(Type::object(BTreeMap::new()), ValueData::Object(map)),
    );
    let expr_can_false = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "can".into(),
            args: vec![Expression::Traversal(
                Box::new(crate::ast::expr::Traversal {
                    expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                    operators: vec![TraversalOperator::GetAttr(
                        "missing".to_string(),
                        empty_span(),
                    )],
                }),
                empty_span(),
            )],
            expand_final: false,
        }),
        empty_span(),
    );
    let (val_can1, _) = Evaluator::new(&ctx).evaluate(&expr_can_false)?;
    assert_eq!(*val_can1.data, ValueData::Bool(false));
    let expr_can_true = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "can".into(),
            args: vec![Expression::Traversal(
                Box::new(crate::ast::expr::Traversal {
                    expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                    operators: vec![TraversalOperator::GetAttr(
                        "existing".to_string(),
                        empty_span(),
                    )],
                }),
                empty_span(),
            )],
            expand_final: false,
        }),
        empty_span(),
    );
    let (val_can2, _) = Evaluator::new(&ctx).evaluate(&expr_can_true)?;
    assert_eq!(*val_can2.data, ValueData::Bool(true));
    let expr_can_div_zero = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "can".into(),
            args: vec![Expression::BinaryOp(
                BinaryOp::Div,
                Box::new(Expression::Number(
                    Number::new(BigDecimal::from(10)),
                    empty_span(),
                )),
                Box::new(Expression::Number(
                    Number::new(BigDecimal::from(0)),
                    empty_span(),
                )),
                empty_span(),
            )],
            expand_final: false,
        }),
        empty_span(),
    );
    let (val_can_div, _) = Evaluator::new(&ctx).evaluate(&expr_can_div_zero)?;
    assert_eq!(*val_can_div.data, ValueData::Bool(false));
    ctx.set_variable("unk", Value::unknown(Type::String));
    let expr_can_unk = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "can".into(),
            args: vec![Expression::Variable("unk".to_string(), empty_span())],
            expand_final: false,
        }),
        empty_span(),
    );
    let (val_can_unk, _) = Evaluator::new(&ctx).evaluate(&expr_can_unk)?;
    assert!(val_can_unk.is_unknown());
    assert_eq!(val_can_unk.ty(), &Type::Bool);
    let expr_can_bad_arity = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "can".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    let err_can_bad = Evaluator::new(&ctx)
        .evaluate(&expr_can_bad_arity)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_can_bad.errors()[0]
            .error
            .to_string()
            .contains("requires exactly one argument")
    );
    Ok(())
}
#[test]
fn test_eval_function_signatures_and_type_checking() -> Result<(), crate::error::HclError> {
    use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
    use std::sync::Arc;
    let mut ctx = Context::new();
    let sig_calc = FunctionSignature::with_static_return_type(
        vec![
            FunctionParamSpec::new("num1", Type::Number),
            FunctionParamSpec::new("num2", Type::Number),
        ],
        Type::Number,
    );
    let fn_add = Function::new(
        "add_nums",
        Arc::new(|args| {
            let n1 = match *args[0].data {
                ValueData::Number(ref n) => n.clone(),
                _ => return Err(crate::error::HclError::Type("expected number".to_string())),
            };
            let n2 = match *args[1].data {
                ValueData::Number(ref n) => n.clone(),
                _ => return Err(crate::error::HclError::Type("expected number".to_string())),
            };
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(&n1.0 + &n2.0)),
            ))
        }),
    )
    .with_signature(sig_calc);
    assert!(
        (fn_add.func)(&[
            Value::new(Type::Bool, ValueData::Bool(true)),
            Value::new(Type::Bool, ValueData::Bool(true)),
        ])
        .is_err()
    );
    assert!(
        (fn_add.func)(&[
            Value::new(
                Type::Number,
                ValueData::Number(Number::new(BigDecimal::from(1)))
            ),
            Value::new(Type::Bool, ValueData::Bool(true)),
        ])
        .is_err()
    );
    ctx.set_function("add_nums", fn_add);
    let expr_add_ok = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "add_nums".into(),
            args: vec![
                Expression::String("10".to_string(), empty_span()),
                Expression::Number(Number::new(BigDecimal::from(20)), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let (val_add, _) = Evaluator::new(&ctx).evaluate(&expr_add_ok)?;
    assert_eq!(
        *val_add.data,
        ValueData::Number(Number::new(BigDecimal::from(30)))
    );
    let expr_add_few = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "add_nums".into(),
            args: vec![Expression::Number(
                Number::new(BigDecimal::from(10)),
                empty_span(),
            )],
            expand_final: false,
        }),
        empty_span(),
    );
    let err_few = Evaluator::new(&ctx)
        .evaluate(&expr_add_few)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_few.errors()[0]
            .error
            .to_string()
            .contains("Wrong number of arguments")
    );
    Ok(())
}

#[test]
fn test_eval_function_signatures_incompat_and_null() -> Result<(), crate::error::HclError> {
    use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
    use std::sync::Arc;
    let mut ctx = Context::new();
    let sig_calc = FunctionSignature::with_static_return_type(
        vec![
            FunctionParamSpec::new("num1", Type::Number),
            FunctionParamSpec::new("num2", Type::Number),
        ],
        Type::Number,
    );
    let fn_add = Function::new(
        "add_nums",
        Arc::new(|args| {
            let n1 = match *args[0].data {
                ValueData::Number(ref n) => n.clone(),
                _ => return Err(crate::error::HclError::Type("expected number".to_string())),
            };
            let n2 = match *args[1].data {
                ValueData::Number(ref n) => n.clone(),
                _ => return Err(crate::error::HclError::Type("expected number".to_string())),
            };
            Ok(Value::new(
                Type::Number,
                ValueData::Number(Number::new(&n1.0 + &n2.0)),
            ))
        }),
    )
    .with_signature(sig_calc);
    ctx.set_function("add_nums", fn_add);
    let expr_add_many = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "add_nums".into(),
            args: vec![
                Expression::Number(Number::new(BigDecimal::from(1)), empty_span()),
                Expression::Number(Number::new(BigDecimal::from(2)), empty_span()),
                Expression::Number(Number::new(BigDecimal::from(3)), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let err_many = Evaluator::new(&ctx)
        .evaluate(&expr_add_many)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_many.errors()[0]
            .error
            .to_string()
            .contains("Wrong number of arguments")
    );
    let expr_add_incompat = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "add_nums".into(),
            args: vec![
                Expression::Bool(true, empty_span()),
                Expression::Number(Number::new(BigDecimal::from(2)), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let err_incompat = Evaluator::new(&ctx)
        .evaluate(&expr_add_incompat)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_incompat.errors()[0]
            .error
            .to_string()
            .contains("Invalid argument")
    );
    let expr_add_null = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "add_nums".into(),
            args: vec![
                Expression::Null(empty_span()),
                Expression::Number(Number::new(BigDecimal::from(2)), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let err_null = Evaluator::new(&ctx)
        .evaluate(&expr_add_null)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_null.errors()[0]
            .error
            .to_string()
            .contains("cannot be null")
    );
    Ok(())
}

#[test]
fn test_eval_function_signatures_variadic_and_errors() -> Result<(), crate::error::HclError> {
    use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
    use std::sync::Arc;
    let mut ctx = Context::new();
    let sig_join_custom = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("sep", Type::String)],
        Type::String,
    )
    .with_variadic(FunctionParamSpec::new("items", Type::String));
    let fn_join_custom = Function::new(
        "custom_join",
        Arc::new(|args| {
            let sep = match *args[0].data {
                ValueData::String(ref s) => s.clone(),
                _ => return Err(crate::error::HclError::Type("expected string".to_string())),
            };
            let mut parts = Vec::new();
            for arg in &args[1..] {
                if let ValueData::String(ref s) = *arg.data {
                    parts.push(s.clone());
                }
            }
            Ok(Value::new(
                Type::String,
                ValueData::String(parts.join(&sep)),
            ))
        }),
    )
    .with_signature(sig_join_custom);
    assert!((fn_join_custom.func)(&[Value::new(Type::Bool, ValueData::Bool(true))]).is_err());
    let _ = (fn_join_custom.func)(&[
        Value::new(Type::String, ValueData::String(":".to_string())),
        Value::new(Type::Bool, ValueData::Bool(true)),
    ]);
    ctx.set_function("custom_join", fn_join_custom);
    let expr_join_ok = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "custom_join".into(),
            args: vec![
                Expression::String(":".to_string(), empty_span()),
                Expression::String("a".to_string(), empty_span()),
                Expression::String("b".to_string(), empty_span()),
                Expression::String("c".to_string(), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let (val_join, _) = Evaluator::new(&ctx).evaluate(&expr_join_ok)?;
    assert_eq!(*val_join.data, ValueData::String("a:b:c".to_string()));
    let expr_join_few = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "custom_join".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    let err_join_few = Evaluator::new(&ctx)
        .evaluate(&expr_join_few)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_join_few.errors()[0]
            .error
            .to_string()
            .contains("Too few arguments")
    );
    Ok(())
}
#[test]
fn test_eval_traversal_errors() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let t1 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Null(empty_span())),
        operators: vec![TraversalOperator::GetAttr("a".to_string(), empty_span())],
    };
    let errs = evaluator
        .evaluate(&Expression::Traversal(Box::new(t1), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Attempt to traverse null value")
    );
    let evaluator = Evaluator::new(&ctx);
    let t2 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Bool(true, empty_span())),
        operators: vec![TraversalOperator::GetAttr("a".to_string(), empty_span())],
    };
    let errs = evaluator
        .evaluate(&Expression::Traversal(Box::new(t2), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Cannot get attribute from non-object")
    );
    let mut map = BTreeMap::new();
    map.insert(
        "a".to_string(),
        Value::new(Type::Bool, ValueData::Bool(true)),
    );
    let obj = Value::new(Type::object(BTreeMap::new()), ValueData::Object(map));
    ctx.set_variable("obj", obj);
    let evaluator = Evaluator::new(&ctx);
    let t3 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
        operators: vec![TraversalOperator::GetAttr("b".to_string(), empty_span())],
    };
    let errs = evaluator
        .evaluate(&Expression::Traversal(Box::new(t3), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(errs.errors()[0].error.to_string().contains("not found"));
    let arr = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
    ctx.set_variable("arr", arr);
    let evaluator = Evaluator::new(&ctx);
    let t4 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
        operators: vec![TraversalOperator::Index(
            Expression::Bool(true, empty_span()),
            empty_span(),
        )],
    };
    let errs = evaluator
        .evaluate(&Expression::Traversal(Box::new(t4), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Invalid index operation")
    );
    Ok(())
}

#[test]
fn test_eval_traversal_errors_bounds_and_legacy() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let mut map = BTreeMap::new();
    map.insert(
        "a".to_string(),
        Value::new(Type::Bool, ValueData::Bool(true)),
    );
    let obj = Value::new(Type::object(BTreeMap::new()), ValueData::Object(map));
    ctx.set_variable("obj", obj);
    let arr = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
    ctx.set_variable("arr", arr);
    let evaluator = Evaluator::new(&ctx);
    let t4 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
        operators: vec![TraversalOperator::Index(
            Expression::Number(Number::from_str("10")?, empty_span()),
            empty_span(),
        )],
    };
    let errs = evaluator
        .evaluate(&Expression::Traversal(Box::new(t4), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Index out of bounds")
    );
    let evaluator = Evaluator::new(&ctx);
    let t4 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
        operators: vec![TraversalOperator::LegacyIndex(10, empty_span())],
    };
    let errs = evaluator
        .evaluate(&Expression::Traversal(Box::new(t4), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Index out of bounds")
    );
    let evaluator = Evaluator::new(&ctx);
    let t4 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Bool(true, empty_span())),
        operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
    };
    let errs = evaluator
        .evaluate(&Expression::Traversal(Box::new(t4), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Legacy index on non-array")
    );
    let evaluator = Evaluator::new(&ctx);
    let t5 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
        operators: vec![TraversalOperator::Index(
            Expression::String("b".to_string(), empty_span()),
            empty_span(),
        )],
    };
    let errs = evaluator
        .evaluate(&Expression::Traversal(Box::new(t5), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(errs.errors()[0].error.to_string().contains("not found"));
    let mut ctx2 = Context::new();
    ctx2.set_variable(
        "arr",
        Value::new(Type::Tuple(vec![]), ValueData::Array(vec![])),
    );
    ctx2.set_variable("unk", Value::unknown(Type::Number));
    let evaluator = Evaluator::new(&ctx2);
    let t5 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
        operators: vec![TraversalOperator::Index(
            Expression::Variable("unk".to_string(), empty_span()),
            empty_span(),
        )],
    };
    let (_val, _) = evaluator.evaluate(&Expression::Traversal(Box::new(t5), empty_span()))?;
    Ok(())
}
#[test]
fn test_eval_for_expr_errors() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let f = ForExpr {
        key_var: None,
        val_var: "v".to_string(),
        collection: Box::new(Expression::Bool(true, empty_span())),
        key_expr: None,
        val_expr: Box::new(Expression::Bool(true, empty_span())),
        cond_expr: None,
        grouping: false,
    };
    let errs = evaluator
        .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("collection must be a tuple")
    );
    ctx.set_variable("unk", Value::unknown(Type::Tuple(vec![])));
    let evaluator = Evaluator::new(&ctx);
    let f = ForExpr {
        key_var: None,
        val_var: "v".to_string(),
        collection: Box::new(Expression::Variable("unk".to_string(), empty_span())),
        key_expr: None,
        val_expr: Box::new(Expression::Bool(true, empty_span())),
        cond_expr: None,
        grouping: false,
    };
    let (_val, _) = evaluator.evaluate(&Expression::ForExpr(Box::new(f), empty_span()))?;
    let arr = Value::new(
        Type::Tuple(vec![]),
        ValueData::Array(vec![Value::new(Type::Bool, ValueData::Bool(true))]),
    );
    ctx.set_variable("arr", arr);
    let evaluator = Evaluator::new(&ctx);
    let f = ForExpr {
        key_var: None,
        val_var: "v".to_string(),
        collection: Box::new(Expression::Variable("arr".to_string(), empty_span())),
        key_expr: None,
        val_expr: Box::new(Expression::Bool(true, empty_span())),
        cond_expr: Some(Box::new(Expression::Number(
            Number::from_str("1")?,
            empty_span(),
        ))),
        grouping: false,
    };
    let errs = evaluator
        .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("condition must be a boolean")
    );
    let evaluator = Evaluator::new(&ctx);
    let f = ForExpr {
        key_var: None,
        val_var: "v".to_string(),
        collection: Box::new(Expression::Variable("arr".to_string(), empty_span())),
        key_expr: None,
        val_expr: Box::new(Expression::Bool(true, empty_span())),
        cond_expr: Some(Box::new(Expression::Bool(false, empty_span()))),
        grouping: false,
    };
    let (val, _) = evaluator.evaluate(&Expression::ForExpr(Box::new(f), empty_span()))?;
    assert_eq!(val.data.as_ref(), &ValueData::Array(vec![]));
    let evaluator = Evaluator::new(&ctx);
    let f = ForExpr {
        key_var: None,
        val_var: "v".to_string(),
        collection: Box::new(Expression::Variable("arr".to_string(), empty_span())),
        key_expr: Some(Box::new(Expression::Bool(true, empty_span()))),
        val_expr: Box::new(Expression::Bool(true, empty_span())),
        cond_expr: None,
        grouping: false,
    };
    let errs = evaluator
        .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("key must evaluate to string")
    );
    Ok(())
}
#[test]
fn test_eval_for_expr_grouping() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let arr = Value::new(
        Type::Tuple(vec![]),
        ValueData::Array(vec![
            Value::new(Type::Bool, ValueData::Bool(true)),
            Value::new(Type::Bool, ValueData::Bool(false)),
        ]),
    );
    ctx.set_variable("arr", arr);
    let evaluator = Evaluator::new(&ctx);
    let f = ForExpr {
        key_var: None,
        val_var: "v".to_string(),
        collection: Box::new(Expression::Variable("arr".to_string(), empty_span())),
        key_expr: Some(Box::new(Expression::String(
            "fixed".to_string(),
            empty_span(),
        ))),
        val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
        cond_expr: None,
        grouping: true,
    };
    let (val, _) = evaluator.evaluate(&Expression::ForExpr(Box::new(f), empty_span()))?;
    let mut expected_map = BTreeMap::new();
    expected_map.insert(
        "fixed".to_string(),
        Value::new(
            Type::Tuple(vec![]),
            ValueData::Array(vec![
                Value::new(Type::Bool, ValueData::Bool(true)),
                Value::new(Type::Bool, ValueData::Bool(false)),
            ]),
        ),
    );
    assert_eq!(val.data.as_ref(), &ValueData::Object(expected_map));
    Ok(())
}
#[test]
fn test_eval_traversal() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    let mut map = BTreeMap::new();
    map.insert(
        "a".to_string(),
        Value::new(Type::Number, ValueData::Number(Number::from_str("1")?)),
    );
    let obj = Value::new(Type::object(BTreeMap::new()), ValueData::Object(map));
    ctx.set_variable("obj", obj);
    let arr = Value::new(
        Type::Tuple(vec![]),
        ValueData::Array(vec![Value::new(Type::Bool, ValueData::Bool(true))]),
    );
    ctx.set_variable("arr", arr);
    let evaluator = Evaluator::new(&ctx);
    let t1 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
        operators: vec![TraversalOperator::GetAttr("a".to_string(), empty_span())],
    };
    let (val, _) = evaluator.evaluate(&Expression::Traversal(Box::new(t1), empty_span()))?;
    assert_eq!(
        val,
        Value::new(Type::Number, ValueData::Number(Number::from_str("1")?))
    );
    let evaluator = Evaluator::new(&ctx);
    let t2 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
        operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
    };
    let (val2, _) = evaluator.evaluate(&Expression::Traversal(Box::new(t2), empty_span()))?;
    assert_eq!(val2, Value::new(Type::Bool, ValueData::Bool(true)));
    Ok(())
}
#[test]
fn test_eval_for_expr_tuple() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let f = ForExpr {
        key_var: None,
        val_var: "v".to_string(),
        collection: Box::new(Expression::Tuple(
            vec![
                Expression::Number(Number::from_str("1")?, empty_span()),
                Expression::Number(Number::from_str("2")?, empty_span()),
            ],
            empty_span(),
        )),
        key_expr: None,
        val_expr: Box::new(Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Variable("v".to_string(), empty_span())),
            empty_span(),
        )),
        cond_expr: None,
        grouping: false,
    };
    let (val, _) = evaluator.evaluate(&Expression::ForExpr(Box::new(f), empty_span()))?;
    let expected_arr = vec![
        Value::new(Type::Number, ValueData::Number(Number::from_str("-1")?)),
        Value::new(Type::Number, ValueData::Number(Number::from_str("-2")?)),
    ];
    assert_eq!(val.data.as_ref(), &ValueData::Array(expected_arr));
    Ok(())
}
#[test]
fn test_eval_for_expr_object() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let evaluator = Evaluator::new(&ctx);
    let k1 = Expression::String("a".to_string(), empty_span());
    let v1 = Expression::Number(Number::from_str("1")?, empty_span());
    let f = ForExpr {
        key_var: Some("k".to_string()),
        val_var: "v".to_string(),
        collection: Box::new(Expression::Object(vec![(k1, v1)], empty_span())),
        key_expr: Some(Box::new(Expression::Variable(
            "k".to_string(),
            empty_span(),
        ))),
        val_expr: Box::new(Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Variable("v".to_string(), empty_span())),
            empty_span(),
        )),
        cond_expr: None,
        grouping: false,
    };
    let (val, _) = evaluator.evaluate(&Expression::ForExpr(Box::new(f), empty_span()))?;
    let mut expected_map = BTreeMap::new();
    expected_map.insert(
        "a".to_string(),
        Value::new(Type::Number, ValueData::Number(Number::from_str("-1")?)),
    );
    assert_eq!(val.data.as_ref(), &ValueData::Object(expected_map));
    Ok(())
}
#[test]
fn test_eval_cov() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::object(BTreeMap::new())));
    let evaluator = Evaluator::new(&ctx);
    let t1 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("unk".to_string(), empty_span())),
        operators: vec![TraversalOperator::GetAttr("a".to_string(), empty_span())],
    };
    let (_val, _) = evaluator.evaluate(&Expression::Traversal(Box::new(t1), empty_span()))?;
    let arr = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
    ctx.set_variable("arr", arr);
    let evaluator = Evaluator::new(&ctx);
    let t2 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
        operators: vec![TraversalOperator::Index(
            Expression::Number(Number::from_str("-1")?, empty_span()),
            empty_span(),
        )],
    };
    let errs = evaluator
        .evaluate(&Expression::Traversal(Box::new(t2), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Invalid array index")
    );
    let evaluator = Evaluator::new(&ctx);
    let f = ForExpr {
        key_var: None,
        val_var: "v".to_string(),
        collection: Box::new(Expression::Tuple(
            vec![Expression::Bool(true, empty_span())],
            empty_span(),
        )),
        key_expr: None,
        val_expr: Box::new(Expression::Bool(true, empty_span())),
        cond_expr: Some(Box::new(Expression::Number(
            Number::from_str("1")?,
            empty_span(),
        ))),
        grouping: false,
    };
    let errs = evaluator
        .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("must be a boolean")
    );
    let evaluator = Evaluator::new(&ctx);
    let t3 = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("unk".to_string(), empty_span())),
        operators: vec![TraversalOperator::Index(
            Expression::Variable("unk".to_string(), empty_span()),
            empty_span(),
        )],
    };
    let (_val, _) = evaluator.evaluate(&Expression::Traversal(Box::new(t3), empty_span()))?;
    Ok(())
}
#[test]
fn test_eval_template_parts() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable(
        "num",
        Value::new(
            Type::Number,
            ValueData::Number(crate::number::Number::new(std::str::FromStr::from_str(
                "42.0",
            )?)),
        ),
    );
    ctx.set_variable("bool", Value::new(Type::Bool, ValueData::Bool(true)));
    ctx.set_variable(
        "obj",
        Value::new(
            Type::object(std::collections::BTreeMap::new()),
            ValueData::Object(std::collections::BTreeMap::new()),
        ),
    );
    ctx.set_variable("null", Value::null(Type::Dynamic));
    let evaluator = Evaluator::new(&ctx);
    let span = empty_span();
    let expr = Expression::Template(
        vec![
            crate::ast::expr::TemplatePart::Literal("literal ".to_string(), span.clone()),
            crate::ast::expr::TemplatePart::Interpolation(
                Expression::Variable("num".to_string(), span.clone()),
                span.clone(),
            ),
            crate::ast::expr::TemplatePart::Literal(" ".to_string(), span.clone()),
            crate::ast::expr::TemplatePart::Interpolation(
                Expression::Variable("bool".to_string(), span.clone()),
                span.clone(),
            ),
            crate::ast::expr::TemplatePart::Literal(" ".to_string(), span.clone()),
            crate::ast::expr::TemplatePart::Interpolation(
                Expression::Variable("null".to_string(), span.clone()),
                span.clone(),
            ),
            crate::ast::expr::TemplatePart::Literal(" ".to_string(), span.clone()),
            crate::ast::expr::TemplatePart::Interpolation(
                Expression::Variable("obj".to_string(), span.clone()),
                span.clone(),
            ),
            crate::ast::expr::TemplatePart::Directive(
                crate::ast::expr::Directive::If {
                    cond: Expression::Bool(true, span.clone()),
                    true_expr: vec![],
                    else_ifs: vec![],
                    false_expr: None,
                },
                span.clone(),
            ),
        ],
        span,
    );
    let result = evaluator.evaluate(&expr);
    assert!(result.is_err());
    let diags = result
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert_eq!(diags.errors().len(), 1);
    assert!(
        diags.errors()[0]
            .error
            .to_string()
            .contains("Cannot stringify complex value")
    );
    Ok(())
}
#[test]
fn test_eval_binary_math_unknown_rhs() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Number));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        crate::ast::expr::BinaryOp::Add,
        Box::new(Expression::Number(
            crate::number::Number::new(std::str::FromStr::from_str("1.0")?),
            empty_span(),
        )),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert!(val.is_unknown());
    assert_eq!(*val.ty(), Type::Number);
    let expr2 = Expression::BinaryOp(
        crate::ast::expr::BinaryOp::Eq,
        Box::new(Expression::Number(
            crate::number::Number::new(std::str::FromStr::from_str("1.0")?),
            empty_span(),
        )),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let evaluator2 = Evaluator::new(&ctx);
    let (val2, _) = evaluator2.evaluate(&expr2)?;
    assert!(val2.is_unknown());
    assert_eq!(*val2.ty(), Type::Bool);
    Ok(())
}
#[test]
fn test_eval_template_interpolation_null() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("null_var", Value::null(Type::String));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::Template(
        vec![crate::ast::expr::TemplatePart::Interpolation(
            Expression::Variable("null_var".to_string(), empty_span()),
            empty_span(),
        )],
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val.ty(), &Type::String);
    Ok(())
}
#[test]
fn test_eval_binary_math_unknown() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Number));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        crate::ast::expr::BinaryOp::Add,
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        Box::new(Expression::Bool(false, empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert!(val.is_unknown());
    assert_eq!(*val.ty(), Type::Number);
    Ok(())
}
#[test]
fn test_eval_cov2() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Bool));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(false)));
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));
    let _evaluator = Evaluator::new(&ctx);
    let expr = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Bool(true, empty_span())),
            operators: vec![TraversalOperator::FullSplat(empty_span())],
        }),
        empty_span(),
    );
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .to_lowercase()
            .contains("splat")
    );
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::Template(vec![], empty_span());
    let val = evaluator.evaluate(&expr)?.0;
    assert_eq!(
        val,
        Value::new(Type::String, ValueData::String(String::new()))
    );
    let expr = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "test_func_err".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    let mut ctx = Context::new();
    ctx.set_function(
        "test_func_err",
        crate::eval::func::Function {
            name: "test_func_err".to_string(),
            func: std::sync::Arc::new(|_| {
                Err(crate::error::HclError::Eval("func error".to_string()))
            }),
            signature: None,
        },
    );
    let _evaluator = Evaluator::new(&ctx);
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Function call 'test_func_err' failed")
    );
    let expr = Expression::Conditional(
        Box::new(crate::ast::expr::Conditional {
            cond_expr: Expression::String("not bool".to_string(), empty_span()),
            true_expr: Expression::Bool(true, empty_span()),
            false_expr: Expression::Bool(false, empty_span()),
        }),
        empty_span(),
    );
    let _evaluator = Evaluator::new(&ctx);
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Condition must be a boolean")
    );
    Ok(())
}

#[test]
fn test_eval_cov2_logical_and_traversal() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let expr = Expression::BinaryOp(
        crate::ast::expr::BinaryOp::And,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::String("not bool".to_string(), empty_span())),
        empty_span(),
    );
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Right operand of && must be a boolean")
    );
    let expr = Expression::BinaryOp(
        crate::ast::expr::BinaryOp::Or,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::String("not bool".to_string(), empty_span())),
        empty_span(),
    );
    let evaluator = Evaluator::new(&ctx);
    let errs = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Right operand of || must be a boolean")
    );
    let expr = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Tuple(vec![], empty_span())),
            operators: vec![crate::ast::expr::TraversalOperator::Index(
                Expression::Number(crate::number::Number::from_str("1")?, empty_span()),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let errs = Evaluator::new(&ctx)
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Index out of bounds")
    );
    let expr = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Object(vec![], empty_span())),
            operators: vec![crate::ast::expr::TraversalOperator::Index(
                Expression::String("missing".to_string(), empty_span()),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let errs = Evaluator::new(&ctx)
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Key 'missing' not found")
    );
    Ok(())
}

#[test]
fn test_eval_cov2_for_cond_not_bool() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let expr = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![Expression::Bool(true, empty_span())],
                empty_span(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
            cond_expr: Some(Box::new(Expression::String(
                "not bool".to_string(),
                empty_span(),
            ))),
            grouping: false,
        }),
        empty_span(),
    );
    let errs = Evaluator::new(&ctx)
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("For expression condition must be a boolean")
    );
    Ok(())
}

#[test]
fn test_eval_cov2_part_2() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_function(
        "test_func_err",
        crate::eval::func::Function {
            name: "test_func_err".to_string(),
            func: std::sync::Arc::new(|_| {
                Err(crate::error::HclError::Eval("func error".to_string()))
            }),
            signature: None,
        },
    );
    let expr = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![Expression::Bool(true, empty_span())],
                empty_span(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
            cond_expr: Some(Box::new(Expression::FuncCall(
                Box::new(crate::ast::expr::FuncCall {
                    name: "test_func_err".into(),
                    args: vec![],
                    expand_final: false,
                }),
                empty_span(),
            ))),
            grouping: false,
        }),
        empty_span(),
    );
    let errs = Evaluator::new(&ctx)
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Function call 'test_func_err' failed")
    );
    assert!(
        errs.errors()[1]
            .error
            .to_string()
            .contains("For expression condition must be a boolean")
    );
    let expr = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![Expression::Bool(true, empty_span())],
                empty_span(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::FuncCall(
                Box::new(crate::ast::expr::FuncCall {
                    name: "test_func_err".into(),
                    args: vec![],
                    expand_final: false,
                }),
                empty_span(),
            )),
            cond_expr: None,
            grouping: false,
        }),
        empty_span(),
    );
    let errs = Evaluator::new(&ctx)
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Function call 'test_func_err' failed")
    );
    Ok(())
}

#[test]
fn test_eval_cov2_part_2b() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_function(
        "test_func_err",
        crate::eval::func::Function {
            name: "test_func_err".to_string(),
            func: std::sync::Arc::new(|_| {
                Err(crate::error::HclError::Eval("func error".to_string()))
            }),
            signature: None,
        },
    );
    let _expr = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![Expression::Bool(true, empty_span())],
                empty_span(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
            cond_expr: Some(Box::new(Expression::BinaryOp(
                crate::ast::expr::BinaryOp::And,
                Box::new(Expression::Bool(false, empty_span())),
                Box::new(Expression::FuncCall(
                    Box::new(crate::ast::expr::FuncCall {
                        name: "test_func_err".into(),
                        args: vec![],
                        expand_final: false,
                    }),
                    empty_span(),
                )),
                empty_span(),
            ))),
            grouping: false,
        }),
        empty_span(),
    );
    let expr = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![Expression::Bool(true, empty_span())],
                empty_span(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
            cond_expr: Some(Box::new(Expression::BinaryOp(
                crate::ast::expr::BinaryOp::Or,
                Box::new(Expression::Bool(false, empty_span())),
                Box::new(Expression::FuncCall(
                    Box::new(crate::ast::expr::FuncCall {
                        name: "test_func_err".into(),
                        args: vec![],
                        expand_final: false,
                    }),
                    empty_span(),
                )),
                empty_span(),
            ))),
            grouping: false,
        }),
        empty_span(),
    );
    let errs = Evaluator::new(&ctx)
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        errs.errors()[0]
            .error
            .to_string()
            .contains("Function call 'test_func_err' failed")
    );
    let expr =
        Expression::Parentheses(Box::new(Expression::Bool(true, empty_span())), empty_span());
    let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));
    let expr = Expression::Conditional(
        Box::new(crate::ast::expr::Conditional {
            cond_expr: Expression::Bool(false, empty_span()),
            true_expr: Expression::String("true".to_string(), empty_span()),
            false_expr: Expression::String("false".to_string(), empty_span()),
        }),
        empty_span(),
    );
    let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
    assert_eq!(
        val,
        Value::new(Type::String, ValueData::String("false".to_string()))
    );
    Ok(())
}

#[test]
fn test_eval_cov2_part_3() -> Result<(), crate::error::HclError> {
    let ctx = Context::new();
    let expr = Expression::BinaryOp(
        crate::ast::expr::BinaryOp::And,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::Bool(true, empty_span())),
        empty_span(),
    );
    let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));
    let expr = Expression::BinaryOp(
        crate::ast::expr::BinaryOp::Or,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::Bool(false, empty_span())),
        empty_span(),
    );
    let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
    assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(false)));
    let expr = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Tuple(
                vec![Expression::String("a".to_string(), empty_span())],
                empty_span(),
            )),
            operators: vec![crate::ast::expr::TraversalOperator::Index(
                Expression::Number(crate::number::Number::from_str("0")?, empty_span()),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
    assert_eq!(
        val,
        Value::new(Type::String, ValueData::String("a".to_string()))
    );
    let expr = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Object(
                vec![(
                    Expression::String("k".to_string(), empty_span()),
                    Expression::String("v".to_string(), empty_span()),
                )],
                empty_span(),
            )),
            operators: vec![crate::ast::expr::TraversalOperator::Index(
                Expression::String("k".to_string(), empty_span()),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
    assert_eq!(
        val,
        Value::new(Type::String, ValueData::String("v".to_string()))
    );
    let expr = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![Expression::Bool(true, empty_span())],
                empty_span(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
            cond_expr: Some(Box::new(Expression::Bool(true, empty_span()))),
            grouping: false,
        }),
        empty_span(),
    );
    let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
    assert_eq!(
        val.data.as_ref(),
        &ValueData::Array(vec![Value::new(Type::Bool, ValueData::Bool(true))])
    );
    Ok(())
}
#[test]
fn test_eval_extra_cov() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::new();
    ctx.set_variable(
        "str_var",
        Value::new(Type::String, ValueData::String("s".to_string())),
    );
    let evaluator = Evaluator::new(&ctx);
    let expr = Expression::Template(
        vec![crate::ast::expr::TemplatePart::Interpolation(
            Expression::Variable("str_var".to_string(), empty_span()),
            empty_span(),
        )],
        empty_span(),
    );
    let (val, _) = evaluator.evaluate(&expr)?;
    assert_eq!(
        val,
        Value::new(Type::String, ValueData::String("s".to_string()))
    );
    let evaluator = Evaluator::new(&ctx);
    let expr_and = Expression::BinaryOp(
        crate::ast::expr::BinaryOp::And,
        Box::new(Expression::Bool(true, empty_span())),
        Box::new(Expression::String("not bool".to_string(), empty_span())),
        empty_span(),
    );
    assert!(evaluator.evaluate(&expr_and).is_err());
    let evaluator = Evaluator::new(&ctx);
    let expr_or = Expression::BinaryOp(
        crate::ast::expr::BinaryOp::Or,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::String("not bool".to_string(), empty_span())),
        empty_span(),
    );
    assert!(evaluator.evaluate(&expr_or).is_err());
    ctx.set_variable("unk", Value::unknown(Type::Number));
    for op in [
        crate::ast::expr::BinaryOp::NotEq,
        crate::ast::expr::BinaryOp::Less,
        crate::ast::expr::BinaryOp::LessEq,
        crate::ast::expr::BinaryOp::Greater,
        crate::ast::expr::BinaryOp::GreaterEq,
    ] {
        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::BinaryOp(
            op,
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr)?;
        assert_eq!(*val.ty(), Type::Bool);
    }
    Ok(())
}
#[test]
fn test_eval_binary_right_operand_errors_covered() -> Result<(), crate::error::HclError> {
    let input = r"
        a = true && 123
        b = false || 123
    ";
    let ctx = Context::new();
    let body = crate::api::parse(input)?;
    let res_a = Evaluator::new(&ctx).evaluate(&body.attributes["a"].expr);
    let res_b = Evaluator::new(&ctx).evaluate(&body.attributes["b"].expr);
    assert!(res_a.is_err());
    assert!(res_b.is_err());
    Ok(())
}
#[test]
fn test_eval_template_directives_exhaustive() -> Result<(), crate::error::HclError> {
    use crate::ast::expr::{Directive, Expression, TemplatePart};
    let span = empty_span();
    let mut ctx = Context::new();
    ctx.set_variable("admin", Value::new(Type::Bool, ValueData::Bool(true)));
    let expr_if_true = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::If {
                cond: Expression::Variable("admin".to_string(), span.clone()),
                true_expr: vec![TemplatePart::Literal("Admin".to_string(), span.clone())],
                else_ifs: vec![],
                false_expr: Some(vec![TemplatePart::Literal(
                    "User".to_string(),
                    span.clone(),
                )]),
            },
            span.clone(),
        )],
        span.clone(),
    );
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_if_true)?;
    assert_eq!(
        v,
        Value::new(Type::String, ValueData::String("Admin".to_string()))
    );
    ctx.set_variable("admin", Value::new(Type::Bool, ValueData::Bool(false)));
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_if_true)?;
    assert_eq!(
        v,
        Value::new(Type::String, ValueData::String("User".to_string()))
    );
    let expr_no_else = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::If {
                cond: Expression::Bool(false, span.clone()),
                true_expr: vec![TemplatePart::Literal("Admin".to_string(), span.clone())],
                else_ifs: vec![],
                false_expr: None,
            },
            span.clone(),
        )],
        span.clone(),
    );
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_no_else)?;
    assert_eq!(
        v,
        Value::new(Type::String, ValueData::String(String::new()))
    );
    let expr_elif = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::If {
                cond: Expression::Bool(false, span.clone()),
                true_expr: vec![TemplatePart::Literal("A".to_string(), span.clone())],
                else_ifs: vec![
                    (
                        Expression::Bool(false, span.clone()),
                        vec![TemplatePart::Literal("B".to_string(), span.clone())],
                    ),
                    (
                        Expression::Bool(true, span.clone()),
                        vec![TemplatePart::Literal("C".to_string(), span.clone())],
                    ),
                ],
                false_expr: Some(vec![TemplatePart::Literal("D".to_string(), span.clone())]),
            },
            span.clone(),
        )],
        span,
    );
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_elif)?;
    assert_eq!(
        v,
        Value::new(Type::String, ValueData::String("C".to_string()))
    );
    Ok(())
}

#[test]
fn test_eval_template_directives_if_errors_and_for() -> Result<(), crate::error::HclError> {
    use crate::ast::expr::{Directive, Expression, TemplatePart};
    let span = empty_span();
    let mut ctx = Context::new();
    let expr_unk_cond = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::If {
                cond: Expression::Variable("unk".to_string(), span.clone()),
                true_expr: vec![TemplatePart::Literal("A".to_string(), span.clone())],
                else_ifs: vec![],
                false_expr: None,
            },
            span.clone(),
        )],
        span.clone(),
    );
    ctx.set_variable("unk", Value::unknown(Type::Bool));
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_unk_cond)?;
    assert!(v.is_unknown());
    let expr_bad_cond = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::If {
                cond: Expression::Number(Number::from_str("123")?, span.clone()),
                true_expr: vec![],
                else_ifs: vec![],
                false_expr: None,
            },
            span.clone(),
        )],
        span.clone(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_bad_cond).is_err());
    let expr_elif_unk = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::If {
                cond: Expression::Bool(false, span.clone()),
                true_expr: vec![],
                else_ifs: vec![(
                    Expression::Variable("unk".to_string(), span.clone()),
                    vec![],
                )],
                false_expr: None,
            },
            span.clone(),
        )],
        span.clone(),
    );
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_elif_unk)?;
    assert!(v.is_unknown());
    let expr_elif_bad = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::If {
                cond: Expression::Bool(false, span.clone()),
                true_expr: vec![],
                else_ifs: vec![(
                    Expression::String("not bool".to_string(), span.clone()),
                    vec![],
                )],
                false_expr: None,
            },
            span.clone(),
        )],
        span.clone(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_elif_bad).is_err());
    let expr_for_arr = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::For {
                key_var: None,
                val_var: "item".to_string(),
                collection: Expression::Tuple(
                    vec![
                        Expression::String("a".to_string(), span.clone()),
                        Expression::String("b".to_string(), span.clone()),
                    ],
                    span.clone(),
                ),
                body: vec![
                    TemplatePart::Literal("[".to_string(), span.clone()),
                    TemplatePart::Interpolation(
                        Expression::Variable("item".to_string(), span.clone()),
                        span.clone(),
                    ),
                    TemplatePart::Literal("]".to_string(), span.clone()),
                ],
            },
            span.clone(),
        )],
        span,
    );
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_for_arr)?;
    assert_eq!(
        v,
        Value::new(Type::String, ValueData::String("[a][b]".to_string()))
    );
    Ok(())
}

#[test]
fn test_eval_template_directives_for_loops_and_errors() -> Result<(), crate::error::HclError> {
    use crate::ast::expr::{Directive, Expression, TemplatePart};
    let span = empty_span();
    let mut ctx = Context::new();
    let expr_for_key_val = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::For {
                key_var: Some("idx".to_string()),
                val_var: "item".to_string(),
                collection: Expression::Tuple(
                    vec![
                        Expression::String("x".to_string(), span.clone()),
                        Expression::String("y".to_string(), span.clone()),
                    ],
                    span.clone(),
                ),
                body: vec![
                    TemplatePart::Interpolation(
                        Expression::Variable("idx".to_string(), span.clone()),
                        span.clone(),
                    ),
                    TemplatePart::Literal(":".to_string(), span.clone()),
                    TemplatePart::Interpolation(
                        Expression::Variable("item".to_string(), span.clone()),
                        span.clone(),
                    ),
                    TemplatePart::Literal(";".to_string(), span.clone()),
                ],
            },
            span.clone(),
        )],
        span.clone(),
    );
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_for_key_val)?;
    assert_eq!(
        v,
        Value::new(Type::String, ValueData::String("0:x;1:y;".to_string()))
    );
    let mut set_vals = std::collections::BTreeSet::new();
    set_vals.insert(Value::new(
        Type::String,
        ValueData::String("set_item".to_string()),
    ));
    ctx.set_variable(
        "my_set",
        Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_vals)),
    );
    let expr_for_set = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::For {
                key_var: None,
                val_var: "item".to_string(),
                collection: Expression::Variable("my_set".to_string(), span.clone()),
                body: vec![TemplatePart::Interpolation(
                    Expression::Variable("item".to_string(), span.clone()),
                    span.clone(),
                )],
            },
            span.clone(),
        )],
        span,
    );
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_for_set)?;
    assert_eq!(
        v,
        Value::new(Type::String, ValueData::String("set_item".to_string()))
    );
    Ok(())
}

#[test]
fn test_eval_template_directives_for_object() -> Result<(), crate::error::HclError> {
    use crate::ast::expr::{Directive, Expression, TemplatePart};
    let span = empty_span();
    let mut ctx = Context::new();
    let mut obj_vals = BTreeMap::new();
    obj_vals.insert(
        "k1".to_string(),
        Value::new(Type::String, ValueData::String("v1".to_string())),
    );
    ctx.set_variable(
        "my_obj",
        Value::new(Type::object(BTreeMap::new()), ValueData::Object(obj_vals)),
    );
    let expr_for_obj = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::For {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Expression::Variable("my_obj".to_string(), span.clone()),
                body: vec![
                    TemplatePart::Interpolation(
                        Expression::Variable("k".to_string(), span.clone()),
                        span.clone(),
                    ),
                    TemplatePart::Literal("=".to_string(), span.clone()),
                    TemplatePart::Interpolation(
                        Expression::Variable("v".to_string(), span.clone()),
                        span.clone(),
                    ),
                ],
            },
            span.clone(),
        )],
        span,
    );
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_for_obj)?;
    assert_eq!(
        v,
        Value::new(Type::String, ValueData::String("k1=v1".to_string()))
    );
    Ok(())
}

#[test]
fn test_eval_template_directives_edge_cases() -> Result<(), crate::error::HclError> {
    use crate::ast::expr::{Directive, Expression, TemplatePart};
    let span = empty_span();
    let mut ctx = Context::new();
    ctx.set_variable("unk", Value::unknown(Type::Bool));
    let expr_for_unk = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::For {
                key_var: None,
                val_var: "x".to_string(),
                collection: Expression::Variable("unk".to_string(), span.clone()),
                body: vec![],
            },
            span.clone(),
        )],
        span.clone(),
    );
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_for_unk)?;
    assert!(v.is_unknown());
    let expr_for_null = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::For {
                key_var: None,
                val_var: "x".to_string(),
                collection: Expression::Null(span.clone()),
                body: vec![TemplatePart::Literal("never".to_string(), span.clone())],
            },
            span.clone(),
        )],
        span.clone(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_for_null).is_err());
    let expr_for_bad = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::For {
                key_var: None,
                val_var: "x".to_string(),
                collection: Expression::Number(Number::from_str("42")?, span.clone()),
                body: vec![],
            },
            span.clone(),
        )],
        span.clone(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_for_bad).is_err());
    let expr_strip = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::Strip {
                strip_left: true,
                strip_right: true,
            },
            span.clone(),
        )],
        span,
    );
    let (v, _) = Evaluator::new(&ctx).evaluate(&expr_strip)?;
    assert_eq!(
        v,
        Value::new(Type::String, ValueData::String(String::new()))
    );
    let input = r#"
        greeting = "Hello, %{ for name in names }${name}%{ if name == \"Alice\" } (boss)%{ endif }, %{ endfor }bye!"
    "#;
    let mut full_ctx = Context::new();
    full_ctx.set_variable(
        "names",
        Value::new(
            Type::Tuple(vec![]),
            ValueData::Array(vec![
                Value::new(Type::String, ValueData::String("Alice".to_string())),
                Value::new(Type::String, ValueData::String("Bob".to_string())),
            ]),
        ),
    );
    let body = crate::api::parse(input)?;
    let (val, _) = Evaluator::new(&full_ctx).evaluate(&body.attributes["greeting"].expr)?;
    assert_eq!(
        val,
        Value::new(
            Type::String,
            ValueData::String("Hello, Alice (boss), Bob, bye!".to_string())
        )
    );
    Ok(())
}
#[test]
fn test_mark_propagation_comprehensive() -> Result<(), crate::error::HclError> {
    use crate::types::ValueMark;
    let mut ctx = Context::with_stdlib();
    let sens_num =
        Value::new(Type::Number, ValueData::Number(10_i32.into())).mark(ValueMark::Sensitive);
    let sens_bool = Value::new(Type::Bool, ValueData::Bool(true)).mark(ValueMark::Sensitive);
    let sens_str =
        Value::new(Type::String, ValueData::String("vault".into())).mark(ValueMark::Sensitive);
    ctx.set_variable("sens_num", sens_num);
    ctx.set_variable("sens_bool", sens_bool);
    ctx.set_variable("sens_str", sens_str);
    let parse_expr = |s: &str| -> Result<Expression, crate::error::HclError> {
        let body = crate::api::parse(&format!("v = {s}"))?;
        Ok(body.attributes["v"].expr.clone())
    };
    let cases = [
        ("sens_num + 5", Type::Number),
        ("20 - sens_num", Type::Number),
        ("sens_num * 3", Type::Number),
        ("sens_num / 2", Type::Number),
        ("sens_num % 3", Type::Number),
    ];
    for (expr_str, expected_ty) in cases {
        let expr = parse_expr(expr_str)?;
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
        assert_eq!(val.ty(), &expected_ty);
        assert!(val.has_mark(&ValueMark::Sensitive),);
    }
    let div_zero = parse_expr("sens_num / 0")?;
    let mut sub_eval = Evaluator::new(&ctx);
    let val = sub_eval.eval_expr(&div_zero);
    assert!(val.is_unknown());
    assert!(val.has_mark(&ValueMark::Sensitive));
    assert!(sub_eval.diagnostics.errors().iter().any(|d| {
        d.summary
            .as_deref()
            .unwrap_or("")
            .contains("Division by zero")
    }));
    let mod_zero = parse_expr("sens_num % 0")?;
    let mut sub_eval2 = Evaluator::new(&ctx);
    let val = sub_eval2.eval_expr(&mod_zero);
    assert!(val.is_unknown());
    assert!(val.has_mark(&ValueMark::Sensitive));
    assert!(sub_eval2.diagnostics.errors().iter().any(|d| {
        d.summary
            .as_deref()
            .unwrap_or("")
            .contains("Division by zero in modulo")
    }));
    let cmp_cases = [
        "sens_num == 10",
        "sens_num != 5",
        "sens_num > 5",
        "sens_num >= 10",
        "sens_num < 20",
        "sens_num <= 10",
    ];
    for expr_str in cmp_cases {
        let expr = parse_expr(expr_str)?;
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
        assert_eq!(val.ty(), &Type::Bool);
        assert_eq!(val.data.as_ref(), &ValueData::Bool(true));
        assert!(val.has_mark(&ValueMark::Sensitive),);
    }
    let log_cases = [
        "sens_bool && true",
        "true && sens_bool",
        "sens_bool || false",
        "false || sens_bool",
    ];
    for expr_str in log_cases {
        let expr = parse_expr(expr_str)?;
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr)?;
        assert_eq!(val.data.as_ref(), &ValueData::Bool(true));
        assert!(val.has_mark(&ValueMark::Sensitive),);
    }
    let not_expr = parse_expr("!sens_bool")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&not_expr)?;
    assert_eq!(val.data.as_ref(), &ValueData::Bool(false));
    assert!(val.has_mark(&ValueMark::Sensitive));
    let neg_expr = parse_expr("-sens_num")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&neg_expr)?;
    assert!(val.has_mark(&ValueMark::Sensitive));
    let cond_expr = parse_expr("sens_bool ? \"yes\" : \"no\"")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&cond_expr)?;
    assert_eq!(val.data.as_ref(), &ValueData::String("yes".into()));
    assert!(val.has_mark(&ValueMark::Sensitive));
    Ok(())
}

#[test]
fn test_mark_propagation_traversals_and_templates() -> Result<(), crate::error::HclError> {
    use crate::types::ValueMark;
    let mut ctx = Context::with_stdlib();
    let sens_bool = Value::new(Type::Bool, ValueData::Bool(true)).mark(ValueMark::Sensitive);
    let sens_str =
        Value::new(Type::String, ValueData::String("vault".into())).mark(ValueMark::Sensitive);
    ctx.set_variable("sens_bool", sens_bool);
    ctx.set_variable("sens_str", sens_str);
    let parse_expr = |s: &str| -> Result<Expression, crate::error::HclError> {
        let body = crate::api::parse(&format!("v = {s}"))?;
        Ok(body.attributes["v"].expr.clone())
    };
    let mut obj_map = BTreeMap::new();
    obj_map.insert(
        "secret".to_string(),
        Value::new(Type::String, ValueData::String("pass123".into())),
    );
    let marked_obj =
        Value::new(Type::Dynamic, ValueData::Object(obj_map)).mark(ValueMark::Sensitive);
    ctx.set_variable("marked_obj", marked_obj);
    let t_attr = parse_expr("marked_obj.secret")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&t_attr)?;
    assert_eq!(val.data.as_ref(), &ValueData::String("pass123".into()));
    assert!(val.has_mark(&ValueMark::Sensitive));
    let t_idx = parse_expr("marked_obj[\"secret\"]")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&t_idx)?;
    assert_eq!(val.data.as_ref(), &ValueData::String("pass123".into()));
    assert!(val.has_mark(&ValueMark::Sensitive));
    let marked_arr = Value::new(
        Type::List(Box::new(Type::String)),
        ValueData::Array(vec![Value::new(
            Type::String,
            ValueData::String("item0".into()),
        )]),
    )
    .mark(ValueMark::Sensitive);
    ctx.set_variable("marked_arr", marked_arr);
    let t_arr_idx = parse_expr("marked_arr[0]")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&t_arr_idx)?;
    assert_eq!(val.data.as_ref(), &ValueData::String("item0".into()));
    assert!(val.has_mark(&ValueMark::Sensitive));
    let t_arr_legacy = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("marked_arr".to_string(), empty_span())),
            operators: vec![crate::ast::expr::TraversalOperator::LegacyIndex(
                0,
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let (val, _) = Evaluator::new(&ctx).evaluate(&t_arr_legacy)?;
    assert_eq!(val.data.as_ref(), &ValueData::String("item0".into()));
    assert!(val.has_mark(&ValueMark::Sensitive));
    Ok(())
}

#[test]
fn test_mark_propagation_templates_and_stdlib() -> Result<(), crate::error::HclError> {
    use crate::types::ValueMark;
    let mut ctx = Context::with_stdlib();
    let sens_bool = Value::new(Type::Bool, ValueData::Bool(true)).mark(ValueMark::Sensitive);
    let sens_str =
        Value::new(Type::String, ValueData::String("vault".into())).mark(ValueMark::Sensitive);
    let marked_arr = Value::new(
        Type::List(Box::new(Type::String)),
        ValueData::Array(vec![Value::new(
            Type::String,
            ValueData::String("item0".into()),
        )]),
    )
    .mark(ValueMark::Sensitive);
    ctx.set_variable("sens_bool", sens_bool);
    ctx.set_variable("sens_str", sens_str);
    ctx.set_variable("marked_arr", marked_arr);
    let parse_expr = |s: &str| -> Result<Expression, crate::error::HclError> {
        let body = crate::api::parse(&format!("v = {s}"))?;
        Ok(body.attributes["v"].expr.clone())
    };
    let tpl_expr = parse_expr(&["\"prefix-", "${", "sens_str", "}-suffix\""].concat())?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&tpl_expr)?;
    assert_eq!(
        val.data.as_ref(),
        &ValueData::String("prefix-vault-suffix".into())
    );
    assert!(val.has_mark(&ValueMark::Sensitive));
    let tpl_if = parse_expr(
        &[
            "\"result: ",
            "%{",
            " if sens_bool }visible",
            "%{",
            " endif }\"",
        ]
        .concat(),
    )?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&tpl_if)?;
    assert_eq!(
        val.data.as_ref(),
        &ValueData::String("result: visible".into())
    );
    assert!(val.has_mark(&ValueMark::Sensitive));
    let tpl_for = parse_expr(
        &[
            "\"items: ",
            "%{",
            " for x in marked_arr }",
            "${",
            "x}",
            "%{",
            " endfor }\"",
        ]
        .concat(),
    )?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&tpl_for)?;
    assert_eq!(val.data.as_ref(), &ValueData::String("items: item0".into()));
    assert!(val.has_mark(&ValueMark::Sensitive));
    let for_tuple = Expression::ForExpr(
        Box::new(crate::ast::expr::ForExpr {
            key_var: None,
            val_var: "x".to_string(),
            collection: Box::new(Expression::Variable("marked_arr".to_string(), empty_span())),
            key_expr: None,
            val_expr: Box::new(parse_expr("upper(x)")?),
            cond_expr: None,
            grouping: false,
        }),
        empty_span(),
    );
    let (val, _) = Evaluator::new(&ctx).evaluate(&for_tuple)?;
    assert!(val.has_mark(&ValueMark::Sensitive));
    let func_upper = parse_expr("upper(sens_str)")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&func_upper)?;
    assert_eq!(val.data.as_ref(), &ValueData::String("VAULT".into()));
    assert!(val.has_mark(&ValueMark::Sensitive));
    let is_sens = parse_expr("issensitive(sens_str)")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&is_sens)?;
    assert_eq!(val.data.as_ref(), &ValueData::Bool(true));
    assert!(!val.has_mark(&ValueMark::Sensitive));
    let is_not_sens = parse_expr("issensitive(\"plain\")")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&is_not_sens)?;
    assert_eq!(val.data.as_ref(), &ValueData::Bool(false));
    assert!(!val.has_mark(&ValueMark::Sensitive));
    let non_sens = parse_expr("nonsensitive(sens_str)")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&non_sens)?;
    assert_eq!(val.data.as_ref(), &ValueData::String("vault".into()));
    assert!(!val.has_mark(&ValueMark::Sensitive));
    let make_sens = parse_expr("sensitive(\"raw\")")?;
    let (val, _) = Evaluator::new(&ctx).evaluate(&make_sens)?;
    assert_eq!(val.data.as_ref(), &ValueData::String("raw".into()));
    assert!(val.has_mark(&ValueMark::Sensitive));
    Ok(())
}
#[test]
fn test_evaluator_coverage_exhaustive() {
    let mut ctx = Context::new();
    let expr_empty_expand = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "foo".into(),
            args: vec![],
            expand_final: true,
        }),
        empty_span(),
    );
    let res_empty = Evaluator::new(&ctx).eval_expr(&expr_empty_expand);
    assert!(res_empty.is_unknown());
    let sig = crate::eval::func::FunctionSignature::with_static_return_type(
        vec![crate::eval::func::FunctionParamSpec::new(
            "p1",
            Type::String,
        )],
        Type::Dynamic,
    )
    .with_variadic(crate::eval::func::FunctionParamSpec::new(
        "rest",
        Type::Number,
    ));
    ctx.set_function(
        "test_var",
        crate::eval::func::Function::new(
            "test_var",
            std::sync::Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
        )
        .with_signature(sig),
    );
    let expr_null_fixed = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "test_var".into(),
            args: vec![
                Expression::Null(empty_span()),
                Expression::Number(1.into(), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let mut evaluator_null_fixed = Evaluator::new(&ctx);
    let res_null_fixed = evaluator_null_fixed.eval_expr(&expr_null_fixed);
    assert!(res_null_fixed.is_unknown());
    assert!(evaluator_null_fixed.diagnostics.has_errors());
    let expr_null_variadic = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "test_var".into(),
            args: vec![
                Expression::String("a".to_string(), empty_span()),
                Expression::Null(empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let mut evaluator_null_variadic = Evaluator::new(&ctx);
    let res_null_variadic = evaluator_null_variadic.eval_expr(&expr_null_variadic);
    assert!(res_null_variadic.is_unknown());
    assert!(evaluator_null_variadic.diagnostics.has_errors());
    let expr_bad_fixed = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "test_var".into(),
            args: vec![
                Expression::Tuple(vec![], empty_span()),
                Expression::Number(1.into(), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let mut evaluator_bad_fixed = Evaluator::new(&ctx);
    let res_bad_fixed = evaluator_bad_fixed.eval_expr(&expr_bad_fixed);
    assert!(res_bad_fixed.is_unknown());
    assert!(evaluator_bad_fixed.diagnostics.has_errors());
}

#[test]
fn test_evaluator_coverage_variadic_and_coerce() {
    let mut ctx = Context::new();
    let sig = crate::eval::func::FunctionSignature::with_static_return_type(
        vec![crate::eval::func::FunctionParamSpec::new(
            "p1",
            Type::String,
        )],
        Type::Dynamic,
    )
    .with_variadic(crate::eval::func::FunctionParamSpec::new(
        "rest",
        Type::Number,
    ));
    ctx.set_function(
        "test_var",
        crate::eval::func::Function::new(
            "test_var",
            std::sync::Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
        )
        .with_signature(sig),
    );
    let expr_bad_variadic = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "test_var".into(),
            args: vec![
                Expression::String("a".to_string(), empty_span()),
                Expression::Tuple(vec![], empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let mut evaluator_bad_variadic = Evaluator::new(&ctx);
    let res_bad_variadic = evaluator_bad_variadic.eval_expr(&expr_bad_variadic);
    assert!(res_bad_variadic.is_unknown());
    assert!(evaluator_bad_variadic.diagnostics.has_errors());
    let expr_coerce_success = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "test_var".into(),
            args: vec![
                Expression::String("a".to_string(), empty_span()),
                Expression::String("42".to_string(), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let res_coerce_success = Evaluator::new(&ctx).eval_expr(&expr_coerce_success);
    assert_eq!(*res_coerce_success.data, ValueData::Bool(true));
}

#[test]
fn test_evaluator_coverage_nonsensitive_and_logical() {
    use crate::types::ValueMark;
    let ctx = Context::new();
    let mut ctx_std = Context::with_stdlib();
    let mut custom_marked = Value::new(Type::String, ValueData::String("secret".to_string()));
    custom_marked
        .marks
        .insert(ValueMark::Custom("tag".to_string()));
    custom_marked.marks.insert(ValueMark::Sensitive);
    ctx_std.set_variable("marked_val", custom_marked);
    let expr_non_sens = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "nonsensitive".into(),
            args: vec![Expression::Variable("marked_val".to_string(), empty_span())],
            expand_final: false,
        }),
        empty_span(),
    );
    let res_non_sens = Evaluator::new(&ctx_std).eval_expr(&expr_non_sens);
    assert!(!res_non_sens.marks.contains(&ValueMark::Sensitive));
    assert!(
        res_non_sens
            .marks
            .contains(&ValueMark::Custom("tag".to_string()))
    );
    let expr_and = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Number(1.into(), empty_span())),
        Box::new(Expression::Bool(true, empty_span())),
        empty_span(),
    );
    let res_and = Evaluator::new(&ctx).eval_expr(&expr_and);
    assert_eq!(*res_and.data, ValueData::Bool(true));
    let expr_or = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Number(1.into(), empty_span())),
        Box::new(Expression::Bool(false, empty_span())),
        empty_span(),
    );
    let res_or = Evaluator::new(&ctx).eval_expr(&expr_or);
    assert_eq!(*res_or.data, ValueData::Bool(false));
}

#[test]
fn test_evaluator_coverage_string_refinements() {
    use crate::types::refinement::Refinement;
    let mut ctx = Context::new();
    let mut ref_l = Refinement::not_null();
    ref_l.string_prefix = Some("pre_".to_string());
    ref_l.string_suffix = Some("_suf".to_string());
    ref_l.string_length_min = Some(10);
    ref_l.string_length_max = Some(20);
    let unk_left = Value::unknown_refined(Type::String, ref_l);
    let mut ref_r = Refinement::not_null();
    ref_r.string_prefix = Some("start_".to_string());
    ref_r.string_suffix = Some("_end".to_string());
    ref_r.string_length_min = Some(5);
    ref_r.string_length_max = Some(15);
    let unk_right = Value::unknown_refined(Type::String, ref_r);
    let known_mid = Value::new(Type::String, ValueData::String("middle".to_string()));
    ctx.set_variable("unk_left", unk_left);
    ctx.set_variable("unk_right", unk_right);
    ctx.set_variable("known_mid", known_mid);
    let expr_both_unknown = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("unk_left".to_string(), empty_span())),
        Box::new(Expression::Variable("unk_right".to_string(), empty_span())),
        empty_span(),
    );
    let res_both_unknown = Evaluator::new(&ctx).eval_expr(&expr_both_unknown);
    assert!(res_both_unknown.is_unknown());
    assert_eq!(
        res_both_unknown
            .refinement()
            .and_then(|r| r.string_prefix.clone())
            .as_deref(),
        Some("pre_")
    );
    assert_eq!(
        res_both_unknown
            .refinement()
            .and_then(|r| r.string_suffix.clone())
            .as_deref(),
        Some("_end")
    );
    assert_eq!(
        res_both_unknown
            .refinement()
            .and_then(|r| r.string_length_min),
        Some(15)
    );
    assert_eq!(
        res_both_unknown
            .refinement()
            .and_then(|r| r.string_length_max),
        Some(35)
    );
    let expr_known_left = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("known_mid".to_string(), empty_span())),
        Box::new(Expression::Variable("unk_right".to_string(), empty_span())),
        empty_span(),
    );
    let res_known_left = Evaluator::new(&ctx).eval_expr(&expr_known_left);
    assert!(res_known_left.is_unknown());
    assert_eq!(
        res_known_left
            .refinement()
            .and_then(|r| r.string_prefix.clone())
            .as_deref(),
        Some("middlestart_")
    );
    assert_eq!(
        res_known_left
            .refinement()
            .and_then(|r| r.string_suffix.clone())
            .as_deref(),
        Some("_end")
    );
    let expr_known_right = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("unk_left".to_string(), empty_span())),
        Box::new(Expression::Variable("known_mid".to_string(), empty_span())),
        empty_span(),
    );
    let res_known_right = Evaluator::new(&ctx).eval_expr(&expr_known_right);
    assert!(res_known_right.is_unknown());
    assert_eq!(
        res_known_right
            .refinement()
            .and_then(|r| r.string_prefix.clone())
            .as_deref(),
        Some("pre_")
    );
    assert_eq!(
        res_known_right
            .refinement()
            .and_then(|r| r.string_suffix.clone())
            .as_deref(),
        Some("_sufmiddle")
    );
}

#[test]
fn test_evaluator_coverage_string_refinements_bare_and_corrupt() {
    let mut ctx = Context::new();
    let known_mid = Value::new(Type::String, ValueData::String("middle".to_string()));
    ctx.set_variable("known_mid", known_mid);
    let unk_bare = Value::unknown(Type::String);
    ctx.set_variable("unk_bare", unk_bare);
    let expr_known_bare = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("known_mid".to_string(), empty_span())),
        Box::new(Expression::Variable("unk_bare".to_string(), empty_span())),
        empty_span(),
    );
    let res_known_bare = Evaluator::new(&ctx).eval_expr(&expr_known_bare);
    assert!(res_known_bare.is_unknown());
    assert_eq!(
        res_known_bare
            .refinement()
            .and_then(|r| r.string_prefix.clone())
            .as_deref(),
        Some("middle")
    );
    let expr_bare_known = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("unk_bare".to_string(), empty_span())),
        Box::new(Expression::Variable("known_mid".to_string(), empty_span())),
        empty_span(),
    );
    let res_bare_known = Evaluator::new(&ctx).eval_expr(&expr_bare_known);
    assert!(res_bare_known.is_unknown());
    assert_eq!(
        res_bare_known
            .refinement()
            .and_then(|r| r.string_suffix.clone())
            .as_deref(),
        Some("middle")
    );
    let corrupted_str = Value::new(Type::String, ValueData::Bool(true));
    ctx.set_variable("corrupted_str", corrupted_str);
    let expr_corrupt_left = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable(
            "corrupted_str".to_string(),
            empty_span(),
        )),
        Box::new(Expression::Variable("unk_bare".to_string(), empty_span())),
        empty_span(),
    );
    let res_corrupt_left = Evaluator::new(&ctx).eval_expr(&expr_corrupt_left);
    assert!(res_corrupt_left.is_unknown());
    let expr_corrupt_right = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("unk_bare".to_string(), empty_span())),
        Box::new(Expression::Variable(
            "corrupted_str".to_string(),
            empty_span(),
        )),
        empty_span(),
    );
    let res_corrupt_right = Evaluator::new(&ctx).eval_expr(&expr_corrupt_right);
    assert!(res_corrupt_right.is_unknown());
}

#[test]
fn test_evaluator_coverage_string_add_and_splats() {
    let mut ctx = Context::new();
    ctx.set_variable("unk_bare", Value::unknown(Type::String));
    let expr_str_concat = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::String("foo_".to_string(), empty_span())),
        Box::new(Expression::String("bar".to_string(), empty_span())),
        empty_span(),
    );
    let res_str_concat = Evaluator::new(&ctx).eval_expr(&expr_str_concat);
    assert_eq!(
        *res_str_concat.data,
        ValueData::String("foo_bar".to_string())
    );
    let expr_num_str = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::String("10".to_string(), empty_span())),
        Box::new(Expression::String("foo".to_string(), empty_span())),
        empty_span(),
    );
    let res_num_str = Evaluator::new(&ctx).eval_expr(&expr_num_str);
    assert_eq!(*res_num_str.data, ValueData::String("10foo".to_string()));
    let expr_num_num = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::String("10".to_string(), empty_span())),
        Box::new(Expression::String("20".to_string(), empty_span())),
        empty_span(),
    );
    let res_num_num = Evaluator::new(&ctx).eval_expr(&expr_num_num);
    assert_eq!(*res_num_num.data, ValueData::Number(30.into()));
    let bad_str_a = Value::new(Type::String, ValueData::Bool(true));
    let bad_str_b = Value::new(Type::String, ValueData::String("ok".to_string()));
    ctx.set_variable("bad_a", bad_str_a);
    ctx.set_variable("bad_b", bad_str_b);
    let expr_bad_str = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("bad_a".to_string(), empty_span())),
        Box::new(Expression::Variable("bad_b".to_string(), empty_span())),
        empty_span(),
    );
    let mut eval_bad_s = Evaluator::new(&ctx);
    let res_bad_s = eval_bad_s.eval_expr(&expr_bad_str);
    assert!(res_bad_s.is_unknown());
    assert!(eval_bad_s.diagnostics.has_errors());
    let bad_num_val = Value::new(Type::Number, ValueData::Bool(true));
    ctx.set_variable("bad_num_val", bad_num_val);
    let expr_bad_num = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable(
            "bad_num_val".to_string(),
            empty_span(),
        )),
        Box::new(Expression::Number(1.into(), empty_span())),
        empty_span(),
    );
    let mut eval_bad_n = Evaluator::new(&ctx);
    let res_bad_n = eval_bad_n.eval_expr(&expr_bad_num);
    assert!(res_bad_n.is_unknown());
    assert!(eval_bad_n.diagnostics.has_errors());
}

#[test]
fn test_evaluator_coverage_attr_splats() {
    let mut ctx = Context::new();
    ctx.set_variable("unk_bare", Value::unknown(Type::String));
    let base_bool_val = Value::new(Type::Bool, ValueData::Bool(true));
    let mut eval_step_attr = Evaluator::new(&ctx);
    let step_attr_res = eval_step_attr
        .eval_single_step(&base_bool_val, &TraversalOperator::AttrSplat(empty_span()));
    assert!(step_attr_res.is_unknown());
    assert!(eval_step_attr.diagnostics.has_errors());
    let mut eval_step_full = Evaluator::new(&ctx);
    let step_full_res = eval_step_full
        .eval_single_step(&base_bool_val, &TraversalOperator::FullSplat(empty_span()));
    assert!(step_full_res.is_unknown());
    assert!(eval_step_full.diagnostics.has_errors());
    let t_null_splat = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Null(empty_span())),
        operators: vec![
            TraversalOperator::AttrSplat(empty_span()),
            TraversalOperator::GetAttr("a".to_string(), empty_span()),
            TraversalOperator::Index(Expression::Number(0.into(), empty_span()), empty_span()),
        ],
    };
    let mut eval_null_splat = Evaluator::new(&ctx);
    let res_null_splat = eval_null_splat.eval_traversal(&t_null_splat, empty_span());
    assert!(res_null_splat.is_unknown());
    let mut set_map = BTreeSet::new();
    let mut map_item = BTreeMap::new();
    map_item.insert(
        "name".to_string(),
        Value::new(Type::String, ValueData::String("set_item".to_string())),
    );
    set_map.insert(Value::new(Type::Dynamic, ValueData::Object(map_item)));
    ctx.set_variable(
        "my_set_obj",
        Value::new(Type::Set(Box::new(Type::Dynamic)), ValueData::Set(set_map)),
    );
    let t_set_traversal = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("my_set_obj".to_string(), empty_span())),
        operators: vec![
            TraversalOperator::AttrSplat(empty_span()),
            TraversalOperator::GetAttr("name".to_string(), empty_span()),
        ],
    };
    let res_set_splat = Evaluator::new(&ctx).eval_traversal(&t_set_traversal, empty_span());
    assert_eq!(
        *res_set_splat.data,
        ValueData::Array(vec![Value::new(
            Type::String,
            ValueData::String("set_item".to_string())
        )])
    );
    let t_arr_idx = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Tuple(
            vec![Expression::Tuple(
                vec![Expression::Number(42.into(), empty_span())],
                empty_span(),
            )],
            empty_span(),
        )),
        operators: vec![
            TraversalOperator::AttrSplat(empty_span()),
            TraversalOperator::Index(Expression::Number(0.into(), empty_span()), empty_span()),
        ],
    };
    let res_arr_idx = Evaluator::new(&ctx).eval_traversal(&t_arr_idx, empty_span());
    assert_eq!(
        *res_arr_idx.data,
        ValueData::Array(vec![Value::new(Type::Number, ValueData::Number(42.into()))])
    );
    let t_unk_attr = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("unk_bare".to_string(), empty_span())),
        operators: vec![TraversalOperator::AttrSplat(empty_span())],
    };
    let res_unk_attr = Evaluator::new(&ctx).eval_traversal(&t_unk_attr, empty_span());
    assert!(res_unk_attr.is_unknown());
}

#[test]
fn test_evaluator_coverage_full_splats() {
    let mut ctx = Context::new();
    let t_null_full = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Null(empty_span())),
        operators: vec![
            TraversalOperator::FullSplat(empty_span()),
            TraversalOperator::FullSplat(empty_span()),
        ],
    };
    let res_null_full = Evaluator::new(&ctx).eval_traversal(&t_null_full, empty_span());
    assert_eq!(*res_null_full.data, ValueData::Array(vec![]));
    let t_null_full_chain = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Null(empty_span())),
        operators: vec![
            TraversalOperator::FullSplat(empty_span()),
            TraversalOperator::GetAttr("foo".to_string(), empty_span()),
            TraversalOperator::FullSplat(empty_span()),
        ],
    };
    let res_null_chain = Evaluator::new(&ctx).eval_traversal(&t_null_full_chain, empty_span());
    assert_eq!(*res_null_chain.data, ValueData::Array(vec![]));
    let mut set_raw = BTreeSet::new();
    set_raw.insert(Value::new(
        Type::String,
        ValueData::String("item".to_string()),
    ));
    ctx.set_variable(
        "simple_set",
        Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_raw)),
    );
    let t_full_set = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("simple_set".to_string(), empty_span())),
        operators: vec![TraversalOperator::FullSplat(empty_span())],
    };
    let res_full_set = Evaluator::new(&ctx).eval_traversal(&t_full_set, empty_span());
    assert_eq!(
        *res_full_set.data,
        ValueData::Array(vec![Value::new(
            Type::String,
            ValueData::String("item".to_string())
        )])
    );
    let t_splat_splat = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Tuple(
            vec![Expression::Tuple(
                vec![Expression::String("nested".to_string(), empty_span())],
                empty_span(),
            )],
            empty_span(),
        )),
        operators: vec![
            TraversalOperator::FullSplat(empty_span()),
            TraversalOperator::FullSplat(empty_span()),
        ],
    };
    let res_splat_splat = Evaluator::new(&ctx).eval_traversal(&t_splat_splat, empty_span());
    assert_eq!(
        *res_splat_splat.data,
        ValueData::Array(vec![Value::new(
            Type::Tuple(vec![Type::String]),
            ValueData::Array(vec![Value::new(
                Type::String,
                ValueData::String("nested".to_string())
            )])
        )])
    );
}

#[test]
fn test_evaluator_coverage_empty_splat_and_template_err() {
    let ctx = Context::new();
    let t_empty_full = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Tuple(vec![], empty_span())),
        operators: vec![TraversalOperator::FullSplat(empty_span())],
    };
    let res_empty_full = Evaluator::new(&ctx).eval_traversal(&t_empty_full, empty_span());
    assert_eq!(*res_empty_full.data, ValueData::Array(vec![]));
    let expr_template_err = Expression::Template(
        vec![crate::ast::expr::TemplatePart::Directive(
            crate::ast::expr::Directive::For {
                key_var: None,
                val_var: "item".to_string(),
                collection: Expression::Tuple(
                    vec![Expression::Number(1.into(), empty_span())],
                    empty_span(),
                ),
                body: vec![crate::ast::expr::TemplatePart::Interpolation(
                    Expression::BinaryOp(
                        BinaryOp::Div,
                        Box::new(Expression::Number(1.into(), empty_span())),
                        Box::new(Expression::Number(0.into(), empty_span())),
                        empty_span(),
                    ),
                    empty_span(),
                )],
            },
            empty_span(),
        )],
        empty_span(),
    );
    let mut evaluator_template_err = Evaluator::new(&ctx);
    let _ = evaluator_template_err.eval_expr(&expr_template_err);
    assert!(evaluator_template_err.diagnostics.has_errors());
}
#[test]
fn test_func_value_return_type_coercion() {
    use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
    use std::sync::Arc;
    let mut ctx = Context::new();
    let sig = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("arg", Type::String)],
        Type::Dynamic,
    )
    .with_value_return_type(Arc::new(|_args| Ok(Type::Number)));
    let func =
        Function::new("return_coerced", Arc::new(|args| Ok(args[0].clone()))).with_signature(sig);
    ctx.set_function("return_coerced", func);
    let expr = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "return_coerced".into(),
            args: vec![Expression::String("42".to_string(), empty_span())],
            expand_final: false,
        }),
        empty_span(),
    );
    let mut evaluator = Evaluator::new(&ctx);
    let val = evaluator.eval_expr(&expr);
    assert_eq!(val.ty(), &Type::Number);
    assert_eq!(val.to_string(), "42");
}
#[test]
fn test_evaluator_subexpression_callouts_on_error() -> Result<(), crate::error::HclError> {
    use crate::types::ValueMark;
    let mut ctx = Context::new();
    ctx.set_variable(
        "count",
        Value::new(Type::Number, ValueData::Number(Number::from(10))),
    );
    ctx.set_variable(
        "secret_token",
        Value::new(Type::String, ValueData::String("token123".into())).mark(ValueMark::Sensitive),
    );
    let expr = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("count".to_string(), empty_span())),
        Box::new(Expression::Variable(
            "secret_token".to_string(),
            empty_span(),
        )),
        empty_span(),
    );
    let evaluator = Evaluator::new(&ctx);
    let diags = evaluator
        .evaluate(&expr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(diags.has_errors());
    let diag = &diags.errors()[0];
    assert_ne!(diag.eval_callouts.len(), 0);
    let count_callout = diag
        .eval_callouts
        .iter()
        .find(|c| c.expression_text == "var.count")
        .ok_or_else(|| crate::error::HclError::Eval("missing count callout".to_string()))?;
    assert_eq!(count_callout.evaluated_value, "10");
    let secret_callout = diag
        .eval_callouts
        .iter()
        .find(|c| c.expression_text == "var.secret_token")
        .ok_or_else(|| crate::error::HclError::Eval("missing secret_token callout".to_string()))?;
    assert_eq!(secret_callout.evaluated_value, "(sensitive value)");
    Ok(())
}
#[test]
fn test_variable_and_attribute_typo_suggestions() -> Result<(), crate::error::HclError> {
    let mut parent_ctx = Context::new();
    parent_ctx.set_variable(
        "instance_type",
        Value::new(Type::String, ValueData::String("t3.micro".into())),
    );
    let mut child_ctx = Context::new_child(&parent_ctx);
    child_ctx.set_variable(
        "user_count",
        Value::new(Type::Number, ValueData::Number(Number::from(5))),
    );
    let mut map = std::collections::BTreeMap::new();
    map.insert(
        "hostname".to_string(),
        Value::new(Type::String, ValueData::String("localhost".into())),
    );
    map.insert(
        "port_number".to_string(),
        Value::new(Type::Number, ValueData::Number(Number::from(8080))),
    );
    child_ctx.set_variable(
        "server",
        Value::new(
            Type::object(std::collections::BTreeMap::new()),
            ValueData::Object(map),
        ),
    );
    let expr_var = Expression::Variable("user_cont".to_string(), empty_span());
    let err_var = Evaluator::new(&child_ctx)
        .evaluate(&expr_var)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    let diag_var = &err_var.errors()[0];
    assert_eq!(
        diag_var.detail.as_deref(),
        Some("Did you mean var.user_count?")
    );
    let expr_parent = Expression::Variable("var.instance_typ".to_string(), empty_span());
    let err_parent = Evaluator::new(&child_ctx)
        .evaluate(&expr_parent)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    let diag_parent = &err_parent.errors()[0];
    assert_eq!(
        diag_parent.detail.as_deref(),
        Some("Did you mean var.instance_type?")
    );
    let expr_attr = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("server".to_string(), empty_span())),
            operators: vec![TraversalOperator::GetAttr(
                "hostnam".to_string(),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let err_attr = Evaluator::new(&child_ctx)
        .evaluate(&expr_attr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    let diag_attr = &err_attr.errors()[0];
    assert_eq!(diag_attr.detail.as_deref(), Some("Did you mean .hostname?"));
    let expr_idx = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("server".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::String("port_numbr".to_string(), empty_span()),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let err_idx = Evaluator::new(&child_ctx)
        .evaluate(&expr_idx)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    let diag_idx = &err_idx.errors()[0];
    assert_eq!(
        diag_idx.detail.as_deref(),
        Some("Did you mean .port_number?")
    );
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Matrix2x2(i64, i64, i64, i64);

fn add_matrix2x2_access_ops(ops: crate::types::ty::CapsuleOps) -> crate::types::ty::CapsuleOps {
    use std::sync::Arc;
    ops.with_neg(Arc::new(|a| {
        let m = a
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        Ok(Box::new(Matrix2x2(-m.0, -m.1, -m.2, -m.3)))
    }))
    .with_cmp(Arc::new(|a, b| {
        let Matrix2x2(a1, b1, c1, d1) = *a
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        let Matrix2x2(a2, b2, c2, d2) = *b
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        let det1 = a1 * d1 - b1 * c1;
        let det2 = a2 * d2 - b2 * c2;
        Ok(det1.cmp(&det2))
    }))
    .with_attr_get(Arc::new(|a, name| {
        let m = a
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        match name {
            "a" => Ok(Value::new(Type::Number, ValueData::Number(m.0.into()))),
            "b" => Ok(Value::new(Type::Number, ValueData::Number(m.1.into()))),
            "c" => Ok(Value::new(Type::Number, ValueData::Number(m.2.into()))),
            "d" => Ok(Value::new(Type::Number, ValueData::Number(m.3.into()))),
            _ => Err(crate::error::HclError::Eval(format!(
                "unknown matrix field '{name}'"
            ))),
        }
    }))
    .with_index_get(Arc::new(|a, idx| {
        let m = a
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        if let ValueData::Number(n) = &*idx.data {
            match n.0.to_usize() {
                Some(0) => Ok(Value::new(Type::Number, ValueData::Number(m.0.into()))),
                Some(1) => Ok(Value::new(Type::Number, ValueData::Number(m.1.into()))),
                Some(2) => Ok(Value::new(Type::Number, ValueData::Number(m.2.into()))),
                Some(3) => Ok(Value::new(Type::Number, ValueData::Number(m.3.into()))),
                _ => Err(crate::error::HclError::Eval(
                    "matrix index out of bounds".to_string(),
                )),
            }
        } else {
            Err(crate::error::HclError::Eval(
                "matrix index must be an integer".to_string(),
            ))
        }
    }))
}

fn build_matrix2x2_ops() -> std::sync::Arc<crate::types::ty::CapsuleOps> {
    use crate::types::ty::CapsuleOps;
    use std::sync::Arc;
    let base = CapsuleOps::new(
        "matrix2x2",
        Arc::new(|a, b| {
            a.downcast_ref::<Matrix2x2>()
                .zip(b.downcast_ref::<Matrix2x2>())
                .is_some_and(|(m1, m2)| m1 == m2)
        }),
        Arc::new(crate::eval::evaluator::tests::dummy_hash),
    )
    .with_add(Arc::new(|a, b| {
        let m1 = a
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        let m2 = b
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        Ok(Box::new(Matrix2x2(
            m1.0 + m2.0,
            m1.1 + m2.1,
            m1.2 + m2.2,
            m1.3 + m2.3,
        )))
    }))
    .with_sub(Arc::new(|a, b| {
        let m1 = a
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        let m2 = b
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        Ok(Box::new(Matrix2x2(
            m1.0 - m2.0,
            m1.1 - m2.1,
            m1.2 - m2.2,
            m1.3 - m2.3,
        )))
    }))
    .with_mul(Arc::new(|a, b| {
        let Matrix2x2(a1, b1, c1, d1) = *a
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        let Matrix2x2(a2, b2, c2, d2) = *b
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        Ok(Box::new(Matrix2x2(
            a1 * a2 + b1 * c2,
            a1 * b2 + b1 * d2,
            c1 * a2 + d1 * c2,
            c1 * b2 + d1 * d2,
        )))
    }))
    .with_div(Arc::new(|a, b| {
        let m1 = a
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        let m2 = b
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        if m2.0 == 0 || m2.3 == 0 {
            return Err(crate::error::HclError::Eval(
                "matrix division by singular matrix".to_string(),
            ));
        }
        Ok(Box::new(Matrix2x2(
            m1.0 / m2.0,
            m1.1 / m2.1,
            m1.2 / m2.2,
            m1.3 / m2.3,
        )))
    }))
    .with_modulo(Arc::new(|a, b| {
        let m1 = a
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        let m2 = b
            .downcast_ref::<Matrix2x2>()
            .ok_or_else(|| crate::error::HclError::Type("expected matrix".to_string()))?;
        if m2.0 == 0 {
            return Err(crate::error::HclError::Eval(
                "matrix modulo by zero".to_string(),
            ));
        }
        Ok(Box::new(Matrix2x2(
            m1.0 % m2.0,
            m1.1 % m2.1,
            m1.2 % m2.2,
            m1.3 % m2.3,
        )))
    }));
    Arc::new(add_matrix2x2_access_ops(base))
}

#[test]
fn test_capsule_operator_overloading_and_hooks() -> Result<(), crate::error::HclError> {
    let ops = build_matrix2x2_ops();
    let m1_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(1, 2, 3, 4));
    let m2_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(5, 6, 7, 8));
    let mut ctx = Context::new();
    ctx.set_variable("m1", m1_val);
    ctx.set_variable("m2", m2_val);
    let expr_add = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("m2".to_string(), empty_span())),
        empty_span(),
    );
    let (res_add, _) = Evaluator::new(&ctx).evaluate(&expr_add)?;
    assert_eq!(
        res_add.downcast_ref::<Matrix2x2>(),
        Some(&Matrix2x2(6, 8, 10, 12))
    );
    let expr_sub = Expression::BinaryOp(
        BinaryOp::Sub,
        Box::new(Expression::Variable("m2".to_string(), empty_span())),
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        empty_span(),
    );
    let (res_sub, _) = Evaluator::new(&ctx).evaluate(&expr_sub)?;
    assert_eq!(
        res_sub.downcast_ref::<Matrix2x2>(),
        Some(&Matrix2x2(4, 4, 4, 4))
    );
    let expr_mul = Expression::BinaryOp(
        BinaryOp::Mul,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("m2".to_string(), empty_span())),
        empty_span(),
    );
    let (res_mul, _) = Evaluator::new(&ctx).evaluate(&expr_mul)?;
    assert_eq!(
        res_mul.downcast_ref::<Matrix2x2>(),
        Some(&Matrix2x2(19, 22, 43, 50))
    );
    let expr_div = Expression::BinaryOp(
        BinaryOp::Div,
        Box::new(Expression::Variable("m2".to_string(), empty_span())),
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        empty_span(),
    );
    let (res_div, _) = Evaluator::new(&ctx).evaluate(&expr_div)?;
    assert_eq!(
        res_div.downcast_ref::<Matrix2x2>(),
        Some(&Matrix2x2(5, 3, 2, 2))
    );
    let expr_mod = Expression::BinaryOp(
        BinaryOp::Mod,
        Box::new(Expression::Variable("m2".to_string(), empty_span())),
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        empty_span(),
    );
    let (res_mod, _) = Evaluator::new(&ctx).evaluate(&expr_mod)?;
    assert_eq!(
        res_mod.downcast_ref::<Matrix2x2>(),
        Some(&Matrix2x2(0, 0, 1, 0))
    );
    let expr_neg = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        empty_span(),
    );
    let (res_neg, _) = Evaluator::new(&ctx).evaluate(&expr_neg)?;
    assert_eq!(
        res_neg.downcast_ref::<Matrix2x2>(),
        Some(&Matrix2x2(-1, -2, -3, -4))
    );
    let m3_val = Value::capsule_with_ops("matrix2x2", ops, Matrix2x2(1, 0, 0, 1));
    ctx.set_variable("m3", m3_val);
    let expr_lt = Expression::BinaryOp(
        BinaryOp::Less,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("m3".to_string(), empty_span())),
        empty_span(),
    );
    let (res_lt, _) = Evaluator::new(&ctx).evaluate(&expr_lt)?;
    assert_eq!(*res_lt.data, ValueData::Bool(true));
    Ok(())
}

#[test]
fn test_capsule_operator_comparisons_and_traversals() -> Result<(), crate::error::HclError> {
    let ops = build_matrix2x2_ops();
    let m1_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(1, 2, 3, 4));
    let m2_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(5, 6, 7, 8));
    let m3_val = Value::capsule_with_ops("matrix2x2", ops, Matrix2x2(1, 0, 0, 1));
    let mut ctx = Context::new();
    ctx.set_variable("m1", m1_val);
    ctx.set_variable("m2", m2_val);
    ctx.set_variable("m3", m3_val);
    let expr_lte = Expression::BinaryOp(
        BinaryOp::LessEq,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("m2".to_string(), empty_span())),
        empty_span(),
    );
    let (res_lte, _) = Evaluator::new(&ctx).evaluate(&expr_lte)?;
    assert_eq!(*res_lte.data, ValueData::Bool(true));
    let expr_greater = Expression::BinaryOp(
        BinaryOp::Greater,
        Box::new(Expression::Variable("m3".to_string(), empty_span())),
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        empty_span(),
    );
    let (res_greater, _) = Evaluator::new(&ctx).evaluate(&expr_greater)?;
    assert_eq!(*res_greater.data, ValueData::Bool(true));
    let expr_greatereq = Expression::BinaryOp(
        BinaryOp::GreaterEq,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("m2".to_string(), empty_span())),
        empty_span(),
    );
    let (res_greatereq, _) = Evaluator::new(&ctx).evaluate(&expr_greatereq)?;
    assert_eq!(*res_greatereq.data, ValueData::Bool(true));
    let expr_eq = Expression::BinaryOp(
        BinaryOp::Eq,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        empty_span(),
    );
    let (res_eq, _) = Evaluator::new(&ctx).evaluate(&expr_eq)?;
    assert_eq!(*res_eq.data, ValueData::Bool(true));
    let expr_noteq = Expression::BinaryOp(
        BinaryOp::NotEq,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("m2".to_string(), empty_span())),
        empty_span(),
    );
    let (res_noteq, _) = Evaluator::new(&ctx).evaluate(&expr_noteq)?;
    assert_eq!(*res_noteq.data, ValueData::Bool(true));
    Ok(())
}

#[test]
fn test_capsule_operator_traversals() -> Result<(), crate::error::HclError> {
    let ops = build_matrix2x2_ops();
    let m1_val = Value::capsule_with_ops("matrix2x2", ops, Matrix2x2(1, 2, 3, 4));
    let mut ctx = Context::new();
    ctx.set_variable("m1", m1_val);
    let expr_attr_a = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
            operators: vec![TraversalOperator::GetAttr("a".to_string(), empty_span())],
        }),
        empty_span(),
    );
    let (res_a, _) = Evaluator::new(&ctx).evaluate(&expr_attr_a)?;
    assert_eq!(*res_a.data, ValueData::Number(1.into()));
    let expr_idx_2 = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::Number(2.into(), empty_span()),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let (res_idx, _) = Evaluator::new(&ctx).evaluate(&expr_idx_2)?;
    assert_eq!(*res_idx.data, ValueData::Number(3.into()));
    let expr_legacy_idx = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
            operators: vec![TraversalOperator::LegacyIndex(3, empty_span())],
        }),
        empty_span(),
    );
    let (res_legacy, _) = Evaluator::new(&ctx).evaluate(&expr_legacy_idx)?;
    assert_eq!(*res_legacy.data, ValueData::Number(4.into()));
    for (attr_name, expected_num) in [("b", 2), ("c", 3), ("d", 4)] {
        let expr_attr = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
                operators: vec![TraversalOperator::GetAttr(
                    attr_name.to_string(),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let (res, _) = Evaluator::new(&ctx).evaluate(&expr_attr)?;
        assert_eq!(*res.data, ValueData::Number(expected_num.into()));
    }
    for (idx_val, expected_num) in [(0, 1), (1, 2)] {
        let expr_idx = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
                operators: vec![TraversalOperator::Index(
                    Expression::Number(idx_val.into(), empty_span()),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let (res, _) = Evaluator::new(&ctx).evaluate(&expr_idx)?;
        assert_eq!(*res.data, ValueData::Number(expected_num.into()));
    }
    Ok(())
}

#[test]
fn test_capsule_operator_errors_part_1() -> Result<(), crate::error::HclError> {
    use crate::types::ty::CapsuleOps;
    use std::sync::Arc;
    let ops = build_matrix2x2_ops();
    let m1_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(1, 2, 3, 4));
    let m2_val = Value::capsule_with_ops("matrix2x2", ops, Matrix2x2(5, 6, 7, 8));
    let mut ctx = Context::new();
    ctx.set_variable("m1", m1_val.clone());
    ctx.set_variable("m2", m2_val);
    let expr_str_idx = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::String("not_an_int".to_string(), empty_span()),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let err_str_idx = Evaluator::new(&ctx)
        .evaluate(&expr_str_idx)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_str_idx.errors()[0]
            .error
            .to_string()
            .contains("matrix index must be an integer")
    );
    let expr_m_eq_num = Expression::BinaryOp(
        BinaryOp::Eq,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Number(42.into(), empty_span())),
        empty_span(),
    );
    let (res_m_eq_num, _) = Evaluator::new(&ctx).evaluate(&expr_m_eq_num)?;
    assert_eq!(*res_m_eq_num.data, ValueData::Bool(false));
    let other_capsule = Value::capsule_with_ops(
        "other_capsule",
        Arc::new(CapsuleOps::new(
            "other_capsule",
            Arc::new(crate::eval::evaluator::tests::dummy_eq),
            Arc::new(crate::eval::evaluator::tests::dummy_hash),
        )),
        999_i32,
    );
    ctx.set_variable("other", other_capsule);
    let expr_m_eq_other = Expression::BinaryOp(
        BinaryOp::Eq,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("other".to_string(), empty_span())),
        empty_span(),
    );
    let (res_other, _) = Evaluator::new(&ctx).evaluate(&expr_m_eq_other)?;
    assert_eq!(*res_other.data, ValueData::Bool(false));
    let corrupt_matrix = Value::new(m1_val.ty().clone(), ValueData::Capsule(Arc::new(999_i32)));
    assert_ne!(m1_val.unmark().0, corrupt_matrix.unmark().0);
    let mut eval = Evaluator::new(&ctx);
    let res_dir_bin = eval.evaluate_binary_op(
        BinaryOp::Add,
        &Expression::Variable("m1".to_string(), empty_span()),
        &Expression::Variable("m2".to_string(), empty_span()),
        empty_span(),
    );
    assert_eq!(
        res_dir_bin.downcast_ref::<Matrix2x2>(),
        Some(&Matrix2x2(6, 8, 10, 12))
    );
    let res_dir_un = eval.evaluate_unary_op(
        UnaryOp::Neg,
        &Expression::Variable("m1".to_string(), empty_span()),
        empty_span(),
    );
    assert_eq!(
        res_dir_un.downcast_ref::<Matrix2x2>(),
        Some(&Matrix2x2(-1, -2, -3, -4))
    );
    Ok(())
}

#[test]
fn test_capsule_operator_errors_part_1b() -> Result<(), crate::error::HclError> {
    let ops = build_matrix2x2_ops();
    let m1_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(1, 2, 3, 4));
    let mut ctx = Context::new();
    ctx.set_variable("m1", m1_val);
    let m_zero = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(0, 0, 0, 0));
    ctx.set_variable("m_zero", m_zero);
    let expr_bad_div = Expression::BinaryOp(
        BinaryOp::Div,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("m_zero".to_string(), empty_span())),
        empty_span(),
    );
    let err_div = Evaluator::new(&ctx)
        .evaluate(&expr_bad_div)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_div.errors()[0]
            .error
            .to_string()
            .contains("matrix division by singular matrix")
    );
    let m_div_d0 = Value::capsule_with_ops("matrix2x2", ops, Matrix2x2(1, 0, 0, 0));
    ctx.set_variable("m_div_d0", m_div_d0);
    let expr_div_d0 = Expression::BinaryOp(
        BinaryOp::Div,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("m_div_d0".to_string(), empty_span())),
        empty_span(),
    );
    let err_div_d0 = Evaluator::new(&ctx)
        .evaluate(&expr_div_d0)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_div_d0.errors()[0]
            .error
            .to_string()
            .contains("matrix division by singular matrix")
    );
    let expr_bad_mod = Expression::BinaryOp(
        BinaryOp::Mod,
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        Box::new(Expression::Variable("m_zero".to_string(), empty_span())),
        empty_span(),
    );
    let err_mod = Evaluator::new(&ctx)
        .evaluate(&expr_bad_mod)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_mod.errors()[0]
            .error
            .to_string()
            .contains("matrix modulo by zero")
    );
    Ok(())
}

#[test]
fn test_capsule_operator_errors_part_2() -> Result<(), crate::error::HclError> {
    let ops = build_matrix2x2_ops();
    let m1_val = Value::capsule_with_ops("matrix2x2", ops, Matrix2x2(1, 2, 3, 4));
    let mut ctx = Context::new();
    ctx.set_variable("m1", m1_val);
    let expr_bad_attr = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
            operators: vec![TraversalOperator::GetAttr(
                "unknown_field".to_string(),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let err_attr = Evaluator::new(&ctx)
        .evaluate(&expr_bad_attr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_attr.errors()[0]
            .error
            .to_string()
            .contains("unknown matrix field 'unknown_field'")
    );
    let expr_bad_idx = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::Number(99.into(), empty_span()),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let err_idx = Evaluator::new(&ctx)
        .evaluate(&expr_bad_idx)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_idx.errors()[0]
            .error
            .to_string()
            .contains("matrix index out of bounds")
    );
    Ok(())
}

#[test]
fn test_capsule_operator_errors_bare() -> Result<(), crate::error::HclError> {
    use crate::types::ty::CapsuleOps;
    use std::sync::Arc;
    let mut ctx = Context::new();
    let basic_ops = Arc::new(CapsuleOps::new(
        "bare_capsule",
        Arc::new(crate::eval::evaluator::tests::dummy_eq),
        Arc::new(crate::eval::evaluator::tests::dummy_hash),
    ));
    let bare = Value::capsule_with_ops("bare_capsule", basic_ops, 42_i32);
    ctx.set_variable("bare", bare);
    let expr_bare_neg = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Variable("bare".to_string(), empty_span())),
        empty_span(),
    );
    let err_bare_neg = Evaluator::new(&ctx)
        .evaluate(&expr_bare_neg)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_bare_neg.errors()[0]
            .error
            .to_string()
            .contains("does not support unary negation")
    );
    let expr_bare_attr = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("bare".to_string(), empty_span())),
            operators: vec![TraversalOperator::GetAttr(
                "field".to_string(),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let err_bare_attr = Evaluator::new(&ctx)
        .evaluate(&expr_bare_attr)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_bare_attr.errors()[0]
            .error
            .to_string()
            .contains("does not support attribute access")
    );
    let expr_bare_idx = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("bare".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::Number(0.into(), empty_span()),
                empty_span(),
            )],
        }),
        empty_span(),
    );
    let err_bare_idx = Evaluator::new(&ctx)
        .evaluate(&expr_bare_idx)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_bare_idx.errors()[0]
            .error
            .to_string()
            .contains("does not support indexing")
    );
    let expr_bare_legacy = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("bare".to_string(), empty_span())),
            operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
        }),
        empty_span(),
    );
    let err_bare_legacy = Evaluator::new(&ctx)
        .evaluate(&expr_bare_legacy)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_bare_legacy.errors()[0]
            .error
            .to_string()
            .contains("does not support indexing")
    );
    Ok(())
}

#[test]
fn test_capsule_operator_errors_part_3() -> Result<(), crate::error::HclError> {
    use std::sync::Arc;
    let ops = build_matrix2x2_ops();
    let m1_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(1, 2, 3, 4));
    let m2_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(5, 6, 7, 8));
    let mut ctx = Context::new();
    ctx.set_variable("m1", m1_val.clone());
    ctx.set_variable("m2", m2_val);
    let unk_cap = Value::unknown(m1_val.ty().clone());
    ctx.set_variable("unk_cap", unk_cap);
    let expr_unk_add = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("unk_cap".to_string(), empty_span())),
        Box::new(Expression::Variable("m1".to_string(), empty_span())),
        empty_span(),
    );
    let (res_unk_add, _) = Evaluator::new(&ctx).evaluate(&expr_unk_add)?;
    assert!(res_unk_add.is_unknown());
    assert_eq!(res_unk_add.ty(), m1_val.ty());
    let expr_unk_neg = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Variable("unk_cap".to_string(), empty_span())),
        empty_span(),
    );
    let (res_unk_neg, _) = Evaluator::new(&ctx).evaluate(&expr_unk_neg)?;
    assert!(res_unk_neg.is_unknown());
    assert_eq!(res_unk_neg.ty(), m1_val.ty());
    let m1_marked = m1_val.mark(crate::types::val::ValueMark::Sensitive);
    ctx.set_variable("m1_marked", m1_marked);
    let expr_marked_add = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("m1_marked".to_string(), empty_span())),
        Box::new(Expression::Variable("m2".to_string(), empty_span())),
        empty_span(),
    );
    let (res_marked_add, _) = Evaluator::new(&ctx).evaluate(&expr_marked_add)?;
    assert!(res_marked_add.has_mark(&crate::types::val::ValueMark::Sensitive));
    let invalid_any: Arc<dyn std::any::Any + Send + Sync> = Arc::new(12345_u32);
    let valid_any: Arc<dyn std::any::Any + Send + Sync> = Arc::new(Matrix2x2(1, 2, 3, 4));
    let add_fn = ops
        .add
        .as_ref()
        .ok_or_else(|| crate::error::HclError::Eval("missing add".to_string()))?;
    assert!(add_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
    assert!(add_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());
    let sub_fn = ops
        .sub
        .as_ref()
        .ok_or_else(|| crate::error::HclError::Eval("missing sub".to_string()))?;
    assert!(sub_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
    assert!(sub_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());
    let mul_fn = ops
        .mul
        .as_ref()
        .ok_or_else(|| crate::error::HclError::Eval("missing mul".to_string()))?;
    assert!(mul_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
    assert!(mul_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());
    let div_fn = ops
        .div
        .as_ref()
        .ok_or_else(|| crate::error::HclError::Eval("missing div".to_string()))?;
    assert!(div_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
    assert!(div_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());
    let mod_fn = ops
        .modulo
        .as_ref()
        .ok_or_else(|| crate::error::HclError::Eval("missing modulo".to_string()))?;
    assert!(mod_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
    assert!(mod_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());
    let neg_fn = ops
        .neg
        .as_ref()
        .ok_or_else(|| crate::error::HclError::Eval("missing neg".to_string()))?;
    assert!(neg_fn(invalid_any.as_ref()).is_err());
    let cmp_fn = ops
        .cmp
        .as_ref()
        .ok_or_else(|| crate::error::HclError::Eval("missing cmp".to_string()))?;
    assert!(cmp_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
    assert!(cmp_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());
    let attr_fn = ops
        .attr_get
        .as_ref()
        .ok_or_else(|| crate::error::HclError::Eval("missing attr_get".to_string()))?;
    assert!(attr_fn(invalid_any.as_ref(), "a").is_err());
    let idx_fn = ops
        .index_get
        .as_ref()
        .ok_or_else(|| crate::error::HclError::Eval("missing index_get".to_string()))?;
    assert!(
        idx_fn(
            invalid_any.as_ref(),
            &Value::new(Type::Number, ValueData::Number(0.into()))
        )
        .is_err()
    );
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FailableCapsule(i32);

fn build_failable_capsule_ops() -> std::sync::Arc<crate::types::ty::CapsuleOps> {
    use crate::types::ty::CapsuleOps;
    use std::sync::Arc;
    Arc::new(
        CapsuleOps::new(
            "failable_capsule",
            Arc::new(|a, b| {
                a.downcast_ref::<FailableCapsule>()
                    .zip(b.downcast_ref::<FailableCapsule>())
                    .is_some_and(|(x, y)| x == y)
            }),
            Arc::new(crate::eval::evaluator::tests::dummy_hash),
        )
        .with_add(Arc::new(|a, b| {
            let x = a.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
            let y = b.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
            if x == 99 || y == 99 {
                return Err(crate::error::HclError::Eval(
                    "add failure triggered".to_string(),
                ));
            }
            Ok(Box::new(FailableCapsule(x + y)))
        }))
        .with_cmp(Arc::new(|a, b| {
            let x = a.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
            let y = b.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
            if x == 99 || y == 99 {
                return Err(crate::error::HclError::Eval(
                    "cmp failure triggered".to_string(),
                ));
            }
            Ok(x.cmp(&y))
        }))
        .with_neg(Arc::new(|a| {
            let x = a.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
            if x == 99 {
                return Err(crate::error::HclError::Eval(
                    "neg failure triggered".to_string(),
                ));
            }
            Ok(Box::new(FailableCapsule(-x)))
        }))
        .with_index_get(Arc::new(|a, _idx| {
            let x = a.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
            if x == 99 {
                return Err(crate::error::HclError::Eval(
                    "index_get failure triggered".to_string(),
                ));
            }
            Ok(Value::new(Type::Number, ValueData::Number(x.into())))
        })),
    )
}

#[test]
fn test_capsule_operator_error_branches_and_edge_cases() -> Result<(), crate::error::HclError> {
    let ops = build_failable_capsule_ops();
    let ok_val = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(1));
    let bad_val = Value::capsule_with_ops("failable_capsule", ops, FailableCapsule(99));
    let unk_val = Value::unknown(ok_val.ty().clone());
    let mut ctx = Context::new();
    ctx.set_variable("ok", ok_val);
    ctx.set_variable("bad", bad_val);
    ctx.set_variable("unk", unk_val);
    ctx.set_variable("unk_num", Value::unknown(Type::Number));
    ctx.set_variable("num", Value::new(Type::Number, ValueData::Number(5.into())));
    let expr_bad_add = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("bad".to_string(), empty_span())),
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        empty_span(),
    );
    let err_add = Evaluator::new(&ctx)
        .evaluate(&expr_bad_add)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_add.errors()[0]
            .error
            .to_string()
            .contains("add failure triggered")
    );
    let expr_bad_cmp = Expression::BinaryOp(
        BinaryOp::Less,
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        Box::new(Expression::Variable("bad".to_string(), empty_span())),
        empty_span(),
    );
    let err_cmp = Evaluator::new(&ctx)
        .evaluate(&expr_bad_cmp)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_cmp.errors()[0]
            .error
            .to_string()
            .contains("cmp failure triggered")
    );
    let expr_bad_cmp_left = Expression::BinaryOp(
        BinaryOp::Less,
        Box::new(Expression::Variable("bad".to_string(), empty_span())),
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        empty_span(),
    );
    let err_cmp_l = Evaluator::new(&ctx)
        .evaluate(&expr_bad_cmp_left)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_cmp_l.errors()[0]
            .error
            .to_string()
            .contains("cmp failure triggered")
    );
    let expr_bad_neg = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Variable("bad".to_string(), empty_span())),
        empty_span(),
    );
    let err_neg = Evaluator::new(&ctx)
        .evaluate(&expr_bad_neg)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_neg.errors()[0]
            .error
            .to_string()
            .contains("neg failure triggered")
    );
    let expr_bad_legacy = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("bad".to_string(), empty_span())),
            operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
        }),
        empty_span(),
    );
    let err_legacy = Evaluator::new(&ctx)
        .evaluate(&expr_bad_legacy)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_legacy.errors()[0]
            .error
            .to_string()
            .contains("index_get failure triggered")
    );
    Ok(())
}

#[test]
fn test_capsule_operator_unknown_combinations() -> Result<(), crate::error::HclError> {
    let ops = build_failable_capsule_ops();
    let ok_val = Value::capsule_with_ops("failable_capsule", ops, FailableCapsule(1));
    let unk_val = Value::unknown(ok_val.ty().clone());
    let mut ctx = Context::new();
    ctx.set_variable("ok", ok_val.clone());
    ctx.set_variable("unk", unk_val);
    ctx.set_variable("unk_num", Value::unknown(Type::Number));
    let expr_rhs_unk_add = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (res_rhs_unk_add, _) = Evaluator::new(&ctx).evaluate(&expr_rhs_unk_add)?;
    assert!(res_rhs_unk_add.is_unknown());
    assert_eq!(res_rhs_unk_add.ty(), ok_val.ty());
    let expr_rhs_unk_sub = Expression::BinaryOp(
        BinaryOp::Sub,
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        empty_span(),
    );
    let (res_rhs_unk_sub, _) = Evaluator::new(&ctx).evaluate(&expr_rhs_unk_sub)?;
    assert!(res_rhs_unk_sub.is_unknown());
    assert_eq!(res_rhs_unk_sub.ty(), ok_val.ty());
    let expr_left_unk_sub = Expression::BinaryOp(
        BinaryOp::Sub,
        Box::new(Expression::Variable("unk".to_string(), empty_span())),
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        empty_span(),
    );
    let (res_left_unk_sub, _) = Evaluator::new(&ctx).evaluate(&expr_left_unk_sub)?;
    assert!(res_left_unk_sub.is_unknown());
    assert_eq!(res_left_unk_sub.ty(), ok_val.ty());
    let expr_unk_num_add = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("unk_num".to_string(), empty_span())),
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        empty_span(),
    );
    let (res_unk_num_add, _) = Evaluator::new(&ctx).evaluate(&expr_unk_num_add)?;
    assert!(res_unk_num_add.is_unknown());
    assert_eq!(res_unk_num_add.ty(), ok_val.ty());
    let expr_unk_num_sub = Expression::BinaryOp(
        BinaryOp::Sub,
        Box::new(Expression::Variable("unk_num".to_string(), empty_span())),
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        empty_span(),
    );
    let (res_unk_num_sub, _) = Evaluator::new(&ctx).evaluate(&expr_unk_num_sub)?;
    assert!(res_unk_num_sub.is_unknown());
    assert_eq!(res_unk_num_sub.ty(), ok_val.ty());
    Ok(())
}

#[test]
fn test_capsule_operator_error_branches_part_2() -> Result<(), crate::error::HclError> {
    use crate::types::ty::CapsuleOps;
    use std::sync::Arc;
    let ops = build_failable_capsule_ops();
    let ok_val = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(1));
    let bad_val = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(99));
    let mut ctx = Context::new();
    ctx.set_variable("ok", ok_val);
    ctx.set_variable("bad", bad_val);
    ctx.set_variable("num", Value::new(Type::Number, ValueData::Number(5.into())));
    let expr_rhs_cap = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("num".to_string(), empty_span())),
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        empty_span(),
    );
    let (res_rhs_cap, _) = Evaluator::new(&ctx).evaluate(&expr_rhs_cap)?;
    assert_eq!(
        res_rhs_cap.downcast_ref::<FailableCapsule>(),
        Some(&FailableCapsule(1))
    );
    let val1 = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(10));
    let val2 = Value::capsule_with_ops("failable_capsule", ops, FailableCapsule(20));
    assert_ne!(val1.unmark().0, val2.unmark().0);
    assert_eq!(val1.unmark().0, val1.unmark().0);
    assert_ne!(
        val1.unmark().0,
        Value::new(Type::Number, ValueData::Number(10.into()))
            .unmark()
            .0
    );
    let other_capsule = Value::capsule_with_ops(
        "other_capsule",
        Arc::new(CapsuleOps::new(
            "other_capsule",
            Arc::new(crate::eval::evaluator::tests::dummy_eq),
            Arc::new(crate::eval::evaluator::tests::dummy_hash),
        )),
        999_i32,
    );
    ctx.set_variable("other_cap", other_capsule);
    let expr_cap_other = Expression::BinaryOp(
        BinaryOp::Eq,
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        Box::new(Expression::Variable("other_cap".to_string(), empty_span())),
        empty_span(),
    );
    let (res_co, _) = Evaluator::new(&ctx).evaluate(&expr_cap_other)?;
    assert_eq!(*res_co.data, ValueData::Bool(false));
    let corrupt_failable = Value::new(val1.ty().clone(), ValueData::Capsule(Arc::new(999_i32)));
    assert_ne!(val1.unmark().0, corrupt_failable.unmark().0);
    Ok(())
}

#[test]
fn test_capsule_operator_error_branches_part_3() -> Result<(), crate::error::HclError> {
    let ops = build_failable_capsule_ops();
    let ok_val = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(1));
    let bad_val = Value::capsule_with_ops("failable_capsule", ops, FailableCapsule(99));
    let mut ctx = Context::new();
    ctx.set_variable("ok", ok_val);
    ctx.set_variable("bad", bad_val);
    let expr_add_right_bad = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        Box::new(Expression::Variable("bad".to_string(), empty_span())),
        empty_span(),
    );
    let err_add_r = Evaluator::new(&ctx)
        .evaluate(&expr_add_right_bad)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_add_r.errors()[0]
            .error
            .to_string()
            .contains("add failure triggered")
    );
    let expr_add_ok = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        empty_span(),
    );
    let (res_add_ok, _) = Evaluator::new(&ctx).evaluate(&expr_add_ok)?;
    assert_eq!(
        res_add_ok.downcast_ref::<FailableCapsule>(),
        Some(&FailableCapsule(2))
    );
    for (cmp_op, expected) in [
        (BinaryOp::Less, false),
        (BinaryOp::LessEq, true),
        (BinaryOp::Greater, false),
        (BinaryOp::GreaterEq, true),
    ] {
        let expr_cmp_ok = Expression::BinaryOp(
            cmp_op,
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            empty_span(),
        );
        let (res_cmp_ok, _) = Evaluator::new(&ctx).evaluate(&expr_cmp_ok)?;
        assert_eq!(*res_cmp_ok.data, ValueData::Bool(expected));
    }
    let expr_neg_ok = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Variable("ok".to_string(), empty_span())),
        empty_span(),
    );
    let (res_neg_ok, _) = Evaluator::new(&ctx).evaluate(&expr_neg_ok)?;
    assert_eq!(
        res_neg_ok.downcast_ref::<FailableCapsule>(),
        Some(&FailableCapsule(-1))
    );
    let expr_idx_ok = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("ok".to_string(), empty_span())),
            operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
        }),
        empty_span(),
    );
    let (res_idx_ok, _) = Evaluator::new(&ctx).evaluate(&expr_idx_ok)?;
    assert_eq!(*res_idx_ok.data, ValueData::Number(1.into()));
    let expr_bad_idx = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("bad".to_string(), empty_span())),
            operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
        }),
        empty_span(),
    );
    let err_bad_idx = Evaluator::new(&ctx)
        .evaluate(&expr_bad_idx)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        err_bad_idx.errors()[0]
            .error
            .to_string()
            .contains("index_get failure triggered")
    );
    Ok(())
}

#[test]
fn test_capsule_operator_mixed_and_ordering() -> Result<(), crate::error::HclError> {
    use crate::types::ty::CapsuleOps;
    use std::sync::Arc;
    let ops = build_failable_capsule_ops();
    let mut ctx = Context::new();
    ctx.set_variable("num", Value::new(Type::Number, ValueData::Number(5.into())));
    let all_ops = Arc::new(
        CapsuleOps::new(
            "full_capsule",
            Arc::new(crate::eval::evaluator::tests::dummy_eq),
            Arc::new(crate::eval::evaluator::tests::dummy_hash),
        )
        .with_add(Arc::new(|_, _| Ok(Box::new(100_i32))))
        .with_sub(Arc::new(|_, _| Ok(Box::new(101_i32))))
        .with_mul(Arc::new(|_, _| Ok(Box::new(102_i32))))
        .with_div(Arc::new(|_, _| Ok(Box::new(103_i32))))
        .with_modulo(Arc::new(|_, _| Ok(Box::new(104_i32))))
        .with_cmp(Arc::new(|_, _| Ok(std::cmp::Ordering::Equal))),
    );
    let full_val = Value::capsule_with_ops("full_capsule", all_ops, 1_i32);
    ctx.set_variable("full", full_val);
    for (bin_op, exp_val) in [
        (BinaryOp::Add, 100_i32),
        (BinaryOp::Sub, 101_i32),
        (BinaryOp::Mul, 102_i32),
        (BinaryOp::Div, 103_i32),
        (BinaryOp::Mod, 104_i32),
    ] {
        let expr_mixed = Expression::BinaryOp(
            bin_op,
            Box::new(Expression::Variable("num".to_string(), empty_span())),
            Box::new(Expression::Variable("full".to_string(), empty_span())),
            empty_span(),
        );
        let (res, _) = Evaluator::new(&ctx).evaluate(&expr_mixed)?;
        assert_eq!(res.downcast_ref::<i32>(), Some(&exp_val));
    }
    let expr_cmp_left_non_cap = Expression::BinaryOp(
        BinaryOp::LessEq,
        Box::new(Expression::Variable("num".to_string(), empty_span())),
        Box::new(Expression::Variable("full".to_string(), empty_span())),
        empty_span(),
    );
    let (res_cmp_left, _) = Evaluator::new(&ctx).evaluate(&expr_cmp_left_non_cap)?;
    assert_eq!(*res_cmp_left.data, ValueData::Bool(true));
    let expr_cmp_right_non_cap = Expression::BinaryOp(
        BinaryOp::GreaterEq,
        Box::new(Expression::Variable("full".to_string(), empty_span())),
        Box::new(Expression::Variable("num".to_string(), empty_span())),
        empty_span(),
    );
    let (res_cmp_right, _) = Evaluator::new(&ctx).evaluate(&expr_cmp_right_non_cap)?;
    assert_eq!(*res_cmp_right.data, ValueData::Bool(true));
    let expr_arith_rnc = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("full".to_string(), empty_span())),
        Box::new(Expression::Variable("num".to_string(), empty_span())),
        empty_span(),
    );
    let (res_arnc, _) = Evaluator::new(&ctx).evaluate(&expr_arith_rnc)?;
    assert_eq!(res_arnc.downcast_ref::<i32>(), Some(&100));
    let val1 = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(10));
    let val2 = Value::capsule_with_ops("failable_capsule", ops, FailableCapsule(20));
    assert_eq!(val1.cmp(&val2), std::cmp::Ordering::Less);
    assert_eq!(val2.cmp(&val1), std::cmp::Ordering::Greater);
    assert_eq!(val1.cmp(&val1), std::cmp::Ordering::Equal);
    Ok(())
}
/// Tests edge-case branch conditions in Evaluator including variable naming prefixes,
/// typos, string arithmetic coercion, logical OR, and function signature validation.
#[test]
fn test_evaluator_remaining_branch_coverage() -> Result<(), crate::error::HclError> {
    use crate::eval::func::Function;
    use std::sync::Arc;
    let mut ctx = Context::with_stdlib();
    ctx.set_variable(
        "var.x",
        Value::new(Type::String, ValueData::String("val1".to_string())),
    );
    let (val_var, _) =
        Evaluator::new(&ctx).evaluate(&Expression::Variable("var.x".to_string(), empty_span()))?;
    assert_eq!(
        val_var,
        Value::new(Type::String, ValueData::String("val1".to_string()))
    );
    ctx.set_variable(
        "local.y",
        Value::new(Type::String, ValueData::String("val2".to_string())),
    );
    let (val_loc, _) = Evaluator::new(&ctx)
        .evaluate(&Expression::Variable("local.y".to_string(), empty_span()))?;
    assert_eq!(
        val_loc,
        Value::new(Type::String, ValueData::String("val2".to_string()))
    );
    ctx.set_variable(
        "var.configuration_option",
        Value::new(Type::String, ValueData::String("cfg".to_string())),
    );
    let diags_typo = Evaluator::new(&ctx)
        .evaluate(&Expression::Variable(
            "var.configuraton_option".to_string(),
            empty_span(),
        ))
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        diags_typo.errors()[0]
            .detail
            .as_deref()
            .unwrap_or("")
            .contains("Did you mean var.configuration_option?")
    );
    ctx.set_function(
        "target_function",
        Function::new(
            "target_function",
            Arc::new(|_| Ok(Value::new(Type::Number, ValueData::Number(1.into())))),
        ),
    );
    let expr_typo_func = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "target_functon".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    let diags_func = Evaluator::new(&ctx)
        .evaluate(&expr_typo_func)
        .err()
        .ok_or_else(|| crate::error::HclError::Eval("expected error".to_string()))?;
    assert!(
        diags_func.errors()[0]
            .detail
            .as_deref()
            .unwrap_or("")
            .contains("target_function")
    );
    let expr_valid_func = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "target_function".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    let (res_valid_fn, _) = Evaluator::new(&ctx).evaluate(&expr_valid_func)?;
    assert_eq!(*res_valid_fn.data, ValueData::Number(1.into()));
    let expr_str_num_add = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::String("123".to_string(), empty_span())),
        Box::new(Expression::String("abc".to_string(), empty_span())),
        empty_span(),
    );
    let (res_str_add, _) = Evaluator::new(&ctx).evaluate(&expr_str_num_add)?;
    assert_eq!(
        res_str_add,
        Value::new(Type::String, ValueData::String("123abc".to_string()))
    );
    let expr_or = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Bool(false, empty_span())),
        Box::new(Expression::Bool(true, empty_span())),
        empty_span(),
    );
    let (res_or, _) = Evaluator::new(&ctx).evaluate(&expr_or)?;
    assert_eq!(*res_or.data, ValueData::Bool(true));
    Ok(())
}

#[test]
fn test_evaluator_remaining_branch_coverage_part_1b() -> Result<(), crate::error::HclError> {
    use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
    use std::sync::Arc;
    let sig_var = FunctionSignature::with_static_return_type(
        vec![
            FunctionParamSpec::new("p1", Type::String),
            FunctionParamSpec::new("p2", Type::String),
        ],
        Type::String,
    )
    .with_variadic(FunctionParamSpec::new("rest", Type::String));
    let fn_var = Function::new(
        "fn_var",
        Arc::new(|_| {
            Ok(Value::new(
                Type::String,
                ValueData::String("ok".to_string()),
            ))
        }),
    )
    .with_signature(sig_var);
    let mut ctx_fn = Context::new();
    ctx_fn.set_function("fn_var", fn_var);
    let expr_too_few = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "fn_var".into(),
            args: vec![Expression::String("only_one".to_string(), empty_span())],
            expand_final: false,
        }),
        empty_span(),
    );
    assert!(Evaluator::new(&ctx_fn).evaluate(&expr_too_few).is_err());
    let expr_valid_var = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "fn_var".into(),
            args: vec![
                Expression::String("a".to_string(), empty_span()),
                Expression::String("b".to_string(), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let (res_fn_var, _) = Evaluator::new(&ctx_fn).evaluate(&expr_valid_var)?;
    assert_eq!(*res_fn_var.data, ValueData::String("ok".to_string()));
    Ok(())
}

#[test]
fn test_evaluator_remaining_branch_coverage_part_2() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::with_stdlib();
    ctx.set_variable("unk_str", Value::unknown(Type::String));
    let expr_template_unk = Expression::Template(
        vec![
            crate::ast::expr::TemplatePart::Literal("prefix_".to_string(), empty_span()),
            crate::ast::expr::TemplatePart::Interpolation(
                Expression::Variable("unk_str".to_string(), empty_span()),
                empty_span(),
            ),
            crate::ast::expr::TemplatePart::Literal("_suffix".to_string(), empty_span()),
        ],
        empty_span(),
    );
    let (res_tpl_unk, _) = Evaluator::new(&ctx).evaluate(&expr_template_unk)?;
    assert!(res_tpl_unk.is_unknown());
    let expr_try_empty = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "try".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_try_empty).is_err());
    let expr_can_two = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "can".into(),
            args: vec![
                Expression::Bool(true, empty_span()),
                Expression::Bool(false, empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_can_two).is_err());
    let expr_can_unk = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "can".into(),
            args: vec![Expression::Variable("unk_str".to_string(), empty_span())],
            expand_final: false,
        }),
        empty_span(),
    );
    let (res_can_unk, _) = Evaluator::new(&ctx).evaluate(&expr_can_unk)?;
    assert!(res_can_unk.is_unknown());
    let expr_nonsens = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "nonsensitive".into(),
            args: vec![Expression::String("secret".to_string(), empty_span())],
            expand_final: false,
        }),
        empty_span(),
    );
    let (res_nonsens, _) = Evaluator::new(&ctx).evaluate(&expr_nonsens)?;
    assert_eq!(*res_nonsens.data, ValueData::String("secret".to_string()));
    let expr_issens = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "issensitive".into(),
            args: vec![Expression::String("secret".to_string(), empty_span())],
            expand_final: false,
        }),
        empty_span(),
    );
    let (res_issens, _) = Evaluator::new(&ctx).evaluate(&expr_issens)?;
    assert_eq!(*res_issens.data, ValueData::Bool(false));
    Ok(())
}

#[test]
fn test_evaluator_remaining_branch_coverage_part_2b() -> Result<(), crate::error::HclError> {
    let mut ctx = Context::with_stdlib();
    ctx.set_variable("unk_str", Value::unknown(Type::String));
    let expr_exp_empty = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "min".into(),
            args: vec![],
            expand_final: true,
        }),
        empty_span(),
    );
    let (res_exp_empty, _) = Evaluator::new(&ctx).evaluate(&expr_exp_empty)?;
    assert!(res_exp_empty.is_unknown());
    let expr_exp_unk = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "min".into(),
            args: vec![Expression::Variable("unk_str".to_string(), empty_span())],
            expand_final: true,
        }),
        empty_span(),
    );
    let (res_exp_unk, _) = Evaluator::new(&ctx).evaluate(&expr_exp_unk)?;
    assert!(res_exp_unk.is_unknown());
    let expr_div_zero = Expression::BinaryOp(
        BinaryOp::Div,
        Box::new(Expression::Number(1.into(), empty_span())),
        Box::new(Expression::Number(0.into(), empty_span())),
        empty_span(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_div_zero).is_err());
    let expr_mod_zero = Expression::BinaryOp(
        BinaryOp::Mod,
        Box::new(Expression::Number(1.into(), empty_span())),
        Box::new(Expression::Number(0.into(), empty_span())),
        empty_span(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_mod_zero).is_err());
    let mut eval_diag = Evaluator::new(&ctx);
    let mut pre_diag = Diagnostic::error("existing callouts", "", empty_span());
    pre_diag
        .eval_callouts
        .push(crate::diagnostic::EvalCallout::new(empty_span(), "p", "v"));
    eval_diag.push_diagnostic(pre_diag);
    let expr_empty_trav = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Number(1.into(), empty_span())),
            operators: vec![],
        }),
        empty_span(),
    );
    let (res_et, _) = Evaluator::new(&ctx).evaluate(&expr_empty_trav)?;
    assert_eq!(*res_et.data, ValueData::Number(1.into()));
    Ok(())
}

#[test]
fn test_evaluator_remaining_branch_coverage_part_3() -> Result<(), crate::error::HclError> {
    use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
    use std::sync::Arc;
    let sig_null = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("p", Type::String).with_allow_null(true)],
        Type::String,
    )
    .with_variadic(FunctionParamSpec::new("rest", Type::String).with_allow_null(true));
    let fn_null = Function::new(
        "fn_null",
        Arc::new(|_| {
            Ok(Value::new(
                Type::String,
                ValueData::String("ok".to_string()),
            ))
        }),
    )
    .with_signature(sig_null);
    let mut ctx_null = Context::new();
    ctx_null.set_function("fn_null", fn_null);
    let expr_call_null = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "fn_null".into(),
            args: vec![
                Expression::Null(empty_span()),
                Expression::Null(empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    let (res_null, _) = Evaluator::new(&ctx_null).evaluate(&expr_call_null)?;
    assert_eq!(*res_null.data, ValueData::String("ok".to_string()));
    let sig_dyn = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("p", Type::Dynamic)],
        Type::String,
    )
    .with_variadic(FunctionParamSpec::new("rest", Type::Dynamic));
    let fn_dyn = Function::new(
        "fn_dyn",
        Arc::new(|_| {
            Ok(Value::new(
                Type::String,
                ValueData::String("ok".to_string()),
            ))
        }),
    )
    .with_signature(sig_dyn);
    ctx_null.set_function("fn_dyn", fn_dyn);
    let expr_call_dyn = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "fn_dyn".into(),
            args: vec![
                Expression::Variable("dyn_arg".to_string(), empty_span()),
                Expression::Variable("dyn_arg".to_string(), empty_span()),
            ],
            expand_final: false,
        }),
        empty_span(),
    );
    ctx_null.set_variable("dyn_arg", Value::unknown(Type::Dynamic));
    let (res_dyn, _) = Evaluator::new(&ctx_null).evaluate(&expr_call_dyn)?;
    assert_eq!(*res_dyn.data, ValueData::String("ok".to_string()));
    Ok(())
}

#[test]
fn test_evaluator_remaining_branch_coverage_part_3b() -> Result<(), crate::error::HclError> {
    use crate::eval::func::{Function, FunctionSignature};
    use std::sync::Arc;
    let mut ctx_null = Context::new();
    let sig_calc = FunctionSignature::with_static_return_type(vec![], Type::Dynamic)
        .with_value_return_type(Arc::new(|_| Ok(Type::Dynamic)));
    let fn_calc = Function::new(
        "fn_calc",
        Arc::new(|_| Ok(Value::new(Type::Number, ValueData::Number(10.into())))),
    )
    .with_signature(sig_calc);
    ctx_null.set_function("fn_calc", fn_calc);
    let expr_calc = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "fn_calc".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    let (res_calc, _) = Evaluator::new(&ctx_null).evaluate(&expr_calc)?;
    assert_eq!(*res_calc.data, ValueData::Number(10.into()));
    let sig_calc_err = FunctionSignature::with_static_return_type(vec![], Type::Dynamic)
        .with_value_return_type(Arc::new(|_| {
            Err(crate::error::HclError::Type("failed".to_string()))
        }));
    let fn_calc_err = Function::new(
        "fn_calc_err",
        Arc::new(|_| Ok(Value::new(Type::Number, ValueData::Number(10.into())))),
    )
    .with_signature(sig_calc_err);
    ctx_null.set_function("fn_calc_err", fn_calc_err);
    let expr_calc_err = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "fn_calc_err".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    let (res_calc_err, _) = Evaluator::new(&ctx_null).evaluate(&expr_calc_err)?;
    assert_eq!(*res_calc_err.data, ValueData::Number(10.into()));
    let expr_str_num_bad = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::String("hello".to_string(), empty_span())),
        Box::new(Expression::Number(123.into(), empty_span())),
        empty_span(),
    );
    assert!(
        Evaluator::new(&ctx_null)
            .evaluate(&expr_str_num_bad)
            .is_err()
    );
    Ok(())
}
#[test]
fn test_evaluator_missing_branch_coverage() {
    use crate::ast::expr::Traversal;
    use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
    use std::sync::Arc;
    let mut ctx = Context::new();
    let ref_alpha = crate::types::refinement::Refinement::new().with_prefix("abc");
    let unk_str_alpha = Value::unknown_refined(Type::String, ref_alpha);
    ctx.set_variable("unk_str", unk_str_alpha);
    ctx.set_variable("unk_bool", Value::unknown(Type::Bool));
    let expr_add = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Variable("unk_str".into(), empty_span())),
        Box::new(Expression::Variable("unk_bool".into(), empty_span())),
        empty_span(),
    );
    let eval_res = Evaluator::new(&ctx).evaluate(&expr_add);
    assert!(eval_res.is_ok());
    ctx.set_variable("null_var", Value::null(Type::Dynamic));
    let splat_trav = Traversal {
        expr: Box::new(Expression::Variable("null_var".into(), empty_span())),
        operators: vec![
            TraversalOperator::FullSplat(empty_span()),
            TraversalOperator::GetAttr("foo".into(), empty_span()),
        ],
    };
    let expr_trav = Expression::Traversal(Box::new(splat_trav), empty_span());
    let splat_res = Evaluator::new(&ctx).evaluate(&expr_trav);
    assert!(splat_res.is_ok());
    let sig_allow_null = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("arg", Type::Dynamic).with_allow_null(true)],
        Type::Dynamic,
    );
    let fn_allow_null = Function::new(
        "fn_allow_null",
        Arc::new(|_| Ok(Value::new(Type::Number, ValueData::Number(100.into())))),
    )
    .with_signature(sig_allow_null);
    ctx.set_function("fn_allow_null", fn_allow_null);
    ctx.set_variable("val_null", Value::null(Type::Dynamic));
    let expr_null_arg = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "fn_allow_null".into(),
            args: vec![Expression::Variable("val_null".into(), empty_span())],
            expand_final: false,
        }),
        empty_span(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_null_arg).is_ok());
    let sig_typed_param = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("str_arg", Type::String)],
        Type::Dynamic,
    );
    let fn_typed_param = Function::new(
        "fn_typed_param",
        Arc::new(|_| Ok(Value::new(Type::String, ValueData::String("ok".into())))),
    )
    .with_signature(sig_typed_param);
    ctx.set_function("fn_typed_param", fn_typed_param);
    ctx.set_variable("dyn_arg", Value::unknown(Type::Dynamic));
    let expr_dyn_arg = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "fn_typed_param".into(),
            args: vec![Expression::Variable("dyn_arg".into(), empty_span())],
            expand_final: false,
        }),
        empty_span(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_dyn_arg).is_ok());
}

#[test]
fn test_evaluator_missing_branch_coverage_return_types() {
    use crate::eval::func::{Function, FunctionSignature};
    use std::sync::Arc;
    let mut ctx = Context::new();
    let sig_match_ret = FunctionSignature::with_static_return_type(vec![], Type::Dynamic)
        .with_value_return_type(Arc::new(|_| Ok(Type::Number)));
    let fn_match_ret = Function::new(
        "fn_match_ret",
        Arc::new(|_| Ok(Value::new(Type::Number, ValueData::Number(42.into())))),
    )
    .with_signature(sig_match_ret);
    ctx.set_function("fn_match_ret", fn_match_ret);
    let expr_match_ret = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "fn_match_ret".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_match_ret).is_ok());
    let sig_uncoercible_ret = FunctionSignature::with_static_return_type(vec![], Type::Dynamic)
        .with_value_return_type(Arc::new(|_| Ok(Type::Number)));
    let fn_uncoercible_ret = Function::new(
        "fn_uncoercible_ret",
        Arc::new(|_| {
            Ok(Value::new(
                Type::List(Box::new(Type::Number)),
                ValueData::Array(vec![Value::new(Type::Number, ValueData::Number(1.into()))]),
            ))
        }),
    )
    .with_signature(sig_uncoercible_ret);
    ctx.set_function("fn_uncoercible_ret", fn_uncoercible_ret);
    let expr_uncoercible_ret = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "fn_uncoercible_ret".into(),
            args: vec![],
            expand_final: false,
        }),
        empty_span(),
    );
    assert!(Evaluator::new(&ctx).evaluate(&expr_uncoercible_ret).is_ok());
}
