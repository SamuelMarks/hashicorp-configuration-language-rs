//! Comprehensive unit tests for legacy HCL 1.0 parsing, lexing, and migration.

use crate::ast::expr::Expression;
use crate::hcl1::lex::Hcl1Lexer;
use crate::hcl1::migrate::migrate_hcl1_to_hcl2;
use crate::hcl1::parser::Hcl1Parser;

#[test]
fn test_hcl1_lexer_basics() {
    let input = r#"
        # Line comment
        // C-style line comment
        /* Multi-line
           comment */
        variable "name" = "default_value"
        count = 42
        negative = -10
        active = true
        empty = null
    "#;

    let mut lexer = Hcl1Lexer::new(input);
    let tokens = lexer.tokenize().expect("tokenization succeeds");
    assert_ne!(tokens.len(), 0);
}

#[test]
fn test_hcl1_lexer_heredoc() {
    let input = "data = <<EOF
line 1
line 2
EOF
";
    let mut lexer = Hcl1Lexer::new(input);
    let tokens = lexer.tokenize().expect("tokenization succeeds");
    assert!(tokens.iter().any(|t| t.text.contains(
        "line 1
line 2"
    )));

    // Indented heredoc
    let input_indented = "data = <<-EOF
    indented line
  EOF
";
    let mut lexer_indented = Hcl1Lexer::new(input_indented);
    let tokens_indented = lexer_indented.tokenize().expect("tokenization succeeds");
    assert!(
        tokens_indented
            .iter()
            .any(|t| t.text.contains("indented line"))
    );
}

#[test]
fn test_hcl1_lexer_errors() {
    // Unterminated string
    let mut lexer_unterm = Hcl1Lexer::new("\"unclosed string");
    assert!(lexer_unterm.tokenize().is_err());

    // Invalid heredoc
    let mut lexer_bad_hero = Hcl1Lexer::new("<<");
    assert!(lexer_bad_hero.tokenize().is_err());

    // Unterminated heredoc
    let mut lexer_unterm_hero = Hcl1Lexer::new(
        "<<EOF
some content
",
    );
    assert!(lexer_unterm_hero.tokenize().is_err());
}

#[test]
fn test_hcl1_parser_terraform_011_config() {
    let input = r#"
        # Legacy Terraform 0.11 syntax: block assignment with '=' and comma-less lists
        resource "aws_instance" "web" = {
            ami = "${var.ami_id}"
            count = 3
            instance_type = "t2.micro"

            tags = ["web" "production" "frontend"]

            ebs_block_device {
                device_name = "/dev/sda1"
                volume_size = 50
            }
        }
    "#;

    let mut lexer = Hcl1Lexer::new(input);
    let tokens = lexer.tokenize().expect("tokenize succeeds");
    let mut parser = Hcl1Parser::new(tokens);
    let hcl1_body = parser.parse_body().expect("parse succeeds");

    assert_eq!(hcl1_body.items.len(), 1);

    // Migrate to HCL2
    let (hcl2_body, diags) = migrate_hcl1_to_hcl2(&hcl1_body).expect("migration succeeds");

    // Verify block migration
    assert_eq!(hcl2_body.blocks.len(), 1);
    let block = &hcl2_body.blocks[0];
    assert_eq!(block.block_type, "resource");
    assert_eq!(block.labels, &["aws_instance", "web"]);

    // Verify comma-less list migrated to Tuple
    let tags_attr = block.body.attributes.get("tags").expect("tags exists");
    match &tags_attr.expr {
        Expression::Tuple(elements, _) => {
            assert_eq!(elements.len(), 3);
        }
        other => panic!("expected Tuple, got {other:?}"),
    }

    // Verify unescaped interpolation migrated to native expression
    let ami_attr = block.body.attributes.get("ami").expect("ami exists");
    match &ami_attr.expr {
        Expression::Traversal(t, _) => {
            match t.expr.as_ref() {
                Expression::Variable(name, _) => assert_eq!(name, "var"),
                other => panic!("expected Variable, got {other:?}"),
            }
            assert_eq!(t.operators.len(), 1);
            match &t.operators[0] {
                crate::ast::expr::TraversalOperator::GetAttr(name, _) => {
                    assert_eq!(name, "ami_id");
                }
                other => panic!("expected GetAttr, got {other:?}"),
            }
        }
        other => panic!("expected Traversal, got {other:?}"),
    }

    // Verify migration diagnostics
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("Legacy block assignment"))
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("Interpolated expression"))
    );
}

