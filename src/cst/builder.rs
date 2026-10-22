//! Programmatic CST builders and mutation engine (`hclwrite`).
//!
//! Provides [`CstFile`], [`CstBody`], [`CstBlock`], and [`CstAttribute`] for creating,
//! mutating, and serializing concrete syntax trees while preserving comments,
//! indentation, and whitespace trivia.
use crate::ast::expr::{BinaryOp, Expression, TemplatePart, Traversal, TraversalOperator, UnaryOp};
use crate::cst::document::{Document, DocumentItem, TokenStream};
use crate::cst::parser::CstParser;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::lex::token::{Token, TokenKind};
use crate::span::Span;
use crate::types::val::{Value, ValueData};
use std::fmt;
use std::fmt::Write;
/// An item in a `CstBody` preserving exact source ordering.
#[derive(Debug, Clone, PartialEq)]
pub enum CstItem {
    /// An attribute assignment.
    Attribute(CstAttribute),
    /// A block definition.
    Block(CstBlock),
    /// Standalone formatting trivia (comments, whitespace, newlines).
    Trivia(TokenStream),
}
impl CstItem {
    /// Returns the item as an attribute if it is an attribute.
    #[must_use]
    pub const fn as_attribute(&self) -> Option<&CstAttribute> {
        match self {
            Self::Attribute(a) => Some(a),
            _ => None,
        }
    }
    /// Returns the item as a block if it is a block.
    #[must_use]
    pub const fn as_block(&self) -> Option<&CstBlock> {
        match self {
            Self::Block(b) => Some(b),
            _ => None,
        }
    }
}
/// A CST attribute with name, expression tokens, and formatting trivia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CstAttribute {
    /// Attribute name.
    pub name: String,
    /// Expression tokens representing the assigned value.
    pub expr_tokens: Vec<Token>,
    /// Leading formatting and comments before the attribute.
    pub leading: TokenStream,
    /// Trailing formatting after the attribute.
    pub trailing: TokenStream,
}
impl CstAttribute {
    /// Creates a new `CstAttribute`.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    /// * `expr_tokens` - The expression tokens.
    #[must_use]
    pub fn new(name: impl Into<String>, expr_tokens: Vec<Token>) -> Self {
        Self {
            name: name.into(),
            expr_tokens,
            leading: TokenStream::new(),
            trailing: TokenStream::new(),
        }
    }
    /// Returns the attribute name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the expression tokens.
    #[must_use]
    pub fn expr_tokens(&self) -> &[Token] {
        &self.expr_tokens
    }
    /// Replaces the attribute expression with a new AST [`Expression`] in-place,
    /// preserving leading whitespace, `=`, and trailing comments/trivia.
    ///
    /// # Arguments
    /// * `new_expr` - The new AST expression.
    pub fn replace_expr(&mut self, new_expr: &Expression) {
        self.expr_tokens = tokens_for_expression(new_expr);
    }
    /// Renames the attribute in-place without altering surrounding trivia or expression tokens.
    ///
    /// # Arguments
    /// * `new_name` - The new attribute name.
    pub fn replace_name(&mut self, new_name: &str) {
        self.name = new_name.to_string();
    }
    /// Sets this attribute's value to a string literal in-place.
    ///
    /// # Arguments
    /// * `val` - The string value.
    pub fn set_string(&mut self, val: &str) {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let expr = Expression::String(val.to_string(), dummy_span);
        self.replace_expr(&expr);
    }
    /// Sets this attribute's value to a number literal in-place.
    ///
    /// # Arguments
    /// * `val` - The number value.
    pub fn set_number(&mut self, val: &crate::number::Number) {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let expr = Expression::Number(val.clone(), dummy_span);
        self.replace_expr(&expr);
    }
    /// Sets this attribute's value to a boolean literal in-place.
    ///
    /// # Arguments
    /// * `val` - The boolean value.
    pub fn set_bool(&mut self, val: bool) {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let expr = Expression::Bool(val, dummy_span);
        self.replace_expr(&expr);
    }
    /// Sets this attribute's value to an AST traversal in-place.
    ///
    /// # Arguments
    /// * `traversal` - The AST traversal to set.
    pub fn set_traversal(&mut self, traversal: &crate::ast::expr::Traversal) {
        self.expr_tokens = tokens_for_traversal(traversal);
    }
    /// Renames all identifier tokens matching `old_name` to `new_name` in this attribute's expression.
    ///
    /// # Arguments
    /// * `old_name` - Target identifier to replace.
    /// * `new_name` - Replacement identifier.
    ///
    /// # Returns
    /// Number of identifier tokens renamed.
    pub fn rename_variable(&mut self, old_name: &str, new_name: &str) -> usize {
        let mut count = 0;
        for token in &mut self.expr_tokens {
            if token.kind == TokenKind::Ident && token.text == old_name {
                token.text = new_name.to_string();
                count += 1;
            }
        }
        count
    }
    /// Programmatically attaches a leading comment before this attribute.
    ///
    /// # Arguments
    /// * `comment` - The comment text to attach.
    pub fn with_leading_comment(&mut self, comment: &str) -> &mut Self {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let text =
            if comment.starts_with('#') || comment.starts_with("//") || comment.starts_with("/*") {
                if comment.ends_with('\n') {
                    comment.to_string()
                } else {
                    format!("{comment}\n")
                }
            } else {
                format!("# {comment}\n")
            };
        self.leading
            .push(Token::new(TokenKind::Comment, text, dummy_span));
        self
    }
    /// Programmatically attaches a trailing inline comment on the same line after this attribute.
    ///
    /// # Arguments
    /// * `comment` - The comment text to attach.
    pub fn with_trailing_inline_comment(&mut self, comment: &str) -> &mut Self {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let text =
            if comment.starts_with('#') || comment.starts_with("//") || comment.starts_with("/*") {
                format!(" {comment}\n")
            } else {
                format!(" # {comment}\n")
            };
        self.trailing
            .tokens
            .retain(|t| t.kind != TokenKind::Newline);
        self.trailing
            .push(Token::new(TokenKind::Comment, text, dummy_span));
        self
    }
}
/// A CST block with type, labels, inner body, and formatting trivia.
#[derive(Debug, Clone, PartialEq)]
pub struct CstBlock {
    /// Block type identifier (e.g., `"resource"`).
    pub block_type: String,
    /// Block labels (e.g., `["aws_s3_bucket", "b"]`).
    pub labels: Vec<String>,
    /// Inner body of the block.
    pub body: CstBody,
    /// Leading formatting and comments before the block.
    pub leading: TokenStream,
    /// Trailing formatting after the block.
    pub trailing: TokenStream,
}
impl CstBlock {
    /// Creates a new `CstBlock`.
    ///
    /// # Arguments
    /// * `block_type` - The block type identifier.
    /// * `labels` - The block labels.
    #[must_use]
    pub fn new(block_type: impl Into<String>, labels: Vec<String>) -> Self {
        Self {
            block_type: block_type.into(),
            labels,
            body: CstBody::new(),
            leading: TokenStream::new(),
            trailing: TokenStream::new(),
        }
    }
    /// Returns the block type.
    #[must_use]
    pub fn block_type(&self) -> &str {
        &self.block_type
    }
    /// Returns the block labels.
    #[must_use]
    pub fn labels(&self) -> &[String] {
        &self.labels
    }
    /// Returns a reference to the inner body.
    #[must_use]
    pub const fn body(&self) -> &CstBody {
        &self.body
    }
    /// Returns a mutable reference to the inner body.
    #[must_use]
    pub const fn body_mut(&mut self) -> &mut CstBody {
        &mut self.body
    }
    /// Programmatically attaches a leading comment before this block.
    ///
    /// # Arguments
    /// * `comment` - The comment text to attach.
    pub fn with_leading_comment(&mut self, comment: &str) -> &mut Self {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let text =
            if comment.starts_with('#') || comment.starts_with("//") || comment.starts_with("/*") {
                if comment.ends_with('\n') {
                    comment.to_string()
                } else {
                    format!("{comment}\n")
                }
            } else {
                format!("# {comment}\n")
            };
        self.leading
            .push(Token::new(TokenKind::Comment, text, dummy_span));
        self
    }
    /// Programmatically attaches a trailing inline comment on the same line after this block.
    ///
    /// # Arguments
    /// * `comment` - The comment text to attach.
    pub fn with_trailing_inline_comment(&mut self, comment: &str) -> &mut Self {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let text =
            if comment.starts_with('#') || comment.starts_with("//") || comment.starts_with("/*") {
                format!(" {comment}\n")
            } else {
                format!(" # {comment}\n")
            };
        self.trailing
            .tokens
            .retain(|t| t.kind != TokenKind::Newline);
        self.trailing
            .push(Token::new(TokenKind::Comment, text, dummy_span));
        self
    }
}
/// A CST body containing attributes and nested blocks with formatting awareness.
#[derive(Debug, Clone, PartialEq)]
pub struct CstBody {
    /// Interleaved items in the body preserving exact source ordering.
    pub items: Vec<CstItem>,
    /// Indentation level for this body.
    pub indent_level: usize,
    /// Indentation unit string (defaults to two spaces).
    pub indent_str: String,
    /// Trailing formatting tokens.
    pub trailing: TokenStream,
}
impl Default for CstBody {
    fn default() -> Self {
        Self::new()
    }
}
impl CstBody {
    /// Creates a new, empty `CstBody`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            indent_level: 0,
            indent_str: "  ".to_string(),
            trailing: TokenStream::new(),
        }
    }
    /// Creates a `CstBody` with a specific indentation level.
    ///
    /// # Arguments
    /// * `indent_level` - The indentation nesting level.
    #[must_use]
    pub fn with_indent_level(indent_level: usize) -> Self {
        Self {
            items: Vec::new(),
            indent_level,
            indent_str: "  ".to_string(),
            trailing: TokenStream::new(),
        }
    }
    /// Returns the attributes in this body.
    #[must_use]
    pub fn attributes(&self) -> Vec<&CstAttribute> {
        self.attributes_iter().collect()
    }
    /// Returns an iterator over attributes in this body from interleaved items.
    pub fn attributes_iter(&self) -> impl Iterator<Item = &CstAttribute> {
        self.items.iter().filter_map(|item| match item {
            CstItem::Attribute(attr) => Some(attr),
            _ => None,
        })
    }
    /// Returns the blocks in this body.
    #[must_use]
    pub fn blocks(&self) -> Vec<&CstBlock> {
        self.blocks_iter().collect()
    }
    /// Returns an iterator over blocks in this body from interleaved items.
    pub fn blocks_iter(&self) -> impl Iterator<Item = &CstBlock> {
        self.items.iter().filter_map(|item| match item {
            CstItem::Block(block) => Some(block),
            _ => None,
        })
    }
    /// Returns a slice of the interleaved items in this body.
    #[must_use]
    pub fn items(&self) -> &[CstItem] {
        &self.items
    }
    /// Sets or replaces an attribute by converting a [`Value`] into formatted CST tokens.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    /// * `val` - The value to assign.
    pub fn set_attribute_value(&mut self, name: &str, val: &Value) {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let tokens = value_to_tokens(val, &dummy_span);
        self.set_attribute_raw(name, tokens);
    }
    /// Sets or replaces an attribute using raw tokens.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    /// * `tokens` - The raw expression tokens.
    pub fn set_attribute_raw(&mut self, name: &str, tokens: Vec<Token>) {
        for item in &mut self.items {
            if let CstItem::Attribute(a) = item
                && a.name == name
            {
                a.expr_tokens = tokens;
                return;
            }
        }
        let mut attr = CstAttribute::new(name, tokens);
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let indent = self.indent_str.repeat(self.indent_level);
        if !indent.is_empty() {
            attr.leading.push(Token::new(
                TokenKind::Whitespace,
                &indent,
                dummy_span.clone(),
            ));
        }
        attr.trailing
            .push(Token::new(TokenKind::Newline, "\n", dummy_span));
        self.items.push(CstItem::Attribute(attr));
    }
    /// Replaces an attribute's expression with a new AST [`Expression`].
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    /// * `new_expr` - The new AST expression.
    ///
    /// # Errors
    /// Returns [`crate::error::HclError::MissingField`] if the attribute does not exist.
    pub fn replace_attribute_expr(
        &mut self,
        name: &str,
        new_expr: &Expression,
    ) -> Result<(), crate::error::HclError> {
        for item in &mut self.items {
            if let CstItem::Attribute(a) = item
                && a.name == name
            {
                a.expr_tokens = tokens_for_expression(new_expr);
                return Ok(());
            }
        }
        Err(crate::error::HclError::MissingField(name.to_string()))
    }
    /// Appends a new block to this body, setting its inner indentation level.
    ///
    /// # Arguments
    /// * `block` - The block to append.
    pub fn append_block(&mut self, mut block: CstBlock) {
        let inner_indent = self.indent_level + 1;
        block
            .body
            .set_indent_level_recursive(inner_indent, &self.indent_str);
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let indent = self.indent_str.repeat(self.indent_level);
        if !indent.is_empty() && block.leading.tokens.is_empty() {
            block
                .leading
                .push(Token::new(TokenKind::Whitespace, &indent, dummy_span));
        }
        self.items.push(CstItem::Block(block));
    }
    /// Recursively updates indentation levels on all contained attributes and blocks.
    ///
    /// # Arguments
    /// * `indent_level` - The indentation depth level to apply.
    /// * `indent_str` - The indentation unit string (e.g. `"  "`).
    pub fn set_indent_level_recursive(&mut self, indent_level: usize, indent_str: &str) {
        self.indent_level = indent_level;
        self.indent_str = indent_str.to_string();
        let indent = indent_str.repeat(indent_level);
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        for item in &mut self.items {
            match item {
                CstItem::Attribute(attr) => {
                    let comments: Vec<Token> = attr
                        .leading
                        .tokens
                        .drain(..)
                        .filter(|t| {
                            t.kind == TokenKind::Comment || t.kind == TokenKind::InlineComment
                        })
                        .collect();
                    if !indent.is_empty() {
                        attr.leading.push(Token::new(
                            TokenKind::Whitespace,
                            &indent,
                            dummy_span.clone(),
                        ));
                    }
                    attr.leading.tokens.extend(comments);
                }
                CstItem::Block(block) => {
                    let block_indent = indent_str.repeat(indent_level);
                    let comments: Vec<Token> = block
                        .leading
                        .tokens
                        .drain(..)
                        .filter(|t| {
                            t.kind == TokenKind::Comment || t.kind == TokenKind::InlineComment
                        })
                        .collect();
                    if !block_indent.is_empty() {
                        block.leading.push(Token::new(
                            TokenKind::Whitespace,
                            &block_indent,
                            dummy_span.clone(),
                        ));
                    }
                    block.leading.tokens.extend(comments);
                    block
                        .body
                        .set_indent_level_recursive(indent_level + 1, indent_str);
                }
                CstItem::Trivia(_) => {}
            }
        }
    }
    /// Appends a blank newline token to the body.
    pub fn append_newline(&mut self) {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let mut stream = TokenStream::new();
        stream.push(Token::new(TokenKind::Newline, "\n", dummy_span.clone()));
        self.items.push(CstItem::Trivia(stream));
        self.trailing
            .push(Token::new(TokenKind::Newline, "\n", dummy_span));
    }
    /// Compacts trivia within the body to eliminate dangling blank lines and orphan whitespace.
    pub fn compact_trivia(&mut self) {
        if self.items.is_empty() {
            self.trailing.tokens.clear();
            return;
        }
        while let Some(first) = self.items.first() {
            if let CstItem::Trivia(stream) = first
                && stream
                    .tokens
                    .iter()
                    .all(|t| t.kind == TokenKind::Whitespace || t.kind == TokenKind::Newline)
            {
                self.items.remove(0);
                continue;
            }
            break;
        }
        while let Some(last) = self.items.last() {
            if let CstItem::Trivia(stream) = last
                && stream
                    .tokens
                    .iter()
                    .all(|t| t.kind == TokenKind::Whitespace || t.kind == TokenKind::Newline)
            {
                self.items.pop();
                continue;
            }
            break;
        }
        let mut i = 0;
        while i + 1 < self.items.len() {
            if let (CstItem::Trivia(t1), CstItem::Trivia(t2)) = (&self.items[i], &self.items[i + 1])
            {
                let t1_blank = t1
                    .tokens
                    .iter()
                    .all(|t| t.kind == TokenKind::Whitespace || t.kind == TokenKind::Newline);
                let t2_blank = t2
                    .tokens
                    .iter()
                    .all(|t| t.kind == TokenKind::Whitespace || t.kind == TokenKind::Newline);
                if t1_blank && t2_blank {
                    self.items.remove(i + 1);
                    continue;
                }
            }
            i += 1;
        }
        let expected_indent = self.indent_str.repeat(self.indent_level);
        if !expected_indent.is_empty() {
            match &mut self.items[0] {
                CstItem::Attribute(attr) => {
                    let has_indent = attr.leading.tokens.iter().any(|t| {
                        t.kind == TokenKind::Whitespace && t.text.contains(&expected_indent)
                    });
                    if !has_indent {
                        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
                        attr.leading.tokens.insert(
                            0,
                            Token::new(TokenKind::Whitespace, &expected_indent, dummy_span),
                        );
                    }
                }
                CstItem::Block(blk) => {
                    let has_indent = blk.leading.tokens.iter().any(|t| {
                        t.kind == TokenKind::Whitespace && t.text.contains(&expected_indent)
                    });
                    if !has_indent {
                        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
                        blk.leading.tokens.insert(
                            0,
                            Token::new(TokenKind::Whitespace, &expected_indent, dummy_span),
                        );
                    }
                }
                CstItem::Trivia(_) => {}
            }
        }
    }
    /// Removes an attribute by name, returning the removed attribute if found, and compacting surrounding trivia.
    ///
    /// # Arguments
    /// * `name` - The attribute name to remove.
    ///
    /// # Returns
    /// The removed [`CstAttribute`] if found.
    pub fn remove_attribute(&mut self, name: &str) -> Option<CstAttribute> {
        let mut removed_item = None;
        self.items.retain_mut(|item| {
            if removed_item.is_none()
                && let CstItem::Attribute(a) = item
                && a.name == name
            {
                removed_item = Some(a.clone());
                return false;
            }
            true
        });
        self.compact_trivia();
        removed_item
    }
    /// Returns a mutable reference to the first block matching block type and labels in items.
    ///
    /// # Arguments
    /// * `block_type` - The block type to match.
    /// * `labels` - The label strings to match.
    pub fn block_mut(&mut self, block_type: &str, labels: &[&str]) -> Option<&mut CstBlock> {
        self.items.iter_mut().find_map(|item| match item {
            CstItem::Block(b) => {
                if b.block_type == block_type && b.labels.len() == labels.len() {
                    let matches = b
                        .labels
                        .iter()
                        .zip(labels.iter())
                        .all(|(act, exp)| act == exp);
                    if matches {
                        return Some(b);
                    }
                }
                None
            }
            _ => None,
        })
    }
    /// Removes all blocks matching the given block type and labels, returning the removed block if found.
    ///
    /// # Arguments
    /// * `block_type` - The block type to match.
    /// * `labels` - The label strings to match.
    ///
    /// # Returns
    /// The removed [`CstBlock`] if found.
    pub fn remove_block(&mut self, block_type: &str, labels: &[&str]) -> Option<CstBlock> {
        let matches_block = |b: &CstBlock| -> bool {
            if b.block_type != block_type || b.labels.len() != labels.len() {
                return false;
            }
            b.labels
                .iter()
                .zip(labels.iter())
                .all(|(act, exp)| act == exp)
        };
        let mut removed_item = None;
        self.items.retain_mut(|item| {
            if removed_item.is_none()
                && let CstItem::Block(b) = item
                && matches_block(b)
            {
                removed_item = Some(b.clone());
                return false;
            }
            true
        });
        self.compact_trivia();
        removed_item
    }
    /// Renames all occurrences of a variable identifier token across this body and all nested blocks.
    ///
    /// # Arguments
    /// * `old_name` - The identifier to search for and rename.
    /// * `new_name` - The replacement identifier.
    ///
    /// # Returns
    /// Number of identifier tokens renamed.
    pub fn rename_variable(&mut self, old_name: &str, new_name: &str) -> usize {
        let mut count = 0;
        for item in &mut self.items {
            match item {
                CstItem::Attribute(attr) => {
                    count += attr.rename_variable(old_name, new_name);
                }
                CstItem::Block(block) => {
                    count += block.body.rename_variable(old_name, new_name);
                }
                CstItem::Trivia(_) => {}
            }
        }
        count
    }
    /// Renders this body to a formatted HCL string.
    ///
    /// # Arguments
    /// * `out` - The output string buffer.
    pub fn render(&self, out: &mut String) {
        for item in &self.items {
            match item {
                CstItem::Attribute(attr) => Self::render_cst_attribute(attr, out),
                CstItem::Block(block) => self.render_cst_block(block, out),
                CstItem::Trivia(stream) => {
                    for t in &stream.tokens {
                        out.push_str(&t.text);
                    }
                }
            }
        }
        for t in &self.trailing.tokens {
            out.push_str(&t.text);
        }
    }
    fn render_cst_attribute(attr: &CstAttribute, out: &mut String) {
        for t in &attr.leading.tokens {
            out.push_str(&t.text);
        }
        out.push_str(&attr.name);
        out.push_str(" =");
        let expr_str: String = attr.expr_tokens.iter().map(|t| t.text.as_str()).collect();
        if !expr_str.starts_with(' ') {
            out.push(' ');
        }
        out.push_str(&expr_str);
        for t in &attr.trailing.tokens {
            out.push_str(&t.text);
        }
    }
    fn render_cst_block(&self, block: &CstBlock, out: &mut String) {
        for t in &block.leading.tokens {
            out.push_str(&t.text);
        }
        out.push_str(&block.block_type);
        for lbl in &block.labels {
            let _ = write!(out, " {lbl:?}");
        }
        out.push_str(" {\n");
        block.body.render(out);
        let indent = self.indent_str.repeat(self.indent_level);
        out.push_str(&indent);
        out.push('}');
        if block.trailing.tokens.is_empty() {
            out.push('\n');
        } else {
            for t in &block.trailing.tokens {
                out.push_str(&t.text);
            }
        }
    }
}
/// A complete CST file representing an HCL configuration file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CstFile {
    /// Root body of the file.
    pub body: CstBody,
}
impl CstFile {
    /// Creates a new, empty `CstFile`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            body: CstBody::new(),
        }
    }
    /// Returns a reference to the root body.
    #[must_use]
    pub const fn body(&self) -> &CstBody {
        &self.body
    }
    /// Returns a mutable reference to the root body.
    #[must_use]
    pub const fn body_mut(&mut self) -> &mut CstBody {
        &mut self.body
    }
    /// Removes an attribute by name from the root body.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    ///
    /// # Returns
    /// The removed [`CstAttribute`] if found.
    pub fn remove_attribute(&mut self, name: &str) -> Option<CstAttribute> {
        self.body.remove_attribute(name)
    }
    /// Removes a block matching type and labels from the root body.
    ///
    /// # Arguments
    /// * `block_type` - The block type identifier.
    /// * `labels` - The block labels to match against.
    ///
    /// # Returns
    /// The removed [`CstBlock`] if found.
    pub fn remove_block(&mut self, block_type: &str, labels: &[&str]) -> Option<CstBlock> {
        self.body.remove_block(block_type, labels)
    }
    /// Returns a mutable reference to a block matching type and labels in the root body.
    ///
    /// # Arguments
    /// * `block_type` - The block type identifier.
    /// * `labels` - The block labels to match against.
    pub fn block_mut(&mut self, block_type: &str, labels: &[&str]) -> Option<&mut CstBlock> {
        self.body.block_mut(block_type, labels)
    }
    /// Replaces an attribute's expression with a new AST [`Expression`].
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    /// * `new_expr` - The new expression to assign.
    ///
    /// # Errors
    /// Returns [`crate::error::HclError::MissingField`] if the attribute does not exist.
    pub fn replace_attribute_expr(
        &mut self,
        name: &str,
        new_expr: &Expression,
    ) -> Result<(), crate::error::HclError> {
        self.body.replace_attribute_expr(name, new_expr)
    }
    /// Renames all occurrences of a variable identifier token across the entire file.
    ///
    /// # Arguments
    /// * `old_name` - The identifier to search for and rename.
    /// * `new_name` - The replacement identifier.
    ///
    /// # Returns
    /// Number of identifier tokens renamed.
    pub fn rename_variable(&mut self, old_name: &str, new_name: &str) -> usize {
        self.body.rename_variable(old_name, new_name)
    }
    /// Serializes the CST file into a formatted HCL string.
    #[must_use]
    pub fn write_to_string(&self) -> String {
        let mut out = String::new();
        self.body.render(&mut out);
        out
    }
    /// Parses an HCL input string into a `CstFile`.
    ///
    /// # Arguments
    /// * `input` - The source text.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if lexical or CST parsing encounters errors.
    pub fn parse(input: &str) -> Result<Self, Diagnostics> {
        let parser = CstParser::new(input);
        let doc = parser
            .parse()
            .map_err(|e| Diagnostics::from(Diagnostic::new(e, Span::new(0, 0, 0, 0, 0, 0))))?;
        Ok(from_document(&doc))
    }
}
impl fmt::Display for CstFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.write_to_string())
    }
}
fn from_document(doc: &Document) -> CstFile {
    let mut file = CstFile::new();
    file.body = from_doc_to_body(doc, 0);
    file
}
fn from_doc_to_body(doc: &Document, indent_level: usize) -> CstBody {
    let mut body = CstBody::with_indent_level(indent_level);
    for item in &doc.items {
        match item {
            DocumentItem::Attribute(attr) => {
                let mut cst_attr = CstAttribute::new(&attr.name.text, attr.expr_tokens.clone());
                cst_attr.leading.clone_from(&attr.leading);
                cst_attr.trailing.clone_from(&attr.trailing);
                body.items.push(CstItem::Attribute(cst_attr));
            }
            DocumentItem::Block(block) => {
                let labels = block
                    .labels
                    .iter()
                    .filter(|t| t.kind == TokenKind::Ident || t.kind == TokenKind::String)
                    .map(|tok| {
                        let txt = &tok.text;
                        txt.strip_prefix('"')
                            .and_then(|s| s.strip_suffix('"'))
                            .map_or_else(|| txt.clone(), ToString::to_string)
                    })
                    .collect();
                let mut cst_block = CstBlock::new(&block.type_ident.text, labels);
                cst_block.leading.clone_from(&block.leading);
                cst_block.trailing.clone_from(&block.trailing);
                cst_block.body = from_doc_to_body(&block.body, indent_level + 1);
                body.items.push(CstItem::Block(cst_block));
            }
            DocumentItem::Trivia(stream) => {
                body.items.push(CstItem::Trivia(stream.clone()));
            }
        }
    }
    body.trailing.clone_from(&doc.trailing);
    body
}
fn value_to_tokens(val: &Value, span: &Span) -> Vec<Token> {
    match &*val.data {
        ValueData::Null => vec![Token::new(TokenKind::Ident, "null", span.clone())],
        ValueData::Bool(b) => {
            vec![Token::new(
                TokenKind::Ident,
                if *b { "true" } else { "false" },
                span.clone(),
            )]
        }
        ValueData::Number(n) => {
            vec![Token::new(TokenKind::Number, n.0.to_string(), span.clone())]
        }
        ValueData::String(s) => {
            vec![Token::new(
                TokenKind::String,
                format!("{s:?}"),
                span.clone(),
            )]
        }
        ValueData::Array(elements) => {
            let mut toks = vec![Token::new(TokenKind::OBrack, "[", span.clone())];
            for (idx, elem) in elements.iter().enumerate() {
                if idx > 0 {
                    toks.push(Token::new(TokenKind::Comma, ",", span.clone()));
                    toks.push(Token::new(TokenKind::Whitespace, " ", span.clone()));
                }
                toks.extend(value_to_tokens(elem, span));
            }
            toks.push(Token::new(TokenKind::CBrack, "]", span.clone()));
            toks
        }
        ValueData::Set(set) => {
            let mut toks = vec![Token::new(TokenKind::OBrack, "[", span.clone())];
            for (idx, elem) in set.iter().enumerate() {
                if idx > 0 {
                    toks.push(Token::new(TokenKind::Comma, ",", span.clone()));
                    toks.push(Token::new(TokenKind::Whitespace, " ", span.clone()));
                }
                toks.extend(value_to_tokens(elem, span));
            }
            toks.push(Token::new(TokenKind::CBrack, "]", span.clone()));
            toks
        }
        ValueData::Object(pairs) => {
            let mut toks = vec![
                Token::new(TokenKind::OBrace, "{", span.clone()),
                Token::new(TokenKind::Newline, "\n", span.clone()),
            ];
            for (k, v) in pairs {
                toks.push(Token::new(TokenKind::Whitespace, "  ", span.clone()));
                toks.push(Token::new(TokenKind::Ident, k, span.clone()));
                toks.push(Token::new(TokenKind::Whitespace, " = ", span.clone()));
                toks.extend(value_to_tokens(v, span));
                toks.push(Token::new(TokenKind::Newline, "\n", span.clone()));
            }
            toks.push(Token::new(TokenKind::CBrace, "}", span.clone()));
            toks
        }
        ValueData::Unknown(_) => {
            vec![Token::new(TokenKind::Ident, "unknown", span.clone())]
        }
        ValueData::Capsule(_) => {
            vec![Token::new(TokenKind::Ident, "capsule", span.clone())]
        }
    }
}
/// Synthesizes concrete [`Token`]s representing a typed [`Value`].
///
/// # Arguments
/// * `val` - The value to synthesize tokens for.
#[must_use]
pub fn tokens_for_value(val: &Value) -> Vec<Token> {
    value_to_tokens(val, &Span::new(0, 0, 1, 1, 1, 1))
}
/// Synthesizes concrete [`Token`]s representing an AST [`Traversal`].
///
/// # Arguments
/// * `traversal` - The traversal to synthesize tokens for.
#[must_use]
pub fn tokens_for_traversal(traversal: &Traversal) -> Vec<Token> {
    let mut tokens = tokens_for_expression(&traversal.expr);
    for op in &traversal.operators {
        match op {
            TraversalOperator::GetAttr(name, span) => {
                tokens.push(Token::new(TokenKind::Dot, ".", span.clone()));
                tokens.push(Token::new(TokenKind::Ident, name, span.clone()));
            }
            TraversalOperator::Index(expr, span) => {
                tokens.push(Token::new(TokenKind::OBrack, "[", span.clone()));
                tokens.extend(tokens_for_expression(expr));
                tokens.push(Token::new(TokenKind::CBrack, "]", span.clone()));
            }
            TraversalOperator::LegacyIndex(idx, span) => {
                tokens.push(Token::new(TokenKind::Dot, ".", span.clone()));
                tokens.push(Token::new(TokenKind::Number, idx.to_string(), span.clone()));
            }
            TraversalOperator::AttrSplat(span) => {
                tokens.push(Token::new(TokenKind::Dot, ".", span.clone()));
                tokens.push(Token::new(TokenKind::Star, "*", span.clone()));
            }
            TraversalOperator::FullSplat(span) => {
                tokens.push(Token::new(TokenKind::OBrack, "[", span.clone()));
                tokens.push(Token::new(TokenKind::Star, "*", span.clone()));
                tokens.push(Token::new(TokenKind::CBrack, "]", span.clone()));
            }
        }
    }
    tokens
}
/// Synthesizes concrete [`Token`]s for a [`crate::ast::expr::ForExpr`].
fn tokens_for_for_expr(for_expr: &crate::ast::expr::ForExpr, span: &Span) -> Vec<Token> {
    let is_tuple = for_expr.key_expr.is_none();
    let mut toks = vec![Token::new(
        if is_tuple {
            TokenKind::OBrack
        } else {
            TokenKind::OBrace
        },
        if is_tuple { "[" } else { "{" },
        span.clone(),
    )];
    toks.push(Token::new(TokenKind::Ident, "for", span.clone()));
    toks.push(Token::new(TokenKind::Whitespace, " ", span.clone()));
    if let Some(ref k) = for_expr.key_var {
        toks.push(Token::new(TokenKind::Ident, k, span.clone()));
        toks.push(Token::new(TokenKind::Comma, ",", span.clone()));
        toks.push(Token::new(TokenKind::Whitespace, " ", span.clone()));
    }
    toks.push(Token::new(
        TokenKind::Ident,
        &for_expr.val_var,
        span.clone(),
    ));
    toks.push(Token::new(TokenKind::Whitespace, " in ", span.clone()));
    toks.extend(tokens_for_expression(&for_expr.collection));
    toks.push(Token::new(TokenKind::Whitespace, " : ", span.clone()));
    if let Some(ref k_expr) = for_expr.key_expr {
        toks.extend(tokens_for_expression(k_expr));
        toks.push(Token::new(TokenKind::Whitespace, " => ", span.clone()));
    }
    toks.extend(tokens_for_expression(&for_expr.val_expr));
    if for_expr.grouping {
        toks.push(Token::new(TokenKind::Ellipsis, "...", span.clone()));
    }
    if let Some(ref cond) = for_expr.cond_expr {
        toks.push(Token::new(TokenKind::Whitespace, " if ", span.clone()));
        toks.extend(tokens_for_expression(cond));
    }
    toks.push(Token::new(
        if is_tuple {
            TokenKind::CBrack
        } else {
            TokenKind::CBrace
        },
        if is_tuple { "]" } else { "}" },
        span.clone(),
    ));
    toks
}

