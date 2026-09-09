//! AST Traversal primitives for absolute and relative traversals.
//!
//! Provides [`AbsTraversal`] and [`RelTraversal`] representing static chains
//! of attribute lookups, index lookups, and splats starting either from a root
//! variable identifier or an arbitrary sub-expression.

use crate::ast::expr::{Expression, Traversal, TraversalOperator};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::error::HclError;
use crate::span::Span;
use std::fmt;

/// An absolute traversal starting with a root variable identifier (`ident.attr[0]`).
///
/// In HCL, an absolute traversal begins with a named root identifier (such as
/// a variable name or block reference) followed by zero or more attribute accesses,
/// index lookups, or splat operations.
#[derive(Debug, Clone, PartialEq)]
pub struct AbsTraversal {
    /// The root identifier name (e.g., `"var"`, `"local"`, or `"resource"`).
    pub root: String,
    /// The source span of the root identifier.
    pub root_span: Span,
    /// The sequence of traversal operators applied to the root identifier.
    pub operators: Vec<TraversalOperator>,
    /// The overall source span covering the entire traversal.
    pub span: Span,
}

impl AbsTraversal {
    /// Creates a new `AbsTraversal`.
    ///
    /// # Arguments
    /// * `root` - The root variable identifier.
    /// * `root_span` - The source span of the root identifier.
    /// * `operators` - The operators applied to the root identifier.
    /// * `span` - The overall source span covering root and operators.
    #[must_use]
    pub fn new(
        root: String,
        root_span: Span,
        operators: Vec<TraversalOperator>,
        span: Span,
    ) -> Self {
        Self {
            root,
            root_span,
            operators,
            span,
        }
    }

    /// Returns the root identifier string.
    #[must_use]
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Returns the source span of the root identifier.
    #[must_use]
    pub fn root_span(&self) -> Span {
        self.root_span.clone()
    }

    /// Returns the traversal operators.
    #[must_use]
    pub fn operators(&self) -> &[TraversalOperator] {
        &self.operators
    }

    /// Returns the overall source span of the traversal.
    #[must_use]
    pub fn span(&self) -> Span {
        self.span.clone()
    }

    /// Returns `true` if this absolute traversal has no operators and consists solely of the root variable.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.operators.is_empty()
    }

    /// Converts this `AbsTraversal` back into an AST [`Expression`].
    #[must_use]
    pub fn to_expression(&self) -> Expression {
        if self.operators.is_empty() {
            Expression::Variable(self.root.clone(), self.root_span.clone())
        } else {
            Expression::Traversal(
                Box::new(Traversal {
                    expr: Box::new(Expression::Variable(
                        self.root.clone(),
                        self.root_span.clone(),
                    )),
                    operators: self.operators.clone(),
                }),
                self.span.clone(),
            )
        }
    }

    /// Converts an AST [`Traversal`] with a given span into an `AbsTraversal`.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if the traversal does not originate from a root variable identifier.
    pub fn from_traversal(trav: &Traversal, span: Span) -> Result<Self, Diagnostics> {
        let base_abs = abs_traversal_for_expr(&trav.expr)?;
        let mut ops = base_abs.operators;
        ops.extend(trav.operators.clone());
        Ok(Self::new(base_abs.root, base_abs.root_span, ops, span))
    }

    /// Converts an [`Expression`] into an `AbsTraversal`.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if the expression is not a variable reference or valid absolute traversal.
    pub fn from_expr(expr: &Expression) -> Result<Self, Diagnostics> {
        abs_traversal_for_expr(expr)
    }

    /// Splits this absolute traversal into a static prefix traversal and the remaining operators.
    ///
    /// The static prefix consists of the root identifier and all leading [`TraversalOperator::GetAttr`]
    /// operations up until the first dynamic index, legacy index, or splat operator.
    ///
    /// # Returns
    /// A tuple `(static_prefix, remainder)` where `static_prefix` is an [`AbsTraversal`]
    /// containing only static attribute lookups, and `remainder` contains any trailing operators.
    #[must_use]
    pub fn split_static_prefix(&self) -> (AbsTraversal, Vec<TraversalOperator>) {
        let mut prefix_ops = Vec::new();
        let mut remainder = Vec::new();
        let mut in_remainder = false;

        for op in &self.operators {
            if in_remainder {
                remainder.push(op.clone());
            } else if let TraversalOperator::GetAttr(..) = op {
                prefix_ops.push(op.clone());
            } else {
                in_remainder = true;
                remainder.push(op.clone());
            }
        }

        let prefix_span = if prefix_ops.is_empty() {
            self.root_span.clone()
        } else {
            let last_op_span = prefix_ops
                .last()
                .map_or(self.root_span.clone(), super::expr::TraversalOperator::span);
            Span::new(
                self.root_span.start_byte,
                last_op_span.end_byte,
                self.root_span.start_line,
                self.root_span.start_col,
                last_op_span.end_line,
                last_op_span.end_col,
            )
        };

        (
            AbsTraversal::new(
                self.root.clone(),
                self.root_span.clone(),
                prefix_ops,
                prefix_span,
            ),
            remainder,
        )
    }
}

