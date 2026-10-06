//! Parsing for HCL Type Expressions.
//!
//! Provides the ability to parse strings and token streams into `TypeExpr` AST nodes.
use crate::ast::expr::Expression;
use crate::ast::type_expr::{CollectionType, ObjectAttrType, TypeExpr};
use crate::error::HclError;
use crate::lex::lexer::Lexer;
use crate::lex::token::{Token, TokenKind};
use crate::span::Span;
use crate::types::ty::Type;
use std::collections::HashMap;
/// Parser specifically for type expressions.
pub struct TypeExprParser<'a> {
    lexer: Lexer<'a>,
    current: Option<Token>,
    next_tok: Option<Token>,
}
impl<'a> TypeExprParser<'a> {
    /// Create a new `TypeExprParser` for the given input string.
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        let lexer = Lexer::new(input);
        let mut parser = Self {
            lexer,
            current: None,
            next_tok: None,
        };
        parser.advance();
        parser.advance();
        parser
    }
    fn advance(&mut self) {
        self.current = self.next_tok.take();
        loop {
            let tok_result = self.lexer.next();
            match tok_result {
                None => {
                    self.next_tok = None;
                    break;
                }
                Some(Ok(tok)) => {
                    if tok.kind == TokenKind::Whitespace
                        || tok.kind == TokenKind::Comment
                        || tok.kind == TokenKind::InlineComment
                        || tok.kind == TokenKind::Newline
                    {
                    } else {
                        self.next_tok = Some(tok);
                        break;
                    }
                }
                Some(Err(_err)) => {}
            }
        }
    }
    /// Parse a type expression from the input.
    ///
    /// # Errors
    /// Returns an `HclError::Parse` if the input is not a valid type expression.
    pub fn parse(&mut self) -> Result<TypeExpr, HclError> {
        let expr = self.parse_type_expr()?;
        if self.current.is_some() {
            return Err(HclError::Parse(
                "Unexpected tokens after type expression".to_string(),
            ));
        }
        Ok(expr)
    }
    fn parse_type_expr(&mut self) -> Result<TypeExpr, HclError> {
        let tok = self
            .current
            .as_ref()
            .ok_or_else(|| HclError::Parse("Unexpected EOF".to_string()))?;
        let span = tok.span.clone();
        if tok.kind == TokenKind::Ident {
            let text = tok.text.clone();
            self.advance();
            match text.as_str() {
                "string" => Ok(TypeExpr::Primitive(Type::String, span)),
                "number" => Ok(TypeExpr::Primitive(Type::Number, span)),
                "bool" => Ok(TypeExpr::Primitive(Type::Bool, span)),
                "any" => Ok(TypeExpr::Primitive(Type::Dynamic, span)),
                "list" => self.parse_collection(CollectionType::List, span),
                "set" => self.parse_collection(CollectionType::Set, span),
                "map" => self.parse_collection(CollectionType::Map, span),
                "object" => self.parse_object(span),
                "tuple" => self.parse_tuple(span),
                "optional" => Err(HclError::Parse(
                    "The 'optional' modifier is only valid for object attributes".to_string(),
                )),
                _ => Err(HclError::Parse(format!("Unknown type: {text}"))),
            }
        } else {
            Err(HclError::Parse("Expected type identifier".to_string()))
        }
    }
    fn parse_collection(
        &mut self,
        coll_type: CollectionType,
        mut span: Span,
    ) -> Result<TypeExpr, HclError> {
        self.expect(&TokenKind::OParen)?;
        let inner = self.parse_type_expr()?;
        let close_span = self.expect(&TokenKind::CParen)?;
        span.end_line = close_span.end_line;
        span.end_col = close_span.end_col;
        span.end_byte = close_span.end_byte;
        Ok(TypeExpr::Collection(coll_type, Box::new(inner), span))
    }
    fn parse_object(&mut self, mut span: Span) -> Result<TypeExpr, HclError> {
        self.expect(&TokenKind::OParen)?;
        self.expect(&TokenKind::OBrace)?;
        let mut attrs = HashMap::new();
        while let Some(tok) = &self.current {
            if tok.kind == TokenKind::CBrace {
                break;
            }
            if tok.kind != TokenKind::Ident {
                return Err(HclError::Parse("Expected attribute name".to_string()));
            }
            let name = tok.text.clone();
            self.advance();
            let next_tok = self.current.as_ref().ok_or_else(|| {
                HclError::Parse("Expected '=' or ':' after attribute name".to_string())
            })?;
            if next_tok.kind != TokenKind::Assign && next_tok.kind != TokenKind::Colon {
                return Err(HclError::Parse(
                    "Expected '=' or ':' after attribute name".to_string(),
                ));
            }
            self.advance();
            let attr_type = self.parse_object_attr_type()?;
            attrs.insert(name, attr_type);
            if let Some(next) = &self.current {
                match next.kind {
                    TokenKind::Comma => {
                        self.advance();
                    }
                    TokenKind::CBrace => {}
                    _ => return Err(HclError::Parse("Expected ',' or '}'".to_string())),
                }
            }
        }
        self.expect(&TokenKind::CBrace)?;
        let close_span = self.expect(&TokenKind::CParen)?;
        span.end_line = close_span.end_line;
        span.end_col = close_span.end_col;
        span.end_byte = close_span.end_byte;
        Ok(TypeExpr::Object(attrs, span))
    }
    fn parse_object_attr_type(&mut self) -> Result<ObjectAttrType, HclError> {
        if let Some(tok) = &self.current {
            if tok.kind == TokenKind::Ident && tok.text == "optional" {
                self.advance();
                self.expect(&TokenKind::OParen)?;
                let inner_ty = self.parse_type_expr()?;
                let default_val = if let Some(t) = &self.current {
                    if t.kind == TokenKind::Comma {
                        self.advance();
                        Some(self.parse_default_expr()?)
                    } else {
                        None
                    }
                } else {
                    None
                };
                self.expect(&TokenKind::CParen)?;
                return Ok(ObjectAttrType::optional(inner_ty, default_val));
            }
        }
        let ty = self.parse_type_expr()?;
        Ok(ObjectAttrType::required(ty))
    }
    fn parse_default_expr(&mut self) -> Result<Expression, HclError> {
        let tok = self
            .current
            .as_ref()
            .ok_or_else(|| HclError::Parse("Unexpected EOF in default expression".to_string()))?;
        let span = tok.span.clone();
        match tok.kind {
            TokenKind::Number => {
                let n = tok
                    .text
                    .parse()
                    .unwrap_or_else(|_| crate::number::Number(bigdecimal::BigDecimal::from(0)));
                self.advance();
                Ok(Expression::Number(n, span))
            }
            TokenKind::String => {
                let s = tok.text.clone();
                self.advance();
                let clean = if s.starts_with('"') {
                    if s.ends_with('"') {
                        if s.len() >= 2 {
                            s[1..s.len() - 1].to_string()
                        } else {
                            s
                        }
                    } else {
                        s
                    }
                } else {
                    s
                };
                Ok(Expression::String(clean, span))
            }
            TokenKind::Ident => {
                let text = tok.text.clone();
                self.advance();
                match text.as_str() {
                    "true" => Ok(Expression::Bool(true, span)),
                    "false" => Ok(Expression::Bool(false, span)),
                    "null" => Ok(Expression::Null(span)),
                    _ => Ok(Expression::Variable(text, span)),
                }
            }
            TokenKind::Minus => {
                self.advance();
                let inner = self.parse_default_expr()?;
                if let Expression::Number(n, sp) = inner {
                    Ok(Expression::Number(-n, sp))
                } else {
                    Err(HclError::Parse(
                        "Expected number after '-' in default expression".to_string(),
                    ))
                }
            }
            TokenKind::OBrack => {
                self.advance();
                let mut elems = Vec::new();
                while let Some(t) = &self.current {
                    if t.kind == TokenKind::CBrack {
                        break;
                    }
                    let el = self.parse_default_expr()?;
                    elems.push(el);
                    if let Some(next) = &self.current {
                        if next.kind == TokenKind::Comma {
                            self.advance();
                        } else if next.kind != TokenKind::CBrack {
                            return Err(HclError::Parse(
                                "Expected ',' or ']' in tuple default".to_string(),
                            ));
                        }
                    }
                }
                let close_span = if let Some(tok) = self.current.take() {
                    let s = tok.span;
                    self.advance();
                    s
                } else {
                    return Err(HclError::Parse("Expected ']' in tuple default".to_string()));
                };
                let tuple_span = Span::new(
                    span.start_byte,
                    close_span.end_byte,
                    span.start_line,
                    span.start_col,
                    close_span.end_line,
                    close_span.end_col,
                );
                Ok(Expression::Tuple(elems, tuple_span))
            }
            TokenKind::OBrace => {
                self.advance();
                let mut entries = Vec::new();
                while let Some(t) = &self.current {
                    if t.kind == TokenKind::CBrace {
                        break;
                    }
                    let key = self.parse_default_expr()?;
                    let sep = self.current.as_ref().ok_or_else(|| {
                        HclError::Parse("Expected '=' or ':' in object default".to_string())
                    })?;
                    if sep.kind != TokenKind::Assign && sep.kind != TokenKind::Colon {
                        return Err(HclError::Parse(
                            "Expected '=' or ':' in object default".to_string(),
                        ));
                    }
                    self.advance();
                    let val = self.parse_default_expr()?;
                    entries.push((key, val));
                    if let Some(next) = &self.current {
                        if next.kind == TokenKind::Comma {
                            self.advance();
                        } else if next.kind != TokenKind::CBrace {
                            return Err(HclError::Parse(
                                "Expected ',' or '}' in object default".to_string(),
                            ));
                        }
                    }
                }
                let close_span = if let Some(tok) = self.current.take() {
                    let s = tok.span;
                    self.advance();
                    s
                } else {
                    return Err(HclError::Parse(
                        "Expected '}' in object default".to_string(),
                    ));
                };
                let obj_span = Span::new(
                    span.start_byte,
                    close_span.end_byte,
                    span.start_line,
                    span.start_col,
                    close_span.end_line,
                    close_span.end_col,
                );
                Ok(Expression::Object(entries, obj_span))
            }
            _ => Err(HclError::Parse(format!(
                "Unsupported default expression token: {:?}",
                tok.kind
            ))),
        }
    }
    fn parse_tuple(&mut self, mut span: Span) -> Result<TypeExpr, HclError> {
        self.expect(&TokenKind::OParen)?;
        self.expect(&TokenKind::OBrack)?;
        let mut elems = Vec::new();
        while let Some(tok) = &self.current {
            if tok.kind == TokenKind::CBrack {
                break;
            }
            let elem_type = self.parse_type_expr()?;
            elems.push(elem_type);
            if let Some(next) = &self.current {
                match next.kind {
                    TokenKind::Comma => {
                        self.advance();
                    }
                    TokenKind::CBrack => {}
                    _ => return Err(HclError::Parse("Expected ',' or ']'".to_string())),
                }
            }
        }
        self.expect(&TokenKind::CBrack)?;
        let close_span = self.expect(&TokenKind::CParen)?;
        span.end_line = close_span.end_line;
        span.end_col = close_span.end_col;
        span.end_byte = close_span.end_byte;
        Ok(TypeExpr::Tuple(elems, span))
    }
    fn expect(&mut self, kind: &TokenKind) -> Result<Span, HclError> {
        if let Some(tok) = &self.current {
            if tok.kind == *kind {
                let span = tok.span.clone();
                self.advance();
                Ok(span)
            } else {
                Err(HclError::Parse(format!(
                    "Expected {kind:?} but got {:?}",
                    tok.kind
                )))
            }
        } else {
            Err(HclError::Parse(format!("Expected {kind:?} but got EOF")))
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
    #[test]
    fn test_parse_primitive() {
        let mut parser = TypeExprParser::new("string");
        let expr = parser.parse().unwrap_or(TypeExpr::Primitive(
            Type::Dynamic,
            Span::new(0, 0, 1, 1, 1, 1),
        ));
        assert_eq!(
            expr,
            TypeExpr::Primitive(Type::String, Span::new(0, 6, 1, 1, 1, 7))
        );
        let mut parser = TypeExprParser::new("number");
        let expr = parser.parse().unwrap_or(TypeExpr::Primitive(
            Type::Dynamic,
            Span::new(0, 0, 1, 1, 1, 1),
        ));
        assert_eq!(
            expr,
            TypeExpr::Primitive(Type::Number, Span::new(0, 6, 1, 1, 1, 7))
        );
        let mut parser = TypeExprParser::new("bool");
        let expr = parser.parse().unwrap_or(TypeExpr::Primitive(
            Type::Dynamic,
            Span::new(0, 0, 1, 1, 1, 1),
        ));
        assert_eq!(
            expr,
            TypeExpr::Primitive(Type::Bool, Span::new(0, 4, 1, 1, 1, 5))
        );
        let mut parser = TypeExprParser::new("any");
        let expr = parser.parse().unwrap_or(TypeExpr::Primitive(
            Type::String,
            Span::new(0, 0, 1, 1, 1, 1),
        ));
        assert_eq!(
            expr,
            TypeExpr::Primitive(Type::Dynamic, Span::new(0, 3, 1, 1, 1, 4))
        );
    }
    #[test]
    fn test_parse_collection() {
        let mut parser = TypeExprParser::new("list(string)");
        let expr = parser.parse().unwrap();
        assert_eq!(
            expr,
            TypeExpr::Collection(
                CollectionType::List,
                Box::new(TypeExpr::Primitive(
                    Type::String,
                    Span::new(5, 11, 1, 6, 1, 12)
                )),
                Span::new(0, 12, 1, 1, 1, 13),
            )
        );
        let mut parser = TypeExprParser::new("set(number)");
        let expr = parser.parse().unwrap();
        assert_eq!(
            expr,
            TypeExpr::Collection(
                CollectionType::Set,
                Box::new(TypeExpr::Primitive(
                    Type::Number,
                    Span::new(4, 10, 1, 5, 1, 11)
                )),
                Span::new(0, 11, 1, 1, 1, 12),
            )
        );
        let mut parser = TypeExprParser::new("map(bool)");
        let expr = parser.parse().unwrap();
        assert_eq!(
            expr,
            TypeExpr::Collection(
                CollectionType::Map,
                Box::new(TypeExpr::Primitive(Type::Bool, Span::new(4, 8, 1, 5, 1, 9))),
                Span::new(0, 9, 1, 1, 1, 10),
            )
        );
    }
    #[test]
    fn test_parse_object() {
        let mut parser = TypeExprParser::new("object({ foo : string, bar = number })");
        let expr = parser.parse().unwrap();
        let mut expected_attrs = HashMap::new();
        expected_attrs.insert(
            "foo".to_string(),
            ObjectAttrType::required(TypeExpr::Primitive(
                Type::String,
                Span::new(15, 21, 1, 16, 1, 22),
            )),
        );
        expected_attrs.insert(
            "bar".to_string(),
            ObjectAttrType::required(TypeExpr::Primitive(
                Type::Number,
                Span::new(29, 35, 1, 30, 1, 36),
            )),
        );
        assert_eq!(
            expr,
            TypeExpr::Object(expected_attrs, Span::new(0, 38, 1, 1, 1, 39))
        );
        let mut parser = TypeExprParser::new("object({ foo : string, })");
        let expr = parser.parse().unwrap();
        let mut expected_attrs2 = HashMap::new();
        expected_attrs2.insert(
            "foo".to_string(),
            ObjectAttrType::required(TypeExpr::Primitive(
                Type::String,
                Span::new(15, 21, 1, 16, 1, 22),
            )),
        );
        assert_eq!(
            expr,
            TypeExpr::Object(expected_attrs2, Span::new(0, 25, 1, 1, 1, 26))
        );
    }
    #[test]
    fn test_parse_object_optional() {
        let check_obj = |e: &TypeExpr| -> usize {
            if let TypeExpr::Object(attrs, _) = e {
                attrs.len()
            } else {
                0
            }
        };
        fn get_attr_info(expr: &TypeExpr, name: &str) -> (bool, Option<Expression>) {
            match expr {
                TypeExpr::Object(attrs, _) => match attrs.get(name) {
                    Some(a) => (a.optional, a.default_val.clone()),
                    None => (false, None),
                },
                _ => (false, None),
            }
        }
        let mut parser =
            TypeExprParser::new("object({ name = string, port = optional(number, 8080) })");
        let expr = parser.parse().unwrap_or(TypeExpr::Primitive(
            Type::Dynamic,
            Span::new(0, 0, 1, 1, 1, 1),
        ));
        assert_eq!(get_attr_info(&expr, "name"), (false, None));
        assert_eq!(
            get_attr_info(&expr, "port"),
            (
                true,
                Some(Expression::Number(
                    8080_i32.into(),
                    Span::new(48, 52, 1, 49, 1, 53)
                ))
            )
        );
        assert_eq!(get_attr_info(&expr, "missing"), (false, None));
        assert_eq!(
            get_attr_info(
                &TypeExpr::Primitive(Type::Bool, Span::new(0, 0, 1, 1, 1, 1)),
                "port"
            ),
            (false, None)
        );
        assert_eq!(check_obj(&expr), 2);
        let mut parser2 = TypeExprParser::new("object({ desc = optional(string) })");
        let expr2 = parser2.parse().unwrap_or(TypeExpr::Primitive(
            Type::Dynamic,
            Span::new(0, 0, 1, 1, 1, 1),
        ));
        assert_eq!(get_attr_info(&expr2, "desc"), (true, None));
        assert_eq!(check_obj(&expr2), 1);
        assert_eq!(
            check_obj(&TypeExpr::Primitive(
                Type::Bool,
                Span::new(0, 0, 1, 1, 1, 1)
            )),
            0
        );
        let complex_src = "object({
            s = optional(string, \"default\"),
            b = optional(bool, true),
            n = optional(string, null),
            neg = optional(number, -10),
            t = optional(list(string), [\"a\", \"b\"]),
            o = optional(map(number), { a = 1, b : 2 })
        })";
        let mut p_complex = TypeExprParser::new(complex_src);
        assert!(p_complex.parse().is_ok());
        assert!(TypeExprParser::new("optional(string)").parse().is_err());
        assert!(
            TypeExprParser::new("list(optional(string))")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("set(optional(number))")
                .parse()
                .is_err()
        );
        assert!(TypeExprParser::new("map(optional(bool))").parse().is_err());
    }
    #[test]
    fn test_parse_tuple() {
        let mut parser = TypeExprParser::new("tuple([string, number, bool])");
        let expr = parser.parse().unwrap();
        let expected_elems = vec![
            TypeExpr::Primitive(Type::String, Span::new(7, 13, 1, 8, 1, 14)),
            TypeExpr::Primitive(Type::Number, Span::new(15, 21, 1, 16, 1, 22)),
            TypeExpr::Primitive(Type::Bool, Span::new(23, 27, 1, 24, 1, 28)),
        ];
        assert_eq!(
            expr,
            TypeExpr::Tuple(expected_elems, Span::new(0, 29, 1, 1, 1, 30))
        );
        let mut parser = TypeExprParser::new("tuple([string, ])");
        let expr = parser.parse().unwrap();
        let expected_elems2 = vec![TypeExpr::Primitive(
            Type::String,
            Span::new(7, 13, 1, 8, 1, 14),
        )];
        assert_eq!(
            expr,
            TypeExpr::Tuple(expected_elems2, Span::new(0, 17, 1, 1, 1, 18))
        );
    }
    #[test]
    fn test_parse_errors() {
        assert!(TypeExprParser::new("@").parse().is_err());
        assert!(TypeExprParser::new("{").parse().is_err());
        assert!(TypeExprParser::new("invalid_type").parse().is_err());
        assert!(TypeExprParser::new("list(string").parse().is_err());
        assert!(TypeExprParser::new("object(foo: string)").parse().is_err());
        assert!(TypeExprParser::new("object({foo: string}").parse().is_err());
        assert!(
            TypeExprParser::new("object({123: string})")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({foo % string})")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({foo: string bar: string})")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("tuple([string number])")
                .parse()
                .is_err()
        );
        assert!(TypeExprParser::new("tuple([string)").parse().is_err());
        assert!(TypeExprParser::new("string string").parse().is_err());
        assert!(TypeExprParser::new("").parse().is_err());
        assert!(TypeExprParser::new("list(").parse().is_err());
        assert!(TypeExprParser::new("object({").parse().is_err());
        assert!(
            TypeExprParser::new("object({ 123 = string })")
                .parse()
                .is_err()
        );
        assert!(TypeExprParser::new("object({ a = string ").parse().is_err());
        assert!(TypeExprParser::new("object({ a : string").parse().is_err());
        assert!(
            TypeExprParser::new("object({ a = string, 123 = bool })")
                .parse()
                .is_err()
        );
        assert!(TypeExprParser::new("tuple([").parse().is_err());
        assert!(TypeExprParser::new("tuple([string").parse().is_err());
        assert!(
            TypeExprParser::new("list(\n /* block */\n # inline\n string\n)")
                .parse()
                .is_ok()
        );
    }
    #[test]
    fn test_type_expr_coverage_gaps_exhaustive() {
        assert!(TypeExprParser::new("list string").parse().is_err());
        assert!(TypeExprParser::new("object string").parse().is_err());
        assert!(TypeExprParser::new("tuple string").parse().is_err());
        assert!(TypeExprParser::new("tuple(string)").parse().is_err());
        assert!(TypeExprParser::new("tuple([bad_type])").parse().is_err());
        assert!(TypeExprParser::new("tuple([string]").parse().is_err());
        assert!(TypeExprParser::new("object({ a").parse().is_err());
        assert!(
            TypeExprParser::new("object({ a = optional string })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(bad_type) })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(string, ) })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(string, \"def\" })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(bool, false) })")
                .parse()
                .is_ok()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(number, - \"abc\") })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(tuple([number, number]), [1 2]) })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(object({ x = number }), { x 1 }) })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(object({ x = number }), { x")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new(
                "object({ a = optional(object({ x = number, y = number }), { x = 1 y = 2 }) })"
            )
            .parse()
            .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(string, ?) })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(string,")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(number, -")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(tuple([number]), [?]) })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(map(number), { ? = 1 }) })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(map(number), { a = ? }) })")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(tuple([number]), [1")
                .parse()
                .is_err()
        );
        assert!(
            TypeExprParser::new("object({ a = optional(map(number), { a = 1")
                .parse()
                .is_err()
        );
        assert!(TypeExprParser::new("object({ a = ? })").parse().is_err());
        assert!(TypeExprParser::new("object({ a =").parse().is_err());
        assert!(
            TypeExprParser::new("object({ a = optional(string")
                .parse()
                .is_err()
        );
        let mut p_bad_num = TypeExprParser::new("");
        p_bad_num.current = Some(Token::new(
            TokenKind::Number,
            "invalid_num",
            Span::new(0, 0, 1, 1, 1, 1),
        ));
        assert!(p_bad_num.parse_default_expr().is_ok());
        let mut p1 = TypeExprParser::new("");
        p1.current = Some(Token::new(
            TokenKind::String,
            "raw_string",
            Span::new(0, 0, 1, 1, 1, 1),
        ));
        assert_eq!(
            p1.parse_default_expr(),
            Ok(Expression::String(
                "raw_string".to_string(),
                Span::new(0, 0, 1, 1, 1, 1)
            ))
        );
        let mut p2 = TypeExprParser::new("");
        p2.current = Some(Token::new(
            TokenKind::String,
            "\"unclosed",
            Span::new(0, 0, 1, 1, 1, 1),
        ));
        assert_eq!(
            p2.parse_default_expr(),
            Ok(Expression::String(
                "\"unclosed".to_string(),
                Span::new(0, 0, 1, 1, 1, 1)
            ))
        );
        let mut p3 = TypeExprParser::new("");
        p3.current = Some(Token::new(
            TokenKind::String,
            "\"",
            Span::new(0, 0, 1, 1, 1, 1),
        ));
        assert_eq!(
            p3.parse_default_expr(),
            Ok(Expression::String(
                "\"".to_string(),
                Span::new(0, 0, 1, 1, 1, 1)
            ))
        );
    }
}