#[test]
fn test_hcl1_parser_packer_template() {
    let input = r#"
        variable "aws_region" {
            default = "us-west-2"
        }

        builders = [
            {
                type = "amazon-ebs"
                region = "${var.aws_region}"
                ssh_username = "ubuntu"
            }
        ]
    "#;

    let mut lexer = Hcl1Lexer::new(input);
    let tokens = lexer.tokenize().expect("tokenize succeeds");
    let mut parser = Hcl1Parser::new(tokens);
    let hcl1_body = parser.parse_body().expect("parse succeeds");

    let (hcl2_body, diags) = migrate_hcl1_to_hcl2(&hcl1_body).expect("migration succeeds");

    assert_eq!(hcl2_body.blocks.len(), 1);
    assert_eq!(hcl2_body.blocks[0].block_type, "variable");
    assert_eq!(hcl2_body.blocks[0].labels, &["aws_region"]);
    assert!(hcl2_body.attributes.contains_key("builders"));
    assert_ne!(diags.len(), 0);
}

#[test]
fn test_hcl1_parser_errors() {
    // Unexpected EOF
    let mut parser_empty = Hcl1Parser::new(vec![]);
    assert!(parser_empty.parse_body().is_ok());

    // Invalid start token
    let bad_tokens = vec![crate::hcl1::token::Hcl1Token::new(
        crate::hcl1::token::Hcl1TokenKind::Comma,
        ",",
        crate::span::Span::new(0, 1, 1, 1, 1, 2),
    )];
    let mut parser_bad = Hcl1Parser::new(bad_tokens);
    assert!(parser_bad.parse_body().is_err());
}

