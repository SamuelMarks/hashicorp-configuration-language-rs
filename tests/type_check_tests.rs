//! Integration tests for the HCL static type checking engine.
#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::analysis::type_check::{
    ScopeSchema, TypeChecker, is_type_compatible, unify_all,
};
use hashicorp_configuration_language_rs::ast::BodySchema;
use hashicorp_configuration_language_rs::ast::expr::{
    BinaryOp, Expression, ForExpr, FuncCall, Traversal, TraversalOperator,
};
use hashicorp_configuration_language_rs::ast::schema::{AttributeSchema, BlockHeaderSchema};
use hashicorp_configuration_language_rs::ast::structure::Body;
use hashicorp_configuration_language_rs::diagnostic::Diagnostics;
use hashicorp_configuration_language_rs::eval::func::{FunctionParamSpec, FunctionSignature};
use hashicorp_configuration_language_rs::number::Number;
use hashicorp_configuration_language_rs::parse::parser::Parser;
use hashicorp_configuration_language_rs::span::Span;
use hashicorp_configuration_language_rs::types::Type;
use std::collections::{BTreeMap, BTreeSet};

/// Parses an HCL input string into a `Body` AST for testing.
fn parse_body(input: &str) -> Body {
    let mut parser = Parser::new(input);
    parser.parse_body()
}

/// Parses an expression string into an AST `Expression` for testing.
fn parse_expr(input: &str) -> Expression {
    let body = parse_body(&format!("val = {input}"));
    body.attributes["val"].expr.clone()
}

/// Asserts that a type-checking result failed with at least one error diagnostic.
fn assert_has_err<T: std::fmt::Debug>(res: &Result<T, Diagnostics>) {
    assert!(
        matches!(res, Err(diags) if diags.has_errors() && !diags.errors().is_empty()),
        "expected non-empty error diagnostics, got {res:?}"
    );
}

/// Asserts that a type-checking result failed with an error diagnostic containing `expected`.
fn assert_err_contains<T: std::fmt::Debug>(res: &Result<T, Diagnostics>, expected: &str) {
    assert!(
        matches!(res, Err(diags) if diags.has_errors() && diags.to_string().contains(expected)),
        "expected error diagnostic containing {expected:?}, got {res:?}"
    );
}

#[test]
fn test_is_type_compatible_permutations() {
    assert!(is_type_compatible(&Type::String, &Type::String));
    assert!(is_type_compatible(&Type::Number, &Type::Number));
    assert!(is_type_compatible(&Type::Bool, &Type::Bool));
    assert!(is_type_compatible(&Type::Dynamic, &Type::String));
    assert!(is_type_compatible(&Type::String, &Type::Dynamic));
    assert!(is_type_compatible(&Type::String, &Type::Number));
    assert!(is_type_compatible(&Type::Number, &Type::String));
    assert!(is_type_compatible(&Type::String, &Type::Bool));
    assert!(is_type_compatible(&Type::Bool, &Type::String));
    let list_num = Type::List(Box::new(Type::Number));
    let set_num = Type::Set(Box::new(Type::Number));
    assert!(is_type_compatible(&list_num, &set_num));
    let tuple_num = Type::Tuple(vec![Type::Number]);
    assert!(is_type_compatible(&tuple_num, &list_num));
    assert!(is_type_compatible(&tuple_num, &set_num));
    let tuple_mixed = Type::Tuple(vec![Type::Number, Type::String]);
    let tuple_mixed_target = Type::Tuple(vec![Type::Number, Type::String]);
    assert!(is_type_compatible(&tuple_mixed, &tuple_mixed_target));
    let tuple_diff_len = Type::Tuple(vec![Type::Number, Type::String, Type::Bool]);
    assert!(!is_type_compatible(&tuple_mixed, &tuple_diff_len));
    let tuple_incompat = Type::Tuple(vec![Type::Number, Type::Bool]);
    let tuple_incompat_target = Type::Tuple(vec![Type::Number, Type::Number]);
    assert!(!is_type_compatible(&tuple_incompat, &tuple_incompat_target));
    let obj_num = Type::Object {
        attrs: {
            let mut map = BTreeMap::new();
            map.insert("a".to_string(), Type::Number);
            map
        },
        optional_attrs: BTreeSet::new(),
    };
    let map_num = Type::Map(Box::new(Type::Number));
    assert!(is_type_compatible(&obj_num, &map_num));

    let target_obj = Type::Object {
        attrs: {
            let mut map = BTreeMap::new();
            map.insert("a".to_string(), Type::Number);
            map
        },
        optional_attrs: BTreeSet::new(),
    };
    assert!(is_type_compatible(&obj_num, &target_obj));

    let obj_mixed = Type::Object {
        attrs: {
            let mut map = BTreeMap::new();
            map.insert("a".to_string(), Type::Number);
            map.insert("b".to_string(), Type::String);
            map
        },
        optional_attrs: BTreeSet::new(),
    };
    assert!(is_type_compatible(&obj_mixed, &target_obj));

    let target_obj_open = Type::Object {
        attrs: {
            let mut map = BTreeMap::new();
            map.insert("a".to_string(), Type::Number);
            map
        },
        optional_attrs: BTreeSet::new(),
    };

    let tuple_num_incompat_target = Type::Tuple(vec![Type::String]);
    assert!(is_type_compatible(&tuple_num, &tuple_num_incompat_target));

    let map_str = Type::Map(Box::new(Type::String));
    assert!(is_type_compatible(&obj_num, &map_str));

    let obj_missing = Type::Object {
        attrs: BTreeMap::new(),
        optional_attrs: BTreeSet::new(),
    };
    assert!(!is_type_compatible(&obj_missing, &target_obj));

    let obj_incompat = Type::Object {
        attrs: {
            let mut map = BTreeMap::new();
            map.insert("a".to_string(), Type::String);
            map
        },
        optional_attrs: BTreeSet::new(),
    };
    assert!(is_type_compatible(&obj_incompat, &target_obj));
    assert!(is_type_compatible(&obj_num, &target_obj_open));
}

#[test]
fn test_unify_all() {
    assert_eq!(unify_all(&[]), Type::Dynamic);
    assert_eq!(unify_all(&[Type::Number]), Type::Number);
    assert_eq!(
        unify_all(&[Type::Number, Type::Number, Type::Number]),
        Type::Number
    );
    assert_eq!(unify_all(&[Type::Number, Type::String]), Type::String);
    assert_eq!(
        unify_all(&[Type::Bool, Type::List(Box::new(Type::Number))]),
        Type::Dynamic
    );
}

