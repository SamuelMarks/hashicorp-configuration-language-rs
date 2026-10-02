//! AST expressions and operations in HCL.

#![allow(clippy::all, clippy::pedantic)]

use crate::number::Number;
use crate::span::Span;

/// An operator for a binary expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Addition
    Add,
    /// Subtraction
    Sub,
    /// Multiplication
    Mul,
    /// Division
    Div,
    /// Modulo
    Mod,
    /// Equality
    Eq,
    /// Inequality
    NotEq,
    /// Less than
    Less,
    /// Less than or equal
    LessEq,
    /// Greater than
    Greater,
    /// Greater than or equal
    GreaterEq,
    /// Logical AND
    And,
    /// Logical OR
    Or,
}

/// An operator for a unary expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Logical NOT
    Not,
    /// Negation
    Neg,
}

/// A step in a traversal.
#[derive(Debug, Clone, PartialEq)]
pub enum TraversalOperator {
    /// `.attr_name`
    AttrSplat(Span), // .*
    /// Full splat `[*]`
    FullSplat(Span),
    /// Attribute access `.name`
    GetAttr(String, Span),
    /// Index access `[expr]`
    Index(Expression, Span),
    /// Legacy index access `.0`
    LegacyIndex(u64, Span),
}

impl TraversalOperator {
    /// Returns `true` if this traversal operator is a splat operator ([`Self::AttrSplat`] or [`Self::FullSplat`]).
    #[must_use]
    pub fn is_splat(&self) -> bool {
        matches!(self, Self::AttrSplat(_) | Self::FullSplat(_))
    }

    /// Returns the source [`Span`] covering this traversal operator.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::AttrSplat(span)
            | Self::FullSplat(span)
            | Self::GetAttr(_, span)
            | Self::Index(_, span)
            | Self::LegacyIndex(_, span) => span.clone(),
        }
    }
}

/// A traversal (either starting from a scope variable or relative to an expression).
#[derive(Debug, Clone, PartialEq)]
pub struct Traversal {
    /// The starting expression, usually an identifier (Variable) but could be a complex expression for `RelativeTraversal`.
    pub expr: Box<Expression>,
    /// The operators applied in sequence.
    pub operators: Vec<TraversalOperator>,
}

impl std::fmt::Display for Traversal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = match &*self.expr {
            Expression::Variable(v, _) => v.clone(),
            Expression::Traversal(t, _) => t.to_string(),
            other => format!("({other:?})"),
        };
        for op in &self.operators {
            match op {
                TraversalOperator::AttrSplat(_) => s.push_str(".*"),
                TraversalOperator::FullSplat(_) => s.push_str("[*]"),
                TraversalOperator::GetAttr(attr, _) => {
                    s.push('.');
                    s.push_str(attr);
                }
                TraversalOperator::Index(idx_expr, _) => match idx_expr {
                    Expression::String(str_val, _) => {
                        s.push('[');
                        s.push('"');
                        s.push_str(str_val);
                        s.push('"');
                        s.push(']');
                    }
                    Expression::Number(n, _) => {
                        s.push('[');
                        s.push_str(&n.0.to_string());
                        s.push(']');
                    }
                    Expression::Variable(v, _) => {
                        s.push('[');
                        s.push_str(v);
                        s.push(']');
                    }
                    _ => s.push_str("[...]"),
                },
                TraversalOperator::LegacyIndex(idx, _) => {
                    s.push('.');
                    s.push_str(&idx.to_string());
                }
            }
        }
        write!(f, "{s}")
    }
}

/// A key-value pair in an object constructor.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectKey {
    /// The expression being splatted
    pub expr: Expression,
    // (We could keep track if it's an unquoted identifier vs string but AST expression handles both as expressions or we have a specific struct).
}

/// The components of a `for` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ForExpr {
    /// Optional key variable
    pub key_var: Option<String>,
    /// Value variable
    pub val_var: String,
    /// Collection to iterate over
    pub collection: Box<Expression>,
    /// Optional key expression
    pub key_expr: Option<Box<Expression>>,
    /// Value expression
    pub val_expr: Box<Expression>,
    /// Optional condition expression
    pub cond_expr: Option<Box<Expression>>,
    /// Whether grouping is enabled `...`
    pub grouping: bool,
}

/// An HCL Expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    /// A null literal.
    Null(Span),
    /// A boolean literal.
    Bool(bool, Span),
    /// A number literal.
    Number(Number, Span),
    /// A string literal.
    String(String, Span),

    /// A tuple constructor: `[expr1, expr2, ...]`.
    Tuple(Vec<Expression>, Span),
    /// An object constructor: `{key1 = val1, key2 = val2}`.
    Object(Vec<(Expression, Expression)>, Span),

    /// A string template (including interpolation and directives).
    Template(Vec<TemplatePart>, Span),

    /// A variable reference (e.g., `foo`).
    Variable(String, Span),

    /// A traversal (`foo.bar[0]`).
    Traversal(Box<Traversal>, Span),

    /// A function call (`name(arg1, arg2...)`).
    FuncCall(Box<FuncCall>, Span),

    /// A conditional expression (`cond ? true_expr : false_expr`).
    Conditional(Box<Conditional>, Span),

    /// A binary operation (`expr + expr`).
    BinaryOp(BinaryOp, Box<Expression>, Box<Expression>, Span),

    /// A unary operation (`-expr` or `!expr`).
    UnaryOp(UnaryOp, Box<Expression>, Span),

    /// A `for` expression (tuple `[for k, v in obj: v]` or object `{for k, v in obj: k => v}`).
    ForExpr(Box<ForExpr>, Span),

    /// A Parenthesized expression (to preserve exact spans and precedence).
    Parentheses(Box<Expression>, Span),
}

/// An identifier with optional namespace qualification (e.g. `provider::aws::arn_parse`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamespacedIdent {
    /// The namespace segments preceding the final function/identifier name.
    pub namespace: Vec<String>,
    /// The unqualified name.
    pub name: String,
    /// The source span of the entire namespaced identifier.
    pub span: Span,
}

