//! Unit tests for the CST builder and mutation engine.
use super::*;
use crate::error::HclError;
use crate::number::Number;
use crate::types::ty::Type;
use std::collections::BTreeMap;
use std::str::FromStr;
#[test]
fn test_cst_attribute_and_block_getters() {
    let span = Span::new(0, 0, 0, 0, 0, 0);
    let attr = CstAttribute::new("foo", vec![Token::new(TokenKind::Ident, "bar", span)]);
    assert_eq!(attr.name(), "foo");
    assert_eq!(attr.expr_tokens().len(), 1);
    let mut block = CstBlock::new("res", vec!["lbl".to_string()]);
    assert_eq!(block.block_type(), "res");
    assert_eq!(block.labels(), &["lbl"]);
    assert_eq!(block.body().attributes().len(), 0);
    block.body_mut().set_attribute_raw("key", vec![]);
    assert_eq!(block.body().attributes().len(), 1);
}
#[test]
fn test_cst_file_builders_and_mutations() -> Result<(), HclError> {
    let mut file = CstFile::new();
    let span = Span::new(0, 0, 0, 0, 0, 0);
    file.body_mut().set_attribute_value(
        "name",
        &Value::new(Type::String, ValueData::String("my_bucket".to_string())),
    );
    file.body_mut().set_attribute_value(
        "count",
        &Value::new(Type::Number, ValueData::Number(Number::from_str("3")?)),
    );
    file.body_mut()
        .set_attribute_value("enabled", &Value::new(Type::Bool, ValueData::Bool(true)));
    file.body_mut()
        .set_attribute_value("tags", &Value::new(Type::Dynamic, ValueData::Null));
    file.body_mut().set_attribute_value(
        "list",
        &Value::new(
            Type::Dynamic,
            ValueData::Array(vec![
                Value::new(Type::Bool, ValueData::Bool(false)),
                Value::new(Type::Bool, ValueData::Bool(true)),
            ]),
        ),
    );
    let mut obj = BTreeMap::new();
    obj.insert(
        "k".to_string(),
        Value::new(Type::String, ValueData::String("v".to_string())),
    );
    file.body_mut()
        .set_attribute_value("map", &Value::new(Type::Dynamic, ValueData::Object(obj)));
    let mut set = std::collections::BTreeSet::new();
    set.insert(Value::new(Type::String, ValueData::String("s".to_string())));
    file.body_mut()
        .set_attribute_value("set", &Value::new(Type::Dynamic, ValueData::Set(set)));
    file.body_mut()
        .set_attribute_value("unk", &Value::unknown(Type::Dynamic));
    file.body_mut()
        .set_attribute_value("cap", &Value::capsule("custom", 123_u32));
    assert_eq!(file.body().attributes().len(), 9);
    file.body_mut().set_attribute_raw(
        "name",
        vec![Token::new(TokenKind::String, r#""updated""#, span.clone())],
    );
    assert_eq!(file.body().attributes().len(), 9);
    let removed = file.body_mut().remove_attribute("tags");
    assert!(removed.is_some());
    assert_eq!(file.body().attributes().len(), 8);
    assert!(file.body_mut().remove_attribute("nonexistent").is_none());
    let mut block = CstBlock::new("server", vec!["web".to_string()]);
    block.body_mut().set_attribute_raw(
        "ip",
        vec![Token::new(TokenKind::String, r#""1.2.3.4""#, span)],
    );
    file.body_mut().append_block(block);
    file.body_mut().append_newline();
    assert_eq!(file.body().blocks().len(), 1);
    let rendered = file.write_to_string();
    assert!(rendered.contains(r#"server "web" {"#));
    assert!(rendered.contains(r#"ip = "1.2.3.4""#));
    assert!(rendered.contains(r#"name = "updated""#));
    assert_eq!(file.to_string(), rendered);
    let block_removed = file.body_mut().remove_block("server", &["web"]);
    assert!(block_removed.is_some());
    assert_eq!(file.body().blocks().len(), 0);
    assert!(file.body_mut().remove_block("server", &["web"]).is_none());
    Ok(())
}
#[test]
fn test_cst_file_parse_and_roundtrip() -> Result<(), HclError> {
    let input = "name = \"foo\"\n\nblock \"type\" \"name\" {\n  attr = true\n}\n";
    let file = CstFile::parse(input)?;
    assert_eq!(file.body().attributes().len(), 1);
    assert_eq!(file.body().blocks().len(), 1);
    let output = file.write_to_string();
    assert!(output.contains(r#"name = "foo""#));
    assert!(output.contains(r#"block "type" "name" {"#));
    Ok(())
}
#[test]
fn test_cst_builder_coverage_extra() -> Result<(), HclError> {
    let span = Span::new(0, 0, 0, 0, 0, 0);
    let default_body = CstBody::default();
    assert_eq!(default_body.indent_level, 0);
    let input = "  # comment\n  attr = 1\n  block unquoted_label {\n    inner = 2\n  }\n\n";
    let parsed = CstFile::parse(input)?;
    assert_eq!(parsed.body().blocks()[0].labels(), &["unquoted_label"]);
    let rendered = parsed.write_to_string();
    assert!(rendered.contains("unquoted_label"));
    assert!(rendered.contains("attr = 1"));
    let mut file = CstFile::new();
    let mut set = std::collections::BTreeSet::new();
    set.insert(Value::new(
        Type::String,
        ValueData::String("elem1".to_string()),
    ));
    set.insert(Value::new(
        Type::Dynamic,
        ValueData::String("elem2".to_string()),
    ));
    assert_eq!(set.len(), 2);
    file.body_mut()
        .set_attribute_value("myset", &Value::new(Type::Dynamic, ValueData::Set(set)));
    let mut parent_block = CstBlock::new("parent", vec!["p1".to_string()]);
    parent_block.body.indent_level = 1;
    parent_block
        .body_mut()
        .set_attribute_value("nested_key", &Value::new(Type::Bool, ValueData::Bool(true)));
    let nested_block = CstBlock::new("child", vec![]);
    parent_block.body_mut().append_block(nested_block);
    let mut block_with_leading = CstBlock::new("child2", vec![]);
    block_with_leading
        .leading
        .push(Token::new(TokenKind::Whitespace, " ", span.clone()));
    parent_block.body_mut().append_block(block_with_leading);
    file.body_mut().append_block(parent_block);
    assert!(
        file.body_mut()
            .remove_block("different_type", &["p1"])
            .is_none()
    );
    assert!(
        file.body_mut()
            .remove_block("parent", &["p1", "extra"])
            .is_none()
    );
    assert!(
        file.body_mut()
            .remove_block("parent", &["different_lbl"])
            .is_none()
    );
    assert!(file.body_mut().remove_block("parent", &["p1"]).is_some());
    assert!(CstFile::parse("invalid {").is_err());
    let mut test_body = CstBody::new();
    let mut test_attr = CstAttribute::new(
        "foo",
        vec![Token::new(TokenKind::Number, "1", span.clone())],
    );
    test_attr
        .leading
        .push(Token::new(TokenKind::Whitespace, " ", span.clone()));
    test_attr
        .trailing
        .push(Token::new(TokenKind::Newline, "\n", span.clone()));
    test_body.items.push(CstItem::Attribute(test_attr));
    let mut test_block = CstBlock::new("blk", vec![]);
    test_block
        .leading
        .push(Token::new(TokenKind::Whitespace, "  ", span.clone()));
    test_block
        .trailing
        .push(Token::new(TokenKind::Newline, "\n", span.clone()));
    test_body.items.push(CstItem::Block(test_block));
    test_body
        .trailing
        .push(Token::new(TokenKind::Newline, "\n", span.clone()));
    let mut rendered_out = String::new();
    test_body.render(&mut rendered_out);
    assert!(rendered_out.contains("foo = 1"));
    assert!(rendered_out.contains("blk {\n}"));
    let mut doc = crate::cst::document::Document::new();
    let edge_block = crate::cst::document::Block {
        leading: crate::cst::document::TokenStream::new(),
        type_ident: Token::new(TokenKind::Ident, "edge", span.clone()),
        labels: vec![
            Token::new(TokenKind::String, "\"unclosed", span.clone()),
            Token::new(TokenKind::String, "\"", span.clone()),
        ],
        open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
        body: Box::new(crate::cst::document::Document::new()),
        close_brace: Token::new(TokenKind::CBrace, "}", span),
        trailing: crate::cst::document::TokenStream::new(),
    };
    doc.append_block(edge_block);
    let cst_edge = from_document(&doc);
    assert_eq!(cst_edge.body().blocks()[0].labels().len(), 2);
    Ok(())
}
#[test]
fn test_tokens_for_expression_and_traversal() {
    let span = Span::new(0, 0, 1, 1, 1, 1);
    let val_num = Value::new(Type::Number, ValueData::Number(Number::from(42)));
    let toks_num = tokens_for_value(&val_num);
    assert_eq!(toks_num[0].text, "42");
    let val_set = Value::new(
        Type::Set(Box::new(Type::String)),
        ValueData::Set(std::collections::BTreeSet::from([Value::new(
            Type::String,
            ValueData::String("elem".into()),
        )])),
    );
    let toks_set = tokens_for_value(&val_set);
    assert_eq!(toks_set[0].kind, TokenKind::OBrack);
    let trav = Traversal {
        expr: Box::new(Expression::Variable("var".to_string(), span.clone())),
        operators: vec![
            TraversalOperator::GetAttr("servers".to_string(), span.clone()),
            TraversalOperator::Index(
                Expression::Number(Number::from(0), span.clone()),
                span.clone(),
            ),
            TraversalOperator::LegacyIndex(1, span.clone()),
            TraversalOperator::AttrSplat(span.clone()),
            TraversalOperator::FullSplat(span.clone()),
        ],
    };
    let trav_toks = tokens_for_traversal(&trav);
    let trav_str: String = trav_toks.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(trav_str, "var.servers[0].1.*[*]");
    assert_eq!(
        tokens_for_expression(&Expression::Null(span.clone()))[0].text,
        "null"
    );
    assert_eq!(
        tokens_for_expression(&Expression::Bool(true, span.clone()))[0].text,
        "true"
    );
    assert_eq!(
        tokens_for_expression(&Expression::String("hi".into(), span.clone()))[0].text,
        "\"hi\""
    );
    assert_eq!(
        tokens_for_expression(&Expression::Variable("foo".into(), span.clone()))[0].text,
        "foo"
    );
    let tup_expr = Expression::Tuple(
        vec![
            Expression::Number(Number::from(1), span.clone()),
            Expression::Number(Number::from(2), span.clone()),
        ],
        span.clone(),
    );
    let tup_toks = tokens_for_expression(&tup_expr);
    let tup_str: String = tup_toks.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(tup_str, "[1, 2]");
    let obj_expr = Expression::Object(
        vec![(
            Expression::Variable("k".into(), span.clone()),
            Expression::String("v".into(), span.clone()),
        )],
        span.clone(),
    );
    let obj_toks = tokens_for_expression(&obj_expr);
    assert!(obj_toks.iter().any(|t| t.text == "k"));
    let bin_expr = Expression::BinaryOp(
        BinaryOp::Add,
        Box::new(Expression::Number(Number::from(1), span.clone())),
        Box::new(Expression::Number(Number::from(2), span.clone())),
        span.clone(),
    );
    let bin_str: String = tokens_for_expression(&bin_expr)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(bin_str, "1 + 2");
    let un_expr = Expression::UnaryOp(
        UnaryOp::Not,
        Box::new(Expression::Bool(true, span.clone())),
        span.clone(),
    );
    let un_str: String = tokens_for_expression(&un_expr)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(un_str, "!true");
    let paren_expr = Expression::Parentheses(Box::new(bin_expr), span);
    let paren_str: String = tokens_for_expression(&paren_expr)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(paren_str, "(1 + 2)");
}

#[test]
fn test_tokens_for_complex_expressions() {
    let span = Span::new(0, 0, 1, 1, 1, 1);
    let cond_expr = Expression::Conditional(
        Box::new(crate::ast::expr::Conditional {
            cond_expr: Expression::Bool(true, span.clone()),
            true_expr: Expression::Number(Number::from(1), span.clone()),
            false_expr: Expression::Number(Number::from(0), span.clone()),
        }),
        span.clone(),
    );
    let cond_str: String = tokens_for_expression(&cond_expr)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(cond_str, "true ? 1 : 0");
    let fn_expr = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "max".into(),
            args: vec![
                Expression::Number(Number::from(1), span.clone()),
                Expression::Number(Number::from(2), span.clone()),
            ],
            expand_final: true,
        }),
        span.clone(),
    );
    let fn_str: String = tokens_for_expression(&fn_expr)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(fn_str, "max(1, 2...)");
    let for_tuple = Expression::ForExpr(
        Box::new(crate::ast::expr::ForExpr {
            key_var: Some("i".to_string()),
            val_var: "v".to_string(),
            collection: Box::new(Expression::Variable("list".into(), span.clone())),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("v".into(), span.clone())),
            cond_expr: Some(Box::new(Expression::Bool(true, span.clone()))),
            grouping: false,
        }),
        span.clone(),
    );
    let for_tup_str: String = tokens_for_expression(&for_tuple)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(for_tup_str, "[for i, v in list : v if true]");
    let for_obj = Expression::ForExpr(
        Box::new(crate::ast::expr::ForExpr {
            key_var: None,
            val_var: "item".to_string(),
            collection: Box::new(Expression::Variable("items".into(), span.clone())),
            key_expr: Some(Box::new(Expression::Variable("item".into(), span.clone()))),
            val_expr: Box::new(Expression::Variable("item".into(), span.clone())),
            cond_expr: None,
            grouping: true,
        }),
        span.clone(),
    );
    let for_obj_str: String = tokens_for_expression(&for_obj)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(for_obj_str, "{for item in items : item => item...}");
    let tmpl = Expression::Template(
        vec![
            TemplatePart::Literal("hello ".to_string(), span.clone()),
            TemplatePart::Interpolation(
                Expression::Variable("name".into(), span.clone()),
                span.clone(),
            ),
        ],
        span,
    );
    let tmpl_str: String = tokens_for_expression(&tmpl)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(tmpl_str, "\"hello ${name}\"");
}

#[test]
fn test_cst_builder_coverage_gaps() {
    let span = Span::new(0, 0, 0, 0, 0, 0);
    let attr = CstAttribute::new("a", vec![]);
    let item_attr = CstItem::Attribute(attr);
    let block = CstBlock::new("blk", vec![]);
    let item_blk = CstItem::Block(block);
    let item_trivia = CstItem::Trivia(TokenStream::new());
    assert!(item_attr.as_attribute().is_some());
    assert!(item_attr.as_block().is_none());
    assert!(item_blk.as_block().is_some());
    assert!(item_blk.as_attribute().is_none());
    assert!(item_trivia.as_attribute().is_none());
    assert!(item_trivia.as_block().is_none());
    let mut body = CstBody::new();
    body.append_newline();
    body.append_block(CstBlock::new("server", vec![]));
    body.set_attribute_raw("other", vec![]);
    body.set_attribute_raw(
        "target",
        vec![Token::new(TokenKind::Ident, "1", span.clone())],
    );
    body.set_attribute_raw(
        "target",
        vec![Token::new(TokenKind::Ident, "2", span.clone())],
    );
    assert_eq!(body.attributes().len(), 2);
    assert!(body.remove_attribute("target").is_some());
    let mut body_indent = CstBody::with_indent_level(1);
    let blk_empty_leading = CstBlock::new("sub", vec![]);
    body_indent.append_block(blk_empty_leading);
    let mut blk_with_leading = CstBlock::new("sub2", vec![]);
    blk_with_leading
        .leading
        .push(Token::new(TokenKind::Whitespace, "  ", span.clone()));
    body_indent.append_block(blk_with_leading);
    let mut doc = Document::new();
    let attr_doc = crate::cst::document::Attribute {
        leading: TokenStream::new(),
        name: Token::new(TokenKind::Ident, "key", span.clone()),
        equals_leading: TokenStream::new(),
        equals: Token::new(TokenKind::Assign, "=", span.clone()),
        expr_tokens: vec![Token::new(TokenKind::Ident, "val", span.clone())],
        trailing: TokenStream::new(),
    };
    doc.items.push(DocumentItem::Attribute(attr_doc));
    let blk_doc = crate::cst::document::Block {
        leading: TokenStream::new(),
        type_ident: Token::new(TokenKind::Ident, "resource", span.clone()),
        labels: vec![
            Token::new(TokenKind::String, "\"my_res\"", span.clone()),
            Token::new(TokenKind::Ident, "unquoted", span.clone()),
        ],
        open_brace: Token::new(TokenKind::OBrace, "{", span.clone()),
        body: Box::new(Document::new()),
        close_brace: Token::new(TokenKind::CBrace, "}", span.clone()),
        trailing: TokenStream::new(),
    };
    doc.append_block(blk_doc);
    let cst_file = from_document(&doc);
    assert_eq!(cst_file.body().attributes().len(), 1);
    assert_eq!(cst_file.body().blocks().len(), 1);
    assert_eq!(
        cst_file.body().blocks()[0].labels(),
        &["my_res", "unquoted"]
    );
    let mut doc2 = Document::new();
    let mut trivia_stream = TokenStream::new();
    trivia_stream.push(Token::new(TokenKind::Comment, "# comment\n", span));
    doc2.items.push(DocumentItem::Trivia(trivia_stream));
    let cst_file2 = from_document(&doc2);
    assert_eq!(cst_file2.body().items().len(), 1);
}

#[test]
fn test_cst_builder_coverage_gaps_operators_and_calls() {
    let span = Span::new(0, 0, 0, 0, 0, 0);
    let ops = [
        (BinaryOp::Sub, "-"),
        (BinaryOp::Mul, "*"),
        (BinaryOp::Div, "/"),
        (BinaryOp::Mod, "%"),
        (BinaryOp::Eq, "=="),
        (BinaryOp::NotEq, "!="),
        (BinaryOp::Less, "<"),
        (BinaryOp::LessEq, "<="),
        (BinaryOp::Greater, ">"),
        (BinaryOp::GreaterEq, ">="),
        (BinaryOp::And, "&&"),
        (BinaryOp::Or, "||"),
    ];
    for (op, expected_op_str) in ops {
        let expr = Expression::BinaryOp(
            op,
            Box::new(Expression::Number(Number::from(1), span.clone())),
            Box::new(Expression::Number(Number::from(2), span.clone())),
            span.clone(),
        );
        let s: String = tokens_for_expression(&expr)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(s, format!("1 {expected_op_str} 2"));
    }
    let neg_expr = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Number(Number::from(5), span.clone())),
        span.clone(),
    );
    let neg_str: String = tokens_for_expression(&neg_expr)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(neg_str, "-5");
    let bool_false = Expression::Bool(false, span.clone());
    let bool_false_str: String = tokens_for_expression(&bool_false)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(bool_false_str, "false");
    let fn_no_expand = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "min".into(),
            args: vec![Expression::Number(Number::from(1), span.clone())],
            expand_final: false,
        }),
        span.clone(),
    );
    let fn_str: String = tokens_for_expression(&fn_no_expand)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(fn_str, "min(1)");
    let trav_expr = Expression::Traversal(
        Box::new(crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("foo".to_string(), span.clone())),
            operators: vec![crate::ast::expr::TraversalOperator::GetAttr(
                "bar".to_string(),
                span.clone(),
            )],
        }),
        span,
    );
    let trav_str: String = tokens_for_expression(&trav_expr)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(trav_str, "foo.bar");
}

#[test]
fn test_cst_builder_coverage_gaps_templates() {
    let span = Span::new(0, 0, 0, 0, 0, 0);
    let tmpl_strip = Expression::Template(
        vec![
            TemplatePart::Directive(
                crate::ast::expr::Directive::Strip {
                    strip_left: true,
                    strip_right: false,
                },
                span.clone(),
            ),
            TemplatePart::Directive(
                crate::ast::expr::Directive::Strip {
                    strip_left: false,
                    strip_right: true,
                },
                span.clone(),
            ),
        ],
        span.clone(),
    );
    let tmpl_strip_str: String = tokens_for_expression(&tmpl_strip)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(tmpl_strip_str, "\"%{~}%{~}\"");
    let tmpl_if = Expression::Template(
        vec![TemplatePart::Directive(
            crate::ast::expr::Directive::If {
                cond: Expression::Bool(true, span.clone()),
                true_expr: vec![TemplatePart::Literal("hello".to_string(), span.clone())],
                else_ifs: vec![(
                    Expression::Bool(false, span.clone()),
                    vec![TemplatePart::Literal("elif".to_string(), span.clone())],
                )],
                false_expr: Some(vec![TemplatePart::Literal(
                    "world".to_string(),
                    span.clone(),
                )]),
            },
            span.clone(),
        )],
        span.clone(),
    );
    let tmpl_if_str: String = tokens_for_expression(&tmpl_if)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(
        tmpl_if_str,
        "\"%{if true}hello%{else if false}elif%{else}world%{endif}\""
    );
    let tmpl_if_no_else = Expression::Template(
        vec![TemplatePart::Directive(
            crate::ast::expr::Directive::If {
                cond: Expression::Bool(true, span.clone()),
                true_expr: vec![TemplatePart::Literal("hello".to_string(), span.clone())],
                else_ifs: vec![],
                false_expr: None,
            },
            span.clone(),
        )],
        span,
    );
    let tmpl_if_no_else_str: String = tokens_for_expression(&tmpl_if_no_else)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(tmpl_if_no_else_str, "\"%{if true}hello%{endif}\"");
}

#[test]
fn test_cst_builder_coverage_gaps_templates_for() {
    let span = Span::new(0, 0, 0, 0, 0, 0);
    let tmpl_for = Expression::Template(
        vec![
            TemplatePart::Directive(
                crate::ast::expr::Directive::For {
                    key_var: Some("k".to_string()),
                    val_var: "v".to_string(),
                    collection: Expression::Variable("items".to_string(), span.clone()),
                    body: vec![TemplatePart::Interpolation(
                        Expression::Variable("v".to_string(), span.clone()),
                        span.clone(),
                    )],
                },
                span.clone(),
            ),
            TemplatePart::Directive(
                crate::ast::expr::Directive::For {
                    key_var: None,
                    val_var: "item".to_string(),
                    collection: Expression::Variable("items".to_string(), span.clone()),
                    body: vec![TemplatePart::Literal("!".to_string(), span.clone())],
                },
                span.clone(),
            ),
        ],
        span,
    );
    let tmpl_for_str: String = tokens_for_expression(&tmpl_for)
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(
        tmpl_for_str,
        "\"%{for k, v in items}${v}%{endfor}%{for item in items}!%{endfor}\""
    );
}

#[test]
fn test_cst_builder_coverage_gaps_file_mutations() -> Result<(), HclError> {
    let span = Span::new(0, 0, 0, 0, 0, 0);
    let mut file = CstFile::new();
    file.body_mut().set_attribute_raw(
        "attr1",
        vec![Token::new(TokenKind::Number, "10", span.clone())],
    );
    file.body_mut().set_attribute_raw(
        "attr2",
        vec![Token::new(TokenKind::Number, "20", span.clone())],
    );
    file.body_mut()
        .append_block(CstBlock::new("service", vec!["web".to_string()]));
    assert_eq!(file.body().attributes().len(), 2);
    assert_eq!(file.body().blocks().len(), 1);
    let replacement_expr = Expression::Number(Number::from(42_i64), span);
    file.replace_attribute_expr("attr1", &replacement_expr)?;
    file.replace_attribute_expr("attr2", &replacement_expr)?;
    let rendered = file.write_to_string();
    assert!(rendered.contains("42"));
    assert!(
        file.replace_attribute_expr("nonexistent", &replacement_expr)
            .is_err()
    );
    let removed_attr = file.remove_attribute("attr2");
    assert!(removed_attr.is_some());
    assert_eq!(file.body().attributes().len(), 1);
    assert!(file.remove_attribute("attr2").is_none());
    let removed_block = file.remove_block("service", &["web"]);
    assert!(removed_block.is_some());
    assert_eq!(file.body().blocks().len(), 0);
    assert!(file.remove_block("service", &["web"]).is_none());
    Ok(())
}
#[test]
fn test_cst_attribute_inplace_mutation_and_rename_variable() {
    let span = Span::new(0, 0, 1, 1, 1, 1);
    let mut attr = CstAttribute::new(
        "timeout",
        vec![Token::new(TokenKind::Number, "30", span.clone())],
    );
    attr.leading
        .push(Token::new(TokenKind::Whitespace, "  ", span.clone()));
    attr.trailing.push(Token::new(
        TokenKind::Comment,
        " # Default timeout\n",
        span.clone(),
    ));
    let new_expr = Expression::Number(Number::from(60), span.clone());
    attr.replace_expr(&new_expr);
    let mut rendered = String::new();
    CstBody::render_cst_attribute(&attr, &mut rendered);
    assert_eq!(rendered, "  timeout = 60 # Default timeout\n");
    attr.replace_name("connect_timeout");
    let mut rendered2 = String::new();
    CstBody::render_cst_attribute(&attr, &mut rendered2);
    assert_eq!(rendered2, "  connect_timeout = 60 # Default timeout\n");
    attr.set_string("fast");
    let mut rendered_str = String::new();
    CstBody::render_cst_attribute(&attr, &mut rendered_str);
    assert_eq!(
        rendered_str,
        "  connect_timeout = \"fast\" # Default timeout\n"
    );
    attr.set_number(&Number::from(120));
    let mut rendered_num = String::new();
    CstBody::render_cst_attribute(&attr, &mut rendered_num);
    assert_eq!(rendered_num, "  connect_timeout = 120 # Default timeout\n");
    attr.set_bool(true);
    let mut rendered_bool = String::new();
    CstBody::render_cst_attribute(&attr, &mut rendered_bool);
    assert_eq!(
        rendered_bool,
        "  connect_timeout = true # Default timeout\n"
    );
    let traversal = crate::ast::expr::Traversal {
        expr: Box::new(Expression::Variable("var".to_string(), span.clone())),
        operators: vec![crate::ast::expr::TraversalOperator::GetAttr(
            "custom_timeout".to_string(),
            span.clone(),
        )],
    };
    attr.set_traversal(&traversal);
    let mut rendered_trav = String::new();
    CstBody::render_cst_attribute(&attr, &mut rendered_trav);
    assert_eq!(
        rendered_trav,
        "  connect_timeout = var.custom_timeout # Default timeout\n"
    );
    let mut file = CstFile::new();
    let mut a1 = CstAttribute::new(
        "count",
        vec![
            Token::new(TokenKind::Ident, "old_var", span.clone()),
            Token::new(TokenKind::Plus, "+", span.clone()),
            Token::new(TokenKind::Number, "1", span.clone()),
        ],
    );
    a1.trailing.push(Token::new(
        TokenKind::Comment,
        " # references old_var in comment\n",
        span.clone(),
    ));
    file.body_mut().items.push(CstItem::Attribute(a1));
    let mut inner_block = CstBlock::new("server", vec!["app".to_string()]);
    let a2 = CstAttribute::new(
        "replica",
        vec![Token::new(TokenKind::Ident, "old_var", span)],
    );
    inner_block.body.items.push(CstItem::Attribute(a2));
    file.body_mut().append_block(inner_block);
    let renamed_count = file.rename_variable("old_var", "new_var");
    assert_eq!(renamed_count, 2);
    let file_rendered = file.write_to_string();
    assert!(file_rendered.contains("count = new_var+1 # references old_var in comment"));
    assert!(file_rendered.contains("replica = new_var"));
}
#[test]
fn test_programmatic_comments_on_cst_attribute_and_block() {
    let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
    let mut attr = CstAttribute::new(
        "timeout",
        vec![Token::new(TokenKind::Number, "30", dummy_span)],
    );
    attr.with_leading_comment("Leading timeout comment")
        .with_trailing_inline_comment("Inline timeout comment");
    let mut rendered_attr = String::new();
    CstBody::render_cst_attribute(&attr, &mut rendered_attr);
    assert!(rendered_attr.contains("# Leading timeout comment\n"));
    assert!(rendered_attr.contains("timeout = 30 # Inline timeout comment\n"));
    let mut block = CstBlock::new("server", vec!["web".to_string()]);
    block
        .with_leading_comment("// Leading block comment")
        .with_trailing_inline_comment("/* Trailing inline block comment */");
    let mut body = CstBody::new();
    body.append_block(block);
    let mut rendered_block = String::new();
    body.render(&mut rendered_block);
    assert!(rendered_block.contains("// Leading block comment\n"));
    assert!(rendered_block.contains("/* Trailing inline block comment */"));
}
#[test]
fn test_clean_cst_node_deletion_and_trivia_compaction() -> Result<(), HclError> {
    let input = r#"
resource "aws_vpc" "main" {
  cidr_block = "10.0.0.0/16"

  enable_dns_hostnames = true

  enable_dns_support = true
}

resource "aws_subnet" "sub" {
  cidr_block = "10.0.1.0/24"
}
"#;
    let mut file = CstFile::parse(input)?;
    let vpc_block = file
        .block_mut("resource", &["aws_vpc", "main"])
        .ok_or_else(|| HclError::CstNodeNotFound("aws_vpc".to_string()))?;
    let removed_first = vpc_block.body.remove_attribute("cidr_block");
    assert!(removed_first.is_some());
    assert_eq!(
        removed_first.as_ref().map(|a| a.name.as_str()),
        Some("cidr_block")
    );
    assert_eq!(vpc_block.body.attributes().len(), 2);
    let rendered_after_first = file.write_to_string();
    assert!(!rendered_after_first.contains("10.0.0.0/16"));
    assert!(rendered_after_first.contains("  enable_dns_hostnames = true"));
    let vpc_block = file
        .block_mut("resource", &["aws_vpc", "main"])
        .ok_or_else(|| HclError::CstNodeNotFound("aws_vpc".to_string()))?;
    let removed_mid = vpc_block.body.remove_attribute("enable_dns_hostnames");
    assert!(removed_mid.is_some());
    let vpc_block = file
        .block_mut("resource", &["aws_vpc", "main"])
        .ok_or_else(|| HclError::CstNodeNotFound("aws_vpc".to_string()))?;
    let removed_last = vpc_block.body.remove_attribute("enable_dns_support");
    assert!(removed_last.is_some());
    assert_eq!(vpc_block.body.items, [] as [CstItem; 0]);
    let rendered_empty_block = file.write_to_string();
    assert!(rendered_empty_block.contains("resource \"aws_vpc\" \"main\" {\n}\n"));
    let removed_block = file.remove_block("resource", &["aws_vpc", "main"]);
    assert!(removed_block.is_some());
    let final_rendered = file.write_to_string();
    assert!(!final_rendered.contains("aws_vpc"));
    assert!(final_rendered.contains("resource \"aws_subnet\" \"sub\" {"));
    let removed_sub = file.remove_block("resource", &["aws_subnet", "sub"]);
    assert!(removed_sub.is_some());
    assert_eq!(file.body().blocks().len(), 0);
    let empty_rendered = file.write_to_string();
    assert_eq!(empty_rendered.trim(), "");
    Ok(())
}
#[test]
fn test_cst_builder_coverage_completion() {
    let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
    let mut attr = CstAttribute::new("test", vec![]);
    attr.trailing
        .push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));
    attr.trailing
        .push(Token::new(TokenKind::Newline, "\n", dummy_span.clone()));
    attr.with_leading_comment("# hash comment\n");
    attr.with_leading_comment("# hash comment no newline");
    attr.with_leading_comment("// slash comment\n");
    attr.with_leading_comment("// slash comment no newline");
    attr.with_leading_comment("/* block comment */\n");
    attr.with_leading_comment("/* block comment */ no newline");
    attr.with_trailing_inline_comment("# inline hash\n");
    attr.with_trailing_inline_comment("// inline slash\n");
    attr.with_trailing_inline_comment("/* inline block */");
    attr.with_trailing_inline_comment("plain text");
    let mut block = CstBlock::new("block", vec![]);
    block
        .trailing
        .push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));
    block
        .trailing
        .push(Token::new(TokenKind::Newline, "\n", dummy_span));
    block.with_leading_comment("# hash\n");
    block.with_leading_comment("# hash no newline");
    block.with_leading_comment("// slash\n");
    block.with_leading_comment("// slash no newline");
    block.with_leading_comment("/* block */\n");
    block.with_leading_comment("/* block */ no newline");
    block.with_leading_comment("plain leading comment");
    block.with_trailing_inline_comment("# inline hash\n");
    block.with_trailing_inline_comment("// inline slash\n");
    block.with_trailing_inline_comment("/* inline block */");
    block.with_trailing_inline_comment("plain text");
}

