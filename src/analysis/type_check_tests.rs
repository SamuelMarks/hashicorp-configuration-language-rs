//! Unit tests for the static AOT type checking engine.
use super::*;
use crate::ast::schema::{AttributeSchema, BlockHeaderSchema};
use crate::eval::func::FunctionParamSpec;
use crate::number::Number;
use crate::parse::parser::Parser;
/// Parses an HCL input string into a Body AST for testing.
fn parse_body(input: &str) -> Body {
    let mut parser = Parser::new(input);
    parser.parse_body()
}
/// Parses an expression string into an AST Expression for testing.
fn parse_expr(input: &str) -> Expression {
    let body = parse_body(&format!("val = {input}"));
    body.attributes["val"].expr.clone()
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
    let list_str = Type::List(Box::new(Type::String));
    assert!(is_type_compatible(&list_num, &list_num));
    assert!(is_type_compatible(&list_num, &set_num));
    assert!(is_type_compatible(&set_num, &list_num));
    assert!(is_type_compatible(&list_num, &list_str));
    let tup_nums = Type::Tuple(vec![Type::Number, Type::Number]);
    assert!(is_type_compatible(&tup_nums, &list_num));
    assert!(is_type_compatible(&tup_nums, &set_num));
    let tup_mixed = Type::Tuple(vec![Type::Number, Type::String]);
    assert!(is_type_compatible(&tup_mixed, &tup_mixed));
    assert!(!is_type_compatible(
        &tup_nums,
        &Type::Tuple(vec![Type::Number])
    ));
    let map_num = Type::Map(Box::new(Type::Number));
    let map_str = Type::Map(Box::new(Type::String));
    assert!(is_type_compatible(&map_num, &map_num));
    assert!(is_type_compatible(&map_num, &map_str));
    let mut obj_attrs = BTreeMap::new();
    obj_attrs.insert("a".to_string(), Type::Number);
    obj_attrs.insert("b".to_string(), Type::Number);
    let obj_num = Type::Object {
        attrs: obj_attrs.clone(),
        optional_attrs: BTreeSet::new(),
    };
    assert!(is_type_compatible(&obj_num, &map_num));
    let mut target_attrs = BTreeMap::new();
    target_attrs.insert("a".to_string(), Type::Number);
    let mut target_opts = BTreeSet::new();
    target_opts.insert("opt".to_string());
    target_attrs.insert("opt".to_string(), Type::String);
    let target_obj = Type::Object {
        attrs: target_attrs,
        optional_attrs: target_opts,
    };
    assert!(is_type_compatible(&obj_num, &target_obj));
    assert!(!is_type_compatible(&Type::Bool, &list_num));
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
    let scope = ScopeSchema::new()
        .with_variable("x", Type::Number)
        .with_variables(vec![("y".to_string(), Type::Bool)])
        .with_function("my_fn", sig)
        .with_allow_undefined_variables(true)
        .with_allow_undefined_functions(true);
    assert_eq!(scope.get_variable("x"), Some(&Type::Number));
    assert_eq!(scope.get_variable("y"), Some(&Type::Bool));
    assert!(scope.get_function("my_fn").is_some());
    assert!(scope.allow_undefined_variables);
    assert!(scope.allow_undefined_functions);
    let child = scope.child_scope();
    assert_eq!(child.get_variable("x"), Some(&Type::Number));
}
#[test]
fn test_type_checker_primitives_and_collections() -> Result<(), HclError> {
    let checker = TypeChecker::new();
    let body_null = parse_body("val = null");
    let ty = checker.infer_expression_type(&body_null.attributes["val"].expr)?;
    assert_eq!(ty, Type::Dynamic);
    let body_bool = parse_body("val = true");
    let ty = checker.infer_expression_type(&body_bool.attributes["val"].expr)?;
    assert_eq!(ty, Type::Bool);
    let body_num = parse_body("val = 42");
    let ty = checker.infer_expression_type(&body_num.attributes["val"].expr)?;
    assert_eq!(ty, Type::Number);
    let body_str = parse_body(r#"val = "hello""#);
    let ty = checker.infer_expression_type(&body_str.attributes["val"].expr)?;
    assert_eq!(ty, Type::String);
    let body_tup = parse_body("val = [1, 2, 3]");
    let ty = checker.infer_expression_type(&body_tup.attributes["val"].expr)?;
    assert_eq!(
        ty,
        Type::Tuple(vec![Type::Number, Type::Number, Type::Number])
    );
    let body_obj = parse_body(r#"val = { a = 1, b = "two" }"#);
    let ty = checker.infer_expression_type(&body_obj.attributes["val"].expr)?;
    let mut expected_attrs = BTreeMap::new();
    expected_attrs.insert("a".to_string(), Type::Number);
    expected_attrs.insert("b".to_string(), Type::String);
    assert_eq!(
        ty,
        Type::Object {
            attrs: expected_attrs,
            optional_attrs: BTreeSet::new(),
        }
    );
    let body_paren = parse_body("val = (10)");
    let ty = checker.infer_expression_type(&body_paren.attributes["val"].expr)?;
    assert_eq!(ty, Type::Number);
    assert!(
        checker
            .check_expression(&body_num.attributes["val"].expr, &Type::Number)
            .is_ok()
    );
    assert!(
        checker
            .check_expression(&body_str.attributes["val"].expr, &Type::Bool)
            .is_ok()
    );
    Ok(())
}
#[test]
fn test_type_checker_binary_and_unary_ops() -> Result<(), HclError> {
    let checker = TypeChecker::new();
    let b1 = parse_body("val = 1 + 2 * 3 - 4 / 2 % 1");
    assert_eq!(
        checker.infer_expression_type(&b1.attributes["val"].expr)?,
        Type::Number
    );
    let b2 = parse_body("val = 1 < 2");
    assert_eq!(
        checker.infer_expression_type(&b2.attributes["val"].expr)?,
        Type::Bool
    );
    let b3 = parse_body("val = true && false || true");
    assert_eq!(
        checker.infer_expression_type(&b3.attributes["val"].expr)?,
        Type::Bool
    );
    let b4 = parse_body("val = 1 == 2");
    assert_eq!(
        checker.infer_expression_type(&b4.attributes["val"].expr)?,
        Type::Bool
    );
    let b5 = parse_body("val = !true");
    assert_eq!(
        checker.infer_expression_type(&b5.attributes["val"].expr)?,
        Type::Bool
    );
    let b6 = parse_body("val = -42");
    assert_eq!(
        checker.infer_expression_type(&b6.attributes["val"].expr)?,
        Type::Number
    );
    Ok(())
}
#[test]
fn test_type_checker_conditionals() -> Result<(), HclError> {
    let checker = TypeChecker::new();
    let b1 = parse_body("val = true ? 1 : 2");
    assert_eq!(
        checker.infer_expression_type(&b1.attributes["val"].expr)?,
        Type::Number
    );
    let b2 = parse_body(r#"val = false ? "yes" : "no""#);
    assert_eq!(
        checker.infer_expression_type(&b2.attributes["val"].expr)?,
        Type::String
    );
    Ok(())
}
#[test]
fn test_type_checker_for_expressions() -> Result<(), HclError> {
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
        checker.infer_expression_type(&for_tuple)?,
        Type::List(Box::new(Type::Number))
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
        checker.infer_expression_type(&for_obj)?,
        Type::Map(Box::new(Type::String))
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
        checker.infer_expression_type(&for_group)?,
        Type::Map(Box::new(Type::List(Box::new(Type::String))))
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
        checker.infer_expression_type(&for_cond)?,
        Type::List(Box::new(Type::Number))
    );
    Ok(())
}
#[test]
fn test_type_checker_traversals() -> Result<(), HclError> {
    let mut user_attrs = BTreeMap::new();
    user_attrs.insert("name".to_string(), Type::String);
    user_attrs.insert("age".to_string(), Type::Number);
    let scope = ScopeSchema::new()
        .with_variable(
            "user",
            Type::Object {
                attrs: user_attrs,
                optional_attrs: BTreeSet::new(),
            },
        )
        .with_variable("scores", Type::List(Box::new(Type::Number)))
        .with_variable("pair", Type::Tuple(vec![Type::String, Type::Bool]))
        .with_variable("config", Type::Map(Box::new(Type::String)));
    let checker = TypeChecker::new().with_scope(scope);
    let b_var = parse_body("val = user");
    assert!(
        checker
            .infer_expression_type(&b_var.attributes["val"].expr)
            .is_ok()
    );
    let b_name = parse_body("val = user.name");
    assert_eq!(
        checker.infer_expression_type(&b_name.attributes["val"].expr)?,
        Type::String
    );
    let b_bad_attr = parse_body("val = user.nonexistent");
    assert!(
        checker
            .infer_expression_type(&b_bad_attr.attributes["val"].expr)
            .is_err()
    );
    let b_list_idx = parse_body("val = scores[0]");
    assert_eq!(
        checker.infer_expression_type(&b_list_idx.attributes["val"].expr)?,
        Type::Number
    );
    let b_tup_0 = parse_body("val = pair[0]");
    assert_eq!(
        checker.infer_expression_type(&b_tup_0.attributes["val"].expr)?,
        Type::String
    );
    let b_tup_1 = parse_body("val = pair.1");
    assert_eq!(
        checker.infer_expression_type(&b_tup_1.attributes["val"].expr)?,
        Type::Bool
    );
    let b_tup_oob = parse_body("val = pair[10]");
    assert!(
        checker
            .infer_expression_type(&b_tup_oob.attributes["val"].expr)
            .is_err()
    );
    let b_map_idx = parse_body(r#"val = config["env"]"#);
    assert_eq!(
        checker.infer_expression_type(&b_map_idx.attributes["val"].expr)?,
        Type::String
    );
    let b_splat = parse_body("val = scores.*");
    assert_eq!(
        checker.infer_expression_type(&b_splat.attributes["val"].expr)?,
        Type::List(Box::new(Type::Number))
    );
    let b_undef = parse_body("val = missing_var");
    assert!(
        checker
            .infer_expression_type(&b_undef.attributes["val"].expr)
            .is_err()
    );
    let perm_checker =
        TypeChecker::new().with_scope(ScopeSchema::new().with_allow_undefined_variables(true));
    assert_eq!(
        perm_checker.infer_expression_type(&b_undef.attributes["val"].expr)?,
        Type::Dynamic
    );
    Ok(())
}
#[test]
fn test_type_checker_function_calls() -> Result<(), HclError> {
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
        checker.infer_expression_type(&b1.attributes["val"].expr)?,
        Type::Number
    );
    let b_few = parse_body("val = add(10)");
    assert!(
        checker
            .infer_expression_type(&b_few.attributes["val"].expr)
            .is_err()
    );
    let b_many = parse_body("val = add(10, 20, 30)");
    assert!(
        checker
            .infer_expression_type(&b_many.attributes["val"].expr)
            .is_err()
    );
    let b_mismatch = parse_body(r#"val = add("not_num", [1, 2])"#);
    assert!(
        checker
            .infer_expression_type(&b_mismatch.attributes["val"].expr)
            .is_err()
    );
    let b_var1 = parse_body(r#"val = concat_all("a")"#);
    assert_eq!(
        checker.infer_expression_type(&b_var1.attributes["val"].expr)?,
        Type::String
    );
    let b_var2 = parse_body(r#"val = concat_all("a", "b", "c")"#);
    assert_eq!(
        checker.infer_expression_type(&b_var2.attributes["val"].expr)?,
        Type::String
    );
    let b_range = parse_body("val = range(5)");
    assert_eq!(
        checker.infer_expression_type(&b_range.attributes["val"].expr)?,
        Type::List(Box::new(Type::Number))
    );
    let b_undef_fn = parse_body("val = nonexistent_function(1)");
    assert!(
        checker
            .infer_expression_type(&b_undef_fn.attributes["val"].expr)
            .is_err()
    );
    Ok(())
}
#[test]
fn test_type_checker_body_validation_attributes() -> Result<(), HclError> {
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
    assert!(checker.check_body(&b_valid, &schema).is_ok());
    let b_missing = parse_body(
        r"
        age = 30
    ",
    );
    let Err(err_missing) = checker.check_body(&b_missing, &schema) else {
        return Err(HclError::Validation("expected error".to_string()));
    };
    assert!(err_missing.to_string().contains("is required"));
    let b_typo = parse_body(
        r#"
        name = "production"
        nane = "typo"
    "#,
    );
    let Err(err_typo) = checker.check_body(&b_typo, &schema) else {
        return Err(HclError::Validation("expected error".to_string()));
    };
    assert!(err_typo.to_string().contains("not expected here"));
    let b_bad_type = parse_body(
        r"
        name = [1, 2, 3]
    ",
    );
    let Err(err_bad_type) = checker.check_body(&b_bad_type, &schema) else {
        return Err(HclError::Validation("expected error".to_string()));
    };
    assert!(
        err_bad_type
            .to_string()
            .contains("Invalid value for attribute")
    );
    Ok(())
}

#[test]
fn test_type_checker_body_validation_blocks() -> Result<(), HclError> {
    let schema = BodySchema::new()
        .with_attribute(AttributeSchema::required("name").with_type(Type::String))
        .with_block(
            BlockHeaderSchema::new("server", vec!["name".to_string()]).with_body_schema(
                BodySchema::new()
                    .with_attribute(AttributeSchema::required("ip").with_type(Type::String)),
            ),
        );
    let checker = TypeChecker::new();
    let b_labels = parse_body(
        r#"
        name = "test"
        server "web" "extra_label" {
            ip = "10.0.0.1"
        }
    "#,
    );
    let Err(err_labels) = checker.check_body(&b_labels, &schema) else {
        return Err(HclError::Validation("expected error".to_string()));
    };
    assert!(err_labels.to_string().contains("Wrong number of labels"));
    let b_bad_block = parse_body(
        r#"
        name = "test"
        servr "web" {
            ip = "10.0.0.1"
        }
    "#,
    );
    let Err(err_bad_block) = checker.check_body(&b_bad_block, &schema) else {
        return Err(HclError::Validation("expected error".to_string()));
    };
    assert!(err_bad_block.to_string().contains("Blocks of type"));
    assert_eq!(
        err_bad_block.errors()[0].detail.as_deref(),
        Some("Did you mean \"server\"?")
    );
    let b_unrelated = parse_body(
        r#"
        name = "test"
        totally_unrelated_xyz "web" {}
    "#,
    );
    assert!(checker.check_body(&b_unrelated, &schema).is_err());
    let schema_bare = BodySchema::new().with_block(BlockHeaderSchema::new("bare_block", vec![]));
    let b_bare = parse_body("bare_block {}");
    assert!(checker.check_body(&b_bare, &schema_bare).is_ok());
    let b_inner_err = parse_body(
        r#"
        name = "test"
        server "web" {
            wrong_attr = "oops"
        }
    "#,
    );
    let Err(err_inner) = checker.check_body(&b_inner_err, &schema) else {
        return Err(HclError::Validation("expected error".to_string()));
    };
    assert!(err_inner.to_string().contains("is not expected here"));
    Ok(())
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
    assert!(checker.check_body(&b_dyn, &schema).is_ok());
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
    assert!(checker.check_body(&b_dyn_err, &schema).is_err());
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
    assert!(checker.check_body(&b_dyn_lbls, &schema).is_err());
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
    assert!(checker.check_body(&b_dyn_unexp, &schema).is_err());
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
    assert!(checker.check_expression(&expr_num, &Type::Number).is_ok());
    assert!(
        checker
            .check_expression(&expr_num, &Type::List(Box::new(Type::Bool)))
            .is_err()
    );
    let trav_ok = Traversal {
        expr: Box::new(Expression::Variable(
            "x".to_string(),
            Span::new(0, 0, 0, 0, 0, 0),
        )),
        operators: vec![],
    };
    assert!(checker.check_traversal(&trav_ok).is_ok());
    let trav_err = Traversal {
        expr: Box::new(Expression::Variable(
            "missing".to_string(),
            Span::new(0, 0, 0, 0, 0, 0),
        )),
        operators: vec![],
    };
    assert!(checker.check_traversal(&trav_err).is_err());
    let fc_custom = FuncCall {
        name: "my_fn".into(),
        args: vec![Expression::String(
            "HELLO".to_string(),
            Span::new(0, 0, 0, 0, 0, 0),
        )],
        expand_final: false,
    };
    assert!(
        checker
            .check_func_call(&fc_custom, &Span::new(0, 0, 0, 0, 0, 0))
            .is_ok()
    );
    let fc_range = FuncCall {
        name: "range".into(),
        args: vec![Expression::Number(5.into(), Span::new(0, 0, 0, 0, 0, 0))],
        expand_final: false,
    };
    assert!(
        checker
            .check_func_call(&fc_range, &Span::new(0, 0, 0, 0, 0, 0))
            .is_ok()
    );
    let fc_err = FuncCall {
        name: "nonexistent_fn".into(),
        args: vec![],
        expand_final: false,
    };
    assert!(
        checker
            .check_func_call(&fc_err, &Span::new(0, 0, 0, 0, 0, 0))
            .is_err()
    );
}
/// Tests dynamic blocks with various collection types, invalid labels, custom iterators,
/// and blocks without nested body schemas.
#[test]
fn test_type_checker_dynamic_blocks_comprehensive() -> Result<(), HclError> {
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
        assert!(checker.check_body(&b, &schema).is_ok());
    }
    let b_no_schema = parse_body(
        r#"
        dynamic "no_schema_block" {
            for_each = set_coll
            content {}
        }
    "#,
    );
    assert!(checker.check_body(&b_no_schema, &schema).is_ok());
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
    assert!(checker.check_body(&b_bad_lbl, &schema).is_err());
    let b_dyn_typo = parse_body(
        r#"
        dynamic "itim" {
            for_each = set_coll
            content {}
        }
    "#,
    );
    let Err(err_dyn_typo) = checker.check_body(&b_dyn_typo, &schema) else {
        return Err(HclError::Validation("expected error".to_string()));
    };
    assert_eq!(
        err_dyn_typo.errors()[0].detail.as_deref(),
        Some("Did you mean \"item\"?")
    );
    Ok(())
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
    assert!(
        checker
            .infer_expression_type(&b_if.attributes["val"].expr)
            .is_ok()
    );
    let b_if_err = parse_body(
        r#"
        val = "%{if [1]}yes%{endif}"
    "#,
    );
    assert!(
        checker
            .infer_expression_type(&b_if_err.attributes["val"].expr)
            .is_err()
    );
    let b_elif_err = parse_body(
        r#"
        val = "%{if bool_val}yes%{elif [1]}no%{endif}"
    "#,
    );
    assert!(
        checker
            .infer_expression_type(&b_elif_err.attributes["val"].expr)
            .is_err()
    );
    for coll in ["set_coll", "map_coll", "tup_coll", "obj_coll", "dyn_coll"] {
        let body_str = format!(r#"val = "%{{for k, v in {coll}}}${{k}}=${{v}} %{{endfor}}""#);
        let b_for = parse_body(&body_str);
        assert!(
            checker
                .infer_expression_type(&b_for.attributes["val"].expr)
                .is_ok()
        );
    }
    let b_for_bad = parse_body(
        r#"
        val = "%{for x in 123}${x}%{endfor}"
    "#,
    );
    assert!(
        checker
            .infer_expression_type(&b_for_bad.attributes["val"].expr)
            .is_err()
    );
    let mut diags = Diagnostics::new();
    let span = Span::default();
    let strip_part = crate::ast::expr::TemplatePart::Directive(
        crate::ast::expr::Directive::Strip {
            strip_left: true,
            strip_right: true,
        },
        span,
    );
    checker.check_template_part(&strip_part, checker.scope(), &mut diags);
    assert!(!diags.has_errors());
}
/// Tests static type inference for object keys, undefined variables, conditionals,
/// binary operations, and unary operations.
#[test]
fn test_type_checker_expressions_operators() {
    let mut scope = ScopeSchema::new();
    scope.variables.insert("x".to_string(), Type::Number);
    scope.variables.insert("b".to_string(), Type::Bool);
    scope.variables.insert("s".to_string(), Type::String);
    let checker = TypeChecker::new().with_scope(scope);
    let b_obj_key_bad = parse_body("val = { [1] = 2 }");
    assert!(
        checker
            .infer_expression_type(&b_obj_key_bad.attributes["val"].expr)
            .is_err()
    );
    let b_obj_key_good = parse_body("val = { (123) = 2 }");
    assert!(
        checker
            .infer_expression_type(&b_obj_key_good.attributes["val"].expr)
            .is_ok()
    );
    let loose_checker =
        TypeChecker::new().with_scope(ScopeSchema::new().with_allow_undefined_variables(true));
    let b_undef = parse_body("val = undef_var");
    let ty_undef = loose_checker.infer_expression_type(&b_undef.attributes["val"].expr);
    assert_eq!(ty_undef.ok(), Some(Type::Dynamic));
    let b_cond_bad_cond = parse_body("val = [1] ? 1 : 2");
    assert!(
        checker
            .infer_expression_type(&b_cond_bad_cond.attributes["val"].expr)
            .is_err()
    );
    let b_cond_incompat = parse_body("val = b ? 1 : [1]");
    assert!(
        checker
            .infer_expression_type(&b_cond_incompat.attributes["val"].expr)
            .is_err()
    );
    for invalid_bin in [
        "[1] + 1",
        "1 + [1]",
        "[1] - 1",
        "[1] * 1",
        "[1] / 1",
        "[1] % 1",
        "[1] < 1",
        "1 < [1]",
        "[1] <= 1",
        "[1] > 1",
        "[1] >= 1",
        "[1] && true",
        "true && [1]",
        "[1] || false",
        "false || [1]",
    ] {
        assert!(
            checker
                .infer_expression_type(&parse_expr(invalid_bin))
                .is_err()
        );
    }
    let res_eq_warn = checker.infer_expression_type(&parse_expr("true == [1]"));
    assert_eq!(res_eq_warn.ok(), Some(Type::Bool));
    assert!(checker.infer_expression_type(&parse_expr("![1]")).is_err());
    assert!(checker.infer_expression_type(&parse_expr("-[1]")).is_err());
}

/// Tests static type inference for `for` expressions and collection types.
#[test]
fn test_type_checker_expressions_for_loops() {
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
    assert!(checker.infer_expression_type(&bad_for_coll).is_err());
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
    assert!(checker.infer_expression_type(&bad_for_cond).is_err());
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
    assert!(checker.infer_expression_type(&bad_for_key).is_err());
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
    let res_group = checker.infer_expression_type(&for_group);
    assert_eq!(
        res_group.ok(),
        Some(Type::Map(Box::new(Type::List(Box::new(Type::String)))))
    );
}

/// Tests static type inference for `for` expressions over various collection types.
#[test]
fn test_type_checker_expressions_for_loops_collections() {
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
    for var in ["s", "m", "t", "o", "d"] {
        let for_expr_direct = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(Expression::Variable(var.to_string(), Span::default())),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), Span::default())),
                cond_expr: Some(Box::new(Expression::Bool(true, Span::default()))),
                grouping: false,
            }),
            Span::default(),
        );
        assert!(coll_checker.infer_expression_type(&for_expr_direct).is_ok());
    }
}

