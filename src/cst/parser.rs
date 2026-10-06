//! Parser for constructing the Concrete Syntax Tree.
use crate::cst::document::{Attribute, Block, Document, DocumentItem, TokenStream};
use crate::error::HclError;
use crate::lex::lexer::Lexer;
use crate::lex::token::{Token, TokenKind};
/// A parser that produces a `Document` CST from an HCL string.
pub struct CstParser<'a> {
    lexer: Lexer<'a>,
    current: Option<Token>,
}
impl<'a> CstParser<'a> {
    /// Create a new CST parser for the given input.
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Lexer::new(input);
        let current = loop {
            match lexer.next() {
                Some(Ok(t)) => break Some(t),
                Some(Err(_)) => {}
                None => break None,
            }
        };
        Self { lexer, current }
    }
    /// Advance the lexer and return the consumed token.
    fn advance(&mut self) -> Option<Token> {
        let current = self.current.take();
        self.current = loop {
            match self.lexer.next() {
                Some(Ok(t)) => break Some(t),
                Some(Err(_)) => {}
                None => break None,
            }
        };
        current
    }
    /// Peek at the current token.
    fn peek(&self) -> Option<&Token> {
        self.current.as_ref()
    }
    /// Consumes and returns the currently peeked token.
    fn consume_current(&mut self) -> Token {
        match self.advance() {
            Some(tok) => tok,
            None => Token {
                kind: TokenKind::Whitespace,
                text: String::new(),
                span: crate::span::Span::new(0, 0, 1, 1, 1, 1),
            },
        }
    }
    /// Advance and return the next token if it matches a formatting token.
    fn pop_formatting(&mut self) -> Option<Token> {
        match self.current.as_ref()?.kind {
            TokenKind::Whitespace
            | TokenKind::Newline
            | TokenKind::Comment
            | TokenKind::InlineComment => self.advance(),
            _ => None,
        }
    }
    /// Consume formatting tokens (whitespace, comments) into a `TokenStream`.
    fn consume_formatting(&mut self) -> TokenStream {
        let mut stream = TokenStream::new();
        while let Some(tok) = self.pop_formatting() {
            stream.push(tok);
        }
        stream
    }
    /// Advance the lexer and expect a token, returning `HclError::Parse` on EOF.
    fn advance_expect(&mut self, msg: &str) -> Result<Token, HclError> {
        self.advance()
            .ok_or_else(|| HclError::Parse(msg.to_string()))
    }
    /// Parse a complete HCL document into a CST.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not valid HCL.
    pub fn parse(mut self) -> Result<Document, HclError> {
        self.parse_document(false)
    }
    fn parse_document(&mut self, is_block_body: bool) -> Result<Document, HclError> {
        let mut doc = Document::new();
        doc.leading = self.consume_formatting();
        while let Some(tok) = self.peek() {
            if is_block_body && tok.kind == TokenKind::CBrace {
                break;
            }
            if tok.kind == TokenKind::Ident {
                let ident = self.consume_current();
                let post_ident_formatting = self.consume_formatting();
                if let Some(next) = self.peek() {
                    if next.kind == TokenKind::Assign {
                        let equals = self.consume_current();
                        let post_equals = self.consume_formatting();
                        let mut expr_tokens = Vec::new();
                        expr_tokens.extend(post_equals.tokens);
                        let trailing = self.parse_attribute_expr(&mut expr_tokens);
                        let mut leading = TokenStream::new();
                        std::mem::swap(&mut leading, &mut doc.trailing);
                        let attr = Attribute {
                            leading,
                            name: ident,
                            equals_leading: post_ident_formatting,
                            equals,
                            expr_tokens,
                            trailing,
                        };
                        doc.items.push(DocumentItem::Attribute(attr.clone()));
                        doc.attributes.push(attr);
                    } else if next.kind == TokenKind::Ident
                        || next.kind == TokenKind::String
                        || next.kind == TokenKind::OBrace
                    {
                        let mut labels = Vec::new();
                        labels.extend(post_ident_formatting.tokens);
                        while let Some(label_tok) = self.peek() {
                            match label_tok.kind {
                                TokenKind::Ident | TokenKind::String => {
                                    labels.push(self.consume_current());
                                    labels.extend(self.consume_formatting().tokens);
                                }
                                _ => break,
                            }
                        }
                        if let Some(open_brace) = self.peek() {
                            if open_brace.kind == TokenKind::OBrace {
                                let open_brace_tok = self.consume_current();
                                let body_doc = self.parse_document(true)?;
                                let close_brace_tok = self.advance_expect("Expected '}'")?;
                                let mut leading = TokenStream::new();
                                std::mem::swap(&mut leading, &mut doc.trailing);
                                let block = Block {
                                    leading,
                                    type_ident: ident,
                                    labels,
                                    open_brace: open_brace_tok,
                                    body: Box::new(body_doc),
                                    close_brace: close_brace_tok,
                                    trailing: TokenStream::new(),
                                };
                                doc.items.push(DocumentItem::Block(block.clone()));
                                doc.blocks.push(block);
                            } else {
                                return Err(HclError::Parse(format!(
                                    "Expected '{{' after block labels, found {:?}",
                                    open_brace.kind
                                )));
                            }
                        } else {
                            return Err(HclError::Parse(
                                "Unexpected EOF after block labels".to_string(),
                            ));
                        }
                    } else {
                        return Err(HclError::Parse(format!(
                            "Unexpected token after identifier: {:?}",
                            next.kind
                        )));
                    }
                } else {
                    return Err(HclError::Parse("Unexpected EOF".to_string()));
                }
            } else {
                return Err(HclError::Parse(format!("Unexpected token: {:?}", tok.kind)));
            }
            doc.trailing.tokens.extend(self.consume_formatting().tokens);
        }
        Ok(doc)
    }
    /// Pop the next attribute expression token and determine whether it terminates the expression.
    fn pop_attribute_expr_token(&mut self, depth: &mut usize) -> Option<(Token, bool)> {
        let kind = &self.current.as_ref()?.kind;
        if *depth == 0 && matches!(kind, TokenKind::CBrace) {
            return None;
        }
        let is_trailing =
            *depth == 0 && matches!(kind, TokenKind::Newline | TokenKind::InlineComment);
        let tok = self.consume_current();
        match tok.kind {
            TokenKind::OBrace | TokenKind::OBrack | TokenKind::OParen => *depth += 1,
            TokenKind::CBrack | TokenKind::CParen | TokenKind::CBrace => {
                *depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        Some((tok, is_trailing))
    }
    fn parse_attribute_expr(&mut self, expr_tokens: &mut Vec<Token>) -> TokenStream {
        let mut depth = 0;
        let mut trailing = TokenStream::new();
        while let Some((tok, is_trailing)) = self.pop_attribute_expr_token(&mut depth) {
            if is_trailing {
                trailing.push(tok);
                break;
            }
            expr_tokens.push(tok);
        }
        trailing.tokens.extend(self.consume_formatting().tokens);
        trailing
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
    fn test_cst_parser_basic_attribute() {
        let input = "  foo = 123 \n";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.leading.tokens.len(), 1);
        assert_eq!(doc.leading.tokens[0].kind, TokenKind::Whitespace);
        assert_eq!(doc.attributes.len(), 1);
        let attr = &doc.attributes[0];
        assert_eq!(attr.name.text, "foo");
        assert_eq!(attr.expr_tokens.len(), 3);
        assert_eq!(attr.trailing.tokens.len(), 1);
    }
    #[test]
    fn test_cst_parser_unexpected_eof() {
        let input = "foo";
        let parser = CstParser::new(input);
        assert!(parser.parse().is_err());
    }
    #[test]
    fn test_cst_parser_unexpected_token() {
        let input = "123 = 456";
        let parser = CstParser::new(input);
        assert!(parser.parse().is_err());
        let invalid = "=";
        assert!(CstParser::new(invalid).parse().is_err());
    }
    #[test]
    fn test_cst_parser_block_empty() {
        let input = "block {}";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.blocks.len(), 1);
        let block = &doc.blocks[0];
        assert_eq!(block.type_ident.text, "block");
        assert_eq!(block.labels.len(), 1);
        assert_eq!(block.open_brace.text, "{");
        assert_eq!(block.close_brace.text, "}");
        assert_eq!(block.body.attributes.len(), 0);
        assert_eq!(block.body.blocks.len(), 0);
    }
    #[test]
    fn test_cst_parser_block_with_labels() {
        let input = "resource \"aws_instance\" web { foo = 123 }";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.blocks.len(), 1);
        let block = &doc.blocks[0];
        assert_eq!(block.type_ident.text, "resource");
        assert_eq!(block.labels.len(), 5);
        assert_eq!(block.labels[0].kind, TokenKind::Whitespace);
        assert_eq!(block.labels[1].kind, TokenKind::String);
        assert_eq!(block.labels[1].text, "\"aws_instance\"");
        assert_eq!(block.labels[2].kind, TokenKind::Whitespace);
        assert_eq!(block.labels[3].kind, TokenKind::Ident);
        assert_eq!(block.labels[3].text, "web");
        assert_eq!(block.labels[4].kind, TokenKind::Whitespace);
        assert_eq!(block.body.attributes.len(), 1);
        assert_eq!(block.body.attributes[0].name.text, "foo");
    }
    #[test]
    fn test_cst_parser_block_nested() {
        let input = "block { nested { a = b } }";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.blocks.len(), 1);
        let block = &doc.blocks[0];
        assert_eq!(block.body.blocks.len(), 1);
        assert_eq!(block.body.blocks[0].type_ident.text, "nested");
        assert_eq!(block.body.blocks[0].body.attributes.len(), 1);
    }
    #[test]
    fn test_cst_parser_bracket_paren() {
        let input = "foo = [ ( 1 ) ]\n";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.attributes[0].expr_tokens.len(), 10);
        let input_unmatched = "foo = ) ]\n";
        let parser = CstParser::new(input_unmatched);
        let doc_unmatched = parser.parse().unwrap();
        assert_eq!(doc_unmatched.attributes[0].expr_tokens.len(), 4);
    }
    #[test]
    fn test_cst_parser_block_errors() {
        let err1 = CstParser::new("block").parse().err();
        assert_eq!(err1, Some(HclError::Parse("Unexpected EOF".to_string())));
        let err2 = CstParser::new("block 123").parse().err();
        assert_eq!(
            err2,
            Some(HclError::Parse(
                "Unexpected token after identifier: Number".to_string()
            ))
        );
        let err3 = CstParser::new("block label").parse().err();
        assert_eq!(
            err3,
            Some(HclError::Parse(
                "Unexpected EOF after block labels".to_string()
            ))
        );
        let err4 = CstParser::new("block label =").parse().err();
        assert_eq!(
            err4,
            Some(HclError::Parse(
                "Expected '{' after block labels, found Assign".to_string()
            ))
        );
        let err5 = CstParser::new("block {").parse().err();
        assert_eq!(err5, Some(HclError::Parse("Expected '}'".to_string())));
        let err6 = CstParser::new("block { = }").parse().err();
        assert_eq!(
            err6,
            Some(HclError::Parse("Unexpected token: Assign".to_string()))
        );
    }
    #[test]
    fn test_cst_parser_lexer_error() {
        let input = "`";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.attributes.len(), 0);
    }
    #[test]
    fn test_cst_parser_lexer_error_advance() {
        let input = "foo = `";
        let parser = CstParser::new(input);
        assert!(parser.parse().is_ok());
    }
    #[test]
    fn test_cst_parser_brace_nesting() {
        let input = "foo = { a = 1 }\n";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.attributes[0].expr_tokens.len(), 10);
    }
    #[test]
    fn test_cst_parser_brace_mismatch() {
        let input = "foo = 1 }\n";
        let parser = CstParser::new(input);
        assert!(parser.parse().is_err());
    }
    #[test]
    fn test_cst_parser_inline_comment() {
        let input = "foo = 1 // comment\n";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.attributes[0].expr_tokens.len(), 3);
        assert_eq!(
            doc.attributes[0].trailing.tokens[0].kind,
            TokenKind::InlineComment
        );
    }
    #[test]
    fn test_cst_parser_inline_comment_nested() {
        let input = "foo = { 1 // comment \n}\n";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert!(
            doc.attributes[0]
                .expr_tokens
                .iter()
                .any(|t| t.kind == TokenKind::InlineComment)
        );
    }
    #[test]
    fn test_cst_parser_advance_expect_eof() {
        let mut parser = CstParser::new("");
        assert!(parser.pop_formatting().is_none());
        assert_eq!(parser.consume_formatting().tokens.len(), 0);
        assert!(parser.advance_expect("test eof").is_err());
        assert_eq!(parser.consume_current().kind, TokenKind::Whitespace);
        let mut parser_ident = CstParser::new("foo");
        assert!(parser_ident.pop_formatting().is_none());
        let mut parser_ws = CstParser::new(" ");
        assert_eq!(
            parser_ws.pop_formatting().map(|t| t.kind),
            Some(TokenKind::Whitespace)
        );
        let mut parser_nl = CstParser::new("\n");
        assert_eq!(
            parser_nl.pop_formatting().map(|t| t.kind),
            Some(TokenKind::Newline)
        );
        let mut parser_com = CstParser::new("/* comment */");
        assert_eq!(
            parser_com.pop_formatting().map(|t| t.kind),
            Some(TokenKind::Comment)
        );
        let mut parser_in = CstParser::new("// inline\n");
        assert_eq!(
            parser_in.pop_formatting().map(|t| t.kind),
            Some(TokenKind::InlineComment)
        );
    }
    #[test]
    fn test_cst_parser_comments_and_formatting() {
        let input = "# top comment\n/* block comment */\nfoo = 1\n";
        let parser = CstParser::new(input);
        let doc = parser.parse().unwrap();
        assert_eq!(doc.attributes.len(), 1);
        assert!(
            doc.leading
                .tokens
                .iter()
                .any(|t| t.kind == TokenKind::Comment)
        );
    }
}
