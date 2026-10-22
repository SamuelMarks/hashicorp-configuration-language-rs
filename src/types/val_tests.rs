//! Unit tests for HCL runtime values, marks, paths, and capsules.
#[test]
fn test_ephemeral_marks() {
    let val = Value::new(Type::String, ValueData::String("temp".into()));
    assert!(!val.is_ephemeral());
    let ephemeral_val = val.mark_ephemeral();
    assert!(ephemeral_val.is_ephemeral());
    assert!(ephemeral_val.has_mark(&ValueMark::Ephemeral));
    assert_eq!(ValueMark::Ephemeral.to_string(), "ephemeral");
}

#[test]
fn test_valued_data_clone_all_variants() {
    let v1 = ValueData::Null;
    assert!(matches!(Clone::clone(&v1), ValueData::Null));
    let v2 = ValueData::Unknown(None);
    assert!(matches!(Clone::clone(&v2), ValueData::Unknown(None)));
    let v3 = ValueData::Bool(true);
    assert!(matches!(Clone::clone(&v3), ValueData::Bool(true)));
    let v4 = ValueData::Number(Number::from(1));
    assert!(matches!(Clone::clone(&v4), ValueData::Number(_)));
    let v5 = ValueData::String("test".to_string());
    assert!(matches!(Clone::clone(&v5), ValueData::String(_)));
    let v6 = ValueData::Array(vec![]);
    assert!(matches!(Clone::clone(&v6), ValueData::Array(_)));
    let v7 = ValueData::Set(BTreeSet::new());
    assert!(matches!(Clone::clone(&v7), ValueData::Set(_)));
    let v8 = ValueData::Object(BTreeMap::new());
    assert!(matches!(Clone::clone(&v8), ValueData::Object(_)));
    let v9 = ValueData::Capsule(std::sync::Arc::new(1));
    assert!(matches!(Clone::clone(&v9), ValueData::Capsule(_)));
}
use super::*;
use bigdecimal::BigDecimal;
use std::str::FromStr;
#[test]
fn test_value_creation() {
    let v_unk = Value::unknown(Type::String);
    assert!(v_unk.is_unknown());
    assert!(!v_unk.is_null());
    assert_eq!(v_unk.ty(), &Type::String);
    let v_null = Value::null(Type::Number);
    assert!(!v_null.is_unknown());
    assert!(v_null.is_null());
    assert_eq!(v_null.ty(), &Type::Number);
    let v_bool = Value::new(Type::Bool, ValueData::Bool(true));
    assert_eq!(v_bool.ty(), &Type::Bool);
}
#[test]
fn test_value_eq() -> Result<(), HclError> {
    let v1 = Value::new(Type::Bool, ValueData::Bool(true));
    let v2 = Value::new(Type::Bool, ValueData::Bool(true));
    let v3 = Value::new(Type::Bool, ValueData::Bool(false));
    assert_eq!(v1, v2);
    assert_ne!(v1, v3);
    let v_unk1 = Value::unknown(Type::String);
    let v_unk2 = Value::unknown(Type::String);
    assert_eq!(v_unk1, v_unk2);
    let n1 = Value::new(
        Type::Number,
        ValueData::Number(Number::new(BigDecimal::from_str("1.0")?)),
    );
    let n2 = Value::new(
        Type::Number,
        ValueData::Number(Number::new(BigDecimal::from_str("1.0")?)),
    );
    let n3 = Value::new(
        Type::Number,
        ValueData::Number(Number::new(BigDecimal::from_str("2.0")?)),
    );
    assert_eq!(n1, n2);
    assert_ne!(n1, n3);
    let s1 = Value::new(Type::String, ValueData::String("a".to_string()));
    let s2 = Value::new(Type::String, ValueData::String("a".to_string()));
    let s3 = Value::new(Type::String, ValueData::String("b".to_string()));
    assert_eq!(s1, s2);
    assert_ne!(s1, s3);
    let arr1 = Value::new(
        Type::List(Box::new(Type::String)),
        ValueData::Array(vec![s1.clone()]),
    );
    let arr2 = Value::new(
        Type::List(Box::new(Type::String)),
        ValueData::Array(vec![s2]),
    );
    let arr3 = Value::new(Type::List(Box::new(Type::String)), ValueData::Array(vec![]));
    assert_eq!(arr1, arr2);
    assert_ne!(arr1, arr3);
    let mut set_data = BTreeSet::new();
    set_data.insert(s1.clone());
    let set1 = Value::new(
        Type::Set(Box::new(Type::String)),
        ValueData::Set(set_data.clone()),
    );
    let set2 = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_data));
    assert_eq!(set1, set2);
    let mut map_data = BTreeMap::new();
    map_data.insert("k".to_string(), s1);
    let map1 = Value::new(
        Type::Map(Box::new(Type::String)),
        ValueData::Object(map_data.clone()),
    );
    let map2 = Value::new(
        Type::Map(Box::new(Type::String)),
        ValueData::Object(map_data),
    );
    assert_eq!(map1, map2);
    assert_ne!(v1, v_unk1);
    assert_ne!(
        Value::new(Type::Bool, ValueData::Bool(true)),
        Value::new(Type::Bool, ValueData::Null)
    );
    assert_ne!(ValueData::Bool(true), ValueData::Null);
    let v_debug = format!("{v1:?}");
    assert!(v_debug.contains("Bool"));
    Ok(())
}
#[test]
fn test_value_hash() -> Result<(), HclError> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    let v1 = Value::new(Type::Bool, ValueData::Bool(true));
    v1.hash(&mut h1);
    v1.hash(&mut h2);
    assert_eq!(h1.finish(), h2.finish());
    let mut h3 = DefaultHasher::new();
    let v2 = Value::unknown(Type::String);
    v2.hash(&mut h3);
    assert_ne!(h1.finish(), h3.finish());
    let mut h4 = DefaultHasher::new();
    let v3 = Value::null(Type::String);
    v3.hash(&mut h4);
    let mut h5 = DefaultHasher::new();
    let v4 = Value::new(Type::String, ValueData::String("test".to_string()));
    v4.hash(&mut h5);
    let mut h6 = DefaultHasher::new();
    let v5 = Value::new(
        Type::Number,
        ValueData::Number(Number::new(BigDecimal::from_str("1.0")?)),
    );
    v5.hash(&mut h6);
    let mut h7 = DefaultHasher::new();
    let v6 = Value::new(Type::List(Box::new(Type::String)), ValueData::Array(vec![]));
    v6.hash(&mut h7);
    let mut h8 = DefaultHasher::new();
    let v7 = Value::new(
        Type::Set(Box::new(Type::String)),
        ValueData::Set(BTreeSet::new()),
    );
    v7.hash(&mut h8);
    let mut h9 = DefaultHasher::new();
    let v8 = Value::new(
        Type::Map(Box::new(Type::String)),
        ValueData::Object(BTreeMap::new()),
    );
    v8.hash(&mut h9);
    Ok(())
}
#[test]
fn test_value_ord() {
    let v1 = Value::new(Type::Bool, ValueData::Bool(true));
    let v2 = Value::new(Type::Bool, ValueData::Bool(false));
    assert_eq!(v1.partial_cmp(&v2), Some(std::cmp::Ordering::Equal));
    assert_eq!(v1.cmp(&v2), std::cmp::Ordering::Equal);
}
#[test]
fn test_value_coerce() -> Result<(), HclError> {
    let val_unk = Value::unknown(Type::String);
    let coerced_unk = val_unk.coerce(&Type::Number)?;
    assert!(coerced_unk.is_unknown());
    assert_eq!(coerced_unk.ty(), &Type::Number);
    let val_null = Value::null(Type::String);
    let coerced_null = val_null.coerce(&Type::Number)?;
    assert!(coerced_null.is_null());
    assert_eq!(coerced_null.ty(), &Type::Number);
    let val_str = Value::new(Type::String, ValueData::String("1".to_string()));
    let coerced_num = val_str.coerce(&Type::Number)?;
    assert_eq!(coerced_num.ty(), &Type::Number);
    let val_num = Value::new(Type::Number, ValueData::Number(Number::from_str("1")?));
    let coerced_str = val_num.coerce(&Type::String)?;
    assert_eq!(coerced_str.ty(), &Type::String);
    let val_bool_true = Value::new(Type::Bool, ValueData::Bool(true));
    let coerced_str_true = val_bool_true.coerce(&Type::String)?;
    assert_eq!(
        coerced_str_true.data.as_ref(),
        &ValueData::String("true".to_string())
    );
    let val_bool_false = Value::new(Type::Bool, ValueData::Bool(false));
    let coerced_str_false = val_bool_false.coerce(&Type::String)?;
    assert_eq!(
        coerced_str_false.data.as_ref(),
        &ValueData::String("false".to_string())
    );
    let val_str_true = Value::new(Type::String, ValueData::String("true".to_string()));
    let coerced_bool_true = val_str_true.coerce(&Type::Bool)?;
    assert_eq!(coerced_bool_true.data.as_ref(), &ValueData::Bool(true));
    let val_str_false = Value::new(Type::String, ValueData::String("false".to_string()));
    let coerced_bool_false = val_str_false.coerce(&Type::Bool)?;
    assert_eq!(coerced_bool_false.data.as_ref(), &ValueData::Bool(false));
    let val_str_bad = Value::new(Type::String, ValueData::String("bad".to_string()));
    assert!(val_str_bad.coerce(&Type::Bool).is_err());
    assert!(val_str_bad.coerce(&Type::Number).is_err());
    let invalid_str = Value::new(Type::String, ValueData::Bool(true));
    assert!(invalid_str.coerce(&Type::Number).is_err());
    assert!(invalid_str.coerce(&Type::Bool).is_err());
    let invalid_num = Value::new(Type::Number, ValueData::Bool(true));
    assert!(invalid_num.coerce(&Type::String).is_err());
    let invalid_bool = Value::new(Type::Bool, ValueData::String("true".to_string()));
    assert!(invalid_bool.coerce(&Type::String).is_err());
    let coerced_dyn = val_num.coerce(&Type::Dynamic)?;
    assert_eq!(coerced_dyn, val_num);
    let val_dyn = Value::new(Type::Dynamic, ValueData::Null);
    let coerced_from_dyn = val_dyn.coerce(&Type::Number)?;
    assert_eq!(coerced_from_dyn, val_dyn);
    assert!(val_num.coerce(&Type::Bool).is_err());
    Ok(())
}
#[derive(Debug, PartialEq, Eq)]
struct CustomResource {
    id: String,
    count: usize,
}
#[test]
fn test_capsule_value_operations() -> Result<(), HclError> {
    use std::collections::hash_map::DefaultHasher;
    let res = CustomResource {
        id: "res-123".to_string(),
        count: 42,
    };
    let cap_val = Value::capsule("custom_resource", res);
    assert!(cap_val.ty().is_capsule());
    assert_eq!(cap_val.ty().to_string(), "capsule(custom_resource)");
    let downcasted_ref = cap_val.downcast_ref::<CustomResource>();
    assert_eq!(
        downcasted_ref,
        Some(&CustomResource {
            id: "res-123".to_string(),
            count: 42,
        })
    );
    let downcasted = cap_val.downcast::<CustomResource>()?;
    assert_eq!(downcasted.id, "res-123");
    let cap_u32 = Value::capsule("other", 100_u32);
    assert_eq!(cap_u32.downcast_ref::<CustomResource>(), None);
    let err = cap_u32
        .downcast::<CustomResource>()
        .err()
        .ok_or_else(|| HclError::Eval("expected downcast error".to_string()))?;
    assert!(err.to_string().contains("Capsule downcast error"));
    let non_cap = Value::new(Type::String, ValueData::String("test".into()));
    assert_eq!(non_cap.downcast_ref::<CustomResource>(), None);
    let non_cap_err = non_cap
        .downcast::<CustomResource>()
        .err()
        .ok_or_else(|| HclError::Eval("expected downcast error".to_string()))?;
    assert!(non_cap_err.to_string().contains("Capsule downcast error"));
    assert!(non_cap.as_capsule_any().is_none());
    let cap_clone = cap_val.clone();
    assert_eq!(cap_val, cap_clone);
    let cap_other = Value::capsule(
        "custom_resource",
        CustomResource {
            id: "res-123".to_string(),
            count: 42,
        },
    );
    assert_ne!(cap_val, cap_other);
    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    cap_val.hash(&mut h1);
    cap_clone.hash(&mut h2);
    assert_eq!(h1.finish(), h2.finish());
    assert_eq!(cap_val.coerce(cap_val.ty())?, cap_val);
    assert!(cap_val.coerce(&Type::String).is_err());
    assert!(
        Value::new(Type::String, ValueData::String("x".into()))
            .coerce(cap_val.ty())
            .is_err()
    );
    Ok(())
}

