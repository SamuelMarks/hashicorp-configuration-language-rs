//! Tokens produced by the HCL lexer.

use crate::span::Span;
use derive_more::derive::Display;
use logos::Logos;

/// Strongly typed token kinds used in HCL.
#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash, Display)]
pub enum TokenKind {
    /// `\n`
    #[token("\n")]
    #[display("newline")]
    Newline,

    /// Spaces, tabs, carriage returns, form feeds.
    #[regex(r"[ \t\r\f]+")]
    #[display("whitespace")]
    Whitespace,

    /// Multi-line comment block.
    #[regex(r"(?s:/\*.*?\*/)")]
    #[display("comment")]
    Comment,

    // ---- Punctuation ----
    /// `{`
    #[token("{")]
    #[display("{{")]
    OBrace,
    /// `}`
    #[token("}")]
    #[display("}}")]
    CBrace,
    /// `[`
    #[token("[")]
    #[display("[")]
    OBrack,
    /// `]`
    #[token("]")]
    #[display("]")]
    CBrack,
    /// `(`
    #[token("(")]
    #[display("(")]
    OParen,
    /// `)`
    #[token(")")]
    #[display(")")]
    CParen,

    // ---- Operators ----
    /// `=`
    #[token("=")]
    #[display("=")]
    Assign,
    /// `::`
    ///
    /// Double colon delimiter used for namespaced function and symbol references,
    /// such as `provider::aws::arn_parse()`.
    #[token("::")]
    #[display("::")]
    ColonColon,
    /// `:`
    #[token(":")]
    #[display(":")]
    Colon,
    /// `,`
    #[token(",")]
    #[display(",")]
    Comma,
    /// `.`
    #[token(".")]
    #[display(".")]
    Dot,
    /// `=>`
    #[token("=>")]
    #[display("=>")]
    FatArrow,
    /// `...`
    #[token("...")]
    #[display("...")]
    Ellipsis,

    // ---- Math ----
    /// `+`
    #[token("+")]
    #[display("+")]
    Plus,
    /// `-`
    #[token("-")]
    #[display("-")]
    Minus,
    /// `*`
    #[token("*")]
    #[display("*")]
    Star,
    /// `/`
    #[token("/")]
    #[display("/")]
    Slash,
    /// `%`
    #[token("%")]
    #[display("%")]
    Percent,

    // ---- Logic/Comparison ----
    /// `==`
    #[token("==")]
    #[display("==")]
    Eq,
    /// `!=`
    #[token("!=")]
    #[display("!=")]
    NotEq,
    /// `<`
    #[token("<")]
    #[display("<")]
    Lt,
    /// `<=`
    #[token("<=")]
    #[display("<=")]
    Lte,
    /// `>`
    #[token(">")]
    #[display(">")]
    Gt,
    /// `>=`
    #[token(">=")]
    #[display(">=")]
    Gte,
    /// `&&`
    #[token("&&")]
    #[display("&&")]
    And,
    /// `||`
    #[token("||")]
    #[display("||")]
    Or,
    /// `!`
    #[token("!")]
    #[display("!")]
    Not,
    /// `?`
    #[token("?")]
    #[display("?")]
    Question,

    // ---- Literals ----
    /// An identifier (e.g. `var`, `module`, Unicode supported).
    #[regex(r"[\p{XID_Start}_][\p{XID_Continue}-]*")]
    #[display("identifier")]
    Ident,

    /// A number literal.
    #[regex(r"[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?")]
    #[display("number")]
    Number,

    /// A string literal.
    #[regex(r#""([^"\\]|\\["\\/bfnrt]|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8})*""#)]
    #[display("string")]
    String,

    /// Single line comment `#` or `//`.
    #[regex(r"(#|//)[^\n]*", allow_greedy = true)]
    #[display("comment")]
    InlineComment,

    // ---- Heredocs ----
    /// A standard or indented Heredoc. Handled uniquely by the lexer outer loop.
    #[display("heredoc")]
    Heredoc,
}

/// A token bound to a specific span in the source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The kind of token.
    pub kind: TokenKind,
    /// The text content of the token.
    pub text: String,
    /// The exact source span.
    pub span: Span,
}

impl Token {
    /// Create a new token.
    #[must_use]
    pub fn new(kind: TokenKind, text: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            text: text.into(),
            span,
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
    use logos::Logos;

    #[test]
    fn test_token_creation() {
        let span = Span::new(0, 1, 1, 1, 1, 2);
        let t = Token::new(TokenKind::Ident, "foo", span.clone());
        assert_eq!(t.kind, TokenKind::Ident);
        assert!(t.text.contains("foo"));
        assert_eq!(t.span, span);
    }

    #[test]
    fn test_token_kinds() {
        let mut lex = TokenKind::lexer("{ } [ ] ( )");
        assert_eq!(lex.next(), Some(Ok(TokenKind::OBrace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::CBrace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::OBrack)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::CBrack)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::OParen)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::CParen)));

        let mut lex = TokenKind::lexer("= :: : , . => ...");
        assert_eq!(lex.next(), Some(Ok(TokenKind::Assign)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::ColonColon)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Colon)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Comma)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Dot)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::FatArrow)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Ellipsis)));

        let mut lex = TokenKind::lexer("+ - * / %");
        assert_eq!(lex.next(), Some(Ok(TokenKind::Plus)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Minus)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Star)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Slash)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Percent)));

        let mut lex = TokenKind::lexer("== != < <= > >= && || ! ?");
        assert_eq!(lex.next(), Some(Ok(TokenKind::Eq)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::NotEq)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Lt)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Lte)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Gt)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Gte)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::And)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Or)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Not)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Question)));

        let mut lex = TokenKind::lexer("var_name 123.45e-2 \"str\\n\"");
        assert_eq!(lex.next(), Some(Ok(TokenKind::Ident)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Number)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::String)));

        // Test unicode identifiers
        let mut lex = TokenKind::lexer("こんにちは");
        assert_eq!(lex.next(), Some(Ok(TokenKind::Ident)));

        // Test whitespace and comment rules
        let mut lex =
            TokenKind::lexer("  \n # some comment \n // another comment \n /* block */ var");
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Newline)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::InlineComment)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Newline)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::InlineComment)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Newline)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Comment)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Whitespace)));
        assert_eq!(lex.next(), Some(Ok(TokenKind::Ident)));

        // Display trait tests for derive_more
        assert!(TokenKind::OBrace.to_string().contains('{'));
        assert!(TokenKind::Ident.to_string().contains("identifier"));
        assert!(TokenKind::Heredoc.to_string().contains("heredoc"));
    }
}