#[test]
fn test_hcl1_lexer_exhaustive_coverage() {
    // Punctuation tokens: ':' ',' '.' '(' ')'
    // Identifier starting with '_'
    // String escapes: \n, \t, \r, \\, \", \z
    let input = "_ident . : [1, 2.3] (bar) \"hello\\n\\t\\r\\\\\\\"\\z\"";
    let mut lexer = Hcl1Lexer::new(input);
    let res = lexer.tokenize();
    assert!(res.is_ok());

    // String ending with backslash before EOF
    let mut lexer_slash_eof = Hcl1Lexer::new("\"foo\\");
    assert!(lexer_slash_eof.tokenize().is_err());

    // Unknown character error
    let mut lexer_unknown = Hcl1Lexer::new("@invalid");
    assert!(lexer_unknown.tokenize().is_err());

    // Heredoc ending immediately without newline after marker
    let mut lexer_heredoc_eof = Hcl1Lexer::new("<<EOF");
    assert!(lexer_heredoc_eof.tokenize().is_err());

    // Heredoc with trailing spaces before newline
    let input_heredoc_spaces = "<<EOF   \nline\nEOF\n";
    let mut lexer_heredoc_spaces = Hcl1Lexer::new(input_heredoc_spaces);
    assert!(lexer_heredoc_spaces.tokenize().is_ok());

    // Whitespace starting with tab and carriage return, and boolean false
    let mut lexer_ws = Hcl1Lexer::new("\t\r false");
    let ws_tokens = lexer_ws.tokenize();
    assert!(ws_tokens.is_ok());

    let mut lexer_ws_cr = Hcl1Lexer::new("\r\t ");
    assert!(lexer_ws_cr.tokenize().is_ok());

    // Comments at EOF without trailing newline
    let mut lexer_hash_eof = Hcl1Lexer::new("# comment at eof");
    assert!(lexer_hash_eof.tokenize().is_ok());

    let mut lexer_slash_slash_eof = Hcl1Lexer::new("// comment at eof");
    assert!(lexer_slash_slash_eof.tokenize().is_ok());

    // Block comment at EOF without closing delimiter
    let mut lexer_star_eof = Hcl1Lexer::new("/* unclosed star comment");
    assert!(lexer_star_eof.tokenize().is_ok());

    // Block comment with asterisk not followed by slash
    let mut lexer_star_text = Hcl1Lexer::new("/* * not slash */");
    assert!(lexer_star_text.tokenize().is_ok());

    // Numbers: negative numbers, scientific notation with e, E, +, -
    let mut lexer_nums = Hcl1Lexer::new("-42 1e10 2.5E+3 3.14e-2");
    assert!(lexer_nums.tokenize().is_ok());

    // Minus not followed by digit
    let mut lexer_minus_non_digit = Hcl1Lexer::new("- not_a_digit");
    assert!(lexer_minus_non_digit.tokenize().is_err());

    let mut lexer_minus_eof = Hcl1Lexer::new("-");
    assert!(lexer_minus_eof.tokenize().is_err());

    // Heredoc EOF immediately after <<
    let mut lexer_heredoc_bare_lt = Hcl1Lexer::new("<<");
    assert!(lexer_heredoc_bare_lt.tokenize().is_err());

    // Single slash not followed by / or *
    let mut lexer_single_slash = Hcl1Lexer::new("/invalid");
    assert!(lexer_single_slash.tokenize().is_err());

    // Single less-than not followed by <
    let mut lexer_single_lt = Hcl1Lexer::new("<invalid");
    assert!(lexer_single_lt.tokenize().is_err());

    // Identifier containing hyphen
    let mut lexer_hyphen_ident = Hcl1Lexer::new("my-custom-ident");
    assert!(lexer_hyphen_ident.tokenize().is_ok());

    // Heredoc marker containing underscore
    let input_heredoc_underscore = "<<MY_MARKER\nhello\nMY_MARKER\n";
    let mut lexer_heredoc_underscore = Hcl1Lexer::new(input_heredoc_underscore);
    assert!(lexer_heredoc_underscore.tokenize().is_ok());
}