/// Tests static type checking for `GetAttr` and `Index` traversal operators.
#[test]
fn test_type_checker_traversals_attr_and_index() {
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
        checker.infer_expression_type(&parse_expr("d.foo")).ok(),
        Some(Type::Dynamic)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("m.any_key")).ok(),
        Some(Type::Number)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("o.name")).ok(),
        Some(Type::String)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("d[0]")).ok(),
        Some(Type::Dynamic)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("l[0]")).ok(),
        Some(Type::String)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("s[0]")).ok(),
        Some(Type::String)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("t[0]")).ok(),
        Some(Type::String)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("t[1]")).ok(),
        Some(Type::Number)
    );
    assert!(checker.infer_expression_type(&parse_expr("t[n]")).is_ok());
    let float_trav = Traversal {
        expr: Box::new(Expression::Variable("t".to_string(), Span::default())),
        operators: vec![TraversalOperator::Index(
            Expression::Number(
                crate::number::Number(bigdecimal::BigDecimal::from(-1)),
                Span::default(),
            ),
            Span::default(),
        )],
    };
    assert!(checker.check_traversal(&float_trav).is_ok());
    assert_eq!(
        checker.infer_expression_type(&parse_expr(r#"m["k"]"#)).ok(),
        Some(Type::Number)
    );
    assert_eq!(
        checker
            .infer_expression_type(&parse_expr(r#"o["name"]"#))
            .ok(),
        Some(Type::String)
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr("o[str_key]"))
            .is_ok()
    );
}

/// Tests static type checking for traversal error cases, `LegacyIndex`, and splat operators.
#[test]
fn test_type_checker_traversals_legacy_and_splat() {
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
    scope.variables.insert("n".to_string(), Type::Number);
    scope.variables.insert("b".to_string(), Type::Bool);
    let checker = TypeChecker::new().with_scope(scope);
    for invalid_trav in [
        "o.missing",
        "n.attr",
        "l[[1]]",
        "t[99]",
        "t[[1]]",
        "m[[1]]",
        r#"o["missing"]"#,
        "o[[1]]",
        "b[0]",
        "t.99",
        "n.0",
        "n.*",
    ] {
        assert!(
            checker
                .infer_expression_type(&parse_expr(invalid_trav))
                .is_err()
        );
    }
    assert_eq!(
        checker.infer_expression_type(&parse_expr("d.0")).ok(),
        Some(Type::Dynamic)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("t.0")).ok(),
        Some(Type::String)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("l.0")).ok(),
        Some(Type::String)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("d.*")).ok(),
        Some(Type::Dynamic)
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("l.*")).ok(),
        Some(Type::List(Box::new(Type::String)))
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("s.*")).ok(),
        Some(Type::List(Box::new(Type::String)))
    );
    assert_eq!(
        checker.infer_expression_type(&parse_expr("t.*")).ok(),
        Some(Type::List(Box::new(Type::String)))
    );
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
        Arc::new(|_| Err(HclError::Type("failed return type calc".to_string()))),
    );
    scope.functions.insert("my_err_fn".to_string(), sig_err);
    scope.variables.insert("dyn_val".to_string(), Type::Dynamic);
    let checker = TypeChecker::new().with_scope(scope);
    assert!(
        checker
            .infer_expression_type(&parse_expr(r#"my_var_fn("a")"#))
            .is_err()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr("my_var_fn(dyn_val, 10)"))
            .is_err()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr(r#"my_var_fn("a", [1])"#))
            .is_err()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr(r#"my_var_fn("a", 10, "not_tuple")"#))
            .is_err()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr(r#"my_var_fn("a", 10, [1], [2])"#))
            .is_ok()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"])"#))
            .is_err()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"], 10, 20)"#))
            .is_err()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"], dyn_val)"#))
            .is_err()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr(r#"my_fixed_fn("not_tuple", 10)"#))
            .is_err()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"], 10)"#))
            .is_ok()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr("my_err_fn()"))
            .is_err()
    );
    let loose_checker =
        TypeChecker::new().with_scope(ScopeSchema::new().with_allow_undefined_functions(true));
    assert_eq!(
        loose_checker
            .infer_expression_type(&parse_expr("unknown_fn(1, 2)"))
            .ok(),
        Some(Type::Dynamic)
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
    let mut parser = Parser::new("any_attr = 123\n");
    let body = parser.parse_body();
    let checker = TypeChecker::new();
    let diags = checker.check_body(&body, &body_schema);
    assert!(diags.is_ok());
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
    assert!(
        checker
            .infer_expression_type(&parse_expr("dyn_v == 1"))
            .is_ok()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr("1 == dyn_v"))
            .is_ok()
    );
    assert!(checker.infer_expression_type(&parse_expr("1 == 2")).is_ok());
    assert!(
        checker
            .infer_expression_type(&parse_expr("list_v == tuple_v"))
            .is_ok()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr(r#"var_dyn_fn(1, "a")"#))
            .is_ok()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr("var_str_fn(dyn_v)"))
            .is_ok()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr("fixed_dyn_fn(1)"))
            .is_ok()
    );
    assert!(
        checker
            .infer_expression_type(&parse_expr("fixed_str_fn(dyn_v)"))
            .is_ok()
    );
}

#[test]
fn test_type_checker_function_calls_strict_args() -> Result<(), HclError> {
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
        .with_function("strict_fixed_fn", sig_fixed);

    let checker = TypeChecker::new().with_scope(scope);

    let expr_fixed = crate::parse::parser::Parser::new(r"strict_fixed_fn([1])")
        .parse_expression()
        .ok_or_else(|| HclError::Parse("failed to parse expression".to_string()))?;
    let res_fixed = checker.infer_expression_type(&expr_fixed);
    assert!(res_fixed.is_err());

    let expr_var = crate::parse::parser::Parser::new(r"strict_var_fn(1, [2])")
        .parse_expression()
        .ok_or_else(|| HclError::Parse("failed to parse expression".to_string()))?;
    let res_var = checker.infer_expression_type(&expr_var);
    assert!(res_var.is_err());
    Ok(())
}
