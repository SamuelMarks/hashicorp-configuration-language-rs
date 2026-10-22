//! Type Unification rules.
//!
//! Unification finds the most general supertype of a set of types.
//! It is used for lists (where all elements must share a type), maps,
//! and binary operator coercion.
use crate::types::Type;
use std::collections::BTreeMap;
/// Finds the most general type that can represent both `a` and `b`.
/// Returns `None` if the types are entirely incompatible.
#[must_use]
pub fn unify(a: &Type, b: &Type) -> Option<Type> {
    if a == b {
        return Some(a.clone());
    }
    match (a, b) {
        (Type::Dynamic, other) | (other, Type::Dynamic) => Some(other.clone()),
        (Type::String, Type::Number | Type::Bool) | (Type::Number | Type::Bool, Type::String) => {
            Some(Type::String)
        }
        (Type::List(inner_a), Type::List(inner_b)) => {
            unify(inner_a, inner_b).map(|t| Type::List(Box::new(t)))
        }
        (Type::Set(inner_a), Type::Set(inner_b)) => {
            unify(inner_a, inner_b).map(|t| Type::Set(Box::new(t)))
        }
        (Type::Map(inner_a), Type::Map(inner_b)) => {
            unify(inner_a, inner_b).map(|t| Type::Map(Box::new(t)))
        }
        (Type::Tuple(els), Type::List(inner)) | (Type::List(inner), Type::Tuple(els)) => {
            let mut current = *inner.clone();
            for el in els {
                current = unify(&current, el)?;
            }
            Some(Type::List(Box::new(current)))
        }
        (Type::Object { attrs, .. }, Type::Map(inner))
        | (Type::Map(inner), Type::Object { attrs, .. }) => {
            let mut current = *inner.clone();
            for el in attrs.values() {
                current = unify(&current, el)?;
            }
            Some(Type::Map(Box::new(current)))
        }
        (Type::Tuple(a_els), Type::Tuple(b_els)) => {
            let mut current = None;
            for el in a_els.iter().chain(b_els.iter()) {
                current = match current {
                    Some(c) => Some(unify(&c, el)?),
                    None => Some(el.clone()),
                };
            }
            Some(Type::List(Box::new(current.unwrap_or(Type::Dynamic))))
        }
        (
            Type::Object {
                attrs: a_attrs,
                optional_attrs: a_opt,
            },
            Type::Object {
                attrs: b_attrs,
                optional_attrs: b_opt,
            },
        ) => {
            if (!a_opt.is_empty() || !b_opt.is_empty())
                && a_attrs.keys().collect::<std::collections::BTreeSet<_>>()
                    == b_attrs.keys().collect::<std::collections::BTreeSet<_>>()
            {
                let mut unified_attrs = BTreeMap::new();
                let mut unified_opts = std::collections::BTreeSet::new();
                for ((k, a_ty), b_ty) in a_attrs.iter().zip(b_attrs.values()) {
                    let u_ty = unify(a_ty, b_ty)?;
                    unified_attrs.insert(k.clone(), u_ty);
                    if a_opt.contains(k) && b_opt.contains(k) {
                        unified_opts.insert(k.clone());
                    }
                }
                return Some(Type::object_with_optional(unified_attrs, unified_opts));
            }
            let mut current = None;
            for el in a_attrs.values().chain(b_attrs.values()) {
                current = match current {
                    Some(c) => Some(unify(&c, el)?),
                    None => Some(el.clone()),
                };
            }
            Some(Type::Map(Box::new(current.unwrap_or(Type::Dynamic))))
        }
        _ => None,
    }
}
/// Unifies two optional refinements by computing their intersection.
///
/// # Arguments
/// * `a` - The first optional refinement.
/// * `b` - The second optional refinement.
///
/// # Errors
/// Returns [`crate::error::HclError`] if the two refinements have conflicting constraints.
pub fn unify_refinement(
    a: Option<&crate::types::refinement::Refinement>,
    b: Option<&crate::types::refinement::Refinement>,
) -> Result<Option<crate::types::refinement::Refinement>, crate::error::HclError> {
    match (a, b) {
        (Some(ra), Some(rb)) => ra.intersect(rb).map(Some),
        (Some(ra), None) => Ok(Some(ra.clone())),
        (None, Some(rb)) => Ok(Some(rb.clone())),
        (None, None) => Ok(None),
    }
}
/// Validates that an unknown value's refinement constraints are compatible with a target type.
///
/// # Arguments
/// * `target` - The target type.
/// * `refinement` - The refinement constraints to validate against the target type.
///
/// # Errors
/// Returns [`crate::error::HclError::Type`] if the refinement constraints conflict with the target type schema.
pub fn validate_unknown_refinement(
    target: &Type,
    refinement: &crate::types::refinement::Refinement,
) -> Result<(), crate::error::HclError> {
    if let Type::Object { attrs, .. } = target {
        for (k, attr_ref) in &refinement.object_attrs {
            if let Some(expected_ty) = attrs.get(k) {
                validate_unknown_refinement(expected_ty, attr_ref)?;
            } else if attr_ref.not_null {
                return Err(crate::error::HclError::Type(format!(
                    "attribute {k:?} is not allowed in object type {target}"
                )));
            }
        }
    }
    Ok(())
}
#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::error::HclError;
    use std::collections::BTreeMap;
    #[test]
    fn test_unify_primitives_and_collections() {
        assert_eq!(unify(&Type::String, &Type::String), Some(Type::String));
        assert_eq!(unify(&Type::Dynamic, &Type::Number), Some(Type::Number));
        assert_eq!(unify(&Type::Bool, &Type::Dynamic), Some(Type::Bool));
        assert_eq!(unify(&Type::String, &Type::Number), Some(Type::String));
        assert_eq!(unify(&Type::Number, &Type::String), Some(Type::String));
        assert_eq!(unify(&Type::String, &Type::Bool), Some(Type::String));
        assert_eq!(unify(&Type::Bool, &Type::String), Some(Type::String));
        assert_eq!(unify(&Type::Number, &Type::Bool), None);
        assert_eq!(
            unify(
                &Type::List(Box::new(Type::Number)),
                &Type::List(Box::new(Type::Bool))
            ),
            None
        );
        assert_eq!(
            unify(
                &Type::Set(Box::new(Type::Number)),
                &Type::Set(Box::new(Type::Bool))
            ),
            None
        );
        assert_eq!(
            unify(
                &Type::Map(Box::new(Type::Number)),
                &Type::Map(Box::new(Type::Bool))
            ),
            None
        );
        let list_str = Type::List(Box::new(Type::String));
        let list_num = Type::List(Box::new(Type::Number));
        assert_eq!(
            unify(&list_str, &list_num),
            Some(Type::List(Box::new(Type::String)))
        );
        let set_str = Type::Set(Box::new(Type::String));
        let set_num = Type::Set(Box::new(Type::Number));
        assert_eq!(
            unify(&set_str, &set_num),
            Some(Type::Set(Box::new(Type::String)))
        );
    }

    #[test]
    fn test_unify_tuples_and_objects() {
        let list_str = Type::List(Box::new(Type::String));
        let list_num = Type::List(Box::new(Type::Number));
        let map_str = Type::Map(Box::new(Type::String));
        let map_num = Type::Map(Box::new(Type::Number));
        assert_eq!(
            unify(&map_str, &map_num),
            Some(Type::Map(Box::new(Type::String)))
        );
        let tup = Type::Tuple(vec![Type::Number, Type::String]);
        assert_eq!(
            unify(&tup, &list_str),
            Some(Type::List(Box::new(Type::String)))
        );
        assert_eq!(
            unify(&list_str, &tup),
            Some(Type::List(Box::new(Type::String)))
        );
        let tup_single_bool = Type::Tuple(vec![Type::Bool]);
        assert_eq!(unify(&tup_single_bool, &list_num), None);
        assert_eq!(unify(&list_num, &tup_single_bool), None);
        let tup_mixed_bool = Type::Tuple(vec![Type::Number, Type::Bool]);
        assert_eq!(unify(&tup_mixed_bool, &list_num), None);
        assert_eq!(unify(&list_num, &tup_mixed_bool), None);
        let mut obj_attrs = BTreeMap::new();
        obj_attrs.insert("a".to_string(), Type::Number);
        let obj = Type::object(obj_attrs);
        assert_eq!(
            unify(&obj, &map_str),
            Some(Type::Map(Box::new(Type::String)))
        );
        assert_eq!(
            unify(&map_str, &obj),
            Some(Type::Map(Box::new(Type::String)))
        );
        let mut obj_bad_attrs = BTreeMap::new();
        obj_bad_attrs.insert("a".to_string(), Type::Bool);
        let obj_bad = Type::object(obj_bad_attrs);
        assert_eq!(unify(&obj_bad, &map_num), None);
        assert_eq!(unify(&map_num, &obj_bad), None);
        let tup2 = Type::Tuple(vec![Type::String]);
        assert_eq!(unify(&tup, &tup2), Some(Type::List(Box::new(Type::String))));
        let tup_empty = Type::Tuple(vec![]);
        assert_eq!(unify(&tup_empty, &tup_empty), Some(Type::Tuple(vec![])));
        assert_eq!(
            unify(&tup_empty, &Type::Tuple(vec![Type::String])),
            Some(Type::List(Box::new(Type::String)))
        );
        let mut second_obj_attrs = BTreeMap::new();
        second_obj_attrs.insert("b".to_string(), Type::String);
        let second_obj = Type::object(second_obj_attrs);
        assert_eq!(
            unify(&obj, &second_obj),
            Some(Type::Map(Box::new(Type::String)))
        );
        let obj_empty = Type::object(BTreeMap::new());
        assert_eq!(
            unify(&obj_empty, &obj_empty),
            Some(Type::object(BTreeMap::new()))
        );
        assert_eq!(
            unify(&obj_empty, &second_obj),
            Some(Type::Map(Box::new(Type::String)))
        );
        assert_eq!(
            unify(&tup, &tup_mixed_bool),
            Some(Type::List(Box::new(Type::String)))
        );
        assert_eq!(unify(&obj, &obj_bad), None);
    }

    #[test]
    fn test_unify_nested_and_optional() -> Result<(), HclError> {
        let tup_nested1 = Type::Tuple(vec![Type::List(Box::new(Type::String))]);
        let tup_nested2 = Type::Tuple(vec![Type::List(Box::new(Type::Number))]);
        assert_eq!(
            unify(&tup_nested1, &tup_nested2),
            Some(Type::List(Box::new(Type::List(Box::new(Type::String)))))
        );
        let tup_nested_bad = Type::Tuple(vec![Type::List(Box::new(Type::Bool))]);
        let tup_nested_num = Type::Tuple(vec![Type::List(Box::new(Type::Number))]);
        assert_eq!(unify(&tup_nested_bad, &tup_nested_num), None);
        let mut obj_nested1_attrs = BTreeMap::new();
        obj_nested1_attrs.insert("a".to_string(), Type::List(Box::new(Type::String)));
        let obj_nested1 = Type::object(obj_nested1_attrs);
        let mut obj_nested2_attrs = BTreeMap::new();
        obj_nested2_attrs.insert("a".to_string(), Type::List(Box::new(Type::Number)));
        let obj_nested2 = Type::object(obj_nested2_attrs);
        assert_eq!(
            unify(&obj_nested1, &obj_nested2),
            Some(Type::Map(Box::new(Type::List(Box::new(Type::String)))))
        );
        let mut obj_nested_bad_attrs = BTreeMap::new();
        obj_nested_bad_attrs.insert("a".to_string(), Type::List(Box::new(Type::Bool)));
        let obj_nested_bad = Type::object(obj_nested_bad_attrs);
        assert_eq!(unify(&obj_nested_bad, &obj_nested2), None);
        let mut first_opt_attrs = BTreeMap::new();
        first_opt_attrs.insert("name".to_string(), Type::String);
        first_opt_attrs.insert("port".to_string(), Type::Number);
        let mut first_opt_set = std::collections::BTreeSet::new();
        first_opt_set.insert("port".to_string());
        first_opt_set.insert("name".to_string());
        let obj_first_opt = Type::object_with_optional(first_opt_attrs, first_opt_set);
        let mut second_opt_attrs = BTreeMap::new();
        second_opt_attrs.insert("name".to_string(), Type::String);
        second_opt_attrs.insert("port".to_string(), Type::Number);
        let mut second_opt_set = std::collections::BTreeSet::new();
        second_opt_set.insert("port".to_string());
        let obj_second_opt = Type::object_with_optional(second_opt_attrs.clone(), second_opt_set);
        let unified_opt = unify(&obj_first_opt, &obj_second_opt)
            .ok_or_else(|| HclError::Validation("unify failed".to_string()))?;
        assert!(unified_opt.is_attr_optional("port"));
        assert!(!unified_opt.is_attr_optional("name"));
        let unified_opt_rev = unify(&obj_second_opt, &obj_first_opt)
            .ok_or_else(|| HclError::Validation("unify failed".to_string()))?;
        assert!(unified_opt_rev.is_attr_optional("port"));
        assert!(!unified_opt_rev.is_attr_optional("name"));
        let obj_no_opt = Type::object(second_opt_attrs);
        let unified_from_no_opt = unify(&obj_no_opt, &obj_second_opt)
            .ok_or_else(|| HclError::Validation("unify failed".to_string()))?;
        assert!(!unified_from_no_opt.is_attr_optional("port"));
        let mut diff_keys_attrs = BTreeMap::new();
        diff_keys_attrs.insert("name".to_string(), Type::String);
        diff_keys_attrs.insert("extra".to_string(), Type::String);
        let obj_diff_keys = Type::object(diff_keys_attrs);
        let unified_diff_keys = unify(&obj_first_opt, &obj_diff_keys);
        assert_eq!(unified_diff_keys, Some(Type::Map(Box::new(Type::String))));
        let mut bad_opt_attrs = BTreeMap::new();
        bad_opt_attrs.insert("name".to_string(), Type::String);
        bad_opt_attrs.insert("port".to_string(), Type::Bool);
        let obj_bad_opt =
            Type::object_with_optional(bad_opt_attrs, std::collections::BTreeSet::new());
        assert_eq!(unify(&obj_first_opt, &obj_bad_opt), None);
        let cap_first = Type::capsule::<u32>("custom_u32");
        let cap_same = Type::capsule::<u32>("custom_u32");
        let cap_second = Type::capsule::<u64>("custom_u64");
        assert_eq!(unify(&cap_first, &cap_same), Some(cap_first.clone()));
        assert_eq!(unify(&cap_first, &Type::Dynamic), Some(cap_first.clone()));
        assert_eq!(unify(&Type::Dynamic, &cap_first), Some(cap_first.clone()));
        assert_eq!(unify(&cap_first, &cap_second), None);
        assert_eq!(unify(&cap_first, &Type::String), None);
        Ok(())
    }
    #[test]
    fn test_unify_refinement_operations() -> Result<(), HclError> {
        use crate::types::refinement::Refinement;
        let r1 = Refinement::not_null().with_prefix("pre-");
        let r2 = Refinement::not_null().with_prefix("pre-prod-");
        let unified = unify_refinement(Some(&r1), Some(&r2))?;
        assert_eq!(
            unified.as_ref().and_then(|r| r.string_prefix.as_deref()),
            Some("pre-prod-")
        );
        let unified_single = unify_refinement(Some(&r1), None)?;
        assert_eq!(unified_single, Some(r1.clone()));
        let unified_reverse = unify_refinement(None, Some(&r2))?;
        assert_eq!(unified_reverse, Some(r2));
        let unified_none = unify_refinement(None, None)?;
        assert_eq!(unified_none, None);
        let r_bad = Refinement::new().with_prefix("post-");
        assert!(unify_refinement(Some(&r1), Some(&r_bad)).is_err());
        let mut obj_schema = BTreeMap::new();
        obj_schema.insert("name".to_string(), Type::String);
        let obj_ty = Type::object(obj_schema);
        let valid_ref = Refinement::new()
            .with_object_attr("name", Refinement::not_null().with_prefix("server-"));
        assert!(validate_unknown_refinement(&obj_ty, &valid_ref).is_ok());
        let invalid_ref =
            Refinement::new().with_object_attr("unknown_field", Refinement::not_null());
        assert!(validate_unknown_refinement(&obj_ty, &invalid_ref).is_err());
        let nullable_unknown_ref =
            Refinement::new().with_object_attr("unknown_optional", Refinement::new());
        assert!(validate_unknown_refinement(&obj_ty, &nullable_unknown_ref).is_ok());
        let mut nested_schema = BTreeMap::new();
        nested_schema.insert("sub".to_string(), Type::String);
        let mut parent_schema = BTreeMap::new();
        parent_schema.insert("child".to_string(), Type::object(nested_schema));
        let parent_obj_ty = Type::object(parent_schema);
        let nested_invalid_ref = Refinement::new().with_object_attr(
            "child",
            Refinement::new().with_object_attr("unknown_nested", Refinement::not_null()),
        );
        assert!(validate_unknown_refinement(&parent_obj_ty, &nested_invalid_ref).is_err());
        Ok(())
    }
}