#[test]
fn test_scope_schema_builder() {
    let sig = FunctionSignature::with_static_return_type(vec![], Type::String);
    let mut scope = ScopeSchema::new()
        .with_variable("x", Type::Number)
        .with_variables(vec![("y".to_string(), Type::Bool)])
        .with_function("my_fn", sig.clone())
        .with_allow_undefined_variables(true)
        .with_allow_undefined_functions(true);
    assert_eq!(scope.get_variable("x"), Some(&Type::Number));
    assert_eq!(scope.get_variable("y"), Some(&Type::Bool));
    assert!(scope.get_function("my_fn").is_some());
    assert!(scope.allow_undefined_variables);
    assert!(scope.allow_undefined_functions);

    scope = scope.with_function("other_fn".to_string(), sig);
    assert!(scope.get_function("other_fn").is_some());

    let child = scope.child_scope();
    assert_eq!(child.get_variable("x"), Some(&Type::Number));
}

#[test]
fn test_type_checker_primitives_and_collections() {
    let checker = TypeChecker::new();
    let body_null = parse_body("val = null");
    assert_eq!(
        checker.infer_expression_type(&body_null.attributes["val"].expr),
        Ok(Type::Dynamic)
    );
    let body_bool = parse_body("val = true");
    assert_eq!(
        checker.infer_expression_type(&body_bool.attributes["val"].expr),
        Ok(Type::Bool)
    );
    let body_num = parse_body("val = 42");
    assert_eq!(
        checker.infer_expression_type(&body_num.attributes["val"].expr),
        Ok(Type::Number)
    );
    let body_str = parse_body(r#"val = "hello""#);
    assert_eq!(
        checker.infer_expression_type(&body_str.attributes["val"].expr),
        Ok(Type::String)
    );
    let body_tup = parse_body("val = [1, 2, 3]");
    assert_eq!(
        checker.infer_expression_type(&body_tup.attributes["val"].expr),
        Ok(Type::Tuple(vec![Type::Number, Type::Number, Type::Number]))
    );
    let body_obj = parse_body(r#"val = { a = 1, b = "two" }"#);
    let mut expected_attrs = BTreeMap::new();
    expected_attrs.insert("a".to_string(), Type::Number);
    expected_attrs.insert("b".to_string(), Type::String);
    assert_eq!(
        checker.infer_expression_type(&body_obj.attributes["val"].expr),
        Ok(Type::Object {
            attrs: expected_attrs,
            optional_attrs: BTreeSet::new(),
        })
    );
    let body_paren = parse_body("val = (10)");
    assert_eq!(
        checker.infer_expression_type(&body_paren.attributes["val"].expr),
        Ok(Type::Number)
    );
    assert_eq!(
        checker.check_expression(&body_num.attributes["val"].expr, &Type::Number),
        Ok(Type::Number)
    );
    assert_eq!(
        checker.check_expression(&body_str.attributes["val"].expr, &Type::Bool),
        Ok(Type::String)
    );
}

#[test]
fn test_type_checker_binary_and_unary_ops() {
    let checker = TypeChecker::new();
    let b1 = parse_body("val = 1 + 2 * 3 - 4 / 2 % 1");
    assert_eq!(
        checker.infer_expression_type(&b1.attributes["val"].expr),
        Ok(Type::Number)
    );
    let b2 = parse_body("val = 1 < 2");
    assert_eq!(
        checker.infer_expression_type(&b2.attributes["val"].expr),
        Ok(Type::Bool)
    );
    let b3 = parse_body("val = true && false || true");
    assert_eq!(
        checker.infer_expression_type(&b3.attributes["val"].expr),
        Ok(Type::Bool)
    );
    let b4 = parse_body("val = 1 == 2");
    assert_eq!(
        checker.infer_expression_type(&b4.attributes["val"].expr),
        Ok(Type::Bool)
    );
    let b5 = parse_body("val = !true");
    assert_eq!(
        checker.infer_expression_type(&b5.attributes["val"].expr),
        Ok(Type::Bool)
    );
    let b6 = parse_body("val = -42");
    assert_eq!(
        checker.infer_expression_type(&b6.attributes["val"].expr),
        Ok(Type::Number)
    );
}

#[test]
fn test_type_checker_conditionals() {
    let checker = TypeChecker::new();
    let b1 = parse_body("val = true ? 1 : 2");
    assert_eq!(
        checker.infer_expression_type(&b1.attributes["val"].expr),
        Ok(Type::Number)
    );
    let b2 = parse_body(r#"val = false ? "yes" : "no""#);
    assert_eq!(
        checker.infer_expression_type(&b2.attributes["val"].expr),
        Ok(Type::String)
    );
}

#[test]
fn test_type_checker_for_expressions() {
    let span = Span::default();
    let scope = ScopeSchema::new()
        .with_variable("items", Type::List(Box::new(Type::Number)))
        .with_variable("tags", Type::Map(Box::new(Type::String)));
    let checker = TypeChecker::new().with_scope(scope);
    let for_tuple = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "x".to_string(),
            collection: Box::new(Expression::Variable("items".to_string(), span.clone())),
            key_expr: None,
            val_expr: Box::new(Expression::BinaryOp(
                BinaryOp::Mul,
                Box::new(Expression::Variable("x".to_string(), span.clone())),
                Box::new(Expression::Number(Number::from(2), span.clone())),
                span.clone(),
            )),
            cond_expr: None,
            grouping: false,
        }),
        span.clone(),
    );
    assert_eq!(
        checker.infer_expression_type(&for_tuple),
        Ok(Type::List(Box::new(Type::Number)))
    );
    let for_obj = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: Some("k".to_string()),
            val_var: "v".to_string(),
            collection: Box::new(Expression::Variable("tags".to_string(), span.clone())),
            key_expr: Some(Box::new(Expression::Variable(
                "k".to_string(),
                span.clone(),
            ))),
            val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
            cond_expr: None,
            grouping: false,
        }),
        span.clone(),
    );
    assert_eq!(
        checker.infer_expression_type(&for_obj),
        Ok(Type::Map(Box::new(Type::String)))
    );
    let for_group = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: Some("k".to_string()),
            val_var: "v".to_string(),
            collection: Box::new(Expression::Variable("tags".to_string(), span.clone())),
            key_expr: Some(Box::new(Expression::Variable(
                "k".to_string(),
                span.clone(),
            ))),
            val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
            cond_expr: None,
            grouping: true,
        }),
        span.clone(),
    );
    assert_eq!(
        checker.infer_expression_type(&for_group),
        Ok(Type::Map(Box::new(Type::List(Box::new(Type::String)))))
    );
    let for_cond = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "x".to_string(),
            collection: Box::new(Expression::Variable("items".to_string(), span.clone())),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("x".to_string(), span.clone())),
            cond_expr: Some(Box::new(Expression::BinaryOp(
                BinaryOp::Greater,
                Box::new(Expression::Variable("x".to_string(), span.clone())),
                Box::new(Expression::Number(Number::from(0), span.clone())),
                span.clone(),
            ))),
            grouping: false,
        }),
        span,
    );
    assert_eq!(
        checker.infer_expression_type(&for_cond),
        Ok(Type::List(Box::new(Type::Number)))
    );
}