#[test]
fn test_hcl1_parser_exhaustive_coverage() {
    let span = crate::span::Span::new(0, 0, 1, 1, 1, 1);

    // Hcl1Expression::span() on all variants
    let str_expr = crate::hcl1::parser::Hcl1Expression::String("a".into(), span.clone());
    let num_expr = crate::hcl1::parser::Hcl1Expression::Number("1".into(), span.clone());
    let bool_expr = crate::hcl1::parser::Hcl1Expression::Bool(true, span.clone());
    let null_expr = crate::hcl1::parser::Hcl1Expression::Null(span.clone());
    let var_expr = crate::hcl1::parser::Hcl1Expression::Variable("v".into(), span.clone());
    let list_expr = crate::hcl1::parser::Hcl1Expression::List(vec![], span.clone());
    let map_expr = crate::hcl1::parser::Hcl1Expression::Map(vec![], span.clone());

    assert_eq!(str_expr.span(), span);
    assert_eq!(num_expr.span(), span);
    assert_eq!(bool_expr.span(), span);
    assert_eq!(null_expr.span(), span);
    assert_eq!(var_expr.span(), span);
    assert_eq!(list_expr.span(), span);
    assert_eq!(map_expr.span(), span);

    // parse_item on empty parser
    let mut p_empty = Hcl1Parser::new(vec![]);
    assert!(p_empty.parse_item().is_err());

    // Valid parses for bools, null, variable, comma lists, colon/equals maps
    let input_valid = r#"
        b_true = true
        b_false = false
        n_null = null
        v_var = my_var
        list_trailing = [1, 2, 3,]
        map_mixed = {
            key1: 1,
            key2 = "two",
            key3 "three",
        }
        "quoted_attr" = "val"
        custom_block ident_label1 ident_label2 {
            nested = 1
        }
    "#;
    let mut lexer = Hcl1Lexer::new(input_valid);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => panic!("lex failed: {e:?}"),
    };
    let mut parser = Hcl1Parser::new(tokens);
    let body = match parser.parse_body() {
        Ok(b) => b,
        Err(e) => panic!("parse failed: {e:?}"),
    };
    assert_eq!(body.items.len(), 8);

    // Specific error scenarios:
    // 1. unexpected EOF after identifier
    let mut p1 = Hcl1Parser::new(Hcl1Lexer::new("foo").tokenize().unwrap_or_default());
    assert!(p1.parse_body().is_err());

    // 2. unexpected token after identifier: 'foo 123 {'
    let mut p2 = Hcl1Parser::new(Hcl1Lexer::new("foo 123 {").tokenize().unwrap_or_default());
    assert!(p2.parse_body().is_err());

    // 3. unexpected EOF while expecting expression: 'foo = '
    let mut p3 = Hcl1Parser::new(Hcl1Lexer::new("foo = ").tokenize().unwrap_or_default());
    assert!(p3.parse_body().is_err());

    // 4. block parse_body error: 'foo { = }'
    let mut p4 = Hcl1Parser::new(Hcl1Lexer::new("foo { = }").tokenize().unwrap_or_default());
    assert!(p4.parse_body().is_err());

    // 5. unclosed block: 'foo {'
    let mut p5 = Hcl1Parser::new(Hcl1Lexer::new("foo {").tokenize().unwrap_or_default());
    assert!(p5.parse_body().is_err());

    // 6. unclosed legacy block assignment: 'resource "aws" = {'
    let mut p6 = Hcl1Parser::new(
        Hcl1Lexer::new("resource \"aws\" = {")
            .tokenize()
            .unwrap_or_default(),
    );
    assert!(p6.parse_body().is_err());

    // 7. unexpected token in expression: 'foo = }'
    let mut p7 = Hcl1Parser::new(Hcl1Lexer::new("foo = }").tokenize().unwrap_or_default());
    assert!(p7.parse_body().is_err());

    // 8. unclosed list: 'foo = ['
    let mut p8 = Hcl1Parser::new(Hcl1Lexer::new("foo = [").tokenize().unwrap_or_default());
    assert!(p8.parse_body().is_err());

    // 8b. list element expression error: 'foo = [ 1, } ]'
    let mut p8b = Hcl1Parser::new(
        Hcl1Lexer::new("foo = [ 1, } ]")
            .tokenize()
            .unwrap_or_default(),
    );
    assert!(p8b.parse_body().is_err());

    // 9. unclosed map: 'foo = {'
    let mut p9 = Hcl1Parser::new(Hcl1Lexer::new("foo = {").tokenize().unwrap_or_default());
    assert!(p9.parse_body().is_err());

    // 10. unexpected EOF inside map: 'foo = { a '
    let mut p10 = Hcl1Parser::new(Hcl1Lexer::new("foo = { a ").tokenize().unwrap_or_default());
    assert!(p10.parse_body().is_err());

    // 11. expression error inside map: 'foo = { a = }'
    let mut p11 = Hcl1Parser::new(
        Hcl1Lexer::new("foo = { a = }")
            .tokenize()
            .unwrap_or_default(),
    );
    assert!(p11.parse_body().is_err());
}

