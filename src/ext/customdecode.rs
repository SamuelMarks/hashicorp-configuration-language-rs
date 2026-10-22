//! Custom Expression Interception Hooks.
//!
//! This module provides traits and helper types for deferred evaluation and static inspection
//! of HCL AST expressions without triggering standard dynamic evaluation.
//! This maps directly to the `ext/customdecode` package in `HashiCorp`'s HCL Go repository.

use crate::ast::expr::{Directive, Expression, TemplatePart};
use crate::diagnostic::Diagnostics;
use crate::eval::context::Context;

/// A trait for intercepting HCL expressions before standard structural decoding and evaluation.
///
/// Types implementing this trait can manually inspect the raw `Expression` AST node and its
/// associated evaluation context, enabling partial evaluation, speculative evaluation, or
/// extracting static configuration elements (like bare variable names).
pub trait CustomDecodeExpression: Sized {
    /// Decode an HCL expression directly.
    ///
    /// The implementation can emit both warnings and errors by returning a `Diagnostics`
    /// containing multiple messages. If a fatal error occurs, it should return an `Err(Diagnostics)`.
    /// Otherwise, it should return an `Ok(Self)` representing the decoded result.
    ///
    /// # Errors
    /// Returns `Diagnostics` if the custom decoding logic encounters an error
    /// evaluating the expression.
    fn custom_decode_expr(expr: &Expression, ctx: &mut Context<'_>) -> Result<Self, Diagnostics>;
}

/// A wrapper type that lazily captures an expression and its context for deferred execution.
///
/// Similar to `hcl.ExpressionSpy` in the upstream `HashiCorp` implementation, this captures
/// the raw structure.
#[derive(Debug, Clone)]
pub struct ExpressionSpy {
    /// The raw unevaluated AST expression.
    pub expr: Expression,
}

impl ExpressionSpy {
    /// Create a new `ExpressionSpy` from an expression.
    #[must_use]
    pub const fn new(expr: Expression) -> Self {
        Self { expr }
    }
}

impl CustomDecodeExpression for ExpressionSpy {
    fn custom_decode_expr(expr: &Expression, _ctx: &mut Context<'_>) -> Result<Self, Diagnostics> {
        Ok(Self::new(expr.clone()))
    }
}

/// Static inspector for un-evaluated expressions.
///
/// Extracts static references, literal values, and variable identifiers from an expression
/// without risking runtime evaluation errors on unknown variables.
#[derive(Debug)]
pub struct StaticExpressionInspector;

impl StaticExpressionInspector {
    /// Extract top-level referenced variable names (e.g. `var.foo`, `local.bar`)
    /// from an expression without evaluating it.
    #[must_use]
    pub fn extract_referenced_variables(expr: &Expression) -> Vec<String> {
        let mut vars = Vec::new();
        // Traverse expression and extract traversal roots.
        Self::walk_expr(expr, &mut vars);
        vars.sort();
        vars.dedup();
        vars
    }

    fn walk_expr(expr: &Expression, vars: &mut Vec<String>) {
        match expr {
            Expression::Variable(ident, _) => {
                vars.push(ident.clone());
            }
            Expression::Traversal(traversal, _) => {
                // The root of a traversal is usually a Variable, we walk it
                Self::walk_expr(&traversal.expr, vars);
            }
            Expression::FuncCall(fc, _) => {
                for arg in &fc.args {
                    Self::walk_expr(arg, vars);
                }
            }
            Expression::Tuple(tuple, _) => {
                for item in tuple {
                    Self::walk_expr(item, vars);
                }
            }
            Expression::Object(obj, _) => {
                for (k, v) in obj {
                    Self::walk_expr(k, vars);
                    Self::walk_expr(v, vars);
                }
            }
            Expression::BinaryOp(_, lhs, rhs, _) => {
                Self::walk_expr(lhs, vars);
                Self::walk_expr(rhs, vars);
            }
            Expression::UnaryOp(_, expr, _) | Expression::Parentheses(expr, _) => {
                Self::walk_expr(expr, vars);
            }
            Expression::Conditional(cond, _) => {
                Self::walk_expr(&cond.cond_expr, vars);
                Self::walk_expr(&cond.true_expr, vars);
                Self::walk_expr(&cond.false_expr, vars);
            }
            Expression::ForExpr(for_expr, _) => {
                Self::walk_expr(&for_expr.collection, vars);
                Self::walk_expr(&for_expr.val_expr, vars);
                if let Some(key_expr) = &for_expr.key_expr {
                    Self::walk_expr(key_expr, vars);
                }
                if let Some(cond) = &for_expr.cond_expr {
                    Self::walk_expr(cond, vars);
                }
            }
            Expression::Template(parts, _) => {
                Self::walk_template(parts, vars);
            }
            _ => {}
        }
    }

