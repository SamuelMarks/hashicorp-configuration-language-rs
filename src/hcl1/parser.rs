//! Parser for legacy HCL 1.0 syntax.
//!
//! Parses HCL 1.0 documents, supporting legacy block assignments with `=` (`block "name" = { ... }`),
//! comma-less lists (`["a" "b"]`), and unquoted object keys.
use crate::error::HclError;
use crate::hcl1::token::{Hcl1Token, Hcl1TokenKind};
use crate::span::Span;
/// An AST expression in HCL 1.0.
#[derive(Debug, Clone, PartialEq)]
pub enum Hcl1Expression {
    /// A string literal, possibly containing `${...}` interpolations.
    String(String, Span),
    /// A numeric literal.
    Number(String, Span),
    /// A boolean literal.
    Bool(bool, Span),
    /// A null literal.
    Null(Span),
    /// A list of expressions.
    List(Vec<Hcl1Expression>, Span),
    /// A map or object constructor.
    Map(Vec<(String, Hcl1Expression)>, Span),
    /// A bare identifier reference.
    Variable(String, Span),
}
impl Hcl1Expression {
    /// Returns the source [`Span`] of this HCL 1.0 expression.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::String(_, s)
            | Self::Number(_, s)
            | Self::Bool(_, s)
            | Self::Null(s)
            | Self::List(_, s)
            | Self::Map(_, s)
            | Self::Variable(_, s) => s.clone(),
        }
    }
}
/// An attribute assignment in HCL 1.0 (`key = value` or `key: value`).
#[derive(Debug, Clone, PartialEq)]
pub struct Hcl1Attribute {
    /// The attribute name.
    pub name: String,
    /// The attribute value expression.
    pub expr: Hcl1Expression,
    /// The source span of the attribute.
    pub span: Span,
}
/// A block definition in HCL 1.0 (`type "label1" "label2" { ... }` or `type "label1" = { ... }`).
#[derive(Debug, Clone, PartialEq)]
pub struct Hcl1Block {
    /// The block type identifier (e.g. `resource`, `variable`).
    pub block_type: String,
    /// The block labels.
    pub labels: Vec<String>,
    /// The block body attributes and nested blocks.
    pub body: Hcl1Body,
    /// Whether the block had an assignment equals sign (`=`) before the opening brace.
    pub has_equals_assign: bool,
    /// The source span of the block.
    pub span: Span,
}
/// An item in an HCL 1.0 body.
#[derive(Debug, Clone, PartialEq)]
pub enum Hcl1Item {
    /// An attribute assignment.
    Attribute(Hcl1Attribute),
    /// A block definition.
    Block(Hcl1Block),
}
/// A collection of items representing an HCL 1.0 body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hcl1Body {
    /// The items in the body.
    pub items: Vec<Hcl1Item>,
    /// The source span covering the body.
    pub span: Span,
}
/// Parser for HCL 1.0 syntax.
#[derive(Debug)]
pub struct Hcl1Parser {
    tokens: Vec<Hcl1Token>,
    cursor: usize,
}
impl Hcl1Parser {
    /// Creates a new [`Hcl1Parser`] from token stream.
    ///
    /// # Arguments
    /// * `tokens` - The tokens to parse.
    #[must_use]
    pub fn new(tokens: Vec<Hcl1Token>) -> Self {
        let meaningful: Vec<Hcl1Token> = tokens
            .into_iter()
            .filter(|t| t.kind != Hcl1TokenKind::Whitespace && t.kind != Hcl1TokenKind::Comment)
            .collect();
        Self {
            tokens: meaningful,
            cursor: 0,
        }
    }
    fn peek(&self) -> Option<&Hcl1Token> {
        self.tokens.get(self.cursor)
    }
    fn advance(&mut self) -> Option<Hcl1Token> {
        let tok = self.tokens.get(self.cursor)?.clone();
        self.cursor += 1;
        Some(tok)
    }
    fn skip_newlines(&mut self) {
        while let Some(tok) = self.peek() {
            if tok.kind == Hcl1TokenKind::Newline {
                self.advance();
            } else {
                break;
            }
        }
    }
    /// Parses the tokens into a top-level [`Hcl1Body`].
    ///
    /// # Errors
    /// Returns [`HclError::Parse`] if a syntax error is encountered.
    pub fn parse_body(&mut self) -> Result<Hcl1Body, HclError> {
        let mut items = Vec::new();
        let start_span = self
            .peek()
            .map_or(Span::new(0, 0, 1, 1, 1, 1), |t| t.span.clone());
        self.skip_newlines();
        while self.cursor < self.tokens.len() {
            if self.peek().map(|t| &t.kind) == Some(&Hcl1TokenKind::CBrace) {
                break;
            }
            let item = self.parse_item()?;
            items.push(item);
            self.skip_newlines();
        }
        let end_span = self
            .tokens
            .get(self.cursor.saturating_sub(1))
            .map_or(start_span.clone(), |t| t.span.clone());
        Ok(Hcl1Body {
            items,
            span: start_span.merge(&end_span),
        })
    }
    /// Parses a single item (attribute or block) from the token stream.
    ///
    /// # Errors
    /// Returns [`HclError::Parse`] if syntax is invalid or EOF is unexpectedly reached.
    pub fn parse_item(&mut self) -> Result<Hcl1Item, HclError> {
        self.skip_newlines();
        let first = self.advance().ok_or_else(|| {
            HclError::Parse("unexpected EOF while expecting identifier or block".to_string())
        })?;
        if first.kind != Hcl1TokenKind::Ident && first.kind != Hcl1TokenKind::String {
            return Err(HclError::Parse(format!(
                "expected identifier or string key, got {:?}",
                first.kind
            )));
        }
        let name = first.text.clone();
        let mut labels = Vec::new();
        while let Some(tok) = self.peek() {
            if tok.kind == Hcl1TokenKind::String || tok.kind == Hcl1TokenKind::Ident {
                labels.push(tok.text.clone());
                self.advance();
            } else {
                break;
            }
        }
        self.skip_newlines();
        let next_tok = self
            .peek()
            .ok_or_else(|| HclError::Parse(format!("unexpected EOF after identifier `{name}`")))?;
        match next_tok.kind {
            Hcl1TokenKind::Assign | Hcl1TokenKind::Colon => {
                self.advance();
                self.skip_newlines();
                if self.peek().map(|t| &t.kind) == Some(&Hcl1TokenKind::OBrace)
                    && !labels.is_empty()
                {
                    let block = self.parse_block_body(name, labels, true, &first.span)?;
                    return Ok(Hcl1Item::Block(block));
                }
                let expr = self.parse_expression()?;
                let span = first.span.merge(&expr.span());
                Ok(Hcl1Item::Attribute(Hcl1Attribute { name, expr, span }))
            }
            Hcl1TokenKind::OBrace => {
                let block = self.parse_block_body(name, labels, false, &first.span)?;
                Ok(Hcl1Item::Block(block))
            }
            _ => Err(HclError::Parse(format!(
                "expected '=', ':', or '{{' after `{name}`, got {:?}",
                next_tok.kind
            ))),
        }
    }
    fn parse_block_body(
        &mut self,
        block_type: String,
        labels: Vec<String>,
        has_equals_assign: bool,
        start_span: &Span,
    ) -> Result<Hcl1Block, HclError> {
        self.advance();
        let body = self.parse_body()?;
        self.skip_newlines();
        let close = self
            .advance()
            .ok_or_else(|| HclError::Parse("unclosed block, expected '}'".to_string()))?;
        let span = start_span.merge(&close.span);
        Ok(Hcl1Block {
            block_type,
            labels,
            body,
            has_equals_assign,
            span,
        })
    }
    /// Parses an HCL 1.0 expression.
    ///
    /// # Errors
    /// Returns [`HclError::Parse`] if expression parsing fails.
    pub fn parse_expression(&mut self) -> Result<Hcl1Expression, HclError> {
        self.skip_newlines();
        let tok = self.advance().ok_or_else(|| {
            HclError::Parse("unexpected EOF while expecting expression".to_string())
        })?;
        match tok.kind {
            Hcl1TokenKind::String | Hcl1TokenKind::Heredoc => {
                Ok(Hcl1Expression::String(tok.text, tok.span))
            }
            Hcl1TokenKind::Number => Ok(Hcl1Expression::Number(tok.text, tok.span)),
            Hcl1TokenKind::Bool => Ok(Hcl1Expression::Bool(tok.text == "true", tok.span)),
            Hcl1TokenKind::Null => Ok(Hcl1Expression::Null(tok.span)),
            Hcl1TokenKind::Ident => Ok(Hcl1Expression::Variable(tok.text, tok.span)),
            Hcl1TokenKind::OBrack => {
                let mut elements = Vec::new();
                let start_span = tok.span;
                let close_span = loop {
                    self.skip_newlines();
                    match self.peek() {
                        Some(t) if t.kind == Hcl1TokenKind::CBrack => {
                            let s = t.span.clone();
                            self.advance();
                            break s;
                        }
                        None => {
                            return Err(HclError::Parse("unclosed list, expected ']'".to_string()));
                        }
                        _ => {}
                    }
                    elements.push(self.parse_expression()?);
                    self.skip_newlines();
                    if self.peek().map(|t| &t.kind) == Some(&Hcl1TokenKind::Comma) {
                        self.advance();
                    }
                };
                let span = start_span.merge(&close_span);
                Ok(Hcl1Expression::List(elements, span))
            }
            Hcl1TokenKind::OBrace => {
                let mut pairs = Vec::new();
                let start_span = tok.span;
                let close_span = loop {
                    self.skip_newlines();
                    match self.peek() {
                        Some(t) if t.kind == Hcl1TokenKind::CBrace => {
                            let s = t.span.clone();
                            self.advance();
                            break s;
                        }
                        _ => {}
                    }
                    let k_tok = self
                        .advance()
                        .ok_or_else(|| HclError::Parse("unclosed map, expected '}'".to_string()))?;
                    let key = k_tok.text;
                    self.skip_newlines();
                    if let Some(eq) = self.peek() {
                        if eq.kind == Hcl1TokenKind::Assign || eq.kind == Hcl1TokenKind::Colon {
                            self.advance();
                        }
                    }
                    let val = self.parse_expression()?;
                    pairs.push((key, val));
                    self.skip_newlines();
                    if self.peek().map(|t| &t.kind) == Some(&Hcl1TokenKind::Comma) {
                        self.advance();
                    }
                };
                let span = start_span.merge(&close_span);
                Ok(Hcl1Expression::Map(pairs, span))
            }
            _ => Err(HclError::Parse(format!(
                "unexpected token in expression: {:?}",
                tok.kind
            ))),
        }
    }
}