impl fmt::Display for AbsTraversal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.root)?;
        for op in &self.operators {
            match op {
                TraversalOperator::AttrSplat(_) => write!(f, ".*")?,
                TraversalOperator::FullSplat(_) => write!(f, "[*]")?,
                TraversalOperator::GetAttr(attr, _) => write!(f, ".{attr}")?,
                TraversalOperator::Index(idx_expr, _) => match idx_expr {
                    Expression::String(s, _) => write!(f, "[{s:?}]")?,
                    Expression::Number(n, _) => write!(f, "[{}]", n.0)?,
                    _ => write!(f, "[...]")?,
                },
                TraversalOperator::LegacyIndex(idx, _) => write!(f, ".{idx}")?,
            }
        }
        Ok(())
    }
}

/// A relative traversal starting from an arbitrary sub-expression (`(a + b).attr[0]`).
///
/// Unlike [`AbsTraversal`], a relative traversal does not require a root identifier
/// and can chain traversal operators off any complex expression.
#[derive(Debug, Clone, PartialEq)]
pub struct RelTraversal {
    /// The starting expression that the operators apply to.
    pub expr: Box<Expression>,
    /// The sequence of traversal operators applied to the starting expression.
    pub operators: Vec<TraversalOperator>,
    /// The overall source span covering expression and operators.
    pub span: Span,
}

impl RelTraversal {
    /// Creates a new `RelTraversal`.
    ///
    /// # Arguments
    /// * `expr` - The base expression.
    /// * `operators` - The operators applied to the base expression.
    /// * `span` - The source span covering the relative traversal.
    #[must_use]
    pub fn new(expr: Box<Expression>, operators: Vec<TraversalOperator>, span: Span) -> Self {
        Self {
            expr,
            operators,
            span,
        }
    }

    /// Returns the base expression.
    #[must_use]
    pub fn expr(&self) -> &Expression {
        &self.expr
    }

    /// Returns the traversal operators.
    #[must_use]
    pub fn operators(&self) -> &[TraversalOperator] {
        &self.operators
    }

    /// Returns the overall source span of the relative traversal.
    #[must_use]
    pub fn span(&self) -> Span {
        self.span.clone()
    }

    /// Converts this `RelTraversal` back into an AST [`Expression`].
    #[must_use]
    pub fn to_expression(&self) -> Expression {
        if self.operators.is_empty() {
            *self.expr.clone()
        } else {
            Expression::Traversal(
                Box::new(Traversal {
                    expr: self.expr.clone(),
                    operators: self.operators.clone(),
                }),
                self.span.clone(),
            )
        }
    }

    /// Converts an AST [`Traversal`] with a given span into a `RelTraversal`.
    ///
    /// # Errors
    /// This function succeeds unconditionally for any AST traversal.
    pub fn from_traversal(trav: &Traversal, span: Span) -> Result<Self, Diagnostics> {
        Ok(Self::new(trav.expr.clone(), trav.operators.clone(), span))
    }

    /// Converts an [`Expression`] into a `RelTraversal`.
    ///
    /// # Errors
    /// This function succeeds unconditionally for any expression.
    pub fn from_expr(expr: &Expression) -> Result<Self, Diagnostics> {
        rel_traversal_for_expr(expr)
    }
}