#[test]
fn test_value_data_debug_formats() {
    let cap_val = Value::capsule(
        "custom_resource",
        CustomResource {
            id: "res-123".to_string(),
            count: 42,
        },
    );
    let r_debug = Refinement::not_null().with_prefix("pre");
    for (val_data, expected) in [
        (ValueData::Null, "Null"),
        (ValueData::Unknown(None), "Unknown"),
        (ValueData::Unknown(Some(Arc::new(r_debug))), "Unknown"),
        (ValueData::Bool(true), "Bool"),
        (ValueData::Number(10.into()), "Number"),
        (ValueData::String("s".into()), "String"),
        (ValueData::Array(vec![]), "Array"),
        (ValueData::Set(BTreeSet::new()), "Set"),
        (ValueData::Object(BTreeMap::new()), "Object"),
    ] {
        let dbg = format!("{val_data:?}");
        assert!(dbg.contains(expected));
    }
    let debug_str = format!("{:?}", cap_val.data);
    assert!(debug_str.starts_with("Capsule("));
}

#[test]
fn test_capsule_value_func_and_debug() -> Result<(), HclError> {
    let res = CustomResource {
        id: "res-123".to_string(),
        count: 42,
    };
    let cap_val = Value::capsule("custom_resource", res);
    let non_cap = Value::new(Type::String, ValueData::String("test".into()));
    let func = crate::eval::func::Function {
        name: "bump_count".to_string(),
        func: Arc::new(|args| {
            let resource = args[0].downcast::<CustomResource>()?;
            let updated = CustomResource {
                id: resource.id.clone(),
                count: resource.count + 1,
            };
            Ok(Value::capsule("custom_resource", updated))
        }),
        signature: None,
    };
    let result = (func.func)(&[cap_val])?;
    assert_eq!(
        result.downcast_ref::<CustomResource>(),
        Some(&CustomResource {
            id: "res-123".to_string(),
            count: 43,
        })
    );
    assert!((func.func)(&[non_cap]).is_err());
    Ok(())
}
#[test]
fn test_value_marks_comprehensive() {
    let sens = ValueMark::Sensitive;
    assert_eq!(sens.to_string(), "sensitive");
    let custom = ValueMark::custom("origin:env");
    assert_eq!(custom.to_string(), "origin:env");
    assert_ne!(sens, custom);
    let mut initial_marks = BTreeSet::new();
    initial_marks.insert(sens);
    let val = Value::new_with_marks(
        Type::String,
        ValueData::String("secret".into()),
        initial_marks.clone(),
    );
    assert_eq!(val.marks(), &initial_marks);
    assert!(val.has_mark(&ValueMark::Sensitive));
    assert!(!val.has_mark(&custom));
    assert!(val.is_sensitive());
    assert_eq!(val.to_string(), "(sensitive value)");
    let marked = val.mark(custom.clone());
    assert!(marked.has_mark(&custom));
    assert!(marked.has_mark(&ValueMark::Sensitive));
    assert_eq!(marked.marks().len(), 2);
    let (unmarked, removed) = marked.unmark();
    assert_eq!(unmarked.marks().len(), 0);
    assert_eq!(removed.len(), 2);
    assert!(!unmarked.is_sensitive());
    assert_eq!(unmarked.to_string(), "\"secret\"");
    let reset = unmarked.with_marks(initial_marks);
    assert!(reset.is_sensitive());
}

#[test]
fn test_value_marks_deep_and_coercion() {
    let custom = ValueMark::custom("origin:env");
    let s1 = Value::new(Type::String, ValueData::String("public".into()));
    let s2 =
        Value::new(Type::String, ValueData::String("hidden".into())).mark(ValueMark::Sensitive);
    let s3 = Value::new(Type::String, ValueData::String("tagged".into())).mark(custom.clone());
    let arr_val = Value::new(
        Type::List(Box::new(Type::String)),
        ValueData::Array(vec![s1.clone(), s2.clone()]),
    );
    assert!(arr_val.is_sensitive());
    let (unmarked_arr, arr_marks) = arr_val.unmark_deep();
    assert!(arr_marks.contains(&ValueMark::Sensitive));
    assert!(!unmarked_arr.is_sensitive());
    let mut set_data = BTreeSet::new();
    set_data.insert(s1.clone());
    set_data.insert(s2.clone());
    let set_val = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_data));
    assert!(set_val.is_sensitive());
    let (unmarked_set, set_marks) = set_val.unmark_deep();
    assert!(set_marks.contains(&ValueMark::Sensitive));
    assert!(!unmarked_set.is_sensitive());
    let mut obj_data = BTreeMap::new();
    obj_data.insert("a".to_string(), s1);
    obj_data.insert("b".to_string(), s2.clone());
    obj_data.insert("c".to_string(), s3);
    let obj_val = Value::new(Type::Dynamic, ValueData::Object(obj_data));
    assert!(obj_val.is_sensitive());
    let (unmarked_obj, obj_marks) = obj_val.unmark_deep();
    assert!(obj_marks.contains(&ValueMark::Sensitive));
    assert!(obj_marks.contains(&custom));
    assert!(!unmarked_obj.is_sensitive());
    let obj_no_sens = obj_val.unmark_sensitive();
    assert!(!obj_no_sens.is_sensitive());
    assert_eq!(obj_no_sens.paths_with_mark(&custom).len(), 1);
    let (prim_unmark, prim_marks) = s2.unmark_deep();
    assert_eq!(prim_marks.len(), 1);
    assert_eq!(prim_unmark.marks().len(), 0);
    let arr_no_sens = arr_val.unmark_sensitive();
    assert!(!arr_no_sens.is_sensitive());
    let set_no_sens = set_val.unmark_sensitive();
    assert!(!set_no_sens.is_sensitive());
    let rendered_obj = unmarked_obj.to_string();
    assert!(rendered_obj.contains("\"a\" = \"public\""));
    assert_eq!(unmarked_arr.to_string(), "[\"public\", \"hidden\"]");
}

