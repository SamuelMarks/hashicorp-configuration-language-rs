//! CST Node definitions.

use crate::ast::expr::Expression;
use crate::lex::token::{Token, TokenKind};
use crate::span::Span;

/// A stream of formatting tokens (whitespace, newlines, comments) that precede or succeed a node.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenStream {
    /// The tokens in the stream.
    pub tokens: Vec<Token>,
}

impl TokenStream {
    /// Create a new, empty token stream.
    #[must_use]
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    /// Push a token to the stream.
    pub fn push(&mut self, token: Token) {
        self.tokens.push(token);
    }
}

/// An item in a CST document preserving exact source ordering.
#[derive(Debug, Clone, PartialEq)]
pub enum DocumentItem {
    /// An attribute assignment.
    Attribute(Attribute),
    /// A block definition.
    Block(Block),
    /// Standalone formatting trivia (comments, whitespace, newlines).
    Trivia(TokenStream),
}

impl DocumentItem {
    /// Returns the item as an attribute if it is an attribute.
    #[must_use]
    pub fn as_attribute(&self) -> Option<&Attribute> {
        match self {
            Self::Attribute(a) => Some(a),
            _ => None,
        }
    }

    /// Returns the item as a block if it is a block.
    #[must_use]
    pub fn as_block(&self) -> Option<&Block> {
        match self {
            Self::Block(b) => Some(b),
            _ => None,
        }
    }
}

/// A complete HCL document preserving its exact textual representation.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Leading formatting before the first item.
    pub leading: TokenStream,
    /// Interleaved items preserving exact source ordering.
    pub items: Vec<DocumentItem>,
    /// The blocks in this document.
    pub blocks: Vec<Block>,
    /// The attributes in this document.
    pub attributes: Vec<Attribute>,
    /// Trailing formatting after the last item.
    pub trailing: TokenStream,
}

impl Document {
    /// Create a new, empty Document.
    #[must_use]
    pub fn new() -> Self {
        Self {
            leading: TokenStream::new(),
            items: Vec::new(),
            blocks: Vec::new(),
            attributes: Vec::new(),
            trailing: TokenStream::new(),
        }
    }

    /// Returns an iterator over attributes in this document.
    pub fn attributes_iter(&self) -> impl Iterator<Item = &Attribute> {
        self.items.iter().filter_map(|item| match item {
            DocumentItem::Attribute(attr) => Some(attr),
            _ => None,
        })
    }

