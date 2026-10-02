//! Unit and property tests for partial evaluation and expression reduction.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]

use crate::api::parse;
use crate::ast::expr::{
    BinaryOp, Conditional, Directive, Expression, ForExpr, FuncCall, NamespacedIdent, TemplatePart,
    Traversal, TraversalOperator, UnaryOp,
};
use crate::ast::structure::{
    Attribute, Block, Body, DynamicBlock, PostconditionBlock, PreconditionBlock, ValidationBlock,
};
use crate::eval::context::Context;
use crate::eval::evaluator::Evaluator;
use crate::eval::partial::{partial_eval, partial_eval_body, value_to_expression};
use crate::number::Number;
use crate::span::Span;
use crate::types::{Type, Value, ValueData};
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn empty_span() -> Span {
    Span::new(0, 0, 0, 0, 0, 0)
}

#[test]
fn test_partial_eval_literals() {
    let ctx = Context::new();
    let span = empty_span();

    let null_expr = Expression::Null(span.clone());
    assert_eq!(partial_eval(&null_expr, &ctx).unwrap(), null_expr);

    let bool_expr = Expression::Bool(true, span.clone());
    assert_eq!(partial_eval(&bool_expr, &ctx).unwrap(), bool_expr);

    let num_expr = Expression::Number(Number::from(42), span.clone());
    assert_eq!(partial_eval(&num_expr, &ctx).unwrap(), num_expr);

    let str_expr = Expression::String("hello".to_string(), span.clone());
    assert_eq!(partial_eval(&str_expr, &ctx).unwrap(), str_expr);
}

#[test]
fn test_partial_eval_parentheses() {
    let ctx = Context::new();
    let span = empty_span();

    // Literal in parentheses gets unwrapped
    let paren_lit = Expression::Parentheses(
        Box::new(Expression::Number(Number::from(10), span.clone())),
        span.clone(),
    );
    let folded = partial_eval(&paren_lit, &ctx).unwrap();
    assert_eq!(folded, Expression::Number(Number::from(10), span.clone()));

    // Unknown expression preserves parentheses
    let paren_unk = Expression::Parentheses(
        Box::new(Expression::Variable(
            "unknown_var".to_string(),
            span.clone(),
        )),
        span.clone(),
    );
    let folded_unk = partial_eval(&paren_unk, &ctx).unwrap();
    assert!(matches!(folded_unk, Expression::Parentheses(..)));
}

#[test]
fn test_partial_eval_variables() {
    let mut ctx = Context::new();
    let span = empty_span();
    ctx.set_variable(
        "known_num",
        Value::new(Type::Number, ValueData::Number(Number::from(100))),
    );
    ctx.set_variable(
        "known_str",
        Value::new(Type::String, ValueData::String("val".to_string())),
    );
    ctx.set_variable("unknown_val", Value::unknown(Type::String));

    let var1 = Expression::Variable("known_num".to_string(), span.clone());
    assert_eq!(
        partial_eval(&var1, &ctx).unwrap(),
        Expression::Number(Number::from(100), span.clone())
    );

    let var2 = Expression::Variable("known_str".to_string(), span.clone());
    assert_eq!(
        partial_eval(&var2, &ctx).unwrap(),
        Expression::String("val".to_string(), span.clone())
    );

    // Unknown variable in context is preserved as variable
    let var_unk = Expression::Variable("unknown_val".to_string(), span.clone());
    assert_eq!(
        partial_eval(&var_unk, &ctx).unwrap(),
        Expression::Variable("unknown_val".to_string(), span.clone())
    );

    // Missing variable is preserved
    let var_missing = Expression::Variable("missing".to_string(), span.clone());
    assert_eq!(
        partial_eval(&var_missing, &ctx).unwrap(),
        Expression::Variable("missing".to_string(), span.clone())
    );
}

#[test]
fn test_partial_eval_unary_ops() {
    let ctx = Context::new();
    let span = empty_span();

    // !true -> false
    let not_true = Expression::UnaryOp(
        UnaryOp::Not,
        Box::new(Expression::Bool(true, span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&not_true, &ctx).unwrap(),
        Expression::Bool(false, span.clone())
    );

    // !false -> true
    let not_false = Expression::UnaryOp(
        UnaryOp::Not,
        Box::new(Expression::Bool(false, span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&not_false, &ctx).unwrap(),
        Expression::Bool(true, span.clone())
    );

    // -10 -> -10
    let neg_num = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Number(Number::from(10), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&neg_num, &ctx).unwrap(),
        Expression::Number(Number::from(-10), span.clone())
    );

    // Unary on unknown
    let unk_neg = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Variable("x".to_string(), span.clone())),
        span.clone(),
    );
    assert!(matches!(
        partial_eval(&unk_neg, &ctx).unwrap(),
        Expression::UnaryOp(UnaryOp::Neg, ..)
    ));
}