#[test]
fn test_cst_builder_coverage_completion_trivia() {
    let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
    let mut body_empty = CstBody::new();
    body_empty.compact_trivia();
    assert_eq!(body_empty.items.len(), 0);
    let mut body_only_newlines = CstBody::new();
    body_only_newlines.append_newline();
    body_only_newlines.append_newline();
    body_only_newlines.compact_trivia();
    assert_eq!(body_only_newlines.items.len(), 0);
    let mut body_consec = CstBody::new();
    body_consec
        .items
        .push(CstItem::Attribute(CstAttribute::new("p", vec![])));
    let mut t1_with_ws = TokenStream::new();
    t1_with_ws.push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));
    t1_with_ws.push(Token::new(TokenKind::Newline, "\n", dummy_span.clone()));
    body_consec.items.push(CstItem::Trivia(t1_with_ws));
    body_consec.append_newline();
    body_consec
        .items
        .push(CstItem::Attribute(CstAttribute::new("q", vec![])));
    body_consec.compact_trivia();
    assert_eq!(body_consec.items.len(), 3);
    let mut body = CstBody::with_indent_level(1);
    let mut ws_stream = TokenStream::new();
    ws_stream.push(Token::new(TokenKind::Whitespace, "   ", dummy_span.clone()));
    body.items.push(CstItem::Trivia(ws_stream.clone()));
    body.append_newline();
    let mut comment_stream = TokenStream::new();
    comment_stream.push(Token::new(
        TokenKind::Comment,
        "# keep\n",
        dummy_span.clone(),
    ));
    body.items.push(CstItem::Trivia(comment_stream.clone()));
    body.append_newline();
    body.items.push(CstItem::Trivia(ws_stream.clone()));
    body.items.push(CstItem::Trivia(comment_stream.clone()));
    body.items
        .push(CstItem::Attribute(CstAttribute::new("a", vec![])));
    body.items.push(CstItem::Block(CstBlock::new("b", vec![])));
    body.append_newline();
    body.items.push(CstItem::Trivia(ws_stream));
    body.compact_trivia();
    let mut body_indented_blk = CstBody::with_indent_level(2);
    body_indented_blk
        .items
        .push(CstItem::Block(CstBlock::new("child", vec![])));
    body_indented_blk.compact_trivia();
    let mut body_already_indented = CstBody::with_indent_level(1);
    let mut attr_indented = CstAttribute::new("x", vec![]);
    attr_indented
        .leading
        .push(Token::new(TokenKind::Comment, "# c", dummy_span.clone()));
    attr_indented
        .leading
        .push(Token::new(TokenKind::Whitespace, " ", dummy_span.clone()));
    attr_indented
        .leading
        .push(Token::new(TokenKind::Whitespace, "  ", dummy_span));
    body_already_indented
        .items
        .push(CstItem::Attribute(attr_indented));
    body_already_indented.compact_trivia();
    let mut body_trivia_first = CstBody::with_indent_level(1);
    body_trivia_first
        .items
        .push(CstItem::Trivia(comment_stream));
    body_trivia_first.compact_trivia();
}

