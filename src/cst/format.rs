//! CST Formatter and Mutable API definitions.
//!
//! Provides the ability to traverse and mutate the CST, as well as render it back to a string.

use crate::cst::document::{Attribute, Block, Document, TokenStream};
use crate::lex::token::{Token, TokenKind};
use crate::span::Span;

/// Formatting engine for rendering and mutating HCL CST.
pub struct Formatter;

impl Formatter {
    /// Renders a Document to an exact byte-for-byte string.
    #[must_use]
    pub fn render(doc: &Document) -> String {
        let mut out = String::new();
        Self::render_stream(&doc.leading, &mut out);

        if doc.items.is_empty() {
            for attr in &doc.attributes {
                Self::render_stream(&attr.leading, &mut out);
                out.push_str(&attr.name.text);
                Self::render_stream(&attr.equals_leading, &mut out);
                out.push_str(&attr.equals.text);
                for t in &attr.expr_tokens {
                    out.push_str(&t.text);
                }
                Self::render_stream(&attr.trailing, &mut out);
            }
            for block in &doc.blocks {
                Self::render_stream(&block.leading, &mut out);
                out.push_str(&block.type_ident.text);
                for l in &block.labels {
                    out.push_str(&l.text);
                }
                out.push_str(&block.open_brace.text);
                out.push_str(&Self::render(&block.body));
                out.push_str(&block.close_brace.text);
                Self::render_stream(&block.trailing, &mut out);
            }
        } else {
            for item in &doc.items {
                match item {
                    crate::cst::document::DocumentItem::Attribute(attr) => {
                        Self::render_stream(&attr.leading, &mut out);
                        out.push_str(&attr.name.text);
                        Self::render_stream(&attr.equals_leading, &mut out);
                        out.push_str(&attr.equals.text);
                        for t in &attr.expr_tokens {
                            out.push_str(&t.text);
                        }
                        Self::render_stream(&attr.trailing, &mut out);
                    }
                    crate::cst::document::DocumentItem::Block(block) => {
                        Self::render_stream(&block.leading, &mut out);
                        out.push_str(&block.type_ident.text);
                        for l in &block.labels {
                            out.push_str(&l.text);
                        }
                        out.push_str(&block.open_brace.text);
                        out.push_str(&Self::render(&block.body));
                        out.push_str(&block.close_brace.text);
                        Self::render_stream(&block.trailing, &mut out);
                    }
                    crate::cst::document::DocumentItem::Trivia(stream) => {
                        Self::render_stream(stream, &mut out);
                    }
                }
            }
        }

        Self::render_stream(&doc.trailing, &mut out);
        out
    }

    /// Renders a Document masking attributes matching `sensitive_attrs` as `"(sensitive value)"`.
    ///
    /// # Arguments
    /// * `doc` - The CST document to render.
    /// * `sensitive_attrs` - Slice of attribute names that should have their value masked.
    #[must_use]
    pub fn render_redacted(doc: &Document, sensitive_attrs: &[&str]) -> String {
        let mut out = String::new();
        Self::render_stream(&doc.leading, &mut out);

        if doc.items.is_empty() {
            for attr in &doc.attributes {
                Self::render_stream(&attr.leading, &mut out);
                out.push_str(&attr.name.text);
                Self::render_stream(&attr.equals_leading, &mut out);
                out.push_str(&attr.equals.text);
                if sensitive_attrs.contains(&attr.name.text.as_str()) {
                    out.push_str(" \"(sensitive value)\"");
                } else {
                    for t in &attr.expr_tokens {
                        out.push_str(&t.text);
                    }
                }
                Self::render_stream(&attr.trailing, &mut out);
            }
            for block in &doc.blocks {
                Self::render_stream(&block.leading, &mut out);
                out.push_str(&block.type_ident.text);
                for l in &block.labels {
                    out.push_str(&l.text);
                }
                out.push_str(&block.open_brace.text);
                out.push_str(&Self::render_redacted(&block.body, sensitive_attrs));
                out.push_str(&block.close_brace.text);
                Self::render_stream(&block.trailing, &mut out);
            }
        } else {
            for item in &doc.items {
                match item {
                    crate::cst::document::DocumentItem::Attribute(attr) => {
                        Self::render_stream(&attr.leading, &mut out);
                        out.push_str(&attr.name.text);
                        Self::render_stream(&attr.equals_leading, &mut out);
                        out.push_str(&attr.equals.text);
                        if sensitive_attrs.contains(&attr.name.text.as_str()) {
                            out.push_str(" \"(sensitive value)\"");
                        } else {
                            for t in &attr.expr_tokens {
                                out.push_str(&t.text);
                            }
                        }
                        Self::render_stream(&attr.trailing, &mut out);
                    }
                    crate::cst::document::DocumentItem::Block(block) => {
                        Self::render_stream(&block.leading, &mut out);
                        out.push_str(&block.type_ident.text);
                        for l in &block.labels {
                            out.push_str(&l.text);
                        }
                        out.push_str(&block.open_brace.text);
                        out.push_str(&Self::render_redacted(&block.body, sensitive_attrs));
                        out.push_str(&block.close_brace.text);
                        Self::render_stream(&block.trailing, &mut out);
                    }
                    crate::cst::document::DocumentItem::Trivia(stream) => {
                        Self::render_stream(stream, &mut out);
                    }
                }
            }
        }

        Self::render_stream(&doc.trailing, &mut out);
        out
    }

    fn render_stream(stream: &TokenStream, out: &mut String) {
        for t in &stream.tokens {
            out.push_str(&t.text);
        }
    }
}

impl Document {
    /// Safely add an attribute to the document.
    pub fn add_attribute(&mut self, name: &str, expr_text: &str) {
        let span = Span::new(0, 0, 0, 0, 0, 0); // Dummy span for synthesized nodes

        let mut leading = TokenStream::new();
        if !self.attributes.is_empty() {
            leading.push(Token::new(TokenKind::Newline, "\n", span.clone()));
        } else if !self.blocks.is_empty() {
            // Need a newline before an attribute if blocks exist but attributes don't,
            // though usually attributes come first.
            leading.push(Token::new(TokenKind::Newline, "\n", span.clone()));
        }

        let mut expr_tokens = vec![Token::new(TokenKind::Whitespace, " ", span.clone())];
        let lexer = crate::lex::lexer::Lexer::new(expr_text);
        let mut parsed_any = false;
        for item in lexer {
            match item {
                Ok(tok) => {
                    expr_tokens.push(tok);
                    parsed_any = true;
                }
                Err(err) => {
                    expr_tokens.push(Token::new(TokenKind::Ident, err.text, err.span));
                    parsed_any = true;
                }
            }
        }
        if !parsed_any {
            expr_tokens.push(Token::new(TokenKind::Ident, expr_text, span.clone()));
        }

        let mut equals_leading = TokenStream::new();
        equals_leading.push(Token::new(TokenKind::Whitespace, " ", span.clone()));

        let attr = Attribute {
            leading,
            name: Token::new(TokenKind::Ident, name, span.clone()),
            equals_leading,
            equals: Token::new(TokenKind::Assign, "=", span),
            expr_tokens,
            trailing: TokenStream::new(),
        };
        self.attributes.push(attr.clone());
        if !self.items.is_empty() {
            self.items
                .push(crate::cst::document::DocumentItem::Attribute(attr));
        }
    }