#[test]
fn test_value_display_and_mark_coercion() -> Result<(), HclError> {
    let s1 = Value::new(Type::String, ValueData::String("public".into()));
    assert_eq!(Value::null(Type::Dynamic).to_string(), "null");
    assert_eq!(
        Value::unknown(Type::Dynamic).to_string(),
        "(known after apply)"
    );
    assert_eq!(
        Value::new(Type::Bool, ValueData::Bool(true)).to_string(),
        "true"
    );
    assert_eq!(
        Value::new(Type::Number, ValueData::Number(42_i32.into())).to_string(),
        "42"
    );
    assert_eq!(s1.to_string(), "\"public\"");
    let cap = Value::capsule("res", 123_u32);
    assert!(cap.to_string().starts_with("capsule("));
    let sens_str =
        Value::new(Type::String, ValueData::String("42".into())).mark(ValueMark::Sensitive);
    let coerced_num = sens_str.coerce(&Type::Number)?;
    assert!(coerced_num.has_mark(&ValueMark::Sensitive));
    let unk_sens = Value::unknown(Type::String).mark(ValueMark::Sensitive);
    assert!(
        unk_sens
            .coerce(&Type::Number)?
            .has_mark(&ValueMark::Sensitive)
    );
    let null_sens = Value::null(Type::String).mark(ValueMark::Sensitive);
    assert!(
        null_sens
            .coerce(&Type::Number)?
            .has_mark(&ValueMark::Sensitive)
    );
    Ok(())
}
#[test]
fn test_value_set_display_and_refinement_arc() {
    let empty_set = Value::new(
        Type::Set(Box::new(Type::Number)),
        ValueData::Set(BTreeSet::new()),
    );
    assert_eq!(empty_set.to_string(), "[]");
    let multi_set = Value::new(
        Type::Set(Box::new(Type::Number)),
        ValueData::Set(BTreeSet::from([
            Value::new(Type::Number, ValueData::Number(1_i32.into())),
            Value::new(Type::Number, ValueData::Number(2_i32.into())),
        ])),
    );
    assert_eq!(multi_set.to_string(), "[1, 2]");
    let non_unk = Value::new(Type::Bool, ValueData::Bool(true));
    assert!(non_unk.refinement_arc().is_none());
}
#[test]
fn test_value_unknown_refined_coercion() -> Result<(), HclError> {
    use crate::types::refinement::Refinement;
    let ref_digit = Refinement::new()
        .with_prefix("100")
        .with_string_length(3, 10)?;
    let unk_str_digit = Value::unknown_refined(Type::String, ref_digit);
    let coerced_num = unk_str_digit.coerce(&Type::Number)?;
    assert_eq!(coerced_num.ty, Type::Number);
    let ref_after = coerced_num
        .refinement()
        .ok_or_else(|| HclError::Eval("expected refinement".to_string()))?;
    assert!(ref_after.string_prefix.is_none());
    assert!(ref_after.string_length_min.is_none());
    for prefix in ["-42", "+42", ".5", ""] {
        let r = Refinement::new().with_prefix(prefix);
        let unk_str = Value::unknown_refined(Type::String, r);
        assert!(unk_str.coerce(&Type::Number).is_ok());
    }
    let ref_no_prefix = Refinement::new().with_string_length(1, 10)?;
    let unk_str_no_prefix = Value::unknown_refined(Type::String, ref_no_prefix);
    assert!(unk_str_no_prefix.coerce(&Type::Number).is_ok());
    let ref_alpha = Refinement::new().with_prefix("abc");
    let unk_str_alpha = Value::unknown_refined(Type::String, ref_alpha);
    assert!(unk_str_alpha.coerce(&Type::Number).is_err());
    let ref_num = Refinement::new().with_number_range(
        crate::number::Number::from(1),
        crate::number::Number::from(50),
    )?;
    let unk_num = Value::unknown_refined(Type::Number, ref_num);
    let coerced_str = unk_num.coerce(&Type::String)?;
    assert_eq!(coerced_str.ty, Type::String);
    let ref_str_after = coerced_str
        .refinement()
        .ok_or_else(|| HclError::Eval("expected refinement".to_string()))?;
    assert!(ref_str_after.number_min.is_none());
    assert!(ref_str_after.number_max.is_none());
    let ref_not_null = Refinement::not_null();
    let unk_val_other = Value::unknown_refined(Type::Set(Box::new(Type::String)), ref_not_null);
    let target_list = Type::List(Box::new(Type::String));
    let coerced_other = unk_val_other.coerce(&target_list)?;
    assert_eq!(coerced_other.ty, target_list);
    let mut src_schema = BTreeMap::new();
    src_schema.insert("foo".to_string(), Type::String);
    let src_ty = Type::object(src_schema);
    let mut dst_schema = BTreeMap::new();
    dst_schema.insert("bar".to_string(), Type::String);
    let dst_ty = Type::object(dst_schema);
    let unk_bad_ref = Value::unknown_refined(
        src_ty,
        Refinement::new().with_object_attr("not_allowed", Refinement::not_null()),
    );
    assert!(unk_bad_ref.coerce(&dst_ty).is_err());
    Ok(())
}
struct CountdownWriter(usize);
impl std::fmt::Write for CountdownWriter {
    fn write_str(&mut self, _s: &str) -> std::fmt::Result {
        if self.0 == 0 {
            Err(std::fmt::Error)
        } else {
            self.0 -= 1;
            Ok(())
        }
    }
}
#[test]
fn test_value_display_error_paths() {
    use std::fmt::Write;
    let arr = Value::new(
        Type::List(Box::new(Type::Number)),
        ValueData::Array(vec![
            Value::new(Type::Number, ValueData::Number(1_i32.into())),
            Value::new(Type::Number, ValueData::Number(2_i32.into())),
        ]),
    );
    for count in 0..6 {
        let mut writer = CountdownWriter(count);
        let _ = write!(writer, "{arr}");
    }
    let set = Value::new(
        Type::Set(Box::new(Type::Number)),
        ValueData::Set(BTreeSet::from([
            Value::new(Type::Number, ValueData::Number(1_i32.into())),
            Value::new(Type::Number, ValueData::Number(2_i32.into())),
        ])),
    );
    for count in 0..6 {
        let mut writer = CountdownWriter(count);
        let _ = write!(writer, "{set}");
    }
    let mut map = BTreeMap::new();
    map.insert(
        "k1".to_string(),
        Value::new(Type::Number, ValueData::Number(1_i32.into())),
    );
    map.insert(
        "k2".to_string(),
        Value::new(Type::Number, ValueData::Number(2_i32.into())),
    );
    let obj = Value::new(Type::object(BTreeMap::new()), ValueData::Object(map));
    for count in 0..8 {
        let mut writer = CountdownWriter(count);
        let _ = write!(writer, "{obj}");
    }
}
#[test]
fn test_value_path_traversal_and_transform() -> Result<(), HclError> {
    use crate::encode::EncodeValue;
    use crate::types::path::{Path, PathStep};
    let mut node1 = BTreeMap::new();
    node1.insert("host".to_string(), "node-1".encode_value());
    let mut node2 = BTreeMap::new();
    node2.insert("host".to_string(), "node-2".encode_value());
    let servers = Value::new(
        Type::List(Box::new(Type::Dynamic)),
        ValueData::Array(vec![
            Value::new(Type::Dynamic, ValueData::Object(node1)),
            Value::new(Type::Dynamic, ValueData::Object(node2)),
        ]),
    );
    let mut cluster = BTreeMap::new();
    cluster.insert("servers".to_string(), servers);
    let mut root_map = BTreeMap::new();
    root_map.insert(
        "cluster".to_string(),
        Value::new(Type::Dynamic, ValueData::Object(cluster)),
    );
    let root = Value::new(Type::Dynamic, ValueData::Object(root_map));
    let path = Path::empty()
        .with_step(PathStep::GetAttr("cluster".to_string()))
        .with_step(PathStep::GetAttr("servers".to_string()))
        .with_step(PathStep::Index(1_i64.encode_value()))
        .with_step(PathStep::Key("host".encode_value()));
    let val = root.get_path(&path)?;
    assert_eq!(val.to_string(), "\"node-2\"");
    let bad_attr_path = Path::empty().with_step(PathStep::GetAttr("nonexistent".to_string()));
    assert!(root.get_path(&bad_attr_path).is_err());
    let bad_type_attr_path = Path::empty()
        .with_step(PathStep::GetAttr("cluster".to_string()))
        .with_step(PathStep::GetAttr("servers".to_string()))
        .with_step(PathStep::GetAttr("invalid_on_array".to_string()));
    assert!(root.get_path(&bad_type_attr_path).is_err());
    let out_of_bounds_path = Path::empty()
        .with_step(PathStep::GetAttr("cluster".to_string()))
        .with_step(PathStep::GetAttr("servers".to_string()))
        .with_step(PathStep::Index(99_i64.encode_value()));
    assert!(root.get_path(&out_of_bounds_path).is_err());
    let bad_index_val_path = Path::empty()
        .with_step(PathStep::GetAttr("cluster".to_string()))
        .with_step(PathStep::GetAttr("servers".to_string()))
        .with_step(PathStep::Index("not_a_num".encode_value()));
    assert!(root.get_path(&bad_index_val_path).is_err());
    let mut visited_paths = Vec::new();
    root.walk(&mut |p, _val| {
        visited_paths.push(p.to_string());
        Ok(true)
    })?;
    assert!(
        visited_paths
            .iter()
            .any(|p| p.contains("cluster.servers[0].host"))
    );
    let transformed = root.transform(&mut |p, val| {
        if p.to_string().ends_with("host") {
            Ok("REDACTED".encode_value())
        } else {
            Ok(val.clone())
        }
    })?;
    let redacted_val = transformed.get_path(&path)?;
    assert_eq!(redacted_val.to_string(), "\"REDACTED\"");
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CustomOpaque {
    id: u64,
    payload: String,
}
fn make_custom_opaque_ops() -> Arc<crate::types::ty::CapsuleOps> {
    use crate::encode::EncodeValue;
    use crate::types::ty::CapsuleOps;
    use std::collections::hash_map::DefaultHasher;
    Arc::new(
        CapsuleOps::new(
            "custom_opaque",
            Arc::new(|a, b| {
                a.downcast_ref::<CustomOpaque>()
                    .zip(b.downcast_ref::<CustomOpaque>())
                    .is_some_and(|(x, y)| x == y)
            }),
            Arc::new(|a| {
                a.downcast_ref::<CustomOpaque>().map_or(0, |x| {
                    let mut s = DefaultHasher::new();
                    x.hash(&mut s);
                    s.finish()
                })
            }),
        )
        .with_conversion_to(Arc::new(|a, target| {
            if target == &Type::String {
                a.downcast_ref::<CustomOpaque>()
                    .map(|x| format!("opaque-{}:{}", x.id, x.payload).encode_value())
            } else {
                None
            }
        }))
        .with_conversion_from(Arc::new(|val| {
            let ValueData::String(ref s) = *val.data else {
                return None;
            };
            let (id_str, payload) = s.strip_prefix("opaque-")?.split_once(':')?;
            let id = id_str.parse::<u64>().ok()?;
            Some(Arc::new(CustomOpaque {
                id,
                payload: payload.to_string(),
            }))
        })),
    )
}
#[test]
fn test_capsule_ops_custom_hooks() {
    use crate::encode::EncodeValue;
    use std::collections::hash_map::DefaultHasher;
    let ops = make_custom_opaque_ops();
    let obj1 = CustomOpaque {
        id: 100,
        payload: "alpha".to_string(),
    };
    let obj2 = CustomOpaque {
        id: 100,
        payload: "alpha".to_string(),
    };
    let obj3 = CustomOpaque {
        id: 200,
        payload: "beta".to_string(),
    };
    let cap1 = Value::capsule_with_ops("custom_opaque", ops.clone(), obj1);
    let cap2 = Value::capsule_with_ops("custom_opaque", ops.clone(), obj2);
    let cap3 = Value::capsule_with_ops("custom_opaque", ops.clone(), obj3);
    assert_eq!(cap1, cap2);
    assert_ne!(cap1, cap3);
    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    cap1.hash(&mut h1);
    cap2.hash(&mut h2);
    assert_eq!(h1.finish(), h2.finish());
    let coerced_str = cap1.coerce(&Type::String);
    assert_eq!(
        coerced_str.as_ref().ok().map(ToString::to_string),
        Some("\"opaque-100:alpha\"".to_string())
    );
    let str_source = "opaque-300:gamma".encode_value();
    let coerced_cap = str_source.coerce(cap1.ty());
    let downcasted = coerced_cap
        .as_ref()
        .ok()
        .and_then(|c| c.downcast::<CustomOpaque>().ok());
    assert_eq!(downcasted.as_ref().map(|d| d.id), Some(300));
    assert_eq!(
        downcasted.as_ref().map(|d| d.payload.as_str()),
        Some("gamma")
    );
    let bad_any: Arc<dyn std::any::Any + Send + Sync> = Arc::new(123_i32);
    assert!(!(ops.equals)(&bad_any, &bad_any));
    assert_eq!((ops.hash)(&bad_any), 0);
    assert!(
        ops.conversion_to
            .as_ref()
            .is_some_and(|conv| (conv)(&bad_any, &Type::Number).is_none())
    );
    assert!(
        ops.conversion_to
            .as_ref()
            .is_some_and(|conv| (conv)(&bad_any, &Type::String).is_none())
    );
    for invalid in [
        123_i64.encode_value(),
        "non_opaque".encode_value(),
        "opaque-notanumber:xyz".encode_value(),
        "opaque-123".encode_value(),
        "opaque-".encode_value(),
    ] {
        assert!(
            ops.conversion_from
                .as_ref()
                .is_some_and(|conv| (conv)(&invalid).is_none())
        );
    }
    let from_cap = Value::from_capsule(
        CustomOpaque {
            id: 400,
            payload: "delta".to_string(),
        },
        ops,
    );
    assert_eq!(
        from_cap.downcast_ref::<CustomOpaque>().map(|c| c.id),
        Some(400)
    );
    assert!(from_cap.downcast_ref::<String>().is_none());
    assert!(
        42_i64
            .encode_value()
            .downcast_ref::<CustomOpaque>()
            .is_none()
    );
}

#[test]
fn test_capsule_downcast_and_diagnostic_path() {
    use crate::encode::EncodeValue;
    let from_cap = Value::capsule(
        "custom_opaque",
        CustomOpaque {
            id: 400,
            payload: "x".to_string(),
        },
    );
    assert_eq!(
        from_cap.as_capsule::<CustomOpaque>().ok().map(|c| c.id),
        Some(400)
    );
    assert!(from_cap.downcast::<CustomOpaque>().is_ok());
    assert!(from_cap.downcast_ref::<CustomOpaque>().is_some());
    assert!(from_cap.as_capsule::<String>().is_err());
    assert!(42_i64.encode_value().as_capsule::<CustomOpaque>().is_err());
    assert!(from_cap.downcast::<String>().is_err());
    assert!(42_i64.encode_value().downcast::<CustomOpaque>().is_err());
    let from_cap_str = Value::capsule("str_capsule", "hello".to_string());
    assert!(from_cap_str.as_capsule::<String>().is_ok());
    assert!(from_cap_str.downcast::<String>().is_ok());
    assert!(from_cap_str.downcast_ref::<String>().is_some());
    assert!(42_i64.encode_value().as_capsule::<String>().is_err());
    assert!(42_i64.encode_value().downcast::<String>().is_err());
    assert!(42_i64.encode_value().downcast_ref::<String>().is_none());
    assert!(from_cap_str.as_capsule::<CustomOpaque>().is_err());
    assert!(from_cap_str.downcast::<CustomOpaque>().is_err());
    assert!(from_cap_str.downcast_ref::<CustomOpaque>().is_none());
    assert_eq!(
        ValueMark::custom("audit"),
        ValueMark::Custom("audit".to_string())
    );
    let dummy_span = crate::span::Span::new(0, 0, 1, 1, 1, 10);
    let p = crate::types::path::Path::empty()
        .with_step(crate::types::path::PathStep::GetAttr(
            "settings".to_string(),
        ))
        .with_step(crate::types::path::PathStep::Index(0_i64.encode_value()));
    let diag = crate::diagnostic::Diagnostic::error("Type Error", "Invalid schema", dummy_span)
        .with_path(p);
    let writer = crate::diagnostic::DiagnosticWriter::new(false);
    let rendered = writer.format_diagnostic(&diag, "test.hcl", "settings = [123]\n");
    assert!(rendered.contains("at path: settings[0]"));
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, crate::types::CapsuleType)]
#[hcl(
    type_name = "managed_client",
    convert_to = "client_to_string",
    convert_from = "client_from_string"
)]
struct ManagedClient {
    endpoint: String,
    retries: u32,
}
fn client_to_string(client: &ManagedClient, target: &Type) -> Option<Value> {
    if target == &Type::String {
        Some(Value::new(
            Type::String,
            ValueData::String(format!("client://{}:{}", client.endpoint, client.retries)),
        ))
    } else {
        None
    }
}
fn client_from_string(val: &Value) -> Option<ManagedClient> {
    let ValueData::String(ref s) = *val.data else {
        return None;
    };
    let (endpoint, retries_str) = s.strip_prefix("client://")?.split_once(':')?;
    let retries = retries_str.parse::<u32>().ok()?;
    Some(ManagedClient {
        endpoint: endpoint.to_string(),
        retries,
    })
}
#[test]
fn test_derive_capsule_type_integration() {
    use crate::encode::EncodeValue;
    assert!(client_from_string(&"not_client_protocol".encode_value()).is_none());
    assert!(client_from_string(&"client://endpoint:bad_int".encode_value()).is_none());
    assert!(client_from_string(&123_i64.encode_value()).is_none());
    let client1 = ManagedClient {
        endpoint: "api.example.com".to_string(),
        retries: 3,
    };
    let client2 = ManagedClient {
        endpoint: "api.example.com".to_string(),
        retries: 3,
    };
    let client3 = ManagedClient {
        endpoint: "api.other.com".to_string(),
        retries: 1,
    };
    let val1: Value = client1.clone().into();
    let val2: Value = client2.into();
    let val3: Value = client3.into();
    assert_eq!(val1.ty().to_string(), "capsule(managed_client)");
    assert_eq!(val1, val2);
    assert_ne!(val1, val3);
    assert_eq!(
        val1.as_capsule::<ManagedClient>()
            .ok()
            .map(|c| c.endpoint.as_str()),
        Some("api.example.com")
    );
    assert_eq!(
        val1.as_capsule::<ManagedClient>().ok().map(|c| c.retries),
        Some(3)
    );
    assert!(val1.downcast::<ManagedClient>().is_ok());
    assert!(val1.downcast_ref::<ManagedClient>().is_some());
    let non_cap_mc = 42_i64.encode_value();
    assert!(non_cap_mc.as_capsule::<ManagedClient>().is_err());
    assert!(non_cap_mc.downcast::<ManagedClient>().is_err());
    assert!(non_cap_mc.downcast_ref::<ManagedClient>().is_none());
    let wrong_cap_mc = Value::capsule("other", 100_u32);
    assert!(wrong_cap_mc.as_capsule::<ManagedClient>().is_err());
    assert!(wrong_cap_mc.downcast::<ManagedClient>().is_err());
    assert!(wrong_cap_mc.downcast_ref::<ManagedClient>().is_none());
    let coerced = val1.coerce(&Type::String);
    assert_eq!(
        coerced.as_ref().ok().map(ToString::to_string),
        Some("\"client://api.example.com:3\"".to_string())
    );
    let cap_ty =
        Type::capsule_with_ops::<ManagedClient>("managed_client", ManagedClient::capsule_ops());
    let back = coerced.as_ref().map(|c| c.coerce(&cap_ty));
    assert_eq!(
        back.as_ref()
            .ok()
            .and_then(|r| r.as_ref().ok())
            .and_then(|v| v.as_capsule::<ManagedClient>().ok()),
        Some(&client1)
    );
    let mut ctx = crate::eval::context::Context::new();
    ctx.set_variable("client", val1.clone());
    let expr =
        crate::ast::expr::Expression::Variable("client".to_string(), crate::span::Span::default());
    let mut eval = crate::eval::evaluator::Evaluator::new(&ctx);
    let eval_val = eval.eval_expr(&expr);
    assert_eq!(eval_val, val1);
    assert!(client_to_string(&client1, &Type::Number).is_none());
    assert!(client_from_string(&"not_client".encode_value()).is_none());
    assert!(client_from_string(&"client://onlyendpoint".encode_value()).is_none());
    assert!(client_from_string(&"client://endpoint:badretries".encode_value()).is_none());
}
#[test]
fn test_value_coverage_exhaustive() {
    use crate::encode::EncodeValue;
    use crate::types::path::{Path, PathStep};
    let non_arr = "not_array".encode_value();
    let p_idx = Path::empty().with_step(PathStep::Index(0_i64.encode_value()));
    assert!(non_arr.get_path(&p_idx).is_err());
    let mut obj_map = std::collections::BTreeMap::new();
    obj_map.insert("foo".to_string(), 100_i64.encode_value());
    let obj = Value::new(
        Type::object(std::collections::BTreeMap::new()),
        ValueData::Object(obj_map),
    );
    let p_missing = Path::empty().with_step(PathStep::Key("missing".encode_value()));
    assert!(obj.get_path(&p_missing).is_err());
    let p_num_key = Path::empty().with_step(PathStep::Key(123_i64.encode_value()));
    assert!(obj.get_path(&p_num_key).is_err());
    let non_obj = 42_i64.encode_value();
    let p_key = Path::empty().with_step(PathStep::Key("foo".encode_value()));
    assert!(non_obj.get_path(&p_key).is_err());
    let mut s = std::collections::BTreeSet::new();
    s.insert(1_i64.encode_value());
    s.insert(2_i64.encode_value());
    let set_val = Value::new(Type::Set(Box::new(Type::Number)), ValueData::Set(s));
    let transformed_set = set_val.transform(&mut |_p, v| {
        if let ValueData::Number(n) = &*v.data {
            Ok(Value::new(
                Type::Number,
                ValueData::Number(n.clone() + crate::number::Number::from(10)),
            ))
        } else {
            Ok(v.clone())
        }
    });
    assert_eq!(
        transformed_set.as_ref().ok().map(Value::ty),
        Some(&Type::Set(Box::new(Type::Number)))
    );
    let mut count = 0;
    let walk_res = set_val.walk(&mut |_p, _v| {
        count += 1;
        Ok(true)
    });
    assert!(walk_res.is_ok());
    assert_eq!(count, 3);
    let mut pruned_count = 0;
    let prune_res = set_val.walk(&mut |_p, _v| {
        pruned_count += 1;
        Ok(false)
    });
    assert!(prune_res.is_ok());
    assert_eq!(pruned_count, 1);
}

