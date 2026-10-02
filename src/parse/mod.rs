//! HCL Parsing implementation.

/// The main parser.
pub mod parser;

/// JSON parsing for HCL JSON profile.
pub mod json;

/// Type expression parsing.
pub mod type_expr;

/// Merging multiple parsed files.
pub mod merge;

/// Multi-file parser cache and file manager.
pub mod file_manager;

pub use file_manager::{FileManager, HclParser};
pub use merge::{
    MergeOptions, block_priority, merge_bodies, merge_bodies_with_options, merge_directory,
    merge_directory_with_options, merge_files,
};
pub use parser::{Parser, strip_heredoc_indentation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// Operator precedence
pub enum Precedence {
    /// Lowest precedence
    Lowest,
    /// Conditional precedence
    Conditional,
    /// Logical OR precedence
    Or,
    /// Logical AND precedence
    And,
    /// Equality precedence
    Equality,
    /// Relational precedence
    Relational,
    /// Additive precedence
    Additive,
    /// Multiplicative precedence
    Multiplicative,
    /// Unary precedence
    Unary,
    /// Function call precedence
    Call,
    /// Primary precedence
    Primary,
}

impl Precedence {
    #[must_use]
    /// Determine precedence from a token kind
    pub fn from_token(kind: &crate::lex::token::TokenKind) -> Self {
        use crate::lex::token::TokenKind;
        match kind {
            TokenKind::Question => Precedence::Conditional,
            TokenKind::Or => Precedence::Or,
            TokenKind::And => Precedence::And,
            TokenKind::Eq | TokenKind::NotEq => Precedence::Equality,
            TokenKind::Lt | TokenKind::Lte | TokenKind::Gt | TokenKind::Gte => {
                Precedence::Relational
            }
            TokenKind::Plus | TokenKind::Minus => Precedence::Additive,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Precedence::Multiplicative,
            TokenKind::OParen | TokenKind::OBrack | TokenKind::Dot => Precedence::Call,
            _ => Precedence::Lowest,
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
    use crate::lex::token::TokenKind;

    #[test]
    fn test_precedence_from_token() {
        assert_eq!(
            Precedence::from_token(&TokenKind::Question),
            Precedence::Conditional
        );
        assert_eq!(Precedence::from_token(&TokenKind::Or), Precedence::Or);
        assert_eq!(Precedence::from_token(&TokenKind::And), Precedence::And);
        assert_eq!(Precedence::from_token(&TokenKind::Eq), Precedence::Equality);
        assert_eq!(
            Precedence::from_token(&TokenKind::NotEq),
            Precedence::Equality
        );
        assert_eq!(
            Precedence::from_token(&TokenKind::Lt),
            Precedence::Relational
        );
        assert_eq!(
            Precedence::from_token(&TokenKind::Lte),
            Precedence::Relational
        );
        assert_eq!(
            Precedence::from_token(&TokenKind::Gt),
            Precedence::Relational
        );
        assert_eq!(
            Precedence::from_token(&TokenKind::Gte),
            Precedence::Relational
        );
        assert_eq!(
            Precedence::from_token(&TokenKind::Plus),
            Precedence::Additive
        );
        assert_eq!(
            Precedence::from_token(&TokenKind::Minus),
            Precedence::Additive
        );
        assert_eq!(
            Precedence::from_token(&TokenKind::Star),
            Precedence::Multiplicative
        );
        assert_eq!(
            Precedence::from_token(&TokenKind::Slash),
            Precedence::Multiplicative
        );
        assert_eq!(
            Precedence::from_token(&TokenKind::Percent),
            Precedence::Multiplicative
        );
        assert_eq!(Precedence::from_token(&TokenKind::OParen), Precedence::Call);
        assert_eq!(Precedence::from_token(&TokenKind::OBrack), Precedence::Call);
        assert_eq!(Precedence::from_token(&TokenKind::Dot), Precedence::Call);

        // Some unrelated token
        assert_eq!(
            Precedence::from_token(&TokenKind::Ident),
            Precedence::Lowest
        );
    }
}