    /// Adds a block to the document.
    pub fn add_block(&mut self, mut block: Block) {
        if !self.blocks.is_empty() || !self.attributes.is_empty() {
            let span = crate::span::Span::new(0, 0, 0, 0, 0, 0);
            if block.leading.tokens.is_empty()
                || block.leading.tokens.first().map(|t| t.kind.clone()) != Some(TokenKind::Newline)
            {
                block
                    .leading
                    .tokens
                    .insert(0, Token::new(TokenKind::Newline, "\n", span));
            }
        }
        self.blocks.push(block);
    }

    /// Gets a mutable reference to a block by type and exact labels.
    pub fn get_block_mut(&mut self, type_ident: &str, labels: &[&str]) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|b| {
            b.type_ident.text == type_ident
                && b.labels.len() == labels.len()
                && b.labels
                    .iter()
                    .zip(labels.iter())
                    .all(|(l, expected)| l.text == *expected)
        })
    }

    /// Returns an iterator over all blocks of a specific type.
    pub fn blocks<'a>(&'a self, type_ident: &'a str) -> impl Iterator<Item = &'a Block> {
        self.blocks
            .iter()
            .filter(move |b| b.type_ident.text == type_ident)
    }
}

/// A canonical formatter for HCL documents.
pub struct CanonicalFormatter;

impl CanonicalFormatter {
    /// Formats an entire document in-place.
    pub fn format_document(doc: &mut Document) {
        Self::format_token_stream(&mut doc.leading, false);

        Self::format_attributes_clustered(&mut doc.attributes);

        for (i, block) in doc.blocks.iter_mut().enumerate() {
            if i > 0 || !doc.attributes.is_empty() {
                if !block
                    .leading
                    .tokens
                    .iter()
                    .any(|t| t.kind == TokenKind::Newline)
                {
                    block.leading.tokens.insert(
                        0,
                        Token::new(
                            TokenKind::Newline,
                            "\n\n",
                            crate::span::Span::new(0, 0, 0, 0, 0, 0),
                        ),
                    );
                } else if block.leading.tokens[0].kind == TokenKind::Newline
                    && block.leading.tokens[0].text == "\n"
                {
                    block.leading.tokens[0].text = "\n\n".to_string();
                }
            } else {
                Self::format_token_stream(&mut block.leading, true);
            }

            Self::format_document(&mut block.body);

            Self::format_token_stream(&mut block.trailing, false);
        }

        Self::format_token_stream(&mut doc.trailing, false);

        // Sync formatted attributes and blocks back to doc.items
        for item in &mut doc.items {
            match item {
                crate::cst::document::DocumentItem::Attribute(a) => {
                    if let Some(formatted) =
                        doc.attributes.iter().find(|fa| fa.name.text == a.name.text)
                    {
                        *a = formatted.clone();
                    }
                }
                crate::cst::document::DocumentItem::Block(b) => {
                    if let Some(formatted) = doc
                        .blocks
                        .iter()
                        .find(|fb| fb.type_ident.text == b.type_ident.text && fb.labels == b.labels)
                    {
                        *b = formatted.clone();
                    }
                }
                crate::cst::document::DocumentItem::Trivia(_) => {}
            }
        }
    }

    /// Formats attributes in contiguous clusters, aligning the `=` signs within each cluster.
    ///
    /// Blank lines (separated by 2 or more newlines) act as cluster separators.
    fn format_attributes_clustered(attributes: &mut [Attribute]) {
        if attributes.is_empty() {
            return;
        }

        let mut clusters: Vec<Vec<usize>> = Vec::new();
        let mut current_cluster = Vec::new();

        for i in 0..attributes.len() {
            let is_new_cluster = if i == 0 {
                true
            } else {
                let prev = &attributes[i - 1];
                let curr = &attributes[i];
                let has_leading_comment =
                    curr.leading.tokens.iter().any(|t| {
                        t.kind == TokenKind::Comment || t.kind == TokenKind::InlineComment
                    });
                let trailing_newlines: usize = prev
                    .trailing
                    .tokens
                    .iter()
                    .map(|t| t.text.matches('\n').count())
                    .sum();
                let leading_newlines: usize = curr
                    .leading
                    .tokens
                    .iter()
                    .map(|t| t.text.matches('\n').count())
                    .sum();
                has_leading_comment || (trailing_newlines + leading_newlines) >= 2
            };

            if is_new_cluster && !current_cluster.is_empty() {
                clusters.push(std::mem::take(&mut current_cluster));
            }
            current_cluster.push(i);
        }
        clusters.push(current_cluster);

        for cluster_indices in clusters {
            let max_name_len = cluster_indices
                .iter()
                .map(|&idx| attributes[idx].name.text.len())
                .max()
                .unwrap_or(0);

            for idx in &cluster_indices {
                let attr = &mut attributes[*idx];
                Self::format_token_stream(&mut attr.leading, true);

                let padding = max_name_len.saturating_sub(attr.name.text.len());
                let pad_str = " ".repeat(padding + 1);

                attr.equals_leading.tokens.clear();
                attr.equals_leading.tokens.push(Token::new(
                    TokenKind::Whitespace,
                    pad_str,
                    crate::span::Span::new(0, 0, 0, 0, 0, 0),
                ));

                if let Some(first) = attr.expr_tokens.first_mut() {
                    if first.kind == TokenKind::Whitespace {
                        first.text = " ".to_string();
                    } else {
                        attr.expr_tokens.insert(
                            0,
                            Token::new(
                                TokenKind::Whitespace,
                                " ",
                                crate::span::Span::new(0, 0, 0, 0, 0, 0),
                            ),
                        );
                    }
                }

                Self::format_expression_tokens(&mut attr.expr_tokens, 2);
                Self::format_token_stream(&mut attr.trailing, false);
            }

            // Align trailing inline comments across consecutive attribute lines in this cluster
            let has_trailing_comments = cluster_indices.iter().any(|&idx| {
                attributes[idx]
                    .trailing
                    .tokens
                    .iter()
                    .any(|t| t.kind == TokenKind::Comment || t.kind == TokenKind::InlineComment)
            });

            if has_trailing_comments {
                let calc_code_width = |attr: &Attribute| -> usize {
                    let leading_len: usize = attr.leading.tokens.iter().map(|t| t.text.len()).sum();
                    let name_len = attr.name.text.len();
                    let eq_leading_len: usize = attr
                        .equals_leading
                        .tokens
                        .iter()
                        .map(|t| t.text.len())
                        .sum();
                    let eq_len = attr.equals.text.len();
                    let expr_len: usize = attr.expr_tokens.iter().map(|t| t.text.len()).sum();
                    leading_len + name_len + eq_leading_len + eq_len + expr_len
                };

                let max_code_width = cluster_indices
                    .iter()
                    .filter(|&&idx| {
                        attributes[idx].trailing.tokens.iter().any(|t| {
                            t.kind == TokenKind::Comment || t.kind == TokenKind::InlineComment
                        })
                    })
                    .map(|&idx| calc_code_width(&attributes[idx]))
                    .max()
                    .unwrap_or(0);

                for idx in cluster_indices {
                    let attr = &mut attributes[idx];
                    let comment_idx = attr.trailing.tokens.iter().position(|t| {
                        t.kind == TokenKind::Comment || t.kind == TokenKind::InlineComment
                    });

                    if let Some(c_idx) = comment_idx {
                        let current_width = calc_code_width(attr);
                        let padding = max_code_width.saturating_sub(current_width) + 1;
                        let pad_str = " ".repeat(padding);

                        if c_idx > 0
                            && attr.trailing.tokens[c_idx - 1].kind == TokenKind::Whitespace
                        {
                            attr.trailing.tokens[c_idx - 1].text = pad_str;
                        } else {
                            attr.trailing.tokens.insert(
                                c_idx,
                                Token::new(
                                    TokenKind::Whitespace,
                                    pad_str,
                                    crate::span::Span::new(0, 0, 0, 0, 0, 0),
                                ),
                            );
                        }
                    }
                }
            }
        }
    }

