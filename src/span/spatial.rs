//! Spatial coordinate types and AST node references for IDE and LSP tooling.

#![deny(missing_docs)]

use crate::span::Span;

/// Represents a spatial coordinate (line, column, and byte offset) within a source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Position {
    /// 1-based line index.
    pub line: usize,
    /// 1-based column index.
    pub column: usize,
    /// 0-based byte offset within the source buffer.
    pub byte_offset: usize,
}

impl Position {
    /// Creates a new `Position` with explicit line, column, and byte offset.
    ///
    /// # Arguments
    /// * `line` - 1-based line index.
    /// * `column` - 1-based column index.
    /// * `byte_offset` - 0-based byte offset.
    #[must_use]
    pub const fn new(line: usize, column: usize, byte_offset: usize) -> Self {
        Self {
            line,
            column,
            byte_offset,
        }
    }

    /// Checks if this position is contained within the given [`Span`].
    ///
    /// Equivalent to `span.contains_position(self)`.
    ///
    /// # Arguments
    /// * `span` - The source span to check against.
    #[must_use]
    pub fn contains(&self, span: &Span) -> bool {
        span.contains_position(self)
    }
}

/// A borrowed reference to a strongly-typed AST node variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AstNodeRef<'a> {
    /// Reference to an AST [`Body`](crate::ast::structure::Body).
    Body(&'a crate::ast::structure::Body),
    /// Reference to an AST [`Attribute`](crate::ast::structure::Attribute).
    Attribute(&'a crate::ast::structure::Attribute),
    /// Reference to an AST [`Block`](crate::ast::structure::Block).
    Block(&'a crate::ast::structure::Block),
    /// Reference to an AST [`Expression`](crate::ast::expr::Expression).
    Expression(&'a crate::ast::expr::Expression),
}

impl AstNodeRef<'_> {
    /// Returns the source [`Span`] associated with the referenced AST node.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Body(body) => body.span.clone(),
            Self::Attribute(attr) => attr.span.clone(),
            Self::Block(block) => block.span.clone(),
            Self::Expression(expr) => expr.span(),
        }
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
    use crate::api::parse;

    #[test]
    fn test_position_span_containment() {
        let span = Span::new(10, 20, 2, 5, 2, 15);
        let pos_inside = Position::new(2, 10, 15);
        let pos_before = Position::new(2, 4, 9);
        let pos_after = Position::new(2, 16, 20);

        assert!(span.contains_position(&pos_inside));
        assert!(pos_inside.contains(&span));
        assert!(!span.contains_position(&pos_before));
        assert!(!span.contains_position(&pos_after));

        // Test derived traits on Position
        let default_pos = Position::default();
        assert_eq!(default_pos.line, 0);
        assert_eq!(default_pos.column, 0);
        assert_eq!(default_pos.byte_offset, 0);

        let cloned_pos = pos_inside;
        assert_eq!(pos_inside, cloned_pos);
        assert_ne!(pos_inside, pos_before);
        assert_ne!(pos_inside, Position::new(3, 10, 15));
        assert_ne!(pos_inside, Position::new(2, 11, 15));
        assert_ne!(pos_inside, Position::new(2, 10, 16));

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();
        pos_inside.hash(&mut h1);
        cloned_pos.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());

        let debug_str = format!("{pos_inside:?}");
        assert!(debug_str.contains("Position"));
    }

    #[test]
    fn test_spatial_query_block_type_and_labels() {
        let src = "resource \"aws_s3_bucket\" \"my_bucket\" {\n  bucket = \"prod\"\n}\n";
        let body = parse(src).unwrap();

        // 1. Position inside block type "resource" (offset 3)
        let pos_type = Position::new(1, 4, 3);
        let node_type = body.node_at_position(&pos_type).unwrap();
        let block_enc = body.enclosing_block_at(&pos_type).unwrap();
        assert_eq!(node_type, AstNodeRef::Block(block_enc));
        assert_eq!(block_enc.block_type, "resource");
        assert_eq!(node_type.span(), block_enc.span);

        // 2. Position inside block label "aws_s3_bucket" (offset 15)
        let pos_label1 = Position::new(1, 16, 15);
        let node_label1 = body.node_at_position(&pos_label1).unwrap();
        assert_eq!(node_label1, AstNodeRef::Block(block_enc));

        // 3. Position inside block label "my_bucket" (offset 30)
        let pos_label2 = Position::new(1, 31, 30);
        let node_label2 = body.node_at_position(&pos_label2).unwrap();
        assert_eq!(node_label2, AstNodeRef::Block(block_enc));
    }

    #[test]
    fn test_spatial_query_attribute_name_and_value() {
        let src = "service {\n  count = 42\n}\n";
        let body = parse(src).unwrap();

        // 1. Position inside attribute name "count" (offset 14)
        let pos_name = Position::new(2, 4, 14);
        let node_name = body.node_at_position(&pos_name).unwrap();
        let attr = body.attribute_at(&pos_name).unwrap();
        assert_eq!(node_name, AstNodeRef::Attribute(attr));
        assert_eq!(attr.name, "count");
        assert_eq!(node_name.span(), attr.span);

        // 2. Position inside attribute value literal "42" (offset 21)
        let pos_val = Position::new(2, 11, 21);
        let node_val = body.node_at_position(&pos_val).unwrap();
        assert_eq!(node_val, AstNodeRef::Expression(&attr.expr));
        assert_eq!(node_val.span(), attr.expr.span());
    }

    #[test]
    fn test_spatial_query_nested_template_interpolations() {
        let src = "msg = \"hello ${var.target}!\"\n";
        let body = parse(src).unwrap();

        // Offset 18 is inside "var.target"
        let pos_interp = Position::new(1, 19, 18);
        let node = body.node_at_position(&pos_interp).unwrap();
        assert!(matches!(
            node,
            AstNodeRef::Expression(expr) if matches!(
                expr,
                crate::ast::expr::Expression::Traversal(_, _)
                    | crate::ast::expr::Expression::Variable(_, _)
            )
        ));

        // Test AstNodeRef derived traits
        let cloned_node = node;
        assert_eq!(node, cloned_node);
        let debug_node = format!("{node:?}");
        assert!(debug_node.contains("Expression"));

        let body_ref = AstNodeRef::Body(&body);
        assert_ne!(node, body_ref);
    }

    #[test]
    fn test_spatial_query_whitespace_and_comments_return_none() {
        let src = "a = 1\n\n// comment line\n\nb = 2\n";
        let body = parse(src).unwrap();

        // Offset 6 is in the empty line between a=1 and the comment
        let pos_ws = Position::new(2, 1, 6);
        assert!(body.node_at_position(&pos_ws).is_none());
        assert!(body.attribute_at(&pos_ws).is_none());
        assert!(body.enclosing_block_at(&pos_ws).is_none());

        // Offset 10 is inside the comment line
        let pos_comment = Position::new(3, 3, 10);
        assert!(body.node_at_position(&pos_comment).is_none());
        assert!(body.attribute_at(&pos_comment).is_none());
        assert!(body.enclosing_block_at(&pos_comment).is_none());

        let node_body = AstNodeRef::Body(&body);
        assert_eq!(node_body.span(), body.span);
    }

    #[test]
    fn test_spatial_query_nested_block() {
        let src = "outer {\n  inner {\n    val = 1\n  }\n}\n";
        let body = parse(src).unwrap();
        let pos_inner = Position::new(3, 5, 22);
        let enc = body.enclosing_block_at(&pos_inner).unwrap();
        assert_eq!(enc.block_type, "inner");
    }
}
