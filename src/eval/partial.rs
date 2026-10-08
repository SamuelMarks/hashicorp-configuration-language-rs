//! Partial evaluation and expression reduction engine for HCL.
//!
//! Evaluates expressions partially when some variables or functions are known,
//! while preserving unknowns, dynamic traversals, and missing variables for runtime.
//! Performs constant folding, operator reduction, and branch pruning.
use crate::ast::expr::{
    BinaryOp, Conditional, Expression, ForExpr, FuncCall, TemplatePart, Traversal,
    TraversalOperator, UnaryOp,
};
use crate::ast::structure::{
    Attribute, Block, Body, DynamicBlock, PostconditionBlock, PreconditionBlock, ValidationBlock,
};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::eval::context::Context;
use crate::eval::evaluator::Evaluator;
use crate::span::Span;
use crate::types::{Value, ValueData};
use std::collections::HashMap;
/// Converts an evaluated [`Value`] into an equivalent AST [`Expression`].
///
/// # Arguments
/// * `val` - The value to convert.
/// * `span` - The source span to attach to the generated AST node.
///
/// # Examples
/// ```rust
/// use hashicorp_configuration_language_rs::eval::partial::value_to_expression;
/// use hashicorp_configuration_language_rs::ast::expr::Expression;
/// use hashicorp_configuration_language_rs::span::Span;
/// use hashicorp_configuration_language_rs::types::{Type, Value, ValueData};
///
/// let val = Value::new(Type::String, ValueData::String("hello".into()));
/// let expr = value_to_expression(&val, Span::new(0, 5, 1, 1, 1, 6));
/// assert!(matches!(expr, Expression::String(..)));
/// ```
#[must_use]
pub fn value_to_expression(val: &Value, span: Span) -> Expression {
    match &*val.data {
        ValueData::Null => Expression::Null(span),
        ValueData::Bool(b) => Expression::Bool(*b, span),
        ValueData::Number(n) => Expression::Number(n.clone(), span),
        ValueData::String(s) => Expression::String(s.clone(), span),
        ValueData::Array(arr) => {
            let exprs: Vec<Expression> = arr
                .iter()
                .map(|item| value_to_expression(item, span.clone()))
                .collect();
            Expression::Tuple(exprs, span)
        }
        ValueData::Set(set) => {
            let exprs: Vec<Expression> = set
                .iter()
                .map(|item| value_to_expression(item, span.clone()))
                .collect();
            Expression::Tuple(exprs, span)
        }
        ValueData::Object(obj) => {
            let kvs = obj
                .iter()
                .map(|(k, v)| {
                    (
                        Expression::String(k.clone(), span.clone()),
                        value_to_expression(v, span.clone()),
                    )
                })
                .collect();
            Expression::Object(kvs, span)
        }
        ValueData::Unknown(_) | ValueData::Capsule(_) => {
            Expression::Variable("__unknown__".to_string(), span)
        }
    }
}
/// Checks whether an expression represents a known static literal value.
#[must_use]
fn is_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::Null(_)
            | Expression::Bool(_, _)
            | Expression::Number(_, _)
            | Expression::String(_, _)
    )
}
/// Partially evaluates an [`Expression`], constant-folding operations where inputs are known.
///
/// # Arguments
/// * `expr` - The expression to partially evaluate.
/// * `ctx` - The evaluation context containing known variables and functions.
///
/// # Errors
/// Returns [`Diagnostics`] if a critical evaluation error occurs (e.g. division by zero).
///
/// # Examples
/// ```rust
/// use hashicorp_configuration_language_rs::eval::context::Context;
/// use hashicorp_configuration_language_rs::eval::partial::partial_eval;
/// use hashicorp_configuration_language_rs::ast::expr::{BinaryOp, Expression};
/// use hashicorp_configuration_language_rs::number::Number;
/// use hashicorp_configuration_language_rs::span::Span;
///
/// let ctx = Context::new();
/// let span = Span::new(0, 0, 0, 0, 0, 0);
/// let expr = Expression::BinaryOp(
///     BinaryOp::Add,
///     Box::new(Expression::Number(Number::from(2), span.clone())),
///     Box::new(Expression::Number(Number::from(3), span.clone())),
///     span,
/// );
/// let folded = partial_eval(&expr, &ctx).unwrap();
/// if let Expression::Number(n, _) = folded {
///     assert_eq!(n, Number::from(5));
/// } else {
///     panic!("expected folded number");
/// }
/// ```
pub fn partial_eval(expr: &Expression, ctx: &Context) -> Result<Expression, Diagnostics> {
    match expr {
        Expression::Null(span) => Ok(Expression::Null(span.clone())),
        Expression::Bool(b, span) => Ok(Expression::Bool(*b, span.clone())),
        Expression::Number(n, span) => Ok(Expression::Number(n.clone(), span.clone())),
        Expression::String(s, span) => Ok(Expression::String(s.clone(), span.clone())),
        Expression::Parentheses(inner, span) => {
            let folded = partial_eval(inner, ctx)?;
            if is_literal(&folded) {
                Ok(folded)
            } else {
                Ok(Expression::Parentheses(Box::new(folded), span.clone()))
            }
        }
        Expression::Variable(name, span) => {
            if let Some(val) = ctx.get_variable(name) {
                if !val.is_unknown() {
                    return Ok(value_to_expression(val, span.clone()));
                }
            }
            Ok(Expression::Variable(name.clone(), span.clone()))
        }
        Expression::UnaryOp(op, inner, span) => {
            let folded_inner = partial_eval(inner, ctx)?;
            match (op, &folded_inner) {
                (UnaryOp::Not, Expression::Bool(b, _)) => Ok(Expression::Bool(!b, span.clone())),
                (UnaryOp::Neg, Expression::Number(n, _)) => {
                    Ok(Expression::Number(-n.clone(), span.clone()))
                }
                _ => Ok(Expression::UnaryOp(
                    *op,
                    Box::new(folded_inner),
                    span.clone(),
                )),
            }
        }
        Expression::BinaryOp(op, left, right, span) => {
            let folded_left = partial_eval(left, ctx)?;
            if *op == BinaryOp::And {
                if let Expression::Bool(false, _) = folded_left {
                    return Ok(Expression::Bool(false, span.clone()));
                }
                if let Expression::Bool(true, _) = folded_left {
                    return partial_eval(right, ctx);
                }
            } else if *op == BinaryOp::Or {
                if let Expression::Bool(true, _) = folded_left {
                    return Ok(Expression::Bool(true, span.clone()));
                }
                if let Expression::Bool(false, _) = folded_left {
                    return partial_eval(right, ctx);
                }
            }
            let folded_right = partial_eval(right, ctx)?;
            if *op == BinaryOp::And {
                if let Expression::Bool(true, _) = folded_right {
                    return Ok(folded_left);
                }
            } else if *op == BinaryOp::Or {
                if let Expression::Bool(false, _) = folded_right {
                    return Ok(folded_left);
                }
            }
            if let (Expression::Number(n1, _), Expression::Number(n2, _)) =
                (&folded_left, &folded_right)
            {
                match op {
                    BinaryOp::Add => {
                        return Ok(Expression::Number(n1.clone() + n2.clone(), span.clone()));
                    }
                    BinaryOp::Sub => {
                        return Ok(Expression::Number(n1.clone() - n2.clone(), span.clone()));
                    }
                    BinaryOp::Mul => {
                        return Ok(Expression::Number(n1.clone() * n2.clone(), span.clone()));
                    }
                    BinaryOp::Div => {
                        if let Some(res) = n1.checked_div(n2) {
                            return Ok(Expression::Number(res, span.clone()));
                        }
                        let mut diags = Diagnostics::new();
                        diags.push(Diagnostic::error(
                            "Division by zero".to_string(),
                            "Cannot divide by zero in constant expression.".to_string(),
                            span.clone(),
                        ));
                        return Err(diags);
                    }
                    BinaryOp::Mod => {
                        if let Some(res) = n1.checked_rem(n2) {
                            return Ok(Expression::Number(res, span.clone()));
                        }
                        let mut diags = Diagnostics::new();
                        diags.push(Diagnostic::error(
                            "Modulo by zero".to_string(),
                            "Cannot calculate modulo with divisor of zero.".to_string(),
                            span.clone(),
                        ));
                        return Err(diags);
                    }
                    BinaryOp::Eq => {
                        return Ok(Expression::Bool(n1 == n2, span.clone()));
                    }
                    BinaryOp::NotEq => {
                        return Ok(Expression::Bool(n1 != n2, span.clone()));
                    }
                    BinaryOp::Less => {
                        return Ok(Expression::Bool(n1 < n2, span.clone()));
                    }
                    BinaryOp::LessEq => {
                        return Ok(Expression::Bool(n1 <= n2, span.clone()));
                    }
                    BinaryOp::Greater => {
                        return Ok(Expression::Bool(n1 > n2, span.clone()));
                    }
                    BinaryOp::GreaterEq => {
                        return Ok(Expression::Bool(n1 >= n2, span.clone()));
                    }
                    _ => {}
                }
            }
            if let (Expression::Bool(b1, _), Expression::Bool(b2, _)) =
                (&folded_left, &folded_right)
            {
                match op {
                    BinaryOp::Eq => return Ok(Expression::Bool(b1 == b2, span.clone())),
                    BinaryOp::NotEq => {
                        return Ok(Expression::Bool(b1 != b2, span.clone()));
                    }
                    _ => {}
                }
            }
            if let (Expression::String(s1, _), Expression::String(s2, _)) =
                (&folded_left, &folded_right)
            {
                match op {
                    BinaryOp::Eq => return Ok(Expression::Bool(s1 == s2, span.clone())),
                    BinaryOp::NotEq => {
                        return Ok(Expression::Bool(s1 != s2, span.clone()));
                    }
                    _ => {}
                }
            }
            Ok(Expression::BinaryOp(
                *op,
                Box::new(folded_left),
                Box::new(folded_right),
                span.clone(),
            ))
        }
        Expression::Conditional(cond, span) => {
            let folded_cond = partial_eval(&cond.cond_expr, ctx)?;
            match folded_cond {
                Expression::Bool(true, _) => partial_eval(&cond.true_expr, ctx),
                Expression::Bool(false, _) => partial_eval(&cond.false_expr, ctx),
                _ => {
                    let folded_true = partial_eval(&cond.true_expr, ctx)?;
                    let folded_false = partial_eval(&cond.false_expr, ctx)?;
                    if folded_true == folded_false {
                        Ok(folded_true)
                    } else {
                        Ok(Expression::Conditional(
                            Box::new(Conditional {
                                cond_expr: folded_cond,
                                true_expr: folded_true,
                                false_expr: folded_false,
                            }),
                            span.clone(),
                        ))
                    }
                }
            }
        }
        Expression::Tuple(elements, span) => {
            let mut folded_elements = Vec::with_capacity(elements.len());
            for elem in elements {
                folded_elements.push(partial_eval(elem, ctx)?);
            }
            Ok(Expression::Tuple(folded_elements, span.clone()))
        }
        Expression::Object(elements, span) => {
            let mut folded_elements = Vec::with_capacity(elements.len());
            for (k, v) in elements {
                let folded_k = partial_eval(k, ctx)?;
                let folded_v = partial_eval(v, ctx)?;
                folded_elements.push((folded_k, folded_v));
            }
            Ok(Expression::Object(folded_elements, span.clone()))
        }
        Expression::Template(parts, span) => {
            let mut folded_parts = Vec::with_capacity(parts.len());
            let mut combined = String::new();
            let mut all_literals = true;
            for part in parts {
                match part {
                    TemplatePart::Literal(s, part_span) => {
                        combined.push_str(s);
                        folded_parts.push(TemplatePart::Literal(s.clone(), part_span.clone()));
                    }
                    TemplatePart::Interpolation(inner, part_span) => {
                        let folded_inner = partial_eval(inner, ctx)?;
                        if let Expression::String(s, _) = &folded_inner {
                            combined.push_str(s);
                            folded_parts.push(TemplatePart::Literal(s.clone(), part_span.clone()));
                        } else {
                            all_literals = false;
                            folded_parts
                                .push(TemplatePart::Interpolation(folded_inner, part_span.clone()));
                        }
                    }
                    TemplatePart::Directive(dir, part_span) => {
                        all_literals = false;
                        folded_parts.push(TemplatePart::Directive(dir.clone(), part_span.clone()));
                    }
                }
            }
            if all_literals {
                Ok(Expression::String(combined, span.clone()))
            } else {
                Ok(Expression::Template(folded_parts, span.clone()))
            }
        }
        Expression::Traversal(trav, span) => {
            if let Expression::Variable(ref root_name, _) = *trav.expr {
                if ctx.get_variable(root_name).is_some() {
                    let eval_res = Evaluator::new(ctx).evaluate(expr);
                    if let Ok((val, _)) = eval_res {
                        if !val.is_unknown() {
                            return Ok(value_to_expression(&val, span.clone()));
                        }
                    }
                }
            }
            let folded_base = partial_eval(&trav.expr, ctx)?;
            let mut folded_ops = Vec::with_capacity(trav.operators.len());
            for op in &trav.operators {
                match op {
                    TraversalOperator::Index(idx_expr, op_span) => {
                        let folded_idx = partial_eval(idx_expr, ctx)?;
                        folded_ops.push(TraversalOperator::Index(folded_idx, op_span.clone()));
                    }
                    _ => folded_ops.push(op.clone()),
                }
            }
            Ok(Expression::Traversal(
                Box::new(Traversal {
                    expr: Box::new(folded_base),
                    operators: folded_ops,
                }),
                span.clone(),
            ))
        }
        Expression::FuncCall(fc, span) => {
            let mut folded_args = Vec::with_capacity(fc.args.len());
            let mut all_args_literal = true;
            for arg in &fc.args {
                let folded_arg = partial_eval(arg, ctx)?;
                if !is_literal(&folded_arg) {
                    all_args_literal = false;
                }
                folded_args.push(folded_arg);
            }
            if let Some(func) = ctx.get_namespaced_func(&fc.name) {
                if func.is_deterministic() && all_args_literal && !fc.expand_final {
                    let synthetic_call = Expression::FuncCall(
                        Box::new(FuncCall {
                            name: fc.name.clone(),
                            args: folded_args.clone(),
                            expand_final: false,
                        }),
                        span.clone(),
                    );
                    let eval_res = Evaluator::new(ctx).evaluate(&synthetic_call);
                    if let Ok((val, _)) = eval_res {
                        if !val.is_unknown() {
                            return Ok(value_to_expression(&val, span.clone()));
                        }
                    }
                }
            }
            Ok(Expression::FuncCall(
                Box::new(FuncCall {
                    name: fc.name.clone(),
                    args: folded_args,
                    expand_final: fc.expand_final,
                }),
                span.clone(),
            ))
        }
        Expression::ForExpr(for_expr, span) => {
            let folded_coll = partial_eval(&for_expr.collection, ctx)?;
            let folded_val = partial_eval(&for_expr.val_expr, ctx)?;
            let folded_key = if let Some(ref k) = for_expr.key_expr {
                Some(Box::new(partial_eval(k, ctx)?))
            } else {
                None
            };
            let folded_cond = if let Some(ref c) = for_expr.cond_expr {
                Some(Box::new(partial_eval(c, ctx)?))
            } else {
                None
            };
            let synthetic_for = Expression::ForExpr(
                Box::new(ForExpr {
                    key_var: for_expr.key_var.clone(),
                    val_var: for_expr.val_var.clone(),
                    collection: Box::new(folded_coll.clone()),
                    key_expr: folded_key.clone(),
                    val_expr: Box::new(folded_val.clone()),
                    cond_expr: folded_cond.clone(),
                    grouping: for_expr.grouping,
                }),
                span.clone(),
            );
            let eval_res = Evaluator::new(ctx).evaluate(&synthetic_for);
            if let Ok((val, _)) = eval_res {
                if !val.is_unknown() {
                    return Ok(value_to_expression(&val, span.clone()));
                }
            }
            Ok(Expression::ForExpr(
                Box::new(ForExpr {
                    key_var: for_expr.key_var.clone(),
                    val_var: for_expr.val_var.clone(),
                    collection: Box::new(folded_coll),
                    key_expr: folded_key,
                    val_expr: Box::new(folded_val),
                    cond_expr: folded_cond,
                    grouping: for_expr.grouping,
                }),
                span.clone(),
            ))
        }
    }
}
/// Partially evaluates an entire AST [`Body`], simplifying all attribute expressions and inner blocks.
///
/// # Arguments
/// * `body` - The AST body to partially evaluate.
/// * `ctx` - The evaluation context containing known variables and functions.
///
/// # Errors
/// Returns [`Diagnostics`] if a critical evaluation error occurs.
///
/// # Examples
/// ```rust
/// use hashicorp_configuration_language_rs::eval::context::Context;
/// use hashicorp_configuration_language_rs::eval::partial::partial_eval_body;
/// use hashicorp_configuration_language_rs::api::parse;
///
/// let src = "a = 3";
/// let body = parse(src).unwrap();
/// let ctx = Context::new();
/// let reduced = partial_eval_body(&body, &ctx).unwrap();
/// assert!(reduced.attributes.contains_key("a"));
/// ```
pub fn partial_eval_body(body: &Body, ctx: &Context) -> Result<Body, Diagnostics> {
    let mut new_attributes = HashMap::new();
    for (name, attr) in &body.attributes {
        let folded_expr = partial_eval(&attr.expr, ctx)?;
        new_attributes.insert(
            name.clone(),
            Attribute {
                name: attr.name.clone(),
                expr: folded_expr,
                span: attr.span.clone(),
                name_span: attr.name_span.clone(),
                equals_span: attr.equals_span.clone(),
                leading_comments: attr.leading_comments.clone(),
                trailing_comment: attr.trailing_comment.clone(),
            },
        );
    }
    let mut new_blocks = Vec::with_capacity(body.blocks.len());
    for block in &body.blocks {
        let folded_inner_body = partial_eval_body(&block.body, ctx)?;
        new_blocks.push(Block {
            block_type: block.block_type.clone(),
            labels: block.labels.clone(),
            body: folded_inner_body,
            span: block.span.clone(),
            type_span: block.type_span.clone(),
            label_spans: block.label_spans.clone(),
            open_brace_span: block.open_brace_span.clone(),
            close_brace_span: block.close_brace_span.clone(),
            leading_comments: block.leading_comments.clone(),
            trailing_comment: block.trailing_comment.clone(),
        });
    }
    let mut new_dyn_blocks = Vec::with_capacity(body.dynamic_blocks.len());
    for dyn_b in &body.dynamic_blocks {
        let folded_for_each = partial_eval(&dyn_b.for_each, ctx)?;
        let folded_content = partial_eval_body(&dyn_b.content, ctx)?;
        new_dyn_blocks.push(DynamicBlock {
            block_type: dyn_b.block_type.clone(),
            for_each: folded_for_each,
            iterator: dyn_b.iterator.clone(),
            labels: dyn_b.labels.clone(),
            content: folded_content,
            span: dyn_b.span.clone(),
            type_span: dyn_b.type_span.clone(),
        });
    }
    let mut new_validations = Vec::with_capacity(body.validations.len());
    for v in &body.validations {
        let folded_cond = partial_eval(&v.condition, ctx)?;
        let folded_err = partial_eval(&v.error_message, ctx)?;
        new_validations.push(ValidationBlock {
            condition: folded_cond,
            error_message: folded_err,
            span: v.span.clone(),
        });
    }
    let mut new_preconditions = Vec::with_capacity(body.preconditions.len());
    for p in &body.preconditions {
        let folded_cond = partial_eval(&p.condition, ctx)?;
        let folded_err = partial_eval(&p.error_message, ctx)?;
        new_preconditions.push(PreconditionBlock {
            condition: folded_cond,
            error_message: folded_err,
            span: p.span.clone(),
        });
    }
    let mut new_postconditions = Vec::with_capacity(body.postconditions.len());
    for p in &body.postconditions {
        let folded_cond = partial_eval(&p.condition, ctx)?;
        let folded_err = partial_eval(&p.error_message, ctx)?;
        new_postconditions.push(PostconditionBlock {
            condition: folded_cond,
            error_message: folded_err,
            span: p.span.clone(),
        });
    }
    Ok(Body {
        attributes: new_attributes,
        blocks: new_blocks,
        functions: body.functions.clone(),
        dynamic_blocks: new_dyn_blocks,
        validations: new_validations,
        preconditions: new_preconditions,
        postconditions: new_postconditions,
        span: body.span.clone(),
    })
}