#[test]
fn test_value_coverage_exhaustive_walk_errors() {
    use crate::encode::EncodeValue;
    use crate::error::HclError;
    use crate::types::path::Path;
    let mut obj_map = std::collections::BTreeMap::new();
    obj_map.insert("foo".to_string(), 100_i64.encode_value());
    let obj = Value::new(
        Type::object(std::collections::BTreeMap::new()),
        ValueData::Object(obj_map),
    );
    let mut s = std::collections::BTreeSet::new();
    s.insert(1_i64.encode_value());
    s.insert(2_i64.encode_value());
    let set_val = Value::new(Type::Set(Box::new(Type::Number)), ValueData::Set(s));
    let mut should_descend = true;
    let mut toggle_count = 0;
    let mut toggle_visitor = |_p: &Path, _v: &Value| {
        toggle_count += 1;
        let res = should_descend;
        should_descend = false;
        Ok(res)
    };
    assert!(set_val.walk(&mut toggle_visitor).is_ok());
    assert!(toggle_count >= 2);
    let arr_val = Value::new(
        Type::List(Box::new(Type::Number)),
        ValueData::Array(vec![1_i64.encode_value()]),
    );
    assert!(
        arr_val
            .transform(&mut |_p, _v| Err(HclError::Type("fail".into())))
            .is_err()
    );
    assert!(
        obj.transform(&mut |_p, _v| Err(HclError::Type("fail".into())))
            .is_err()
    );
    assert!(
        set_val
            .transform(&mut |_p, _v| Err(HclError::Type("fail".into())))
            .is_err()
    );
    assert!(
        arr_val
            .walk(&mut |_p, _v| Err(HclError::Type("fail root".into())))
            .is_err()
    );
    assert!(
        arr_val
            .walk(&mut |_p, v| {
                if matches!(v.ty(), Type::Number) {
                    Err(HclError::Type("fail child".into()))
                } else {
                    Ok(true)
                }
            })
            .is_err()
    );
    assert!(
        obj.walk(&mut |_p, v| {
            if matches!(v.ty(), Type::Number) {
                Err(HclError::Type("fail child".into()))
            } else {
                Ok(true)
            }
        })
        .is_err()
    );
    assert!(
        set_val
            .walk(&mut |_p, v| {
                if matches!(v.ty(), Type::Number) {
                    Err(HclError::Type("fail child".into()))
                } else {
                    Ok(true)
                }
            })
            .is_err()
    );
    let cap_ty = Type::capsule::<()>("opaque_cap");
    assert!("hello".encode_value().coerce(&cap_ty).is_err());
}
#[test]
fn test_granular_path_based_marks() -> Result<(), HclError> {
    use crate::encode::EncodeValue;
    let mut db = BTreeMap::new();
    db.insert("host".to_string(), "localhost".encode_value());
    db.insert("password".to_string(), "supersecret".encode_value());
    let servers = vec!["web-1".encode_value(), "web-2".encode_value()];
    let mut tags = BTreeMap::new();
    tags.insert("env".to_string(), "production".encode_value());
    let mut root_map = BTreeMap::new();
    root_map.insert(
        "database".to_string(),
        Value::new(Type::Dynamic, ValueData::Object(db)),
    );
    root_map.insert(
        "servers".to_string(),
        Value::new(Type::Dynamic, ValueData::Array(servers)),
    );
    root_map.insert(
        "tags".to_string(),
        Value::new(Type::Dynamic, ValueData::Object(tags)),
    );
    let root_val = Value::new(Type::Dynamic, ValueData::Object(root_map));
    assert!(!root_val.has_marked_children());
    assert!(root_val.paths_with_mark(&ValueMark::Sensitive).is_empty());
    let path_pass = Path::new(vec![
        PathStep::GetAttr("database".to_string()),
        PathStep::GetAttr("password".to_string()),
    ]);
    let marked_root = root_val.mark_path(&path_pass, ValueMark::Sensitive)?;
    assert!(marked_root.has_marked_children());
    let sens_paths = marked_root.paths_with_mark(&ValueMark::Sensitive);
    assert_eq!(sens_paths.len(), 1);
    assert_eq!(sens_paths[0].to_string(), "database.password");
    let path_srv = Path::new(vec![
        PathStep::GetAttr("servers".to_string()),
        PathStep::Index(Value::new(Type::Number, ValueData::Number(1.into()))),
    ]);
    let marked_both = marked_root.mark_path(&path_srv, ValueMark::Sensitive)?;
    let both_paths = marked_both.paths_with_mark(&ValueMark::Sensitive);
    assert_eq!(both_paths.len(), 2);
    let path_tag = Path::new(vec![
        PathStep::GetAttr("tags".to_string()),
        PathStep::Key("env".encode_value()),
    ]);
    let custom_mark = ValueMark::custom("env:prod");
    let marked_custom = marked_both.mark_path(&path_tag, custom_mark.clone())?;
    assert_eq!(marked_custom.paths_with_mark(&custom_mark).len(), 1);
    let marked_root_self = marked_custom.mark_path(&Path::empty(), ValueMark::Sensitive)?;
    assert!(marked_root_self.has_mark(&ValueMark::Sensitive));
    let (unmarked_pass, removed_marks) = marked_both.unmark_path(&path_pass)?;
    assert!(removed_marks.contains(&ValueMark::Sensitive));
    let remaining = unmarked_pass.paths_with_mark(&ValueMark::Sensitive);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].to_string(), "servers[1]");
    let (unmarked_all_root, root_rem) = marked_root_self.unmark_path(&Path::empty())?;
    assert!(root_rem.contains(&ValueMark::Sensitive));
    assert!(!unmarked_all_root.has_mark(&ValueMark::Sensitive));
    let mut test_set = BTreeSet::new();
    test_set.insert("item1".encode_value().mark(ValueMark::Sensitive));
    let set_val = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(test_set));
    assert!(set_val.has_marked_children());
    assert_eq!(set_val.paths_with_mark(&ValueMark::Sensitive).len(), 1);
    Ok(())
}

