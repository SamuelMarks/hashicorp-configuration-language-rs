//! Integration tests for the HCL static analysis and linting engine.
#![forbid(clippy::all, clippy::pedantic, clippy::nursery)]
#![forbid(missing_docs)]
#![forbid(rustdoc::missing_crate_level_docs)]
#![forbid(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::analysis::{
    Linter, type_check::ScopeSchema, type_check::TypeChecker,
};
use hashicorp_configuration_language_rs::ast::expr::{
    Conditional, Directive, Expression, ForExpr, FuncCall, NamespacedIdent, TemplatePart,
};
use hashicorp_configuration_language_rs::ast::structure::{Block, Body, DynamicBlock};
use hashicorp_configuration_language_rs::diagnostic::Diagnostics;
use hashicorp_configuration_language_rs::parse::parser::Parser;
use hashicorp_configuration_language_rs::span::Span;
use hashicorp_configuration_language_rs::types::ty::Type;

fn parse_body(input: &str) -> Body {
    let mut parser = Parser::new(input);
    parser.parse_body()
}

#[test]
fn test_lint_unused_locals_and_variables() {
    let input = r#"
        locals {
            used_local   = "hello"
            unused_local = "dead"
        }

        variable "used_var" {
            default = 10
        }

        variable "unused_var" {
            default = 20
        }

        output = "${local.used_local} - ${var.used_var}"
    "#;
    let body = parse_body(input);
    let linter = Linter::new();
    let diags = linter.lint_body(&body);
    let msgs: Vec<String> = diags.iter().map(|d| d.error.to_string()).collect();
    assert!(msgs.iter().any(|m| m.contains("unused_local")));
    assert!(msgs.iter().any(|m| m.contains("unused_var")));
    assert!(!msgs.iter().any(|m| m.contains(r#""used_local""#)));
    assert!(!msgs.iter().any(|m| m.contains(r#""used_var""#)));
}

#[test]
fn test_lint_tautological_conditionals() {
    let linter = Linter::new();

    let b1 = parse_body("val = true ? 1 : 2");
    let d1 = linter.lint_body(&b1);
    assert!(
        d1.iter()
            .any(|d| d.summary.as_deref() == Some("Condition is constant true"))
    );

    let b2 = parse_body("val = false ? 1 : 2");
    let d2 = linter.lint_body(&b2);
    assert!(
        d2.iter()
            .any(|d| d.summary.as_deref() == Some("Condition is constant false"))
    );

    // Number vs Number
    let b3 = parse_body("val = 1 == 1 ? 1 : 2");
    let d3 = linter.lint_body(&b3);
    assert!(
        d3.iter()
            .any(|d| d.summary.as_deref() == Some("Comparison of identical expressions"))
    );

    let b4 = parse_body("val = 1 != 1 ? 1 : 2");
    let d4 = linter.lint_body(&b4);
    assert!(
        d4.iter()
            .any(|d| d.summary.as_deref() == Some("Comparison of identical expressions"))
    );

    // String vs String
    let b_str = parse_body(r#"val = "a" == "a" ? 1 : 2"#);
    let d_str = linter.lint_body(&b_str);
    assert!(
        d_str
            .iter()
            .any(|d| d.summary.as_deref() == Some("Comparison of identical expressions"))
    );

    // Bool vs Bool
    let b_bool = parse_body("val = true == true ? 1 : 2");
    let d_bool = linter.lint_body(&b_bool);
    assert!(
        d_bool
            .iter()
            .any(|d| d.summary.as_deref() == Some("Comparison of identical expressions"))
    );

    // Variable vs Variable
    let b_var = parse_body("val = count == count ? 1 : 2");
    let d_var = linter.lint_body(&b_var);
    assert!(
        d_var
            .iter()
            .any(|d| d.summary.as_deref() == Some("Comparison of identical expressions"))
    );

    // Null vs Null
    let b_null = parse_body("val = null == null ? 1 : 2");
    let d_null = linter.lint_body(&b_null);
    assert!(
        d_null
            .iter()
            .any(|d| d.summary.as_deref() == Some("Comparison of identical expressions"))
    );

    let b_clean = parse_body("val = count > 0 ? 1 : 2");
    let d_clean = linter.lint_body(&b_clean);
    assert_eq!(d_clean.iter().count(), 0);

    let b_neq_diff = parse_body("val = 1 != 2 ? 1 : 2");
    let d_neq_diff = linter.lint_body(&b_neq_diff);
    assert_eq!(d_neq_diff.iter().count(), 0);

    let b_mismatch = parse_body(r#"val = 1 == "1" ? 1 : 2"#);
    let d_mismatch = linter.lint_body(&b_mismatch);
    assert_eq!(d_mismatch.iter().count(), 0);

    // Not a conditional
    let mut diags = Diagnostics::new();
    linter.lint_tautologies(
        &Expression::Bool(true, Span::new(0, 0, 1, 1, 1, 1)),
        &mut diags,
    );
    assert_eq!(diags.iter().count(), 0);
}

#[test]
fn test_lint_unreachable_template_directives() {
    let linter = Linter::new();

    let b1 = parse_body(r#"val = "hello %{ if false }unreachable%{ endif } world""#);
    let d1 = linter.lint_body(&b1);
    assert!(d1.iter().any(|d| {
        d.error
            .to_string()
            .contains("Unreachable template directive")
    }));

    let b2 = parse_body(r#"val = "hello %{ if true }always%{ else }never%{ endif }""#);
    let d2 = linter.lint_body(&b2);
    assert!(
        d2.iter()
            .any(|d| d.error.to_string().contains("Unreachable else/elif"))
    );

    let b3 = parse_body(r#"val = "hello %{ if cond }a%{ elif false }never%{ endif }""#);
    let d3 = linter.lint_body(&b3);
    assert!(
        d3.iter()
            .any(|d| d.error.to_string().contains("Unreachable template else-if"))
    );

    let sp = Span::new(0, 0, 1, 1, 1, 1);
    let mut diags = Diagnostics::new();
    // Not a template
    linter.lint_unreachable_templates(&Expression::Bool(true, sp), &mut diags);
    assert_eq!(diags.iter().count(), 0);
}

#[test]
fn test_lint_redundant_conversions() {
    let scope = ScopeSchema::new()
        .with_variable("my_list", Type::List(Box::new(Type::Number)))
        .with_variable("my_map", Type::Map(Box::new(Type::String)))
        .with_variable("my_set", Type::Set(Box::new(Type::String)));
    let tc = TypeChecker::new().with_scope(scope);
    let linter = Linter::new().with_type_checker(tc);

    let b_str = parse_body(r#"val = tostring("already string")"#);
    let d_str = linter.lint_body(&b_str);
    assert!(
        d_str
            .iter()
            .any(|d| d.summary.as_deref() == Some("Redundant `tostring` call"))
    );

    let b_str_tpl = parse_body(&format!("val = tostring(\"{}{{1}}\")", '$'));
    let d_str_tpl = linter.lint_body(&b_str_tpl);
    assert!(
        d_str_tpl
            .iter()
            .any(|d| d.summary.as_deref() == Some("Redundant `tostring` call"))
    );

    let b_num = parse_body("val = tonumber(42)");
    let d_num = linter.lint_body(&b_num);
    assert!(
        d_num
            .iter()
            .any(|d| d.summary.as_deref() == Some("Redundant `tonumber` call"))
    );

    let b_bool = parse_body("val = tobool(true)");
    let d_bool = linter.lint_body(&b_bool);
    assert!(
        d_bool
            .iter()
            .any(|d| d.summary.as_deref() == Some("Redundant `tobool` call"))
    );

    let b_list = parse_body("val = tolist(my_list)");
    let d_list = linter.lint_body(&b_list);
    assert!(
        d_list
            .iter()
            .any(|d| d.summary.as_deref() == Some("Redundant `tolist` call"))
    );

    let b_map = parse_body("val = tomap(my_map)");
    let d_map = linter.lint_body(&b_map);
    assert!(
        d_map
            .iter()
            .any(|d| d.summary.as_deref() == Some("Redundant `tomap` call"))
    );

    let b_set = parse_body("val = toset(my_set)");
    let d_set = linter.lint_body(&b_set);
    assert!(
        d_set
            .iter()
            .any(|d| d.summary.as_deref() == Some("Redundant `toset` call"))
    );

    let b_ok = parse_body("val = tostring(123)");
    let d_ok = linter.lint_body(&b_ok);
    assert_eq!(d_ok.iter().count(), 0);

    let b_bad_args = parse_body("val = tostring(123, 456)");
    let d_bad_args = linter.lint_body(&b_bad_args);
    assert_eq!(d_bad_args.iter().count(), 0);

    // Non func-call
    let mut diags = Diagnostics::new();
    linter.lint_redundant_conversions(
        &Expression::Bool(true, Span::new(0, 0, 1, 1, 1, 1)),
        &mut diags,
    );
    assert_eq!(diags.iter().count(), 0);
}

#[test]
fn test_lint_clean_run() {
    let input = r#"
        locals {
            app = "web"
        }

        variable "environment" {
            default = "production"
        }

        resource "server" "primary" {
            name = "${local.app}-${var.environment}"
            enabled = var.environment == "production" ? true : false
        }
    "#;
    let body = parse_body(input);
    let linter = Linter::new();
    let diags = linter.lint_body(&body);
    assert_eq!(diags.iter().count(), 0);
}

#[test]
fn test_lint_expression_api_and_default() {
    let linter = Linter::default();
    let sp = Span::new(0, 0, 1, 1, 1, 1);
    let tautology = Expression::Conditional(
        Box::new(Conditional {
            cond_expr: Expression::Bool(true, sp.clone()),
            true_expr: Expression::Number(
                hashicorp_configuration_language_rs::number::Number::from(1),
                sp.clone(),
            ),
            false_expr: Expression::Number(
                hashicorp_configuration_language_rs::number::Number::from(2),
                sp.clone(),
            ),
        }),
        sp,
    );
    let diags = linter.lint_expression(&tautology);
    assert_eq!(diags.iter().count(), 1);
    assert!(
        diags
            .iter()
            .any(|d| d.summary.as_deref() == Some("Condition is constant true"))
    );
}

#[test]
fn test_lint_unused_definitions_edge_cases() {
    let input = r#"
        locals {
            idx_local = "val"
        }
        variable "idx_var" {
            default = 1
        }
        # Referencing via index instead of GetAttr: local[0] and var[0]
        out = "${local[0]} ${var[0]}"
        
        # Bare module, local, var reference (no operators)
        out2 = local
        out3 = var
        out4 = other_root
    "#;
    let mut body = parse_body(input);
    let linter = Linter::new();
    let diags = linter.lint_body(&body);
    assert!(
        diags
            .iter()
            .any(|d| d.error.to_string().contains("idx_local"))
    );
    assert!(
        diags
            .iter()
            .any(|d| d.error.to_string().contains("idx_var"))
    );

    let sp = Span::new(0, 0, 1, 1, 1, 1);
    let mut var_no_labels = Block::new(
        "variable".to_string(),
        vec![],
        Body::new(sp.clone()),
        sp.clone(),
    );
    body.blocks.push(var_no_labels.clone());
    var_no_labels.labels.push("unlabeled_var".to_string());
    var_no_labels.label_spans.clear();
    body.blocks.push(var_no_labels);

    let empty_locals = Block::new(
        "locals".to_string(),
        vec![],
        Body::new(sp.clone()),
        sp.clone(),
    );
    body.blocks.push(empty_locals);

    let other_block = Block::new("resource".to_string(), vec![], Body::new(sp.clone()), sp);
    body.blocks.push(other_block);

    let diags2 = linter.lint_body(&body);
    assert!(
        diags2
            .iter()
            .any(|d| d.error.to_string().contains("unlabeled_var"))
    );
}

#[test]
fn test_lint_templates_exhaustive() -> Result<(), Box<dyn std::error::Error>> {
    let linter = Linter::new();

    let b1 = parse_body(r#"val = "hello %{ if true }world%{ endif }""#);
    let d1 = linter.lint_body(&b1);
    assert_eq!(d1.iter().count(), 0);

    let b_elif_true = parse_body(r#"val = "hello %{ if true }a%{ elif x }b%{ endif }""#);
    let d_elif_true = linter.lint_body(&b_elif_true);
    assert!(
        d_elif_true
            .iter()
            .any(|d| d.error.to_string().contains("Unreachable else/elif"))
    );

    let b2 =
        parse_body(r#"val = "hello %{ if x }a%{ elif false }never%{ elif y }maybe%{ endif }""#);
    let d2 = linter.lint_body(&b2);
    assert!(
        d2.iter()
            .any(|d| d.error.to_string().contains("Unreachable template else-if"))
    );

    let b3 = parse_body(r#"val = "hello %{ for item in [1, 2] }${item}%{ endfor }""#);
    let d3 = linter.lint_body(&b3);
    assert_eq!(d3.iter().count(), 0);

    let mut b4 = parse_body(r#"val = "text""#);
    let sp = Span::new(0, 0, 1, 1, 1, 1);
    let attr = b4.attributes.get_mut("val").ok_or("missing val")?;
    attr.expr = Expression::Template(
        vec![TemplatePart::Directive(
            Directive::Strip {
                strip_left: true,
                strip_right: false,
            },
            sp.clone(),
        )],
        sp,
    );
    let d4 = linter.lint_body(&b4);
    assert_eq!(d4.iter().count(), 0);
    Ok(())
}

#[test]
fn test_lint_body_expressions_and_recursive_types() {
    let input = r#"
        dynamic "tag" {
            for_each = [1 == 1 ? 1 : 2]
            labels   = ["env"]
            content {
                inner = 2 == 2 ? 10 : 20
            }
        }

        dynamic "nolabels" {
            for_each = [1 == 1 ? 1 : 2]
            content {
                inner = 2 == 2 ? 10 : 20
            }
        }

        tuple_val = [1 == 1 ? 1 : 2]
        obj_val   = { (1 == 1 ? "k" : "k2") = 2 == 2 ? 1 : 2 }
        paren_val = (1 == 1 ? 1 : 2)
        unary_val = -(1 == 1 ? 1 : 2)
        trav_val  = my_arr[1 == 1 ? 0 : 1]
        trav_val2 = my_arr.*
        trav_val3 = my_arr.attr
    "#;
    let mut body = parse_body(input);

    let sp = Span::new(0, 0, 1, 1, 1, 1);
    body.dynamic_blocks.push(DynamicBlock {
        block_type: "dyn_none".to_string(),
        type_span: sp.clone(),
        labels: None,
        iterator: None,
        for_each: Expression::Tuple(vec![], sp.clone()),
        content: Body::new(sp.clone()),
        span: sp,
    });

    let linter = Linter::new();
    let diags = linter.lint_body(&body);
    assert!(diags.iter().count() >= 6);
    assert!(
        diags
            .iter()
            .all(|d| d.summary.as_deref() == Some("Comparison of identical expressions"))
    );
}

#[test]
fn test_lint_for_expressions_and_namespaced_funcs() {
    let linter = Linter::new();
    let sp = Span::new(0, 0, 1, 1, 1, 1);

    let for_expr_some = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: Some("k".to_string()),
            val_var: "v".to_string(),
            collection: Box::new(Expression::Number(
                hashicorp_configuration_language_rs::number::Number::from(1),
                sp.clone(),
            )),
            key_expr: Some(Box::new(Expression::Conditional(
                Box::new(Conditional {
                    cond_expr: Expression::Bool(true, sp.clone()),
                    true_expr: Expression::Number(
                        hashicorp_configuration_language_rs::number::Number::from(1),
                        sp.clone(),
                    ),
                    false_expr: Expression::Number(
                        hashicorp_configuration_language_rs::number::Number::from(2),
                        sp.clone(),
                    ),
                }),
                sp.clone(),
            ))),
            val_expr: Box::new(Expression::Number(
                hashicorp_configuration_language_rs::number::Number::from(3),
                sp.clone(),
            )),
            cond_expr: Some(Box::new(Expression::Bool(true, sp.clone()))),
            grouping: false,
        }),
        sp.clone(),
    );
    let d_for_some = linter.lint_expression(&for_expr_some);
    assert!(
        d_for_some
            .iter()
            .any(|d| d.summary.as_deref() == Some("Condition is constant true"))
    );

    let for_expr_none = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Number(
                hashicorp_configuration_language_rs::number::Number::from(1),
                sp.clone(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::Number(
                hashicorp_configuration_language_rs::number::Number::from(3),
                sp.clone(),
            )),
            cond_expr: None,
            grouping: false,
        }),
        sp.clone(),
    );
    let d_for_none = linter.lint_expression(&for_expr_none);
    assert_eq!(d_for_none.iter().count(), 0);

    let namespaced_func = Expression::FuncCall(
        Box::new(FuncCall {
            name: NamespacedIdent {
                namespace: vec!["provider".to_string()],
                name: "call".to_string(),
                span: sp.clone(),
            },
            args: vec![Expression::Number(
                hashicorp_configuration_language_rs::number::Number::from(1),
                sp.clone(),
            )],
            expand_final: false,
        }),
        sp,
    );
    let d_ns = linter.lint_expression(&namespaced_func);
    assert_eq!(d_ns.iter().count(), 0);
}

#[test]
fn test_lint_redundant_conversions_edge_cases() {
    let linter = Linter::new();
    let body = parse_body(
        r#"
        v1 = tostring(123)
        v2 = tonumber("123")
        v3 = tobool("true")
        v4 = tolist("not_list")
        v5 = tomap("not_map")
        v6 = toset("not_set")
        v7 = length([1, 2])
        v8 = concat([1], [2])
    "#,
    );
    let diags = linter.lint_body(&body);
    assert_eq!(diags.iter().count(), 0);
}