#[test]
fn test_partial_eval_binary_arithmetic() {
    let ctx = Context::new();
    let span = empty_span();

    // 10 + 20 -> 30
    let add = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Number(Number::from(10), span.clone())),
        Box::new(Expression::Number(Number::from(20), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&add, &ctx).unwrap(),
        Expression::Number(Number::from(30), span.clone())
    );

    // 25 - 5 -> 20
    let sub = Expression::BinaryOp(
        BinaryOp::Sub,
        Box::new(Expression::Number(Number::from(25), span.clone())),
        Box::new(Expression::Number(Number::from(5), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&sub, &ctx).unwrap(),
        Expression::Number(Number::from(20), span.clone())
    );

    // 6 * 7 -> 42
    let mul = Expression::BinaryOp(
        BinaryOp::Mul,
        Box::new(Expression::Number(Number::from(6), span.clone())),
        Box::new(Expression::Number(Number::from(7), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&mul, &ctx).unwrap(),
        Expression::Number(Number::from(42), span.clone())
    );

    // 100 / 4 -> 25
    let div = Expression::BinaryOp(
        BinaryOp::Div,
        Box::new(Expression::Number(Number::from(100), span.clone())),
        Box::new(Expression::Number(Number::from(4), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&div, &ctx).unwrap(),
        Expression::Number(Number::from(25), span.clone())
    );

    // 10 % 3 -> 1
    let rem = Expression::BinaryOp(
        BinaryOp::Mod,
        Box::new(Expression::Number(Number::from(10), span.clone())),
        Box::new(Expression::Number(Number::from(3), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&rem, &ctx).unwrap(),
        Expression::Number(Number::from(1), span.clone())
    );

    // Division by zero -> error diagnostic
    let div_zero = Expression::BinaryOp(
        BinaryOp::Div,
        Box::new(Expression::Number(Number::from(10), span.clone())),
        Box::new(Expression::Number(Number::from(0), span.clone())),
        span.clone(),
    );
    let err = partial_eval(&div_zero, &ctx).err().unwrap();
    assert!(err.has_errors());
    assert!(
        err.errors()[0]
            .error
            .to_string()
            .contains("Division by zero")
    );

    // Modulo by zero -> error diagnostic
    let mod_zero = Expression::BinaryOp(
        BinaryOp::Mod,
        Box::new(Expression::Number(Number::from(10), span.clone())),
        Box::new(Expression::Number(Number::from(0), span.clone())),
        span.clone(),
    );
    let err_mod = partial_eval(&mod_zero, &ctx).err().unwrap();
    assert!(err_mod.has_errors());
}

#[test]
fn test_partial_eval_binary_comparisons() {
    let ctx = Context::new();
    let span = empty_span();

    let eq = Expression::BinaryOp(
        BinaryOp::Eq,
        Box::new(Expression::Number(Number::from(5), span.clone())),
        Box::new(Expression::Number(Number::from(5), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&eq, &ctx).unwrap(),
        Expression::Bool(true, span.clone())
    );

    let lt = Expression::BinaryOp(
        BinaryOp::Less,
        Box::new(Expression::Number(Number::from(3), span.clone())),
        Box::new(Expression::Number(Number::from(5), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&lt, &ctx).unwrap(),
        Expression::Bool(true, span.clone())
    );

    let lte = Expression::BinaryOp(
        BinaryOp::LessEq,
        Box::new(Expression::Number(Number::from(5), span.clone())),
        Box::new(Expression::Number(Number::from(5), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&lte, &ctx).unwrap(),
        Expression::Bool(true, span.clone())
    );

    let gt = Expression::BinaryOp(
        BinaryOp::Greater,
        Box::new(Expression::Number(Number::from(10), span.clone())),
        Box::new(Expression::Number(Number::from(5), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&gt, &ctx).unwrap(),
        Expression::Bool(true, span.clone())
    );

    let gte = Expression::BinaryOp(
        BinaryOp::GreaterEq,
        Box::new(Expression::Number(Number::from(10), span.clone())),
        Box::new(Expression::Number(Number::from(10), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&gte, &ctx).unwrap(),
        Expression::Bool(true, span.clone())
    );

    let neq = Expression::BinaryOp(
        BinaryOp::NotEq,
        Box::new(Expression::String("a".to_string(), span.clone())),
        Box::new(Expression::String("b".to_string(), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&neq, &ctx).unwrap(),
        Expression::Bool(true, span.clone())
    );
}

#[test]
fn test_partial_eval_logical_short_circuit() {
    let ctx = Context::new();
    let span = empty_span();

    // false && unknown_var -> false (unknown_var is not evaluated)
    let false_and = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Bool(false, span.clone())),
        Box::new(Expression::Variable("missing".to_string(), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&false_and, &ctx).unwrap(),
        Expression::Bool(false, span.clone())
    );

    // true || unknown_var -> true (unknown_var is not evaluated)
    let true_or = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Bool(true, span.clone())),
        Box::new(Expression::Variable("missing".to_string(), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&true_or, &ctx).unwrap(),
        Expression::Bool(true, span.clone())
    );

    // true && expr -> expr
    let true_and = Expression::BinaryOp(
        BinaryOp::And,
        Box::new(Expression::Bool(true, span.clone())),
        Box::new(Expression::Variable("my_var".to_string(), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&true_and, &ctx).unwrap(),
        Expression::Variable("my_var".to_string(), span.clone())
    );

    // false || expr -> expr
    let false_or = Expression::BinaryOp(
        BinaryOp::Or,
        Box::new(Expression::Bool(false, span.clone())),
        Box::new(Expression::Variable("my_var".to_string(), span.clone())),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&false_or, &ctx).unwrap(),
        Expression::Variable("my_var".to_string(), span.clone())
    );
}

#[test]
fn test_partial_eval_conditional_branch_pruning() {
    let ctx = Context::new();
    let span = empty_span();

    // true ? 1 : 2 -> 1
    let cond_true = Expression::Conditional(
        Box::new(Conditional {
            cond_expr: Expression::Bool(true, span.clone()),
            true_expr: Expression::Number(Number::from(1), span.clone()),
            false_expr: Expression::Number(Number::from(2), span.clone()),
        }),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&cond_true, &ctx).unwrap(),
        Expression::Number(Number::from(1), span.clone())
    );

    // false ? 1 : 2 -> 2
    let cond_false = Expression::Conditional(
        Box::new(Conditional {
            cond_expr: Expression::Bool(false, span.clone()),
            true_expr: Expression::Number(Number::from(1), span.clone()),
            false_expr: Expression::Number(Number::from(2), span.clone()),
        }),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&cond_false, &ctx).unwrap(),
        Expression::Number(Number::from(2), span.clone())
    );

    // unknown ? 42 : 42 -> 42 (identical branches folded)
    let cond_identical = Expression::Conditional(
        Box::new(Conditional {
            cond_expr: Expression::Variable("unk".to_string(), span.clone()),
            true_expr: Expression::Number(Number::from(42), span.clone()),
            false_expr: Expression::Number(Number::from(42), span.clone()),
        }),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&cond_identical, &ctx).unwrap(),
        Expression::Number(Number::from(42), span.clone())
    );
}

#[test]
fn test_partial_eval_collections_and_templates() {
    let ctx = Context::new();
    let span = empty_span();

    // Tuple folding
    let tuple = Expression::Tuple(
        vec![
            Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::Number(Number::from(1), span.clone())),
                Box::new(Expression::Number(Number::from(2), span.clone())),
                span.clone(),
            ),
            Expression::Number(Number::from(4), span.clone()),
        ],
        span.clone(),
    );
    let folded_tuple = partial_eval(&tuple, &ctx).unwrap();
    if let Expression::Tuple(elems, _) = folded_tuple {
        assert_eq!(elems[0], Expression::Number(Number::from(3), span.clone()));
    } else {
        panic!("expected tuple");
    }

    // Object folding
    let obj = Expression::Object(
        vec![(
            Expression::String("k".to_string(), span.clone()),
            Expression::BinaryOp(
                BinaryOp::Mul,
                Box::new(Expression::Number(Number::from(2), span.clone())),
                Box::new(Expression::Number(Number::from(5), span.clone())),
                span.clone(),
            ),
        )],
        span.clone(),
    );
    let folded_obj = partial_eval(&obj, &ctx).unwrap();
    if let Expression::Object(kvs, _) = folded_obj {
        assert_eq!(kvs[0].1, Expression::Number(Number::from(10), span.clone()));
    } else {
        panic!("expected object");
    }

    // Template string literal concatenation
    let tmpl = Expression::Template(
        vec![
            TemplatePart::Literal("hello ".to_string(), span.clone()),
            TemplatePart::Interpolation(
                Expression::String("world".to_string(), span.clone()),
                span.clone(),
            ),
        ],
        span.clone(),
    );
    let folded_tmpl = partial_eval(&tmpl, &ctx).unwrap();
    assert_eq!(
        folded_tmpl,
        Expression::String("hello world".to_string(), span.clone())
    );
}

#[test]
fn test_partial_eval_deterministic_functions() {
    let ctx = Context::with_stdlib();
    let span = empty_span();

    // upper("hello") -> "HELLO"
    let call_upper = Expression::FuncCall(
        Box::new(FuncCall {
            name: NamespacedIdent::simple("upper", span.clone()),
            args: vec![Expression::String("hello".to_string(), span.clone())],
            expand_final: false,
        }),
        span.clone(),
    );
    let folded_upper = partial_eval(&call_upper, &ctx).unwrap();
    assert_eq!(
        folded_upper,
        Expression::String("HELLO".to_string(), span.clone())
    );

    // Call with unknown arg is not folded
    let call_with_unk = Expression::FuncCall(
        Box::new(FuncCall {
            name: NamespacedIdent::simple("upper", span.clone()),
            args: vec![Expression::Variable("unk".to_string(), span.clone())],
            expand_final: false,
        }),
        span.clone(),
    );
    let preserved = partial_eval(&call_with_unk, &ctx).unwrap();
    assert!(matches!(preserved, Expression::FuncCall(..)));
}

#[test]
fn test_partial_eval_body_comprehensive() {
    let src = r#"
        a = 10 + 20
        b = var.external + 5
        block "sub" "item" {
            c = true ? "yes" : "no"
            d = var.missing
        }
        dynamic "dyn" {
            for_each = [1, 2]
            content {
                inner = 100 * 2
            }
        }
        check {
            assert {
                condition = 5 > 2
                error_message = "failed"
            }
        }
    "#;
    let body = parse(src).unwrap();
    let ctx = Context::new();
    let reduced = partial_eval_body(&body, &ctx).unwrap();

    if let Expression::Number(n, _) = &reduced.attributes["a"].expr {
        assert_eq!(*n, Number::from(30));
    } else {
        panic!("expected number for attribute a");
    }

    assert!(matches!(
        reduced.attributes["b"].expr,
        Expression::BinaryOp(..)
    ));

    let sub_block = &reduced.blocks[0];
    if let Expression::String(s, _) = &sub_block.body.attributes["c"].expr {
        assert_eq!(s, "yes");
    } else {
        panic!("expected string for attribute c");
    }

    let dyn_block = &reduced.dynamic_blocks[0];
    if let Expression::Number(n, _) = &dyn_block.content.attributes["inner"].expr {
        assert_eq!(*n, Number::from(200));
    } else {
        panic!("expected number for inner attribute");
    }
}

#[test]
fn test_partial_eval_idempotency() {
    let ctx = Context::with_stdlib();
    let span = empty_span();

    let complex_expr = Expression::Conditional(
        Box::new(Conditional {
            cond_expr: Expression::BinaryOp(
                BinaryOp::Less,
                Box::new(Expression::Number(Number::from(1), span.clone())),
                Box::new(Expression::Number(Number::from(10), span.clone())),
                span.clone(),
            ),
            true_expr: Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::Number(Number::from(5), span.clone())),
                Box::new(Expression::Variable("v".to_string(), span.clone())),
                span.clone(),
            ),
            false_expr: Expression::Number(Number::from(0), span.clone()),
        }),
        span.clone(),
    );

    let step1 = partial_eval(&complex_expr, &ctx).unwrap();
    let step2 = partial_eval(&step1, &ctx).unwrap();
    assert_eq!(step1, step2);
}