#[test]
fn test_cst_builder_coverage_completion_indent_and_search() {
    let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
    let mut body_blk_already_indented = CstBody::with_indent_level(1);
    let mut blk_indented = CstBlock::new("x", vec![]);
    blk_indented
        .leading
        .push(Token::new(TokenKind::Comment, "# c", dummy_span.clone()));
    blk_indented
        .leading
        .push(Token::new(TokenKind::Whitespace, " ", dummy_span.clone()));
    blk_indented
        .leading
        .push(Token::new(TokenKind::Whitespace, "  ", dummy_span.clone()));
    body_blk_already_indented
        .items
        .push(CstItem::Block(blk_indented));
    body_blk_already_indented.compact_trivia();
    let mut body_levels = CstBody::new();
    let mut attr_c = CstAttribute::new("k", vec![]);
    attr_c
        .leading
        .push(Token::new(TokenKind::Comment, "# c\n", dummy_span.clone()));
    attr_c.leading.push(Token::new(
        TokenKind::InlineComment,
        "/* i */",
        dummy_span.clone(),
    ));
    let mut blk_c = CstBlock::new("s", vec![]);
    blk_c
        .leading
        .push(Token::new(TokenKind::Comment, "# c\n", dummy_span.clone()));
    blk_c.leading.push(Token::new(
        TokenKind::InlineComment,
        "/* bi */",
        dummy_span.clone(),
    ));
    body_levels.items.push(CstItem::Attribute(attr_c));
    body_levels.items.push(CstItem::Block(blk_c));
    body_levels.items.push(CstItem::Trivia(TokenStream::new()));
    body_levels.set_indent_level_recursive(0, "  ");
    body_levels.set_indent_level_recursive(2, "  ");
    let mut body_search = CstBody::new();
    body_search.items.push(CstItem::Trivia(TokenStream::new()));
    body_search
        .items
        .push(CstItem::Block(CstBlock::new("dummy", vec![])));
    body_search
        .items
        .push(CstItem::Attribute(CstAttribute::new("other", vec![])));
    body_search
        .items
        .push(CstItem::Attribute(CstAttribute::new("val", vec![])));
    body_search.append_block(CstBlock::new("server", vec!["web".to_string()]));
    assert!(body_search.block_mut("database", &["web"]).is_none());
    assert!(body_search.block_mut("server", &["web", "extra"]).is_none());
    assert!(body_search.block_mut("server", &["api"]).is_none());
    assert!(body_search.block_mut("server", &["web"]).is_some());
    assert!(
        body_search
            .replace_attribute_expr(
                "val",
                &Expression::Number(Number::from(42), dummy_span.clone())
            )
            .is_ok()
    );
    let mut body_attr_only = CstBody::new();
    body_attr_only
        .items
        .push(CstItem::Attribute(CstAttribute::new("orphan", vec![])));
    assert!(body_attr_only.remove_attribute("orphan").is_some());
    assert!(body_attr_only.remove_attribute("absent").is_none());
    let mut body_blk_only = CstBody::new();
    body_blk_only
        .items
        .push(CstItem::Block(CstBlock::new("orphan_blk", vec![])));
    assert!(body_blk_only.remove_block("orphan_blk", &[]).is_some());
    assert!(body_blk_only.remove_block("absent_blk", &[]).is_none());
    assert!(
        body_search
            .replace_attribute_expr("missing", &Expression::Null(dummy_span.clone()))
            .is_err()
    );
    let mut body_rename = CstBody::new();
    body_rename.items.push(CstItem::Trivia(TokenStream::new()));
    let attr_ren = CstAttribute::new(
        "a",
        vec![
            Token::new(TokenKind::Ident, "foo", dummy_span.clone()),
            Token::new(TokenKind::Plus, "+", dummy_span.clone()),
            Token::new(TokenKind::Ident, "bar", dummy_span.clone()),
        ],
    );
    body_rename.items.push(CstItem::Attribute(attr_ren));
    let mut blk_ren = CstBlock::new("res", vec![]);
    let inner_attr = CstAttribute::new("b", vec![Token::new(TokenKind::Ident, "foo", dummy_span)]);
    blk_ren.body.items.push(CstItem::Attribute(inner_attr));
    body_rename.append_block(blk_ren);
    assert_eq!(body_rename.rename_variable("foo", "baz"), 2);
}