#[test]
fn test_type_checker_traversals() {
    let mut user_attrs = BTreeMap::new();
    user_attrs.insert("name".to_string(), Type::String);
    user_attrs.insert("age".to_string(), Type::Number);
    let user_ty = Type::Object {
        attrs: user_attrs,
        optional_attrs: BTreeSet::new(),
    };
    let scope = ScopeSchema::new()
        .with_variable("user", user_ty.clone())
        .with_variable("scores", Type::List(Box::new(Type::Number)))
        .with_variable("pair", Type::Tuple(vec![Type::String, Type::Bool]))
        .with_variable("config", Type::Map(Box::new(Type::String)));
    let checker = TypeChecker::new().with_scope(scope);
    let b_var = parse_body("val = user");
    assert_eq!(
        checker.infer_expression_type(&b_var.attributes["val"].expr),
        Ok(user_ty)
    );
    let b_name = parse_body("val = user.name");
    assert_eq!(
        checker.infer_expression_type(&b_name.attributes["val"].expr),
        Ok(Type::String)
    );
    let b_bad_attr = parse_body("val = user.nonexistent");
    assert_has_err(&checker.infer_expression_type(&b_bad_attr.attributes["val"].expr));
    let b_list_idx = parse_body("val = scores[0]");
    assert_eq!(
        checker.infer_expression_type(&b_list_idx.attributes["val"].expr),
        Ok(Type::Number)
    );
    let b_tup_0 = parse_body("val = pair[0]");
    assert_eq!(
        checker.infer_expression_type(&b_tup_0.attributes["val"].expr),
        Ok(Type::String)
    );
    let b_tup_1 = parse_body("val = pair.1");
    assert_eq!(
        checker.infer_expression_type(&b_tup_1.attributes["val"].expr),
        Ok(Type::Bool)
    );
    let b_tup_oob = parse_body("val = pair[10]");
    assert_has_err(&checker.infer_expression_type(&b_tup_oob.attributes["val"].expr));
    let b_map_idx = parse_body(r#"val = config["env"]"#);
    assert_eq!(
        checker.infer_expression_type(&b_map_idx.attributes["val"].expr),
        Ok(Type::String)
    );
    let b_splat = parse_body("val = scores.*");
    assert_eq!(
        checker.infer_expression_type(&b_splat.attributes["val"].expr),
        Ok(Type::List(Box::new(Type::Number)))
    );
    let b_undef = parse_body("val = missing_var");
    assert_has_err(&checker.infer_expression_type(&b_undef.attributes["val"].expr));
    let perm_checker =
        TypeChecker::new().with_scope(ScopeSchema::new().with_allow_undefined_variables(true));
    assert_eq!(
        perm_checker.infer_expression_type(&b_undef.attributes["val"].expr),
        Ok(Type::Dynamic)
    );
}

#[test]
fn test_type_checker_function_calls() {
    let sig = FunctionSignature::with_static_return_type(
        vec![
            FunctionParamSpec::new("a", Type::Number),
            FunctionParamSpec::new("b", Type::Number),
        ],
        Type::Number,
    );
    let var_sig = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("first", Type::String)],
        Type::String,
    )
    .with_variadic(FunctionParamSpec::new("rest", Type::String));
    let scope = ScopeSchema::new()
        .with_function("add", sig)
        .with_function("concat_all", var_sig);
    let checker = TypeChecker::new().with_scope(scope);
    let b1 = parse_body("val = add(10, 20)");
    assert_eq!(
        checker.infer_expression_type(&b1.attributes["val"].expr),
        Ok(Type::Number)
    );
    let b_few = parse_body("val = add(10)");
    assert_has_err(&checker.infer_expression_type(&b_few.attributes["val"].expr));
    let b_many = parse_body("val = add(10, 20, 30)");
    assert_has_err(&checker.infer_expression_type(&b_many.attributes["val"].expr));
    let b_mismatch = parse_body(r#"val = add("not_num", [1, 2])"#);
    assert_has_err(&checker.infer_expression_type(&b_mismatch.attributes["val"].expr));
    let b_var1 = parse_body(r#"val = concat_all("a")"#);
    assert_eq!(
        checker.infer_expression_type(&b_var1.attributes["val"].expr),
        Ok(Type::String)
    );
    let b_var2 = parse_body(r#"val = concat_all("a", "b", "c")"#);
    assert_eq!(
        checker.infer_expression_type(&b_var2.attributes["val"].expr),
        Ok(Type::String)
    );
    let b_range = parse_body("val = range(5)");
    assert_eq!(
        checker.infer_expression_type(&b_range.attributes["val"].expr),
        Ok(Type::List(Box::new(Type::Number)))
    );
    let b_undef_fn = parse_body("val = nonexistent_function(1)");
    assert_has_err(&checker.infer_expression_type(&b_undef_fn.attributes["val"].expr));
}

#[test]
fn test_type_checker_body_validation() {
    let schema = BodySchema::new()
        .with_attribute(AttributeSchema::required("name").with_type(Type::String))
        .with_attribute(AttributeSchema::optional("age").with_type(Type::Number))
        .with_block(
            BlockHeaderSchema::new("server", vec!["name".to_string()]).with_body_schema(
                BodySchema::new()
                    .with_attribute(AttributeSchema::required("ip").with_type(Type::String)),
            ),
        );
    let checker = TypeChecker::new();
    let b_valid = parse_body(
        r#"
            name = "production"
            age = 30
            server "web" {
                ip = "192.168.1.1"
            }
        "#,
    );
    assert_eq!(checker.check_body(&b_valid, &schema), Ok(()));
    let b_missing = parse_body(
        r"
            age = 30
        ",
    );
    assert_err_contains(&checker.check_body(&b_missing, &schema), "is required");
    let b_typo = parse_body(
        r#"
            name = "production"
            nane = "typo"
        "#,
    );
    assert_err_contains(&checker.check_body(&b_typo, &schema), "not expected here");
    let b_bad_type = parse_body(
        r"
            name = [1, 2, 3]
        ",
    );
    assert_err_contains(
        &checker.check_body(&b_bad_type, &schema),
        "Invalid value for attribute",
    );
    let b_labels = parse_body(
        r#"
            name = "test"
            server "web" "extra_label" {
                ip = "10.0.0.1"
            }
        "#,
    );
    assert_err_contains(
        &checker.check_body(&b_labels, &schema),
        "Wrong number of labels",
    );
    let b_bad_block = parse_body(
        r#"
            name = "test"
            servr "web" {
                ip = "10.0.0.1"
            }
        "#,
    );
    let res_bad_block = checker.check_body(&b_bad_block, &schema);
    assert_err_contains(&res_bad_block, "Blocks of type");
    assert!(matches!(
        &res_bad_block,
        Err(diags) if diags.errors().first().and_then(|d| d.detail.as_deref()) == Some(r#"Did you mean "server"?"#)
    ));
    let b_unrelated = parse_body(
        r#"
            name = "test"
            totally_unrelated_xyz "web" {}
        "#,
    );
    assert_has_err(&checker.check_body(&b_unrelated, &schema));
    let schema_bare = BodySchema::new().with_block(BlockHeaderSchema::new("bare_block", vec![]));
    let b_bare = parse_body("bare_block {}");
    assert_eq!(checker.check_body(&b_bare, &schema_bare), Ok(()));
    let b_inner_err = parse_body(
        r#"
            name = "test"
            server "web" {
                wrong_attr = "oops"
            }
        "#,
    );
    assert_err_contains(
        &checker.check_body(&b_inner_err, &schema),
        "is not expected here",
    );
}

#[test]
fn test_type_checker_dynamic_blocks() {
    let schema = BodySchema::new().with_block(
        BlockHeaderSchema::new("setting", vec!["name".to_string()]).with_body_schema(
            BodySchema::new()
                .with_attribute(AttributeSchema::required("val").with_type(Type::Number)),
        ),
    );
    let scope = ScopeSchema::new().with_variable("configs", Type::List(Box::new(Type::Number)));
    let checker = TypeChecker::new().with_scope(scope);
    let b_dyn = parse_body(
        r#"
            dynamic "setting" {
                for_each = configs
                labels = ["dynamic_label"]
                content {
                    val = setting.value
                }
            }
        "#,
    );
    assert_eq!(checker.check_body(&b_dyn, &schema), Ok(()));
    let b_dyn_err = parse_body(
        r#"
            dynamic "setting" {
                for_each = 123
                labels = ["lbl"]
                content {
                    val = setting.value
                }
            }
        "#,
    );
    assert_has_err(&checker.check_body(&b_dyn_err, &schema));
    let b_dyn_lbls = parse_body(
        r#"
            dynamic "setting" {
                for_each = configs
                labels = ["lbl1", "lbl2"]
                content {
                    val = setting.value
                }
            }
        "#,
    );
    assert_has_err(&checker.check_body(&b_dyn_lbls, &schema));
    let b_dyn_unexp = parse_body(
        r#"
            dynamic "unknown_block" {
                for_each = configs
                content {
                    val = 1
                }
            }
        "#,
    );
    assert_has_err(&checker.check_body(&b_dyn_unexp, &schema));
}

/// Tests compatibility between complex types, including tuple length and element mismatches,
/// and object attributes, types, and optional attributes.
#[test]
fn test_is_type_compatible_edge_cases() {
    let tup1 = Type::Tuple(vec![Type::Number, Type::Bool]);
    let tup2 = Type::Tuple(vec![Type::Number, Type::List(Box::new(Type::Number))]);
    assert!(!is_type_compatible(&tup1, &tup2));
    let mut a_attrs = BTreeMap::new();
    a_attrs.insert("a".to_string(), Type::Number);
    let mut b_attrs = BTreeMap::new();
    b_attrs.insert("a".to_string(), Type::List(Box::new(Type::Bool)));
    let obj_a = Type::Object {
        attrs: a_attrs,
        optional_attrs: BTreeSet::new(),
    };
    let obj_b = Type::Object {
        attrs: b_attrs,
        optional_attrs: BTreeSet::new(),
    };
    assert!(!is_type_compatible(&obj_a, &obj_b));
    let mut req_attrs = BTreeMap::new();
    req_attrs.insert("req".to_string(), Type::String);
    let obj_req = Type::Object {
        attrs: req_attrs,
        optional_attrs: BTreeSet::new(),
    };
    assert!(!is_type_compatible(&obj_a, &obj_req));
    let map_bool = Type::Map(Box::new(Type::List(Box::new(Type::Bool))));
    assert!(!is_type_compatible(&obj_a, &map_bool));
    let list_bool = Type::List(Box::new(Type::List(Box::new(Type::Bool))));
    assert!(!is_type_compatible(&tup1, &list_bool));
    assert!(!is_type_compatible(&Type::Number, &Type::Tuple(vec![])));
}

/// Tests [`TypeChecker`] public methods including scope access, [`TypeChecker::check_traversal`],
/// [`TypeChecker::check_func_call`], and [`TypeChecker::check_expression`].
#[test]
fn test_type_checker_scope_and_methods() {
    let mut checker = TypeChecker::new();
    checker
        .scope_mut()
        .variables
        .insert("x".to_string(), Type::Number);
    checker.scope_mut().functions.insert(
        "my_fn".to_string(),
        FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("s", Type::String)],
            Type::String,
        ),
    );
    assert_eq!(checker.scope().get_variable("x"), Some(&Type::Number));
    let expr_num = parse_expr("42");
    assert_eq!(
        checker.check_expression(&expr_num, &Type::Number),
        Ok(Type::Number)
    );
    assert_has_err(&checker.check_expression(&expr_num, &Type::List(Box::new(Type::Bool))));
    let trav_ok = Traversal {
        expr: Box::new(Expression::Variable(
            "x".to_string(),
            Span::new(0, 0, 0, 0, 0, 0),
        )),
        operators: vec![],
    };
    assert_eq!(checker.check_traversal(&trav_ok), Ok(Type::Number));
    let trav_err = Traversal {
        expr: Box::new(Expression::Variable(
            "missing".to_string(),
            Span::new(0, 0, 0, 0, 0, 0),
        )),
        operators: vec![],
    };
    assert_has_err(&checker.check_traversal(&trav_err));
    let fc_custom = FuncCall {
        name: "my_fn".into(),
        args: vec![Expression::String(
            "HELLO".to_string(),
            Span::new(0, 0, 0, 0, 0, 0),
        )],
        expand_final: false,
    };
    assert_eq!(
        checker.check_func_call(&fc_custom, &Span::new(0, 0, 0, 0, 0, 0)),
        Ok(Type::String)
    );
    let fc_range = FuncCall {
        name: "range".into(),
        args: vec![Expression::Number(5.into(), Span::new(0, 0, 0, 0, 0, 0))],
        expand_final: false,
    };
    assert_eq!(
        checker.check_func_call(&fc_range, &Span::new(0, 0, 0, 0, 0, 0)),
        Ok(Type::List(Box::new(Type::Number)))
    );
    let fc_err = FuncCall {
        name: "nonexistent_fn".into(),
        args: vec![],
        expand_final: false,
    };
    assert_has_err(&checker.check_func_call(&fc_err, &Span::new(0, 0, 0, 0, 0, 0)));
}

/// Tests dynamic blocks with various collection types, invalid labels, custom iterators,
/// and blocks without nested body schemas.
#[test]
fn test_type_checker_dynamic_blocks_comprehensive() {
    let schema = BodySchema::new()
        .with_block(
            BlockHeaderSchema::new("item", vec!["id".to_string()]).with_body_schema(
                BodySchema::new()
                    .with_attribute(AttributeSchema::required("val").with_type(Type::Number)),
            ),
        )
        .with_block(BlockHeaderSchema::new("no_schema_block", vec![]));
    let mut scope = ScopeSchema::new();
    scope
        .variables
        .insert("set_coll".to_string(), Type::Set(Box::new(Type::Number)));
    scope
        .variables
        .insert("map_coll".to_string(), Type::Map(Box::new(Type::Number)));
    scope.variables.insert(
        "tup_coll".to_string(),
        Type::Tuple(vec![Type::Number, Type::Number]),
    );
    let mut obj_attrs = BTreeMap::new();
    obj_attrs.insert("k1".to_string(), Type::Number);
    obj_attrs.insert("k2".to_string(), Type::Number);
    scope.variables.insert(
        "obj_coll".to_string(),
        Type::Object {
            attrs: obj_attrs,
            optional_attrs: BTreeSet::new(),
        },
    );
    scope
        .variables
        .insert("dyn_coll".to_string(), Type::Dynamic);
    let checker = TypeChecker::new().with_scope(scope);
    for coll in ["set_coll", "map_coll", "tup_coll", "obj_coll", "dyn_coll"] {
        let body_str = format!(
            r#"
                dynamic "item" {{
                    for_each = {coll}
                    iterator = my_iter
                    labels = ["lbl"]
                    content {{
                        val = my_iter.value
                    }}
                }}
            "#
        );
        let b = parse_body(&body_str);
        assert_eq!(checker.check_body(&b, &schema), Ok(()));
    }
    let b_no_schema = parse_body(
        r#"
            dynamic "no_schema_block" {
                for_each = set_coll
                content {}
            }
        "#,
    );
    assert_eq!(checker.check_body(&b_no_schema, &schema), Ok(()));
    let b_bad_lbl = parse_body(
        r#"
            dynamic "item" {
                for_each = set_coll
                labels = [[1, 2]]
                content {
                    val = 1
                }
            }
        "#,
    );
    assert_has_err(&checker.check_body(&b_bad_lbl, &schema));
    let b_dyn_typo = parse_body(
        r#"
            dynamic "itim" {
                for_each = set_coll
                content {}
            }
        "#,
    );
    let res_dyn_typo = checker.check_body(&b_dyn_typo, &schema);
    assert!(matches!(
        &res_dyn_typo,
        Err(diags) if diags.errors().first().and_then(|d| d.detail.as_deref()) == Some(r#"Did you mean "item"?"#)
    ));
}

/// Tests template directives including conditional if, `else_if`, else, and for directives
/// with various collection types and error conditions.
#[test]
fn test_type_checker_template_directives_comprehensive() {
    let mut scope = ScopeSchema::new();
    scope.variables.insert("bool_val".to_string(), Type::Bool);
    scope.variables.insert("num_val".to_string(), Type::Number);
    scope
        .variables
        .insert("set_coll".to_string(), Type::Set(Box::new(Type::String)));
    scope
        .variables
        .insert("map_coll".to_string(), Type::Map(Box::new(Type::String)));
    scope.variables.insert(
        "tup_coll".to_string(),
        Type::Tuple(vec![Type::String, Type::String]),
    );
    let mut obj_attrs = BTreeMap::new();
    obj_attrs.insert("k".to_string(), Type::String);
    scope.variables.insert(
        "obj_coll".to_string(),
        Type::Object {
            attrs: obj_attrs,
            optional_attrs: BTreeSet::new(),
        },
    );
    scope
        .variables
        .insert("dyn_coll".to_string(), Type::Dynamic);
    let checker = TypeChecker::new().with_scope(scope);
    let b_if = parse_body(
        r#"
            val = "%{if bool_val}yes%{elif !bool_val}no%{else}maybe%{endif}"
        "#,
    );
    assert_eq!(
        checker.infer_expression_type(&b_if.attributes["val"].expr),
        Ok(Type::String)
    );
    let b_if_err = parse_body(
        r#"
            val = "%{if [1]}yes%{endif}"
        "#,
    );
    assert_has_err(&checker.infer_expression_type(&b_if_err.attributes["val"].expr));
    let b_elif_err = parse_body(
        r#"
            val = "%{if bool_val}yes%{elif [1]}no%{endif}"
        "#,
    );
    assert_has_err(&checker.infer_expression_type(&b_elif_err.attributes["val"].expr));
    for coll in ["set_coll", "map_coll", "tup_coll", "obj_coll", "dyn_coll"] {
        let body_str = format!(r#"val = "%{{for k, v in {coll}}}${{k}}=${{v}} %{{endfor}}""#);
        let b_for = parse_body(&body_str);
        assert_eq!(
            checker.infer_expression_type(&b_for.attributes["val"].expr),
            Ok(Type::String)
        );
    }
    let b_for_bad = parse_body(
        r#"
            val = "%{for x in 123}${x}%{endfor}"
        "#,
    );
    assert_has_err(&checker.infer_expression_type(&b_for_bad.attributes["val"].expr));
    let mut diags = Diagnostics::new();
    let span = Span::default();
    let strip_part = hashicorp_configuration_language_rs::ast::expr::TemplatePart::Directive(
        hashicorp_configuration_language_rs::ast::expr::Directive::Strip {
            strip_left: true,
            strip_right: true,
        },
        span,
    );
    checker.check_template_part(&strip_part, checker.scope(), &mut diags);
    assert!(!diags.has_errors());
}

/// Tests static type inference for object keys, undefined variables, conditionals,
/// binary operations, unary operations, and for expressions.
#[test]
fn test_type_checker_expressions_comprehensive() {
    let mut scope = ScopeSchema::new();
    scope.variables.insert("x".to_string(), Type::Number);
    scope.variables.insert("b".to_string(), Type::Bool);
    scope.variables.insert("s".to_string(), Type::String);
    let checker = TypeChecker::new().with_scope(scope);
    let b_obj_key_bad = parse_body("val = { [1] = 2 }");
    assert_has_err(&checker.infer_expression_type(&b_obj_key_bad.attributes["val"].expr));
    let b_obj_key_good = parse_body("val = { (123) = 2 }");
    assert!(matches!(
        checker.infer_expression_type(&b_obj_key_good.attributes["val"].expr),
        Ok(Type::Object { .. })
    ));
    let loose_checker =
        TypeChecker::new().with_scope(ScopeSchema::new().with_allow_undefined_variables(true));
    let b_undef = parse_body("val = undef_var");
    assert_eq!(
        loose_checker.infer_expression_type(&b_undef.attributes["val"].expr),
        Ok(Type::Dynamic)
    );
    let b_cond_bad_cond = parse_body("val = [1] ? 1 : 2");
    assert_has_err(&checker.infer_expression_type(&b_cond_bad_cond.attributes["val"].expr));
    let b_cond_incompat = parse_body("val = b ? 1 : [1]");
    assert_has_err(&checker.infer_expression_type(&b_cond_incompat.attributes["val"].expr));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] + 1")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("1 + [1]")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] - 1")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] * 1")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] / 1")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] % 1")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] < 1")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("1 < [1]")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] <= 1")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] > 1")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] >= 1")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] && true")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("true && [1]")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("[1] || false")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("false || [1]")));
    assert_eq!(
        checker.infer_expression_type(&parse_expr("true == [1]")),
        Ok(Type::Bool)
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr("![1]")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("-[1]")));
}