impl NamespacedIdent {
    /// Creates a new `NamespacedIdent` with explicit namespace segments, name, and span.
    ///
    /// # Arguments
    /// * `namespace` - Sequence of namespace segment identifiers.
    /// * `name` - The base name of the identifier.
    /// * `span` - Source span of the identifier.
    ///
    /// # Examples
    /// ```rust
    /// use hashicorp_configuration_language_rs::ast::expr::NamespacedIdent;
    /// use hashicorp_configuration_language_rs::span::Span;
    ///
    /// let ident = NamespacedIdent::new(vec!["aws".into()], "arn_parse", Span::new(0, 15, 1, 1, 1, 16));
    /// assert_eq!(ident.to_string(), "aws::arn_parse");
    /// ```
    #[must_use]
    pub fn new(namespace: Vec<String>, name: impl Into<String>, span: Span) -> Self {
        Self {
            namespace,
            name: name.into(),
            span,
        }
    }

    /// Creates an unqualified `NamespacedIdent` with no namespace segments.
    ///
    /// # Arguments
    /// * `name` - The base identifier name.
    /// * `span` - Source span.
    ///
    /// # Examples
    /// ```rust
    /// use hashicorp_configuration_language_rs::ast::expr::NamespacedIdent;
    /// use hashicorp_configuration_language_rs::span::Span;
    ///
    /// let ident = NamespacedIdent::simple("upper", Span::new(0, 5, 1, 1, 1, 6));
    /// assert_eq!(ident.to_string(), "upper");
    /// assert!(ident.is_simple());
    /// ```
    #[must_use]
    pub fn simple(name: impl Into<String>, span: Span) -> Self {
        Self {
            namespace: Vec::new(),
            name: name.into(),
            span,
        }
    }

    /// Parses a colon-colon-separated identifier string into a `NamespacedIdent`.
    ///
    /// # Arguments
    /// * `s` - The identifier string, e.g. `"provider::aws::arn_parse"`.
    /// * `span` - Source span to attach.
    ///
    /// # Examples
    /// ```rust
    /// use hashicorp_configuration_language_rs::ast::expr::NamespacedIdent;
    /// use hashicorp_configuration_language_rs::span::Span;
    ///
    /// let ident = NamespacedIdent::parse("provider::aws::arn_parse", Span::new(0, 24, 1, 1, 1, 25));
    /// assert_eq!(ident.namespace, vec!["provider", "aws"]);
    /// assert_eq!(ident.name, "arn_parse");
    /// ```
    #[must_use]
    pub fn parse(s: &str, span: Span) -> Self {
        let parts: Vec<&str> = s.split("::").collect();
        if parts.len() <= 1 {
            Self::simple(s, span)
        } else {
            let namespace = parts[..parts.len() - 1]
                .iter()
                .map(|p| (*p).to_string())
                .collect();
            let name = parts[parts.len() - 1].to_string();
            Self {
                namespace,
                name,
                span,
            }
        }
    }

    /// Returns `true` if this identifier has no namespace segments.
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.namespace.is_empty()
    }
}

impl std::fmt::Display for NamespacedIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.namespace.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}::{}", self.namespace.join("::"), self.name)
        }
    }
}

impl From<String> for NamespacedIdent {
    fn from(s: String) -> Self {
        Self::parse(&s, Span::new(0, 0, 0, 0, 0, 0))
    }
}

impl From<&str> for NamespacedIdent {
    fn from(s: &str) -> Self {
        Self::parse(s, Span::new(0, 0, 0, 0, 0, 0))
    }
}

impl PartialEq<&str> for NamespacedIdent {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

impl PartialEq<str> for NamespacedIdent {
    fn eq(&self, other: &str) -> bool {
        if self.namespace.is_empty() {
            self.name == other
        } else {
            self.to_string() == other
        }
    }
}

impl PartialEq<String> for NamespacedIdent {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<NamespacedIdent> for &str {
    fn eq(&self, other: &NamespacedIdent) -> bool {
        other == *self
    }
}

impl PartialEq<NamespacedIdent> for str {
    fn eq(&self, other: &NamespacedIdent) -> bool {
        other == self
    }
}

impl PartialEq<NamespacedIdent> for String {
    fn eq(&self, other: &NamespacedIdent) -> bool {
        other == self.as_str()
    }
}

/// A function call.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncCall {
    /// Function name (optionally namespaced)
    pub name: NamespacedIdent,
    /// Function arguments
    pub args: Vec<Expression>,
    /// Expand final argument `...`
    pub expand_final: bool,
}

impl FuncCall {
    /// Returns the unqualified name if the function is not namespaced.
    ///
    /// # Examples
    /// ```rust
    /// use hashicorp_configuration_language_rs::ast::expr::FuncCall;
    ///
    /// let fc = FuncCall {
    ///     name: "upper".into(),
    ///     args: vec![],
    ///     expand_final: false,
    /// };
    /// assert_eq!(fc.simple_name(), Some("upper"));
    /// ```
    #[must_use]
    pub fn simple_name(&self) -> Option<&str> {
        if self.name.namespace.is_empty() {
            Some(&self.name.name)
        } else {
            None
        }
    }
}

/// A conditional expression `cond ? true_expr : false_expr`.
#[derive(Debug, Clone, PartialEq)]
pub struct Conditional {
    /// Condition expression
    pub cond_expr: Expression,
    /// True branch expression
    pub true_expr: Expression,
    /// False branch expression
    pub false_expr: Expression,
}

/// A part of a string template.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    /// A literal string portion.
    Literal(String, Span),
    /// An interpolation `${expr}`.
    Interpolation(Expression, Span),
    /// A control directive `%{ if cond }`, etc.
    Directive(Directive, Span),
}

impl TemplatePart {
    /// Returns the source span of the template part.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            TemplatePart::Literal(_, span)
            | TemplatePart::Interpolation(_, span)
            | TemplatePart::Directive(_, span) => span.clone(),
        }
    }
}

/// Template control directives.
#[derive(Debug, Clone, PartialEq)]
pub enum Directive {
    /// If directive
    If {
        /// Condition expression
        cond: Expression,
        /// True branch body
        true_expr: Vec<TemplatePart>,
        /// Optional else-if branches
        else_ifs: Vec<(Expression, Vec<TemplatePart>)>,
        /// Optional false branch body
        false_expr: Option<Vec<TemplatePart>>,
    },
    /// For directive
    For {
        /// Optional key variable
        key_var: Option<String>,
        /// Value variable
        val_var: String,
        /// Collection to iterate
        collection: Expression,
        /// Loop body
        body: Vec<TemplatePart>,
    },
    /// Direct whitespace strip marker directive
    Strip {
        /// Strip whitespace to the left
        strip_left: bool,
        /// Strip whitespace to the right
        strip_right: bool,
    },
}