#[test]
fn test_cst_builder_coverage_completion_expr_tokens() {
    let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
    let all_bin_ops = [
        BinaryOp::Sub,
        BinaryOp::Mul,
        BinaryOp::Div,
        BinaryOp::Mod,
        BinaryOp::Eq,
        BinaryOp::NotEq,
        BinaryOp::Less,
        BinaryOp::LessEq,
        BinaryOp::Greater,
        BinaryOp::GreaterEq,
        BinaryOp::And,
        BinaryOp::Or,
    ];
    for op in all_bin_ops {
        let expr = Expression::BinaryOp(
            op,
            Box::new(Expression::Number(Number::from(1), dummy_span.clone())),
            Box::new(Expression::Number(Number::from(2), dummy_span.clone())),
            dummy_span.clone(),
        );
        assert_ne!(tokens_for_expression(&expr).len(), 0);
    }
    let neg_expr = Expression::UnaryOp(
        UnaryOp::Neg,
        Box::new(Expression::Number(Number::from(5), dummy_span.clone())),
        dummy_span.clone(),
    );
    assert_eq!(tokens_for_expression(&neg_expr)[0].text, "-");
    let fn_call_expand = Expression::FuncCall(
        Box::new(crate::ast::expr::FuncCall {
            name: "test".into(),
            args: vec![],
            expand_final: true,
        }),
        dummy_span.clone(),
    );
    let fn_toks = tokens_for_expression(&fn_call_expand);
    assert!(fn_toks.iter().any(|t| t.text == "..."));
    let for_tuple = Expression::ForExpr(
        Box::new(crate::ast::expr::ForExpr {
            key_var: Some("k".to_string()),
            val_var: "v".to_string(),
            collection: Box::new(Expression::Variable("list".to_string(), dummy_span.clone())),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("v".to_string(), dummy_span.clone())),
            grouping: true,
            cond_expr: Some(Box::new(Expression::Bool(true, dummy_span.clone()))),
        }),
        dummy_span.clone(),
    );
    let for_toks = tokens_for_expression(&for_tuple);
    assert!(for_toks.iter().any(|t| t.text == "..."));
    assert!(for_toks.iter().any(|t| t.text == " if "));
    let for_obj = Expression::ForExpr(
        Box::new(crate::ast::expr::ForExpr {
            key_var: None,
            val_var: "item".to_string(),
            collection: Box::new(Expression::Variable(
                "items".to_string(),
                dummy_span.clone(),
            )),
            key_expr: Some(Box::new(Expression::Variable(
                "item".to_string(),
                dummy_span.clone(),
            ))),
            val_expr: Box::new(Expression::Variable("item".to_string(), dummy_span.clone())),
            grouping: false,
            cond_expr: None,
        }),
        dummy_span,
    );
    let for_obj_toks = tokens_for_expression(&for_obj);
    assert!(for_obj_toks.iter().any(|t| t.text == " => "));
}

