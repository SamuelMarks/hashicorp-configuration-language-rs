//! Evaluator for Type Expressions.
//!
//! Provides the ability to convert an AST `TypeExpr` into a concrete `Type`.
use crate::ast::type_expr::{CollectionType, TypeExpr};
use crate::types::ty::Type;
use crate::types::val::{Value, ValueData};
use std::collections::{BTreeMap, BTreeSet};
/// Evaluates a `TypeExpr` AST node into a concrete HCL `Type`.
#[must_use]
pub fn eval_type_expr(expr: &TypeExpr) -> Type {
    match expr {
        TypeExpr::Primitive(ty, _) => ty.clone(),
        TypeExpr::Collection(coll_type, inner_expr, _) => {
            let inner_ty = eval_type_expr(inner_expr);
            match coll_type {
                CollectionType::List => Type::List(Box::new(inner_ty)),
                CollectionType::Set => Type::Set(Box::new(inner_ty)),
                CollectionType::Map => Type::Map(Box::new(inner_ty)),
            }
        }
        TypeExpr::Object(attrs, _) => {
            let mut type_map = BTreeMap::new();
            let mut optional_set = BTreeSet::new();
            for (name, attr_spec) in attrs {
                type_map.insert(name.clone(), eval_type_expr(&attr_spec.ty));
                if attr_spec.optional {
                    optional_set.insert(name.clone());
                }
            }
            Type::Object {
                attrs: type_map,
                optional_attrs: optional_set,
            }
        }
        TypeExpr::Tuple(elems, _) => {
            let type_vec = elems.iter().map(eval_type_expr).collect();
            Type::Tuple(type_vec)
        }
    }
}
/// Coerces an object value against an object type expression schema, substituting defaults for optional attributes.
///
/// # Arguments
/// * `val` - The input object value to coerce.
/// * `schema` - The object type expression schema.
/// * `ctx` - The evaluation context for evaluating default expressions.
///
/// # Errors
/// Returns an error message if `val` is not an object, if `schema` is not an object type expression,
/// or if a required attribute is missing.
pub fn coerce_object_with_schema(
    val: &Value,
    schema: &TypeExpr,
    ctx: &crate::eval::context::Context,
) -> Result<Value, String> {
    let TypeExpr::Object(schema_attrs, _) = schema else {
        return Err("Schema is not an object type expression".to_string());
    };
    let ValueData::Object(map) = &*val.data else {
        return Err("Expected object value".to_string());
    };
    let mut new_map = BTreeMap::new();
    for (attr_name, attr_spec) in schema_attrs {
        if let Some(existing) = map.get(attr_name) {
            if let (true, Some(def_expr)) = (
                existing.is_null() && attr_spec.optional,
                &attr_spec.default_val,
            ) {
                let mut evaluator = crate::eval::evaluator::Evaluator::new(ctx);
                let evaluated = evaluator.eval_expr(def_expr);
                new_map.insert(attr_name.clone(), evaluated);
            } else if let TypeExpr::Object(..) = &attr_spec.ty {
                let nested = coerce_object_with_schema(existing, &attr_spec.ty, ctx)?;
                new_map.insert(attr_name.clone(), nested);
            } else {
                let target_ty = eval_type_expr(&attr_spec.ty);
                let coerced = existing.coerce(&target_ty)?;
                new_map.insert(attr_name.clone(), coerced);
            }
        } else if attr_spec.optional {
            if let Some(ref def_expr) = attr_spec.default_val {
                let mut evaluator = crate::eval::evaluator::Evaluator::new(ctx);
                let evaluated = evaluator.eval_expr(def_expr);
                new_map.insert(attr_name.clone(), evaluated);
            } else {
                let target_ty = eval_type_expr(&attr_spec.ty);
                new_map.insert(attr_name.clone(), Value::null(target_ty));
            }
        } else {
            return Err(format!("Missing required attribute: {attr_name}"));
        }
    }
    let out_ty = eval_type_expr(schema);
    Ok(Value::new(out_ty, ValueData::Object(new_map)))
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
    use crate::span::Span;
    use std::collections::HashMap;
    fn empty_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }
    #[test]
    fn test_eval_type_expr_primitive() {
        assert_eq!(
            eval_type_expr(&TypeExpr::Primitive(Type::String, empty_span())),
            Type::String
        );
        assert_eq!(
            eval_type_expr(&TypeExpr::Primitive(Type::Number, empty_span())),
            Type::Number
        );
        assert_eq!(
            eval_type_expr(&TypeExpr::Primitive(Type::Dynamic, empty_span())),
            Type::Dynamic
        );
    }
    #[test]
    fn test_eval_type_expr_collection() {
        let prim = TypeExpr::Primitive(Type::String, empty_span());
        assert_eq!(
            eval_type_expr(&TypeExpr::Collection(
                CollectionType::List,
                Box::new(prim.clone()),
                empty_span()
            )),
            Type::List(Box::new(Type::String))
        );
        assert_eq!(
            eval_type_expr(&TypeExpr::Collection(
                CollectionType::Set,
                Box::new(prim.clone()),
                empty_span()
            )),
            Type::Set(Box::new(Type::String))
        );
        assert_eq!(
            eval_type_expr(&TypeExpr::Collection(
                CollectionType::Map,
                Box::new(prim),
                empty_span()
            )),
            Type::Map(Box::new(Type::String))
        );
    }
    #[test]
    fn test_eval_type_expr_object() {
        use crate::ast::type_expr::ObjectAttrType;
        let mut attrs = HashMap::new();
        attrs.insert(
            "foo".to_string(),
            ObjectAttrType::required(TypeExpr::Primitive(Type::String, empty_span())),
        );
        attrs.insert(
            "bar".to_string(),
            ObjectAttrType::optional(TypeExpr::Primitive(Type::Number, empty_span()), None),
        );
        let obj_expr = TypeExpr::Object(attrs, empty_span());
        let ty = eval_type_expr(&obj_expr);
        let mut expected = std::collections::BTreeMap::new();
        expected.insert("foo".to_string(), Type::String);
        expected.insert("bar".to_string(), Type::Number);
        let mut expected_opts = BTreeSet::new();
        expected_opts.insert("bar".to_string());
        assert_eq!(ty, Type::object_with_optional(expected, expected_opts));
    }
    #[test]
    fn test_coerce_object_with_schema() {
        use crate::ast::expr::Expression;
        use crate::ast::type_expr::ObjectAttrType;
        use crate::eval::context::Context;
        let ctx = Context::new();
        let mut attrs = HashMap::new();
        attrs.insert(
            "name".to_string(),
            ObjectAttrType::required(TypeExpr::Primitive(Type::String, empty_span())),
        );
        attrs.insert(
            "port".to_string(),
            ObjectAttrType::optional(
                TypeExpr::Primitive(Type::Number, empty_span()),
                Some(Expression::Number(8080_i32.into(), empty_span())),
            ),
        );
        attrs.insert(
            "desc".to_string(),
            ObjectAttrType::optional(TypeExpr::Primitive(Type::String, empty_span()), None),
        );
        let schema = TypeExpr::Object(attrs, empty_span());
        let mut in_map = BTreeMap::new();
        in_map.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("srv".into())),
        );
        let in_val = Value::new(Type::Dynamic, ValueData::Object(in_map));
        let coerced = coerce_object_with_schema(&in_val, &schema, &ctx).unwrap();
        let mut expected_map = BTreeMap::new();
        expected_map.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("srv".into())),
        );
        expected_map.insert(
            "port".to_string(),
            Value::new(Type::Number, ValueData::Number(8080_i32.into())),
        );
        expected_map.insert("desc".to_string(), Value::null(Type::String));
        let expected_val = Value::new(eval_type_expr(&schema), ValueData::Object(expected_map));
        assert_eq!(coerced, expected_val);
        let mut in_map2 = BTreeMap::new();
        in_map2.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("srv2".into())),
        );
        in_map2.insert("port".to_string(), Value::null(Type::Number));
        let in_val2 = Value::new(Type::Dynamic, ValueData::Object(in_map2));
        let coerced2 = coerce_object_with_schema(&in_val2, &schema, &ctx).unwrap();
        let mut expected_map2 = BTreeMap::new();
        expected_map2.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("srv2".into())),
        );
        expected_map2.insert(
            "port".to_string(),
            Value::new(Type::Number, ValueData::Number(8080_i32.into())),
        );
        expected_map2.insert("desc".to_string(), Value::null(Type::String));
        let expected_val2 = Value::new(eval_type_expr(&schema), ValueData::Object(expected_map2));
        assert_eq!(coerced2, expected_val2);
        let empty_obj = Value::new(Type::Dynamic, ValueData::Object(BTreeMap::new()));
        assert!(coerce_object_with_schema(&empty_obj, &schema, &ctx).is_err());
        let mut inner_attrs = HashMap::new();
        inner_attrs.insert(
            "a".to_string(),
            ObjectAttrType::required(TypeExpr::Primitive(Type::String, empty_span())),
        );
        let mut outer_attrs = HashMap::new();
        outer_attrs.insert(
            "inner".to_string(),
            ObjectAttrType::required(TypeExpr::Object(inner_attrs, empty_span())),
        );
        let nested_schema = TypeExpr::Object(outer_attrs, empty_span());
        let mut inner_val_map = BTreeMap::new();
        inner_val_map.insert(
            "a".to_string(),
            Value::new(Type::String, ValueData::String("ok".into())),
        );
        let mut outer_val_map = BTreeMap::new();
        outer_val_map.insert(
            "inner".to_string(),
            Value::new(Type::Dynamic, ValueData::Object(inner_val_map)),
        );
        let outer_val = Value::new(Type::Dynamic, ValueData::Object(outer_val_map));
        assert!(coerce_object_with_schema(&outer_val, &nested_schema, &ctx).is_ok());
        let mut bad_outer_val_map = BTreeMap::new();
        bad_outer_val_map.insert(
            "inner".to_string(),
            Value::new(Type::Dynamic, ValueData::Object(BTreeMap::new())),
        );
        let bad_outer_val = Value::new(Type::Dynamic, ValueData::Object(bad_outer_val_map));
        assert!(coerce_object_with_schema(&bad_outer_val, &nested_schema, &ctx).is_err());
        let mut bad_port_map = BTreeMap::new();
        bad_port_map.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("srv".into())),
        );
        bad_port_map.insert(
            "port".to_string(),
            Value::new(Type::Bool, ValueData::Bool(true)),
        );
        let bad_port_val = Value::new(Type::Dynamic, ValueData::Object(bad_port_map));
        assert!(coerce_object_with_schema(&bad_port_val, &schema, &ctx).is_err());
        let non_obj = Value::new(Type::String, ValueData::String("str".into()));
        assert!(coerce_object_with_schema(&non_obj, &schema, &ctx).is_err());
        assert!(
            coerce_object_with_schema(
                &in_val,
                &TypeExpr::Primitive(Type::String, empty_span()),
                &ctx
            )
            .is_err()
        );
    }
    #[test]
    fn test_eval_type_expr_tuple() {
        let elems = vec![
            TypeExpr::Primitive(Type::String, empty_span()),
            TypeExpr::Primitive(Type::Number, empty_span()),
        ];
        let tuple_expr = TypeExpr::Tuple(elems, empty_span());
        let ty = eval_type_expr(&tuple_expr);
        assert_eq!(ty, Type::Tuple(vec![Type::String, Type::Number]));
    }
}