#[test]
fn test_type_checker_for_expressions_comprehensive() {
    let checker = TypeChecker::new();
    let span = Span::default();
    let bad_for_coll = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Number(123.into(), span.clone())),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
            cond_expr: None,
            grouping: false,
        }),
        span.clone(),
    );
    assert_has_err(&checker.infer_expression_type(&bad_for_coll));
    let bad_for_cond = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![Expression::Number(1.into(), span.clone())],
                span.clone(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
            cond_expr: Some(Box::new(Expression::Tuple(vec![], span.clone()))),
            grouping: false,
        }),
        span.clone(),
    );
    assert_has_err(&checker.infer_expression_type(&bad_for_cond));
    let bad_for_key = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![Expression::Number(1.into(), span.clone())],
                span.clone(),
            )),
            key_expr: Some(Box::new(Expression::Tuple(vec![], span.clone()))),
            val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
            cond_expr: None,
            grouping: false,
        }),
        span.clone(),
    );
    assert_has_err(&checker.infer_expression_type(&bad_for_key));
    let for_group = Expression::ForExpr(
        Box::new(ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![Expression::String("a".to_string(), span.clone())],
                span.clone(),
            )),
            key_expr: Some(Box::new(Expression::Variable(
                "v".to_string(),
                span.clone(),
            ))),
            val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
            cond_expr: None,
            grouping: true,
        }),
        span,
    );
    assert_eq!(
        checker.infer_expression_type(&for_group),
        Ok(Type::Map(Box::new(Type::List(Box::new(Type::String)))))
    );
}