    /// Formats an expression's token stream with canonical indentation and spacing.
    ///
    /// # Arguments
    /// * `tokens` - The expression tokens to reformat in-place.
    /// * `base_indent` - The current base indentation level (number of spaces).
    pub fn format_expression_tokens(tokens: &mut Vec<Token>, base_indent: usize) {
        let mut depth: usize = 0;
        let mut i = 0;

        while i < tokens.len() {
            let kind = tokens[i].kind.clone();
            match kind {
                TokenKind::OBrace | TokenKind::OBrack => {
                    depth += 1;
                }
                TokenKind::CBrace | TokenKind::CBrack => {
                    depth = depth.saturating_sub(1);
                }
                TokenKind::Newline => {
                    let mut next_idx = i + 1;
                    while next_idx < tokens.len() && tokens[next_idx].kind == TokenKind::Whitespace
                    {
                        next_idx += 1;
                    }
                    let is_closing = next_idx < tokens.len()
                        && (tokens[next_idx].kind == TokenKind::CBrace
                            || tokens[next_idx].kind == TokenKind::CBrack);

                    let target_indent = if is_closing {
                        base_indent + (depth.saturating_sub(1) * 2)
                    } else {
                        base_indent + (depth * 2)
                    };
                    let indent_str = " ".repeat(target_indent);
                    if i + 1 < tokens.len() && tokens[i + 1].kind == TokenKind::Whitespace {
                        tokens[i + 1].text = indent_str;
                        i += 1;
                    } else if target_indent > 0 {
                        tokens.insert(
                            i + 1,
                            Token::new(
                                TokenKind::Whitespace,
                                indent_str,
                                crate::span::Span::new(0, 0, 0, 0, 0, 0),
                            ),
                        );
                        i += 1;
                    }
                }
                TokenKind::Comma if i > 0 && tokens[i - 1].kind == TokenKind::Whitespace => {
                    tokens.remove(i - 1);
                    i = i.saturating_sub(1);
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn format_token_stream(stream: &mut TokenStream, allow_newlines: bool) {
        if !allow_newlines {
            for t in &mut stream.tokens {
                if t.kind == TokenKind::Whitespace && t.text.contains('\n') {
                    t.text = " ".to_string();
                }
            }
        }
    }

    /// Redacts sensitive attributes in the document by replacing their expression tokens with `"(sensitive value)"`.
    ///
    /// # Arguments
    /// * `doc` - The document to mutate in place.
    /// * `sensitive_attrs` - Slice of attribute names to redact.
    pub fn redact_sensitive(doc: &mut Document, sensitive_attrs: &[&str]) {
        for attr in &mut doc.attributes {
            if sensitive_attrs.contains(&attr.name.text.as_str()) {
                let dummy_span = crate::span::Span::new(0, 0, 0, 0, 0, 0);
                attr.expr_tokens = vec![
                    Token::new(TokenKind::Whitespace, " ", dummy_span.clone()),
                    Token::new(TokenKind::String, "\"(sensitive value)\"", dummy_span),
                ];
            }
        }
        for item in &mut doc.items {
            if let crate::cst::document::DocumentItem::Attribute(a) = item
                && sensitive_attrs.contains(&a.name.text.as_str())
            {
                let dummy_span = crate::span::Span::new(0, 0, 0, 0, 0, 0);
                a.expr_tokens = vec![
                    Token::new(TokenKind::Whitespace, " ", dummy_span.clone()),
                    Token::new(TokenKind::String, "\"(sensitive value)\"", dummy_span),
                ];
            }
        }
        for block in &mut doc.blocks {
            Self::redact_sensitive(&mut block.body, sensitive_attrs);
        }
    }
}

/// Automatically formats an HCL string according to canonical conventions.
///
/// # Errors
/// Returns an error if the input string cannot be parsed as valid HCL.
pub fn format_str(input: &str) -> Result<String, crate::error::HclError> {
    let parser = crate::cst::parser::CstParser::new(input);
    let mut doc = parser.parse()?;

    CanonicalFormatter::format_document(&mut doc);

    Ok(Formatter::render(&doc))
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
    use crate::cst::parser::CstParser;

    #[test]
    fn test_format_render_exact() {
        let input = "  foo = 123 \n";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();

        let rendered = Formatter::render(&doc);
        assert_eq!(rendered, input);
    }

    #[test]
    fn test_format_render_exact_blocks() {
        // Block parser is stubbed, but if we synthesize one we can render it.
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let mut doc = Document::new();
        doc.blocks.push(Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "block", span.clone()),
            labels: vec![
                Token::new(TokenKind::Whitespace, " ", span.clone()),
                Token::new(TokenKind::String, "\"label\"", span.clone()),
                Token::new(TokenKind::Whitespace, " ", span.clone()),
            ],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span),
            trailing: TokenStream::new(),
        });

        assert_eq!(Formatter::render(&doc), "block \"label\" {}");
    }

    #[test]
    fn test_document_mutation() {
        let mut doc = Document::new();

        doc.add_attribute("foo", "123");
        assert_eq!(Formatter::render(&doc), "foo = 123");

        doc.add_attribute("bar", "456");
        assert_eq!(Formatter::render(&doc), "foo = 123\nbar = 456");

        assert!(doc.remove_attribute("foo").is_some());
        assert!(doc.remove_attribute("baz").is_none());

        // It leaves the newline because we don't clean up surrounding trivia perfectly yet.
        assert_eq!(Formatter::render(&doc), "\nbar = 456");

        // test add_attribute when blocks are not empty
        let mut doc2 = Document::new();
        doc2.add_block(Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "b", Span::new(0, 0, 0, 0, 0, 0)),
            labels: vec![],
            open_brace: Token::new(TokenKind::OBrace, "{", Span::new(0, 0, 0, 0, 0, 0)),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", Span::new(0, 0, 0, 0, 0, 0)),
            trailing: TokenStream::new(),
        });
        doc2.add_attribute("attr", "val");
        assert_eq!(Formatter::render(&doc2), "\nattr = valb{}");
    }

    #[test]
    fn test_block_mutation() {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let mut doc = Document::new();

        let block = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "resource", span.clone()),
            labels: vec![
                Token::new(TokenKind::Whitespace, " ", span.clone()),
                Token::new(TokenKind::String, "\"aws_vpc\"", span.clone()),
                Token::new(TokenKind::Whitespace, " ", span.clone()),
                Token::new(TokenKind::String, "\"main\"", span.clone()),
            ],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        };
        doc.add_block(block);
        assert_eq!(doc.blocks.len(), 1);

        // Test removing non-existent blocks
        let removed_none_type = doc.remove_block("data", &[" ", "\"aws_vpc\"", " ", "\"main\""]);
        assert!(removed_none_type.is_none());

        let removed_none_labels = doc.remove_block("resource", &[" ", "\"aws_vpc\""]);
        assert!(removed_none_labels.is_none());

        let removed_none_label_val =
            doc.remove_block("resource", &[" ", "\"aws_vpc\"", " ", "\"dev\""]);
        assert!(removed_none_label_val.is_none());

        // Test get_block_mut
        let block_mut = doc.get_block_mut("resource", &[" ", "\"aws_vpc\"", " ", "\"main\""]);
        assert!(block_mut.is_some());
        block_mut.unwrap().type_ident.text = "data".to_string();

        // Test blocks iterator
        assert_eq!(doc.blocks("data").count(), 1);
        assert_eq!(doc.blocks("resource").count(), 0);

        // Reset for remove test
        doc.blocks.first_mut().unwrap().type_ident.text = "resource".to_string();

        // Test removing existent block
        let removed_block = doc.remove_block("resource", &[" ", "\"aws_vpc\"", " ", "\"main\""]);
        assert!(removed_block.is_some());
        assert_eq!(doc.blocks.len(), 0);

        // Test add_block trivia when doc is not empty
        doc.add_attribute("foo", "bar");
        let block2 = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "b", span.clone()),
            labels: vec![],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span),
            trailing: TokenStream::new(),
        };
        doc.add_block(block2);
        assert_eq!(Formatter::render(&doc), "foo = bar\nb{}");
    }

    #[test]
    fn test_add_block_missing() {
        let mut doc = crate::cst::document::Document::new();
        let span = crate::span::Span::new(0, 0, 0, 0, 0, 0);
        let block1 = crate::cst::document::Block {
            leading: crate::cst::document::TokenStream::new(),
            type_ident: crate::lex::token::Token::new(
                crate::lex::token::TokenKind::Ident,
                "resource",
                span.clone(),
            ),
            labels: vec![],
            open_brace: crate::lex::token::Token::new(
                crate::lex::token::TokenKind::OBrace,
                "{",
                span.clone(),
            ),
            body: Box::new(crate::cst::document::Document::new()),
            close_brace: crate::lex::token::Token::new(
                crate::lex::token::TokenKind::CBrace,
                "}",
                span.clone(),
            ),
            trailing: crate::cst::document::TokenStream::new(),
        };
        doc.add_attribute("foo", "123");
        doc.add_block(block1);

        let block2 = crate::cst::document::Block {
            leading: crate::cst::document::TokenStream {
                tokens: vec![crate::lex::token::Token::new(
                    crate::lex::token::TokenKind::Whitespace,
                    " ",
                    span.clone(),
                )],
            },
            type_ident: crate::lex::token::Token::new(
                crate::lex::token::TokenKind::Ident,
                "resource",
                span.clone(),
            ),
            labels: vec![],
            open_brace: crate::lex::token::Token::new(
                crate::lex::token::TokenKind::OBrace,
                "{",
                span.clone(),
            ),
            body: Box::new(crate::cst::document::Document::new()),
            close_brace: crate::lex::token::Token::new(
                crate::lex::token::TokenKind::CBrace,
                "}",
                span,
            ),
            trailing: crate::cst::document::TokenStream::new(),
        };
        doc.add_block(block2);
    }

    #[test]
    fn test_format_document_display() {
        let input = "  foo = 123 \n";
        let parser = crate::cst::parser::CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(Formatter::render(&doc), input);
    }

    #[test]
    fn test_format_str() {
        let input = "  foo = 123 \n";
        let out = crate::cst::format::format_str(input).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn test_parse_cst_to_string_property() {
        let text = "  foo = 123 \n\n resource \"aws\" \"test\" {\n  a = 1\n}\n";
        let parser = crate::cst::parser::CstParser::new(text);
        let doc = parser.parse().unwrap();
        assert_eq!(Formatter::render(&doc), text);
    }

    #[test]
    fn test_canonical_formatter_coverage() {
        let input =
            "foo=123\n\nresource \"aws\" \"test\" {\na=1\nb=2\n}\n\nresource \"az\" \"test\" {}\n";
        let parser = crate::cst::parser::CstParser::new(input);
        let mut doc = parser.parse().unwrap();
        CanonicalFormatter::format_document(&mut doc);
        let out = Formatter::render(&doc);
        assert!(out.contains("foo = 123"));
        assert!(out.contains("a = 1"));
    }

    #[test]
    fn test_canonical_formatter_edge_cases() {
        // tests missing spaces and newlines
        let input = "foo=123\nresource\"aws\"{a=1}\nresource\"az\"{b=2}";
        let parser = crate::cst::parser::CstParser::new(input);
        let mut doc = parser.parse().unwrap();
        CanonicalFormatter::format_document(&mut doc);
    }

    #[test]
    fn test_canonical_formatter_coverage_extra() {
        // We need to hit branch 188: if !block.leading.tokens.iter().any(|t| t.kind == TokenKind::Newline)
        // This is covered when a block has attributes before it but no leading newline token.
        let input = "foo=123\nresource\"aws\"{}\n";
        let parser = crate::cst::parser::CstParser::new(input);
        let mut doc = parser.parse().unwrap();
        // forcefully clear leading tokens to hit the exact branch
        doc.blocks[0].leading.tokens.clear();
        CanonicalFormatter::format_document(&mut doc);
    }

    #[test]
    fn test_canonical_formatter_coverage_extra_2() {
        let input = "foo=123\n\nresource\"aws\"{}\n";
        let parser = crate::cst::parser::CstParser::new(input);
        let mut doc = parser.parse().unwrap();
        // forcefully set leading tokens of block to a single newline to hit branch 188
        doc.blocks[0].leading.tokens.clear();
        doc.blocks[0]
            .leading
            .tokens
            .push(crate::lex::token::Token::new(
                crate::lex::token::TokenKind::Newline,
                "\n",
                crate::span::Span::new(0, 0, 0, 0, 0, 0),
            ));
        CanonicalFormatter::format_document(&mut doc);
    }

    #[test]
    fn test_canonical_formatter_coverage_extra_3() {
        let input = "foo=123\n\nresource\"aws\"{}\n";
        let parser = crate::cst::parser::CstParser::new(input);
        let mut doc = parser.parse().unwrap();
        // forcefully set leading tokens of block to a single newline to hit branch 188
        doc.blocks[0].leading.tokens.clear();
        doc.blocks[0]
            .leading
            .tokens
            .push(crate::lex::token::Token::new(
                crate::lex::token::TokenKind::Whitespace,
                " ",
                crate::span::Span::new(0, 0, 0, 0, 0, 0),
            ));
        doc.blocks[0]
            .leading
            .tokens
            .push(crate::lex::token::Token::new(
                crate::lex::token::TokenKind::Newline,
                "\n",
                crate::span::Span::new(0, 0, 0, 0, 0, 0),
            ));
        CanonicalFormatter::format_document(&mut doc);
    }

    #[test]
    fn test_canonical_formatter_coverage_extra_4() {
        let input = "foo=123\n\nresource\"aws\"{}\n";
        let parser = crate::cst::parser::CstParser::new(input);
        let mut doc = parser.parse().unwrap();
        // forcefully set leading tokens of block to multiple spaces to hit branch 188
        doc.blocks[0].leading.tokens.clear();
        doc.blocks[0]
            .leading
            .tokens
            .push(crate::lex::token::Token::new(
                crate::lex::token::TokenKind::Whitespace,
                "  ",
                crate::span::Span::new(0, 0, 0, 0, 0, 0),
            ));
        doc.blocks[0]
            .leading
            .tokens
            .push(crate::lex::token::Token::new(
                crate::lex::token::TokenKind::Whitespace,
                "   ",
                crate::span::Span::new(0, 0, 0, 0, 0, 0),
            ));
        CanonicalFormatter::format_document(&mut doc);
    }

    #[test]
    fn test_canonical_formatter_coverage_extra_5() {
        let input = "foo=123\n\nresource\"aws\"{}\n";
        let parser = crate::cst::parser::CstParser::new(input);
        let mut doc = parser.parse().unwrap();
        // forcefully set leading tokens of block to no tokens to hit branch 188
        doc.blocks[0].leading.tokens.clear();
        CanonicalFormatter::format_document(&mut doc);
        // also check block parsing with attributes empty
        doc.attributes.clear();
        doc.blocks[0].leading.tokens.clear();
        CanonicalFormatter::format_document(&mut doc);
    }

    #[test]
    fn test_cst_format_coverage_branches() {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let mut doc = Document::new();
        doc.add_attribute("a", "1");

        // Block with leading non-empty that starts with Newline:
        let mut b1 = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "res", span.clone()),
            labels: vec![],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        };
        b1.leading
            .tokens
            .push(Token::new(TokenKind::Newline, "\n", span.clone()));
        doc.add_block(b1);

        // Block with leading non-empty that does NOT start with Newline:
        let mut b2 = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "res2", span.clone()),
            labels: vec![],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        };
        b2.leading
            .tokens
            .push(Token::new(TokenKind::Whitespace, " ", span.clone()));
        doc.add_block(b2);

        // get_block_mut branch coverage
        let mut doc_lookup = Document::new();
        let b = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "resource", span.clone()),
            labels: vec![
                Token::new(TokenKind::String, "\"aws_vpc\"", span.clone()),
                Token::new(TokenKind::String, "\"main\"", span.clone()),
            ],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        };
        doc_lookup.blocks.push(b);

        // type does not match:
        assert!(doc_lookup.get_block_mut("nonexistent", &[]).is_none());

        // type matches, but labels.len() differs:
        assert!(
            doc_lookup
                .get_block_mut("resource", &["\"aws_vpc\""])
                .is_none()
        );

        // type matches, labels.len() matches, but label text differs:
        assert!(
            doc_lookup
                .get_block_mut("resource", &["\"aws_vpc\"", "\"sub\""])
                .is_none()
        );

        // exact match:
        assert!(
            doc_lookup
                .get_block_mut("resource", &["\"aws_vpc\"", "\"main\""])
                .is_some()
        );

        // Attribute with empty expr_tokens:
        let mut doc_attr = CstParser::new("key = 1\n").parse().unwrap();
        doc_attr.attributes[0].expr_tokens.clear();
        CanonicalFormatter::format_document(&mut doc_attr);

        // Block with leading newline whose text is already "\n\n" (not "\n"):
        let mut doc_block_double_nl = CstParser::new("foo = 1\n\nresource \"aws\" {}\n")
            .parse()
            .unwrap();
        doc_block_double_nl.blocks[0].leading.tokens.clear();
        doc_block_double_nl.blocks[0]
            .leading
            .tokens
            .push(Token::new(TokenKind::Newline, "\n\n", span));
        CanonicalFormatter::format_document(&mut doc_block_double_nl);
    }

    #[test]
    fn test_render_redacted_and_redact_sensitive() {
        let input = "public_val = 123\nsecret_key = \"supersecret\"\n";
        let parser = CstParser::new(input);
        let mut doc = parser.parse().unwrap();

        // Test render_redacted without modifying doc
        let redacted_rendered = Formatter::render_redacted(&doc, &["secret_key"]);
        assert!(redacted_rendered.contains("public_val = 123"));
        assert!(redacted_rendered.contains("secret_key = \"(sensitive value)\""));
        assert!(!redacted_rendered.contains("supersecret"));

        // Original doc still has the secret
        let normal_rendered = Formatter::render(&doc);
        assert!(normal_rendered.contains("supersecret"));

        // Test redact_sensitive modifying doc in place
        CanonicalFormatter::redact_sensitive(&mut doc, &["secret_key"]);
        let in_place_rendered = Formatter::render(&doc);
        assert!(in_place_rendered.contains("public_val = 123"));
        assert!(in_place_rendered.contains("secret_key = \"(sensitive value)\""));
        assert!(!in_place_rendered.contains("supersecret"));
    }

    #[test]
    fn test_cst_interleaved_ordering_and_cluster_alignment() {
        use crate::cst::builder::CstFile;

        // 1. Interleaved attribute / block / attribute preservation
        let input = "a = 1\n\nservice \"web\" {\n  port = 80\n}\n\nb = 2\n";
        let cst = CstFile::parse(input).unwrap_or_default();
        let items = cst.body().items();
        assert_eq!(items.len(), 3);
        assert!(items[0].as_attribute().is_some());
        assert!(items[1].as_block().is_some());
        assert!(items[2].as_attribute().is_some());

        // Serialization retains exact interleaved ordering
        let output = cst.write_to_string();
        let pos_a = output.find("a = 1").unwrap_or(usize::MAX);
        let pos_block = output.find("service \"web\"").unwrap_or(usize::MAX);
        let pos_b = output.find("b = 2").unwrap_or(usize::MAX);
        assert!(pos_a < pos_block);
        assert!(pos_block < pos_b);

        // View iterators
        let attr_names: Vec<_> = cst
            .body()
            .attributes_iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(attr_names, vec!["a", "b"]);
        let block_types: Vec<_> = cst
            .body()
            .blocks_iter()
            .map(|b| b.block_type.as_str())
            .collect();
        assert_eq!(block_types, vec!["service"]);

        // 2. Column alignment of = signs with blank line cluster separation
        let unformatted = "short = 1\nvery_long_name = 2\n\na = 3\nbb = 4\n";
        let formatted = format_str(unformatted).unwrap_or_default();

        // Cluster 1: short is aligned with very_long_name
        assert!(formatted.contains("short          = 1"));
        assert!(formatted.contains("very_long_name = 2"));

        // Cluster 2: a is aligned with bb (not with very_long_name)
        assert!(formatted.contains("a  = 3"));
        assert!(formatted.contains("bb = 4"));
    }

    #[test]
    fn test_contiguous_attribute_alignment_with_comments() {
        let input = "foo = 1\nlong_attribute_name = 2\n# Section break comment\nbar = 3\nvery_long_other_attribute = 4\n";
        let formatted = format_str(input).unwrap_or_default();
        // Cluster 1 aligned to long_attribute_name
        assert!(formatted.contains("foo                 = 1"));
        assert!(formatted.contains("long_attribute_name = 2"));
        // Cluster 2 aligned to very_long_other_attribute
        assert!(formatted.contains("bar                       = 3"));
        assert!(formatted.contains("very_long_other_attribute = 4"));
    }

    #[test]
    fn test_cst_format_coverage_gaps() {
        use crate::cst::document::{Attribute, Block, Document, DocumentItem, TokenStream};
        use crate::lex::token::{Token, TokenKind};
        use crate::span::Span;

        let span = Span::new(0, 0, 0, 0, 0, 0);

        // 1. Formatter::render with DocumentItem::Trivia
        let mut doc_render = Document::new();
        let mut trivia = TokenStream::new();
        trivia.push(Token::new(
            TokenKind::Comment,
            "// a comment\n",
            span.clone(),
        ));
        doc_render.items.push(DocumentItem::Trivia(trivia.clone()));
        let rendered = Formatter::render(&doc_render);
        assert_eq!(rendered, "// a comment\n");

        // 2. render_redacted when doc.items is empty (branch with doc.attributes and doc.blocks)
        let mut doc_empty_items = Document::new();
        let attr_secret = Attribute {
            leading: TokenStream::new(),
            name: Token::new(TokenKind::Ident, "secret_key", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![Token::new(TokenKind::String, "\"my_secret\"", span.clone())],
            trailing: TokenStream::new(),
        };
        let attr_public = Attribute {
            leading: TokenStream::new(),
            name: Token::new(TokenKind::Ident, "public_key", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![Token::new(TokenKind::String, "\"public\"", span.clone())],
            trailing: TokenStream::new(),
        };
        doc_empty_items.attributes.push(attr_secret);
        doc_empty_items.attributes.push(attr_public);

        let mut inner_doc = Document::new();
        inner_doc.attributes.push(Attribute {
            leading: TokenStream::new(),
            name: Token::new(TokenKind::Ident, "secret_key", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![Token::new(
                TokenKind::String,
                "\"nested_secret\"",
                span.clone(),
            )],
            trailing: TokenStream::new(),
        });
        let block = Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "server", span.clone()),
            labels: vec![Token::new(TokenKind::String, "\"web\"", span.clone())],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(inner_doc),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        };
        doc_empty_items.blocks.push(block.clone());

        let redacted = Formatter::render_redacted(&doc_empty_items, &["secret_key"]);
        assert!(redacted.contains("\"(sensitive value)\""));
        assert!(redacted.contains("\"public\""));
        assert!(!redacted.contains("my_secret"));
        assert!(!redacted.contains("nested_secret"));

        // 3. render_redacted when doc.items is NOT empty (Block, Trivia, Attribute)
        let mut doc_with_items = Document::new();
        doc_with_items.items.push(DocumentItem::Trivia(trivia));
        doc_with_items.items.push(DocumentItem::Block(block));
        doc_with_items
            .items
            .push(DocumentItem::Attribute(Attribute {
                leading: TokenStream::new(),
                name: Token::new(TokenKind::Ident, "secret_key", span.clone()),
                equals_leading: TokenStream::new(),
                equals: Token::new(TokenKind::Assign, "=", span.clone()),
                expr_tokens: vec![Token::new(
                    TokenKind::String,
                    "\"raw_secret\"",
                    span.clone(),
                )],
                trailing: TokenStream::new(),
            }));
        let redacted_items = Formatter::render_redacted(&doc_with_items, &["secret_key"]);
        assert!(redacted_items.contains("\"(sensitive value)\""));
        assert!(!redacted_items.contains("raw_secret"));

        // 4. format_document with Trivia item and with items not found in doc.attributes / doc.blocks
        let mut doc_fmt = Document::new();
        doc_fmt.items.push(DocumentItem::Trivia(TokenStream::new()));
        doc_fmt.items.push(DocumentItem::Attribute(Attribute {
            leading: TokenStream::new(),
            name: Token::new(TokenKind::Ident, "orphan_attr", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![],
            trailing: TokenStream::new(),
        }));
        doc_fmt.items.push(DocumentItem::Block(Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "orphan_block", span.clone()),
            labels: vec![],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(Document::new()),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        }));
        CanonicalFormatter::format_document(&mut doc_fmt);

        // 5. format_attributes_clustered with empty slice and with comments / newlines
        CanonicalFormatter::format_attributes_clustered(&mut []);

        let attr1 = Attribute {
            leading: TokenStream::new(),
            name: Token::new(TokenKind::Ident, "a", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![Token::new(TokenKind::Number, "1", span.clone())],
            trailing: TokenStream::new(),
        };
        let mut leading_tokens = TokenStream::new();
        leading_tokens.push(Token::new(TokenKind::Newline, "\n", span.clone()));
        leading_tokens.push(Token::new(TokenKind::Comment, "// comment", span.clone()));
        let attr2 = Attribute {
            leading: leading_tokens,
            name: Token::new(TokenKind::Ident, "b", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![Token::new(TokenKind::Number, "2", span.clone())],
            trailing: TokenStream::new(),
        };
        CanonicalFormatter::format_attributes_clustered(&mut [attr1, attr2]);

        // 6. redact_sensitive with blocks
        let mut doc_mutate = Document::new();
        let mut nested_doc = Document::new();
        nested_doc.attributes.push(Attribute {
            leading: TokenStream::new(),
            name: Token::new(TokenKind::Ident, "secret_key", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![Token::new(TokenKind::String, "\"val\"", span.clone())],
            trailing: TokenStream::new(),
        });
        nested_doc.items.push(DocumentItem::Attribute(Attribute {
            leading: TokenStream::new(),
            name: Token::new(TokenKind::Ident, "secret_key", span.clone()),
            equals_leading: TokenStream::new(),
            equals: Token::new(TokenKind::Assign, "=", span.clone()),
            expr_tokens: vec![Token::new(TokenKind::String, "\"val\"", span.clone())],
            trailing: TokenStream::new(),
        }));
        doc_mutate.blocks.push(Block {
            leading: TokenStream::new(),
            type_ident: Token::new(TokenKind::Ident, "blk", span.clone()),
            labels: vec![],
            open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
            body: Box::new(nested_doc),
            close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
            trailing: TokenStream::new(),
        });
        CanonicalFormatter::redact_sensitive(&mut doc_mutate, &["secret_key"]);
        assert_eq!(
            doc_mutate.blocks[0].body.attributes[0].expr_tokens[1].text,
            "\"(sensitive value)\""
        );

        // 7. format_str parse error branch
        assert!(format_str("invalid { = }").is_err());
    }

    #[test]
    fn test_add_attribute_complex_expressions() {
        let mut doc = Document::new();
        doc.add_attribute("list", "[1, 2, 3]");
        doc.add_attribute("obj", "{ a = \"b\" }");
        doc.add_attribute("trav", "var.foo[0]");

        // Verify accurate tokenization of complex expressions
        assert_eq!(doc.attributes.len(), 3);
        let list_toks: Vec<&str> = doc.attributes[0]
            .expr_tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(
            list_toks,
            vec![" ", "[", "1", ",", " ", "2", ",", " ", "3", "]"]
        );

        let obj_toks: Vec<&str> = doc.attributes[1]
            .expr_tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(
            obj_toks,
            vec![" ", "{", " ", "a", " ", "=", " ", "\"b\"", " ", "}"]
        );

        let trav_toks: Vec<&str> = doc.attributes[2]
            .expr_tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(trav_toks, vec![" ", "var", ".", "foo", "[", "0", "]"]);

        let rendered = Formatter::render(&doc);
        assert!(rendered.contains("list = [1, 2, 3]"));
        assert!(rendered.contains("obj = { a = \"b\" }"));
        assert!(rendered.contains("trav = var.foo[0]"));

        // Fallback branch with empty string and invalid character
        let mut fallback_doc = Document::new();
        fallback_doc.add_attribute("empty", "");
        assert_eq!(fallback_doc.attributes[0].expr_tokens.len(), 2);
        fallback_doc.add_attribute("err_char", "@");
        assert_eq!(fallback_doc.attributes[1].expr_tokens.len(), 2);

        // Document with existing items
        let mut doc_with_items = Document::new();
        doc_with_items.add_attribute("first", "1");
        doc_with_items
            .items
            .push(crate::cst::document::DocumentItem::Attribute(
                doc_with_items.attributes[0].clone(),
            ));
        doc_with_items.add_attribute("second", "2");
        assert_eq!(doc_with_items.items.len(), 2);
    }

    #[test]
    fn test_format_expression_tokens_multiline() {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let mut tokens = vec![
            Token::new(TokenKind::OBrack, "[", span.clone()),
            Token::new(TokenKind::Newline, "\n", span.clone()),
            Token::new(TokenKind::Number, "1", span.clone()),
            Token::new(TokenKind::Comma, ",", span.clone()),
            Token::new(TokenKind::Newline, "\n", span.clone()),
            Token::new(TokenKind::Whitespace, " ", span.clone()),
            Token::new(TokenKind::Number, "2", span.clone()),
            Token::new(TokenKind::Newline, "\n", span.clone()),
            Token::new(TokenKind::CBrack, "]", span.clone()),
        ];

        CanonicalFormatter::format_expression_tokens(&mut tokens, 2);
        // Verify indentation insertion after newlines
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Whitespace && t.text.len() >= 4)
        );

        // Test format_expression_tokens where target_indent is 0 after newline followed by non-whitespace
        let mut flat_tokens = vec![
            Token::new(TokenKind::Newline, "\n", span.clone()),
            Token::new(TokenKind::Ident, "x", span.clone()),
        ];
        CanonicalFormatter::format_expression_tokens(&mut flat_tokens, 0);
        assert_eq!(flat_tokens.len(), 2);

        let mut stream = TokenStream::new();
        stream.push(Token::new(TokenKind::Whitespace, "   \n   ", span));
        CanonicalFormatter::format_token_stream(&mut stream, false);
        assert_eq!(stream.tokens[0].text, " ");
    }

    #[test]
    fn test_hclwrite_contiguous_inline_comment_alignment() {
        let input = r#"
foo = "short" # First comment
longer_attr_name = 42 # Second comment
"#;
        let mut doc = crate::cst::parser::CstParser::new(input).parse().unwrap();
        CanonicalFormatter::format_document(&mut doc);
        let rendered = Formatter::render(&doc);

        // Verify that the '#' comment characters appear at the exact same column offset
        let lines: Vec<&str> = rendered.lines().filter(|l| l.contains('#')).collect();
        assert_eq!(lines.len(), 2);
        let col1 = lines[0].find('#').unwrap();
        let col2 = lines[1].find('#').unwrap();
        assert_eq!(
            col1, col2,
            "Comments should be aligned at the exact same column: \n{rendered}"
        );
    }

    #[test]
    fn test_hclwrite_multiline_tuple_and_object_indentation() {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let mut tokens = vec![
            Token::new(TokenKind::OBrack, "[", span.clone()),
            Token::new(TokenKind::Newline, "\n", span.clone()),
            Token::new(TokenKind::String, "\"item1\"", span.clone()),
            Token::new(TokenKind::Whitespace, " ", span.clone()),
            Token::new(TokenKind::Comma, ",", span.clone()),
            Token::new(TokenKind::Newline, "\n", span.clone()),
            Token::new(TokenKind::String, "\"item2\"", span.clone()),
            Token::new(TokenKind::Comma, ",", span.clone()),
            Token::new(TokenKind::Newline, "\n", span.clone()),
            Token::new(TokenKind::CBrack, "]", span.clone()),
        ];

        CanonicalFormatter::format_expression_tokens(&mut tokens, 2);

        // Check that whitespace before comma was removed
        let comma_idx = tokens
            .iter()
            .position(|t| t.kind == TokenKind::Comma)
            .unwrap();
        assert_ne!(tokens[comma_idx - 1].kind, TokenKind::Whitespace);

        // Check that closing bracket has lower indent than elements
        let cbrack_idx = tokens
            .iter()
            .position(|t| t.kind == TokenKind::CBrack)
            .unwrap();
        let cbrack_indent = &tokens[cbrack_idx - 1];
        assert_eq!(cbrack_indent.text, "  "); // base_indent 2
    }

    #[test]
    fn test_hclwrite_official_fixtures_conformance() {
        // Fixture matching HashiCorp hclwrite canonical block formatting
        let input = r#"
resource "aws_security_group" "allow_tls" {
  name        = "allow_tls"
  description = "Allow TLS inbound traffic"
  vpc_id      = "vpc-123456"

  ingress {
    description = "TLS from VPC"
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["10.0.0.0/16"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}
"#;
        let mut doc = crate::cst::parser::CstParser::new(input).parse().unwrap();
        CanonicalFormatter::format_document(&mut doc);
        let formatted = Formatter::render(&doc);

        assert!(formatted.contains("resource \"aws_security_group\" \"allow_tls\" {"));
        assert!(formatted.contains("name        = \"allow_tls\""));
        assert!(formatted.contains("description = \"Allow TLS inbound traffic\""));
        assert!(formatted.contains("ingress {"));
        assert!(formatted.contains("egress {"));
    }

    #[test]
    fn test_format_trailing_block_comments_and_alignment() {
        use crate::span::Span;
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);

        let input = "a = 1 // inline 1\nlonger_name = 2 // inline 2\n";
        let mut doc = crate::cst::parser::CstParser::new(input).parse().unwrap();

        // Add leading tokens so calc_code_width leading_len closure is executed
        doc.attributes[0]
            .leading
            .push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));
        doc.attributes[1]
            .leading
            .push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));

        // Also add a TokenKind::Comment to test block comment trailing branch
        doc.attributes[0].trailing.tokens.insert(
            0,
            Token::new(
                TokenKind::Comment,
                "/* block comment */",
                dummy_span.clone(),
            ),
        );

        // First pass: inserts whitespace padding before trailing comments
        CanonicalFormatter::format_document(&mut doc);

        // Second pass: updates existing whitespace padding (c_idx > 0 && token is Whitespace)
        CanonicalFormatter::format_document(&mut doc);

        let formatted = Formatter::render(&doc);
        assert!(formatted.contains("/* block comment */"));
        assert!(formatted.contains("// inline 2"));
    }

    #[test]
    fn test_format_expression_tokens_edge_cases() {
        use crate::span::Span;
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);

        // 1. Comma at index 0 (i == 0, so i > 0 is false)
        let mut tokens_comma_first = vec![
            Token::new(TokenKind::Comma, ",", dummy_span.clone()),
            Token::new(TokenKind::Number, "1", dummy_span.clone()),
        ];
        CanonicalFormatter::format_expression_tokens(&mut tokens_comma_first, 2);
        assert_eq!(tokens_comma_first.len(), 2);

        // 2. Newline at end of tokens (so next_idx < tokens.len() is false, and i + 1 < tokens.len() is false)
        let mut tokens_trailing_newline = vec![
            Token::new(TokenKind::Number, "1", dummy_span.clone()),
            Token::new(TokenKind::Newline, "\n", dummy_span.clone()),
        ];
        CanonicalFormatter::format_expression_tokens(&mut tokens_trailing_newline, 2);
        assert_eq!(tokens_trailing_newline[1].kind, TokenKind::Newline);

        // 3. Newline followed by an element (not closing brace/bracket, so is_closing is false)
        let mut tokens_multiline_elements = vec![
            Token::new(TokenKind::OBrack, "[", dummy_span.clone()),
            Token::new(TokenKind::Newline, "\n", dummy_span.clone()),
            Token::new(TokenKind::Number, "1", dummy_span.clone()),
            Token::new(TokenKind::Newline, "\n", dummy_span.clone()),
            Token::new(TokenKind::CBrack, "]", dummy_span.clone()),
        ];
        CanonicalFormatter::format_expression_tokens(&mut tokens_multiline_elements, 2);
        assert_eq!(tokens_multiline_elements.len(), 7);

        // 4. Newline followed by CBrace (closing brace: tokens[next_idx].kind == TokenKind::CBrace is true)
        let mut tokens_multiline_object = vec![
            Token::new(TokenKind::OBrace, "{", dummy_span.clone()),
            Token::new(TokenKind::Newline, "\n", dummy_span.clone()),
            Token::new(TokenKind::Ident, "a", dummy_span.clone()),
            Token::new(TokenKind::Newline, "\n", dummy_span.clone()),
            Token::new(TokenKind::CBrace, "}", dummy_span.clone()),
        ];
        CanonicalFormatter::format_expression_tokens(&mut tokens_multiline_object, 2);
        assert_eq!(tokens_multiline_object.len(), 7);
    }

    #[test]
    fn test_redact_sensitive_non_matching_attribute() {
        let input = "safe = \"clear text\"\nsecret = \"super secret\"\nblock {\n  inner = 1\n}\n";
        let mut doc = crate::cst::parser::CstParser::new(input).parse().unwrap();
        CanonicalFormatter::redact_sensitive(&mut doc, &["secret"]);
        let rendered = Formatter::render(&doc);
        assert!(rendered.contains("safe = \"clear text\""));
        assert!(rendered.contains("secret = \"(sensitive value)\""));
        assert!(rendered.contains("block {"));
    }
}