    /// Returns an iterator over blocks in this document.
    pub fn blocks_iter(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|item| match item {
            DocumentItem::Block(block) => Some(block),
            _ => None,
        })
    }

    /// Sets an attribute on the document, synthesizing its expression tokens from an AST [`Expression`].
    ///
    /// If an attribute with the specified name already exists, its expression tokens are replaced.
    /// Otherwise, a new attribute is appended to the document.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    /// * `expr` - The AST expression to set.
    pub fn set_attribute(&mut self, name: &str, expr: &Expression) {
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let mut tokens = vec![Token::new(TokenKind::Whitespace, " ", span.clone())];
        tokens.extend(crate::cst::builder::tokens_for_expression(expr));

        if let Some(attr) = self.attributes.iter_mut().find(|a| a.name.text == name) {
            attr.expr_tokens.clone_from(&tokens);
            for item in &mut self.items {
                if let DocumentItem::Attribute(a) = item
                    && a.name.text == name
                {
                    a.expr_tokens.clone_from(&tokens);
                    break;
                }
            }
            return;
        }

        let mut leading = TokenStream::new();
        if !self.items.is_empty() || !self.attributes.is_empty() {
            leading.push(Token::new(TokenKind::Newline, "\n", span.clone()));
        }

        let mut equals_leading = TokenStream::new();
        equals_leading.push(Token::new(TokenKind::Whitespace, " ", span.clone()));

        let attr = Attribute {
            leading,
            name: Token::new(TokenKind::Ident, name, span.clone()),
            equals_leading,
            equals: Token::new(TokenKind::Assign, "=", span),
            expr_tokens: tokens,
            trailing: TokenStream::new(),
        };

        self.attributes.push(attr.clone());
        self.items.push(DocumentItem::Attribute(attr));
    }

    /// Removes an attribute by name from the document, returning it if it existed.
    ///
    /// # Arguments
    /// * `name` - The name of the attribute to remove.
    pub fn remove_attribute(&mut self, name: &str) -> Option<Attribute> {
        let idx = self.attributes.iter().position(|a| a.name.text == name)?;
        let removed = self.attributes.remove(idx);
        if let Some(item_idx) = self.items.iter().position(|item| match item {
            DocumentItem::Attribute(a) => a.name.text == name,
            _ => false,
        }) {
            self.items.remove(item_idx);
        }
        Some(removed)
    }

    /// Removes a block matching the specified type and labels, returning it if found.
    ///
    /// # Arguments
    /// * `block_type` - The block type identifier.
    /// * `labels` - The block labels to match against.
    pub fn remove_block(&mut self, block_type: &str, labels: &[&str]) -> Option<Block> {
        let idx = self.blocks.iter().position(|b| {
            b.type_ident.text == block_type
                && (labels.is_empty()
                    || b.labels.iter().map(|l| l.text.as_str()).collect::<Vec<_>>() == labels)
        })?;
        let removed = self.blocks.remove(idx);
        if let Some(item_idx) = self.items.iter().position(|item| match item {
            DocumentItem::Block(b) => {
                b.type_ident.text == block_type
                    && (labels.is_empty()
                        || b.labels.iter().map(|l| l.text.as_str()).collect::<Vec<_>>() == labels)
            }
            _ => false,
        }) {
            self.items.remove(item_idx);
        }
        Some(removed)
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
        if let Some(attr) = self.attributes.iter_mut().find(|a| a.name.text == name) {
            let span = Span::new(0, 0, 1, 1, 1, 1);
            let mut tokens = vec![Token::new(TokenKind::Whitespace, " ", span)];
            tokens.extend(crate::cst::builder::tokens_for_expression(new_expr));
            attr.expr_tokens.clone_from(&tokens);
            for item in &mut self.items {
                if let DocumentItem::Attribute(a) = item
                    && a.name.text == name
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

    /// Appends a [`Block`] to the document.
    ///
    /// # Arguments
    /// * `block` - The block to append.
    pub fn append_block(&mut self, block: Block) {
        self.blocks.push(block.clone());
        self.items.push(DocumentItem::Block(block));
    }

    /// Finds all blocks with the specified block type identifier.
    ///
    /// # Arguments
    /// * `block_type` - The block type identifier string.
    #[must_use]
    pub fn find_blocks(&self, block_type: &str) -> Vec<&Block> {
        self.blocks
            .iter()
            .filter(|b| b.type_ident.text == block_type)
            .collect()
    }

    /// Inserts an attribute before a target attribute in the document.
    ///
    /// # Arguments
    /// * `target` - The name of the existing attribute to insert before.
    /// * `attr` - The attribute to insert.
    ///
    /// # Errors
    /// Returns [`crate::error::HclError::CstNodeNotFound`] if the target attribute is not found.
    pub fn insert_attribute_before(
        &mut self,
        target: &str,
        attr: Attribute,
    ) -> Result<(), crate::error::HclError> {
        let item_idx = self
            .items
            .iter()
            .position(|item| match item {
                DocumentItem::Attribute(a) => a.name.text == target,
                _ => false,
            })
            .ok_or_else(|| crate::error::HclError::CstNodeNotFound(target.to_string()))?;

        let attr_idx = self
            .attributes
            .iter()
            .position(|a| a.name.text == target)
            .unwrap_or(self.attributes.len());

        self.attributes.insert(attr_idx, attr.clone());
        self.items.insert(item_idx, DocumentItem::Attribute(attr));
        Ok(())
    }

    /// Inserts an attribute after a target attribute in the document.
    ///
    /// # Arguments
    /// * `target` - The name of the existing attribute to insert after.
    /// * `attr` - The attribute to insert.
    ///
    /// # Errors
    /// Returns [`crate::error::HclError::CstNodeNotFound`] if the target attribute is not found.
    pub fn insert_attribute_after(
        &mut self,
        target: &str,
        attr: Attribute,
    ) -> Result<(), crate::error::HclError> {
        let item_idx = self
            .items
            .iter()
            .position(|item| match item {
                DocumentItem::Attribute(a) => a.name.text == target,
                _ => false,
            })
            .ok_or_else(|| crate::error::HclError::CstNodeNotFound(target.to_string()))?;

        let attr_idx = self
            .attributes
            .iter()
            .position(|a| a.name.text == target)
            .map_or(self.attributes.len(), |i| i + 1);

        self.attributes.insert(attr_idx, attr.clone());
        self.items
            .insert(item_idx + 1, DocumentItem::Attribute(attr));
        Ok(())
    }

    /// Inserts a block before a target block matching the specified type and labels.
    ///
    /// # Arguments
    /// * `target_type` - The block type identifier of the target block.
    /// * `target_labels` - The labels of the target block.
    /// * `block` - The block to insert.
    ///
    /// # Errors
    /// Returns [`crate::error::HclError::CstNodeNotFound`] if the target block is not found.
    pub fn insert_block_before(
        &mut self,
        target_type: &str,
        target_labels: &[&str],
        block: Block,
    ) -> Result<(), crate::error::HclError> {
        let item_idx = self
            .items
            .iter()
            .position(|item| match item {
                DocumentItem::Block(b) => {
                    b.type_ident.text == target_type
                        && (target_labels.is_empty()
                            || b.labels.iter().map(|l| l.text.as_str()).collect::<Vec<_>>()
                                == target_labels)
                }
                _ => false,
            })
            .ok_or_else(|| crate::error::HclError::CstNodeNotFound(target_type.to_string()))?;

        let block_idx = self
            .blocks
            .iter()
            .position(|b| {
                b.type_ident.text == target_type
                    && (target_labels.is_empty()
                        || b.labels.iter().map(|l| l.text.as_str()).collect::<Vec<_>>()
                            == target_labels)
            })
            .unwrap_or(self.blocks.len());

        self.blocks.insert(block_idx, block.clone());
        self.items.insert(item_idx, DocumentItem::Block(block));
        Ok(())
    }

    /// Inserts a block after a target block matching the specified type and labels.
    ///
    /// # Arguments
    /// * `target_type` - The block type identifier of the target block.
    /// * `target_labels` - The labels of the target block.
    /// * `block` - The block to insert.
    ///
    /// # Errors
    /// Returns [`crate::error::HclError::CstNodeNotFound`] if the target block is not found.
    pub fn insert_block_after(
        &mut self,
        target_type: &str,
        target_labels: &[&str],
        block: Block,
    ) -> Result<(), crate::error::HclError> {
        let item_idx = self
            .items
            .iter()
            .position(|item| match item {
                DocumentItem::Block(b) => {
                    b.type_ident.text == target_type
                        && (target_labels.is_empty()
                            || b.labels.iter().map(|l| l.text.as_str()).collect::<Vec<_>>()
                                == target_labels)
                }
                _ => false,
            })
            .ok_or_else(|| crate::error::HclError::CstNodeNotFound(target_type.to_string()))?;

        let block_idx = self
            .blocks
            .iter()
            .position(|b| {
                b.type_ident.text == target_type
                    && (target_labels.is_empty()
                        || b.labels.iter().map(|l| l.text.as_str()).collect::<Vec<_>>()
                            == target_labels)
            })
            .map_or(self.blocks.len(), |i| i + 1);

        self.blocks.insert(block_idx, block.clone());
        self.items.insert(item_idx + 1, DocumentItem::Block(block));
        Ok(())
    }

    /// Walks all tokens in the document with the provided visitor function.
    ///
    /// # Arguments
    /// * `visitor` - A mutable visitor closure invoked for each token in traversal order.
    pub fn walk_tokens<F: FnMut(&Token)>(&self, mut visitor: F) {
        self.walk_tokens_inner(&mut visitor);
    }

    fn walk_tokens_inner<F: FnMut(&Token)>(&self, visitor: &mut F) {
        for tok in &self.leading.tokens {
            visitor(tok);
        }
        for item in &self.items {
            match item {
                DocumentItem::Attribute(attr) => {
                    for tok in &attr.leading.tokens {
                        visitor(tok);
                    }
                    visitor(&attr.name);
                    for tok in &attr.equals_leading.tokens {
                        visitor(tok);
                    }
                    visitor(&attr.equals);
                    for tok in &attr.expr_tokens {
                        visitor(tok);
                    }
                    for tok in &attr.trailing.tokens {
                        visitor(tok);
                    }
                }
                DocumentItem::Block(block) => {
                    for tok in &block.leading.tokens {
                        visitor(tok);
                    }
                    visitor(&block.type_ident);
                    for tok in &block.labels {
                        visitor(tok);
                    }
                    visitor(&block.open_brace);
                    block.body.walk_tokens_inner(visitor);
                    visitor(&block.close_brace);
                    for tok in &block.trailing.tokens {
                        visitor(tok);
                    }
                }
                DocumentItem::Trivia(stream) => {
                    for tok in &stream.tokens {
                        visitor(tok);
                    }
                }
            }
        }
        for tok in &self.trailing.tokens {
            visitor(tok);
        }
    }

    /// Walks all tokens in the document mutably with the provided visitor function.
    ///
    /// # Arguments
    /// * `visitor` - A mutable visitor closure invoked for each token in traversal order.
    pub fn walk_tokens_mut<F: FnMut(&mut Token)>(&mut self, mut visitor: F) {
        self.walk_tokens_mut_inner(&mut visitor);
    }

    fn walk_tokens_mut_inner<F: FnMut(&mut Token)>(&mut self, visitor: &mut F) {
        for tok in &mut self.leading.tokens {
            visitor(tok);
        }
        for item in &mut self.items {
            match item {
                DocumentItem::Attribute(attr) => {
                    for tok in &mut attr.leading.tokens {
                        visitor(tok);
                    }
                    visitor(&mut attr.name);
                    for tok in &mut attr.equals_leading.tokens {
                        visitor(tok);
                    }
                    visitor(&mut attr.equals);
                    for tok in &mut attr.expr_tokens {
                        visitor(tok);
                    }
                    for tok in &mut attr.trailing.tokens {
                        visitor(tok);
                    }
                }
                DocumentItem::Block(block) => {
                    for tok in &mut block.leading.tokens {
                        visitor(tok);
                    }
                    visitor(&mut block.type_ident);
                    for tok in &mut block.labels {
                        visitor(tok);
                    }
                    visitor(&mut block.open_brace);
                    block.body.walk_tokens_mut_inner(visitor);
                    visitor(&mut block.close_brace);
                    for tok in &mut block.trailing.tokens {
                        visitor(tok);
                    }
                }
                DocumentItem::Trivia(stream) => {
                    for tok in &mut stream.tokens {
                        visitor(tok);
                    }
                }
            }
        }
        for tok in &mut self.trailing.tokens {
            visitor(tok);
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

/// A CST node representing a block.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Formatting before the block.
    pub leading: TokenStream,
    /// The block type identifier token.
    pub type_ident: Token,
    /// The block labels.
    pub labels: Vec<Token>,
    /// The open brace token.
    pub open_brace: Token,
    /// The inner document of the block.
    pub body: Box<Document>,
    /// The close brace token.
    pub close_brace: Token,
    /// Formatting after the block.
    pub trailing: TokenStream,
}

/// A CST node representing an attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// Formatting before the attribute name.
    pub leading: TokenStream,
    /// The attribute name token.
    pub name: Token,
    /// Formatting before the equals sign.
    pub equals_leading: TokenStream,
    /// The equals sign token.
    pub equals: Token,
    /// The unparsed raw tokens of the expression.
    pub expr_tokens: Vec<Token>,
    /// Formatting after the expression.
    pub trailing: TokenStream,
}

impl Attribute {
    /// Replaces the attribute's value with new unparsed raw tokens.
    pub fn set_expr_tokens(&mut self, tokens: Vec<Token>) {
        self.expr_tokens = tokens;
    }

    /// Attaches a leading comment to this attribute.
    ///
    /// # Arguments
    /// * `comment` - The comment text to attach.
    pub fn with_leading_comment(&mut self, comment: &str) -> &mut Self {
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let comment_text = if comment.starts_with('#') || comment.starts_with("//") {
            format!("{comment}\n")
        } else {
            format!("# {comment}\n")
        };
        self.leading
            .tokens
            .insert(0, Token::new(TokenKind::Comment, comment_text, span));
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
    use crate::lex::token::TokenKind;
    use crate::span::Span;

    #[test]
    fn test_token_stream() {
        let mut stream = TokenStream::new();
        assert_eq!(stream.tokens.len(), 0);

        let span = Span::new(0, 1, 1, 1, 1, 2);
        stream.push(Token::new(TokenKind::Whitespace, " ", span));
        assert_eq!(stream.tokens.len(), 1);

        let default_stream = TokenStream::default();
        assert_eq!(default_stream.tokens.len(), 0);
    }

    #[test]
    fn test_attribute_mutations() {
        let span = Span::new(0, 1, 1, 1, 1, 2);
        let mut attr = Attribute {
            leading: TokenStream::new(),
            name: Token::new(TokenKind::Ident, "foo", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![],
            trailing: TokenStream::new(),
        };

        let new_expr = vec![Token::new(TokenKind::Number, "42", span)];
        attr.set_expr_tokens(new_expr.clone());
        assert_eq!(attr.expr_tokens, new_expr);
    }

    #[test]
    fn test_document() {
        let doc = Document::new();
        assert_eq!(doc.blocks.len(), 0);
        assert_eq!(doc.attributes.len(), 0);

        let default_doc = Document::default();
        assert_eq!(default_doc.blocks.len(), 0);
    }

    #[test]
    fn test_attribute_block_creation() {
        let span = Span::new(0, 1, 1, 1, 1, 2);
        let name_tok = Token::new(TokenKind::Ident, "foo", span.clone());
        let equals_tok = Token::new(TokenKind::Assign, "=", span.clone());

        let attr = Attribute {
            leading: TokenStream::new(),
            name: name_tok.clone(),
            equals_leading: TokenStream::new(),
            equals: equals_tok,
            expr_tokens: vec![],
            trailing: TokenStream::new(),
        };

        assert_eq!(attr.name.kind, TokenKind::Ident);

        let block = Block {
            leading: TokenStream::new(),
            type_ident: name_tok,
            labels: vec![],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span),
            trailing: TokenStream::new(),
        };

        assert_eq!(block.type_ident.kind, TokenKind::Ident);
    }

    #[test]
    fn test_document_item_accessors_and_iterators() {
        let span = Span::new(0, 1, 1, 1, 1, 2);
        let attr = Attribute {
            leading: TokenStream::new(),
            name: Token::new(TokenKind::Ident, "key", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![],
            trailing: TokenStream::new(),
        };
        let block = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "b", span.clone()),
            labels: vec![],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span),
            trailing: TokenStream::new(),
        };
        let item_attr = DocumentItem::Attribute(attr.clone());
        let item_block = DocumentItem::Block(block.clone());
        let item_trivia = DocumentItem::Trivia(TokenStream::new());

        assert_eq!(item_attr.as_attribute(), Some(&attr));
        assert_eq!(item_attr.as_block(), None);

        assert_eq!(item_block.as_attribute(), None);
        assert_eq!(item_block.as_block(), Some(&block));

        assert_eq!(item_trivia.as_attribute(), None);
        assert_eq!(item_trivia.as_block(), None);

        let mut doc = Document::new();
        doc.items.push(item_attr);
        doc.items.push(item_block);
        doc.items.push(item_trivia);

        let attrs: Vec<_> = doc.attributes_iter().collect();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].name.text, "key");

        let blocks: Vec<_> = doc.blocks_iter().collect();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].type_ident.text, "b");
    }

    #[test]
    fn test_document_mutation_api() {
        let span = Span::new(0, 1, 1, 1, 1, 2);
        let mut doc = Document::new();

        // 1. set_attribute (new on empty document)
        let expr1 = Expression::Number(crate::number::Number::from(10), span.clone());
        doc.set_attribute("count", &expr1);
        assert_eq!(doc.attributes.len(), 1);
        assert_eq!(doc.items.len(), 1);

        // Add a block to items so subsequent replacement loops iterate past a block
        let blk = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "resource", span.clone()),
            labels: vec![Token::new(TokenKind::Ident, "aws_instance", span.clone())],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        };
        doc.append_block(blk);
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.items.len(), 2);
        assert_eq!(doc.find_blocks("resource").len(), 1);
        assert_eq!(doc.find_blocks("data").len(), 0);

        // Add another attribute with different name
        doc.set_attribute("other", &expr1);
        assert_eq!(doc.attributes.len(), 2);
        assert_eq!(doc.items.len(), 3);

        // 2. set_attribute (replace existing other attribute, looping past count attribute and block)
        let expr2 = Expression::Number(crate::number::Number::from(20), span.clone());
        doc.set_attribute("other", &expr2);
        assert_eq!(doc.attributes.len(), 2);
        assert_eq!(doc.items.len(), 3);
        assert!(doc.attributes[1].expr_tokens.iter().any(|t| t.text == "20"));

        // 3. set_attribute when items is empty but attributes has elements
        let mut doc_attr_only = Document::new();
        doc_attr_only.attributes.push(doc.attributes[0].clone());
        doc_attr_only.set_attribute("extra", &expr1);
        assert_eq!(doc_attr_only.attributes.len(), 2);

        // 4. with_leading_comment on Attribute
        let mut attr = doc.attributes[0].clone();
        attr.with_leading_comment("Server count configuration");
        assert!(
            attr.leading
                .tokens
                .iter()
                .any(|t| t.text.contains("Server count configuration"))
        );

        // with // comment
        attr.with_leading_comment("// Another comment");
        assert!(
            attr.leading
                .tokens
                .iter()
                .any(|t| t.text.contains("// Another comment"))
        );

        // with # comment
        attr.with_leading_comment("# Hash comment");
        assert!(
            attr.leading
                .tokens
                .iter()
                .any(|t| t.text.contains("# Hash comment"))
        );

        // 5. remove_attribute where items has block and another attribute
        let removed = doc.remove_attribute("other");
        assert!(removed.is_some());
        let removed_count = doc.remove_attribute("count");
        assert!(removed_count.is_some());
        assert_eq!(doc.attributes.len(), 0);
        assert_eq!(doc.items.len(), 1); // only the block remains
        assert!(doc.remove_attribute("nonexistent").is_none());

        // 6. remove_attribute when item is not found in items
        let mut doc_missing_item = Document::new();
        doc_missing_item.attributes.push(attr);
        let removed_missing = doc_missing_item.remove_attribute("count");
        assert!(removed_missing.is_some());

        // 7. replace_attribute_expr
        let mut doc_replace = Document::new();
        doc_replace.set_attribute("other", &expr1);
        doc_replace
            .items
            .push(DocumentItem::Trivia(TokenStream::new()));
        doc_replace.set_attribute("timeout", &expr1);
        let new_expr = Expression::Number(crate::number::Number::from(99), span.clone());
        assert!(
            doc_replace
                .replace_attribute_expr("timeout", &new_expr)
                .is_ok()
        );
        assert!(
            doc_replace
                .replace_attribute_expr("nonexistent", &new_expr)
                .is_err()
        );

        // 8. remove_block
        let mut doc_blk = Document::new();
        doc_blk.set_attribute("attr_first", &expr1);
        let blk0 = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "unmatched_service", span.clone()),
            labels: vec![],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        };
        doc_blk.append_block(blk0);
        let blk1 = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "service", span.clone()),
            labels: vec![Token::new(TokenKind::Ident, "web", span.clone())],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        };
        doc_blk.append_block(blk1);
        assert_eq!(doc_blk.blocks.len(), 2);
        assert_eq!(doc_blk.items.len(), 3);

        assert!(doc_blk.remove_block("service", &["web"]).is_some());
        assert_eq!(doc_blk.blocks.len(), 1);
        assert_eq!(doc_blk.items.len(), 2);
        assert!(doc_blk.remove_block("service", &["web"]).is_none());

        // remove_block with empty labels
        let blk2 = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "config", span.clone()),
            labels: vec![],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span),
            trailing: TokenStream::new(),
        };
        doc_blk.append_block(blk2.clone());
        assert!(doc_blk.remove_block("config", &[]).is_some());

        // remove_block when block is in blocks but not in items
        let mut doc_no_item_block = Document::new();
        doc_no_item_block.blocks.push(blk2);
        assert!(doc_no_item_block.remove_block("config", &[]).is_some());
    }

    #[test]
    fn test_surgical_insertions_and_walk_tokens() {
        let span = Span::new(0, 1, 1, 1, 1, 2);
        let make_attr = |name: &str| {
            let mut leading = TokenStream::new();
            leading.push(Token::new(TokenKind::Whitespace, "  ", span.clone()));
            let mut equals_leading = TokenStream::new();
            equals_leading.push(Token::new(TokenKind::Whitespace, " ", span.clone()));
            let mut trailing = TokenStream::new();
            trailing.push(Token::new(TokenKind::Newline, "\n", span.clone()));
            Attribute {
                leading,
                name: Token::new(TokenKind::Ident, name, span.clone()),
                equals_leading,
                equals: Token::new(TokenKind::Assign, "=", span.clone()),
                expr_tokens: vec![Token::new(TokenKind::Number, "1", span.clone())],
                trailing,
            }
        };
        let make_block = |typ: &str, label: &str| {
            let mut leading = TokenStream::new();
            leading.push(Token::new(TokenKind::Whitespace, "  ", span.clone()));
            let mut trailing = TokenStream::new();
            trailing.push(Token::new(TokenKind::Newline, "\n", span.clone()));
            Block {
                leading,
                type_ident: Token::new(TokenKind::Ident, typ, span.clone()),
                labels: if label.is_empty() {
                    vec![]
                } else {
                    vec![Token::new(TokenKind::Ident, label, span.clone())]
                },
                open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
                body: Box::new(Document::new()),
                close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
                trailing,
            }
        };

        let mut doc = Document::new();
        // Insert a trivia item first so attribute search traverses non-attribute items
        let mut lead_trivia = TokenStream::new();
        lead_trivia.push(Token::new(TokenKind::Comment, "# preamble\n", span.clone()));
        doc.items.push(DocumentItem::Trivia(lead_trivia));

        let a2 = make_attr("a2");
        doc.attributes.push(a2.clone());
        doc.items.push(DocumentItem::Attribute(a2));

        // 1. insert_attribute_before
        let a1 = make_attr("a1");
        assert!(doc.insert_attribute_before("a2", a1).is_ok());
        assert_eq!(doc.attributes[0].name.text, "a1");
        assert_eq!(doc.attributes[1].name.text, "a2");

        // 2. insert_attribute_after
        let a3 = make_attr("a3");
        assert!(doc.insert_attribute_after("a2", a3).is_ok());
        assert_eq!(doc.attributes[2].name.text, "a3");

        // 3. Error on missing target
        assert!(
            doc.insert_attribute_before("nonexistent", make_attr("err"))
                .is_err()
        );
        assert!(
            doc.insert_attribute_after("nonexistent", make_attr("err"))
                .is_err()
        );

        // 4. insert_block_before & insert_block_after with mixed items (attributes + blocks)
        let b2 = make_block("resource", "b2");
        doc.blocks.push(b2.clone());
        doc.items.push(DocumentItem::Block(b2));

        let b1 = make_block("resource", "b1");
        assert!(doc.insert_block_before("resource", &["b2"], b1).is_ok());
        assert_eq!(doc.blocks[0].labels[0].text, "b1");

        let b3 = make_block("resource", "b3");
        assert!(doc.insert_block_after("resource", &["b2"], b3).is_ok());
        assert_eq!(doc.blocks[2].labels[0].text, "b3");

        // Error on missing target block
        assert!(
            doc.insert_block_before("missing", &[], make_block("b", ""))
                .is_err()
        );
        assert!(
            doc.insert_block_after("missing", &[], make_block("b", ""))
                .is_err()
        );

        // 0-label block insertion
        let blk_zero = make_block("locals", "");
        doc.blocks.push(blk_zero.clone());
        doc.items.push(DocumentItem::Block(blk_zero));
        assert!(
            doc.insert_block_before("locals", &[], make_block("locals_before", ""))
                .is_ok()
        );
        assert!(
            doc.insert_block_after("locals", &[], make_block("locals_after", ""))
                .is_ok()
        );

        // remove_block with non-empty matching and non-matching labels
        assert!(doc.remove_block("resource", &["wrong"]).is_none());
        assert!(doc.remove_block("resource", &["b1"]).is_some());

        // 5. walk_tokens and walk_tokens_mut
        let mut trivia_stream = TokenStream::new();
        trivia_stream.push(Token::new(TokenKind::Comment, "# trivia\n", span.clone()));
        doc.items.push(DocumentItem::Trivia(trivia_stream));
        doc.leading
            .push(Token::new(TokenKind::Whitespace, " ", span.clone()));
        doc.trailing
            .push(Token::new(TokenKind::Newline, "\n", span));

        let mut token_count = 0;
        doc.walk_tokens(|_| {
            token_count += 1;
        });
        assert!(token_count > 0);

        let mut visited_kinds = Vec::new();
        doc.walk_tokens_mut(|tok| {
            visited_kinds.push(tok.kind.clone());
        });
        assert_eq!(visited_kinds.len(), token_count);
    }
}