#[test]
fn test_cst_builder_coverage_completion_directives() {
    let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
    let dir_if = crate::ast::expr::Directive::If {
        cond: Expression::Bool(true, dummy_span.clone()),
        true_expr: vec![TemplatePart::Literal("T".into(), dummy_span.clone())],
        else_ifs: vec![(
            Expression::Bool(false, dummy_span.clone()),
            vec![TemplatePart::Literal("EI".into(), dummy_span.clone())],
        )],
        false_expr: Some(vec![TemplatePart::Literal("F".into(), dummy_span.clone())]),
    };
    let mut dir_if_str = String::new();
    format_directive(&dir_if, &mut dir_if_str);
    assert!(dir_if_str.contains("%{else if"));
    assert!(dir_if_str.contains("%{else}"));
    let dir_if_no_else = crate::ast::expr::Directive::If {
        cond: Expression::Bool(true, dummy_span.clone()),
        true_expr: vec![],
        else_ifs: vec![],
        false_expr: None,
    };
    let mut dir_if_no_else_str = String::new();
    format_directive(&dir_if_no_else, &mut dir_if_no_else_str);
    assert!(!dir_if_no_else_str.contains("%{else}"));
    let dir_for = crate::ast::expr::Directive::For {
        key_var: Some("k".to_string()),
        val_var: "v".to_string(),
        collection: Expression::Variable("arr".to_string(), dummy_span.clone()),
        body: vec![TemplatePart::Interpolation(
            Expression::Variable("v".to_string(), dummy_span.clone()),
            dummy_span.clone(),
        )],
    };
    let mut dir_for_str = String::new();
    format_directive(&dir_for, &mut dir_for_str);
    assert!(dir_for_str.contains("%{for k, v in arr}"));
    let dir_for_no_key = crate::ast::expr::Directive::For {
        key_var: None,
        val_var: "elem".to_string(),
        collection: Expression::Variable("arr".to_string(), dummy_span.clone()),
        body: vec![],
    };
    let mut dir_for_no_key_str = String::new();
    format_directive(&dir_for_no_key, &mut dir_for_no_key_str);
    assert!(dir_for_no_key_str.contains("%{for elem in arr}"));
    let mut strip_all = String::new();
    format_directive(
        &crate::ast::expr::Directive::Strip {
            strip_left: true,
            strip_right: true,
        },
        &mut strip_all,
    );
    assert_eq!(strip_all, "%{~~}");
    let mut strip_none = String::new();
    format_directive(
        &crate::ast::expr::Directive::Strip {
            strip_left: false,
            strip_right: false,
        },
        &mut strip_none,
    );
    assert_eq!(strip_none, "%{}");
    let tmpl_expr = Expression::Template(
        vec![
            TemplatePart::Literal("hello ".into(), dummy_span.clone()),
            TemplatePart::Directive(dir_for, dummy_span.clone()),
        ],
        dummy_span,
    );
    let tmpl_toks = tokens_for_expression(&tmpl_expr);
    assert_eq!(tmpl_toks[0].kind, TokenKind::String);
}