impl fmt::Display for RelTraversal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(...)")?;
        for op in &self.operators {
            match op {
                TraversalOperator::AttrSplat(_) => write!(f, ".*")?,
                TraversalOperator::FullSplat(_) => write!(f, "[*]")?,
                TraversalOperator::GetAttr(attr, _) => write!(f, ".{attr}")?,
                TraversalOperator::Index(idx_expr, _) => match idx_expr {
                    Expression::String(s, _) => write!(f, "[{s:?}]")?,
                    Expression::Number(n, _) => write!(f, "[{}]", n.0)?,
                    _ => write!(f, "[...]")?,
                },
                TraversalOperator::LegacyIndex(idx, _) => write!(f, ".{idx}")?,
            }
        }
        Ok(())
    }
}

/// Converts an AST [`Expression`] into an [`AbsTraversal`].
///
/// # Arguments
/// * `expr` - The expression to convert.
///
/// # Errors
/// Returns [`Diagnostics`] if the expression is not rooted in a variable identifier.
pub fn abs_traversal_for_expr(expr: &Expression) -> Result<AbsTraversal, Diagnostics> {
    match expr {
        Expression::Variable(name, span) => Ok(AbsTraversal::new(
            name.clone(),
            span.clone(),
            Vec::new(),
            span.clone(),
        )),
        Expression::Traversal(trav, span) => {
            let base = abs_traversal_for_expr(&trav.expr)?;
            let mut ops = base.operators;
            ops.extend(trav.operators.clone());
            Ok(AbsTraversal::new(
                base.root,
                base.root_span,
                ops,
                span.clone(),
            ))
        }
        Expression::Parentheses(inner, _) => abs_traversal_for_expr(inner),
        _ => {
            let diag = Diagnostic::new(
                HclError::Traversal(
                    "Expression does not begin with a root variable identifier.".to_string(),
                ),
                expr.span(),
            );
            Err(Diagnostics::from(diag))
        }
    }
}