const fn binary_op_str(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Eq => "==",
        BinaryOp::NotEq => "!=",
        BinaryOp::Less => "<",
        BinaryOp::LessEq => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEq => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
    }
}

fn tokens_for_func_call(f: &crate::ast::expr::FuncCall, span: &Span) -> Vec<Token> {
    let mut toks = vec![
        Token::new(TokenKind::Ident, f.name.to_string(), span.clone()),
        Token::new(TokenKind::OParen, "(", span.clone()),
    ];
    for (idx, arg) in f.args.iter().enumerate() {
        if idx > 0 {
            toks.push(Token::new(TokenKind::Comma, ",", span.clone()));
            toks.push(Token::new(TokenKind::Whitespace, " ", span.clone()));
        }
        toks.extend(tokens_for_expression(arg));
    }
    if f.expand_final {
        toks.push(Token::new(TokenKind::Ellipsis, "...", span.clone()));
    }
    toks.push(Token::new(TokenKind::CParen, ")", span.clone()));
    toks
}

/// Synthesizes concrete [`Token`]s representing an AST [`Expression`].
///
/// # Arguments
/// * `expr` - The expression to synthesize tokens for.
#[must_use]
pub fn tokens_for_expression(expr: &Expression) -> Vec<Token> {
    match expr {
        Expression::Null(span) => vec![Token::new(TokenKind::Ident, "null", span.clone())],
        Expression::Bool(b, span) => vec![Token::new(
            TokenKind::Ident,
            if *b { "true" } else { "false" },
            span.clone(),
        )],
        Expression::Number(n, span) => {
            vec![Token::new(TokenKind::Number, n.0.to_string(), span.clone())]
        }
        Expression::String(s, span) => {
            vec![Token::new(
                TokenKind::String,
                format!("{s:?}"),
                span.clone(),
            )]
        }
        Expression::Variable(name, span) => {
            vec![Token::new(TokenKind::Ident, name, span.clone())]
        }
        Expression::Tuple(elements, span) => {
            let mut toks = vec![Token::new(TokenKind::OBrack, "[", span.clone())];
            for (idx, elem) in elements.iter().enumerate() {
                if idx > 0 {
                    toks.push(Token::new(TokenKind::Comma, ",", span.clone()));
                    toks.push(Token::new(TokenKind::Whitespace, " ", span.clone()));
                }
                toks.extend(tokens_for_expression(elem));
            }
            toks.push(Token::new(TokenKind::CBrack, "]", span.clone()));
            toks
        }
        Expression::Object(pairs, span) => {
            let mut toks = vec![
                Token::new(TokenKind::OBrace, "{", span.clone()),
                Token::new(TokenKind::Newline, "\n", span.clone()),
            ];
            for (k, v) in pairs {
                toks.push(Token::new(TokenKind::Whitespace, "  ", span.clone()));
                toks.extend(tokens_for_expression(k));
                toks.push(Token::new(TokenKind::Whitespace, " = ", span.clone()));
                toks.extend(tokens_for_expression(v));
                toks.push(Token::new(TokenKind::Newline, "\n", span.clone()));
            }
            toks.push(Token::new(TokenKind::CBrace, "}", span.clone()));
            toks
        }
        Expression::BinaryOp(op, left, right, span) => {
            let mut toks = tokens_for_expression(left);
            toks.push(Token::new(TokenKind::Whitespace, " ", span.clone()));
            toks.push(Token::new(
                TokenKind::Ident,
                binary_op_str(*op),
                span.clone(),
            ));
            toks.push(Token::new(TokenKind::Whitespace, " ", span.clone()));
            toks.extend(tokens_for_expression(right));
            toks
        }
        Expression::UnaryOp(op, inner, span) => {
            let op_str = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            let mut toks = vec![Token::new(TokenKind::Ident, op_str, span.clone())];
            toks.extend(tokens_for_expression(inner));
            toks
        }
        Expression::Conditional(cond, span) => {
            let mut toks = tokens_for_expression(&cond.cond_expr);
            toks.push(Token::new(TokenKind::Whitespace, " ? ", span.clone()));
            toks.extend(tokens_for_expression(&cond.true_expr));
            toks.push(Token::new(TokenKind::Whitespace, " : ", span.clone()));
            toks.extend(tokens_for_expression(&cond.false_expr));
            toks
        }
        Expression::FuncCall(f, span) => tokens_for_func_call(f, span),
        Expression::Traversal(t, _span) => tokens_for_traversal(t),
        Expression::ForExpr(for_expr, span) => tokens_for_for_expr(for_expr, span),
        Expression::Template(parts, span) => {
            let mut out = String::from("\"");
            for part in parts {
                format_template_part(part, &mut out);
            }
            out.push('"');
            vec![Token::new(TokenKind::String, out, span.clone())]
        }
        Expression::Parentheses(inner, span) => {
            let mut toks = vec![Token::new(TokenKind::OParen, "(", span.clone())];
            toks.extend(tokens_for_expression(inner));
            toks.push(Token::new(TokenKind::CParen, ")", span.clone()));
            toks
        }
    }
}
/// Formats a template directive into its canonical HCL string representation.
///
/// # Arguments
/// * `dir` - The directive to format.
/// * `out` - The output string buffer.
fn format_directive(dir: &crate::ast::expr::Directive, out: &mut String) {
    match dir {
        crate::ast::expr::Directive::If {
            cond,
            true_expr,
            else_ifs,
            false_expr,
        } => {
            out.push_str("%{if ");
            for t in tokens_for_expression(cond) {
                out.push_str(&t.text);
            }
            out.push('}');
            for part in true_expr {
                format_template_part(part, out);
            }
            for (elif_cond, elif_body) in else_ifs {
                out.push_str("%{else if ");
                for t in tokens_for_expression(elif_cond) {
                    out.push_str(&t.text);
                }
                out.push('}');
                for part in elif_body {
                    format_template_part(part, out);
                }
            }
            if let Some(false_parts) = false_expr {
                out.push_str("%{else}");
                for part in false_parts {
                    format_template_part(part, out);
                }
            }
            out.push_str("%{endif}");
        }
        crate::ast::expr::Directive::For {
            key_var,
            val_var,
            collection,
            body,
        } => {
            out.push_str("%{for ");
            if let Some(k) = key_var {
                out.push_str(k);
                out.push_str(", ");
            }
            out.push_str(val_var);
            out.push_str(" in ");
            for t in tokens_for_expression(collection) {
                out.push_str(&t.text);
            }
            out.push('}');
            for part in body {
                format_template_part(part, out);
            }
            out.push_str("%{endfor}");
        }
        crate::ast::expr::Directive::Strip {
            strip_left,
            strip_right,
        } => {
            out.push_str("%{");
            if *strip_left {
                out.push('~');
            }
            if *strip_right {
                out.push('~');
            }
            out.push('}');
        }
    }
}
/// Formats an individual template part into an output string buffer.
///
/// # Arguments
/// * `part` - The template part to format.
/// * `out` - The output string buffer.
fn format_template_part(part: &TemplatePart, out: &mut String) {
    match part {
        TemplatePart::Literal(s, _) => out.push_str(s),
        TemplatePart::Interpolation(expr, _) => {
            out.push_str("${");
            for t in tokens_for_expression(expr) {
                out.push_str(&t.text);
            }
            out.push('}');
        }
        TemplatePart::Directive(dir, _) => {
            format_directive(dir, out);
        }
    }
}
#[cfg(test)]
#[path = "builder_tests.rs"]
mod tests;