#[test]
fn test_cst_builder_coverage_completion_values_and_doc() {
    let dummy_span = Span::new(0, 0, 0, 0, 0, 0);
    assert_eq!(
        value_to_tokens(&Value::new(Type::Bool, ValueData::Bool(false)), &dummy_span)[0].text,
        "false"
    );
    assert_eq!(
        value_to_tokens(&Value::unknown(Type::Dynamic), &dummy_span)[0].text,
        "unknown"
    );
    assert_eq!(
        value_to_tokens(&Value::capsule("test_capsule", 42), &dummy_span)[0].text,
        "capsule"
    );
    assert_eq!(
        value_to_tokens(
            &Value::new(Type::List(Box::new(Type::Number)), ValueData::Array(vec![])),
            &dummy_span
        )
        .len(),
        2
    );
    assert_eq!(
        value_to_tokens(
            &Value::new(
                Type::Set(Box::new(Type::Number)),
                ValueData::Set(std::collections::BTreeSet::new())
            ),
            &dummy_span
        )
        .len(),
        2
    );
    assert_eq!(
        value_to_tokens(
            &Value::new(
                Type::object(std::collections::BTreeMap::new()),
                ValueData::Object(std::collections::BTreeMap::new())
            ),
            &dummy_span
        )
        .len(),
        3
    );
    let mut doc = Document::new();
    let attr_doc = crate::cst::document::Attribute {
        name: Token::new(TokenKind::Ident, "timeout", dummy_span.clone()),
        equals_leading: crate::cst::document::TokenStream::new(),
        equals: Token::new(TokenKind::Assign, "=", dummy_span.clone()),
        expr_tokens: vec![Token::new(TokenKind::Number, "10", dummy_span.clone())],
        leading: crate::cst::document::TokenStream::new(),
        trailing: crate::cst::document::TokenStream::new(),
    };
    let blk_doc = crate::cst::document::Block {
        type_ident: Token::new(TokenKind::Ident, "service", dummy_span.clone()),
        labels: vec![
            Token::new(TokenKind::String, "\"quoted\"", dummy_span.clone()),
            Token::new(TokenKind::Ident, "unquoted", dummy_span.clone()),
        ],
        open_brace: Token::new(TokenKind::OBrace, "{", dummy_span.clone()),
        body: Box::new(Document::new()),
        close_brace: Token::new(TokenKind::CBrace, "}", dummy_span.clone()),
        leading: crate::cst::document::TokenStream::new(),
        trailing: crate::cst::document::TokenStream::new(),
    };
    doc.items.push(DocumentItem::Attribute(attr_doc));
    doc.append_block(blk_doc);
    let parsed_doc_body = from_document(&doc);
    assert_eq!(parsed_doc_body.body().attributes().len(), 1);
    assert_eq!(parsed_doc_body.body().blocks().len(), 1);
    assert_eq!(
        parsed_doc_body.body().blocks()[0].labels,
        vec!["quoted", "unquoted"]
    );
    let mut body_raw = CstBody::with_indent_level(1);
    body_raw.set_attribute_raw(
        "new_key",
        vec![Token::new(TokenKind::Number, "1", dummy_span.clone())],
    );
    assert_eq!(body_raw.attributes().len(), 1);
    body_raw.set_attribute_raw(
        "new_key",
        vec![Token::new(TokenKind::Number, "2", dummy_span)],
    );
    assert_eq!(body_raw.attributes()[0].expr_tokens[0].text, "2");
}