#[test]
fn test_granular_path_based_marks_eval() -> Result<(), HclError> {
    use crate::ast::expr::{Expression, Traversal, TraversalOperator};
    use crate::encode::EncodeValue;
    use crate::eval::evaluator::Evaluator;
    let mut db = BTreeMap::new();
    db.insert("host".to_string(), "localhost".encode_value());
    db.insert("password".to_string(), "supersecret".encode_value());
    let servers = vec!["web-1".encode_value(), "web-2".encode_value()];
    let mut root_map = BTreeMap::new();
    root_map.insert(
        "database".to_string(),
        Value::new(Type::Dynamic, ValueData::Object(db)),
    );
    root_map.insert(
        "servers".to_string(),
        Value::new(Type::Dynamic, ValueData::Array(servers)),
    );
    let root_val = Value::new(Type::Dynamic, ValueData::Object(root_map));
    let path_pass = Path::new(vec![
        PathStep::GetAttr("database".to_string()),
        PathStep::GetAttr("password".to_string()),
    ]);
    let marked_root = root_val.mark_path(&path_pass, ValueMark::Sensitive)?;
    let path_srv = Path::new(vec![
        PathStep::GetAttr("servers".to_string()),
        PathStep::Index(Value::new(Type::Number, ValueData::Number(1.into()))),
    ]);
    let marked_both = marked_root.mark_path(&path_srv, ValueMark::Sensitive)?;
    let mut eval_ctx = crate::eval::context::Context::new();
    eval_ctx.set_variable("cfg", marked_both);
    let s = crate::span::Span::default();
    let make_attr_trav = |a1: &str, a2: &str| {
        Expression::Traversal(
            Box::new(Traversal {
                expr: Box::new(Expression::Variable("cfg".to_string(), s.clone())),
                operators: vec![
                    TraversalOperator::GetAttr(a1.to_string(), s.clone()),
                    TraversalOperator::GetAttr(a2.to_string(), s.clone()),
                ],
            }),
            s.clone(),
        )
    };
    let make_idx_trav = |a1: &str, idx: i32| {
        Expression::Traversal(
            Box::new(Traversal {
                expr: Box::new(Expression::Variable("cfg".to_string(), s.clone())),
                operators: vec![
                    TraversalOperator::GetAttr(a1.to_string(), s.clone()),
                    TraversalOperator::Index(Expression::Number(idx.into(), s.clone()), s.clone()),
                ],
            }),
            s.clone(),
        )
    };
    let (val_pass, _) =
        Evaluator::new(&eval_ctx).evaluate(&make_attr_trav("database", "password"))?;
    assert!(val_pass.has_mark(&ValueMark::Sensitive));
    let (val_host, _) = Evaluator::new(&eval_ctx).evaluate(&make_attr_trav("database", "host"))?;
    assert!(!val_host.has_mark(&ValueMark::Sensitive));
    let (val_s1, _) = Evaluator::new(&eval_ctx).evaluate(&make_idx_trav("servers", 1))?;
    assert!(val_s1.has_mark(&ValueMark::Sensitive));
    let (val_s0, _) = Evaluator::new(&eval_ctx).evaluate(&make_idx_trav("servers", 0))?;
    assert!(!val_s0.has_mark(&ValueMark::Sensitive));
    Ok(())
}