#[test]
fn test_evaluator_partial_evaluate_method() {
    let ctx = Context::new();
    let span = empty_span();
    let evaluator = Evaluator::new(&ctx);

    let expr = Expression::BinaryOp(
        BinaryOp::Mul,
        Box::new(Expression::Number(Number::from(3), span.clone())),
        Box::new(Expression::Number(Number::from(4), span.clone())),
        span.clone(),
    );

    let res = evaluator.partial_evaluate(&expr).unwrap();
    assert_eq!(res, Expression::Number(Number::from(12), span));
}

#[test]
fn test_value_to_expression_all_variants() {
    let span = empty_span();

    // Null
    let null_val = Value::null(Type::Dynamic);
    assert_eq!(
        value_to_expression(&null_val, span.clone()),
        Expression::Null(span.clone())
    );

    // Bool
    let bool_val = Value::new(Type::Bool, ValueData::Bool(true));
    assert_eq!(
        value_to_expression(&bool_val, span.clone()),
        Expression::Bool(true, span.clone())
    );

    // Number
    let num_val = Value::new(Type::Number, ValueData::Number(Number::from(42)));
    assert_eq!(
        value_to_expression(&num_val, span.clone()),
        Expression::Number(Number::from(42), span.clone())
    );

    // String
    let str_val = Value::new(Type::String, ValueData::String("hello".to_string()));
    assert_eq!(
        value_to_expression(&str_val, span.clone()),
        Expression::String("hello".to_string(), span.clone())
    );

    // Array / List
    let arr_val = Value::new(
        Type::List(Box::new(Type::Number)),
        ValueData::Array(vec![Value::new(
            Type::Number,
            ValueData::Number(Number::from(1)),
        )]),
    );
    assert_eq!(
        value_to_expression(&arr_val, span.clone()),
        Expression::Tuple(
            vec![Expression::Number(Number::from(1), span.clone())],
            span.clone()
        )
    );

    // Set
    let set_val = Value::new(
        Type::Set(Box::new(Type::Number)),
        ValueData::Set(BTreeSet::from([Value::new(
            Type::Number,
            ValueData::Number(Number::from(2)),
        )])),
    );
    assert_eq!(
        value_to_expression(&set_val, span.clone()),
        Expression::Tuple(
            vec![Expression::Number(Number::from(2), span.clone())],
            span.clone()
        )
    );

    // Object
    let obj_map = BTreeMap::from([(
        "key".to_string(),
        Value::new(Type::Number, ValueData::Number(Number::from(3))),
    )]);
    let obj_val = Value::new(
        Type::Map(Box::new(Type::Number)),
        ValueData::Object(obj_map),
    );
    assert_eq!(
        value_to_expression(&obj_val, span.clone()),
        Expression::Object(
            vec![(
                Expression::String("key".to_string(), span.clone()),
                Expression::Number(Number::from(3), span.clone())
            )],
            span.clone()
        )
    );

    // Unknown
    let unk_val = Value::unknown(Type::String);
    assert_eq!(
        value_to_expression(&unk_val, span.clone()),
        Expression::Variable("__unknown__".to_string(), span.clone())
    );

    // Capsule
    let cap_val = Value::capsule("custom_type", 99_i32);
    assert_eq!(
        value_to_expression(&cap_val, span.clone()),
        Expression::Variable("__unknown__".to_string(), span.clone())
    );
}