#[test]
fn test_type_checker_for_expressions_direct_collections() {
    let span = Span::default();
    let mut coll_scope = ScopeSchema::new();
    coll_scope
        .variables
        .insert("s".to_string(), Type::Set(Box::new(Type::Number)));
    coll_scope
        .variables
        .insert("m".to_string(), Type::Map(Box::new(Type::Number)));
    coll_scope.variables.insert(
        "t".to_string(),
        Type::Tuple(vec![Type::Number, Type::Number]),
    );
    let mut o_attrs = BTreeMap::new();
    o_attrs.insert("k".to_string(), Type::Number);
    coll_scope.variables.insert(
        "o".to_string(),
        Type::Object {
            attrs: o_attrs,
            optional_attrs: BTreeSet::new(),
        },
    );
    coll_scope.variables.insert("d".to_string(), Type::Dynamic);
    let coll_checker = TypeChecker::new().with_scope(coll_scope);
    for (var, expected_elem) in [
        ("s", Type::Number),
        ("m", Type::Number),
        ("t", Type::Number),
        ("o", Type::Number),
        ("d", Type::Dynamic),
    ] {
        let for_expr_direct = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(Expression::Variable(var.to_string(), span.clone())),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                cond_expr: Some(Box::new(Expression::Bool(true, span.clone()))),
                grouping: false,
            }),
            span.clone(),
        );
        assert_eq!(
            coll_checker.infer_expression_type(&for_expr_direct),
            Ok(Type::List(Box::new(expected_elem)))
        );
    }
}