#[test]
fn test_granular_path_based_marks_errors() {
    use crate::encode::EncodeValue;
    let mut db = BTreeMap::new();
    db.insert("host".to_string(), "localhost".encode_value());
    let servers = vec!["web-1".encode_value(), "web-2".encode_value()];
    let mut tags = BTreeMap::new();
    tags.insert("env".to_string(), "production".encode_value());
    let mut root_map = BTreeMap::new();
    root_map.insert(
        "database".to_string(),
        Value::new(Type::Dynamic, ValueData::Object(db)),
    );
    root_map.insert(
        "servers".to_string(),
        Value::new(Type::Dynamic, ValueData::Array(servers)),
    );
    root_map.insert(
        "tags".to_string(),
        Value::new(Type::Dynamic, ValueData::Object(tags)),
    );
    let root_val = Value::new(Type::Dynamic, ValueData::Object(root_map));
    let bad_attr = Path::new(vec![PathStep::GetAttr("nonexistent".to_string())]);
    assert!(root_val.mark_path(&bad_attr, ValueMark::Sensitive).is_err());
    assert!(root_val.unmark_path(&bad_attr).is_err());
    let non_obj = "hello".encode_value();
    assert!(non_obj.mark_path(&bad_attr, ValueMark::Sensitive).is_err());
    assert!(non_obj.unmark_path(&bad_attr).is_err());
    let bad_idx_val = Path::new(vec![
        PathStep::GetAttr("servers".to_string()),
        PathStep::Index("not_a_num".encode_value()),
    ]);
    assert!(
        root_val
            .mark_path(&bad_idx_val, ValueMark::Sensitive)
            .is_err()
    );
    assert!(root_val.unmark_path(&bad_idx_val).is_err());
    let oob_idx = Path::new(vec![
        PathStep::GetAttr("servers".to_string()),
        PathStep::Index(Value::new(Type::Number, ValueData::Number(99.into()))),
    ]);
    assert!(root_val.mark_path(&oob_idx, ValueMark::Sensitive).is_err());
    assert!(root_val.unmark_path(&oob_idx).is_err());
    let idx_on_non_arr = Path::new(vec![
        PathStep::GetAttr("database".to_string()),
        PathStep::Index(Value::new(Type::Number, ValueData::Number(0.into()))),
    ]);
    assert!(
        root_val
            .mark_path(&idx_on_non_arr, ValueMark::Sensitive)
            .is_err()
    );
    assert!(root_val.unmark_path(&idx_on_non_arr).is_err());
    let bad_key_type = Path::new(vec![
        PathStep::GetAttr("tags".to_string()),
        PathStep::Key(123.encode_value()),
    ]);
    assert!(
        root_val
            .mark_path(&bad_key_type, ValueMark::Sensitive)
            .is_err()
    );
    assert!(root_val.unmark_path(&bad_key_type).is_err());
    let missing_key = Path::new(vec![
        PathStep::GetAttr("tags".to_string()),
        PathStep::Key("not_found".encode_value()),
    ]);
    assert!(
        root_val
            .mark_path(&missing_key, ValueMark::Sensitive)
            .is_err()
    );
    assert!(root_val.unmark_path(&missing_key).is_err());
    let key_on_non_obj = Path::new(vec![
        PathStep::GetAttr("servers".to_string()),
        PathStep::Key("any_key".encode_value()),
    ]);
    assert!(
        root_val
            .mark_path(&key_on_non_obj, ValueMark::Sensitive)
            .is_err()
    );
    assert!(root_val.unmark_path(&key_on_non_obj).is_err());
}
/// Tests edge cases, error paths, and remaining branches for `Value`.
#[test]
fn test_value_coverage_exhaustive_gaps() {
    let eq_fn: crate::types::ty::CapsuleEqualsFn = std::sync::Arc::new(|_, _| true);
    let hash_fn: crate::types::ty::CapsuleHashFn = std::sync::Arc::new(|_| 1);
    let _ = eq_fn(&(), &());
    let _ = hash_fn(&());
    let ops_with_ok_cmp = std::sync::Arc::new(
        crate::types::ty::CapsuleOps::new("cap_test", eq_fn.clone(), hash_fn.clone()).with_cmp(
            std::sync::Arc::new(|first, second| {
                let x = match first.downcast_ref::<i32>() {
                    Some(&v) => v,
                    None => 0,
                };
                let y = match second.downcast_ref::<i32>() {
                    Some(&v) => v,
                    None => 0,
                };
                Ok(x.cmp(&y))
            }),
        ),
    );
    let cap_ok1 = Value::capsule_with_ops("cap_test", ops_with_ok_cmp.clone(), 10_i32);
    let cap_ok2 = Value::capsule_with_ops("cap_test", ops_with_ok_cmp.clone(), 20_i32);
    assert_eq!(cap_ok1.cmp(&cap_ok2), std::cmp::Ordering::Less);
    let cap_str = Value::capsule_with_ops("cap_test", ops_with_ok_cmp, "not_i32".to_string());
    assert_eq!(cap_ok1.cmp(&cap_str), std::cmp::Ordering::Greater);
    assert_eq!(cap_str.cmp(&cap_ok1), std::cmp::Ordering::Less);
    let ops_with_err_cmp = std::sync::Arc::new(
        crate::types::ty::CapsuleOps::new("cap_test", eq_fn.clone(), hash_fn.clone()).with_cmp(
            std::sync::Arc::new(|_, _| Err(HclError::Eval("cmp error".to_string()))),
        ),
    );
    let cap_err1 = Value::capsule_with_ops("cap_test", ops_with_err_cmp.clone(), 10_i32);
    let cap_err2 = Value::capsule_with_ops("cap_test", ops_with_err_cmp, 20_i32);
    assert_eq!(cap_err1.cmp(&cap_err2), std::cmp::Ordering::Equal);
    let ops_no_cmp = std::sync::Arc::new(crate::types::ty::CapsuleOps::new(
        "cap_test", eq_fn, hash_fn,
    ));
    let cap_none1 = Value::capsule_with_ops("cap_test", ops_no_cmp.clone(), 10_i32);
    let cap_none2 = Value::capsule_with_ops("cap_test", ops_no_cmp, 20_i32);
    assert_eq!(cap_none1.cmp(&cap_none2), std::cmp::Ordering::Equal);
    let cap_raw1 = Value::new(
        Type::Capsule {
            name: "raw_test",
            type_id: std::any::TypeId::of::<i32>(),
            ops: None,
        },
        ValueData::Capsule(std::sync::Arc::new(10_i32)),
    );
    let cap_raw2 = Value::new(
        Type::Capsule {
            name: "raw_test",
            type_id: std::any::TypeId::of::<i32>(),
            ops: None,
        },
        ValueData::Capsule(std::sync::Arc::new(20_i32)),
    );
    assert_eq!(cap_raw1.cmp(&cap_raw2), std::cmp::Ordering::Equal);
}

#[test]
fn test_value_mark_path_steps_coverage() -> Result<(), HclError> {
    use crate::encode::EncodeValue;
    use std::collections::BTreeMap;
    let base_arr = Value::new(
        Type::List(Box::new(Type::Number)),
        ValueData::Array(vec![100_i64.encode_value()]),
    );
    let neg_idx = &[PathStep::Index((-1_i64).encode_value())];
    assert!(
        base_arr
            .mark_path_steps(neg_idx, ValueMark::Sensitive, &Path::empty())
            .is_err()
    );
    let str_idx = &[PathStep::Index("invalid".encode_value())];
    assert!(
        base_arr
            .mark_path_steps(str_idx, ValueMark::Sensitive, &Path::empty())
            .is_err()
    );
    let num_key = &[PathStep::Key(42_i64.encode_value())];
    let mut obj_fields = BTreeMap::new();
    obj_fields.insert("k".to_string(), 1_i64.encode_value());
    let base_obj = Value::new(Type::object(BTreeMap::new()), ValueData::Object(obj_fields));
    assert!(
        base_obj
            .mark_path_steps(num_key, ValueMark::Sensitive, &Path::empty())
            .is_err()
    );
    let single_idx = &[PathStep::Index(0_i64.encode_value())];
    let plain_val = "string_not_array".encode_value();
    assert!(
        plain_val
            .mark_path_steps(single_idx, ValueMark::Sensitive, &Path::empty())
            .is_err()
    );
    let single_key = &[PathStep::Key("k".encode_value())];
    assert!(
        plain_val
            .mark_path_steps(single_key, ValueMark::Sensitive, &Path::empty())
            .is_err()
    );
    let recurse_idx_err = &[
        PathStep::Index(0_i64.encode_value()),
        PathStep::Index((-1_i64).encode_value()),
    ];
    assert!(
        base_arr
            .mark_path_steps(recurse_idx_err, ValueMark::Sensitive, &Path::empty())
            .is_err()
    );
    let recurse_key_err = &[
        PathStep::Key("k".encode_value()),
        PathStep::Index((-1_i64).encode_value()),
    ];
    assert!(
        base_obj
            .mark_path_steps(recurse_key_err, ValueMark::Sensitive, &Path::empty())
            .is_err()
    );
    assert!(base_arr.unmark_path_steps(neg_idx, &Path::empty()).is_err());
    assert!(base_arr.unmark_path_steps(str_idx, &Path::empty()).is_err());
    assert!(
        base_arr
            .unmark_path_steps(&[PathStep::Index(99_i64.encode_value())], &Path::empty())
            .is_err()
    );
    assert!(base_obj.unmark_path_steps(num_key, &Path::empty()).is_err());
    assert!(
        plain_val
            .unmark_path_steps(single_idx, &Path::empty())
            .is_err()
    );
    assert!(
        plain_val
            .unmark_path_steps(single_key, &Path::empty())
            .is_err()
    );
    assert!(
        base_arr
            .unmark_path_steps(recurse_idx_err, &Path::empty())
            .is_err()
    );
    assert!(
        base_obj
            .unmark_path_steps(recurse_key_err, &Path::empty())
            .is_err()
    );
    let (unmarked_arr_step, _) =
        base_arr.unmark_path_steps(&[PathStep::Index(0_i64.encode_value())], &Path::empty())?;
    assert_eq!(unmarked_arr_step, base_arr);
    let (unmarked_obj_step, _) =
        base_obj.unmark_path_steps(&[PathStep::Key("k".encode_value())], &Path::empty())?;
    assert_eq!(unmarked_obj_step, base_obj);
    Ok(())
}

