#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]
use crate::ast::expr::Expression;
use crate::parse::parser::Parser;
#[test]
fn test_parse_body_empty() {
    let mut parser = Parser::new("");
    let body = parser.parse_body();
    assert!(body.attributes.is_empty());
    assert_eq!(body.blocks, [] as [crate::ast::structure::Block; 0]);
    assert!(!parser.diags.has_errors());
}
#[test]
fn test_parse_body_attributes() {
    let input = "foo = 1\nbar = \"test\"\n";
    let mut parser = Parser::new(input);
    let body = parser.parse_body();
    println!("BODY: {body:?}");
    println!("DIAGS: {:?}", parser.diags.errors());
    assert_eq!(body.attributes.len(), 2);
    assert!(body.attributes.contains_key("foo"));
    assert!(body.attributes.contains_key("bar"));
    if let Expression::Number(ref n, _) = body.attributes["foo"].expr {
        assert_eq!(n.as_f64(), Some(1.0));
    } else {
        panic!("Expected Number for foo");
    }
    if let Expression::String(ref s, _) = body.attributes["bar"].expr {
        assert!(s.contains("test"));
    } else {
        panic!("Expected String for bar");
    }
    assert!(!parser.diags.has_errors());
}
#[test]
fn test_parse_body_blocks() {
    let input = "block_type \"label1\" label2 {\n  foo = 2\n}\n";
    let mut parser = Parser::new(input);
    let body = parser.parse_body();
    assert_eq!(body.blocks.len(), 1);
    let block = &body.blocks[0];
    assert_eq!(block.block_type, "block_type");
    assert_eq!(block.labels, vec!["label1", "label2"]);
    assert_eq!(block.body.attributes.len(), 1);
}
#[test]
fn test_parse_function_block() {
    let input = r#"
            function "add" {
                params = [a, b]
                result = a + b
            }
        "#;
    let mut parser = Parser::new(input);
    let body = parser.parse_body();
    assert_eq!(body.functions.len(), 1);
    let func = &body.functions[0];
    assert_eq!(func.name, "add");
    assert_eq!(func.params.len(), 2);
    assert_eq!(func.params[0].name, "a");
    assert_eq!(func.params[1].name, "b");
    assert!(matches!(
        func.body,
        crate::ast::expr::Expression::BinaryOp(_, _, _, _)
    ));
}
#[test]
fn test_parse_function_block_error_paths() {
    let mut parser = Parser::new("function \"add\" x { result = 1 }");
    let b = parser.parse_body();
    println!("Body: {:?}", b.functions.len());
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
    let mut parser = Parser::new("function \"add\" { result = 1 \n x \n");
    let b = parser.parse_body();
    println!("Body: {:?}", b.functions.len());
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
}
#[test]
fn test_parse_function_block_errors() {
    let mut parser = Parser::new("function add { result = 1 }");
    let b = parser.parse_body();
    println!("Body: {:?}", b.functions.len());
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
    let mut parser = Parser::new("function ");
    let b = parser.parse_body();
    println!("Body: {:?}", b.functions.len());
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
    let mut parser = Parser::new("function \"add\" result = 1 }");
    let b = parser.parse_body();
    println!("Body: {:?}", b.functions.len());
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
    let mut parser = Parser::new("function \"add\" { result = 1 ");
    let b = parser.parse_body();
    println!("Body: {:?}", b.functions.len());
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
    let mut parser = Parser::new("function \"add\" { params = [a] }\n");
    let body = parser.parse_body();
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
    assert_eq!(body.functions.len(), 0);
    let mut parser = Parser::new("function \"add\" { result = 1 } invalid\n");
    let b = parser.parse_body();
    println!("Body: {:?}", b.functions.len());
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
    let mut parser = Parser::new("function \"add\" {");
    let b = parser.parse_body();
    println!("Body: {:?}", b.functions.len());
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
    let mut parser = Parser::new("function \"add\" { params = [\"a\"] \n result = a \n}");
    let body = parser.parse_body();
    assert!(
        !parser.errors().has_errors(),
        "Errors: {:?}",
        parser.errors()
    );
    assert_eq!(body.functions.len(), 1);
    assert_eq!(body.functions[0].params[0].name, "a");
    let mut parser = Parser::new("function \"add\" { return_type = string \n result = 1 \n}");
    let body = parser.parse_body();
    assert!(
        !parser.errors().has_errors(),
        "Errors: {:?}",
        parser.errors()
    );
    assert_eq!(body.functions.len(), 1);
    assert!(body.functions[0].return_type.is_some());
    let mut parser = Parser::new("function \"add\"");
    let b = parser.parse_body();
    println!("Body: {:?}", b.functions.len());
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
    let mut parser = Parser::new("block { function \"f\" { result = 1 } }");
    let body = parser.parse_body();
    assert!(!parser.errors().has_errors());
    assert_eq!(body.blocks.len(), 1);
    assert_eq!(body.blocks[0].body.functions.len(), 1);
    let mut parser = Parser::new("function \"add\" { result = 1 ");
    let b = parser.parse_body();
    println!("Body: {:?}", b.functions.len());
    println!("{:?}", parser.errors().errors());
    assert!(parser.errors().has_errors());
}
#[test]
fn test_parse_body_duplicate_attributes() {
    let input = "foo = 1\nfoo = 2\n";
    let mut parser = Parser::new(input);
    let body = parser.parse_body();
    assert_eq!(body.attributes.len(), 1);
    if let Expression::Number(ref n, _) = body.attributes["foo"].expr {
        assert_eq!(n.as_f64(), Some(1.0));
    }
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert_eq!(errs.len(), 1);
    assert!(errs[0].error.to_string().contains("Attribute redefined"));
}
#[test]
fn test_parse_missing_newline() {
    let input = "foo = 1 bar = 2";
    let mut parser = Parser::new(input);
    let _ = parser.parse_body();
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert!(
        errs[0]
            .error
            .to_string()
            .contains("Missing newline after argument")
    );
}
#[test]
fn test_parse_comma_after_argument() {
    let input = "foo = 1, bar = 2";
    let mut parser = Parser::new(input);
    let _ = parser.parse_body();
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert!(
        errs[0]
            .error
            .to_string()
            .contains("Unexpected comma after argument")
    );
}
#[test]
fn test_parse_invalid_body_item_start() {
    let input = "= 1";
    let mut parser = Parser::new(input);
    let _ = parser.parse_body();
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert!(
        errs[0]
            .error
            .to_string()
            .contains("Argument or block definition required")
    );
}
#[test]
fn test_parse_invalid_argument_no_equals() {
    let input = "foo + 1";
    let mut parser = Parser::new(input);
    let _ = parser.parse_body();
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert!(
        errs[0]
            .error
            .to_string()
            .contains("Argument or block definition required")
    );
}
#[test]
fn test_parse_block_equals() {
    let input = "block_type \"label\" = { }";
    let mut parser = Parser::new(input);
    let _ = parser.parse_body();
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert!(
        errs[0]
            .error
            .to_string()
            .contains("Invalid block definition")
    );
}
#[test]
fn test_parse_block_unclosed() {
    let input = "block_type { foo = 1";
    let mut parser = Parser::new(input);
    let _ = parser.parse_body();
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert!(
        errs[0]
            .error
            .to_string()
            .contains("Unclosed configuration block")
    );
}
#[test]
fn test_parse_block_newline_before_brace() {
    let input = "block_type \"label\" \n { foo = 1 }";
    let mut parser = Parser::new(input);
    let _ = parser.parse_body();
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert!(
        errs[0]
            .error
            .to_string()
            .contains("Invalid block definition")
    );
}
#[test]
fn test_parse_block_invalid_label() {
    let input = "block_type \"label\" + { }";
    let mut parser = Parser::new(input);
    let _ = parser.parse_body();
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert!(
        errs[0]
            .error
            .to_string()
            .contains("Invalid block definition")
    );
}
#[test]
fn test_parse_expression_fallback() {
    let input = "foo = null\nbar = true\nbaz = false\nqux = \"str\"\n";
    let mut parser = Parser::new(input);
    let body = parser.parse_body();
    assert!(matches!(body.attributes["foo"].expr, Expression::Null(_)));
    assert!(matches!(
        body.attributes["bar"].expr,
        Expression::Bool(true, _)
    ));
    assert!(matches!(
        body.attributes["baz"].expr,
        Expression::Bool(false, _)
    ));
    assert!(matches!(
        body.attributes["qux"].expr,
        Expression::String(_, _)
    ));
}
#[test]
fn test_parse_expression_invalid() {
    let input = "foo = }";
    let mut parser = Parser::new(input);
    let _ = parser.parse_body();
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert!(errs[0].error.to_string().contains("Expected expression"));
}
#[test]
fn test_parse_block_missing_newline() {
    let input = "block { } foo = 1";
    let mut parser = Parser::new(input);
    let _ = parser.parse_body();
    assert!(parser.diags.has_errors());
    let errs = parser.diags.errors();
    assert!(
        errs[0]
            .error
            .to_string()
            .contains("Missing newline after block definition")
    );
}
#[test]
fn test_parse_expression_pratt() {
    let input = "foo = 1 + 2 * 3";
    let mut parser = Parser::new(input);
    let body = parser.parse_body();
    let expr = &body.attributes["foo"].expr;
    if let Expression::BinaryOp(op, left, right, _) = expr {
        assert_eq!(*op, crate::ast::expr::BinaryOp::Add);
        assert!(matches!(**left, Expression::Number(..)));
        assert!(matches!(
            **right,
            Expression::BinaryOp(crate::ast::expr::BinaryOp::Mul, _, _, _)
        ));
    } else {
        panic!("Expected binary op ADD at top level");
    }
}
#[test]
fn test_template_directives() {
    use crate::parse::parser::Parser;
    let test_cases = vec![
        r#""hello %{if true} world %{endif}""#,
        r#""hello %{~if true~} world %{~endif~}""#,
        r#""%{for x in items} ${x} %{endfor}""#,
        r#""%{for k, v in items} ${k}=${v} %{endfor}""#,
        r#""%{if true} yes %{else} no %{endif}""#,
        r#""%{if true} yes %{elseif false} maybe %{else} no %{endif}""#,
        r#""%{if true} unclosed""#,
        r#""%{for x in items} unclosed""#,
    ];
    for src in test_cases {
        let mut parser = Parser::new(src);
        let _ = parser.parse_expression();
    }
}
#[test]
fn test_parse_template_direct_exhaustive() {
    use crate::diagnostic::Diagnostics;
    use crate::parse::parser::Parser;
    use crate::span::Span;
    let mut diags = Diagnostics::new();
    let span = Span::default();
    let parts1 = Parser::parse_template(&mut diags, r#""%{if true}yes%{else}no%{endif}""#, &span);
    assert_eq!(parts1.len(), 1);
    let parts2 = Parser::parse_template(
        &mut diags,
        r#""%{if true}a%{elseif false}b%{else}c%{endif}""#,
        &span,
    );
    assert_eq!(parts2.len(), 1);
    let parts3 = Parser::parse_template(&mut diags, r#""%{if true}unclosed""#, &span);
    assert_eq!(parts3.len(), 1);
    let parts4 = Parser::parse_template(&mut diags, r#""%{for x in items}${x}%{endfor}""#, &span);
    assert_eq!(parts4.len(), 1);
    assert!(matches!(
        &parts4[0],
        crate::ast::expr::TemplatePart::Directive(crate::ast::expr::Directive::For { .. }, _)
    ));
    let parts5 = Parser::parse_template(
        &mut diags,
        r#""%{for k, v in items}${k}=${v}%{endfor}""#,
        &span,
    );
    assert_eq!(parts5.len(), 1);
    let parts6 = Parser::parse_template(&mut diags, r#""%{for x in items}unclosed""#, &span);
    assert_eq!(parts6.len(), 1);
    let _ = Parser::parse_template(&mut diags, r#""%{else}%{endif}%{endfor}""#, &span);
    assert!(diags.has_errors());
    let parts8 = Parser::parse_template(&mut diags, r#""%{~ if true ~}   %{~ endif ~}""#, &span);
    assert_eq!(parts8.len(), 1);
}
#[test]
fn test_bad_index_slice() {
    use crate::parse::parser::Parser;
    let mut p = Parser::new("a[1:2:3]");
    let _ = p.parse_expression();
    let mut p2 = Parser::new("a[1");
    let _ = p2.parse_expression();
}
#[test]
fn test_parser_expr_exhaustive() {
    use crate::parse::parser::Parser;
    let test_cases = vec!["(1", "{ 1 2 }", "a ? 1", "-", "!", "a."];
    for src in test_cases {
        let mut parser = Parser::new(src);
        let _ = parser.parse_expression();
    }
}
#[test]
fn test_parser_expr_exhaustive_more() {
    use crate::parse::parser::Parser;
    let test_cases = vec![
        "a.*", "a.*.", "a(", "a(1 2)", "a ? 1", "a ? 1 :", "{ 1 2 }", "{ 1 : 2,", "{ 1 : }", "[1",
        "[1,", "\"foo", "\"foo${1", "1 +", "1 *", "1 == ", "1 && ", "1 + )", "1 + }",
    ];
    for src in test_cases {
        let mut parser = Parser::new(src);
        let _ = parser.parse_expression();
    }
}
#[test]
fn test_parser_expr_exhaustive_even_more() {
    use crate::parse::parser::Parser;
    let test_cases = vec!["a ? 1", "a ? 1 +", "a ? 1 2", "a.", "a.1", "a[1", "a[1+"];
    for src in test_cases {
        let mut parser = Parser::new(src);
        let _ = parser.parse_expression();
    }
}
#[test]
fn test_parser_expr_valid_traversal() {
    use crate::parse::parser::Parser;
    let mut parser = Parser::new("a.b[1].c");
    let _ = parser.parse_expression();
}
#[test]
fn test_parser_expr_valid_conditional() {
    use crate::parse::parser::Parser;
    let mut parser = Parser::new("a ? 1 : 2");
    let _ = parser.parse_expression();
}
#[test]
fn test_parser_for_expr_valid() {
    use crate::parse::parser::Parser;
    let mut p = Parser::new("[for k, v in a: v if true]");
    assert!(p.parse_expression().is_some());
    let mut p_single = Parser::new("[for v in a: v]");
    assert!(p_single.parse_expression().is_some());
    let mut p_obj = Parser::new("{for k, v in a: k => v... if true}");
    assert!(p_obj.parse_expression().is_some());
    let mut p_obj_single = Parser::new("{for v in a: v => v}");
    assert!(p_obj_single.parse_expression().is_some());
    let mut err1 = Parser::new("[for 123 in a: v]");
    assert!(err1.parse_expression().is_none());
    let mut err2 = Parser::new("[for k, 123 in a: v]");
    assert!(err2.parse_expression().is_none());
    let mut err3 = Parser::new("[for x bad a: x]");
    assert!(err3.parse_expression().is_none());
    let mut err4 = Parser::new("[for x in a bad x]");
    assert!(err4.parse_expression().is_none());
    let mut err5 = Parser::new("[for x in a: x bad]");
    assert!(err5.parse_expression().is_none());
    let mut err_o1 = Parser::new("{for 123 in a: k => v}");
    assert!(err_o1.parse_expression().is_none());
    let mut err_o2 = Parser::new("{for k, 123 in a: k => v}");
    assert!(err_o2.parse_expression().is_none());
    let mut err_o3 = Parser::new("{for k bad a: k => v}");
    assert!(err_o3.parse_expression().is_none());
    let mut err_o3_non_ident = Parser::new("{for k = a: k => v}");
    assert!(err_o3_non_ident.parse_expression().is_none());
    let mut err_o4 = Parser::new("{for k in a bad k => v}");
    assert!(err_o4.parse_expression().is_none());
    let mut err_o5 = Parser::new("{for k in a: k bad v}");
    assert!(err_o5.parse_expression().is_none());
    let mut err_o6 = Parser::new("{for k in a: k => v bad}");
    assert!(err_o6.parse_expression().is_none());
    let mut err_o_eof_after_val = Parser::new("{for k, v in a: k => v");
    assert!(err_o_eof_after_val.parse_expression().is_none());
}
#[test]
fn test_parser_unary() {
    use crate::parse::parser::Parser;
    let mut p = Parser::new("-1");
    let _ = p.parse_expression();
    let mut p2 = Parser::new("!true");
    let _ = p2.parse_expression();
}
#[test]
fn test_parser_expr_exhaustive_all_ops() {
    use crate::parse::parser::Parser;
    let test_cases = vec![
        "1 - 1",
        "1 * 1",
        "1 / 1",
        "1 % 1",
        "1 == 1",
        "1 != 1",
        "1 < 1",
        "1 <= 1",
        "1 > 1",
        "1 >= 1",
        "true && true",
        "true || false",
        "a[1]",
        "a(1...)",
        "a()",
        "a(1)",
    ];
    for src in test_cases {
        let mut parser = Parser::new(src);
        let _ = parser.parse_expression();
    }
}
#[test]
fn test_parser_expr_exhaustive_all_ops_missing() {
    use crate::parse::parser::Parser;
    let test_cases = vec!["(1}", "{ 1 }", "a[1}", "[1 2", "1 + 2 *", "a(1"];
    for src in test_cases {
        let mut parser = Parser::new(src);
        let _ = parser.parse_expression();
    }
}
#[test]
fn test_parser_exhaustive_coverage() {
    use crate::parse::parser::Parser;
    let test_cases = vec![
        "",
        "foo",
        "foo =",
        "foo \n = 1",
        "block 1 {}",
        "block \"lbl\"",
        "block {",
        "}",
        "a = (1",
        "a = {}",
        "a = foo(1,)",
        "a = foo(1..., 2)",
        "a = 1(2)",
        "a = arr[1",
        "a = arr[]",
        "a = arr[*",
        "a = { 1 = 2 }",
        "a = [1",
        "a = [1,]",
        "a = { foo: 1 }",
        "a = 1 +",
        "a = ! ",
        "a = [for x [1]]",
        "a = [for x in [1]: x",
        "a = {for k,v in {}: k=>v}",
        "a = {for k,v in {}: k=>v...}",
        "a = {for k v}",
        "a = \"unclosed",
        "a = <<EOF\nEOF",
    ];
    for src in test_cases {
        let mut parser = Parser::new(src);
        let b = parser.parse_body();
        println!("Body: {:?}", b.functions.len());
        let mut parser_expr = Parser::new(src);
        let _ = parser_expr.parse_expression();
    }
}
#[test]
fn test_parser_extra_coverage() {
    let test_cases = [
        "function \"foo\" { params = [] \n result = 1",
        "function \"foo\" { params = [] \n result = 1 } function \"bar\" {}",
        "function \"foo\" { params = [] \n }",
        "block label {",
        "block \"label\" {",
        "block {",
        "block { a = { b = { c = 1 }",
        "function \"foo\" { params = [] result = { a = {",
        "a = \"${foo}\"",
        "a = \"${~foo~}\"",
        "a = \"${ { a = 1 } }\"",
        "a = \"${ { a = { b = 1 } } }\"",
        "a = \"${}\"",
        "a = \"%{ if {a=1} }x%{ endif }\"",
        "a = \"%{ }\"",
        "a = {\n  foo = 1,\n\n  bar = 2\n}",
        "a = { foo = 1 bar = 2 }",
        "a = }",
        "a = )",
        "a = ]",
        "a = ,",
    ];
    for src in test_cases {
        let mut parser = crate::parse::parser::Parser::new(src);
        let b = parser.parse_body();
        println!("Body: {:?}", b.functions.len());
        let mut parser_expr = crate::parse::parser::Parser::new(src);
        let _ = parser_expr.parse_expression();
    }
}
#[test]
fn test_parser_unclosed_string() {
    let _ = crate::api::parse("a = \"unclosed");
}
#[test]
fn test_parser_uncovered_paths_old() {
    let cases = [
        "function \"foo\" { params = [] \n result = 1 }]",
        "function \"foo\" { params = [] \n result = 1 \n] ",
        "function \"foo\" { params = [] \n result = 1 }a",
        "block \"b\" { a = 1 \n ]",
        "a = \"unclosed",
        "a = \"${}\"",
        "a = \"%{ }\"",
        "a = { a = 1,\n }",
        "a = ]",
    ];
    for case in cases {
        let mut parser = crate::parse::parser::Parser::new(case);
        println!("Parsing: {case}");
        let b = parser.parse_body();
        println!("Body: {:?}", b.functions.len());
    }
}
#[test]
fn test_parser_coverage_gaps() {
    let mut p = Parser::new("function \"n\" \n");
    let _body = p.parse_body();
    let mut p2 = Parser::new("function ");
    let _body2 = p2.parse_body();
    let mut p3 = Parser::new("function \"n\" { } bad");
    let _body3 = p3.parse_body();
    let mut p4 = Parser::new("\"\\u12G\"");
    p4.parse_expression();
    let mut p5 = Parser::new("\"\\u12345\"");
    p5.parse_expression();
    let mut p6 = Parser::new("\"\\U12345G\"");
    p6.parse_expression();
    let mut p7 = Parser::new("\"\\UD800\"");
    p7.parse_expression();
    let mut p8 = Parser::new("1 1");
    p8.parse_expression();
}
#[test]
fn test_parser_uncovered_paths() {
    let mut p = Parser::new("function ");
    let _body = p.parse_body();
    let mut p2 = Parser::new("function \"n\" { } bad");
    let _body2 = p2.parse_body();
    let mut p3 = Parser::new("a = 1\nfunction \"n\" \n");
    let _body3 = p3.parse_body();
    let mut p4 = Parser::new("function \"n\" { result = \n }");
    let _body4 = p4.parse_body();
    let mut p5 = Parser::new("function \"n\" { result = 1 \n params = [");
    let _body5 = p5.parse_body();
    let mut p6 = Parser::new("1 1");
    p6.parse_expression();
    let mut p7 = Parser::new("\"\\u12G\"");
    p7.parse_expression();
    let mut p8 = Parser::new("\"\\U12345G\"");
    p8.parse_expression();
    let mut p9 = Parser::new("\"\\UD800\"");
    p9.parse_expression();
    let mut p10 = Parser::new("\"\\u12345\"");
    p10.parse_expression();
}
#[test]
fn test_parser_extra_coverage2() {
    use crate::parse::parser::Parser;
    let mut p = Parser::new(r#""$${ %%{ ""#);
    p.parse_expression();
    let mut p2 = Parser::new(r#""\n \r \t \\ \" \u1234 \U00012345""#);
    p2.parse_expression();
    let mut p3 = Parser::new(r#""${}""#);
    p3.parse_expression();
    let mut p4 = Parser::new(r#""%{ }""#);
    p4.parse_expression();
    let mut p5 = Parser::new("[\n1,\n2\n]");
    p5.parse_expression();
    let mut p6 = Parser::new("]");
    p6.parse_expression();
    let mut p7 = Parser::new("\"a");
    p7.parse_expression();
    let mut p8 = Parser::new("[\n\n]");
    p8.parse_expression();
    let mut p9 = Parser::new("{\n\n");
    p9.parse_expression();
    let mut p10 = Parser::new("{\n a = 1 \n }");
    p10.parse_expression();
    println!(
        "{:?}",
        crate::lex::lexer::Lexer::new(r#""\u12""#).collect::<Vec<_>>()
    );
    let mut p11 = Parser::new(r#""\u12""#);
    p11.parse_expression();
    let mut p12 = Parser::new(r#""\u123G""#);
    p12.parse_expression();
    let mut p13 = Parser::new(r#""\uD800""#);
    p13.parse_expression();
    let mut p14 = Parser::new(r#""\U12""#);
    p14.parse_expression();
    let mut p15 = Parser::new(r#""\U0000000G""#);
    p15.parse_expression();
    let mut p16 = Parser::new(r#""\UD8000000""#);
    p16.parse_expression();
    let mut p17 = Parser::new(r#""\a""#);
    p17.parse_expression();
    let mut p18 = Parser::new(r#""\""#);
    p18.parse_expression();
    let mut p19 = Parser::new(r#""${}""#);
    p19.parse_expression();
    let mut p20 = Parser::new(r#""%{}""#);
    p20.parse_expression();
}
#[test]
fn test_parse_dynamic_block_success_and_errors() {
    let src = r#"
    dynamic "setting" {
        for_each = ["a", "b"]
        iterator = custom_iter
        labels   = ["lbl"]
        content {
            val = custom_iter.value
        }
    }
    "#;
    let mut parser = Parser::new(src);
    let body = parser.parse_body();
    assert!(!parser.errors().has_errors());
    assert_eq!(body.dynamic_blocks.len(), 1);
    let dyn_b = &body.dynamic_blocks[0];
    assert_eq!(dyn_b.block_type, "setting");
    assert_eq!(dyn_b.iterator.as_deref(), Some("custom_iter"));
    assert!(dyn_b.labels.is_some());
    assert_eq!(dyn_b.content.attributes.len(), 1);
    let src_ident = r"
    dynamic setting {
        for_each = []
        content {}
    }
    ";
    let mut parser2 = Parser::new(src_ident);
    let body2 = parser2.parse_body();
    assert!(!parser2.errors().has_errors());
    assert_eq!(body2.dynamic_blocks.len(), 1);
    let src_attr = "dynamic = \"attribute_value\"\n";
    let mut parser3 = Parser::new(src_attr);
    let body3 = parser3.parse_body();
    assert!(!parser3.errors().has_errors());
    assert_eq!(body3.attributes.len(), 1);
    let src_no_lbl = "dynamic { for_each = [] content {} }\n";
    let mut p_no_lbl = Parser::new(src_no_lbl);
    p_no_lbl.parse_body();
    assert!(p_no_lbl.errors().has_errors());
    let src_eof = "dynamic";
    let mut p_eof = Parser::new(src_eof);
    p_eof.parse_body();
    assert!(p_eof.errors().has_errors());
    let src_no_brace = "dynamic \"tag\" for_each = []\n";
    let mut p_no_brace = Parser::new(src_no_brace);
    p_no_brace.parse_body();
    assert!(p_no_brace.errors().has_errors());
    let src_eof_lbl = "dynamic \"tag\"";
    let mut p_eof_lbl = Parser::new(src_eof_lbl);
    p_eof_lbl.parse_body();
    assert!(p_eof_lbl.errors().has_errors());
    let src_unclosed = "dynamic \"tag\" { for_each = [] content {}";
    let mut p_unclosed = Parser::new(src_unclosed);
    p_unclosed.parse_body();
    assert!(p_unclosed.errors().has_errors());
    let src_no_each = "dynamic \"tag\" { content {} }\n";
    let mut p_no_each = Parser::new(src_no_each);
    p_no_each.parse_body();
    assert!(p_no_each.errors().has_errors());
    let src_no_content = "dynamic \"tag\" { for_each = [] }\n";
    let mut p_no_content = Parser::new(src_no_content);
    p_no_content.parse_body();
    assert!(p_no_content.errors().has_errors());
    let src_bad_iter = "dynamic \"tag\" {\n for_each = []\n iterator = 123\n content {}\n }\n";
    let mut p_bad_iter = Parser::new(src_bad_iter);
    let body_bad_iter = p_bad_iter.parse_body();
    assert!(p_bad_iter.errors().has_errors());
    assert_eq!(body_bad_iter.dynamic_blocks.len(), 1);
    assert_eq!(body_bad_iter.dynamic_blocks[0].iterator, None);
    let src_bad_lbls = "dynamic \"tag\" {\n for_each = []\n labels = \"lbl\"\n content {}\n }\n";
    let mut p_bad_lbls = Parser::new(src_bad_lbls);
    let body_bad_lbls = p_bad_lbls.parse_body();
    assert!(p_bad_lbls.errors().has_errors());
    assert_eq!(body_bad_lbls.dynamic_blocks.len(), 1);
    assert_eq!(body_bad_lbls.dynamic_blocks[0].labels, None);
}
#[test]
fn test_parse_validation_blocks() {
    let src = r#"
        validation {
            condition = var.port > 0
            error_message = "Port must be positive."
        }
    "#;
    let mut p = Parser::new(src);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    assert_eq!(body.validations.len(), 1);
    let src_no_cond = r#"
        validation {
            error_message = "Missing condition"
        }
    "#;
    let mut p_no_cond = Parser::new(src_no_cond);
    let body_no_cond = p_no_cond.parse_body();
    assert!(p_no_cond.errors().has_errors());
    assert_eq!(body_no_cond.validations.len(), 0);
    let src_no_msg = r"
        validation {
            condition = true
        }
    ";
    let mut p_no_msg = Parser::new(src_no_msg);
    let body_no_msg = p_no_msg.parse_body();
    assert!(p_no_msg.errors().has_errors());
    assert_eq!(body_no_msg.validations.len(), 0);
    let src_unclosed = "validation {\n condition = true\n error_message = \"msg\"\n";
    let mut p_unclosed = Parser::new(src_unclosed);
    p_unclosed.parse_body();
    assert!(p_unclosed.errors().has_errors());
    let src_labeled = r#"
        validation "rule_1" {
            enabled = true
        }
    "#;
    let mut p_labeled = Parser::new(src_labeled);
    let body_labeled = p_labeled.parse_body();
    assert!(!p_labeled.errors().has_errors());
    assert_eq!(body_labeled.blocks.len(), 1);
    assert_eq!(body_labeled.blocks[0].block_type, "validation");
    assert_eq!(body_labeled.blocks[0].labels, vec!["rule_1"]);
    assert_eq!(body_labeled.validations.len(), 0);
    let src_attr = "validation = true\n";
    let mut p_attr = Parser::new(src_attr);
    let body_attr = p_attr.parse_body();
    assert!(!p_attr.errors().has_errors());
    assert_eq!(body_attr.attributes.len(), 1);
    assert_eq!(body_attr.validations.len(), 0);
}
#[test]
fn test_parse_preconditions_and_postconditions() {
    let src = r#"
        lifecycle {
            precondition {
                condition = var.enabled
                error_message = "Must be enabled."
            }
            postcondition {
                condition = self.ready
                error_message = "Must be ready."
            }
        }
    "#;
    let mut p = Parser::new(src);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    assert_eq!(body.blocks.len(), 1);
    let lifecycle = &body.blocks[0];
    assert_eq!(lifecycle.body.preconditions.len(), 1);
    assert_eq!(lifecycle.body.postconditions.len(), 1);
    let src_pre_no_cond = "precondition { error_message = \"fail\" }\n";
    let mut p_pre_no_cond = Parser::new(src_pre_no_cond);
    let b_pre = p_pre_no_cond.parse_body();
    assert!(p_pre_no_cond.errors().has_errors());
    assert_eq!(b_pre.preconditions.len(), 0);
    let src_post_no_msg = "postcondition { condition = true }\n";
    let mut p_post_no_msg = Parser::new(src_post_no_msg);
    let b_post = p_post_no_msg.parse_body();
    assert!(p_post_no_msg.errors().has_errors());
    assert_eq!(b_post.postconditions.len(), 0);
    let src_unclosed_pre = "precondition {\n condition = true\n error_message = \"msg\"\n";
    let mut p_unclosed_pre = Parser::new(src_unclosed_pre);
    p_unclosed_pre.parse_body();
    assert!(p_unclosed_pre.errors().has_errors());
    let src_unclosed_post = "postcondition {\n condition = true\n error_message = \"msg\"\n";
    let mut p_unclosed_post = Parser::new(src_unclosed_post);
    p_unclosed_post.parse_body();
    assert!(p_unclosed_post.errors().has_errors());
}
#[test]
fn test_parser_coverage_comprehensive() {
    let src_comments = "/* block comment */\n// line comment\n# hash comment\nfoo = 1\n";
    let mut p_comments = Parser::new(src_comments);
    let b_comments = p_comments.parse_body();
    assert_eq!(b_comments.attributes.len(), 1);
    let src_named_items =
        "dynamic = 123\nvalidation \"label\" {}\nprecondition = 1\npostcondition = 2\n";
    let mut p_named = Parser::new(src_named_items);
    let b_named = p_named.parse_body();
    assert_eq!(b_named.attributes.len(), 3);
    assert_eq!(b_named.blocks.len(), 1);
    let src_func_param = "function \"foo\" {\n  params = [123]\n  result = \"ok\"\n}\n";
    let mut p_func_param = Parser::new(src_func_param);
    let b_func_param = p_func_param.parse_body();
    assert_eq!(b_func_param.functions.len(), 1);
    assert_eq!(b_func_param.functions[0].params.len(), 0);
    let src_func_unclosed = "function \"f\" {\n  params = []\n";
    let mut p_func_unclosed = Parser::new(src_func_unclosed);
    let b_func_unclosed = p_func_unclosed.parse_body();
    assert!(p_func_unclosed.errors().has_errors());
    assert_eq!(b_func_unclosed.functions.len(), 0);
    let src_dyn_nl = "dynamic \"setting\"\n{\n  for_each = []\n  content {}\n}\n";
    let mut p_dyn_nl = Parser::new(src_dyn_nl);
    let b_dyn_nl = p_dyn_nl.parse_body();
    assert_eq!(b_dyn_nl.dynamic_blocks.len(), 1);
    let src_block_bad = "block \"bad\" 123 { inner { a = 1 } }\nfoo = 1\n";
    let mut p_block_bad = Parser::new(src_block_bad);
    let b_block_bad = p_block_bad.parse_body();
    assert!(p_block_bad.errors().has_errors());
    assert_eq!(b_block_bad.attributes.len(), 1);
    let src_tpl = "str = \"${~ foo ~} bar %{~ if true ~} yes %{~ else ~} no %{~ endif ~}\"\n";
    let mut p_tpl = Parser::new(src_tpl);
    let b_tpl = p_tpl.parse_body();
    assert_eq!(b_tpl.attributes.len(), 1);
    let src_tpl_nested = "str = \"${ { a = 1 } } %{ if { a = 1 } != null } ok %{ endif }\"\n";
    let mut p_tpl_nested = Parser::new(src_tpl_nested);
    let b_tpl_nested = p_tpl_nested.parse_body();
    assert_eq!(b_tpl_nested.attributes.len(), 1);
    let src_heredoc = "str = <<EOF\nplain text\nEOF\n";
    let mut p_heredoc = Parser::new(src_heredoc);
    let b_heredoc = p_heredoc.parse_body();
    assert_eq!(b_heredoc.attributes.len(), 1);
    let src_esc = "str = \"hello\\/world\"\n";
    let mut p_esc = Parser::new(src_esc);
    let b_esc = p_esc.parse_body();
    assert_eq!(b_esc.attributes.len(), 1);
    let mut diags = crate::diagnostic::Diagnostics::new();
    let empty_tpl =
        Parser::parse_template(&mut diags, "", &crate::span::Span::new(0, 0, 0, 0, 0, 0));
    assert_eq!(empty_tpl.len(), 0);
    let trailing_bs_tpl =
        Parser::parse_template(&mut diags, "a\\", &crate::span::Span::new(0, 0, 0, 0, 0, 0));
    assert_eq!(trailing_bs_tpl.len(), 1);
    let single_quote_tpl =
        Parser::parse_template(&mut diags, "\"", &crate::span::Span::new(0, 0, 0, 0, 0, 0));
    assert_eq!(single_quote_tpl.len(), 0);
    let src_dyn_bad_close = "dynamic \"x\" { for_each = [] content {} 123";
    let mut p_dyn_close = Parser::new(src_dyn_bad_close);
    p_dyn_close.parse_body();
    assert!(p_dyn_close.errors().has_errors());
    let src_val_bad_close = "validation { condition = true error_message = \"msg\" 123";
    let mut p_val_close = Parser::new(src_val_bad_close);
    p_val_close.parse_body();
    assert!(p_val_close.errors().has_errors());
    let src_func_bad_params =
        "function \"f\" {\n  params = \"bad\"\n  return_type = invalid_type\n  result = 1\n}\n";
    let mut p_func_bad = Parser::new(src_func_bad_params);
    let b_func_bad = p_func_bad.parse_body();
    assert_eq!(b_func_bad.functions.len(), 1);
    assert_eq!(b_func_bad.functions[0].params.len(), 0);
    assert!(b_func_bad.functions[0].return_type.is_none());
    let src_func_num_ret = "function \"f\" {\n  return_type = 123\n  result = 1\n}\n";
    let mut p_func_num_ret = Parser::new(src_func_num_ret);
    let b_func_num_ret = p_func_num_ret.parse_body();
    assert!(b_func_num_ret.functions[0].return_type.is_none());
    let src_unclosed_tpl =
        "str = \"${foo\"\nstr2 = \"%{if true\"\nstr3 = \"%{ if +++ }ok%{ endif }\"\n";
    let mut p_unclosed_tpl = Parser::new(src_unclosed_tpl);
    let b_unclosed_tpl = p_unclosed_tpl.parse_body();
    assert_eq!(b_unclosed_tpl.attributes.len(), 3);
    let src_esc_symbols = "str = \"$$dollar %%percent\"\n";
    let mut p_esc_sym = Parser::new(src_esc_symbols);
    let b_esc_sym = p_esc_sym.parse_body();
    assert_eq!(b_esc_sym.attributes.len(), 1);
}
#[test]
fn test_parse_template_directives_exhaustive() {
    use crate::ast::expr::{Directive, Expression, TemplatePart};
    use crate::parse::parser::Parser;
    let src = r#"msg = "Hello %{ if a }A%{ else if b }B%{ elif c }C%{ else }D%{ endif }!""#;
    let mut p = Parser::new(src);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    let attr = &body.attributes["msg"];
    if let Expression::Template(parts, _) = &attr.expr {
        assert_eq!(parts.len(), 3);
        if let TemplatePart::Directive(
            Directive::If {
                cond,
                true_expr,
                else_ifs,
                false_expr,
            },
            _,
        ) = &parts[1]
        {
            assert!(matches!(cond, Expression::Variable(name, _) if name == "a"));
            assert_eq!(true_expr.len(), 1);
            assert_eq!(else_ifs.len(), 2);
            assert!(false_expr.is_some());
        } else {
            panic!("expected Directive::If");
        }
    } else {
        panic!("expected Template");
    }
    let src_for1 = r#"msg = "%{ for item in items }${item}%{ endfor }""#;
    let mut p_for1 = Parser::new(src_for1);
    let body_for1 = p_for1.parse_body();
    assert!(!p_for1.errors().has_errors());
    if let Expression::Template(parts, _) = &body_for1.attributes["msg"].expr {
        if let TemplatePart::Directive(
            Directive::For {
                key_var, val_var, ..
            },
            _,
        ) = &parts[0]
        {
            assert!(key_var.is_none());
            assert_eq!(val_var, "item");
        } else {
            panic!("expected Directive::For");
        }
    }
    let src_for2 = r#"msg = "%{ for k, v in pairs }${k}=${v}%{ endfor }""#;
    let mut p_for2 = Parser::new(src_for2);
    let body_for2 = p_for2.parse_body();
    assert!(!p_for2.errors().has_errors());
    if let Expression::Template(parts, _) = &body_for2.attributes["msg"].expr {
        if let TemplatePart::Directive(
            Directive::For {
                key_var, val_var, ..
            },
            _,
        ) = &parts[0]
        {
            assert_eq!(key_var.as_deref(), Some("k"));
            assert_eq!(val_var, "v");
        } else {
            panic!("expected Directive::For");
        }
    }
    let src_err1 = r#"msg = "%{ for x }foo%{ endfor }""#;
    let mut p_err1 = Parser::new(src_err1);
    p_err1.parse_body();
    assert!(p_err1.errors().has_errors());
    let src_err2 = r#"msg = "%{ unknown_directive }foo""#;
    let mut p_err2 = Parser::new(src_err2);
    p_err2.parse_body();
    assert!(p_err2.errors().has_errors());
    let src_err3 = r#"msg = "%{ if true }unclosed""#;
    let mut p_err3 = Parser::new(src_err3);
    p_err3.parse_body();
    assert!(p_err3.errors().has_errors());
    let src_err4 = r#"msg = "%{ for x in xs }unclosed""#;
    let mut p_err4 = Parser::new(src_err4);
    p_err4.parse_body();
    assert!(p_err4.errors().has_errors());
    let src_err5 = r#"msg = "hello %{ else } unexpected""#;
    let mut p_err5 = Parser::new(src_err5);
    p_err5.parse_body();
    assert!(p_err5.errors().has_errors());
    let src_err6 = r#"msg = "%{ if true }a%{ else }b%{ else }c%{ endif }""#;
    let mut p_err6 = Parser::new(src_err6);
    p_err6.parse_body();
    assert!(p_err6.errors().has_errors());
    let src_strip = r#"msg = "hello %{~ if true ~} world %{~ endif ~}!""#;
    let mut p_strip = Parser::new(src_strip);
    let body_strip = p_strip.parse_body();
    assert!(!p_strip.errors().has_errors());
    if let Expression::Template(parts, _) = &body_strip.attributes["msg"].expr {
        assert_eq!(parts.len(), 3);
        assert_eq!(
            parts[0],
            TemplatePart::Literal("hello".to_string(), parts[0].span())
        );
    }
}
#[test]
fn test_heredoc_indentation_stripping_and_directives() {
    use crate::ast::expr::TemplatePart;
    use crate::eval::context::Context;
    use crate::eval::evaluator::Evaluator;
    use crate::parse::strip_heredoc_indentation;
    use crate::types::{Type, Value, ValueData};
    assert_eq!(strip_heredoc_indentation("").unwrap_or_default(), "");
    let single_line = "    single indented line";
    assert_eq!(
        strip_heredoc_indentation(single_line).unwrap_or_default(),
        "single indented line"
    );
    let multi_nested = "  first\n    second nested\n  third";
    assert_eq!(
        strip_heredoc_indentation(multi_nested).unwrap_or_default(),
        "first\n  second nested\nthird"
    );
    let multi_with_empty = "    first\n\n    second\n  \n    third";
    assert_eq!(
        strip_heredoc_indentation(multi_with_empty).unwrap_or_default(),
        "first\n\nsecond\n\nthird"
    );
    let crlf = "  line1\r\n    line2\r\n  line3";
    assert_eq!(
        strip_heredoc_indentation(crlf).unwrap_or_default(),
        "line1\r\n  line2\r\nline3"
    );
    let tab_indented = "\t\tfirst\n\t\t\tsecond\n\t\tthird";
    assert_eq!(
        strip_heredoc_indentation(tab_indented).unwrap_or_default(),
        "first\n\tsecond\nthird"
    );
    let mixed_single = "  \tline1\n  \tline2";
    let err_single = strip_heredoc_indentation(mixed_single).unwrap_err();
    assert!(
        err_single
            .to_string()
            .contains("mixed tabs and spaces in leading whitespace")
    );
    let mixed_lines = "  line1\n\tline2";
    let err_lines = strip_heredoc_indentation(mixed_lines).unwrap_err();
    assert!(
        err_lines
            .to_string()
            .contains("mixed tabs and spaces across lines")
    );
    let src_indented = "msg = <<-EOF\n  line1\n    nested line2\n  line3\n  EOF\n";
    let mut p_ind = Parser::new(src_indented);
    let body_ind = p_ind.parse_body();
    assert!(!p_ind.errors().has_errors());
    if let Expression::Template(parts, _) = &body_ind.attributes["msg"].expr {
        let text = match &parts[0] {
            TemplatePart::Literal(s, _) => s.as_str(),
            _ => panic!("expected literal"),
        };
        assert_eq!(text, "line1\n  nested line2\nline3");
    } else {
        panic!("expected template expression");
    }
    let src_std = "msg = <<EOF\n  line1\n    nested line2\n  line3\nEOF\n";
    let mut p_std = Parser::new(src_std);
    let body_std = p_std.parse_body();
    assert!(!p_std.errors().has_errors());
    if let Expression::Template(parts, _) = &body_std.attributes["msg"].expr {
        let text = match &parts[0] {
            TemplatePart::Literal(s, _) => s.as_str(),
            _ => panic!("expected literal"),
        };
        assert_eq!(text, "  line1\n    nested line2\n  line3");
    } else {
        panic!("expected template expression");
    }
    let src_tpl = "msg = <<-EOF\n  hello, ${~ name ~} world!\n  EOF\n";
    let mut p_tpl = Parser::new(src_tpl);
    let body_tpl = p_tpl.parse_body();
    assert!(!p_tpl.errors().has_errors());
    let mut ctx = Context::new();
    ctx.set_variable(
        "name",
        Value::new(Type::String, ValueData::String("Alice".into())),
    );
    let (val, _) = Evaluator::new(&ctx)
        .evaluate(&body_tpl.attributes["msg"].expr)
        .unwrap();
    assert_eq!(
        val.data.as_ref(),
        &ValueData::String("hello,Aliceworld!".into())
    );
    let src_bad_hd = "msg = <<-EOF\n  line1\n\tline2\n  EOF\n";
    let mut p_bad = Parser::new(src_bad_hd);
    p_bad.parse_body();
    assert!(p_bad.errors().has_errors());
    assert!(
        p_bad
            .errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Heredoc error"))
    );
}
#[test]
fn test_parse_func_call_expand_final_rules() {
    let src_ok = "val = foo(a, b...)";
    let mut p_ok = Parser::new(src_ok);
    let body_ok = p_ok.parse_body();
    assert!(!p_ok.errors().has_errors());
    if let Expression::FuncCall(fc, _) = &body_ok.attributes["val"].expr {
        assert!(fc.expand_final);
        assert_eq!(fc.args.len(), 2);
    } else {
        panic!("expected func call");
    }
    let src_trailing = "val = foo(a, b,)";
    let mut p_trailing = Parser::new(src_trailing);
    let body_trailing = p_trailing.parse_body();
    assert!(!p_trailing.errors().has_errors());
    if let Expression::FuncCall(fc, _) = &body_trailing.attributes["val"].expr {
        assert_eq!(fc.args.len(), 2);
    } else {
        panic!("expected func call");
    }
    let src_newlines = "val = foo(\n  a\n,  b\n)";
    let mut p_newlines = Parser::new(src_newlines);
    let body_newlines = p_newlines.parse_body();
    assert!(!p_newlines.errors().has_errors());
    if let Expression::FuncCall(fc, _) = &body_newlines.attributes["val"].expr {
        assert_eq!(fc.args.len(), 2);
    } else {
        panic!("expected func call");
    }
    let src_bad_pos = "val = foo(a..., b)";
    let mut p_bad_pos = Parser::new(src_bad_pos);
    p_bad_pos.parse_body();
    assert!(p_bad_pos.errors().has_errors());
    assert!(
        p_bad_pos
            .errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Expected `)` after `...`"))
    );
    let src_bad_comma = "val = foo(a...,)";
    let mut p_bad_comma = Parser::new(src_bad_comma);
    p_bad_comma.parse_body();
    assert!(p_bad_comma.errors().has_errors());
    assert!(
        p_bad_comma
            .errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Expected `)` after `...`"))
    );
}
/// Tests additional parser coverage edge cases including heredoc common prefix,
/// variadic param variations in function blocks, heredoc directives, and traversal operators.
#[test]
fn test_parser_additional_coverage_gaps() {
    let empty_heredoc = "val = <<-EOF\n\nEOF\n";
    let mut p = Parser::new(empty_heredoc);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    assert!(body.attributes.contains_key("val"));
    assert_eq!(
        crate::parse::parser::strip_heredoc_indentation("\n").ok(),
        Some("\n".to_string())
    );
    assert_eq!(
        crate::parse::parser::strip_heredoc_indentation("\n\n").ok(),
        Some("\n\n".to_string())
    );
    let breaking_heredoc = "val = <<-EOF\n  line one\nline two\nEOF\n";
    let mut p = Parser::new(breaking_heredoc);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    assert!(body.attributes.contains_key("val"));
    let func_vp_str = r#"
        function "f1" {
            variadic_param = "extra"
            result = 42
        }
    "#;
    let mut p = Parser::new(func_vp_str);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    assert_eq!(body.functions.len(), 1);
    assert_eq!(
        body.functions[0]
            .variadic_param
            .as_ref()
            .map(|vp| vp.name.as_str()),
        Some("extra")
    );
    let func_vp_num = r#"
        function "f2" {
            variadic_param = 123
            result = 42
        }
    "#;
    let mut p = Parser::new(func_vp_num);
    let body = p.parse_body();
    assert_eq!(body.functions.len(), 1);
    assert!(body.functions[0].variadic_param.is_none());
    let func_vp_block = r#"
        function "f3" {
            other_block {
                foo = "bar"
            }
            variadic_param {
                name = "args"
                type = "list(string)"
            }
            result = 42
        }
    "#;
    let mut p = Parser::new(func_vp_block);
    let body = p.parse_body();
    assert_eq!(body.functions.len(), 1);
    assert_eq!(
        body.functions[0]
            .variadic_param
            .as_ref()
            .map(|vp| vp.name.as_str()),
        Some("args")
    );
    let func_vp_bad_name = r#"
        function "f4" {
            variadic_param {
                name = 999
                type = "string"
            }
            result = 42
        }
    "#;
    let mut p = Parser::new(func_vp_bad_name);
    let body = p.parse_body();
    assert_eq!(body.functions.len(), 1);
    assert!(body.functions[0].variadic_param.is_none());
    let func_vp_bad_type = r#"
        function "f5" {
            variadic_param {
                name = "args"
                type = "invalid type {"
            }
            result = 42
        }
    "#;
    let mut p = Parser::new(func_vp_bad_type);
    let body = p.parse_body();
    assert_eq!(body.functions.len(), 1);
    assert_eq!(
        body.functions[0]
            .variadic_param
            .as_ref()
            .map(|vp| vp.name.as_str()),
        Some("args")
    );
    assert!(
        body.functions[0]
            .variadic_param
            .as_ref()
            .and_then(|vp| vp.type_expr.as_ref())
            .is_none()
    );
    let func_vp_num_type = r#"
        function "f6" {
            variadic_param {
                name = "args"
                type = 123
            }
            result = 42
        }
    "#;
    let mut p = Parser::new(func_vp_num_type);
    let body = p.parse_body();
    assert_eq!(body.functions.len(), 1);
    assert!(
        body.functions[0]
            .variadic_param
            .as_ref()
            .and_then(|vp| vp.type_expr.as_ref())
            .is_none()
    );
    let func_vp_label = r#"
        function "f7" {
            variadic_param "rest" {
            }
            result = 42
        }
    "#;
    let mut p = Parser::new(func_vp_label);
    let body = p.parse_body();
    assert_eq!(body.functions.len(), 1);
    assert_eq!(
        body.functions[0]
            .variadic_param
            .as_ref()
            .map(|vp| vp.name.as_str()),
        Some("rest")
    );
    let func_vp_empty = r#"
        function "f8" {
            variadic_param {
            }
            result = 42
        }
    "#;
    let mut p = Parser::new(func_vp_empty);
    let body = p.parse_body();
    assert_eq!(body.functions.len(), 1);
    assert!(body.functions[0].variadic_param.is_none());
    let heredoc_directive = "val = <<EOF\n%{ if true }\nhello\n%{ endif }\nEOF\n";
    let mut p = Parser::new(heredoc_directive);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    assert!(body.attributes.contains_key("val"));
    let full_splat = "val = foo[*].bar";
    let mut p = Parser::new(full_splat);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    assert!(body.attributes.contains_key("val"));
    let bad_full_splat = "val = foo[*bar]";
    let mut p = Parser::new(bad_full_splat);
    p.parse_body();
    assert!(p.errors().has_errors());
    assert!(p.errors().errors().iter().any(|d| {
        d.error
            .to_string()
            .contains("Expected `]` to close the full splat operator")
    }));
    let eof_full_splat = "val = foo[*";
    let mut p = Parser::new(eof_full_splat);
    p.parse_body();
    assert!(p.errors().has_errors());
    let eof_index = "val = foo[1 + 2";
    let mut p = Parser::new(eof_index);
    p.parse_body();
    assert!(p.errors().has_errors());
    let float_index = "val = foo.1.5";
    let mut p = Parser::new(float_index);
    p.parse_body();
    assert!(p.errors().has_errors());
    assert!(p.errors().errors().iter().any(|d| {
        d.error
            .to_string()
            .contains("Expected an integer for legacy index")
    }));
    let bad_attr = "val = foo.+";
    let mut p = Parser::new(bad_attr);
    p.parse_body();
    assert!(p.errors().has_errors());
    assert!(p.errors().errors().iter().any(|d| {
        d.error
            .to_string()
            .contains("Expected attribute name after `.`")
    }));
    let eof_block = "block_name \"label\"";
    let mut p = Parser::new(eof_block);
    p.parse_body();
    assert!(p.errors().has_errors());
    assert!(p.errors().errors().iter().any(|d| {
        d.error
            .to_string()
            .contains("Either a quoted string block label or an opening brace")
    }));
    let func_vp_var = r#"
        function "f9" {
            variadic_param {
                name = args
                type = string
            }
            result = 42
        }
    "#;
    let mut p = Parser::new(func_vp_var);
    let body = p.parse_body();
    assert_eq!(body.functions.len(), 1);
    assert_eq!(
        body.functions[0]
            .variadic_param
            .as_ref()
            .map(|vp| vp.name.as_str()),
        Some("args")
    );
    let unclosed_if = "val = \"%{ if true } foo ${ bar }\"";
    let mut p = Parser::new(unclosed_if);
    p.parse_body();
    assert!(p.errors().has_errors());
    assert!(
        p.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Unclosed %{ if } directive"))
    );
    let unclosed_for = "val = \"%{ for x in y } foo ${ bar }\"";
    let mut p = Parser::new(unclosed_for);
    p.parse_body();
    assert!(p.errors().has_errors());
    assert!(
        p.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Unclosed %{ for } directive"))
    );
    let bad_paren = "val = (1 + 2]";
    let mut p = Parser::new(bad_paren);
    p.parse_body();
    assert!(p.errors().has_errors());
    assert!(
        p.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Expected closing parenthesis"))
    );
    let bad_tuple = "val = [1 2]";
    let mut p = Parser::new(bad_tuple);
    p.parse_body();
    assert!(p.errors().has_errors());
    assert!(
        p.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Expected comma or `]`"))
    );
    let bad_obj = "val = { a + 1 }";
    let mut p = Parser::new(bad_obj);
    p.parse_body();
    assert!(p.errors().has_errors());
    assert!(
        p.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Expected `=` or `:`"))
    );
    let bad_call = "val = foo(1 2)";
    let mut p = Parser::new(bad_call);
    p.parse_body();
    assert!(p.errors().has_errors());
    assert!(
        p.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Expected `,` or `)`"))
    );
}
#[test]
fn test_parse_block_label_unescaping() {
    let hcl = r#"
resource "aws_\"special\"\\instance" "my\nlabel\t\r" "\u0041" "\U0001F600" {
  name = "test"
}
"#;
    let mut parser = Parser::new(hcl);
    let body = parser.parse_body();
    assert!(!parser.errors().has_errors());
    assert_eq!(body.blocks.len(), 1);
    let block = &body.blocks[0];
    assert_eq!(block.block_type, "resource");
    assert_eq!(
        block.labels,
        vec!["aws_\"special\"\\instance", "my\nlabel\t\r", "A", "😀",]
    );
    let bad_hcl = r#"
resource "bad_\uD800_label" {
  name = "test"
}
"#;
    let mut bad_parser = Parser::new(bad_hcl);
    let bad_body = bad_parser.parse_body();
    assert!(bad_parser.errors().has_errors());
    assert_eq!(bad_body.blocks.len(), 1);
    assert!(
        bad_parser
            .errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Invalid unicode escape"))
    );
}
/// Tests dynamic block parsing errors for invalid iterator, invalid labels, and missing content block.
#[test]
fn test_parse_dynamic_block_invalid_arguments_and_missing_content() {
    let bad_iter = r#"
        dynamic "b" {
            for_each = []
            iterator = 123
            content {}
        }
    "#;
    let mut p1 = Parser::new(bad_iter);
    p1.parse_body();
    assert!(p1.errors().has_errors());
    assert!(
        p1.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Invalid iterator argument"))
    );
    let bad_labels = r#"
        dynamic "b" {
            for_each = []
            labels = "not_a_tuple"
            content {}
        }
    "#;
    let mut p2 = Parser::new(bad_labels);
    p2.parse_body();
    assert!(p2.errors().has_errors());
    assert!(
        p2.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Invalid labels argument"))
    );
    let missing_content = r#"
        dynamic "b" {
            for_each = []
        }
    "#;
    let mut p3 = Parser::new(missing_content);
    p3.parse_body();
    assert!(p3.errors().has_errors());
    assert!(
        p3.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Missing required block"))
    );
    let with_other_block = r#"
        dynamic "b" {
            for_each = []
            extra {}
            content {}
        }
    "#;
    let mut p4 = Parser::new(with_other_block);
    p4.parse_body();
    let with_other_no_content = r#"
        dynamic "b" {
            for_each = []
            extra {}
        }
    "#;
    let mut p5 = Parser::new(with_other_no_content);
    p5.parse_body();
    assert!(p5.errors().has_errors());
}
#[test]
fn test_parse_namespaced_func_call_zero_namespace() {
    let src = r#"val = upper("foo")"#;
    let mut p = Parser::new(src);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    if let Expression::FuncCall(fc, _) = &body.attributes["val"].expr {
        assert_eq!(fc.name.namespace.len(), 0);
        assert_eq!(fc.name.name, "upper");
        assert_eq!(fc.simple_name(), Some("upper"));
        assert_eq!(fc.name.to_string(), "upper");
        assert_eq!(fc.args.len(), 1);
    } else {
        panic!("expected func call");
    }
}
#[test]
fn test_parse_namespaced_func_call_single_namespace() {
    let src = r"val = aws::arn_parse(var.arn)";
    let mut p = Parser::new(src);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    if let Expression::FuncCall(fc, _) = &body.attributes["val"].expr {
        assert_eq!(fc.name.namespace, vec!["aws"]);
        assert_eq!(fc.name.name, "arn_parse");
        assert_eq!(fc.simple_name(), None);
        assert_eq!(fc.name.to_string(), "aws::arn_parse");
        assert_eq!(fc.args.len(), 1);
    } else {
        panic!("expected func call");
    }
}
#[test]
fn test_parse_namespaced_func_call_multi_namespace() {
    let src = r#"val = terraform::provider::aws::arn_parse("arn:aws:s3:::my_bucket")"#;
    let mut p = Parser::new(src);
    let body = p.parse_body();
    assert!(!p.errors().has_errors());
    if let Expression::FuncCall(fc, _) = &body.attributes["val"].expr {
        assert_eq!(fc.name.namespace, vec!["terraform", "provider", "aws"]);
        assert_eq!(fc.name.name, "arn_parse");
        assert_eq!(fc.simple_name(), None);
        assert_eq!(fc.name.to_string(), "terraform::provider::aws::arn_parse");
        assert_eq!(fc.args.len(), 1);
    } else {
        panic!("expected func call");
    }
}
#[test]
fn test_parse_namespaced_func_call_errors() {
    let src1 = "val = foo::()";
    let mut p1 = Parser::new(src1);
    p1.parse_body();
    assert!(p1.errors().has_errors());
    assert!(p1.errors().errors().iter().any(|d| {
        d.error
            .to_string()
            .contains("Expected identifier after '::'")
    }));
    let src2 = "val = foo::bar::";
    let mut p2 = Parser::new(src2);
    p2.parse_body();
    assert!(p2.errors().has_errors());
    assert!(p2.errors().errors().iter().any(|d| {
        d.error
            .to_string()
            .contains("Expected identifier after '::'")
    }));
    let src3 = "val = ::foo()";
    let mut p3 = Parser::new(src3);
    p3.parse_body();
    assert!(p3.errors().has_errors());
    assert!(
        p3.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Unexpected leading '::'"))
    );
    let src4 = "val = foo::bar";
    let mut p4 = Parser::new(src4);
    p4.parse_body();
    assert!(p4.errors().has_errors());
    assert!(p4.errors().errors().iter().any(|d| {
        d.error
            .to_string()
            .contains("Expected function call after namespaced identifier")
    }));
    let src_dyn = "dynamic \"setting\" {\n  for_each = [1, 2]\n}\n";
    let mut p_dyn = Parser::new(src_dyn);
    p_dyn.parse_body();
    assert!(p_dyn.errors().has_errors());
    assert!(
        p_dyn
            .errors()
            .errors()
            .iter()
            .any(|d| d.summary.as_deref() == Some("Missing required block"))
    );
    let src_if = "msg = \"hello %{if true} world\"\n";
    let mut p_if = Parser::new(src_if);
    p_if.parse_body();
    assert!(p_if.errors().has_errors());
    assert!(
        p_if.errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Unclosed %{ if } directive"))
    );
    let src_if_else = "msg = \"hello %{if true} yes %{else} no\"\n";
    let mut p_if_else = Parser::new(src_if_else);
    p_if_else.parse_body();
    assert!(p_if_else.errors().has_errors());
    assert!(
        p_if_else
            .errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Unclosed %{ if } directive"))
    );
    let src_for = "msg = \"hello %{for x in items} $x\"\n";
    let mut p_for = Parser::new(src_for);
    p_for.parse_body();
    assert!(p_for.errors().has_errors());
    assert!(
        p_for
            .errors()
            .errors()
            .iter()
            .any(|d| d.error.to_string().contains("Unclosed %{ for } directive"))
    );
}
#[test]
fn test_ast_comment_and_trivia_preservation_roundtrip() {
    let input = r#"
# Leading comment for foo
// Another leading line for foo
/* Multi-line comment
   for foo */
foo = "bar" # Inline comment for foo

# Leading block comment
service "web" {
  # Intra-block attribute comment
  port = 8080 // Inline port comment
} # Inline block close comment
"#;
    let mut parser = Parser::new(input);
    let body = parser.parse_body();
    assert!(!parser.errors().has_errors());
    let foo_attr = &body.attributes["foo"];
    assert_eq!(foo_attr.leading_comments.len(), 3);
    assert_eq!(foo_attr.leading_comments[0], "# Leading comment for foo");
    assert_eq!(
        foo_attr.leading_comments[1],
        "// Another leading line for foo"
    );
    assert!(foo_attr.leading_comments[2].contains("Multi-line comment"));
    assert_eq!(
        foo_attr.trailing_comment.as_deref(),
        Some("# Inline comment for foo")
    );
    assert_eq!(body.blocks.len(), 1);
    let block = &body.blocks[0];
    assert_eq!(block.leading_comments.len(), 1);
    assert_eq!(block.leading_comments[0], "# Leading block comment");
    assert_eq!(
        block.trailing_comment.as_deref(),
        Some("# Inline block close comment")
    );
    let port_attr = &block.body.attributes["port"];
    assert_eq!(port_attr.leading_comments.len(), 1);
    assert_eq!(
        port_attr.leading_comments[0],
        "# Intra-block attribute comment"
    );
    assert_eq!(
        port_attr.trailing_comment.as_deref(),
        Some("// Inline port comment")
    );
    let mut cst_body = crate::cst::builder::CstBody::new();
    crate::encode::EncodeBody::encode_into_body(&body, &mut cst_body).unwrap();
    let mut rendered = String::new();
    cst_body.render(&mut rendered);
    assert!(rendered.contains("# Leading comment for foo"));
    assert!(rendered.contains("// Another leading line for foo"));
    assert!(rendered.contains("Multi-line comment"));
    assert!(rendered.contains("# Inline comment for foo"));
    assert!(rendered.contains("# Leading block comment"));
    assert!(rendered.contains("# Intra-block attribute comment"));
    assert!(rendered.contains("// Inline port comment"));
    assert!(rendered.contains("# Inline block close comment"));
    let mut round_parser = Parser::new(&rendered);
    let round_body = round_parser.parse_body();
    assert!(!round_parser.errors().has_errors());
    let round_foo = &round_body.attributes["foo"];
    assert_eq!(round_foo.leading_comments.len(), 3);
    assert_eq!(
        round_foo.trailing_comment.as_deref(),
        Some("# Inline comment for foo")
    );
    let round_block = &round_body.blocks[0];
    assert_eq!(round_block.leading_comments.len(), 1);
    assert_eq!(
        round_block.trailing_comment.as_deref(),
        Some("# Inline block close comment")
    );
    let round_port = &round_block.body.attributes["port"];
    assert_eq!(round_port.leading_comments.len(), 1);
    assert_eq!(
        round_port.trailing_comment.as_deref(),
        Some("// Inline port comment")
    );
}
/// Tests template whitespace stripping and stray directive branches.
#[test]
fn test_template_branch_coverage_gaps() {
    let input = "\"${foo~}${bar}\"";
    let mut parser = Parser::new(input);
    let expr = parser.parse_expression();
    assert!(expr.is_some());
    let input2 = "\"${foo}${~bar}\"";
    let mut parser2 = Parser::new(input2);
    let expr2 = parser2.parse_expression();
    assert!(expr2.is_some());
    let input3 = "\"hello %{ else if true } world\"";
    let mut parser3 = Parser::new(input3);
    let expr3 = parser3.parse_expression();
    assert!(expr3.is_some());
    assert!(parser3.errors().has_errors());
}
/// Tests `advance_token` and `current_span_or` helper methods for fallback paths.
#[test]
fn test_parser_advance_token_and_current_span_helpers() {
    let mut parser = Parser::new("");
    let tok = parser.advance_token();
    assert_eq!(tok.text, "");
    assert_eq!(tok.kind, crate::lex::TokenKind::Ident);
    let span = parser.current_span_or(crate::span::Span::new(1, 2, 3, 4, 5, 6));
    assert_eq!(span.start_byte, 1);
    assert_eq!(span.end_byte, 2);
}
/// Tests that empty or invalid expression inside parentheses returns None (covering early return in `OParen`).
#[test]
fn test_parser_parentheses_empty_expression() {
    let mut parser = Parser::new("()");
    let expr = parser.parse_expression();
    assert!(expr.is_none());
    assert!(parser.errors().has_errors());
}
/// Tests multiline expressions with newlines between tokens in comprehensions, calls, and parenthesized conditionals.
#[test]
fn test_multiline_newlines_in_expressions() {
    let tuple_input = "[\nfor\nk,\nv\nin\ncollection\n:\nk\nif\ntrue\n]";
    let mut p_tuple = Parser::new(tuple_input);
    assert!(p_tuple.parse_expression().is_some());
    let obj_input = "{\nfor\nk,\nv\nin\ncollection\n:\nk\n=>\nv...\nif\ntrue\n}";
    let mut p_obj = Parser::new(obj_input);
    assert!(p_obj.parse_expression().is_some());
    let cond_input = "(\ntrue ?\n1 :\n2\n)";
    let mut p_cond = Parser::new(cond_input);
    assert!(p_cond.parse_expression().is_some());
    let call_empty = "func(\n)";
    let mut p_call1 = Parser::new(call_empty);
    assert!(p_call1.parse_expression().is_some());
    let call_expand = "func(\nargs...\n)";
    let mut p_call2 = Parser::new(call_expand);
    assert!(p_call2.parse_expression().is_some());
    let call_multi = "func(\n1,\n2,\n)";
    let mut p_call3 = Parser::new(call_multi);
    assert!(p_call3.parse_expression().is_some());
    let infix_input = "(1 +\n 2)";
    let mut p_infix = Parser::new(infix_input);
    assert!(p_infix.parse_expression().is_some());
    let cond_colon_newline = "(true ? 1\n : 2)";
    let mut p_cond_col = Parser::new(cond_colon_newline);
    assert!(p_cond_col.parse_expression().is_some());
    let call_trailing_comma = "func(1,\n)";
    let mut p_call_trail = Parser::new(call_trailing_comma);
    assert!(p_call_trail.parse_expression().is_some());
}
