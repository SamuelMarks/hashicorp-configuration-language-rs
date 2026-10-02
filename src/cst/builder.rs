//! Programmatic CST builders and mutation engine (`hclwrite`).
//!
//! Provides [`CstFile`], [`CstBody`], [`CstBlock`], and [`CstAttribute`] for creating,
//! mutating, and serializing concrete syntax trees while preserving comments,
//! indentation, and whitespace trivia.
use std::fmt::Write;

use crate::ast::expr::{BinaryOp, Expression, TemplatePart, Traversal, TraversalOperator, UnaryOp};
use crate::cst::document::{Document, DocumentItem, TokenStream};
use crate::cst::parser::CstParser;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::lex::token::{Token, TokenKind};
use crate::span::Span;
use crate::types::val::{Value, ValueData};
use std::fmt;

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
    pub fn as_attribute(&self) -> Option<&CstAttribute> {
        match self {
            Self::Attribute(a) => Some(a),
            _ => None,
        }
    }

    /// Returns the item as a block if it is a block.
    #[must_use]
    pub fn as_block(&self) -> Option<&CstBlock> {
        match self {
            Self::Block(b) => Some(b),
            _ => None,
        }
    }
}

/// A CST attribute with name, expression tokens, and formatting trivia.
#[derive(Debug, Clone, PartialEq)]
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
    pub fn body(&self) -> &CstBody {
        &self.body
    }

    /// Returns a mutable reference to the inner body.
    #[must_use]
    pub fn body_mut(&mut self) -> &mut CstBody {
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
    /// Attributes in the body.
    pub attributes: Vec<CstAttribute>,
    /// Blocks in the body.
    pub blocks: Vec<CstBlock>,
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
            attributes: Vec::new(),
            blocks: Vec::new(),
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
            attributes: Vec::new(),
            blocks: Vec::new(),
            indent_level,
            indent_str: "  ".to_string(),
            trailing: TokenStream::new(),
        }
    }

    /// Returns the attributes in this body.
    #[must_use]
    pub fn attributes(&self) -> &[CstAttribute] {
        &self.attributes
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
    pub fn blocks(&self) -> &[CstBlock] {
        &self.blocks
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
        if let Some(existing) = self.attributes.iter_mut().find(|a| a.name == name) {
            existing.expr_tokens.clone_from(&tokens);
            for item in &mut self.items {
                if let CstItem::Attribute(a) = item
                    && a.name == name
                {
                    a.expr_tokens.clone_from(&tokens);
                    break;
                }
            }
        } else {
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
            self.items.push(CstItem::Attribute(attr.clone()));
            self.attributes.push(attr);
        }
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
        if let Some(attr) = self.attributes.iter_mut().find(|a| a.name == name) {
            let tokens = tokens_for_expression(new_expr);
            attr.expr_tokens.clone_from(&tokens);
            for item in &mut self.items {
                if let CstItem::Attribute(a) = item
                    && a.name == name
                {
                    a.expr_tokens = tokens;
                    break;
                }
            }
            Ok(())
        } else {
            Err(crate::error::HclError::MissingField(name.to_string()))
        }
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

        self.items.push(CstItem::Block(block.clone()));
        self.blocks.push(block);
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

        for attr in &mut self.attributes {
            let comments: Vec<Token> = attr
                .leading
                .tokens
                .drain(..)
                .filter(|t| t.kind == TokenKind::Comment || t.kind == TokenKind::InlineComment)
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
        for block in &mut self.blocks {
            let block_indent = indent_str.repeat(indent_level);
            let comments: Vec<Token> = block
                .leading
                .tokens
                .drain(..)
                .filter(|t| t.kind == TokenKind::Comment || t.kind == TokenKind::InlineComment)
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

        // 1. Remove leading trivia items that consist solely of blank newlines or whitespace
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

        // 2. Remove trailing trivia items that consist solely of blank newlines or whitespace
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

        // 3. Compact multiple consecutive blank line trivia into a single blank line
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

        // 4. Ensure the first item in the block has correct indentation
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
                false
            } else {
                true
            }
        });

        let removed_attr = self
            .attributes
            .iter()
            .position(|a| a.name == name)
            .map(|pos| self.attributes.remove(pos));

        self.compact_trivia();
        removed_item.or(removed_attr)
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
                false
            } else {
                true
            }
        });

        let removed_block = self
            .blocks
            .iter()
            .position(matches_block)
            .map(|pos| self.blocks.remove(pos));

        self.compact_trivia();
        removed_item.or(removed_block)
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
        for attr in &mut self.attributes {
            count += attr.rename_variable(old_name, new_name);
        }
        for item in &mut self.items {
            match item {
                CstItem::Attribute(attr) => {
                    let _ = attr.rename_variable(old_name, new_name);
                }
                CstItem::Block(block) => {
                    count += block.body.rename_variable(old_name, new_name);
                }
                CstItem::Trivia(_) => {}
            }
        }
        for block in &mut self.blocks {
            let _ = block.body.rename_variable(old_name, new_name);
        }
        count
    }

    /// Renders this body to a formatted HCL string.
    ///
    /// # Arguments
    /// * `out` - The output string buffer.
    pub fn render(&self, out: &mut String) {
        if self.items.is_empty() {
            for attr in &self.attributes {
                Self::render_cst_attribute(attr, out);
            }
            for block in &self.blocks {
                self.render_cst_block(block, out);
            }
        } else {
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
    pub fn body(&self) -> &CstBody {
        &self.body
    }

    /// Returns a mutable reference to the root body.
    #[must_use]
    pub fn body_mut(&mut self) -> &mut CstBody {
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

    if doc.items.is_empty() {
        for attr in &doc.attributes {
            let mut cst_attr = CstAttribute::new(&attr.name.text, attr.expr_tokens.clone());
            cst_attr.leading.clone_from(&attr.leading);
            cst_attr.trailing.clone_from(&attr.trailing);
            body.items.push(CstItem::Attribute(cst_attr.clone()));
            body.attributes.push(cst_attr);
        }

        for block in &doc.blocks {
            let labels = block
                .labels
                .iter()
                .filter(|t| t.kind == TokenKind::Ident || t.kind == TokenKind::String)
                .map(|tok| {
                    let txt = &tok.text;
                    if let Some(unquoted) = txt.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
                    {
                        unquoted.to_string()
                    } else {
                        txt.clone()
                    }
                })
                .collect();

            let mut cst_block = CstBlock::new(&block.type_ident.text, labels);
            cst_block.leading.clone_from(&block.leading);
            cst_block.trailing.clone_from(&block.trailing);
            cst_block.body = from_doc_to_body(&block.body, indent_level + 1);
            body.items.push(CstItem::Block(cst_block.clone()));
            body.blocks.push(cst_block);
        }
    } else {
        for item in &doc.items {
            match item {
                DocumentItem::Attribute(attr) => {
                    let mut cst_attr = CstAttribute::new(&attr.name.text, attr.expr_tokens.clone());
                    cst_attr.leading.clone_from(&attr.leading);
                    cst_attr.trailing.clone_from(&attr.trailing);
                    body.items.push(CstItem::Attribute(cst_attr.clone()));
                    body.attributes.push(cst_attr);
                }
                DocumentItem::Block(block) => {
                    let labels = block
                        .labels
                        .iter()
                        .filter(|t| t.kind == TokenKind::Ident || t.kind == TokenKind::String)
                        .map(|tok| {
                            let txt = &tok.text;
                            if let Some(unquoted) =
                                txt.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
                            {
                                unquoted.to_string()
                            } else {
                                txt.clone()
                            }
                        })
                        .collect();

                    let mut cst_block = CstBlock::new(&block.type_ident.text, labels);
                    cst_block.leading.clone_from(&block.leading);
                    cst_block.trailing.clone_from(&block.trailing);
                    cst_block.body = from_doc_to_body(&block.body, indent_level + 1);
                    body.items.push(CstItem::Block(cst_block.clone()));
                    body.blocks.push(cst_block);
                }
                DocumentItem::Trivia(stream) => {
                    body.items.push(CstItem::Trivia(stream.clone()));
                }
            }
        }
    }

    body.trailing.clone_from(&doc.trailing);
    body
}

fn value_to_tokens(val: &Value, span: &Span) -> Vec<Token> {
    match &*val.data {
        ValueData::Null => vec![Token::new(TokenKind::Ident, "null", span.clone())],
        ValueData::Bool(b) => vec![Token::new(
            TokenKind::Ident,
            if *b { "true" } else { "false" },
            span.clone(),
        )],
        ValueData::Number(n) => vec![Token::new(TokenKind::Number, n.0.to_string(), span.clone())],
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
        ValueData::Unknown(_) => vec![Token::new(TokenKind::Ident, "unknown", span.clone())],
        ValueData::Capsule(_) => vec![Token::new(TokenKind::Ident, "capsule", span.clone())],
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
            let op_str = match op {
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
            };
            toks.push(Token::new(TokenKind::Ident, op_str, span.clone()));
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
        Expression::FuncCall(f, span) => {
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
        Expression::Traversal(t, _span) => tokens_for_traversal(t),
        Expression::ForExpr(for_expr, span) => {
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
        Expression::Template(parts, span) => {
            let mut out = String::new();
            out.push('"');
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
    use crate::types::ty::Type;
    use std::collections::BTreeMap;
    use std::str::FromStr;

    #[test]
    fn test_cst_attribute_and_block_getters() {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let attr = CstAttribute::new("foo", vec![Token::new(TokenKind::Ident, "bar", span)]);
        assert_eq!(attr.name(), "foo");
        assert_eq!(attr.expr_tokens().len(), 1);

        let mut block = CstBlock::new("res", vec!["lbl".to_string()]);
        assert_eq!(block.block_type(), "res");
        assert_eq!(block.labels(), &["lbl"]);
        assert_eq!(block.body().attributes().len(), 0);

        block.body_mut().set_attribute_raw("key", vec![]);
        assert_eq!(block.body().attributes().len(), 1);
    }

    #[test]
    fn test_cst_file_builders_and_mutations() {
        let mut file = CstFile::new();
        let span = Span::new(0, 0, 0, 0, 0, 0);

        file.body_mut().set_attribute_value(
            "name",
            &Value::new(Type::String, ValueData::String("my_bucket".to_string())),
        );
        file.body_mut().set_attribute_value(
            "count",
            &Value::new(
                Type::Number,
                ValueData::Number(Number::from_str("3").unwrap()),
            ),
        );
        file.body_mut()
            .set_attribute_value("enabled", &Value::new(Type::Bool, ValueData::Bool(true)));
        file.body_mut()
            .set_attribute_value("tags", &Value::new(Type::Dynamic, ValueData::Null));
        file.body_mut().set_attribute_value(
            "list",
            &Value::new(
                Type::Dynamic,
                ValueData::Array(vec![
                    Value::new(Type::Bool, ValueData::Bool(false)),
                    Value::new(Type::Bool, ValueData::Bool(true)),
                ]),
            ),
        );
        let mut obj = BTreeMap::new();
        obj.insert(
            "k".to_string(),
            Value::new(Type::String, ValueData::String("v".to_string())),
        );
        file.body_mut()
            .set_attribute_value("map", &Value::new(Type::Dynamic, ValueData::Object(obj)));
        let mut set = std::collections::BTreeSet::new();
        set.insert(Value::new(Type::String, ValueData::String("s".to_string())));
        file.body_mut()
            .set_attribute_value("set", &Value::new(Type::Dynamic, ValueData::Set(set)));
        file.body_mut()
            .set_attribute_value("unk", &Value::unknown(Type::Dynamic));
        file.body_mut()
            .set_attribute_value("cap", &Value::capsule("custom", 123_u32));

        assert_eq!(file.body().attributes().len(), 9);

        // Replace attribute
        file.body_mut().set_attribute_raw(
            "name",
            vec![Token::new(TokenKind::String, r#""updated""#, span.clone())],
        );
        assert_eq!(file.body().attributes().len(), 9);

        // Remove attribute
        let removed = file.body_mut().remove_attribute("tags");
        assert!(removed.is_some());
        assert_eq!(file.body().attributes().len(), 8);
        assert!(file.body_mut().remove_attribute("nonexistent").is_none());

        // Blocks
        let mut block = CstBlock::new("server", vec!["web".to_string()]);
        block.body_mut().set_attribute_raw(
            "ip",
            vec![Token::new(TokenKind::String, r#""1.2.3.4""#, span)],
        );
        file.body_mut().append_block(block);
        file.body_mut().append_newline();

        assert_eq!(file.body().blocks().len(), 1);
        let rendered = file.write_to_string();
        assert!(rendered.contains(r#"server "web" {"#));
        assert!(rendered.contains(r#"ip = "1.2.3.4""#));
        assert!(rendered.contains(r#"name = "updated""#));
        assert_eq!(file.to_string(), rendered);

        // Remove block
        let block_removed = file.body_mut().remove_block("server", &["web"]);
        assert!(block_removed.is_some());
        assert_eq!(file.body().blocks().len(), 0);
        assert!(file.body_mut().remove_block("server", &["web"]).is_none());
    }

    #[test]
    fn test_cst_file_parse_and_roundtrip() {
        let input = "name = \"foo\"\n\nblock \"type\" \"name\" {\n  attr = true\n}\n";
        let file = CstFile::parse(input).unwrap();
        assert_eq!(file.body().attributes().len(), 1);
        assert_eq!(file.body().blocks().len(), 1);

        let output = file.write_to_string();
        assert!(output.contains(r#"name = "foo""#));
        assert!(output.contains(r#"block "type" "name" {"#));
    }

    #[test]
    fn test_cst_builder_coverage_extra() {
        let span = Span::new(0, 0, 0, 0, 0, 0);

        // Default for CstBody
        let default_body = CstBody::default();
        assert_eq!(default_body.indent_level, 0);

        // Unquoted block label in parse, with comments and newlines for render coverage
        let input = "  # comment\n  attr = 1\n  block unquoted_label {\n    inner = 2\n  }\n\n";
        let parsed = CstFile::parse(input).unwrap();
        assert_eq!(parsed.body().blocks()[0].labels(), &["unquoted_label"]);

        // Render with leading and trailing tokens
        let rendered = parsed.write_to_string();
        assert!(rendered.contains("unquoted_label"));
        assert!(rendered.contains("attr = 1"));

        // Multi-element set in value_to_tokens (using Dynamic vs String to achieve ordering inequality)
        let mut file = CstFile::new();
        let mut set = std::collections::BTreeSet::new();
        set.insert(Value::new(
            Type::String,
            ValueData::String("elem1".to_string()),
        ));
        set.insert(Value::new(
            Type::Dynamic,
            ValueData::String("elem2".to_string()),
        ));
        assert_eq!(set.len(), 2);
        file.body_mut()
            .set_attribute_value("myset", &Value::new(Type::Dynamic, ValueData::Set(set)));

        // Nested block with indented attribute and nested block append
        let mut parent_block = CstBlock::new("parent", vec!["p1".to_string()]);
        parent_block.body.indent_level = 1;
        parent_block
            .body_mut()
            .set_attribute_value("nested_key", &Value::new(Type::Bool, ValueData::Bool(true)));

        let nested_block = CstBlock::new("child", vec![]);
        parent_block.body_mut().append_block(nested_block);

        // Append block that already has leading tokens
        let mut block_with_leading = CstBlock::new("child2", vec![]);
        block_with_leading
            .leading
            .push(Token::new(TokenKind::Whitespace, " ", span.clone()));
        parent_block.body_mut().append_block(block_with_leading);

        file.body_mut().append_block(parent_block);

        // remove_block branch coverage:
        // 1. different block_type
        assert!(
            file.body_mut()
                .remove_block("different_type", &["p1"])
                .is_none()
        );
        // 2. different label count
        assert!(
            file.body_mut()
                .remove_block("parent", &["p1", "extra"])
                .is_none()
        );
        // 3. different label text
        assert!(
            file.body_mut()
                .remove_block("parent", &["different_lbl"])
                .is_none()
        );
        // 4. exact match
        assert!(file.body_mut().remove_block("parent", &["p1"]).is_some());

        // parse error for CstFile::parse map_err
        assert!(CstFile::parse("invalid {").is_err());

        // Explicit render with attr.leading, block.trailing, and body.trailing tokens
        let mut test_body = CstBody::new();
        let mut test_attr = CstAttribute::new(
            "foo",
            vec![Token::new(TokenKind::Number, "1", span.clone())],
        );
        test_attr
            .leading
            .push(Token::new(TokenKind::Whitespace, " ", span.clone()));
        test_attr
            .trailing
            .push(Token::new(TokenKind::Newline, "\n", span.clone()));
        test_body.attributes.push(test_attr);

        let mut test_block = CstBlock::new("blk", vec![]);
        test_block
            .leading
            .push(Token::new(TokenKind::Whitespace, "  ", span.clone()));
        test_block
            .trailing
            .push(Token::new(TokenKind::Newline, "\n", span.clone()));
        test_body.blocks.push(test_block);

        test_body
            .trailing
            .push(Token::new(TokenKind::Newline, "\n", span.clone()));

        let mut rendered_out = String::new();
        test_body.render(&mut rendered_out);
        assert!(rendered_out.contains("foo = 1"));
        assert!(rendered_out.contains("blk {\n}"));

        // from_doc_to_body label edge cases: starts_with quote but not ends_with, and len < 2
        let mut doc = crate::cst::document::Document::new();
        let edge_block = crate::cst::document::Block {
            leading: crate::cst::document::TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "edge", span.clone()),
            labels: vec![
                Token::new(TokenKind::String, "\"unclosed", span.clone()),
                Token::new(TokenKind::String, "\"", span.clone()),
            ],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(crate::cst::document::Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span),
            trailing: crate::cst::document::TokenStream::new(),
        };
        doc.blocks.push(edge_block);

        let cst_edge = from_document(&doc);
        assert_eq!(cst_edge.body().blocks()[0].labels().len(), 2);
    }

    #[test]
    fn test_tokens_for_expression_and_traversal() {
        let span = Span::new(0, 0, 1, 1, 1, 1);

        // 1. tokens_for_value
        let val_num = Value::new(Type::Number, ValueData::Number(Number::from(42)));
        let toks_num = tokens_for_value(&val_num);
        assert_eq!(toks_num[0].text, "42");

        let val_set = Value::new(
            Type::Set(Box::new(Type::String)),
            ValueData::Set(std::collections::BTreeSet::from([Value::new(
                Type::String,
                ValueData::String("elem".into()),
            )])),
        );
        let toks_set = tokens_for_value(&val_set);
        assert_eq!(toks_set[0].kind, TokenKind::OBrack);

        // 2. tokens_for_traversal
        let trav = Traversal {
            expr: Box::new(Expression::Variable("var".to_string(), span.clone())),
            operators: vec![
                TraversalOperator::GetAttr("servers".to_string(), span.clone()),
                TraversalOperator::Index(
                    Expression::Number(Number::from(0), span.clone()),
                    span.clone(),
                ),
                TraversalOperator::LegacyIndex(1, span.clone()),
                TraversalOperator::AttrSplat(span.clone()),
                TraversalOperator::FullSplat(span.clone()),
            ],
        };
        let trav_toks = tokens_for_traversal(&trav);
        let trav_str: String = trav_toks.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(trav_str, "var.servers[0].1.*[*]");

        // 3. tokens_for_expression
        // Null, Bool, String, Variable
        assert_eq!(
            tokens_for_expression(&Expression::Null(span.clone()))[0].text,
            "null"
        );
        assert_eq!(
            tokens_for_expression(&Expression::Bool(true, span.clone()))[0].text,
            "true"
        );
        assert_eq!(
            tokens_for_expression(&Expression::String("hi".into(), span.clone()))[0].text,
            "\"hi\""
        );
        assert_eq!(
            tokens_for_expression(&Expression::Variable("foo".into(), span.clone()))[0].text,
            "foo"
        );

        // Tuple
        let tup_expr = Expression::Tuple(
            vec![
                Expression::Number(Number::from(1), span.clone()),
                Expression::Number(Number::from(2), span.clone()),
            ],
            span.clone(),
        );
        let tup_toks = tokens_for_expression(&tup_expr);
        let tup_str: String = tup_toks.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(tup_str, "[1, 2]");

        // Object
        let obj_expr = Expression::Object(
            vec![(
                Expression::Variable("k".into(), span.clone()),
                Expression::String("v".into(), span.clone()),
            )],
            span.clone(),
        );
        let obj_toks = tokens_for_expression(&obj_expr);
        assert!(obj_toks.iter().any(|t| t.text == "k"));

        // BinaryOp & UnaryOp & Parentheses
        let bin_expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Number(Number::from(1), span.clone())),
            Box::new(Expression::Number(Number::from(2), span.clone())),
            span.clone(),
        );
        let bin_str: String = tokens_for_expression(&bin_expr)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(bin_str, "1 + 2");

        let un_expr = Expression::UnaryOp(
            UnaryOp::Not,
            Box::new(Expression::Bool(true, span.clone())),
            span.clone(),
        );
        let un_str: String = tokens_for_expression(&un_expr)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(un_str, "!true");

        let paren_expr = Expression::Parentheses(Box::new(bin_expr), span.clone());
        let paren_str: String = tokens_for_expression(&paren_expr)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(paren_str, "(1 + 2)");

        // Conditional
        let cond_expr = Expression::Conditional(
            Box::new(crate::ast::expr::Conditional {
                cond_expr: Expression::Bool(true, span.clone()),
                true_expr: Expression::Number(Number::from(1), span.clone()),
                false_expr: Expression::Number(Number::from(0), span.clone()),
            }),
            span.clone(),
        );
        let cond_str: String = tokens_for_expression(&cond_expr)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(cond_str, "true ? 1 : 0");

        // FuncCall
        let fn_expr = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "max".into(),
                args: vec![
                    Expression::Number(Number::from(1), span.clone()),
                    Expression::Number(Number::from(2), span.clone()),
                ],
                expand_final: true,
            }),
            span.clone(),
        );
        let fn_str: String = tokens_for_expression(&fn_expr)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(fn_str, "max(1, 2...)");

        // ForExpr tuple
        let for_tuple = Expression::ForExpr(
            Box::new(crate::ast::expr::ForExpr {
                key_var: Some("i".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(Expression::Variable("list".into(), span.clone())),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".into(), span.clone())),
                cond_expr: Some(Box::new(Expression::Bool(true, span.clone()))),
                grouping: false,
            }),
            span.clone(),
        );
        let for_tup_str: String = tokens_for_expression(&for_tuple)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(for_tup_str, "[for i, v in list : v if true]");

        // ForExpr object
        let for_obj = Expression::ForExpr(
            Box::new(crate::ast::expr::ForExpr {
                key_var: None,
                val_var: "item".to_string(),
                collection: Box::new(Expression::Variable("items".into(), span.clone())),
                key_expr: Some(Box::new(Expression::Variable("item".into(), span.clone()))),
                val_expr: Box::new(Expression::Variable("item".into(), span.clone())),
                cond_expr: None,
                grouping: true,
            }),
            span.clone(),
        );
        let for_obj_str: String = tokens_for_expression(&for_obj)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(for_obj_str, "{for item in items : item => item...}");

        // Template
        let tmpl = Expression::Template(
            vec![
                TemplatePart::Literal("hello ".to_string(), span.clone()),
                TemplatePart::Interpolation(
                    Expression::Variable("name".into(), span.clone()),
                    span.clone(),
                ),
            ],
            span.clone(),
        );
        let tmpl_str: String = tokens_for_expression(&tmpl)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(tmpl_str, "\"hello ${name}\"");
    }

    #[test]
    fn test_cst_builder_coverage_gaps() {
        let span = Span::new(0, 0, 0, 0, 0, 0);

        // 1. as_attribute and as_block
        let attr = CstAttribute::new("a", vec![]);
        let item_attr = CstItem::Attribute(attr);
        let block = CstBlock::new("blk", vec![]);
        let item_blk = CstItem::Block(block);
        let item_trivia = CstItem::Trivia(TokenStream::new());

        assert!(item_attr.as_attribute().is_some());
        assert!(item_attr.as_block().is_none());
        assert!(item_blk.as_block().is_some());
        assert!(item_blk.as_attribute().is_none());
        assert!(item_trivia.as_attribute().is_none());
        assert!(item_trivia.as_block().is_none());

        // 2. set_attribute_raw with preceding items, and remove_attribute with non-attribute items
        let mut body = CstBody::new();
        body.append_newline();
        body.append_block(CstBlock::new("server", vec![]));
        body.set_attribute_raw("other", vec![]);
        body.set_attribute_raw(
            "target",
            vec![Token::new(TokenKind::Ident, "1", span.clone())],
        );
        body.set_attribute_raw(
            "target",
            vec![Token::new(TokenKind::Ident, "2", span.clone())],
        );
        assert_eq!(body.attributes().len(), 2);
        assert!(body.remove_attribute("target").is_some());

        // 3. append_block with indent and leading tokens
        let mut body_indent = CstBody::with_indent_level(1);
        let blk_empty_leading = CstBlock::new("sub", vec![]);
        body_indent.append_block(blk_empty_leading);

        let mut blk_with_leading = CstBlock::new("sub2", vec![]);
        blk_with_leading
            .leading
            .push(Token::new(TokenKind::Whitespace, "  ", span.clone()));
        body_indent.append_block(blk_with_leading);

        // 4. from_document with doc.items empty but doc.attributes and doc.blocks populated
        let mut doc = Document::new();
        let attr_doc = crate::cst::document::Attribute {
            leading: TokenStream::new(),
            name: Token::new(TokenKind::Ident, "key", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![Token::new(TokenKind::Ident, "val", span.clone())],
            trailing: TokenStream::new(),
        };
        doc.attributes.push(attr_doc);
        let blk_doc = crate::cst::document::Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "resource", span.clone()),
            labels: vec![
                Token::new(TokenKind::String, "\"my_res\"", span.clone()),
                Token::new(TokenKind::Ident, "unquoted", span.clone()),
            ],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        };
        doc.blocks.push(blk_doc);
        let cst_file = from_document(&doc);
        assert_eq!(cst_file.body().attributes().len(), 1);
        assert_eq!(cst_file.body().blocks().len(), 1);
        assert_eq!(
            cst_file.body().blocks()[0].labels(),
            &["my_res", "unquoted"]
        );

        // 4b. from_document with DocumentItem::Trivia
        let mut doc2 = Document::new();
        let mut trivia_stream = TokenStream::new();
        trivia_stream.push(Token::new(TokenKind::Comment, "# comment\n", span.clone()));
        doc2.items.push(DocumentItem::Trivia(trivia_stream));
        let cst_file2 = from_document(&doc2);
        assert_eq!(cst_file2.body().items().len(), 1);

        // 5. tokens_for_expression for all BinaryOp variants
        let ops = [
            (BinaryOp::Sub, "-"),
            (BinaryOp::Mul, "*"),
            (BinaryOp::Div, "/"),
            (BinaryOp::Mod, "%"),
            (BinaryOp::Eq, "=="),
            (BinaryOp::NotEq, "!="),
            (BinaryOp::Less, "<"),
            (BinaryOp::LessEq, "<="),
            (BinaryOp::Greater, ">"),
            (BinaryOp::GreaterEq, ">="),
            (BinaryOp::And, "&&"),
            (BinaryOp::Or, "||"),
        ];
        for (op, expected_op_str) in ops {
            let expr = Expression::BinaryOp(
                op,
                Box::new(Expression::Number(Number::from(1), span.clone())),
                Box::new(Expression::Number(Number::from(2), span.clone())),
                span.clone(),
            );
            let s: String = tokens_for_expression(&expr)
                .iter()
                .map(|t| t.text.as_str())
                .collect();
            assert_eq!(s, format!("1 {expected_op_str} 2"));
        }

        // UnaryOp::Neg, Expression::Bool(false), FuncCall without expand_final
        let neg_expr = Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Number(Number::from(5), span.clone())),
            span.clone(),
        );
        let neg_str: String = tokens_for_expression(&neg_expr)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(neg_str, "-5");

        let bool_false = Expression::Bool(false, span.clone());
        let bool_false_str: String = tokens_for_expression(&bool_false)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(bool_false_str, "false");

        let fn_no_expand = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "min".into(),
                args: vec![Expression::Number(Number::from(1), span.clone())],
                expand_final: false,
            }),
            span.clone(),
        );
        let fn_str: String = tokens_for_expression(&fn_no_expand)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(fn_str, "min(1)");

        // Expression::Traversal
        let trav_expr = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("foo".to_string(), span.clone())),
                operators: vec![crate::ast::expr::TraversalOperator::GetAttr(
                    "bar".to_string(),
                    span.clone(),
                )],
            }),
            span.clone(),
        );
        let trav_str: String = tokens_for_expression(&trav_expr)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(trav_str, "foo.bar");

        // TemplatePart::Directive - Strip
        let tmpl_strip = Expression::Template(
            vec![
                TemplatePart::Directive(
                    crate::ast::expr::Directive::Strip {
                        strip_left: true,
                        strip_right: false,
                    },
                    span.clone(),
                ),
                TemplatePart::Directive(
                    crate::ast::expr::Directive::Strip {
                        strip_left: false,
                        strip_right: true,
                    },
                    span.clone(),
                ),
            ],
            span.clone(),
        );
        let tmpl_strip_str: String = tokens_for_expression(&tmpl_strip)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(tmpl_strip_str, "\"%{~}%{~}\"");

        // TemplatePart::Directive - If with else_ifs and false_expr
        let tmpl_if = Expression::Template(
            vec![TemplatePart::Directive(
                crate::ast::expr::Directive::If {
                    cond: Expression::Bool(true, span.clone()),
                    true_expr: vec![TemplatePart::Literal("hello".to_string(), span.clone())],
                    else_ifs: vec![(
                        Expression::Bool(false, span.clone()),
                        vec![TemplatePart::Literal("elif".to_string(), span.clone())],
                    )],
                    false_expr: Some(vec![TemplatePart::Literal(
                        "world".to_string(),
                        span.clone(),
                    )]),
                },
                span.clone(),
            )],
            span.clone(),
        );
        let tmpl_if_str: String = tokens_for_expression(&tmpl_if)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(
            tmpl_if_str,
            "\"%{if true}hello%{else if false}elif%{else}world%{endif}\""
        );

        // Directive::If without false_expr (covers false branch of if let Some(false_parts))
        let tmpl_if_no_else = Expression::Template(
            vec![TemplatePart::Directive(
                crate::ast::expr::Directive::If {
                    cond: Expression::Bool(true, span.clone()),
                    true_expr: vec![TemplatePart::Literal("hello".to_string(), span.clone())],
                    else_ifs: vec![],
                    false_expr: None,
                },
                span.clone(),
            )],
            span.clone(),
        );
        let tmpl_if_no_else_str: String = tokens_for_expression(&tmpl_if_no_else)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(tmpl_if_no_else_str, "\"%{if true}hello%{endif}\"");

        // TemplatePart::Directive - For with and without key_var
        let tmpl_for = Expression::Template(
            vec![
                TemplatePart::Directive(
                    crate::ast::expr::Directive::For {
                        key_var: Some("k".to_string()),
                        val_var: "v".to_string(),
                        collection: Expression::Variable("items".to_string(), span.clone()),
                        body: vec![TemplatePart::Interpolation(
                            Expression::Variable("v".to_string(), span.clone()),
                            span.clone(),
                        )],
                    },
                    span.clone(),
                ),
                TemplatePart::Directive(
                    crate::ast::expr::Directive::For {
                        key_var: None,
                        val_var: "item".to_string(),
                        collection: Expression::Variable("items".to_string(), span.clone()),
                        body: vec![TemplatePart::Literal("!".to_string(), span.clone())],
                    },
                    span.clone(),
                ),
            ],
            span.clone(),
        );
        let tmpl_for_str: String = tokens_for_expression(&tmpl_for)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(
            tmpl_for_str,
            "\"%{for k, v in items}${v}%{endfor}%{for item in items}!%{endfor}\""
        );

        // Test CstFile mutation primitives
        let mut file = CstFile::new();
        file.body_mut().set_attribute_raw(
            "attr1",
            vec![Token::new(TokenKind::Number, "10", span.clone())],
        );
        file.body_mut().set_attribute_raw(
            "attr2",
            vec![Token::new(TokenKind::Number, "20", span.clone())],
        );
        file.body_mut()
            .append_block(CstBlock::new("service", vec!["web".to_string()]));

        assert_eq!(file.body().attributes().len(), 2);
        assert_eq!(file.body().blocks().len(), 1);

        // replace_attribute_expr
        let replacement_expr = Expression::Number(Number::from(42_i64), span.clone());
        file.replace_attribute_expr("attr1", &replacement_expr)
            .unwrap();
        file.replace_attribute_expr("attr2", &replacement_expr)
            .unwrap();
        let rendered = file.write_to_string();
        assert!(rendered.contains("42"));

        // replace nonexistent returns error
        assert!(
            file.replace_attribute_expr("nonexistent", &replacement_expr)
                .is_err()
        );

        // remove_attribute
        let removed_attr = file.remove_attribute("attr2");
        assert!(removed_attr.is_some());
        assert_eq!(file.body().attributes().len(), 1);
        assert!(file.remove_attribute("attr2").is_none());

        // remove_block
        let removed_block = file.remove_block("service", &["web"]);
        assert!(removed_block.is_some());
        assert_eq!(file.body().blocks().len(), 0);
        assert!(file.remove_block("service", &["web"]).is_none());
    }

    #[test]
    fn test_cst_attribute_inplace_mutation_and_rename_variable() {
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let mut attr = CstAttribute::new(
            "timeout",
            vec![Token::new(TokenKind::Number, "30", span.clone())],
        );
        attr.leading
            .push(Token::new(TokenKind::Whitespace, "  ", span.clone()));
        attr.trailing.push(Token::new(
            TokenKind::Comment,
            " # Default timeout\n",
            span.clone(),
        ));

        // 1. replace_expr preserves leading indent and trailing comment
        let new_expr = Expression::Number(Number::from(60), span.clone());
        attr.replace_expr(&new_expr);
        let mut rendered = String::new();
        CstBody::render_cst_attribute(&attr, &mut rendered);
        assert_eq!(rendered, "  timeout = 60 # Default timeout\n");

        // 2. replace_name preserves leading and trailing
        attr.replace_name("connect_timeout");
        let mut rendered2 = String::new();
        CstBody::render_cst_attribute(&attr, &mut rendered2);
        assert_eq!(rendered2, "  connect_timeout = 60 # Default timeout\n");

        // 3. Primitive value setters
        attr.set_string("fast");
        let mut rendered_str = String::new();
        CstBody::render_cst_attribute(&attr, &mut rendered_str);
        assert_eq!(
            rendered_str,
            "  connect_timeout = \"fast\" # Default timeout\n"
        );

        attr.set_number(&Number::from(120));
        let mut rendered_num = String::new();
        CstBody::render_cst_attribute(&attr, &mut rendered_num);
        assert_eq!(rendered_num, "  connect_timeout = 120 # Default timeout\n");

        attr.set_bool(true);
        let mut rendered_bool = String::new();
        CstBody::render_cst_attribute(&attr, &mut rendered_bool);
        assert_eq!(
            rendered_bool,
            "  connect_timeout = true # Default timeout\n"
        );

        // 4. set_traversal
        let traversal = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("var".to_string(), span.clone())),
            operators: vec![crate::ast::expr::TraversalOperator::GetAttr(
                "custom_timeout".to_string(),
                span.clone(),
            )],
        };
        attr.set_traversal(&traversal);
        let mut rendered_trav = String::new();
        CstBody::render_cst_attribute(&attr, &mut rendered_trav);
        assert_eq!(
            rendered_trav,
            "  connect_timeout = var.custom_timeout # Default timeout\n"
        );

        // 5. CstFile rename_variable
        let mut file = CstFile::new();
        let mut a1 = CstAttribute::new(
            "count",
            vec![
                Token::new(TokenKind::Ident, "old_var", span.clone()),
                Token::new(TokenKind::Plus, "+", span.clone()),
                Token::new(TokenKind::Number, "1", span.clone()),
            ],
        );
        a1.trailing.push(Token::new(
            TokenKind::Comment,
            " # references old_var in comment\n",
            span.clone(),
        ));
        file.body_mut().items.push(CstItem::Attribute(a1.clone()));
        file.body_mut().attributes.push(a1);

        let mut inner_block = CstBlock::new("server", vec!["app".to_string()]);
        let a2 = CstAttribute::new(
            "replica",
            vec![Token::new(TokenKind::Ident, "old_var", span)],
        );
        inner_block.body.items.push(CstItem::Attribute(a2.clone()));
        inner_block.body.attributes.push(a2);
        file.body_mut().append_block(inner_block);

        let renamed_count = file.rename_variable("old_var", "new_var");
        assert_eq!(renamed_count, 2);

        let file_rendered = file.write_to_string();
        assert!(file_rendered.contains("count = new_var+1 # references old_var in comment"));
        assert!(file_rendered.contains("replica = new_var"));
    }

    #[test]
    fn test_programmatic_comments_on_cst_attribute_and_block() {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
        let mut attr = CstAttribute::new(
            "timeout",
            vec![Token::new(TokenKind::Number, "30", dummy_span.clone())],
        );
        attr.with_leading_comment("Leading timeout comment")
            .with_trailing_inline_comment("Inline timeout comment");

        let mut rendered_attr = String::new();
        CstBody::render_cst_attribute(&attr, &mut rendered_attr);
        assert!(rendered_attr.contains("# Leading timeout comment\n"));
        assert!(rendered_attr.contains("timeout = 30 # Inline timeout comment\n"));

        let mut block = CstBlock::new("server", vec!["web".to_string()]);
        block
            .with_leading_comment("// Leading block comment")
            .with_trailing_inline_comment("/* Trailing inline block comment */");

        let mut body = CstBody::new();
        body.append_block(block);
        let mut rendered_block = String::new();
        body.render(&mut rendered_block);
        assert!(rendered_block.contains("// Leading block comment\n"));
        assert!(rendered_block.contains("/* Trailing inline block comment */"));
    }

    #[test]
    fn test_clean_cst_node_deletion_and_trivia_compaction() {
        let input = r#"
resource "aws_vpc" "main" {
  cidr_block = "10.0.0.0/16"

  enable_dns_hostnames = true

  enable_dns_support = true
}

resource "aws_subnet" "sub" {
  cidr_block = "10.0.1.0/24"
}
"#;

        let mut file = CstFile::parse(input).unwrap();
        let vpc_block = file.block_mut("resource", &["aws_vpc", "main"]).unwrap();

        // 1. Remove first item in block
        let removed_first = vpc_block.body.remove_attribute("cidr_block");
        assert!(removed_first.is_some());
        assert_eq!(
            removed_first.as_ref().map(|a| a.name.as_str()),
            Some("cidr_block")
        );
        assert_eq!(vpc_block.body.attributes.len(), 2);

        // Render and check indentation of the new first item
        let rendered_after_first = file.write_to_string();
        assert!(!rendered_after_first.contains("10.0.0.0/16"));
        assert!(rendered_after_first.contains("  enable_dns_hostnames = true"));

        // 2. Remove middle item in block
        let vpc_block = file.block_mut("resource", &["aws_vpc", "main"]).unwrap();
        let removed_mid = vpc_block.body.remove_attribute("enable_dns_hostnames");
        assert!(removed_mid.is_some());

        // 3. Remove last and only remaining item in block
        let vpc_block = file.block_mut("resource", &["aws_vpc", "main"]).unwrap();
        let removed_last = vpc_block.body.remove_attribute("enable_dns_support");
        assert!(removed_last.is_some());
        assert_eq!(vpc_block.body.items, [] as [CstItem; 0]);

        // Verify removing only item in block preserves correct indentation of braces { }
        let rendered_empty_block = file.write_to_string();
        assert!(rendered_empty_block.contains("resource \"aws_vpc\" \"main\" {\n}\n"));

        // 4. Remove block entirely
        let removed_block = file.remove_block("resource", &["aws_vpc", "main"]);
        assert!(removed_block.is_some());
        let final_rendered = file.write_to_string();
        assert!(!final_rendered.contains("aws_vpc"));
        assert!(final_rendered.contains("resource \"aws_subnet\" \"sub\" {"));

        // 5. Remove only block in file -> verify clean whitespace idempotence
        let removed_sub = file.remove_block("resource", &["aws_subnet", "sub"]);
        assert!(removed_sub.is_some());
        assert_eq!(file.body().blocks.len(), 0);
        let empty_rendered = file.write_to_string();
        assert_eq!(empty_rendered.trim(), "");
    }

    #[test]
    fn test_cst_builder_coverage_completion() {
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);

        // 1. Comments with all prefixes and endings on attribute and block
        let mut attr = CstAttribute::new("test", vec![]);
        attr.trailing
            .push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));
        attr.trailing
            .push(Token::new(TokenKind::Newline, "\n", dummy_span.clone()));
        attr.with_leading_comment("# hash comment\n");
        attr.with_leading_comment("# hash comment no newline");
        attr.with_leading_comment("// slash comment\n");
        attr.with_leading_comment("// slash comment no newline");
        attr.with_leading_comment("/* block comment */\n");
        attr.with_leading_comment("/* block comment */ no newline");
        attr.with_trailing_inline_comment("# inline hash\n");
        attr.with_trailing_inline_comment("// inline slash\n");
        attr.with_trailing_inline_comment("/* inline block */");
        attr.with_trailing_inline_comment("plain text");

        let mut block = CstBlock::new("block", vec![]);
        block
            .trailing
            .push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));
        block
            .trailing
            .push(Token::new(TokenKind::Newline, "\n", dummy_span.clone()));
        block.with_leading_comment("# hash\n");
        block.with_leading_comment("# hash no newline");
        block.with_leading_comment("// slash\n");
        block.with_leading_comment("// slash no newline");
        block.with_leading_comment("/* block */\n");
        block.with_leading_comment("/* block */ no newline");
        block.with_leading_comment("plain leading comment");
        block.with_trailing_inline_comment("# inline hash\n");
        block.with_trailing_inline_comment("// inline slash\n");
        block.with_trailing_inline_comment("/* inline block */");
        block.with_trailing_inline_comment("plain text");

        // 2. compact_trivia comprehensive branches
        let mut body_empty = CstBody::new();
        body_empty.compact_trivia();
        assert_eq!(body_empty.items.len(), 0);

        let mut body_only_newlines = CstBody::new();
        body_only_newlines.append_newline();
        body_only_newlines.append_newline();
        body_only_newlines.compact_trivia();
        assert_eq!(body_only_newlines.items.len(), 0);

        let mut body_consec = CstBody::new();
        body_consec
            .items
            .push(CstItem::Attribute(CstAttribute::new("p", vec![])));
        let mut t1_with_ws = TokenStream::new();
        t1_with_ws.push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));
        t1_with_ws.push(Token::new(TokenKind::Newline, "\n", dummy_span.clone()));
        body_consec.items.push(CstItem::Trivia(t1_with_ws));
        body_consec.append_newline();
        body_consec
            .items
            .push(CstItem::Attribute(CstAttribute::new("q", vec![])));
        body_consec.compact_trivia();
        assert_eq!(body_consec.items.len(), 3);

        let mut body = CstBody::with_indent_level(1);
        let mut ws_stream = TokenStream::new();
        ws_stream.push(Token::new(TokenKind::Whitespace, "   ", dummy_span.clone()));
        body.items.push(CstItem::Trivia(ws_stream.clone())); // leading whitespace trivia
        body.append_newline(); // leading newline trivia

        let mut comment_stream = TokenStream::new();
        comment_stream.push(Token::new(
            TokenKind::Comment,
            "# keep\n",
            dummy_span.clone(),
        ));
        body.items.push(CstItem::Trivia(comment_stream.clone())); // leading non-blank trivia

        body.append_newline();
        body.items.push(CstItem::Trivia(ws_stream.clone())); // consecutive blank trivia with whitespace

        body.items.push(CstItem::Trivia(comment_stream.clone())); // non-blank trivia
        body.items
            .push(CstItem::Attribute(CstAttribute::new("a", vec![])));
        body.items.push(CstItem::Block(CstBlock::new("b", vec![])));
        body.append_newline(); // trailing blank newline trivia
        body.items.push(CstItem::Trivia(ws_stream)); // trailing blank whitespace trivia

        body.compact_trivia();

        // Indentation for first item in body
        let mut body_indented_blk = CstBody::with_indent_level(2);
        body_indented_blk
            .items
            .push(CstItem::Block(CstBlock::new("child", vec![])));
        body_indented_blk.compact_trivia();

        let mut body_already_indented = CstBody::with_indent_level(1);
        let mut attr_indented = CstAttribute::new("x", vec![]);
        attr_indented
            .leading
            .push(Token::new(TokenKind::Comment, "# c", dummy_span.clone()));
        attr_indented
            .leading
            .push(Token::new(TokenKind::Whitespace, " ", dummy_span.clone()));
        attr_indented
            .leading
            .push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));
        body_already_indented
            .items
            .push(CstItem::Attribute(attr_indented));
        body_already_indented.compact_trivia();

        let mut body_blk_already_indented = CstBody::with_indent_level(1);
        let mut blk_indented = CstBlock::new("x", vec![]);
        blk_indented
            .leading
            .push(Token::new(TokenKind::Comment, "# c", dummy_span.clone()));
        blk_indented
            .leading
            .push(Token::new(TokenKind::Whitespace, " ", dummy_span.clone()));
        blk_indented
            .leading
            .push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));
        body_blk_already_indented
            .items
            .push(CstItem::Block(blk_indented));
        body_blk_already_indented.compact_trivia();

        let mut body_trivia_first = CstBody::with_indent_level(1);
        body_trivia_first
            .items
            .push(CstItem::Trivia(comment_stream));
        body_trivia_first.compact_trivia();

        // 3. set_indent_level_recursive with level 0 and comments
        let mut body_levels = CstBody::new();
        let mut attr_c = CstAttribute::new("k", vec![]);
        attr_c
            .leading
            .push(Token::new(TokenKind::Comment, "# c\n", dummy_span.clone()));
        attr_c.leading.push(Token::new(
            TokenKind::InlineComment,
            "/* i */",
            dummy_span.clone(),
        ));
        let mut blk_c = CstBlock::new("s", vec![]);
        blk_c
            .leading
            .push(Token::new(TokenKind::Comment, "# c\n", dummy_span.clone()));
        blk_c.leading.push(Token::new(
            TokenKind::InlineComment,
            "/* bi */",
            dummy_span.clone(),
        ));

        body_levels.attributes.push(attr_c.clone());
        body_levels.blocks.push(blk_c.clone());
        body_levels.items.push(CstItem::Attribute(attr_c));
        body_levels.items.push(CstItem::Block(blk_c));
        body_levels.items.push(CstItem::Trivia(TokenStream::new()));

        body_levels.set_indent_level_recursive(0, "  ");
        body_levels.set_indent_level_recursive(2, "  ");

        // 4. block_mut and remove_* variations
        let mut body_search = CstBody::new();
        body_search.items.push(CstItem::Trivia(TokenStream::new()));
        body_search
            .items
            .push(CstItem::Block(CstBlock::new("dummy", vec![])));
        body_search
            .items
            .push(CstItem::Attribute(CstAttribute::new("other", vec![])));
        body_search
            .items
            .push(CstItem::Attribute(CstAttribute::new("val", vec![])));
        body_search
            .attributes
            .push(CstAttribute::new("val", vec![]));
        body_search.append_block(CstBlock::new("server", vec!["web".to_string()]));

        assert!(body_search.block_mut("database", &["web"]).is_none());
        assert!(body_search.block_mut("server", &["web", "extra"]).is_none());
        assert!(body_search.block_mut("server", &["api"]).is_none());
        assert!(body_search.block_mut("server", &["web"]).is_some());

        // replace_attribute_expr success with preceding items
        assert!(
            body_search
                .replace_attribute_expr(
                    "val",
                    &Expression::Number(Number::from(42), dummy_span.clone())
                )
                .is_ok()
        );

        // remove_attribute when only in attributes vector
        let mut body_attr_only = CstBody::new();
        body_attr_only
            .attributes
            .push(CstAttribute::new("orphan", vec![]));
        assert!(body_attr_only.remove_attribute("orphan").is_some());
        assert!(body_attr_only.remove_attribute("absent").is_none());

        // remove_block when only in blocks vector
        let mut body_blk_only = CstBody::new();
        body_blk_only
            .blocks
            .push(CstBlock::new("orphan_blk", vec![]));
        assert!(body_blk_only.remove_block("orphan_blk", &[]).is_some());
        assert!(body_blk_only.remove_block("absent_blk", &[]).is_none());

        // replace_attribute_expr missing field error
        assert!(
            body_search
                .replace_attribute_expr("missing", &Expression::Null(dummy_span.clone()))
                .is_err()
        );

        // rename_variable with non-matching identifier and trivia item
        let mut body_rename = CstBody::new();
        body_rename.items.push(CstItem::Trivia(TokenStream::new()));
        let attr_ren = CstAttribute::new(
            "a",
            vec![
                Token::new(TokenKind::Ident, "foo", dummy_span.clone()),
                Token::new(TokenKind::Plus, "+", dummy_span.clone()),
                Token::new(TokenKind::Ident, "bar", dummy_span.clone()),
            ],
        );
        body_rename.attributes.push(attr_ren.clone());
        body_rename.items.push(CstItem::Attribute(attr_ren));
        let mut blk_ren = CstBlock::new("res", vec![]);
        let inner_attr = CstAttribute::new(
            "b",
            vec![Token::new(TokenKind::Ident, "foo", dummy_span.clone())],
        );
        blk_ren.body.attributes.push(inner_attr.clone());
        blk_ren.body.items.push(CstItem::Attribute(inner_attr));
        body_rename.append_block(blk_ren);
        assert_eq!(body_rename.rename_variable("foo", "baz"), 2);

        // 5. tokens_for_expression and format_directive comprehensive branches
        let all_bin_ops = [
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
        for op in all_bin_ops {
            let expr = Expression::BinaryOp(
                op,
                Box::new(Expression::Number(Number::from(1), dummy_span.clone())),
                Box::new(Expression::Number(Number::from(2), dummy_span.clone())),
                dummy_span.clone(),
            );
            assert_ne!(tokens_for_expression(&expr).len(), 0);
        }

        let neg_expr = Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Number(Number::from(5), dummy_span.clone())),
            dummy_span.clone(),
        );
        assert_eq!(tokens_for_expression(&neg_expr)[0].text, "-");

        let fn_call_expand = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "test".into(),
                args: vec![],
                expand_final: true,
            }),
            dummy_span.clone(),
        );
        let fn_toks = tokens_for_expression(&fn_call_expand);
        assert!(fn_toks.iter().any(|t| t.text == "..."));

        // ForExpr variations
        let for_tuple = Expression::ForExpr(
            Box::new(crate::ast::expr::ForExpr {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(Expression::Variable("list".to_string(), dummy_span.clone())),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), dummy_span.clone())),
                grouping: true,
                cond_expr: Some(Box::new(Expression::Bool(true, dummy_span.clone()))),
            }),
            dummy_span.clone(),
        );
        let for_toks = tokens_for_expression(&for_tuple);
        assert!(for_toks.iter().any(|t| t.text == "..."));
        assert!(for_toks.iter().any(|t| t.text == " if "));

        let for_obj = Expression::ForExpr(
            Box::new(crate::ast::expr::ForExpr {
                key_var: None,
                val_var: "item".to_string(),
                collection: Box::new(Expression::Variable(
                    "items".to_string(),
                    dummy_span.clone(),
                )),
                key_expr: Some(Box::new(Expression::Variable(
                    "item".to_string(),
                    dummy_span.clone(),
                ))),
                val_expr: Box::new(Expression::Variable("item".to_string(), dummy_span.clone())),
                grouping: false,
                cond_expr: None,
            }),
            dummy_span.clone(),
        );
        let for_obj_toks = tokens_for_expression(&for_obj);
        assert!(for_obj_toks.iter().any(|t| t.text == " => "));

        // Template directives
        let dir_if = crate::ast::expr::Directive::If {
            cond: Expression::Bool(true, dummy_span.clone()),
            true_expr: vec![TemplatePart::Literal("T".into(), dummy_span.clone())],
            else_ifs: vec![(
                Expression::Bool(false, dummy_span.clone()),
                vec![TemplatePart::Literal("EI".into(), dummy_span.clone())],
            )],
            false_expr: Some(vec![TemplatePart::Literal("F".into(), dummy_span.clone())]),
        };
        let mut dir_if_str = String::new();
        format_directive(&dir_if, &mut dir_if_str);
        assert!(dir_if_str.contains("%{else if"));
        assert!(dir_if_str.contains("%{else}"));

        let dir_if_no_else = crate::ast::expr::Directive::If {
            cond: Expression::Bool(true, dummy_span.clone()),
            true_expr: vec![],
            else_ifs: vec![],
            false_expr: None,
        };
        let mut dir_if_no_else_str = String::new();
        format_directive(&dir_if_no_else, &mut dir_if_no_else_str);
        assert!(!dir_if_no_else_str.contains("%{else}"));

        let dir_for = crate::ast::expr::Directive::For {
            key_var: Some("k".to_string()),
            val_var: "v".to_string(),
            collection: Expression::Variable("arr".to_string(), dummy_span.clone()),
            body: vec![TemplatePart::Interpolation(
                Expression::Variable("v".to_string(), dummy_span.clone()),
                dummy_span.clone(),
            )],
        };
        let mut dir_for_str = String::new();
        format_directive(&dir_for, &mut dir_for_str);
        assert!(dir_for_str.contains("%{for k, v in arr}"));

        let dir_for_no_key = crate::ast::expr::Directive::For {
            key_var: None,
            val_var: "elem".to_string(),
            collection: Expression::Variable("arr".to_string(), dummy_span.clone()),
            body: vec![],
        };
        let mut dir_for_no_key_str = String::new();
        format_directive(&dir_for_no_key, &mut dir_for_no_key_str);
        assert!(dir_for_no_key_str.contains("%{for elem in arr}"));

        let mut strip_all = String::new();
        format_directive(
            &crate::ast::expr::Directive::Strip {
                strip_left: true,
                strip_right: true,
            },
            &mut strip_all,
        );
        assert_eq!(strip_all, "%{~~}");

        let mut strip_none = String::new();
        format_directive(
            &crate::ast::expr::Directive::Strip {
                strip_left: false,
                strip_right: false,
            },
            &mut strip_none,
        );
        assert_eq!(strip_none, "%{}");

        let tmpl_expr = Expression::Template(
            vec![
                TemplatePart::Literal("hello ".into(), dummy_span.clone()),
                TemplatePart::Directive(dir_for, dummy_span.clone()),
            ],
            dummy_span.clone(),
        );
        let tmpl_toks = tokens_for_expression(&tmpl_expr);
        assert_eq!(tmpl_toks[0].kind, TokenKind::String);

        // 6. value_to_tokens remaining variants
        assert_eq!(
            value_to_tokens(&Value::new(Type::Bool, ValueData::Bool(false)), &dummy_span)[0].text,
            "false"
        );
        assert_eq!(
            value_to_tokens(&Value::unknown(Type::Dynamic), &dummy_span)[0].text,
            "unknown"
        );
        assert_eq!(
            value_to_tokens(&Value::capsule("test_capsule", 42), &dummy_span)[0].text,
            "capsule"
        );
        assert_eq!(
            value_to_tokens(
                &Value::new(Type::List(Box::new(Type::Number)), ValueData::Array(vec![])),
                &dummy_span
            )
            .len(),
            2
        );
        assert_eq!(
            value_to_tokens(
                &Value::new(
                    Type::Set(Box::new(Type::Number)),
                    ValueData::Set(std::collections::BTreeSet::new())
                ),
                &dummy_span
            )
            .len(),
            2
        );
        assert_eq!(
            value_to_tokens(
                &Value::new(
                    Type::object(std::collections::BTreeMap::new()),
                    ValueData::Object(std::collections::BTreeMap::new())
                ),
                &dummy_span
            )
            .len(),
            3
        );

        // 7. from_doc_to_body with doc.items.is_empty() and unquoted labels
        let mut doc = Document::new();
        let attr_doc = crate::cst::document::Attribute {
            name: Token::new(TokenKind::Ident, "timeout", dummy_span.clone()),
            equals_leading: crate::cst::document::TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", dummy_span.clone()),
            expr_tokens: vec![Token::new(TokenKind::Number, "10", dummy_span.clone())],
            leading: crate::cst::document::TokenStream::new(),
            trailing: crate::cst::document::TokenStream::new(),
        };
        let blk_doc = crate::cst::document::Block {
            type_ident: Token::new(TokenKind::Ident, "service", dummy_span.clone()),
            labels: vec![
                Token::new(TokenKind::String, "\"quoted\"", dummy_span.clone()),
                Token::new(TokenKind::Ident, "unquoted", dummy_span.clone()),
            ],
            open_brace: Token::new(TokenKind::OBrace, "{", dummy_span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", dummy_span.clone()),
            leading: crate::cst::document::TokenStream::new(),
            trailing: crate::cst::document::TokenStream::new(),
        };
        doc.attributes.push(attr_doc);
        doc.blocks.push(blk_doc);
        let parsed_doc_body = from_document(&doc);
        assert_eq!(parsed_doc_body.body().attributes.len(), 1);
        assert_eq!(parsed_doc_body.body().blocks.len(), 1);
        assert_eq!(
            parsed_doc_body.body().blocks[0].labels,
            vec!["quoted", "unquoted"]
        );

        // set_attribute_raw with indent_level > 0 and 0
        let mut body_raw = CstBody::with_indent_level(1);
        body_raw.set_attribute_raw(
            "new_key",
            vec![Token::new(TokenKind::Number, "1", dummy_span.clone())],
        );
        assert_eq!(body_raw.attributes.len(), 1);
        body_raw.set_attribute_raw(
            "new_key",
            vec![Token::new(TokenKind::Number, "2", dummy_span)],
        );
        assert_eq!(body_raw.attributes[0].expr_tokens[0].text, "2");
    }
}