#[test]
fn test_value_coverage_exhaustive_gaps_part_2() {
    use crate::encode::EncodeValue;
    use std::collections::{BTreeMap, BTreeSet};
    let leaf_marked = "secret".encode_value().mark(ValueMark::Sensitive);
    let mut inner_obj = BTreeMap::new();
    inner_obj.insert("unmarked_field".to_string(), 10_i64.encode_value());
    inner_obj.insert("prop".to_string(), leaf_marked);
    let mid_obj_val = Value::new(Type::object(BTreeMap::new()), ValueData::Object(inner_obj));
    let parent_arr = Value::new(
        Type::List(Box::new(Type::Dynamic)),
        ValueData::Array(vec![1_i64.encode_value(), mid_obj_val]),
    );
    assert!(!parent_arr.has_mark(&ValueMark::Sensitive));
    assert!(parent_arr.has_marked_children());
    let parent_set = Value::new(
        Type::Set(Box::new(parent_arr.ty().clone())),
        ValueData::Set(BTreeSet::from([2_i64.encode_value(), parent_arr])),
    );
    assert!(parent_set.has_marked_children());
    let val_a = 1_i64.encode_value();
    let val_b = 2_i64.encode_value();
    assert_eq!(val_a.partial_cmp(&val_b), Some(std::cmp::Ordering::Less));
    assert!(val_a < val_b);
    assert_eq!(val_a.cmp(&val_b), std::cmp::Ordering::Less);
    let mut v_clone = 10_i64.encode_value();
    v_clone.clone_from(&val_b);
    assert_eq!(v_clone, val_b);
    let null_dbg = format!("{:?}", ValueData::Null);
    assert!(!null_dbg.is_empty());
    assert_eq!(format!("{}", ValueMark::Sensitive), "sensitive");
    assert_eq!(
        format!("{}", 42_i64.encode_value().mark(ValueMark::Sensitive)),
        "(sensitive value)"
    );
    assert!(
        42_i64
            .encode_value()
            .mark(ValueMark::Sensitive)
            .is_sensitive()
    );
}

#[test]
fn test_value_coverage_exhaustive_gaps_part_2b() -> Result<(), HclError> {
    use crate::encode::EncodeValue;
    use std::collections::{BTreeMap, BTreeSet};
    let eq_fn: crate::types::ty::CapsuleEqualsFn = std::sync::Arc::new(|_, _| true);
    let hash_fn: crate::types::ty::CapsuleHashFn = std::sync::Arc::new(|_| 1);
    let ops_no_cmp = std::sync::Arc::new(crate::types::ty::CapsuleOps::new(
        "cap_test", eq_fn, hash_fn,
    ));
    let cap_ok1 = Value::capsule_with_ops("cap_test", ops_no_cmp, 10_i32);
    let multi_arr_disp = Value::new(
        Type::List(Box::new(Type::Number)),
        ValueData::Array(vec![1_i64.encode_value(), 2_i64.encode_value()]),
    );
    assert_eq!(format!("{multi_arr_disp}"), "[1, 2]");
    let multi_set_disp = Value::new(
        Type::Set(Box::new(Type::Number)),
        ValueData::Set(BTreeSet::from([1_i64.encode_value(), 2_i64.encode_value()])),
    );
    assert_eq!(format!("{multi_set_disp}"), "[1, 2]");
    let mut multi_obj_fields = BTreeMap::new();
    multi_obj_fields.insert("a".to_string(), 1_i64.encode_value());
    multi_obj_fields.insert("b".to_string(), 2_i64.encode_value());
    let multi_obj_disp = Value::new(
        Type::object(BTreeMap::new()),
        ValueData::Object(multi_obj_fields),
    );
    let multi_obj_str = format!("{multi_obj_disp}");
    assert!(multi_obj_str.contains("\"a\" = 1"));
    let marked_set = BTreeSet::from([ValueMark::Sensitive]);
    let val_with_m = 42_i64.encode_value().with_marks(marked_set);
    assert!(val_with_m.has_mark(&ValueMark::Sensitive));
    assert_ne!(val_with_m, 42_i64.encode_value());
    let (unmarked_deep, removed_deep) = val_with_m.unmark_deep();
    assert!(!unmarked_deep.has_mark(&ValueMark::Sensitive));
    assert_eq!(removed_deep.len(), 1);
    let unmarked_sens = val_with_m.unmark_sensitive();
    assert!(!unmarked_sens.has_mark(&ValueMark::Sensitive));
    assert!(val_with_m.refinement().is_none());
    assert!(val_with_m.refinement_arc().is_none());
    let msgpack_bytes = val_with_m.to_msgpack()?;
    let deserialized_val = Value::from_msgpack(&msgpack_bytes, val_with_m.ty())?;
    assert_eq!(deserialized_val, val_with_m);
    assert!(val_with_m.as_capsule_any().is_none());
    assert!(cap_ok1.as_capsule::<String>().is_err());
    let directly_marked_arr = Value::new(
        Type::List(Box::new(Type::Number)),
        ValueData::Array(vec![1_i64.encode_value().mark(ValueMark::Sensitive)]),
    );
    assert!(directly_marked_arr.has_marked_children());
    Ok(())
}