    fn walk_template(parts: &[TemplatePart], vars: &mut Vec<String>) {
        for part in parts {
            match part {
                TemplatePart::Interpolation(expr, _) => {
                    Self::walk_expr(expr, vars);
                }
                TemplatePart::Directive(dir, _) => match dir {
                    Directive::If {
                        cond,
                        true_expr,
                        else_ifs,
                        false_expr,
                    } => {
                        Self::walk_expr(cond, vars);
                        Self::walk_template(true_expr, vars);
                        for (c, t) in else_ifs {
                            Self::walk_expr(c, vars);
                            Self::walk_template(t, vars);
                        }
                        if let Some(f) = false_expr {
                            Self::walk_template(f, vars);
                        }
                    }
                    Directive::For {
                        collection, body, ..
                    } => {
                        Self::walk_expr(collection, vars);
                        Self::walk_template(body, vars);
                    }
                    Directive::Strip { .. } => {}
                },
                TemplatePart::Literal(..) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::expr::BinaryOp;
    use crate::error::HclError;
    use crate::eval::context::Context;
    use crate::span::Span;

    #[test]
    fn test_expression_spy() -> Result<(), HclError> {
        let span = Span::new(0, 0, 1, 1, 1, 5);
        let expr = Expression::Variable("foo".to_string(), span);
        let spy = ExpressionSpy::new(expr.clone());
        assert_eq!(spy.expr, expr);

        let mut ctx = Context::new();
        let decoded = ExpressionSpy::custom_decode_expr(&expr, &mut ctx)?;
        assert_eq!(decoded.expr, expr);
        Ok(())
    }

    #[test]
    fn test_walk_expr_comprehensive() {
        use crate::ast::expr::*;
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let var1 = Expression::Variable("v1".into(), span.clone());
        let var2 = Expression::Variable("v2".into(), span.clone());
        let var3 = Expression::Variable("v3".into(), span.clone());
        let var4 = Expression::Variable("v4".into(), span.clone());
        let var5 = Expression::Variable("v5".into(), span.clone());
        let var6 = Expression::Variable("v6".into(), span.clone());

        let trav = Expression::Traversal(
            Box::new(Traversal {
                expr: Box::new(var1.clone()),
                operators: vec![],
            }),
            span.clone(),
        );
        let fncall = Expression::FuncCall(
            Box::new(FuncCall {
                name: "f".into(),
                args: vec![var2.clone()],
                expand_final: false,
            }),
            span.clone(),
        );
        let tup = Expression::Tuple(vec![trav, fncall], span.clone());
        let obj = Expression::Object(vec![(var3.clone(), tup)], span.clone());
        let null_expr = Expression::Null(span.clone());
        let un = Expression::UnaryOp(UnaryOp::Not, Box::new(obj), span.clone());
        let paren = Expression::Parentheses(Box::new(un), span.clone());
        let cond = Expression::Conditional(
            Box::new(Conditional {
                cond_expr: paren,
                true_expr: var4.clone(),
                false_expr: var5.clone(),
            }),
            span.clone(),
        );
        let for_e = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "x".into(),
                collection: Box::new(cond),
                key_expr: Some(Box::new(var6.clone())),
                val_expr: Box::new(var1.clone()),
                cond_expr: Some(Box::new(var2.clone())),
                grouping: false,
            }),
            span.clone(),
        );
        let tpl = Expression::Template(
            vec![
                TemplatePart::Literal("a".into(), span.clone()),
                TemplatePart::Interpolation(null_expr, span.clone()),
                TemplatePart::Interpolation(for_e, span.clone()),
                TemplatePart::Directive(
                    Directive::If {
                        cond: var3.clone(),
                        true_expr: vec![TemplatePart::Interpolation(var4, span.clone())],
                        else_ifs: vec![(
                            var5,
                            vec![TemplatePart::Interpolation(var6, span.clone())],
                        )],
                        false_expr: Some(vec![TemplatePart::Interpolation(var1, span.clone())]),
                    },
                    span.clone(),
                ),
                TemplatePart::Directive(
                    Directive::For {
                        key_var: None,
                        val_var: "y".into(),
                        collection: var2,
                        body: vec![TemplatePart::Interpolation(var3, span.clone())],
                    },
                    span.clone(),
                ),
                TemplatePart::Directive(
                    Directive::Strip {
                        strip_left: false,
                        strip_right: false,
                    },
                    span.clone(),
                ),
            ],
            span,
        );
        let extracted = StaticExpressionInspector::extract_referenced_variables(&tpl);
        assert_eq!(extracted.len(), 6);
    }

    #[test]
    fn test_walk_expr_simple_template() {
        use crate::ast::expr::*;
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let for_simple = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".into(),
                collection: Box::new(Expression::Variable("coll".into(), span.clone())),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("val".into(), span.clone())),
                cond_expr: None,
                grouping: false,
            }),
            span.clone(),
        );
        let tpl_no_else = Expression::Template(
            vec![
                TemplatePart::Interpolation(for_simple, span.clone()),
                TemplatePart::Directive(
                    Directive::If {
                        cond: Expression::Variable("cond_v".into(), span.clone()),
                        true_expr: vec![],
                        else_ifs: vec![],
                        false_expr: None,
                    },
                    span.clone(),
                ),
            ],
            span,
        );
        let extracted_simple =
            StaticExpressionInspector::extract_referenced_variables(&tpl_no_else);
        assert_eq!(extracted_simple.len(), 3);
    }
    #[test]
    fn test_static_expression_inspector_variables() {
        let span = Span::new(0, 0, 1, 1, 1, 5);
        let var_expr = Expression::Variable("var".to_string(), span.clone());
        let local_expr = Expression::Variable("local".to_string(), span.clone());

        let binary_expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(var_expr),
            Box::new(local_expr),
            span,
        );

        let extracted = StaticExpressionInspector::extract_referenced_variables(&binary_expr);
        assert_eq!(extracted, vec!["local".to_string(), "var".to_string()]);
    }
}