#[test]
fn test_hcl1_migrate_exhaustive_coverage() {
    let span = crate::span::Span::new(0, 0, 1, 1, 1, 1);

    // 1. Attribute with Map value
    let input_map_attr = r#"
        config = {
            host = "localhost"
            port = 8080
        }
    "#;
    let tokens = Hcl1Lexer::new(input_map_attr)
        .tokenize()
        .unwrap_or_default();
    let body = match Hcl1Parser::new(tokens).parse_body() {
        Ok(b) => b,
        Err(_) => crate::hcl1::parser::Hcl1Body {
            items: vec![],
            span: span.clone(),
        },
    };
    let (migrated, diags) = match migrate_hcl1_to_hcl2(&body) {
        Ok(res) => res,
        Err(e) => panic!("migration failed: {e:?}"),
    };
    assert!(migrated.attributes.contains_key("config"));
    assert_eq!(diags, [] as [crate::hcl1::migrate::MigrationDiagnostic; 0]);

    // 2. Null, Bool, Variable, and Template strings
    let input_primitives = r#"
        is_null = null
        is_bool = true
        is_var = other_setting
        mixed_template = "prefix ${var.foo} suffix"
        invalid_expr_template = "${+++}"
        starts_with_not_ends = "${var.foo} suffix"
        starts_and_ends_multiple = "${var.foo} and ${var.bar}"
    "#;
    let tokens_p = Hcl1Lexer::new(input_primitives)
        .tokenize()
        .unwrap_or_default();
    let body_p = match Hcl1Parser::new(tokens_p).parse_body() {
        Ok(b) => b,
        Err(_) => crate::hcl1::parser::Hcl1Body {
            items: vec![],
            span: span.clone(),
        },
    };
    let (migrated_p, diags_p) = match migrate_hcl1_to_hcl2(&body_p) {
        Ok(res) => res,
        Err(e) => panic!("migration failed: {e:?}"),
    };
    assert!(migrated_p.attributes.contains_key("is_null"));
    assert!(migrated_p.attributes.contains_key("is_bool"));
    assert!(migrated_p.attributes.contains_key("is_var"));
    assert!(migrated_p.attributes.contains_key("mixed_template"));
    assert!(migrated_p.attributes.contains_key("invalid_expr_template"));
    assert!(diags_p.iter().any(|d| d.message.contains("template")));

    // 3. Error branches in migrate (invalid Number literal in various AST positions)
    let bad_num = crate::hcl1::parser::Hcl1Expression::Number("invalid_num".into(), span.clone());

    // 3a. Attribute with invalid number
    let body_bad_attr = crate::hcl1::parser::Hcl1Body {
        items: vec![crate::hcl1::parser::Hcl1Item::Attribute(
            crate::hcl1::parser::Hcl1Attribute {
                name: "bad".into(),
                expr: bad_num.clone(),
                span: span.clone(),
            },
        )],
        span: span.clone(),
    };
    assert!(migrate_hcl1_to_hcl2(&body_bad_attr).is_err());

    // 3b. List with invalid number
    let body_bad_list = crate::hcl1::parser::Hcl1Body {
        items: vec![crate::hcl1::parser::Hcl1Item::Attribute(
            crate::hcl1::parser::Hcl1Attribute {
                name: "bad_list".into(),
                expr: crate::hcl1::parser::Hcl1Expression::List(
                    vec![bad_num.clone()],
                    span.clone(),
                ),
                span: span.clone(),
            },
        )],
        span: span.clone(),
    };
    assert!(migrate_hcl1_to_hcl2(&body_bad_list).is_err());

    // 3c. Map with invalid number
    let body_bad_map = crate::hcl1::parser::Hcl1Body {
        items: vec![crate::hcl1::parser::Hcl1Item::Attribute(
            crate::hcl1::parser::Hcl1Attribute {
                name: "bad_map".into(),
                expr: crate::hcl1::parser::Hcl1Expression::Map(
                    vec![("k".into(), bad_num.clone())],
                    span.clone(),
                ),
                span: span.clone(),
            },
        )],
        span: span.clone(),
    };
    assert!(migrate_hcl1_to_hcl2(&body_bad_map).is_err());

    // 3d. Block with invalid number
    let body_bad_block = crate::hcl1::parser::Hcl1Body {
        items: vec![crate::hcl1::parser::Hcl1Item::Block(
            crate::hcl1::parser::Hcl1Block {
                block_type: "b".into(),
                labels: vec![],
                body: body_bad_attr,
                has_equals_assign: false,
                span: span.clone(),
            },
        )],
        span,
    };
    assert!(migrate_hcl1_to_hcl2(&body_bad_block).is_err());
}