#[test]
fn test_value_coverage_exhaustive_gaps_part_3() {
    use crate::encode::EncodeValue;
    let eq_fn: crate::types::ty::CapsuleEqualsFn = std::sync::Arc::new(|_, _| true);
    let hash_fn: crate::types::ty::CapsuleHashFn = std::sync::Arc::new(|_| 1);
    let ops_no_cmp = std::sync::Arc::new(crate::types::ty::CapsuleOps::new(
        "cap_test",
        eq_fn.clone(),
        hash_fn.clone(),
    ));
    let cap_conv_ops = std::sync::Arc::new(
        crate::types::ty::CapsuleOps::new("test_conv_capsule", eq_fn, hash_fn)
            .with_conversion_to(std::sync::Arc::new(|_, target| {
                if target == &Type::String {
                    Some("converted_str".encode_value())
                } else {
                    None
                }
            }))
            .with_conversion_from(std::sync::Arc::new(|val| {
                if let ValueData::String(ref s) = *val.data {
                    if s == "valid_token" {
                        Some(std::sync::Arc::new(777_i32))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })),
    );
    let cap_val_conv = Value::capsule_with_ops("test_conv_capsule", cap_conv_ops, 123_i32);
    assert!(cap_val_conv.coerce(&Type::String).is_ok());
    assert!(cap_val_conv.coerce(&Type::Number).is_err());
    assert!(
        "valid_token"
            .encode_value()
            .coerce(cap_val_conv.ty())
            .is_ok()
    );
    assert!(
        "invalid_token"
            .encode_value()
            .coerce(cap_val_conv.ty())
            .is_err()
    );
    assert!(100_i64.encode_value().coerce(cap_val_conv.ty()).is_err());
    let target_no_conv = Type::capsule_with_ops::<i32>("cap_no_conv", ops_no_cmp);
    assert!(10_i64.encode_value().coerce(&target_no_conv).is_err());
    let unk_str_ref = Value::unknown_refined(
        Type::String,
        crate::types::refinement::Refinement::new().with_prefix("prefix"),
    );
    assert!(unk_str_ref.coerce(&Type::Bool).is_ok());
}
/// Tests comprehensive branch, region, and line coverage for `walk_internal` and `transform_internal`
/// across all structural variants (primitive, array, object, set) and error/pruning paths.
#[test]
fn test_walk_transform_instantiation_full_coverage() {
    use crate::encode::EncodeValue;
    use crate::types::path::Path;
    let val_prim = 42_i64.encode_value();
    let val_arr = Value::new(
        Type::List(Box::new(Type::Number)),
        ValueData::Array(vec![1_i64.encode_value(), 2_i64.encode_value()]),
    );
    let mut obj_map = BTreeMap::new();
    obj_map.insert("key".to_string(), 10_i64.encode_value());
    let val_obj = Value::new(Type::Dynamic, ValueData::Object(obj_map));
    let mut set_data = std::collections::BTreeSet::new();
    set_data.insert(100_i64.encode_value());
    let val_set = Value::new(Type::Set(Box::new(Type::Number)), ValueData::Set(set_data));
    let val_err_arr = Value::new(
        Type::List(Box::new(Type::String)),
        ValueData::Array(vec!["trigger_err".encode_value()]),
    );
    let mut err_obj_map = BTreeMap::new();
    err_obj_map.insert("sub".to_string(), "trigger_err".encode_value());
    let val_err_obj = Value::new(Type::Dynamic, ValueData::Object(err_obj_map));
    let mut err_set_data = std::collections::BTreeSet::new();
    err_set_data.insert("trigger_err".encode_value());
    let val_err_set = Value::new(
        Type::Set(Box::new(Type::String)),
        ValueData::Set(err_set_data),
    );
    let val_prune = Value::new(
        Type::List(Box::new(Type::String)),
        ValueData::Array(vec!["prune".encode_value()]),
    );
    let mut visitor = |_path: &Path, val: &Value| -> Result<bool, HclError> {
        match &*val.data {
            ValueData::String(s) if s == "trigger_err" => {
                Err(HclError::Type("intentional_walk_err".into()))
            }
            ValueData::String(s) if s == "prune" => Ok(false),
            _ => Ok(true),
        }
    };
    assert!(val_prim.walk(&mut visitor).is_ok());
    assert!("normal_str".encode_value().walk(&mut visitor).is_ok());
    assert!(val_prune.walk(&mut visitor).is_ok());
    assert!(val_arr.walk(&mut visitor).is_ok());
    assert!(val_err_arr.walk(&mut visitor).is_err());
    assert!(val_obj.walk(&mut visitor).is_ok());
    assert!(val_err_obj.walk(&mut visitor).is_err());
    assert!(val_set.walk(&mut visitor).is_ok());
    assert!(val_err_set.walk(&mut visitor).is_err());
    assert!("trigger_err".encode_value().walk(&mut visitor).is_err());
    let mut transformer = |_path: &Path, val: &Value| -> Result<Value, HclError> {
        match &*val.data {
            ValueData::String(s) if s == "trigger_err" => {
                Err(HclError::Type("intentional_transform_err".into()))
            }
            _ => Ok(val.clone()),
        }
    };
    assert!(val_prim.transform(&mut transformer).is_ok());
    assert!(
        "normal_str"
            .encode_value()
            .transform(&mut transformer)
            .is_ok()
    );
    assert!(val_arr.transform(&mut transformer).is_ok());
    assert!(val_err_arr.transform(&mut transformer).is_err());
    assert!(val_obj.transform(&mut transformer).is_ok());
    assert!(val_err_obj.transform(&mut transformer).is_err());
    assert!(val_set.transform(&mut transformer).is_ok());
    assert!(val_err_set.transform(&mut transformer).is_err());
    assert!(
        "trigger_err"
            .encode_value()
            .transform(&mut transformer)
            .is_err()
    );
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct EphemeralMark {
    generation: u32,
}
impl std::fmt::Display for EphemeralMark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ephemeral:{}", self.generation)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TaintMark;
impl std::fmt::Display for TaintMark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tainted")
    }
}
#[derive(Clone, Debug)]
struct SameDisplayDiffEq {
    name: String,
    id: u32,
}
impl PartialEq for SameDisplayDiffEq {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for SameDisplayDiffEq {}
impl std::hash::Hash for SameDisplayDiffEq {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
impl std::fmt::Display for SameDisplayDiffEq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
#[test]
fn test_typed_value_marks() {
    let val = Value::new(Type::String, ValueData::String("secure_data".into()));
    assert!(!val.has_mark_type::<EphemeralMark>());
    assert!(val.get_mark::<EphemeralMark>().is_none());
    let marked_eph = val.mark_typed(EphemeralMark { generation: 42 });
    assert!(marked_eph.has_mark_type::<EphemeralMark>());
    assert_eq!(
        marked_eph.get_mark::<EphemeralMark>(),
        Some(&EphemeralMark { generation: 42 })
    );
    assert!(!marked_eph.has_mark_type::<TaintMark>());
    assert!(marked_eph.get_mark::<TaintMark>().is_none());
    let custom_tm = ValueMark::typed(TaintMark);
    assert_eq!(format!("{custom_tm}"), "tainted");
    let marked_taint_only = val.mark(custom_tm.clone());
    assert!(marked_taint_only.get_mark::<EphemeralMark>().is_none());
    let marked_both = marked_eph.mark(custom_tm.clone());
    assert!(marked_both.has_mark_type::<EphemeralMark>());
    assert!(marked_both.has_mark_type::<TaintMark>());
    assert!(marked_both.has_mark(&custom_tm));
    let sensitive_val = val
        .mark(ValueMark::Sensitive)
        .mark(ValueMark::custom("tag"));
    assert!(sensitive_val.get_mark::<EphemeralMark>().is_none());
    assert!(!sensitive_val.has_mark_type::<EphemeralMark>());
    let (unmarked, removed) = marked_both.unmark();
    assert!(!unmarked.has_mark_type::<EphemeralMark>());
    assert!(!unmarked.has_mark_type::<TaintMark>());
    assert_eq!(removed.len(), 2);
}

#[test]
fn test_typed_value_marks_traits() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let tm1 = TypedMark::new(EphemeralMark { generation: 1 });
    let tm2 = TypedMark::new(EphemeralMark { generation: 1 });
    let tm3 = TypedMark::new(EphemeralMark { generation: 2 });
    let tm_other = TypedMark::new(TaintMark);
    assert_eq!(tm1, tm2);
    assert_ne!(tm1, tm3);
    assert_ne!(tm1, tm_other);
    assert_eq!(format!("{tm1}"), "ephemeral:1");
    let tm1_dbg = format!("{tm1:?}");
    assert!(tm1_dbg.contains("EphemeralMark"));
    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    tm1.hash(&mut h1);
    tm2.hash(&mut h2);
    assert_eq!(h1.finish(), h2.finish());
    let mut h_other = DefaultHasher::new();
    tm_other.hash(&mut h_other);
    assert_ne!(h_other.finish(), 0);
    let any_mark: Box<dyn AnyValueMark> = Box::new(TaintMark);
    let mut h_any = DefaultHasher::new();
    any_mark.dyn_hash(&mut h_any);
    assert_eq!(any_mark.mark_name(), std::any::type_name::<TaintMark>());
    assert!(any_mark.dyn_eq(&TaintMark));
    assert!(!any_mark.dyn_eq(&EphemeralMark { generation: 1 }));
}

#[test]
fn test_typed_value_marks_ordering_and_hash() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let tm1 = TypedMark::new(EphemeralMark { generation: 1 });
    let tm2 = TypedMark::new(EphemeralMark { generation: 1 });
    let tm3 = TypedMark::new(EphemeralMark { generation: 2 });
    let m1 = TypedMark::new(SameDisplayDiffEq {
        name: "same".into(),
        id: 1,
    });
    let m2 = TypedMark::new(SameDisplayDiffEq {
        name: "same".into(),
        id: 2,
    });
    assert_ne!(m1, m2);
    assert_ne!(m1.cmp(&m2), std::cmp::Ordering::Equal);
    let mut h_m1 = DefaultHasher::new();
    m1.hash(&mut h_m1);
    let mut h_m2 = DefaultHasher::new();
    m2.hash(&mut h_m2);
    assert_ne!(h_m1.finish(), h_m2.finish());
    assert_eq!(tm1.partial_cmp(&tm2), Some(std::cmp::Ordering::Equal));
    assert_eq!(tm1.cmp(&tm3), std::cmp::Ordering::Less);
    assert!(tm1 < tm3);
    assert!(tm1.cmp(&tm2).is_eq());
}
#[derive(Debug, PartialEq, Eq)]
struct Counter(i32);
#[test]
fn test_capsule_methods() -> Result<(), crate::error::HclError> {
    let eq_fn: crate::types::ty::CapsuleEqualsFn = std::sync::Arc::new(|a, b| {
        let c1 = a.downcast_ref::<Counter>();
        let c2 = b.downcast_ref::<Counter>();
        c1.is_some() && c1 == c2
    });
    let hash_fn: crate::types::ty::CapsuleHashFn = std::sync::Arc::new(|a| {
        a.downcast_ref::<Counter>()
            .map_or(0, |c| u64::try_from(c.0).unwrap_or(0))
    });
    let mut ops = crate::types::ty::CapsuleOps::new("Counter", eq_fn, hash_fn);
    let inc_method: crate::types::ty::CapsuleMethodFn = std::sync::Arc::new(|any, args| {
        let c = any
            .downcast_ref::<Counter>()
            .ok_or_else(|| crate::error::HclError::Capsule("invalid counter".into()))?;
        let delta = if args.is_empty() {
            1
        } else {
            match &*args[0].data {
                ValueData::Number(n) => {
                    use bigdecimal::num_traits::ToPrimitive;
                    n.0.to_i32()
                        .ok_or_else(|| crate::error::HclError::Capsule("overflow".into()))?
                }
                _ => {
                    return Err(crate::error::HclError::Capsule("expected number".into()));
                }
            }
        };
        Ok(Value::new(
            Type::Number,
            ValueData::Number((c.0 + delta).into()),
        ))
    });
    ops = ops.with_method("increment", inc_method);
    let c1 = Counter(10);
    let c2 = Counter(10);
    let c3 = Counter(20);
    assert!((ops.equals)(&c1, &c2));
    assert!(!(ops.equals)(&c1, &c3));
    assert_eq!((ops.hash)(&c1), (ops.hash)(&c2));
    let ops_arc = std::sync::Arc::new(ops);
    let cap_ty = Type::capsule_with_ops::<Counter>("Counter", ops_arc.clone());
    let cap_val = Value::new(cap_ty, ValueData::Capsule(std::sync::Arc::new(Counter(10))));
    let delta_val = Value::new(Type::Number, ValueData::Number(5.into()));
    let res = cap_val.call_method("increment", &[delta_val])?;
    assert_eq!(res.to_string(), "15");
    let unk_arg = Value::unknown(Type::Number);
    let unk_res = cap_val.call_method("increment", &[unk_arg])?;
    assert!(unk_res.is_unknown());
    let marked_cap = cap_val.mark(ValueMark::Sensitive);
    let marked_res = marked_cap.call_method("increment", &[])?;
    assert!(marked_res.is_sensitive());
    assert_eq!(marked_res.to_string(), "(sensitive value)");
    assert_eq!(marked_res.unmark().0.to_string(), "11");
    let err = cap_val
        .call_method("decrement", &[])
        .err()
        .ok_or(crate::error::HclError::Eval("expected error".into()))?;
    assert_eq!(
        err,
        crate::error::HclError::CapsuleMethodNotFound {
            capsule_type: "Counter",
            method: "decrement".to_string(),
        }
    );
    let str_val = Value::new(Type::String, ValueData::String("text".into()));
    assert!(str_val.call_method("trim", &[]).is_err());
    let no_ops_ty = Type::capsule::<Counter>("Counter");
    let no_ops_val = Value::new(
        no_ops_ty,
        ValueData::Capsule(std::sync::Arc::new(Counter(10))),
    );
    assert!(no_ops_val.call_method("increment", &[]).is_err());
    let str_arg = Value::new(Type::String, ValueData::String("not_a_num".into()));
    let err_bad_arg = cap_val.call_method("increment", &[str_arg]);
    assert!(err_bad_arg.is_err());
    let huge_num = Value::new(
        Type::Number,
        ValueData::Number(
            crate::number::Number::from(i64::MAX) * crate::number::Number::from(i64::MAX),
        ),
    );
    let err_overflow = cap_val.call_method("increment", &[huge_num]);
    assert!(err_overflow.is_err());
    let bad_any = "not a counter";
    assert!((ops_arc.methods["increment"])(&bad_any, &[]).is_err());
    Ok(())
}