/// Tests static type checking for `GetAttr`, `Index`, `LegacyIndex`, and splat operators across
/// Dynamic, Object, Map, List, Set, Tuple, and unsupported types.
#[test]
fn test_type_checker_traversals_comprehensive() {
    let mut scope = ScopeSchema::new();
    scope.variables.insert("d".to_string(), Type::Dynamic);
    scope
        .variables
        .insert("m".to_string(), Type::Map(Box::new(Type::Number)));
    let mut o_attrs = BTreeMap::new();
    o_attrs.insert("name".to_string(), Type::String);
    scope.variables.insert(
        "o".to_string(),
        Type::Object {
            attrs: o_attrs,
            optional_attrs: BTreeSet::new(),
        },
    );
    scope
        .variables
        .insert("l".to_string(), Type::List(Box::new(Type::String)));
    scope
        .variables
        .insert("s".to_string(), Type::Set(Box::new(Type::String)));
    scope.variables.insert(
        "t".to_string(),
        Type::Tuple(vec![Type::String, Type::Number]),
    );
    scope.variables.insert("str_key".to_string(), Type::String);
    scope.variables.insert("n".to_string(), Type::Number);
    scope.variables.insert("b".to_string(), Type::Bool);
    let checker = TypeChecker::new().with_scope(scope);
    assert_eq!(
        checker.infer_expression_type(&parse_expr("d.foo")),
        Ok(Type::Dynamic)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("m.any_key")),
        Ok(Type::Number)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("o.name")),
        Ok(Type::String)
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr("o.missing")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("n.attr")));
    assert_eq!(
        checker.infer_expression_type(&parse_expr("d[0]")),
        Ok(Type::Dynamic)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("l[0]")),
        Ok(Type::String)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("s[0]")),
        Ok(Type::String)
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr("l[[1]]")));
    assert_eq!(
        checker.infer_expression_type(&parse_expr("t[0]")),
        Ok(Type::String)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("t[1]")),
        Ok(Type::Number)
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr("t[99]")));
    assert_has_err(&checker.infer_expression_type(&parse_expr("t[[1]]")));
    assert_eq!(
        checker.infer_expression_type(&parse_expr("t[n]")),
        Ok(Type::String)
    );
    let float_trav = Traversal {
        expr: Box::new(Expression::Variable("t".to_string(), Span::default())),
        operators: vec![TraversalOperator::Index(
            Expression::Number(
                hashicorp_configuration_language_rs::number::Number(bigdecimal::BigDecimal::from(
                    -1,
                )),
                Span::default(),
            ),
            Span::default(),
        )],
    };
    assert_eq!(checker.check_traversal(&float_trav), Ok(Type::String));
    assert_eq!(
        checker.infer_expression_type(&parse_expr(r#"m["k"]"#)),
        Ok(Type::Number)
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr("m[[1]]")));
}