impl Expression {
    /// Returns the source span of the expression.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Expression::Null(span)
            | Expression::Bool(_, span)
            | Expression::Number(_, span)
            | Expression::String(_, span)
            | Expression::Tuple(_, span)
            | Expression::Object(_, span)
            | Expression::Template(_, span)
            | Expression::Variable(_, span)
            | Expression::Traversal(_, span)
            | Expression::FuncCall(_, span)
            | Expression::Conditional(_, span)
            | Expression::BinaryOp(_, _, _, span)
            | Expression::UnaryOp(_, _, span)
            | Expression::ForExpr(_, span)
            | Expression::Parentheses(_, span) => span.clone(),
        }
    }

    /// Finds the innermost sub-expression at the given spatial [`Position`](crate::span::Position).
    ///
    /// Recursively traverses tuples, objects, traversals, function calls, conditionals,
    /// binary/unary operations, template interpolations, and directives.
    ///
    /// # Arguments
    /// * `pos` - The coordinate position to query.
    #[must_use]
    pub fn expr_at_position(&self, pos: &crate::span::Position) -> Option<&Expression> {
        if !self.span().contains_position(pos) {
            return None;
        }

        match self {
            Expression::Tuple(elements, _) => {
                for elem in elements {
                    if let Some(inner) = elem.expr_at_position(pos) {
                        return Some(inner);
                    }
                }
                Some(self)
            }
            Expression::Object(pairs, _) => {
                for (k, v) in pairs {
                    if let Some(inner) = k.expr_at_position(pos) {
                        return Some(inner);
                    }
                    if let Some(inner) = v.expr_at_position(pos) {
                        return Some(inner);
                    }
                }
                Some(self)
            }
            Expression::Template(parts, _) => {
                for part in parts {
                    match part {
                        TemplatePart::Interpolation(expr, span) => {
                            if span.contains_position(pos) {
                                if let Some(inner) = expr.expr_at_position(pos) {
                                    return Some(inner);
                                }
                                return Some(expr);
                            }
                        }
                        TemplatePart::Directive(directive, span) => {
                            if span.contains_position(pos) {
                                if let Some(inner) = directive_expr_at(directive, pos) {
                                    return Some(inner);
                                }
                            }
                        }
                        TemplatePart::Literal(_, _) => {}
                    }
                }
                Some(self)
            }
            Expression::Traversal(t, _) => {
                for op in &t.operators {
                    if let TraversalOperator::Index(idx_expr, _) = op {
                        if let Some(inner) = idx_expr.expr_at_position(pos) {
                            return Some(inner);
                        }
                    }
                }
                if let Some(inner) = t.expr.expr_at_position(pos) {
                    return Some(inner);
                }
                Some(self)
            }
            Expression::FuncCall(f, _) => {
                for arg in &f.args {
                    if let Some(inner) = arg.expr_at_position(pos) {
                        return Some(inner);
                    }
                }
                Some(self)
            }
            Expression::Conditional(c, _) => {
                if let Some(inner) = c.cond_expr.expr_at_position(pos) {
                    return Some(inner);
                }
                if let Some(inner) = c.true_expr.expr_at_position(pos) {
                    return Some(inner);
                }
                if let Some(inner) = c.false_expr.expr_at_position(pos) {
                    return Some(inner);
                }
                Some(self)
            }
            Expression::BinaryOp(_, lhs, rhs, _) => {
                if let Some(inner) = lhs.expr_at_position(pos) {
                    return Some(inner);
                }
                if let Some(inner) = rhs.expr_at_position(pos) {
                    return Some(inner);
                }
                Some(self)
            }
            Expression::UnaryOp(_, inner, _) => {
                if let Some(child) = inner.expr_at_position(pos) {
                    return Some(child);
                }
                Some(self)
            }
            Expression::ForExpr(f, _) => {
                if let Some(inner) = f.collection.expr_at_position(pos) {
                    return Some(inner);
                }
                if let Some(ref key_expr) = f.key_expr {
                    if let Some(inner) = key_expr.expr_at_position(pos) {
                        return Some(inner);
                    }
                }
                if let Some(inner) = f.val_expr.expr_at_position(pos) {
                    return Some(inner);
                }
                if let Some(ref cond_expr) = f.cond_expr {
                    if let Some(inner) = cond_expr.expr_at_position(pos) {
                        return Some(inner);
                    }
                }
                Some(self)
            }
            Expression::Parentheses(inner, _) => {
                if let Some(child) = inner.expr_at_position(pos) {
                    return Some(child);
                }
                Some(self)
            }
            Expression::Null(_)
            | Expression::Bool(_, _)
            | Expression::Number(_, _)
            | Expression::String(_, _)
            | Expression::Variable(_, _) => Some(self),
        }
    }
}

