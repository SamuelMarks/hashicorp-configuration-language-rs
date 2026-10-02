//! User-Defined Function AST.
//!
//! Provides the AST node for `function "name" { ... }` definitions.

use crate::ast::expr::Expression;
use crate::ast::type_expr::TypeExpr;
use crate::span::Span;

/// A parameter definition in a user-defined function.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionParam {
    /// The name of the parameter.
    pub name: String,
    /// The optional type constraint for the parameter.
    pub type_expr: Option<TypeExpr>,
    /// The source span of the parameter definition.
    pub span: Span,
}

/// An HCL `function` block definition.
///
/// Represents a user-defined function parsed from HCL configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionBlock {
    /// The name of the function.
    pub name: String,
    /// The parameters accepted by the function.
    pub params: Vec<FunctionParam>,
    /// The optional variadic parameter collecting extra arguments.
    pub variadic_param: Option<FunctionParam>,
    /// The optional return type constraint.
    pub return_type: Option<TypeExpr>,
    /// The expression representing the function body/result.
    pub body: Expression,
    /// The total source span of the function block.
    pub span: Span,
}

impl FunctionBlock {
    /// Creates a new `FunctionBlock`.
    #[must_use]
    pub fn new(
        name: String,
        params: Vec<FunctionParam>,
        return_type: Option<TypeExpr>,
        body: Expression,
        span: Span,
    ) -> Self {
        Self {
            name,
            params,
            variadic_param: None,
            return_type,
            body,
            span,
        }
    }

    /// Sets the variadic parameter for this function block.
    ///
    /// # Arguments
    /// * `variadic` - The variadic parameter definition.
    #[must_use]
    pub fn with_variadic_param(mut self, variadic: Option<FunctionParam>) -> Self {
        self.variadic_param = variadic;
        self
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::pedantic,
        clippy::nursery
    )]

    use super::*;
    use crate::number::Number;
    use std::str::FromStr;

    #[test]
    fn test_function_block_new() {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let params = vec![FunctionParam {
            name: "x".to_string(),
            type_expr: None,
            span: span.clone(),
        }];
        let body = Expression::Number(Number::from_str("42").unwrap(), span.clone());

        let fb = FunctionBlock::new("test_func".to_string(), params, None, body, span.clone())
            .with_variadic_param(Some(FunctionParam {
                name: "rest".to_string(),
                type_expr: None,
                span: span.clone(),
            }));
        assert_eq!(fb.name, "test_func");
        assert_eq!(fb.params.len(), 1);
        assert!(fb.variadic_param.is_some());
        assert_eq!(
            fb.variadic_param.as_ref().map(|p| p.name.as_str()),
            Some("rest")
        );
        assert!(fb.return_type.is_none());
        assert_eq!(fb.span, span);
    }
}