#[test]
fn test_partial_eval_binary_ops_exhaustive() {
    let ctx = Context::new();
    let span = empty_span();

    // BinaryOp::And with unknown on left
    {
        let unk_and_t = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Variable("unk".to_string(), span.clone())),
            Box::new(Expression::Bool(true, span.clone())),
            span.clone(),
        );
        assert_eq!(
            partial_eval(&unk_and_t, &ctx).unwrap(),
            Expression::Variable("unk".to_string(), span.clone())
        );

        let unk_and_f = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Variable("unk".to_string(), span.clone())),
            Box::new(Expression::Bool(false, span.clone())),
            span.clone(),
        );
        assert_eq!(
            partial_eval(&unk_and_f, &ctx).unwrap(),
            Expression::BinaryOp(
                BinaryOp::And,
                Box::new(Expression::Variable("unk".to_string(), span.clone())),
                Box::new(Expression::Bool(false, span.clone())),
                span.clone(),
            )
        );

        let unk_and_other = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Variable("unk1".to_string(), span.clone())),
            Box::new(Expression::Variable("unk2".to_string(), span.clone())),
            span.clone(),
        );
        assert_eq!(
            partial_eval(&unk_and_other, &ctx).unwrap(),
            Expression::BinaryOp(
                BinaryOp::And,
                Box::new(Expression::Variable("unk1".to_string(), span.clone())),
                Box::new(Expression::Variable("unk2".to_string(), span.clone())),
                span.clone(),
            )
        );
    }

    // BinaryOp::Or with unknown on left
    {
        let unk_or_f = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Variable("unk".to_string(), span.clone())),
            Box::new(Expression::Bool(false, span.clone())),
            span.clone(),
        );
        assert_eq!(
            partial_eval(&unk_or_f, &ctx).unwrap(),
            Expression::Variable("unk".to_string(), span.clone())
        );

        let unk_or_t = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Variable("unk".to_string(), span.clone())),
            Box::new(Expression::Bool(true, span.clone())),
            span.clone(),
        );
        assert_eq!(
            partial_eval(&unk_or_t, &ctx).unwrap(),
            Expression::BinaryOp(
                BinaryOp::Or,
                Box::new(Expression::Variable("unk".to_string(), span.clone())),
                Box::new(Expression::Bool(true, span.clone())),
                span.clone(),
            )
        );

        let unk_or_other = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Variable("unk1".to_string(), span.clone())),
            Box::new(Expression::Variable("unk2".to_string(), span.clone())),
            span.clone(),
        );
        assert_eq!(
            partial_eval(&unk_or_other, &ctx).unwrap(),
            Expression::BinaryOp(
                BinaryOp::Or,
                Box::new(Expression::Variable("unk1".to_string(), span.clone())),
                Box::new(Expression::Variable("unk2".to_string(), span.clone())),
                span.clone(),
            )
        );
    }

    // Number comparisons and fallback
    {
        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::NotEq,
                    Box::new(Expression::Number(Number::from(5), span.clone())),
                    Box::new(Expression::Number(Number::from(6), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(true, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::NotEq,
                    Box::new(Expression::Number(Number::from(5), span.clone())),
                    Box::new(Expression::Number(Number::from(5), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(false, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::Eq,
                    Box::new(Expression::Number(Number::from(5), span.clone())),
                    Box::new(Expression::Number(Number::from(6), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(false, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::Less,
                    Box::new(Expression::Number(Number::from(6), span.clone())),
                    Box::new(Expression::Number(Number::from(5), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(false, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::LessEq,
                    Box::new(Expression::Number(Number::from(6), span.clone())),
                    Box::new(Expression::Number(Number::from(5), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(false, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::Greater,
                    Box::new(Expression::Number(Number::from(5), span.clone())),
                    Box::new(Expression::Number(Number::from(6), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(false, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::GreaterEq,
                    Box::new(Expression::Number(Number::from(4), span.clone())),
                    Box::new(Expression::Number(Number::from(5), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(false, span.clone())
        );

        // Number with non-arithmetic op (hits _ => {})
        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::And,
                    Box::new(Expression::Number(Number::from(1), span.clone())),
                    Box::new(Expression::Number(Number::from(2), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::BinaryOp(
                BinaryOp::And,
                Box::new(Expression::Number(Number::from(1), span.clone())),
                Box::new(Expression::Number(Number::from(2), span.clone())),
                span.clone(),
            )
        );
    }

    // Bool comparisons and fallback
    {
        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::Eq,
                    Box::new(Expression::Bool(true, span.clone())),
                    Box::new(Expression::Bool(true, span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(true, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::Eq,
                    Box::new(Expression::Bool(true, span.clone())),
                    Box::new(Expression::Bool(false, span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(false, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::NotEq,
                    Box::new(Expression::Bool(true, span.clone())),
                    Box::new(Expression::Bool(false, span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(true, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::NotEq,
                    Box::new(Expression::Bool(true, span.clone())),
                    Box::new(Expression::Bool(true, span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(false, span.clone())
        );

        // Bool with non-comparison op (hits _ => {})
        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::Add,
                    Box::new(Expression::Bool(true, span.clone())),
                    Box::new(Expression::Bool(false, span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::Bool(true, span.clone())),
                Box::new(Expression::Bool(false, span.clone())),
                span.clone(),
            )
        );
    }

    // String comparisons and fallback
    {
        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::Eq,
                    Box::new(Expression::String("a".to_string(), span.clone())),
                    Box::new(Expression::String("a".to_string(), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(true, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::Eq,
                    Box::new(Expression::String("a".to_string(), span.clone())),
                    Box::new(Expression::String("b".to_string(), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(false, span.clone())
        );

        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::NotEq,
                    Box::new(Expression::String("a".to_string(), span.clone())),
                    Box::new(Expression::String("a".to_string(), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::Bool(false, span.clone())
        );

        // String with non-comparison op (hits _ => {})
        assert_eq!(
            partial_eval(
                &Expression::BinaryOp(
                    BinaryOp::Sub,
                    Box::new(Expression::String("a".to_string(), span.clone())),
                    Box::new(Expression::String("b".to_string(), span.clone())),
                    span.clone(),
                ),
                &ctx
            )
            .unwrap(),
            Expression::BinaryOp(
                BinaryOp::Sub,
                Box::new(Expression::String("a".to_string(), span.clone())),
                Box::new(Expression::String("b".to_string(), span.clone())),
                span.clone(),
            )
        );
    }
}

#[test]
fn test_partial_eval_conditional_unresolved() {
    let ctx = Context::new();
    let span = empty_span();

    let cond_unresolved = Expression::Conditional(
        Box::new(Conditional {
            cond_expr: Expression::Variable("unk".to_string(), span.clone()),
            true_expr: Expression::Number(Number::from(1), span.clone()),
            false_expr: Expression::Number(Number::from(2), span.clone()),
        }),
        span.clone(),
    );
    let folded = partial_eval(&cond_unresolved, &ctx).unwrap();
    assert_eq!(
        folded,
        Expression::Conditional(
            Box::new(Conditional {
                cond_expr: Expression::Variable("unk".to_string(), span.clone()),
                true_expr: Expression::Number(Number::from(1), span.clone()),
                false_expr: Expression::Number(Number::from(2), span.clone()),
            }),
            span.clone()
        )
    );
}

#[test]
fn test_partial_eval_template_non_literals() {
    let ctx = Context::new();
    let span = empty_span();

    // Template with unknown variable interpolation
    let tmpl_unk = Expression::Template(
        vec![
            TemplatePart::Literal("prefix_".to_string(), span.clone()),
            TemplatePart::Interpolation(
                Expression::Variable("unk".to_string(), span.clone()),
                span.clone(),
            ),
        ],
        span.clone(),
    );
    let folded_unk = partial_eval(&tmpl_unk, &ctx).unwrap();
    assert!(matches!(folded_unk, Expression::Template(..)));

    // Template with directive
    let tmpl_dir = Expression::Template(
        vec![
            TemplatePart::Literal("prefix_".to_string(), span.clone()),
            TemplatePart::Directive(
                Directive::Strip {
                    strip_left: true,
                    strip_right: false,
                },
                span.clone(),
            ),
        ],
        span.clone(),
    );
    let folded_dir = partial_eval(&tmpl_dir, &ctx).unwrap();
    assert!(matches!(folded_dir, Expression::Template(..)));
}

#[test]
fn test_partial_eval_traversal_advanced() {
    let mut ctx = Context::new();
    let span = empty_span();

    let user_map = BTreeMap::from([(
        "name".to_string(),
        Value::new(Type::String, ValueData::String("Alice".to_string())),
    )]);
    ctx.set_variable(
        "user",
        Value::new(
            Type::Map(Box::new(Type::String)),
            ValueData::Object(user_map),
        ),
    );

    // Traversal on defined variable resolving successfully
    let trav_resolved = Expression::Traversal(
        Box::new(Traversal {
            expr: Box::new(Expression::Variable("user".to_string(), span.clone())),
            operators: vec![TraversalOperator::GetAttr("name".to_string(), span.clone())],
        }),
        span.clone(),
    );
    assert_eq!(
        partial_eval(&trav_resolved, &ctx).unwrap(),
        Expression::String("Alice".to_string(), span.clone())
    );

    // Traversal on defined variable failing evaluation (missing attribute) -> falls through
    let trav_missing_field = Expression::Traversal(
        Box::new(Traversal {
            expr: Box::new(Expression::Variable("user".to_string(), span.clone())),
            operators: vec![TraversalOperator::GetAttr("age".to_string(), span.clone())],
        }),
        span.clone(),
    );
    let folded_missing = partial_eval(&trav_missing_field, &ctx).unwrap();
    assert!(matches!(folded_missing, Expression::Traversal(..)));

    // Traversal on unknown base with Index operator (index is folded)
    let trav_index = Expression::Traversal(
        Box::new(Traversal {
            expr: Box::new(Expression::Variable("items".to_string(), span.clone())),
            operators: vec![TraversalOperator::Index(
                Expression::BinaryOp(
                    BinaryOp::Add,
                    Box::new(Expression::Number(Number::from(1), span.clone())),
                    Box::new(Expression::Number(Number::from(2), span.clone())),
                    span.clone(),
                ),
                span.clone(),
            )],
        }),
        span.clone(),
    );
    let folded_index = partial_eval(&trav_index, &ctx).unwrap();
    if let Expression::Traversal(t, _) = folded_index {
        assert_eq!(
            t.operators[0],
            TraversalOperator::Index(
                Expression::Number(Number::from(3), span.clone()),
                span.clone()
            )
        );
    } else {
        panic!("expected traversal");
    }

    // Traversal on unknown base with non-index operator (GetAttr)
    let trav_attr = Expression::Traversal(
        Box::new(Traversal {
            expr: Box::new(Expression::Variable("items".to_string(), span.clone())),
            operators: vec![TraversalOperator::GetAttr("foo".to_string(), span.clone())],
        }),
        span.clone(),
    );
    let folded_attr = partial_eval(&trav_attr, &ctx).unwrap();
    if let Expression::Traversal(t, _) = folded_attr {
        assert_eq!(
            t.operators[0],
            TraversalOperator::GetAttr("foo".to_string(), span.clone())
        );
    } else {
        panic!("expected traversal");
    }
}

#[test]
fn test_partial_eval_for_expr_exhaustive() {
    let ctx = Context::new();
    let span = empty_span();

    // 1. Fully resolvable tuple for-expr
    let for_tuple = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "x".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![
                    Expression::Number(Number::from(1), span.clone()),
                    Expression::Number(Number::from(2), span.clone()),
                ],
                span.clone(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::BinaryOp(
                BinaryOp::Mul,
                Box::new(Expression::Variable("x".to_string(), span.clone())),
                Box::new(Expression::Number(Number::from(10), span.clone())),
                span.clone(),
            )),
            cond_expr: None,
            grouping: false,
        }),
        span.clone(),
    );
    let folded_tuple = partial_eval(&for_tuple, &ctx).unwrap();
    assert_eq!(
        folded_tuple,
        Expression::Tuple(
            vec![
                Expression::Number(Number::from(10), span.clone()),
                Expression::Number(Number::from(20), span.clone()),
            ],
            span.clone()
        )
    );

    // 2. Fully resolvable object for-expr with key_expr and cond_expr
    let for_obj = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: Some("k".to_string()),
            val_var: "v".to_string(),
            collection: Box::new(Expression::Object(
                vec![
                    (
                        Expression::String("a".to_string(), span.clone()),
                        Expression::Number(Number::from(1), span.clone()),
                    ),
                    (
                        Expression::String("b".to_string(), span.clone()),
                        Expression::Number(Number::from(2), span.clone()),
                    ),
                ],
                span.clone(),
            )),
            key_expr: Some(Box::new(Expression::Variable(
                "k".to_string(),
                span.clone(),
            ))),
            val_expr: Box::new(Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::Variable("v".to_string(), span.clone())),
                Box::new(Expression::Number(Number::from(100), span.clone())),
                span.clone(),
            )),
            cond_expr: Some(Box::new(Expression::BinaryOp(
                BinaryOp::Greater,
                Box::new(Expression::Variable("v".to_string(), span.clone())),
                Box::new(Expression::Number(Number::from(1), span.clone())),
                span.clone(),
            ))),
            grouping: false,
        }),
        span.clone(),
    );
    let folded_obj = partial_eval(&for_obj, &ctx).unwrap();
    assert_eq!(
        folded_obj,
        Expression::Object(
            vec![(
                Expression::String("b".to_string(), span.clone()),
                Expression::Number(Number::from(102), span.clone()),
            )],
            span.clone()
        )
    );

    // 3. Fully resolvable for-expr with grouping
    let for_group = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "x".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![
                    Expression::Number(Number::from(1), span.clone()),
                    Expression::Number(Number::from(2), span.clone()),
                ],
                span.clone(),
            )),
            key_expr: Some(Box::new(Expression::String("g".to_string(), span.clone()))),
            val_expr: Box::new(Expression::Variable("x".to_string(), span.clone())),
            cond_expr: None,
            grouping: true,
        }),
        span.clone(),
    );
    let folded_group = partial_eval(&for_group, &ctx).unwrap();
    assert_eq!(
        folded_group,
        Expression::Object(
            vec![(
                Expression::String("g".to_string(), span.clone()),
                Expression::Tuple(
                    vec![
                        Expression::Number(Number::from(1), span.clone()),
                        Expression::Number(Number::from(2), span.clone()),
                    ],
                    span.clone()
                ),
            )],
            span.clone()
        )
    );

    // 4. For-expr with unknown collection (preserved as ForExpr)
    let for_unk = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: Some("k".to_string()),
            val_var: "v".to_string(),
            collection: Box::new(Expression::Variable("unk_coll".to_string(), span.clone())),
            key_expr: Some(Box::new(Expression::Variable(
                "k".to_string(),
                span.clone(),
            ))),
            val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
            cond_expr: Some(Box::new(Expression::Bool(true, span.clone()))),
            grouping: false,
        }),
        span.clone(),
    );
    let folded_unk_for = partial_eval(&for_unk, &ctx).unwrap();
    assert!(matches!(folded_unk_for, Expression::ForExpr(..)));
}

#[test]
fn test_partial_eval_func_call_branches() {
    let ctx = Context::with_stdlib();
    let span = empty_span();

    // 1. Non-deterministic function call (uuid) -> preserved
    let call_uuid = Expression::FuncCall(
        Box::new(FuncCall {
            name: NamespacedIdent::simple("uuid", span.clone()),
            args: vec![],
            expand_final: false,
        }),
        span.clone(),
    );
    let folded_uuid = partial_eval(&call_uuid, &ctx).unwrap();
    assert!(matches!(folded_uuid, Expression::FuncCall(..)));

    // 2. expand_final = true -> preserved
    let call_expand = Expression::FuncCall(
        Box::new(FuncCall {
            name: NamespacedIdent::simple("upper", span.clone()),
            args: vec![Expression::String("hello".to_string(), span.clone())],
            expand_final: true,
        }),
        span.clone(),
    );
    let folded_expand = partial_eval(&call_expand, &ctx).unwrap();
    assert!(matches!(folded_expand, Expression::FuncCall(..)));

    // 3. Evaluation fails (e.g. passing null to upper) -> preserved
    let call_err = Expression::FuncCall(
        Box::new(FuncCall {
            name: NamespacedIdent::simple("upper", span.clone()),
            args: vec![Expression::Null(span.clone())],
            expand_final: false,
        }),
        span.clone(),
    );
    let folded_err = partial_eval(&call_err, &ctx).unwrap();
    assert!(matches!(folded_err, Expression::FuncCall(..)));

    // 4. Function not in context -> preserved
    let call_unknown_fn = Expression::FuncCall(
        Box::new(FuncCall {
            name: NamespacedIdent::simple("no_such_func", span.clone()),
            args: vec![Expression::String("hello".to_string(), span.clone())],
            expand_final: false,
        }),
        span.clone(),
    );
    let folded_unknown_fn = partial_eval(&call_unknown_fn, &ctx).unwrap();
    assert!(matches!(folded_unknown_fn, Expression::FuncCall(..)));
}

#[test]
fn test_partial_eval_body_validations_and_lifecycle() {
    let ctx = Context::new();
    let span = empty_span();

    let body = Body {
        attributes: HashMap::new(),
        blocks: vec![],
        functions: vec![],
        dynamic_blocks: vec![],
        validations: vec![ValidationBlock {
            condition: Expression::BinaryOp(
                BinaryOp::Greater,
                Box::new(Expression::Number(Number::from(5), span.clone())),
                Box::new(Expression::Number(Number::from(2), span.clone())),
                span.clone(),
            ),
            error_message: Expression::String("invalid".to_string(), span.clone()),
            span: span.clone(),
        }],
        preconditions: vec![PreconditionBlock {
            condition: Expression::BinaryOp(
                BinaryOp::Eq,
                Box::new(Expression::Number(Number::from(1), span.clone())),
                Box::new(Expression::Number(Number::from(1), span.clone())),
                span.clone(),
            ),
            error_message: Expression::String("precondition failed".to_string(), span.clone()),
            span: span.clone(),
        }],
        postconditions: vec![PostconditionBlock {
            condition: Expression::BinaryOp(
                BinaryOp::Less,
                Box::new(Expression::Number(Number::from(1), span.clone())),
                Box::new(Expression::Number(Number::from(2), span.clone())),
                span.clone(),
            ),
            error_message: Expression::String("postcondition failed".to_string(), span.clone()),
            span: span.clone(),
        }],
        span: span.clone(),
    };

    let reduced = partial_eval_body(&body, &ctx).unwrap();
    assert_eq!(
        reduced.validations[0].condition,
        Expression::Bool(true, span.clone())
    );
    assert_eq!(
        reduced.preconditions[0].condition,
        Expression::Bool(true, span.clone())
    );
    assert_eq!(
        reduced.postconditions[0].condition,
        Expression::Bool(true, span.clone())
    );
}

#[test]
fn test_partial_eval_error_propagation() {
    let ctx = Context::new();
    let span = empty_span();

    let div_zero = Expression::BinaryOp(
        BinaryOp::Div,
        Box::new(Expression::Number(Number::from(1), span.clone())),
        Box::new(Expression::Number(Number::from(0), span.clone())),
        span.clone(),
    );

    // Parentheses error
    let paren_err = Expression::Parentheses(Box::new(div_zero.clone()), span.clone());
    assert!(partial_eval(&paren_err, &ctx).is_err());

    // UnaryOp error
    let unary_err = Expression::UnaryOp(UnaryOp::Neg, Box::new(div_zero.clone()), span.clone());
    assert!(partial_eval(&unary_err, &ctx).is_err());

    // BinaryOp::And errors
    {
        let and_l = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(div_zero.clone()),
            Box::new(Expression::Bool(true, span.clone())),
            span.clone(),
        );
        assert!(partial_eval(&and_l, &ctx).is_err());

        let and_r = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Variable("unk".to_string(), span.clone())),
            Box::new(div_zero.clone()),
            span.clone(),
        );
        assert!(partial_eval(&and_r, &ctx).is_err());
    }

    // BinaryOp::Or errors
    {
        let or_l = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(div_zero.clone()),
            Box::new(Expression::Bool(false, span.clone())),
            span.clone(),
        );
        assert!(partial_eval(&or_l, &ctx).is_err());

        let or_r = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Variable("unk".to_string(), span.clone())),
            Box::new(div_zero.clone()),
            span.clone(),
        );
        assert!(partial_eval(&or_r, &ctx).is_err());
    }

    // BinaryOp non-short-circuit errors
    {
        let add_l = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(div_zero.clone()),
            Box::new(Expression::Number(Number::from(1), span.clone())),
            span.clone(),
        );
        assert!(partial_eval(&add_l, &ctx).is_err());

        let add_r = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Number(Number::from(1), span.clone())),
            Box::new(div_zero.clone()),
            span.clone(),
        );
        assert!(partial_eval(&add_r, &ctx).is_err());
    }

    // Conditional errors
    {
        let cond_c = Expression::Conditional(
            Box::new(Conditional {
                cond_expr: div_zero.clone(),
                true_expr: Expression::Number(Number::from(1), span.clone()),
                false_expr: Expression::Number(Number::from(2), span.clone()),
            }),
            span.clone(),
        );
        assert!(partial_eval(&cond_c, &ctx).is_err());

        let cond_t = Expression::Conditional(
            Box::new(Conditional {
                cond_expr: Expression::Variable("unk".to_string(), span.clone()),
                true_expr: div_zero.clone(),
                false_expr: Expression::Number(Number::from(2), span.clone()),
            }),
            span.clone(),
        );
        assert!(partial_eval(&cond_t, &ctx).is_err());

        let cond_f = Expression::Conditional(
            Box::new(Conditional {
                cond_expr: Expression::Variable("unk".to_string(), span.clone()),
                true_expr: Expression::Number(Number::from(1), span.clone()),
                false_expr: div_zero.clone(),
            }),
            span.clone(),
        );
        assert!(partial_eval(&cond_f, &ctx).is_err());
    }

    // Tuple error
    let tuple_err = Expression::Tuple(vec![div_zero.clone()], span.clone());
    assert!(partial_eval(&tuple_err, &ctx).is_err());

    // Object errors
    {
        let obj_k = Expression::Object(
            vec![(
                div_zero.clone(),
                Expression::Number(Number::from(1), span.clone()),
            )],
            span.clone(),
        );
        assert!(partial_eval(&obj_k, &ctx).is_err());

        let obj_v = Expression::Object(
            vec![(
                Expression::String("k".to_string(), span.clone()),
                div_zero.clone(),
            )],
            span.clone(),
        );
        assert!(partial_eval(&obj_v, &ctx).is_err());
    }

    // Template error
    let tmpl_err = Expression::Template(
        vec![TemplatePart::Interpolation(div_zero.clone(), span.clone())],
        span.clone(),
    );
    assert!(partial_eval(&tmpl_err, &ctx).is_err());

    // Traversal errors
    {
        let trav_base = Expression::Traversal(
            Box::new(Traversal {
                expr: Box::new(div_zero.clone()),
                operators: vec![TraversalOperator::GetAttr("a".to_string(), span.clone())],
            }),
            span.clone(),
        );
        assert!(partial_eval(&trav_base, &ctx).is_err());

        let trav_idx = Expression::Traversal(
            Box::new(Traversal {
                expr: Box::new(Expression::Variable("unk".to_string(), span.clone())),
                operators: vec![TraversalOperator::Index(div_zero.clone(), span.clone())],
            }),
            span.clone(),
        );
        assert!(partial_eval(&trav_idx, &ctx).is_err());
    }

    // FuncCall error
    let fn_err = Expression::FuncCall(
        Box::new(FuncCall {
            name: NamespacedIdent::simple("upper", span.clone()),
            args: vec![div_zero.clone()],
            expand_final: false,
        }),
        span.clone(),
    );
    assert!(partial_eval(&fn_err, &ctx).is_err());

    // ForExpr errors
    {
        let for_coll = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "x".to_string(),
                collection: Box::new(div_zero.clone()),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("x".to_string(), span.clone())),
                cond_expr: None,
                grouping: false,
            }),
            span.clone(),
        );
        assert!(partial_eval(&for_coll, &ctx).is_err());

        let for_val = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "x".to_string(),
                collection: Box::new(Expression::Tuple(vec![], span.clone())),
                key_expr: None,
                val_expr: Box::new(div_zero.clone()),
                cond_expr: None,
                grouping: false,
            }),
            span.clone(),
        );
        assert!(partial_eval(&for_val, &ctx).is_err());

        let for_key = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "x".to_string(),
                collection: Box::new(Expression::Tuple(vec![], span.clone())),
                key_expr: Some(Box::new(div_zero.clone())),
                val_expr: Box::new(Expression::Variable("x".to_string(), span.clone())),
                cond_expr: None,
                grouping: false,
            }),
            span.clone(),
        );
        assert!(partial_eval(&for_key, &ctx).is_err());

        let for_cond = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "x".to_string(),
                collection: Box::new(Expression::Tuple(vec![], span.clone())),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("x".to_string(), span.clone())),
                cond_expr: Some(Box::new(div_zero.clone())),
                grouping: false,
            }),
            span.clone(),
        );
        assert!(partial_eval(&for_cond, &ctx).is_err());
    }

    // partial_eval_body errors
    let empty_body = Body {
        attributes: HashMap::new(),
        blocks: vec![],
        functions: vec![],
        dynamic_blocks: vec![],
        validations: vec![],
        preconditions: vec![],
        postconditions: vec![],
        span: span.clone(),
    };

    // Attribute error
    let mut body_attr_err = empty_body.clone();
    body_attr_err.attributes.insert(
        "err".to_string(),
        Attribute {
            name: "err".to_string(),
            expr: div_zero.clone(),
            span: span.clone(),
            name_span: span.clone(),
            equals_span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        },
    );
    assert!(partial_eval_body(&body_attr_err, &ctx).is_err());

    // Block error
    let mut body_block_err = empty_body.clone();
    body_block_err.blocks.push(Block {
        block_type: "b".to_string(),
        labels: vec![],
        body: body_attr_err.clone(),
        span: span.clone(),
        type_span: span.clone(),
        label_spans: vec![],
        open_brace_span: span.clone(),
        close_brace_span: span.clone(),
        leading_comments: Vec::new(),
        trailing_comment: None,
    });
    assert!(partial_eval_body(&body_block_err, &ctx).is_err());

    // DynamicBlock for_each error
    let mut body_dyn_err = empty_body.clone();
    body_dyn_err.dynamic_blocks.push(DynamicBlock {
        block_type: "d".to_string(),
        for_each: div_zero.clone(),
        iterator: None,
        labels: None,
        content: empty_body.clone(),
        span: span.clone(),
        type_span: span.clone(),
    });
    assert!(partial_eval_body(&body_dyn_err, &ctx).is_err());

    // DynamicBlock content error
    let mut body_dyn_content_err = empty_body.clone();
    body_dyn_content_err.dynamic_blocks.push(DynamicBlock {
        block_type: "d".to_string(),
        for_each: Expression::Tuple(vec![], span.clone()),
        iterator: None,
        labels: None,
        content: body_attr_err.clone(),
        span: span.clone(),
        type_span: span.clone(),
    });
    assert!(partial_eval_body(&body_dyn_content_err, &ctx).is_err());

    // Validation condition error
    let mut body_val_cond_err = empty_body.clone();
    body_val_cond_err.validations.push(ValidationBlock {
        condition: div_zero.clone(),
        error_message: Expression::String("msg".to_string(), span.clone()),
        span: span.clone(),
    });
    assert!(partial_eval_body(&body_val_cond_err, &ctx).is_err());

    // Validation error_message error
    let mut body_val_msg_err = empty_body.clone();
    body_val_msg_err.validations.push(ValidationBlock {
        condition: Expression::Bool(true, span.clone()),
        error_message: div_zero.clone(),
        span: span.clone(),
    });
    assert!(partial_eval_body(&body_val_msg_err, &ctx).is_err());

    // Precondition condition error
    let mut body_pre_cond_err = empty_body.clone();
    body_pre_cond_err.preconditions.push(PreconditionBlock {
        condition: div_zero.clone(),
        error_message: Expression::String("msg".to_string(), span.clone()),
        span: span.clone(),
    });
    assert!(partial_eval_body(&body_pre_cond_err, &ctx).is_err());

    // Precondition error_message error
    let mut body_pre_msg_err = empty_body.clone();
    body_pre_msg_err.preconditions.push(PreconditionBlock {
        condition: Expression::Bool(true, span.clone()),
        error_message: div_zero.clone(),
        span: span.clone(),
    });
    assert!(partial_eval_body(&body_pre_msg_err, &ctx).is_err());

    // Postcondition condition error
    let mut body_post_cond_err = empty_body.clone();
    body_post_cond_err.postconditions.push(PostconditionBlock {
        condition: div_zero.clone(),
        error_message: Expression::String("msg".to_string(), span.clone()),
        span: span.clone(),
    });
    assert!(partial_eval_body(&body_post_cond_err, &ctx).is_err());

    // Postcondition error_message error
    let mut body_post_msg_err = empty_body.clone();
    body_post_msg_err.postconditions.push(PostconditionBlock {
        condition: Expression::Bool(true, span.clone()),
        error_message: div_zero,
        span: span.clone(),
    });
    assert!(partial_eval_body(&body_post_msg_err, &ctx).is_err());
}