fn directive_expr_at<'a>(
    directive: &'a Directive,
    pos: &crate::span::Position,
) -> Option<&'a Expression> {
    match directive {
        Directive::If {
            cond,
            true_expr,
            else_ifs,
            false_expr,
        } => {
            if let Some(inner) = cond.expr_at_position(pos) {
                return Some(inner);
            }
            for part in true_expr {
                if let TemplatePart::Interpolation(expr, span) = part {
                    if span.contains_position(pos) {
                        return expr.expr_at_position(pos).or(Some(expr));
                    }
                }
            }
            for (elif_cond, elif_parts) in else_ifs {
                if let Some(inner) = elif_cond.expr_at_position(pos) {
                    return Some(inner);
                }
                for part in elif_parts {
                    if let TemplatePart::Interpolation(expr, span) = part {
                        if span.contains_position(pos) {
                            return expr.expr_at_position(pos).or(Some(expr));
                        }
                    }
                }
            }
            if let Some(false_parts) = false_expr {
                for part in false_parts {
                    if let TemplatePart::Interpolation(expr, span) = part {
                        if span.contains_position(pos) {
                            return expr.expr_at_position(pos).or(Some(expr));
                        }
                    }
                }
            }
            None
        }
        Directive::For {
            collection, body, ..
        } => {
            if let Some(inner) = collection.expr_at_position(pos) {
                return Some(inner);
            }
            for part in body {
                if let TemplatePart::Interpolation(expr, span) = part {
                    if span.contains_position(pos) {
                        return expr.expr_at_position(pos).or(Some(expr));
                    }
                }
            }
            None
        }
        Directive::Strip { .. } => None,
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
    use crate::span::Span;

    #[test]
    fn test_expression_span() {
        let s = Span::new(0, 5, 1, 1, 1, 6);

        let exprs = vec![
            Expression::Null(s.clone()),
            Expression::Bool(true, s.clone()),
            Expression::Number(Number::from(1), s.clone()),
            Expression::String("test".to_string(), s.clone()),
            Expression::Tuple(vec![], s.clone()),
            Expression::Object(vec![], s.clone()),
            Expression::Template(vec![], s.clone()),
            Expression::Variable("var".to_string(), s.clone()),
            Expression::Traversal(
                Box::new(Traversal {
                    expr: Box::new(Expression::Null(s.clone())),
                    operators: vec![],
                }),
                s.clone(),
            ),
            Expression::FuncCall(
                Box::new(FuncCall {
                    name: "f".into(),
                    args: vec![],
                    expand_final: false,
                }),
                s.clone(),
            ),
            Expression::Conditional(
                Box::new(Conditional {
                    cond_expr: Expression::Null(s.clone()),
                    true_expr: Expression::Null(s.clone()),
                    false_expr: Expression::Null(s.clone()),
                }),
                s.clone(),
            ),
            Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::Null(s.clone())),
                Box::new(Expression::Null(s.clone())),
                s.clone(),
            ),
            Expression::UnaryOp(
                UnaryOp::Not,
                Box::new(Expression::Null(s.clone())),
                s.clone(),
            ),
            Expression::ForExpr(
                Box::new(ForExpr {
                    key_var: None,
                    val_var: "v".to_string(),
                    collection: Box::new(Expression::Null(s.clone())),
                    key_expr: None,
                    val_expr: Box::new(Expression::Null(s.clone())),
                    cond_expr: None,
                    grouping: false,
                }),
                s.clone(),
            ),
            Expression::Parentheses(Box::new(Expression::Null(s.clone())), s.clone()),
        ];

        for expr in exprs {
            assert_eq!(expr.span(), s);
        }
    }

    #[test]
    fn test_traversal_operator_methods() {
        let s = Span::new(1, 10, 1, 2, 1, 11);
        let null_expr = Expression::Null(s.clone());

        let attr_splat = TraversalOperator::AttrSplat(s.clone());
        let full_splat = TraversalOperator::FullSplat(s.clone());
        let get_attr = TraversalOperator::GetAttr("attr".to_string(), s.clone());
        let index = TraversalOperator::Index(null_expr.clone(), s.clone());
        let legacy_index = TraversalOperator::LegacyIndex(42, s.clone());

        assert!(attr_splat.is_splat());
        assert!(full_splat.is_splat());
        assert!(!get_attr.is_splat());
        assert!(!index.is_splat());
        assert!(!legacy_index.is_splat());

        assert_eq!(attr_splat.span(), s);
        assert_eq!(full_splat.span(), s);
        assert_eq!(get_attr.span(), s);
        assert_eq!(index.span(), s);
        assert_eq!(legacy_index.span(), s);
    }

    #[test]
    fn test_template_part_span() {
        let s = Span::new(2, 8, 1, 3, 1, 9);
        let lit = TemplatePart::Literal("hello".to_string(), s.clone());
        let interp = TemplatePart::Interpolation(Expression::Null(s.clone()), s.clone());
        let directive = TemplatePart::Directive(
            Directive::Strip {
                strip_left: true,
                strip_right: false,
            },
            s.clone(),
        );

        assert_eq!(lit.span(), s);
        assert_eq!(interp.span(), s);
        assert_eq!(directive.span(), s);
    }

    #[test]
    fn test_expr_types_derives() {
        let s = Span::new(0, 5, 1, 1, 1, 6);
        let key = ObjectKey {
            expr: Expression::Null(s.clone()),
        };
        let key_clone = key.clone();
        assert_eq!(key, key_clone);
        assert_eq!(format!("{key:?}"), format!("{key_clone:?}"));

        let if_dir = Directive::If {
            cond: Expression::Bool(true, s.clone()),
            true_expr: vec![],
            else_ifs: vec![(Expression::Bool(false, s.clone()), vec![])],
            false_expr: Some(vec![]),
        };
        let for_dir = Directive::For {
            key_var: Some("k".to_string()),
            val_var: "v".to_string(),
            collection: Expression::Null(s.clone()),
            body: vec![],
        };
        assert_ne!(if_dir, for_dir);

        let bin_op = BinaryOp::Sub;
        let un_op = UnaryOp::Neg;
        assert_eq!(bin_op, BinaryOp::Sub);
        assert_eq!(un_op, UnaryOp::Neg);
    }

    #[test]
    fn test_expr_at_position_comprehensive() {
        use crate::api::parse;
        use crate::ast::expr::ForExpr;
        use crate::span::Position;

        let src = r#"
            tuple = [10, 20, 30]
            obj = { key = "val" }
            bin = 1 + 2
            un = !false
            cond = true ? 100 : 200
            func = test_fn(arg_a, arg_b)
            trav_idx = var.items[target_idx]
            paren = (42)
            tmpl = "hello %{if true}${inner_name}%{else}%{endif}"
            tmpl_for = "%{for item in collection}${item}%{endfor}"
        "#;
        let mut parsed_bodies = Vec::new();
        for s in [src, "invalid {{{"] {
            if let Ok(b) = parse(s) {
                parsed_bodies.push(b);
            }
        }
        for body in &parsed_bodies {
            for attr in body.attributes.values() {
                let span = attr.expr.span();
                let pos = Position::new(span.start_line, span.start_col, span.start_byte);
                let found = attr.expr.expr_at_position(&pos);
                assert!(found.is_some(), "expr_at_position failed for {}", attr.name);
            }

            // Test position outside returns None
            for first_attr in body.attributes.values().take(1) {
                let pos_outside = Position::new(1, 1, 0);
                assert!(first_attr.expr.expr_at_position(&pos_outside).is_none());
            }
        }

        // Test ForExpr directly (covering collection, key_expr, val_expr, and cond_expr)
        let total_span = Span::new(10, 50, 1, 1, 1, 40);
        let coll_span = Span::new(10, 15, 1, 1, 1, 5);
        let key_span = Span::new(16, 25, 1, 6, 1, 15);
        let val_span = Span::new(26, 35, 1, 16, 1, 25);
        let cond_span = Span::new(36, 45, 1, 26, 1, 35);
        let for_expr = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(Expression::Variable("items".to_string(), coll_span.clone())),
                key_expr: Some(Box::new(Expression::Variable(
                    "k_expr".to_string(),
                    key_span.clone(),
                ))),
                val_expr: Box::new(Expression::Variable("v".to_string(), val_span.clone())),
                cond_expr: Some(Box::new(Expression::Bool(true, cond_span.clone()))),
                grouping: false,
            }),
            total_span.clone(),
        );
        let pos_inside = Position::new(1, 2, 12);
        assert!(for_expr.expr_at_position(&pos_inside).is_some());

        let pos_key = Position::new(1, 10, 20);
        assert_eq!(
            for_expr.expr_at_position(&pos_key),
            Some(&Expression::Variable("k_expr".to_string(), key_span))
        );

        let pos_cond = Position::new(1, 30, 40);
        assert_eq!(
            for_expr.expr_at_position(&pos_cond),
            Some(&Expression::Bool(true, cond_span))
        );

        // Position inside for_expr but between child spans falls through to Some(self)
        let pos_whitespace = Position::new(1, 38, 48);
        assert_eq!(for_expr.expr_at_position(&pos_whitespace), Some(&for_expr));

        // Test ForExpr with None key_expr and None cond_expr
        let for_expr_none = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Variable("items".to_string(), coll_span)),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), val_span)),
                cond_expr: None,
                grouping: false,
            }),
            total_span,
        );
        assert_eq!(
            for_expr_none.expr_at_position(&pos_whitespace),
            Some(&for_expr_none)
        );
    }

    #[test]
    fn test_traversal_display_all_variants() {
        let s = Span::new(0, 5, 1, 1, 1, 6);
        let trav_var = Traversal {
            expr: Box::new(Expression::Variable("base".to_string(), s.clone())),
            operators: vec![
                TraversalOperator::AttrSplat(s.clone()),
                TraversalOperator::FullSplat(s.clone()),
                TraversalOperator::GetAttr("sub".to_string(), s.clone()),
                TraversalOperator::Index(
                    Expression::String("key".to_string(), s.clone()),
                    s.clone(),
                ),
                TraversalOperator::Index(
                    Expression::Number(Number::from(42), s.clone()),
                    s.clone(),
                ),
                TraversalOperator::Index(
                    Expression::Variable("idx".to_string(), s.clone()),
                    s.clone(),
                ),
                TraversalOperator::Index(Expression::Null(s.clone()), s.clone()),
                TraversalOperator::LegacyIndex(7, s.clone()),
            ],
        };
        assert_eq!(
            trav_var.to_string(),
            "base.*[*].sub[\"key\"][42][idx][...].7"
        );

        let nested_trav = Traversal {
            expr: Box::new(Expression::Traversal(Box::new(trav_var), s.clone())),
            operators: vec![TraversalOperator::GetAttr("extra".to_string(), s.clone())],
        };
        assert_eq!(
            nested_trav.to_string(),
            "base.*[*].sub[\"key\"][42][idx][...].7.extra"
        );

        let other_trav = Traversal {
            expr: Box::new(Expression::Tuple(vec![], s.clone())),
            operators: vec![TraversalOperator::GetAttr("prop".to_string(), s.clone())],
        };
        let s_out = other_trav.to_string();
        assert!(s_out.starts_with("(Tuple([]"));
        assert!(s_out.ends_with(")).prop"));
    }

    #[test]
    fn test_namespaced_ident_full_coverage() {
        use std::collections::HashSet;

        let s = Span::new(0, 10, 1, 1, 1, 11);
        let id_simple = NamespacedIdent::simple("my_func", s.clone());
        assert!(id_simple.is_simple());
        assert_eq!(id_simple.to_string(), "my_func");
        assert_eq!(id_simple, "my_func");
        assert_eq!("my_func", id_simple);
        assert!(id_simple == *"my_func");
        assert!(*"my_func" == id_simple);
        assert_eq!(id_simple, String::from("my_func"));
        assert_eq!(String::from("my_func"), id_simple);
        assert_ne!(id_simple, "other");
        assert_ne!("other", id_simple);
        assert!(!(id_simple == *"other"));
        assert!(!(*"other" == id_simple));
        assert_ne!(id_simple, String::from("other"));
        assert_ne!(String::from("other"), id_simple);

        let id_namespaced = NamespacedIdent::parse("provider::aws::arn_parse", s.clone());
        assert!(!id_namespaced.is_simple());
        assert_eq!(id_namespaced.to_string(), "provider::aws::arn_parse");
        assert_eq!(id_namespaced, "provider::aws::arn_parse");
        assert_eq!("provider::aws::arn_parse", id_namespaced);
        assert!(id_namespaced == *"provider::aws::arn_parse");
        assert!(*"provider::aws::arn_parse" == id_namespaced);
        assert_eq!(id_namespaced, String::from("provider::aws::arn_parse"));
        assert_eq!(String::from("provider::aws::arn_parse"), id_namespaced);
        assert_ne!(id_namespaced, "provider::aws::other");
        assert_ne!("provider::aws::other", id_namespaced);
        assert!(!(id_namespaced == *"provider::aws::other"));
        assert!(!(*"provider::aws::other" == id_namespaced));
        assert_ne!(id_namespaced, String::from("provider::aws::other"));
        assert_ne!(String::from("provider::aws::other"), id_namespaced);

        // From<String> and From<&str>
        let from_string = NamespacedIdent::from(String::from("ns::func"));
        assert_eq!(from_string.namespace, vec!["ns".to_string()]);
        assert_eq!(from_string.name, "func");

        let from_str = NamespacedIdent::from("ns::func");
        assert_eq!(from_str, from_string);

        // FuncCall simple_name
        let fc_simple = FuncCall {
            name: id_simple.clone(),
            args: vec![],
            expand_final: false,
        };
        assert_eq!(fc_simple.simple_name(), Some("my_func"));

        let fc_namespaced = FuncCall {
            name: id_namespaced.clone(),
            args: vec![],
            expand_final: false,
        };
        assert_eq!(fc_namespaced.simple_name(), None);

        // Hash
        let mut set = HashSet::new();
        set.insert(id_simple.clone());
        assert!(set.contains(&id_simple));
    }

    #[test]
    fn test_all_operators_derives() {
        let ops = vec![
            BinaryOp::Add,
            BinaryOp::Sub,
            BinaryOp::Mul,
            BinaryOp::Div,
            BinaryOp::Mod,
            BinaryOp::Eq,
            BinaryOp::NotEq,
            BinaryOp::Less,
            BinaryOp::LessEq,
            BinaryOp::Greater,
            BinaryOp::GreaterEq,
            BinaryOp::And,
            BinaryOp::Or,
        ];
        for (i, op) in ops.iter().enumerate() {
            assert_eq!(*op, op.clone());
            assert_eq!(format!("{op:?}"), format!("{op:?}"));
            for (j, other) in ops.iter().enumerate() {
                if i == j {
                    assert_eq!(op, other);
                } else {
                    assert_ne!(op, other);
                }
            }
        }

        let uops = vec![UnaryOp::Not, UnaryOp::Neg];
        assert_eq!(uops[0], uops[0].clone());
        assert_eq!(uops[1], uops[1].clone());
        assert_ne!(uops[0], uops[1]);
        assert_eq!(format!("{:?}", uops[0]), "Not");
        assert_eq!(format!("{:?}", uops[1]), "Neg");
    }

    #[test]
    fn test_expr_at_position_branches() {
        use crate::span::Position;

        // Tuple
        let tup_span = Span::new(0, 50, 1, 1, 1, 51);
        let elem1 = Expression::Number(Number::from(1), Span::new(10, 20, 1, 11, 1, 21));
        let elem2 = Expression::Number(Number::from(2), Span::new(25, 35, 1, 26, 1, 36));
        let tuple = Expression::Tuple(vec![elem1.clone(), elem2.clone()], tup_span.clone());

        let pos_elem1 = Position::new(1, 15, 15);
        assert_eq!(tuple.expr_at_position(&pos_elem1), Some(&elem1));
        let pos_tup_pad = Position::new(1, 2, 2);
        assert_eq!(tuple.expr_at_position(&pos_tup_pad), Some(&tuple));

        // Object
        let obj_span = Span::new(0, 60, 1, 1, 1, 61);
        let key = Expression::String("k".to_string(), Span::new(5, 15, 1, 6, 1, 16));
        let val = Expression::String("v".to_string(), Span::new(20, 30, 1, 21, 1, 31));
        let obj = Expression::Object(vec![(key.clone(), val.clone())], obj_span.clone());

        let pos_key = Position::new(1, 10, 10);
        assert_eq!(obj.expr_at_position(&pos_key), Some(&key));
        let pos_val = Position::new(1, 25, 25);
        assert_eq!(obj.expr_at_position(&pos_val), Some(&val));
        let pos_obj_pad = Position::new(1, 2, 2);
        assert_eq!(obj.expr_at_position(&pos_obj_pad), Some(&obj));

        // Traversal
        let trav_span = Span::new(0, 50, 1, 1, 1, 51);
        let base_var = Expression::Variable("v".to_string(), Span::new(0, 10, 1, 1, 1, 11));
        let idx_sub = Expression::Number(Number::from(5), Span::new(20, 25, 1, 21, 1, 26));
        let trav = Expression::Traversal(
            Box::new(Traversal {
                expr: Box::new(base_var.clone()),
                operators: vec![TraversalOperator::Index(
                    idx_sub.clone(),
                    Span::new(15, 30, 1, 16, 1, 31),
                )],
            }),
            trav_span.clone(),
        );

        let pos_idx = Position::new(1, 22, 22);
        assert_eq!(trav.expr_at_position(&pos_idx), Some(&idx_sub));
        let pos_base = Position::new(1, 5, 5);
        assert_eq!(trav.expr_at_position(&pos_base), Some(&base_var));
        let pos_trav_pad = Position::new(1, 12, 12);
        assert_eq!(trav.expr_at_position(&pos_trav_pad), Some(&trav));

        // FuncCall
        let fc_span = Span::new(0, 40, 1, 1, 1, 41);
        let arg = Expression::String("arg".to_string(), Span::new(10, 20, 1, 11, 1, 21));
        let fc = Expression::FuncCall(
            Box::new(FuncCall {
                name: NamespacedIdent::simple("fn_name", Span::new(0, 7, 1, 1, 1, 8)),
                args: vec![arg.clone()],
                expand_final: false,
            }),
            fc_span.clone(),
        );
        let pos_arg = Position::new(1, 15, 15);
        assert_eq!(fc.expr_at_position(&pos_arg), Some(&arg));
        let pos_fc_pad = Position::new(1, 8, 8);
        assert_eq!(fc.expr_at_position(&pos_fc_pad), Some(&fc));

        // Conditional
        let cond_span = Span::new(0, 60, 1, 1, 1, 61);
        let c_cond = Expression::Bool(true, Span::new(0, 10, 1, 1, 1, 11));
        let c_true = Expression::Number(Number::from(1), Span::new(15, 25, 1, 16, 1, 26));
        let c_false = Expression::Number(Number::from(2), Span::new(30, 40, 1, 31, 1, 41));
        let cond = Expression::Conditional(
            Box::new(Conditional {
                cond_expr: c_cond.clone(),
                true_expr: c_true.clone(),
                false_expr: c_false.clone(),
            }),
            cond_span.clone(),
        );
        let pos_c_cond = Position::new(1, 5, 5);
        assert_eq!(cond.expr_at_position(&pos_c_cond), Some(&c_cond));
        let pos_c_true = Position::new(1, 20, 20);
        assert_eq!(cond.expr_at_position(&pos_c_true), Some(&c_true));
        let pos_c_false = Position::new(1, 35, 35);
        assert_eq!(cond.expr_at_position(&pos_c_false), Some(&c_false));
        let pos_cond_pad = Position::new(1, 12, 12);
        assert_eq!(cond.expr_at_position(&pos_cond_pad), Some(&cond));

        // BinaryOp
        let bin_span = Span::new(0, 40, 1, 1, 1, 41);
        let b_lhs = Expression::Number(Number::from(1), Span::new(0, 10, 1, 1, 1, 11));
        let b_rhs = Expression::Number(Number::from(2), Span::new(20, 30, 1, 21, 1, 31));
        let bin = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(b_lhs.clone()),
            Box::new(b_rhs.clone()),
            bin_span.clone(),
        );
        let pos_b_lhs = Position::new(1, 5, 5);
        assert_eq!(bin.expr_at_position(&pos_b_lhs), Some(&b_lhs));
        let pos_b_rhs = Position::new(1, 25, 25);
        assert_eq!(bin.expr_at_position(&pos_b_rhs), Some(&b_rhs));
        let pos_bin_pad = Position::new(1, 15, 15);
        assert_eq!(bin.expr_at_position(&pos_bin_pad), Some(&bin));

        // UnaryOp
        let un_span = Span::new(0, 20, 1, 1, 1, 21);
        let u_inner = Expression::Bool(false, Span::new(5, 15, 1, 6, 1, 16));
        let un = Expression::UnaryOp(UnaryOp::Not, Box::new(u_inner.clone()), un_span.clone());
        let pos_u_inner = Position::new(1, 10, 10);
        assert_eq!(un.expr_at_position(&pos_u_inner), Some(&u_inner));
        let pos_un_pad = Position::new(1, 2, 2);
        assert_eq!(un.expr_at_position(&pos_un_pad), Some(&un));

        // Parentheses
        let par_span = Span::new(0, 20, 1, 1, 1, 21);
        let p_inner = Expression::Number(Number::from(42), Span::new(5, 15, 1, 6, 1, 16));
        let par = Expression::Parentheses(Box::new(p_inner.clone()), par_span.clone());
        let pos_p_inner = Position::new(1, 10, 10);
        assert_eq!(par.expr_at_position(&pos_p_inner), Some(&p_inner));
        let pos_par_pad = Position::new(1, 2, 2);
        assert_eq!(par.expr_at_position(&pos_par_pad), Some(&par));

        // ForExpr branches
        let for_span = Span::new(0, 100, 1, 1, 1, 101);
        let coll = Expression::Variable("coll".to_string(), Span::new(10, 20, 1, 11, 1, 21));
        let key_e = Expression::Variable("k".to_string(), Span::new(25, 35, 1, 26, 1, 36));
        let val_e = Expression::Variable("v".to_string(), Span::new(40, 50, 1, 41, 1, 51));
        let cond_e = Expression::Bool(true, Span::new(55, 65, 1, 56, 1, 66));
        let full_for = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(coll.clone()),
                key_expr: Some(Box::new(key_e.clone())),
                val_expr: Box::new(val_e.clone()),
                cond_expr: Some(Box::new(cond_e.clone())),
                grouping: false,
            }),
            for_span.clone(),
        );
        let pos_coll = Position::new(1, 15, 15);
        assert_eq!(full_for.expr_at_position(&pos_coll), Some(&coll));
        let pos_key_e = Position::new(1, 30, 30);
        assert_eq!(full_for.expr_at_position(&pos_key_e), Some(&key_e));
        let pos_val_e = Position::new(1, 45, 45);
        assert_eq!(full_for.expr_at_position(&pos_val_e), Some(&val_e));
        let pos_cond_e = Position::new(1, 60, 60);
        assert_eq!(full_for.expr_at_position(&pos_cond_e), Some(&cond_e));
        let pos_for_pad = Position::new(1, 2, 2);
        assert_eq!(full_for.expr_at_position(&pos_for_pad), Some(&full_for));

        // Leaf variants
        let s_leaf = Span::new(0, 10, 1, 1, 1, 11);
        let pos_leaf = Position::new(1, 5, 5);
        let null_e = Expression::Null(s_leaf.clone());
        let bool_e = Expression::Bool(true, s_leaf.clone());
        let num_e = Expression::Number(Number::from(1), s_leaf.clone());
        let str_e = Expression::String("s".to_string(), s_leaf.clone());
        let var_e = Expression::Variable("x".to_string(), s_leaf.clone());
        assert_eq!(null_e.expr_at_position(&pos_leaf), Some(&null_e));
        assert_eq!(bool_e.expr_at_position(&pos_leaf), Some(&bool_e));
        assert_eq!(num_e.expr_at_position(&pos_leaf), Some(&num_e));
        assert_eq!(str_e.expr_at_position(&pos_leaf), Some(&str_e));
        assert_eq!(var_e.expr_at_position(&pos_leaf), Some(&var_e));
    }

    #[test]
    fn test_template_and_directive_expr_at_branches() {
        use crate::span::Position;

        // Directive::If
        let if_cond = Expression::Bool(true, Span::new(0, 15, 1, 1, 1, 16));
        let true_sub = Expression::Variable("t_var".to_string(), Span::new(25, 35, 1, 26, 1, 36));
        let true_interp =
            TemplatePart::Interpolation(true_sub.clone(), Span::new(20, 40, 1, 21, 1, 41));
        let true_lit = TemplatePart::Literal("lit".to_string(), Span::new(40, 45, 1, 41, 1, 46));

        let elif_cond = Expression::Bool(false, Span::new(45, 55, 1, 46, 1, 56));
        let elif_sub = Expression::Variable("e_var".to_string(), Span::new(65, 75, 1, 66, 1, 76));
        let elif_interp =
            TemplatePart::Interpolation(elif_sub.clone(), Span::new(60, 80, 1, 61, 1, 81));
        let elif_lit = TemplatePart::Literal("lit2".to_string(), Span::new(80, 85, 1, 81, 1, 86));

        let false_sub =
            Expression::Variable("f_var".to_string(), Span::new(95, 105, 1, 96, 1, 106));
        let false_interp =
            TemplatePart::Interpolation(false_sub.clone(), Span::new(90, 110, 1, 91, 1, 111));
        let false_lit =
            TemplatePart::Literal("lit3".to_string(), Span::new(110, 115, 1, 111, 1, 116));

        let if_dir = Directive::If {
            cond: if_cond.clone(),
            true_expr: vec![true_lit, true_interp],
            else_ifs: vec![(elif_cond.clone(), vec![elif_lit, elif_interp])],
            false_expr: Some(vec![false_lit, false_interp]),
        };

        // Cond
        assert_eq!(
            directive_expr_at(&if_dir, &Position::new(1, 5, 5)),
            Some(&if_cond)
        );
        // True branch inner expr
        assert_eq!(
            directive_expr_at(&if_dir, &Position::new(1, 30, 30)),
            Some(&true_sub)
        );
        // True branch interpolation span fallback
        assert_eq!(
            directive_expr_at(&if_dir, &Position::new(1, 22, 22)),
            Some(&true_sub)
        );
        // Elif cond
        assert_eq!(
            directive_expr_at(&if_dir, &Position::new(1, 50, 50)),
            Some(&elif_cond)
        );
        // Elif branch inner expr
        assert_eq!(
            directive_expr_at(&if_dir, &Position::new(1, 70, 70)),
            Some(&elif_sub)
        );
        // Elif branch interpolation span fallback
        assert_eq!(
            directive_expr_at(&if_dir, &Position::new(1, 62, 62)),
            Some(&elif_sub)
        );
        // False branch inner expr
        assert_eq!(
            directive_expr_at(&if_dir, &Position::new(1, 100, 100)),
            Some(&false_sub)
        );
        // False branch interpolation span fallback
        assert_eq!(
            directive_expr_at(&if_dir, &Position::new(1, 92, 92)),
            Some(&false_sub)
        );
        // Position not matching any sub-expression
        assert_eq!(
            directive_expr_at(&if_dir, &Position::new(1, 120, 120)),
            None
        );

        // Directive::If with false_expr: None
        let if_dir_no_false = Directive::If {
            cond: if_cond.clone(),
            true_expr: vec![],
            else_ifs: vec![],
            false_expr: None,
        };
        assert_eq!(
            directive_expr_at(&if_dir_no_false, &Position::new(1, 100, 100)),
            None
        );

        // Directive::For
        let for_coll = Expression::Variable("my_coll".to_string(), Span::new(0, 15, 1, 1, 1, 16));
        let body_sub = Expression::Variable("b_var".to_string(), Span::new(25, 35, 1, 26, 1, 36));
        let body_interp =
            TemplatePart::Interpolation(body_sub.clone(), Span::new(20, 40, 1, 21, 1, 41));
        let for_dir = Directive::For {
            key_var: None,
            val_var: "item".to_string(),
            collection: for_coll.clone(),
            body: vec![body_interp],
        };
        assert_eq!(
            directive_expr_at(&for_dir, &Position::new(1, 5, 5)),
            Some(&for_coll)
        );
        assert_eq!(
            directive_expr_at(&for_dir, &Position::new(1, 30, 30)),
            Some(&body_sub)
        );
        assert_eq!(
            directive_expr_at(&for_dir, &Position::new(1, 22, 22)),
            Some(&body_sub)
        );
        assert_eq!(directive_expr_at(&for_dir, &Position::new(1, 50, 50)), None);

        // Directive::Strip
        let strip_dir = Directive::Strip {
            strip_left: true,
            strip_right: true,
        };
        assert_eq!(directive_expr_at(&strip_dir, &Position::new(1, 5, 5)), None);

        // Template wrapper covering TemplatePart::Literal, Interpolation fallback, and Directive
        let tmpl_span = Span::new(0, 100, 1, 1, 1, 101);
        let interp_var = Expression::Variable("v".to_string(), Span::new(15, 25, 1, 16, 1, 26));
        let tmpl = Expression::Template(
            vec![
                TemplatePart::Literal("lit".to_string(), Span::new(0, 10, 1, 1, 1, 11)),
                TemplatePart::Interpolation(interp_var.clone(), Span::new(10, 30, 1, 11, 1, 31)),
                TemplatePart::Directive(if_dir, Span::new(30, 90, 1, 31, 1, 91)),
            ],
            tmpl_span.clone(),
        );
        // In interpolation inner expr
        assert_eq!(
            tmpl.expr_at_position(&Position::new(1, 20, 20)),
            Some(&interp_var)
        );
        // In interpolation span but not in inner expr
        assert_eq!(
            tmpl.expr_at_position(&Position::new(1, 12, 12)),
            Some(&interp_var)
        );
        // In literal
        assert_eq!(tmpl.expr_at_position(&Position::new(1, 5, 5)), Some(&tmpl));
        // In directive matching inner
        let pos_in_dir_inner = Position::new(1, 50, 50);
        assert_eq!(tmpl.expr_at_position(&pos_in_dir_inner), Some(&elif_cond));
        // In directive span but directive_expr_at returns None
        let pos_in_dir_none = Position::new(1, 88, 88);
        assert_eq!(tmpl.expr_at_position(&pos_in_dir_none), Some(&tmpl));

        // Template with Directive::For having Literal and Interpolation in body
        let for_body_lit =
            TemplatePart::Literal("txt".to_string(), Span::new(35, 40, 1, 36, 1, 41));
        let for_body_sub =
            Expression::Variable("item_var".to_string(), Span::new(42, 48, 1, 43, 1, 49));
        let for_body_interp =
            TemplatePart::Interpolation(for_body_sub.clone(), Span::new(40, 50, 1, 41, 1, 51));
        let for_dir_complex = Directive::For {
            key_var: None,
            val_var: "item".to_string(),
            collection: for_coll.clone(),
            body: vec![for_body_lit, for_body_interp],
        };
        let tmpl_for = Expression::Template(
            vec![TemplatePart::Directive(
                for_dir_complex,
                Span::new(0, 60, 1, 1, 1, 61),
            )],
            Span::new(0, 70, 1, 1, 1, 71),
        );
        assert_eq!(
            tmpl_for.expr_at_position(&Position::new(1, 5, 5)),
            Some(&for_coll)
        );
        assert_eq!(
            tmpl_for.expr_at_position(&Position::new(1, 45, 45)),
            Some(&for_body_sub)
        );
        assert_eq!(
            tmpl_for.expr_at_position(&Position::new(1, 37, 37)),
            Some(&tmpl_for)
        );
    }
}
