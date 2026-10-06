//! Splicing and source extraction utilities for HCL tokens.
use crate::lex::token::Token;
use crate::span::Span;
/// Extracts the raw source string for a given `Span` from a slice of `Token`s.
///
/// This concatenates the textual representations of all tokens that fall completely
/// or partially within the provided `Span`.
///
/// # Examples
/// ```
/// use hashicorp_configuration_language_rs::lex::token::{Token, TokenKind};
/// use hashicorp_configuration_language_rs::span::Span;
/// use hashicorp_configuration_language_rs::cst::splicing::extract_source;
///
/// let tokens = vec![
///     Token::new(TokenKind::Ident, "foo", Span::new(0, 3, 1, 1, 1, 4)),
///     Token::new(TokenKind::Whitespace, " ", Span::new(3, 4, 1, 4, 1, 5)),
///     Token::new(TokenKind::Assign, "=", Span::new(4, 5, 1, 5, 1, 6)),
/// ];
/// let span = Span::new(0, 3, 1, 1, 1, 4);
/// assert_eq!(extract_source(&tokens, &span), "foo");
/// ```
#[must_use]
pub fn extract_source(tokens: &[Token], span: &Span) -> String {
    let mut out = String::new();
    for token in tokens {
        if token.span.start_byte < span.end_byte && token.span.end_byte > span.start_byte {
            out.push_str(&token.text);
        }
    }
    out
}
/// Splices a slice of tokens, replacing the tokens that intersect with `target_span`
/// with the provided `new_tokens`.
///
/// Returns a new vector of tokens.
///
/// # Examples
/// ```
/// use hashicorp_configuration_language_rs::lex::token::{Token, TokenKind};
/// use hashicorp_configuration_language_rs::span::Span;
/// use hashicorp_configuration_language_rs::cst::splicing::splice_tokens;
///
/// let tokens = vec![
///     Token::new(TokenKind::Ident, "old", Span::new(0, 3, 1, 1, 1, 4)),
/// ];
/// let target_span = Span::new(0, 3, 1, 1, 1, 4);
/// let new_tokens = vec![
///     Token::new(TokenKind::Ident, "new", Span::new(0, 3, 1, 1, 1, 4)),
/// ];
/// let spliced = splice_tokens(&tokens, &target_span, new_tokens);
/// assert_eq!(spliced[0].text, "new");
/// ```
#[must_use]
pub fn splice_tokens(tokens: &[Token], target_span: &Span, new_tokens: Vec<Token>) -> Vec<Token> {
    let mut out = Vec::new();
    let mut inserted = false;
    for token in tokens {
        if token.span.start_byte < target_span.end_byte {
            if token.span.end_byte > target_span.start_byte {
                if !inserted {
                    out.extend(new_tokens.clone());
                    inserted = true;
                }
            } else {
                out.push(token.clone());
            }
        } else {
            out.push(token.clone());
        }
    }
    if !inserted {
        out.extend(new_tokens);
    }
    out
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
    fn test_extract_source() {
        let t1 = Token::new(TokenKind::Ident, "foo", Span::new(0, 3, 1, 1, 1, 4));
        let t2 = Token::new(TokenKind::Whitespace, " ", Span::new(3, 4, 1, 4, 1, 5));
        let t3 = Token::new(TokenKind::Assign, "=", Span::new(4, 5, 1, 5, 1, 6));
        let t4 = Token::new(TokenKind::Whitespace, " ", Span::new(5, 6, 1, 6, 1, 7));
        let t5 = Token::new(TokenKind::Number, "42", Span::new(6, 8, 1, 7, 1, 9));
        let tokens = vec![t1.clone(), t2.clone(), t3.clone(), t4.clone(), t5.clone()];
        let span_all = Span::new(0, 8, 1, 1, 1, 9);
        assert_eq!(extract_source(&tokens, &span_all), "foo = 42");
        let span_val = Span::new(6, 8, 1, 7, 1, 9);
        assert_eq!(extract_source(&tokens, &span_val), "42");
        let span_ident = Span::new(0, 3, 1, 1, 1, 4);
        assert_eq!(extract_source(&tokens, &span_ident), "foo");
    }
    #[test]
    fn test_splice_tokens() {
        let t1 = Token::new(TokenKind::Ident, "foo", Span::new(0, 3, 1, 1, 1, 4));
        let t2 = Token::new(TokenKind::Whitespace, " ", Span::new(3, 4, 1, 4, 1, 5));
        let t3 = Token::new(TokenKind::Assign, "=", Span::new(4, 5, 1, 5, 1, 6));
        let t4 = Token::new(TokenKind::Whitespace, " ", Span::new(5, 6, 1, 6, 1, 7));
        let t5 = Token::new(TokenKind::Number, "42", Span::new(6, 8, 1, 7, 1, 9));
        let tokens = vec![t1, t2, t3, t4, t5];
        let target_span = Span::new(6, 8, 1, 7, 1, 9);
        let new_tokens = vec![Token::new(
            TokenKind::String,
            "\"hello\"",
            target_span.clone(),
        )];
        let spliced = splice_tokens(&tokens, &target_span, new_tokens.clone());
        assert_eq!(spliced.len(), 5);
        assert_eq!(spliced[4].text, "\"hello\"");
        let target_span_miss = Span::new(10, 12, 1, 11, 1, 13);
        let new_tokens_miss = vec![Token::new(
            TokenKind::String,
            "\"miss\"",
            target_span_miss.clone(),
        )];
        let spliced_miss = splice_tokens(&tokens, &target_span_miss, new_tokens_miss);
        assert_eq!(spliced_miss.len(), 6);
        assert_eq!(spliced_miss[5].text, "\"miss\"");
        let target_span_multi = Span::new(4, 8, 1, 5, 1, 9);
        let new_tokens_multi = vec![Token::new(
            TokenKind::String,
            "\"multi\"",
            target_span_multi.clone(),
        )];
        let spliced_multi = splice_tokens(&tokens, &target_span_multi, new_tokens_multi);
        assert_eq!(spliced_multi.len(), 3);
        assert_eq!(spliced_multi[2].text, "\"multi\"");
        let target_span_start = Span::new(0, 3, 1, 1, 1, 4);
        let new_tokens_start = vec![Token::new(
            TokenKind::Ident,
            "bar",
            target_span_start.clone(),
        )];
        let spliced_start = splice_tokens(&tokens, &target_span_start, new_tokens_start);
        assert_eq!(spliced_start.len(), 5);
        assert_eq!(spliced_start[0].text, "bar");
    }
}