#[test]
fn test_partial_eval_uncovered_branches() {
    use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
    use std::sync::Arc;

    let span = empty_span();
    let mut ctx = Context::new();

    // 1. Line 364: Traversal on root variable in context that evaluates to Unknown
    let mut obj_map = std::collections::BTreeMap::new();
    obj_map.insert("field".to_string(), Type::String);
    ctx.set_variable("unk_obj", Value::unknown(Type::object(obj_map)));
    let trav_unk = Expression::Traversal(
        Box::new(Traversal {
            expr: Box::new(Expression::Variable("unk_obj".to_string(), span.clone())),
            operators: vec![TraversalOperator::GetAttr(
                "field".to_string(),
                span.clone(),
            )],
        }),
        span.clone(),
    );
    let folded_trav = match partial_eval(&trav_unk, &ctx) {
        Ok(e) => e,
        Err(e) => panic!("partial eval failed: {e:?}"),
    };
    assert!(matches!(folded_trav, Expression::Traversal(..)));

    // 2. Line 405: Deterministic function call with non-literal (unknown) argument
    let func = Function::new("my_det", Arc::new(|args| Ok(args[0].clone()))).with_signature(
        FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("a", Type::String)],
            Type::String,
        )
        .with_deterministic(true),
    );
    ctx.set_function("my_det", func);
    let fn_call_non_lit = Expression::FuncCall(
        Box::new(FuncCall {
            name: "my_det".into(),
            args: vec![Expression::Variable(
                "missing_var".to_string(),
                span.clone(),
            )],
            expand_final: false,
        }),
        span.clone(),
    );
    let folded_fn_non_lit = match partial_eval(&fn_call_non_lit, &ctx) {
        Ok(e) => e,
        Err(e) => panic!("partial eval failed: {e:?}"),
    };
    assert!(matches!(folded_fn_non_lit, Expression::FuncCall(..)));

    // Line 405: Registered non-deterministic (impure) function call (func.is_deterministic() is false)
    let func_impure = Function::new("my_impure", Arc::new(|_| Ok(Value::null(Type::String))))
        .with_signature(
            FunctionSignature::with_static_return_type(vec![], Type::String)
                .with_deterministic(false),
        );
    ctx.set_function("my_impure", func_impure);
    let fn_call_impure = Expression::FuncCall(
        Box::new(FuncCall {
            name: "my_impure".into(),
            args: vec![],
            expand_final: false,
        }),
        span.clone(),
    );
    let folded_impure = match partial_eval(&fn_call_impure, &ctx) {
        Ok(e) => e,
        Err(e) => panic!("partial eval failed: {e:?}"),
    };
    assert!(matches!(folded_impure, Expression::FuncCall(..)));

    // 3. Line 419: Deterministic function call with literal args returning unknown Value
    let func_returns_unk = Function::new("ret_unk", Arc::new(|_| Ok(Value::unknown(Type::String))))
        .with_signature(
            FunctionSignature::with_static_return_type(
                vec![FunctionParamSpec::new("a", Type::String)],
                Type::String,
            )
            .with_deterministic(true),
        );
    ctx.set_function("ret_unk", func_returns_unk);
    let fn_call_ret_unk = Expression::FuncCall(
        Box::new(FuncCall {
            name: "ret_unk".into(),
            args: vec![Expression::String("hello".to_string(), span.clone())],
            expand_final: false,
        }),
        span.clone(),
    );
    let folded_fn_ret_unk = match partial_eval(&fn_call_ret_unk, &ctx) {
        Ok(e) => e,
        Err(e) => panic!("partial eval failed: {e:?}"),
    };
    assert!(matches!(folded_fn_ret_unk, Expression::FuncCall(..)));

    // 4. Line 464: ForExpr evaluating to an unknown value
    ctx.set_variable(
        "unknown_list",
        Value::unknown(Type::List(Box::new(Type::String))),
    );
    let for_unk = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "item".to_string(),
            collection: Box::new(Expression::Variable(
                "unknown_list".to_string(),
                span.clone(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("item".to_string(), span.clone())),
            cond_expr: None,
            grouping: false,
        }),
        span,
    );
    let folded_for = match partial_eval(&for_unk, &ctx) {
        Ok(e) => e,
        Err(e) => panic!("partial eval failed: {e:?}"),
    };
    assert!(matches!(folded_for, Expression::ForExpr(..)));
}