#[test]
fn test_type_checker_traversals_objects_legacy_and_splats() {
    let mut scope = ScopeSchema::new();
    scope.variables.insert("d".to_string(), Type::Dynamic);
    let mut o_attrs = BTreeMap::new();
    o_attrs.insert("name".to_string(), Type::String);
    scope.variables.insert(
        "o".to_string(),
        Type::Object {
            attrs: o_attrs,
            optional_attrs: BTreeSet::new(),
        },
    );
    scope
        .variables
        .insert("l".to_string(), Type::List(Box::new(Type::String)));
    scope
        .variables
        .insert("s".to_string(), Type::Set(Box::new(Type::String)));
    scope.variables.insert(
        "t".to_string(),
        Type::Tuple(vec![Type::String, Type::Number]),
    );
    scope.variables.insert("str_key".to_string(), Type::String);
    scope.variables.insert("n".to_string(), Type::Number);
    scope.variables.insert("b".to_string(), Type::Bool);
    let checker = TypeChecker::new().with_scope(scope);
    assert_eq!(
        checker.infer_expression_type(&parse_expr(r#"o["name"]"#)),
        Ok(Type::String)
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr(r#"o["missing"]"#)));
    assert_has_err(&checker.infer_expression_type(&parse_expr("o[[1]]")));
    assert_eq!(
        checker.infer_expression_type(&parse_expr("o[str_key]")),
        Ok(Type::String)
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr("b[0]")));
    assert_eq!(
        checker.infer_expression_type(&parse_expr("d.0")),
        Ok(Type::Dynamic)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("t.0")),
        Ok(Type::String)
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr("t.99")));
    assert_eq!(
        checker.infer_expression_type(&parse_expr("l.0")),
        Ok(Type::String)
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr("n.0")));
    assert_eq!(
        checker.infer_expression_type(&parse_expr("d.*")),
        Ok(Type::Dynamic)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("l.*")),
        Ok(Type::List(Box::new(Type::String)))
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("s.*")),
        Ok(Type::List(Box::new(Type::String)))
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("t.*")),
        Ok(Type::List(Box::new(Type::String)))
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr("n.*")));
}

