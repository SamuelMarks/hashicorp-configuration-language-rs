//! AST structures for HCL bodies, attributes, blocks, and dynamic blocks.
use crate::ast::expr::Expression;
use crate::span::Span;
use std::collections::HashMap;
/// Represents an HCL Body, which contains a collection of attributes, blocks, and dynamic blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    /// The attributes contained in this body.
    pub attributes: HashMap<String, Attribute>,
    /// The blocks contained in this body.
    pub blocks: Vec<Block>,
    /// The user-defined functions contained in this body.
    pub functions: Vec<crate::ast::user_func::FunctionBlock>,
    /// The dynamic blocks contained in this body.
    pub dynamic_blocks: Vec<DynamicBlock>,
    /// The declarative validation blocks contained in this body.
    pub validations: Vec<ValidationBlock>,
    /// The lifecycle precondition blocks contained in this body.
    pub preconditions: Vec<PreconditionBlock>,
    /// The lifecycle postcondition blocks contained in this body.
    pub postconditions: Vec<PostconditionBlock>,
    /// The source span for the entire body.
    pub span: Span,
}
impl Body {
    /// Create a new, empty Body.
    ///
    /// # Arguments
    /// * `span` - The source span covering the entire body.
    #[must_use]
    pub fn new(span: Span) -> Self {
        Self {
            attributes: HashMap::new(),
            blocks: Vec::new(),
            functions: Vec::new(),
            dynamic_blocks: Vec::new(),
            validations: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
            span,
        }
    }
    /// Recursively collects all [`ValidationBlock`] instances within this body and its nested blocks.
    #[must_use]
    pub fn collect_all_validations(&self) -> Vec<&ValidationBlock> {
        let mut result = Vec::new();
        for v in &self.validations {
            result.push(v);
        }
        for b in &self.blocks {
            result.extend(b.body.collect_all_validations());
        }
        result
    }
    /// Recursively collects all [`PreconditionBlock`] instances within this body and its nested blocks.
    #[must_use]
    pub fn collect_all_preconditions(&self) -> Vec<&PreconditionBlock> {
        let mut result = Vec::new();
        for p in &self.preconditions {
            result.push(p);
        }
        for b in &self.blocks {
            result.extend(b.body.collect_all_preconditions());
        }
        result
    }
    /// Recursively collects all [`PostconditionBlock`] instances within this body and its nested blocks.
    #[must_use]
    pub fn collect_all_postconditions(&self) -> Vec<&PostconditionBlock> {
        let mut result = Vec::new();
        for p in &self.postconditions {
            result.push(p);
        }
        for b in &self.blocks {
            result.extend(b.body.collect_all_postconditions());
        }
        result
    }
    /// Finds the innermost AST node at the given spatial coordinate [`Position`](crate::span::Position).
    ///
    /// Checks attributes (including nested sub-expressions), blocks (including nested blocks and headers),
    /// and returns the most specific innermost AST node reference. If the position falls into whitespace
    /// or comments, returns `None`.
    ///
    /// # Arguments
    /// * `pos` - The coordinate position within the document.
    #[must_use]
    pub fn node_at_position(
        &self,
        pos: &crate::span::Position,
    ) -> Option<crate::span::AstNodeRef<'_>> {
        for attr in self.attributes.values() {
            if attr.span.contains_position(pos) {
                if let Some(inner_expr) = attr.expr.expr_at_position(pos) {
                    return Some(crate::span::AstNodeRef::Expression(inner_expr));
                }
                return Some(crate::span::AstNodeRef::Attribute(attr));
            }
        }
        for block in &self.blocks {
            if block.span.contains_position(pos) {
                if let Some(inner_node) = block.body.node_at_position(pos) {
                    return Some(inner_node);
                }
                return Some(crate::span::AstNodeRef::Block(block));
            }
        }
        None
    }
    /// Finds the innermost enclosing [`Block`] at the given coordinate [`Position`](crate::span::Position).
    ///
    /// Traverses recursively into nested blocks to find the most deeply nested block
    /// that spans `pos`.
    ///
    /// # Arguments
    /// * `pos` - The coordinate position within the document.
    #[must_use]
    pub fn enclosing_block_at(&self, pos: &crate::span::Position) -> Option<&Block> {
        for block in &self.blocks {
            if block.span.contains_position(pos) {
                if let Some(inner_block) = block.body.enclosing_block_at(pos) {
                    return Some(inner_block);
                }
                return Some(block);
            }
        }
        None
    }
    /// Finds the [`Attribute`] located at the given coordinate [`Position`](crate::span::Position).
    ///
    /// Traverses through this body and recursively into nested blocks to locate the attribute.
    ///
    /// # Arguments
    /// * `pos` - The coordinate position within the document.
    #[must_use]
    pub fn attribute_at(&self, pos: &crate::span::Position) -> Option<&Attribute> {
        for attr in self.attributes.values() {
            if attr.span.contains_position(pos) {
                return Some(attr);
            }
        }
        for block in &self.blocks {
            if block.span.contains_position(pos) {
                if let Some(attr) = block.body.attribute_at(pos) {
                    return Some(attr);
                }
            }
        }
        None
    }
}
/// Represents an HCL Attribute (`name = expression`).
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// The name of the attribute.
    pub name: String,
    /// The expression assigned to the attribute.
    pub expr: Expression,
    /// The source span for the entire attribute.
    pub span: Span,
    /// The source span for the attribute's name.
    pub name_span: Span,
    /// The source span for the equals sign.
    pub equals_span: Span,
    /// Leading trivia and comments before the attribute.
    pub leading_comments: Vec<String>,
    /// Trailing inline comment on the same line after the attribute.
    pub trailing_comment: Option<String>,
}
impl Attribute {
    /// Creates a new `Attribute`.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    /// * `expr` - The assigned expression.
    /// * `span` - Source span covering the attribute.
    #[must_use]
    pub fn new(name: impl Into<String>, expr: Expression, span: Span) -> Self {
        let name_span = span.clone();
        let equals_span = span.clone();
        Self {
            name: name.into(),
            expr,
            span,
            name_span,
            equals_span,
            leading_comments: Vec::new(),
            trailing_comment: None,
        }
    }
    /// Attaches leading comments to this attribute.
    ///
    /// # Arguments
    /// * `comments` - Leading comment strings.
    #[must_use]
    pub fn with_leading_comments(mut self, comments: Vec<String>) -> Self {
        self.leading_comments = comments;
        self
    }
    /// Attaches a trailing inline comment to this attribute.
    ///
    /// # Arguments
    /// * `comment` - Trailing comment text.
    #[must_use]
    pub fn with_trailing_comment(mut self, comment: impl Into<String>) -> Self {
        self.trailing_comment = Some(comment.into());
        self
    }
}
/// Represents an HCL Block (`type "label1" "label2" { body }`).
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The block type identifier.
    pub block_type: String,
    /// The block labels.
    pub labels: Vec<String>,
    /// The nested body of the block.
    pub body: Body,
    /// The source span for the entire block.
    pub span: Span,
    /// The source span for the block type.
    pub type_span: Span,
    /// The source spans for each label.
    pub label_spans: Vec<Span>,
    /// The source span for the open brace.
    pub open_brace_span: Span,
    /// The source span for the close brace.
    pub close_brace_span: Span,
    /// Leading trivia and comments before the block.
    pub leading_comments: Vec<String>,
    /// Trailing inline comment on the same line after the block header or close brace.
    pub trailing_comment: Option<String>,
}
impl Block {
    /// Creates a new `Block`.
    ///
    /// # Arguments
    /// * `block_type` - Block type name.
    /// * `labels` - Label string vector.
    /// * `body` - Nested block body.
    /// * `span` - Source span covering the block.
    #[must_use]
    pub fn new(block_type: impl Into<String>, labels: Vec<String>, body: Body, span: Span) -> Self {
        let type_span = span.clone();
        let open_brace_span = span.clone();
        let close_brace_span = span.clone();
        Self {
            block_type: block_type.into(),
            labels,
            body,
            span,
            type_span,
            label_spans: Vec::new(),
            open_brace_span,
            close_brace_span,
            leading_comments: Vec::new(),
            trailing_comment: None,
        }
    }
    /// Attaches leading comments to this block.
    ///
    /// # Arguments
    /// * `comments` - Leading comment strings.
    #[must_use]
    pub fn with_leading_comments(mut self, comments: Vec<String>) -> Self {
        self.leading_comments = comments;
        self
    }
    /// Attaches a trailing inline comment to this block.
    ///
    /// # Arguments
    /// * `comment` - Trailing comment text.
    #[must_use]
    pub fn with_trailing_comment(mut self, comment: impl Into<String>) -> Self {
        self.trailing_comment = Some(comment.into());
        self
    }
}
/// Represents an HCL dynamic block (`dynamic "type" { for_each = ... content { ... } }`).
///
/// Dynamic blocks dynamically generate repeated nested blocks based on a collection.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicBlock {
    /// The target block type identifier to generate (e.g. `"setting"`).
    pub block_type: String,
    /// The collection expression to iterate over (`for_each`).
    pub for_each: Expression,
    /// The optional custom iterator variable name (defaults to `block_type`).
    pub iterator: Option<String>,
    /// Optional labels expressions evaluated for each generated block.
    pub labels: Option<Vec<Expression>>,
    /// The content body template to expand for each collection item.
    pub content: Body,
    /// The source span for the entire dynamic block.
    pub span: Span,
    /// The source span for the block type name.
    pub type_span: Span,
}
impl DynamicBlock {
    /// Creates a new `DynamicBlock`.
    ///
    /// # Arguments
    /// * `block_type` - The target block type identifier.
    /// * `for_each` - The expression that evaluates to the collection.
    /// * `iterator` - The optional iterator name override.
    /// * `labels` - Optional list of label expressions.
    /// * `content` - The body template for each block.
    /// * `span` - The full source span of the dynamic block.
    /// * `type_span` - The source span of the block type name.
    #[must_use]
    pub fn new(
        block_type: String,
        for_each: Expression,
        iterator: Option<String>,
        labels: Option<Vec<Expression>>,
        content: Body,
        span: Span,
        type_span: Span,
    ) -> Self {
        Self {
            block_type,
            for_each,
            iterator,
            labels,
            content,
            span,
            type_span,
        }
    }
}
/// Represents a declarative validation block (`validation { condition = ... error_message = ... }`).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationBlock {
    /// The boolean expression that must evaluate to true.
    pub condition: Expression,
    /// The error message expression evaluated when the condition is false.
    pub error_message: Expression,
    /// The source span for the entire validation block.
    pub span: Span,
}
impl ValidationBlock {
    /// Creates a new `ValidationBlock`.
    ///
    /// # Arguments
    /// * `condition` - The condition expression to evaluate.
    /// * `error_message` - The error message expression evaluated on failure.
    /// * `span` - The source span covering the validation block.
    #[must_use]
    pub fn new(condition: Expression, error_message: Expression, span: Span) -> Self {
        Self {
            condition,
            error_message,
            span,
        }
    }
}
/// Represents a lifecycle precondition block (`precondition { condition = ... error_message = ... }`).
#[derive(Debug, Clone, PartialEq)]
pub struct PreconditionBlock {
    /// The boolean expression that must evaluate to true before execution.
    pub condition: Expression,
    /// The error message expression evaluated when the condition is false.
    pub error_message: Expression,
    /// The source span for the entire precondition block.
    pub span: Span,
}
impl PreconditionBlock {
    /// Creates a new `PreconditionBlock`.
    ///
    /// # Arguments
    /// * `condition` - The condition expression to evaluate.
    /// * `error_message` - The error message expression evaluated on failure.
    /// * `span` - The source span covering the precondition block.
    #[must_use]
    pub fn new(condition: Expression, error_message: Expression, span: Span) -> Self {
        Self {
            condition,
            error_message,
            span,
        }
    }
    /// Converts this precondition into an equivalent [`ValidationBlock`].
    #[must_use]
    pub fn to_validation(&self) -> ValidationBlock {
        ValidationBlock::new(
            self.condition.clone(),
            self.error_message.clone(),
            self.span.clone(),
        )
    }
}
impl From<PreconditionBlock> for ValidationBlock {
    fn from(pre: PreconditionBlock) -> Self {
        Self::new(pre.condition, pre.error_message, pre.span)
    }
}
/// Represents a lifecycle postcondition block (`postcondition { condition = ... error_message = ... }`).
#[derive(Debug, Clone, PartialEq)]
pub struct PostconditionBlock {
    /// The boolean expression that must evaluate to true after execution.
    pub condition: Expression,
    /// The error message expression evaluated when the condition is false.
    pub error_message: Expression,
    /// The source span for the entire postcondition block.
    pub span: Span,
}
impl PostconditionBlock {
    /// Creates a new `PostconditionBlock`.
    ///
    /// # Arguments
    /// * `condition` - The condition expression to evaluate.
    /// * `error_message` - The error message expression evaluated on failure.
    /// * `span` - The source span covering the postcondition block.
    #[must_use]
    pub fn new(condition: Expression, error_message: Expression, span: Span) -> Self {
        Self {
            condition,
            error_message,
            span,
        }
    }
    /// Converts this postcondition into an equivalent [`ValidationBlock`].
    #[must_use]
    pub fn to_validation(&self) -> ValidationBlock {
        ValidationBlock::new(
            self.condition.clone(),
            self.error_message.clone(),
            self.span.clone(),
        )
    }
}
impl From<PostconditionBlock> for ValidationBlock {
    fn from(post: PostconditionBlock) -> Self {
        Self::new(post.condition, post.error_message, post.span)
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
    #[test]
    fn test_body_and_dynamic_block() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut body = Body::new(span.clone());
        assert!(body.attributes.is_empty());
        assert_eq!(body.blocks, [] as [Block; 0]);
        assert_eq!(
            body.functions,
            [] as [crate::ast::user_func::FunctionBlock; 0]
        );
        assert_eq!(body.dynamic_blocks, [] as [DynamicBlock; 0]);
        assert_eq!(body.validations, [] as [ValidationBlock; 0]);
        assert_eq!(body.preconditions, [] as [PreconditionBlock; 0]);
        assert_eq!(body.postconditions, [] as [PostconditionBlock; 0]);
        assert_eq!(body.span, span);
        let dyn_block = DynamicBlock::new(
            "setting".to_string(),
            Expression::Null(span.clone()),
            Some("item".to_string()),
            Some(vec![Expression::Null(span.clone())]),
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        );
        assert_eq!(dyn_block.block_type, "setting");
        assert_eq!(dyn_block.iterator.as_deref(), Some("item"));
        assert!(dyn_block.labels.is_some());
        assert_eq!(dyn_block.span, span);
        body.dynamic_blocks.push(dyn_block.clone());
        assert_eq!(body.dynamic_blocks.len(), 1);
        assert_eq!(body.dynamic_blocks[0], dyn_block);
        let val_block = ValidationBlock::new(
            Expression::Null(span.clone()),
            Expression::Null(span.clone()),
            span.clone(),
        );
        assert_eq!(val_block.span, span);
        body.validations.push(val_block.clone());
        assert_eq!(body.validations.len(), 1);
        assert_eq!(body.validations[0], val_block);
        let pre_block = PreconditionBlock::new(
            Expression::Null(span.clone()),
            Expression::Null(span.clone()),
            span.clone(),
        );
        assert_eq!(pre_block.span, span);
        let val_from_pre: ValidationBlock = pre_block.clone().into();
        assert_eq!(val_from_pre, pre_block.to_validation());
        body.preconditions.push(pre_block.clone());
        assert_eq!(body.preconditions.len(), 1);
        assert_eq!(body.preconditions[0], pre_block);
        let post_block = PostconditionBlock::new(
            Expression::Null(span.clone()),
            Expression::Null(span.clone()),
            span.clone(),
        );
        assert_eq!(post_block.span, span);
        let val_from_post: ValidationBlock = post_block.clone().into();
        assert_eq!(val_from_post, post_block.to_validation());
        body.postconditions.push(post_block.clone());
        assert_eq!(body.postconditions.len(), 1);
        assert_eq!(body.postconditions[0], post_block);
        assert_eq!(body.collect_all_validations(), vec![&val_block]);
        assert_eq!(body.collect_all_preconditions(), vec![&pre_block]);
        assert_eq!(body.collect_all_postconditions(), vec![&post_block]);
        let mut nested_body = Body::new(span.clone());
        nested_body.validations.push(val_block.clone());
        nested_body.preconditions.push(pre_block.clone());
        nested_body.postconditions.push(post_block.clone());
        let nested_block = Block {
            block_type: "resource".to_string(),
            labels: vec!["a".to_string()],
            body: nested_body,
            span: span.clone(),
            type_span: span.clone(),
            label_spans: vec![span.clone()],
            open_brace_span: span.clone(),
            close_brace_span: span,
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        body.blocks.push(nested_block);
        assert_eq!(body.collect_all_validations(), vec![&val_block, &val_block]);
        assert_eq!(
            body.collect_all_preconditions(),
            vec![&pre_block, &pre_block]
        );
        assert_eq!(
            body.collect_all_postconditions(),
            vec![&post_block, &post_block]
        );
    }
    #[test]
    fn test_body_spatial_queries() {
        use crate::span::{AstNodeRef, Position};
        let outer_span = Span::new(0, 100, 1, 1, 10, 1);
        let inner_span = Span::new(10, 90, 2, 1, 9, 1);
        let attr_span = Span::new(20, 40, 3, 1, 3, 20);
        let mut inner_body = Body::new(inner_span.clone());
        let attr = Attribute {
            name: "nested_attr".to_string(),
            expr: Expression::Null(attr_span.clone()),
            span: attr_span.clone(),
            name_span: attr_span.clone(),
            equals_span: attr_span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        inner_body
            .attributes
            .insert("nested_attr".to_string(), attr);
        let inner_block = Block {
            block_type: "inner".to_string(),
            labels: vec![],
            body: inner_body,
            span: inner_span.clone(),
            type_span: inner_span.clone(),
            label_spans: vec![],
            open_brace_span: inner_span.clone(),
            close_brace_span: inner_span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let mut outer_body = Body::new(outer_span.clone());
        outer_body.blocks.push(inner_block);
        let outer_block = Block {
            block_type: "outer".to_string(),
            labels: vec![],
            body: outer_body,
            span: outer_span.clone(),
            type_span: outer_span.clone(),
            label_spans: vec![],
            open_brace_span: outer_span.clone(),
            close_brace_span: outer_span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let mut root_body = Body::new(outer_span.clone());
        let unrelated_block = Block {
            block_type: "unrelated".to_string(),
            labels: vec![],
            body: Body::new(Span::new(200, 250, 20, 1, 25, 1)),
            span: Span::new(200, 250, 20, 1, 25, 1),
            type_span: Span::new(200, 210, 20, 1, 20, 10),
            label_spans: vec![],
            open_brace_span: Span::new(211, 212, 20, 11, 20, 12),
            close_brace_span: Span::new(249, 250, 25, 1, 25, 2),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        root_body.blocks.push(unrelated_block.clone());
        root_body.blocks.push(outer_block);
        let attr_pos = Position::new(3, 5, 25);
        let found_attr = root_body.attribute_at(&attr_pos);
        assert_eq!(found_attr.map(|a| a.name.as_str()), Some("nested_attr"));
        let enc_block = root_body.enclosing_block_at(&attr_pos);
        assert_eq!(enc_block.map(|b| b.block_type.as_str()), Some("inner"));
        let empty_inner_block = Block {
            block_type: "empty_blk".to_string(),
            labels: vec![],
            body: Body::new(Span::new(50, 60, 5, 1, 5, 10)),
            span: Span::new(45, 65, 4, 1, 6, 1),
            type_span: Span::new(45, 50, 4, 1, 4, 6),
            label_spans: vec![],
            open_brace_span: Span::new(50, 51, 4, 7, 4, 8),
            close_brace_span: Span::new(64, 65, 6, 1, 6, 2),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let mut b = Body::new(Span::new(0, 100, 1, 1, 10, 1));
        b.blocks.push(unrelated_block);
        b.blocks.push(empty_inner_block.clone());
        let header_pos = Position::new(4, 2, 46);
        let node = b.node_at_position(&header_pos);
        assert_eq!(node, Some(AstNodeRef::Block(&empty_inner_block)));
        assert_eq!(b.attribute_at(&header_pos), None);
    }
    /// Tests builders and trivia mutation helpers for Attribute and Block.
    #[test]
    fn test_structure_builders_and_trivia() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let expr = Expression::Variable("v".to_string(), span.clone());
        let attr = Attribute::new("my_attr", expr, span.clone())
            .with_leading_comments(vec!["# leading".to_string()])
            .with_trailing_comment("// trailing");
        assert_eq!(attr.name, "my_attr");
        assert_eq!(attr.leading_comments, vec!["# leading".to_string()]);
        assert_eq!(attr.trailing_comment.as_deref(), Some("// trailing"));
        let blk = Block::new(
            "server",
            vec!["web".to_string()],
            Body::new(span.clone()),
            span,
        )
        .with_leading_comments(vec!["# block leading".to_string()])
        .with_trailing_comment("// block trailing");
        assert_eq!(blk.block_type, "server");
        assert_eq!(blk.labels, vec!["web".to_string()]);
        assert_eq!(blk.leading_comments, vec!["# block leading".to_string()]);
        assert_eq!(blk.trailing_comment.as_deref(), Some("// block trailing"));
    }
}