/// Converts an AST [`Expression`] into a [`RelTraversal`].
///
/// # Arguments
/// * `expr` - The expression to convert.
///
/// # Errors
/// This conversion is always infallible, wrapping non-traversal expressions as the base expression.
pub fn rel_traversal_for_expr(expr: &Expression) -> Result<RelTraversal, Diagnostics> {
    match expr {
        Expression::Traversal(trav, span) => Ok(RelTraversal::new(
            trav.expr.clone(),
            trav.operators.clone(),
            span.clone(),
        )),
        _ => Ok(RelTraversal::new(
            Box::new(expr.clone()),
            Vec::new(),
            expr.span(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::number::Number;

    #[test]
    fn test_abs_traversal_basic() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let var_expr = Expression::Variable("foo".to_string(), span.clone());
        let abs = abs_traversal_for_expr(&var_expr).expect("valid abs traversal");
        assert_eq!(abs.root(), "foo");
        assert_eq!(abs.root_span(), span);
        assert!(abs.is_empty());
        assert_eq!(abs.span(), span);
        assert_eq!(abs.operators().len(), 0);
        assert_eq!(abs.to_string(), "foo");

        let reconverted = abs.to_expression();
        assert_eq!(reconverted, var_expr);
    }

    #[test]
    fn test_abs_traversal_complex() {
        let span = Span::new(0, 20, 1, 1, 1, 21);
        let var_span = Span::new(0, 3, 1, 1, 1, 4);
        let op1 = TraversalOperator::GetAttr("bar".to_string(), span.clone());
        let num = Number::from(0);
        let op2 = TraversalOperator::Index(Expression::Number(num, span.clone()), span.clone());
        let op3 = TraversalOperator::AttrSplat(span.clone());
        let op4 = TraversalOperator::FullSplat(span.clone());
        let op5 = TraversalOperator::LegacyIndex(1, span.clone());
        let op6 = TraversalOperator::Index(
            Expression::String("key".to_string(), span.clone()),
            span.clone(),
        );
        let op7 = TraversalOperator::Index(Expression::Bool(true, span.clone()), span.clone());

        let trav = Traversal {
            expr: Box::new(Expression::Variable("foo".to_string(), var_span.clone())),
            operators: vec![
                op1.clone(),
                op2.clone(),
                op3.clone(),
                op4.clone(),
                op5.clone(),
                op6.clone(),
                op7.clone(),
            ],
        };

        let expr = Expression::Traversal(Box::new(trav.clone()), span.clone());
        let abs = AbsTraversal::from_expr(&expr).expect("valid abs traversal");
        assert_eq!(abs.root(), "foo");
        assert_eq!(abs.root_span(), var_span);
        assert!(!abs.is_empty());
        assert_eq!(abs.operators().len(), 7);
        assert_eq!(abs.to_string(), r#"foo.bar[0].*[*].1["key"][...]"#);

        let abs_from_trav = AbsTraversal::from_traversal(&trav, span).expect("valid abs");
        assert_eq!(abs, abs_from_trav);

        let expr_back = abs.to_expression();
        assert_eq!(expr_back, expr);
    }

    #[test]
    fn test_abs_traversal_nested_and_parentheses() {
        let span = Span::new(0, 15, 1, 1, 1, 16);
        let inner_trav = Traversal {
            expr: Box::new(Expression::Variable("foo".to_string(), span.clone())),
            operators: vec![TraversalOperator::GetAttr("bar".to_string(), span.clone())],
        };
        let paren = Expression::Parentheses(
            Box::new(Expression::Traversal(Box::new(inner_trav), span.clone())),
            span.clone(),
        );
        let outer_trav = Traversal {
            expr: Box::new(paren),
            operators: vec![TraversalOperator::GetAttr("baz".to_string(), span.clone())],
        };
        let outer_expr = Expression::Traversal(Box::new(outer_trav), span);

        let abs = AbsTraversal::from_expr(&outer_expr).expect("valid abs traversal");
        assert_eq!(abs.root(), "foo");
        assert_eq!(abs.operators().len(), 2);
        assert_eq!(abs.to_string(), "foo.bar.baz");
    }

    #[test]
    fn test_abs_traversal_errors() {
        let span = Span::new(0, 5, 1, 1, 1, 6);
        let lit_expr = Expression::Number(Number::from(42), span.clone());
        assert!(abs_traversal_for_expr(&lit_expr).is_err());

        let bad_trav = Traversal {
            expr: Box::new(lit_expr),
            operators: vec![TraversalOperator::GetAttr("bar".to_string(), span.clone())],
        };
        assert!(AbsTraversal::from_traversal(&bad_trav, span.clone()).is_err());
        let bad_trav_expr = Expression::Traversal(Box::new(bad_trav), span);
        let err = abs_traversal_for_expr(&bad_trav_expr);
        assert!(err.is_err());
    }

    struct StepWriter {
        remaining: usize,
    }

    impl std::fmt::Write for StepWriter {
        fn write_str(&mut self, _: &str) -> std::fmt::Result {
            if self.remaining == 0 {
                Err(std::fmt::Error)
            } else {
                self.remaining = self.remaining.saturating_sub(1);
                Ok(())
            }
        }
    }

    #[test]
    fn test_traversal_display_error_paths() {
        let span = Span::new(0, 5, 1, 1, 1, 6);
        let ops = vec![
            TraversalOperator::AttrSplat(span.clone()),
            TraversalOperator::FullSplat(span.clone()),
            TraversalOperator::GetAttr("x".to_string(), span.clone()),
            TraversalOperator::Index(
                Expression::String("s".to_string(), span.clone()),
                span.clone(),
            ),
            TraversalOperator::Index(
                Expression::Number(Number::from(1), span.clone()),
                span.clone(),
            ),
            TraversalOperator::Index(Expression::Null(span.clone()), span.clone()),
            TraversalOperator::LegacyIndex(0, span.clone()),
        ];

        for op in ops {
            let abs = AbsTraversal::new(
                "root".to_string(),
                span.clone(),
                vec![op.clone()],
                span.clone(),
            );
            let mut w0 = StepWriter { remaining: 0 };
            assert!(std::fmt::write(&mut w0, format_args!("{abs}")).is_err());
            let mut w1 = StepWriter { remaining: 1 };
            assert!(std::fmt::write(&mut w1, format_args!("{abs}")).is_err());

            let rel = RelTraversal::new(
                Box::new(Expression::Variable("v".to_string(), span.clone())),
                vec![op],
                span.clone(),
            );
            let mut rw0 = StepWriter { remaining: 0 };
            assert!(std::fmt::write(&mut rw0, format_args!("{rel}")).is_err());
            let mut rw1 = StepWriter { remaining: 1 };
            assert!(std::fmt::write(&mut rw1, format_args!("{rel}")).is_err());
        }
    }

    #[test]
    fn test_rel_traversal() {
        let span = Span::new(0, 15, 1, 1, 1, 16);
        let num = Number::from(10);
        let base_expr = Expression::Number(num, span.clone());

        let rel1 = RelTraversal::from_expr(&base_expr).expect("valid rel");
        assert_eq!(rel1.operators().len(), 0);
        assert_eq!(rel1.expr(), &base_expr);
        assert_eq!(rel1.span(), span);
        assert_eq!(rel1.to_expression(), base_expr);
        assert_eq!(rel1.to_string(), "(...)");

        let op1 = TraversalOperator::GetAttr("attr".to_string(), span.clone());
        let op2 = TraversalOperator::Index(
            Expression::String("k".to_string(), span.clone()),
            span.clone(),
        );
        let op3 = TraversalOperator::Index(
            Expression::Number(Number::from(1), span.clone()),
            span.clone(),
        );
        let op4 = TraversalOperator::AttrSplat(span.clone());
        let op5 = TraversalOperator::FullSplat(span.clone());
        let op6 = TraversalOperator::LegacyIndex(2, span.clone());
        let op7 = TraversalOperator::Index(Expression::Null(span.clone()), span.clone());

        let trav = Traversal {
            expr: Box::new(base_expr.clone()),
            operators: vec![
                op1.clone(),
                op2.clone(),
                op3.clone(),
                op4.clone(),
                op5.clone(),
                op6.clone(),
                op7.clone(),
            ],
        };
        let trav_expr = Expression::Traversal(Box::new(trav.clone()), span.clone());

        let rel2 = RelTraversal::from_traversal(&trav, span.clone()).expect("valid rel");
        assert_eq!(rel2.operators().len(), 7);
        assert_eq!(rel2.expr(), &base_expr);
        assert_eq!(rel2.to_string(), r#"(...).attr["k"][1].*[*].2[...]"#);
        assert_eq!(rel2.to_expression(), trav_expr);

        let rel3 = RelTraversal::from_expr(&trav_expr).expect("valid rel");
        assert_eq!(rel3, rel2);
    }

    #[test]
    fn test_split_static_prefix() {
        let span = Span::new(0, 30, 1, 1, 1, 31);
        let root_span = Span::new(0, 3, 1, 1, 1, 4);
        let op_a = TraversalOperator::GetAttr("servers".to_string(), span.clone());
        let op_b = TraversalOperator::GetAttr("network".to_string(), span.clone());
        let op_splat = TraversalOperator::AttrSplat(span.clone());
        let op_c = TraversalOperator::GetAttr("ip".to_string(), span.clone());

        assert!(op_splat.is_splat());
        assert!(!op_a.is_splat());
        assert_eq!(op_splat.span(), span);

        let abs = AbsTraversal::new(
            "var".to_string(),
            root_span.clone(),
            vec![op_a.clone(), op_b.clone(), op_splat.clone(), op_c.clone()],
            span,
        );

        let (prefix, remainder) = abs.split_static_prefix();
        assert_eq!(prefix.root(), "var");
        assert_eq!(prefix.operators(), &[op_a, op_b]);
        assert_eq!(remainder, &[op_splat, op_c]);

        // Empty operators case
        let abs_empty =
            AbsTraversal::new("local".to_string(), root_span.clone(), vec![], root_span);
        let (prefix_empty, rem_empty) = abs_empty.split_static_prefix();
        assert_eq!(prefix_empty.root(), "local");
        assert!(prefix_empty.is_empty());
        assert_eq!(rem_empty, Vec::new());
    }
}
