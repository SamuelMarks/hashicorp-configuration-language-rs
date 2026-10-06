//! Token model for legacy HCL 1.0 syntax.
//!
//! Defines token kinds and representations conforming to legacy HCL 1.0 lexing rules.
use crate::span::Span;
/// A lexical token kind in HCL 1.0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hcl1TokenKind {
    /// An identifier or keyword (e.g. `resource`, `variable`, `true`).
    Ident,
    /// A quoted string literal.
    String,
    /// A numeric literal.
    Number,
    /// A boolean literal (`true` or `false`).
    Bool,
    /// A null literal (`null`).
    Null,
    /// An assignment operator (`=`).
    Assign,
    /// A colon (`:`).
    Colon,
    /// A comma (`,`).
    Comma,
    /// A dot (`.`).
    Dot,
    /// An opening brace (`{`).
    OBrace,
    /// A closing brace (`}`).
    CBrace,
    /// An opening bracket (`[`).
    OBrack,
    /// A closing bracket (`]`).
    CBrack,
    /// An opening parenthesis (`(`).
    OParen,
    /// A closing parenthesis (`)`).
    CParen,
    /// A heredoc string literal (`<<EOF ... EOF`).
    Heredoc,
    /// A comment (`#`, `//`, or `/* ... */`).
    Comment,
    /// Whitespace trivia (spaces, tabs).
    Whitespace,
    /// A newline character (`\\n` or `\\r\\n`).
    Newline,
}
/// A spanned token in legacy HCL 1.0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hcl1Token {
    /// The token kind.
    pub kind: Hcl1TokenKind,
    /// The literal text of the token.
    pub text: String,
    /// The source span of the token.
    pub span: Span,
}
impl Hcl1Token {
    /// Creates a new [`Hcl1Token`].
    ///
    /// # Arguments
    /// * `kind` - The token kind.
    /// * `text` - The token text.
    /// * `span` - The source span.
    #[must_use]
    pub fn new(kind: Hcl1TokenKind, text: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            text: text.into(),
            span,
        }
    }
}
