#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]
use crate::cst::splicing::{extract_source, splice_tokens};
use crate::lex::token::{Token, TokenKind};
use crate::span::Span;
#[test]
fn test_extract_source() {
    let t1 = Token::new(TokenKind::Ident, "foo", Span::new(0, 3, 1, 1, 1, 4));
    let t2 = Token::new(TokenKind::Whitespace, " ", Span::new(3, 4, 1, 4, 1, 5));
    let t3 = Token::new(TokenKind::Assign, "=", Span::new(4, 5, 1, 5, 1, 6));
    let t4 = Token::new(TokenKind::Whitespace, " ", Span::new(5, 6, 1, 6, 1, 7));
    let t5 = Token::new(TokenKind::Number, "42", Span::new(6, 8, 1, 7, 1, 9));
    let tokens = vec![t1.clone(), t2.clone(), t3.clone(), t4.clone(), t5.clone()];
    let span_all = Span::new(0, 8, 1, 1, 1, 9);
    assert_eq!(extract_source(& tokens, & span_all), "foo = 42");
    let span_val = Span::new(6, 8, 1, 7, 1, 9);
    assert_eq!(extract_source(& tokens, & span_val), "42");
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
    let new_tokens = vec![Token::new(TokenKind::String, "\"hello\"", target_span),];
    let spliced = splice_tokens(&tokens, &target_span, new_tokens);
    assert_eq!(spliced.len(), 5);
    assert_eq!(spliced[4].text, "\"hello\"");
}
#[test]
fn test_tokens_for_namespaced_function_roundtrip() {
    use crate::ast::expr::{Expression, FuncCall, NamespacedIdent};
    use crate::cst::builder::tokens_for_expression;
    let span = Span::new(0, 30, 1, 1, 1, 31);
    let fc = Expression::FuncCall(
        Box::new(FuncCall {
            name: NamespacedIdent::parse("provider::aws::arn_parse", span.clone()),
            args: vec![Expression::Variable("my_arn".to_string(), span.clone())],
            expand_final: false,
        }),
        span,
    );
    let toks = tokens_for_expression(&fc);
    let text: String = toks.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(text, "provider::aws::arn_parse(my_arn)");
    let target = Span::new(0, 10, 1, 1, 1, 11);
    let spliced = splice_tokens(&toks, &target, vec![]);
    assert!(! spliced.is_empty());
}
