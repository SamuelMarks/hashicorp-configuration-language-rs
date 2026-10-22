//! Comprehensive unit tests for legacy HCL 1.0 parsing, lexing, and migration.
use crate::ast::expr::Expression;
use crate::hcl1::lex::Hcl1Lexer;
use crate::hcl1::migrate::migrate_hcl1_to_hcl2;
use crate::hcl1::parser::Hcl1Parser;
#[test]
fn test_hcl1_lexer_basics() -> Result<(), Box<dyn std::error::Error>> {
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
    let tokens = lexer.tokenize()?;
    assert_ne!(tokens.len(), 0);
    Ok(())
}
#[test]
fn test_hcl1_lexer_heredoc() -> Result<(), Box<dyn std::error::Error>> {
    let input = "data = <<EOF
line 1
line 2
EOF
";
    let mut lexer = Hcl1Lexer::new(input);
    let tokens = lexer.tokenize()?;
    assert!(tokens.iter().any(|t| t.text.contains(
        "line 1
line 2"
    )));
    let input_indented = "data = <<-EOF
    indented line
  EOF
";
    let mut lexer_indented = Hcl1Lexer::new(input_indented);
    let tokens_indented = lexer_indented.tokenize()?;
    assert!(
        tokens_indented
            .iter()
            .any(|t| t.text.contains("indented line"))
    );
    Ok(())
}
#[test]
fn test_hcl1_lexer_errors() {
    let mut lexer_unterm = Hcl1Lexer::new(r#""unclosed string"#);
    assert!(lexer_unterm.tokenize().is_err());
    let mut lexer_bad_hero = Hcl1Lexer::new("<<");
    assert!(lexer_bad_hero.tokenize().is_err());
    let mut lexer_unterm_hero = Hcl1Lexer::new(
        "<<EOF
some content
",
    );
    assert!(lexer_unterm_hero.tokenize().is_err());
}
#[test]
fn test_hcl1_parser_terraform_011_config() -> Result<(), Box<dyn std::error::Error>> {
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
    let tokens = lexer.tokenize()?;
    let mut parser = Hcl1Parser::new(tokens);
    let hcl1_body = parser.parse_body()?;
    assert_eq!(hcl1_body.items.len(), 1);
    let (hcl2_body, diags) = migrate_hcl1_to_hcl2(&hcl1_body)?;
    assert_eq!(hcl2_body.blocks.len(), 1);
    let block = &hcl2_body.blocks[0];
    assert_eq!(block.block_type, "resource");
    assert_eq!(block.labels, &["aws_instance", "web"]);
    let tags_attr = block.body.attributes.get("tags").ok_or("missing tags")?;
    assert!(matches!(
        &tags_attr.expr,
        Expression::Tuple(elements, _) if elements.len() == 3
    ));
    let ami_attr = block.body.attributes.get("ami").ok_or("missing ami")?;
    assert!(matches!(
        &ami_attr.expr,
        Expression::Traversal(t, _)
            if matches!(t.expr.as_ref(), Expression::Variable(name, _) if name == "var")
                && t.operators.len() == 1
                && matches!(
                    &t.operators[0],
                    crate::ast::expr::TraversalOperator::GetAttr(name, _) if name == "ami_id"
                )
    ));
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
    Ok(())
}
#[test]
fn test_hcl1_parser_packer_template() -> Result<(), Box<dyn std::error::Error>> {
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
    let tokens = lexer.tokenize()?;
    let mut parser = Hcl1Parser::new(tokens);
    let hcl1_body = parser.parse_body()?;
    let (hcl2_body, diags) = migrate_hcl1_to_hcl2(&hcl1_body)?;
    assert_eq!(hcl2_body.blocks.len(), 1);
    assert_eq!(hcl2_body.blocks[0].block_type, "variable");
    assert_eq!(hcl2_body.blocks[0].labels, &["aws_region"]);
    assert!(hcl2_body.attributes.contains_key("builders"));
    assert_ne!(diags.len(), 0);
    Ok(())
}
#[test]
fn test_hcl1_parser_errors() {
    let mut parser_empty = Hcl1Parser::new(vec![]);
    assert!(parser_empty.parse_body().is_ok());
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
    let bs = char::from(92);
    let tab = char::from(9);
    let cr = char::from(13);
    let input = format!(r#"_ident . : [1, 2.3] (bar) "hello{bs}n{bs}t{bs}r{bs}{bs}{bs}"{bs}z""#);
    let mut lexer = Hcl1Lexer::new(&input);
    let res = lexer.tokenize();
    assert!(res.is_ok());
    let slash_eof = format!(r#""foo{bs}"#);
    let mut lexer_slash_eof = Hcl1Lexer::new(&slash_eof);
    assert!(lexer_slash_eof.tokenize().is_err());
    let mut lexer_unknown = Hcl1Lexer::new("@invalid");
    assert!(lexer_unknown.tokenize().is_err());
    let mut lexer_heredoc_eof = Hcl1Lexer::new("<<EOF");
    assert!(lexer_heredoc_eof.tokenize().is_err());
    let input_heredoc_spaces = "<<EOF   
line
EOF
";
    let mut lexer_heredoc_spaces = Hcl1Lexer::new(input_heredoc_spaces);
    assert!(lexer_heredoc_spaces.tokenize().is_ok());
    let ws = format!("{tab}{cr} false");
    let mut lexer_ws = Hcl1Lexer::new(&ws);
    let ws_tokens = lexer_ws.tokenize();
    assert!(ws_tokens.is_ok());
    let ws_cr = format!("{cr}{tab} ");
    let mut lexer_ws_cr = Hcl1Lexer::new(&ws_cr);
    assert!(lexer_ws_cr.tokenize().is_ok());
    let mut lexer_hash_eof = Hcl1Lexer::new("# comment at eof");
    assert!(lexer_hash_eof.tokenize().is_ok());
    let mut lexer_slash_slash_eof = Hcl1Lexer::new("// comment at eof");
    assert!(lexer_slash_slash_eof.tokenize().is_ok());
    let mut lexer_star_eof = Hcl1Lexer::new("/* unclosed star comment");
    assert!(lexer_star_eof.tokenize().is_ok());
    let mut lexer_star_text = Hcl1Lexer::new("/* * not slash */");
    assert!(lexer_star_text.tokenize().is_ok());
    let mut lexer_nums = Hcl1Lexer::new("-42 1e10 2.5E+3 3.14e-2");
    assert!(lexer_nums.tokenize().is_ok());
    let mut lexer_minus_non_digit = Hcl1Lexer::new("- not_a_digit");
    assert!(lexer_minus_non_digit.tokenize().is_err());
    let mut lexer_minus_eof = Hcl1Lexer::new("-");
    assert!(lexer_minus_eof.tokenize().is_err());
    let mut lexer_heredoc_bare_lt = Hcl1Lexer::new("<<");
    assert!(lexer_heredoc_bare_lt.tokenize().is_err());
    let mut lexer_single_slash = Hcl1Lexer::new("/invalid");
    assert!(lexer_single_slash.tokenize().is_err());
    let mut lexer_single_lt = Hcl1Lexer::new("<invalid");
    assert!(lexer_single_lt.tokenize().is_err());
    let mut lexer_hyphen_ident = Hcl1Lexer::new("my-custom-ident");
    assert!(lexer_hyphen_ident.tokenize().is_ok());
    let input_heredoc_underscore = "<<MY_MARKER
hello
MY_MARKER
";
    let mut lexer_heredoc_underscore = Hcl1Lexer::new(input_heredoc_underscore);
    assert!(lexer_heredoc_underscore.tokenize().is_ok());
}
#[test]
fn test_hcl1_parser_exhaustive_coverage() -> Result<(), Box<dyn std::error::Error>> {
    let span = crate::span::Span::new(0, 0, 1, 1, 1, 1);
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
    let mut p_empty = Hcl1Parser::new(vec![]);
    assert!(p_empty.parse_item().is_err());
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
    let tokens = lexer.tokenize()?;
    let mut parser = Hcl1Parser::new(tokens);
    let body = parser.parse_body()?;
    assert_eq!(body.items.len(), 8);
    let mut p1 = Hcl1Parser::new(Hcl1Lexer::new("foo").tokenize().unwrap_or_default());
    assert!(p1.parse_body().is_err());
    let mut p2 = Hcl1Parser::new(Hcl1Lexer::new("foo 123 {").tokenize().unwrap_or_default());
    assert!(p2.parse_body().is_err());
    let mut p3 = Hcl1Parser::new(Hcl1Lexer::new("foo = ").tokenize().unwrap_or_default());
    assert!(p3.parse_body().is_err());
    let mut p4 = Hcl1Parser::new(Hcl1Lexer::new("foo { = }").tokenize().unwrap_or_default());
    assert!(p4.parse_body().is_err());
    let mut p5 = Hcl1Parser::new(Hcl1Lexer::new("foo {").tokenize().unwrap_or_default());
    assert!(p5.parse_body().is_err());
    let mut p6 = Hcl1Parser::new(
        Hcl1Lexer::new(r#"resource "aws" = {"#)
            .tokenize()
            .unwrap_or_default(),
    );
    assert!(p6.parse_body().is_err());
    let mut p7 = Hcl1Parser::new(Hcl1Lexer::new("foo = }").tokenize().unwrap_or_default());
    assert!(p7.parse_body().is_err());
    let mut parser_unclosed_list =
        Hcl1Parser::new(Hcl1Lexer::new("foo = [").tokenize().unwrap_or_default());
    assert!(parser_unclosed_list.parse_body().is_err());
    let mut parser_bad_list_elem = Hcl1Parser::new(
        Hcl1Lexer::new("foo = [ 1, } ]")
            .tokenize()
            .unwrap_or_default(),
    );
    assert!(parser_bad_list_elem.parse_body().is_err());
    let mut parser_unclosed_map =
        Hcl1Parser::new(Hcl1Lexer::new("foo = {").tokenize().unwrap_or_default());
    assert!(parser_unclosed_map.parse_body().is_err());
    let mut parser_map_key_eof =
        Hcl1Parser::new(Hcl1Lexer::new("foo = { a ").tokenize().unwrap_or_default());
    assert!(parser_map_key_eof.parse_body().is_err());
    let mut parser_map_no_val = Hcl1Parser::new(
        Hcl1Lexer::new("foo = { a = }")
            .tokenize()
            .unwrap_or_default(),
    );
    assert!(parser_map_no_val.parse_body().is_err());
    Ok(())
}
#[test]
fn test_hcl1_migrate_exhaustive_coverage() -> Result<(), Box<dyn std::error::Error>> {
    let span = crate::span::Span::new(0, 0, 1, 1, 1, 1);
    let input_map_attr = r#"
        config = {
            host = "localhost"
            port = 8080
        }
    "#;
    let tokens = Hcl1Lexer::new(input_map_attr)
        .tokenize()
        .unwrap_or_default();
    let body =
        Hcl1Parser::new(tokens)
            .parse_body()
            .unwrap_or_else(|_| crate::hcl1::parser::Hcl1Body {
                items: vec![],
                span: span.clone(),
            });
    let (migrated, diags) = migrate_hcl1_to_hcl2(&body)?;
    assert!(migrated.attributes.contains_key("config"));
    assert_eq!(diags, [] as [crate::hcl1::migrate::MigrationDiagnostic; 0]);
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
    let body_p =
        Hcl1Parser::new(tokens_p)
            .parse_body()
            .unwrap_or_else(|_| crate::hcl1::parser::Hcl1Body {
                items: vec![],
                span: span.clone(),
            });
    let (migrated_p, diags_p) = migrate_hcl1_to_hcl2(&body_p)?;
    assert!(migrated_p.attributes.contains_key("is_null"));
    assert!(migrated_p.attributes.contains_key("is_bool"));
    assert!(migrated_p.attributes.contains_key("is_var"));
    assert!(migrated_p.attributes.contains_key("mixed_template"));
    assert!(migrated_p.attributes.contains_key("invalid_expr_template"));
    assert!(diags_p.iter().any(|d| d.message.contains("template")));
    let bad_num = crate::hcl1::parser::Hcl1Expression::Number("invalid_num".into(), span.clone());
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
    let body_bad_map = crate::hcl1::parser::Hcl1Body {
        items: vec![crate::hcl1::parser::Hcl1Item::Attribute(
            crate::hcl1::parser::Hcl1Attribute {
                name: "bad_map".into(),
                expr: crate::hcl1::parser::Hcl1Expression::Map(
                    vec![("k".into(), bad_num)],
                    span.clone(),
                ),
                span: span.clone(),
            },
        )],
        span: span.clone(),
    };
    assert!(migrate_hcl1_to_hcl2(&body_bad_map).is_err());
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
    Ok(())
}
#[test]
fn test_translate_legacy_interpolations() {
    use crate::hcl1::migrate::translate_legacy_interpolations;
    assert_eq!(
        translate_legacy_interpolations("{{ user `admin` }}"),
        "${var.admin}"
    );
    assert_eq!(
        translate_legacy_interpolations("{{ env `SECRET` }}"),
        r#"${env("SECRET")}"#
    );
    assert_eq!(
        translate_legacy_interpolations("img-{{ timestamp }}"),
        "img-${timestamp()}"
    );
    assert_eq!(
        translate_legacy_interpolations("{{ pwd }}/scripts"),
        "${path.root}/scripts"
    );
    assert_eq!(
        translate_legacy_interpolations("{{ template_dir }}/files"),
        "${path.root}/files"
    );
    assert_eq!(
        translate_legacy_interpolations("{{ build_name }}-{{ build_type }}"),
        "${build.name}-${build.type}"
    );
    let re_fallback = crate::hcl1::migrate::compile_regex("invalid ( regex");
    assert!(!re_fallback.is_match("test"));
    let diag = crate::hcl1::migrate::MigrationDiagnostic::new(
        "test msg",
        crate::span::Span::new(0, 0, 0, 0, 0, 0),
        Some("suggestion"),
    );
    let _ = format!("{diag:?}");
    assert_eq!(diag, diag);
    let d_none = crate::hcl1::migrate::MigrationDiagnostic::new(
        "test msg",
        crate::span::Span::new(0, 0, 0, 0, 0, 0),
        None::<String>,
    );
    assert_ne!(diag, d_none);
}
#[test]
fn test_migrate_legacy_json_templates() -> Result<(), Box<dyn std::error::Error>> {
    use crate::hcl1::migrate::migrate_legacy_json;
    let json = r#"{
        "variables": {
            "iso_url": "ubuntu.iso",
            "iso_checksum": "{{ env `ISO_CHECKSUM` }}",
            "null_var": null,
            "bool_var": true,
            "nested": {
                "key": "val"
            }
        },
        "builders": [
            {
                "type": "qemu",
                "name": "vm",
                "disk_size": 20480,
                "vm_name": "vm-{{ user `iso_url` }}-{{ timestamp }}"
            },
            {
                "disk_size": 1024
            }
        ],
        "provisioners": [
            {
                "type": "shell",
                "inline": ["echo hello"]
            },
            {
                "inline": ["echo default shell"]
            }
        ],
        "description": "Bento image",
        "fallback_template": "prefix-{{ user `10 /` }}",
        "bad_single_interp": "{{ user `10 /` }}",
        "multi_interp": "${a} and ${b}",
        "unclosed_interp": "${unclosed"
    }"#;
    let (body, diags) = migrate_legacy_json(json)?;
    assert_ne!(diags.len(), 0);
    assert_eq!(
        body.blocks
            .iter()
            .filter(|b| b.block_type == "variable")
            .count(),
        5
    );
    let sources: Vec<&crate::ast::structure::Block> = body
        .blocks
        .iter()
        .filter(|b| b.block_type == "source")
        .collect();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].labels, vec!["qemu", "vm"]);
    assert_eq!(sources[1].labels, vec!["unknown", "unknown"]);
    let provs: Vec<&crate::ast::structure::Block> = body
        .blocks
        .iter()
        .filter(|b| b.block_type == "provisioner")
        .collect();
    assert_eq!(provs.len(), 2);
    assert_eq!(provs[0].labels, vec!["shell"]);
    assert_eq!(provs[1].labels, vec!["shell"]);
    assert!(body.attributes.contains_key("description"));
    assert!(migrate_legacy_json("invalid json").is_err());
    assert!(migrate_legacy_json(r#"["not an object"]"#).is_err());
    let non_std = r#"{
        "variables": "not_an_object",
        "builders": "not_an_array",
        "provisioners": "not_an_array"
    }"#;
    assert!(migrate_legacy_json(non_std).is_ok());
    let non_std_items = r#"{
        "builders": ["not_an_obj"],
        "provisioners": ["not_an_obj"]
    }"#;
    assert!(migrate_legacy_json(non_std_items).is_ok());
    Ok(())
}