/// Tests function call type checking including stdlib lookups, variadic and fixed
/// signatures, dynamic type allowances, return type computation errors, and undefined functions.
#[test]
fn test_type_checker_function_calls_comprehensive() {
    use std::sync::Arc;
    let mut scope = ScopeSchema::new();
    let sig_var = FunctionSignature::with_static_return_type(
        vec![
            FunctionParamSpec::new("p1", Type::String).with_allow_dynamic_type(false),
            FunctionParamSpec::new("p2", Type::Number),
        ],
        Type::String,
    )
    .with_variadic(FunctionParamSpec::new(
        "rest",
        Type::Tuple(vec![Type::Number]),
    ));
    scope.functions.insert("my_var_fn".to_string(), sig_var);
    let sig_fixed = FunctionSignature::with_static_return_type(
        vec![
            FunctionParamSpec::new("f1", Type::Tuple(vec![Type::String])),
            FunctionParamSpec::new("f2", Type::Number).with_allow_dynamic_type(false),
        ],
        Type::Number,
    );
    scope.functions.insert("my_fixed_fn".to_string(), sig_fixed);
    let sig_err = FunctionSignature::new(
        vec![],
        Arc::new(|_| {
            Err(hashicorp_configuration_language_rs::error::HclError::Type(
                "failed return type calc".to_string(),
            ))
        }),
    );
    scope.functions.insert("my_err_fn".to_string(), sig_err);
    scope.variables.insert("dyn_val".to_string(), Type::Dynamic);
    let checker = TypeChecker::new().with_scope(scope);
    assert_has_err(&checker.infer_expression_type(&parse_expr(r#"my_var_fn("a")"#)));
    assert_has_err(&checker.infer_expression_type(&parse_expr("my_var_fn(dyn_val, 10)")));
    assert_has_err(&checker.infer_expression_type(&parse_expr(r#"my_var_fn("a", [1])"#)));
    assert_has_err(
        &checker.infer_expression_type(&parse_expr(r#"my_var_fn("a", 10, "not_tuple")"#)),
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr(r#"my_var_fn("a", 10, [1], [2])"#)),
        Ok(Type::String)
    );
    assert_has_err(&checker.infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"])"#)));
    assert_has_err(&checker.infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"], 10, 20)"#)));
    assert_has_err(&checker.infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"], dyn_val)"#)));
    assert_has_err(&checker.infer_expression_type(&parse_expr(r#"my_fixed_fn("not_tuple", 10)"#)));
    assert_eq!(
        checker.infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"], 10)"#)),
        Ok(Type::Number)
    );
    assert_err_contains(
        &checker.infer_expression_type(&parse_expr("my_err_fn()")),
        "failed return type calc",
    );
    let loose_checker =
        TypeChecker::new().with_scope(ScopeSchema::new().with_allow_undefined_functions(true));
    assert_eq!(
        loose_checker.infer_expression_type(&parse_expr("unknown_fn(1, 2)")),
        Ok(Type::Dynamic)
    );
}

#[test]
fn test_type_check_missing_branch_coverage() {
    let mut attr_schemas = std::collections::HashMap::new();
    attr_schemas.insert(
        "any_attr".to_string(),
        AttributeSchema::new("any_attr", false),
    );
    let body_schema = BodySchema {
        attributes: attr_schemas,
        blocks: std::collections::HashMap::new(),
    };
    let mut parser = Parser::new(
        "any_attr = 123
",
    );
    let body = parser.parse_body();
    let checker = TypeChecker::new();
    assert_eq!(checker.check_body(&body, &body_schema), Ok(()));
    let mut scope = ScopeSchema::new();
    scope.variables.insert("dyn_v".to_string(), Type::Dynamic);
    scope
        .variables
        .insert("list_v".to_string(), Type::List(Box::new(Type::Number)));
    scope
        .variables
        .insert("tuple_v".to_string(), Type::Tuple(vec![Type::Number]));
    let sig_var_dyn = FunctionSignature::with_static_return_type(vec![], Type::String)
        .with_variadic(FunctionParamSpec::new("rest", Type::Dynamic).with_allow_dynamic_type(true));
    scope
        .functions
        .insert("var_dyn_fn".to_string(), sig_var_dyn);
    let sig_var_str = FunctionSignature::with_static_return_type(vec![], Type::String)
        .with_variadic(FunctionParamSpec::new("rest", Type::String).with_allow_dynamic_type(true));
    scope
        .functions
        .insert("var_str_fn".to_string(), sig_var_str);
    let sig_fixed_dyn = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("p1", Type::Dynamic).with_allow_dynamic_type(true)],
        Type::Number,
    );
    scope
        .functions
        .insert("fixed_dyn_fn".to_string(), sig_fixed_dyn);
    let sig_fixed_str = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("p1", Type::String).with_allow_dynamic_type(true)],
        Type::Number,
    );
    scope
        .functions
        .insert("fixed_str_fn".to_string(), sig_fixed_str);
    let checker = TypeChecker::new().with_scope(scope);
    assert_eq!(
        checker.infer_expression_type(&parse_expr("dyn_v == 1")),
        Ok(Type::Bool)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("1 == dyn_v")),
        Ok(Type::Bool)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("1 == 2")),
        Ok(Type::Bool)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("list_v == tuple_v")),
        Ok(Type::Bool)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr(r#"var_dyn_fn(1, "a")"#)),
        Ok(Type::String)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("var_str_fn(dyn_v)")),
        Ok(Type::String)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("fixed_dyn_fn(1)")),
        Ok(Type::Number)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("fixed_str_fn(dyn_v)")),
        Ok(Type::Number)
    );
}

#[test]
fn test_type_checker_function_calls_strict_args() -> Result<(), Box<dyn std::error::Error>> {
    let sig_variadic = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("fixed", Type::Number).with_allow_dynamic_type(false)],
        Type::String,
    )
    .with_variadic(FunctionParamSpec::new("var", Type::String).with_allow_dynamic_type(false));

    let sig_fixed = FunctionSignature::with_static_return_type(
        vec![FunctionParamSpec::new("fixed", Type::Number).with_allow_dynamic_type(false)],
        Type::String,
    );

    let scope = ScopeSchema::new()
        .with_function("strict_var_fn", sig_variadic)
        .with_function("strict_fixed_fn", sig_fixed)
        .with_variable("dyn_v", Type::Dynamic);

    let checker = TypeChecker::new().with_scope(scope);

    let expr_fixed = Parser::new(r"strict_fixed_fn([1])")
        .parse_expression()
        .ok_or("failed to parse expr_fixed")?;
    assert_has_err(&checker.infer_expression_type(&expr_fixed));

    let expr_var = Parser::new(r"strict_var_fn(1, [2])")
        .parse_expression()
        .ok_or("failed to parse expr_var")?;
    assert_has_err(&checker.infer_expression_type(&expr_var));

    // Test passing dynamic value to fixed strict arg
    let expr_strict_dyn = Parser::new(r"strict_fixed_fn(dyn_v)")
        .parse_expression()
        .ok_or("failed to parse expr_strict_dyn")?;
    assert_has_err(&checker.infer_expression_type(&expr_strict_dyn));

    // Test passing dynamic value to variadic strict arg
    let expr_strict_var_dyn = Parser::new(r"strict_var_fn(1, dyn_v)")
        .parse_expression()
        .ok_or("failed to parse expr_strict_var_dyn")?;
    assert_has_err(&checker.infer_expression_type(&expr_strict_var_dyn));
    Ok(())
}

#[test]
fn test_type_checker_stdlib_function_call() {
    let checker = TypeChecker::new();
    assert_eq!(
        checker.infer_expression_type(&parse_expr(r"range(1, 10)")),
        Ok(Type::List(Box::new(Type::Number)))
    );
}
