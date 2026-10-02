//! Schema Decoding Engine (`hcldec`).
//!
//! Provides the ability to decode an HCL `Body` into a strongly-typed `Value`
//! based on a given `Spec`.
use std::fmt::Write;

use crate::ast::structure::Body;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::error::HclError;
use crate::eval::context::Context;
use crate::eval::evaluator::Evaluator;
use crate::hcldec::spec::{BlockListSpec, Spec};
use crate::types::ty::Type;
use crate::types::val::{Value, ValueData};
use std::collections::BTreeMap;

/// Decodes an HCL body according to the given specification.
///
/// # Errors
///
/// Returns an `HclError::MissingField` or other decoding errors if the body
/// does not conform to the specification.
pub fn decode(body: &Body, spec: &Spec) -> Result<Value, HclError> {
    let ctx = Context::new();
    decode_internal(body, spec, &ctx)
}

/// Decodes an HCL body according to the given specification using a provided context.
///
/// # Errors
///
/// Returns an `HclError` if decoding fails.
pub fn decode_with_context(body: &Body, spec: &Spec, ctx: &Context) -> Result<Value, HclError> {
    decode_internal(body, spec, ctx)
}

/// Decodes an HCL body according to the given specification, returning both the
/// decoded value and a residual [`Body`] containing all unmatched attributes and blocks.
///
/// # Arguments
/// * `body` - The input HCL body.
/// * `spec` - The dynamic schema specification to decode.
/// * `ctx` - The evaluation context.
///
/// # Errors
///
/// Returns [`Diagnostics`] if decoding fails or required elements are missing.
pub fn partial_decode(
    body: &Body,
    spec: &Spec,
    ctx: &mut Context,
) -> Result<(Value, Body), Diagnostics> {
    let mut residual = body.clone();
    match decode_partial_internal(&mut residual, spec, ctx) {
        Ok(val) => Ok((val, residual)),
        Err(err) => Err(Diagnostics::from(Diagnostic::new(err, body.span.clone()))),
    }
}

fn decode_internal(body: &Body, spec: &Spec, ctx: &Context) -> Result<Value, HclError> {
    match spec {
        Spec::Object(map) => {
            let mut result = BTreeMap::new();
            let mut type_map = BTreeMap::new();
            for (key, inner_spec) in map {
                let val = decode_internal(body, inner_spec, ctx)?;
                type_map.insert(key.clone(), val.ty().clone());
                result.insert(key.clone(), val);
            }
            Ok(Value::new(
                Type::object(type_map),
                ValueData::Object(result),
            ))
        }
        Spec::Array(inner) => match &**inner {
            Spec::Block(block_spec) => {
                let mut vals = Vec::new();
                let mut types = Vec::new();
                for block in &body.blocks {
                    if block.block_type == block_spec.type_name {
                        let val = decode_internal(&block.body, &block_spec.body, ctx)?;
                        let t = val.ty().clone();
                        types.push(t);
                        vals.push(val);
                    }
                }
                Ok(Value::new(Type::Tuple(types), ValueData::Array(vals)))
            }
            _ => Err(HclError::Decode(
                "Array spec can only be used with Block specs in this implementation".to_string(),
            )),
        },
        Spec::Block(block_spec) => {
            let mut found = None;
            for block in &body.blocks {
                if block.block_type == block_spec.type_name {
                    if found.is_some() {
                        return Err(HclError::Decode(format!(
                            "Multiple blocks of type '{}' found, but only one expected",
                            block_spec.type_name
                        )));
                    }
                    found = Some(block);
                }
            }
            match found {
                Some(block) => decode_internal(&block.body, &block_spec.body, ctx),
                None => Err(HclError::MissingField(block_spec.type_name.clone())),
            }
        }
        Spec::BlockList(list_spec) => {
            let mut matching_blocks = Vec::new();
            for block in &body.blocks {
                if block.block_type == list_spec.type_name {
                    matching_blocks.push(block);
                }
            }

            if let Some(min) = list_spec.min_items
                && matching_blocks.len() < min
            {
                return Err(HclError::Decode(format!(
                    "Expected at least {min} blocks of type '{}', found {}",
                    list_spec.type_name,
                    matching_blocks.len()
                )));
            }

            if let Some(max) = list_spec.max_items
                && matching_blocks.len() > max
            {
                return Err(HclError::Decode(format!(
                    "Expected at most {max} blocks of type '{}', found {}",
                    list_spec.type_name,
                    matching_blocks.len()
                )));
            }

            let mut vals = Vec::new();
            let mut types = Vec::new();
            for block in matching_blocks {
                let val = decode_internal(&block.body, &list_spec.nested, ctx)?;
                types.push(val.ty().clone());
                vals.push(val);
            }
            Ok(Value::new(Type::Tuple(types), ValueData::Array(vals)))
        }
        Spec::BlockSet(set_spec) => {
            let mut set = std::collections::BTreeSet::new();
            let mut elem_type = Type::Dynamic;
            for block in &body.blocks {
                if block.block_type == set_spec.type_name {
                    let val = decode_internal(&block.body, &set_spec.nested, ctx)?;
                    elem_type = val.ty().clone();
                    set.insert(val);
                }
            }
            Ok(Value::new(
                Type::Set(Box::new(elem_type)),
                ValueData::Set(set),
            ))
        }
        Spec::BlockMap(map_spec) => build_block_map(body, map_spec, ctx),
        Spec::BlockAttrs(attrs_spec) => {
            let mut found = None;
            for block in &body.blocks {
                if block.block_type == attrs_spec.type_name {
                    if found.is_some() {
                        return Err(HclError::Decode(format!(
                            "Multiple blocks of type '{}' found for BlockAttrs, but only one expected",
                            attrs_spec.type_name
                        )));
                    }
                    found = Some(block);
                }
            }
            let block =
                found.ok_or_else(|| HclError::MissingField(attrs_spec.type_name.clone()))?;
            let mut result = BTreeMap::new();
            let mut type_map = BTreeMap::new();

            for (name, attr) in &block.body.attributes {
                let (val, _) = Evaluator::new(ctx)
                    .evaluate(&attr.expr)
                    .map_err(|diags| HclError::Eval(format!("{diags:?}")))?;
                if let Some(expected_ty) = &attrs_spec.expected_type
                    && val.ty() != expected_ty
                {
                    return Err(HclError::Type(format!(
                        "Attribute '{}' expected type {}, got {}",
                        name,
                        expected_ty,
                        val.ty()
                    )));
                }

                type_map.insert(name.clone(), val.ty().clone());
                result.insert(name.clone(), val);
            }

            Ok(Value::new(
                Type::object(type_map),
                ValueData::Object(result),
            ))
        }
        Spec::Literal(lit_spec) => Ok(lit_spec.value.clone()),
        Spec::Transform(transform_spec) => {
            let inner_val = decode_internal(body, &transform_spec.spec, ctx)?;
            (transform_spec.func)(inner_val)
        }
        Spec::Tuple(tuple_spec) => {
            let mut vals = Vec::new();
            let mut types = Vec::new();
            for elem_spec in &tuple_spec.elements {
                let val = decode_internal(body, elem_spec, ctx)?;
                types.push(val.ty().clone());
                vals.push(val);
            }
            Ok(Value::new(Type::Tuple(types), ValueData::Array(vals)))
        }
        Spec::Attr(attr_spec) => {
            if let Some(attr) = body.attributes.get(&attr_spec.name) {
                let evaluator = Evaluator::new(ctx);
                let (val, _) = evaluator
                    .evaluate(&attr.expr)
                    .map_err(|diags| HclError::Eval(format!("{diags:?}")))?;
                let coerced = val
                    .coerce(&attr_spec.expected_type)
                    .map_err(HclError::Type)?;
                attr_spec.validate_value(&coerced, attr.span.clone())?;
                Ok(coerced)
            } else {
                let mut msg = attr_spec.name.clone();
                if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                    &attr_spec.name,
                    body.attributes.keys().map(String::as_str),
                    2,
                ) {
                    let _ = write!(msg, ". Did you mean \"{candidate}\"?");
                }
                Err(HclError::MissingField(msg))
            }
        }
        Spec::Default(default_spec) => match decode_internal(body, &default_spec.primary, ctx) {
            Ok(val) => Ok(val),
            Err(HclError::MissingField(_)) => Ok(default_spec.default_value.clone()),
            Err(e) => Err(e),
        },
        Spec::Required(inner) => decode_internal(body, inner, ctx),
        Spec::Optional(inner) => match decode_internal(body, inner, ctx) {
            Ok(val) => Ok(val),
            Err(HclError::MissingField(_)) => Ok(Value::new(Type::Dynamic, ValueData::Null)),
            Err(e) => Err(e),
        },
        Spec::Expr(expr_spec) => {
            let evaluator = Evaluator::new(ctx);
            let (val, _) = evaluator
                .evaluate(&expr_spec.expr)
                .map_err(|diags| HclError::Eval(format!("{diags:?}")))?;
            Ok(val)
        }
    }
}

fn decode_partial_internal(
    residual: &mut Body,
    spec: &Spec,
    ctx: &Context,
) -> Result<Value, HclError> {
    match spec {
        Spec::Object(map) => {
            let mut result = BTreeMap::new();
            let mut type_map = BTreeMap::new();
            for (key, inner_spec) in map {
                let val = decode_partial_internal(residual, inner_spec, ctx)?;
                type_map.insert(key.clone(), val.ty().clone());
                result.insert(key.clone(), val);
            }
            Ok(Value::new(
                Type::object(type_map),
                ValueData::Object(result),
            ))
        }
        Spec::Attr(attr_spec) => {
            if let Some(attr) = residual.attributes.remove(&attr_spec.name) {
                let eval_res = Evaluator::new(ctx).evaluate(&attr.expr);
                match eval_res {
                    Ok((val, _)) => {
                        let coerced = val
                            .coerce(&attr_spec.expected_type)
                            .map_err(HclError::Type)?;
                        attr_spec.validate_value(&coerced, attr.span.clone())?;
                        if coerced.contains_unknown() {
                            residual.attributes.insert(attr_spec.name.clone(), attr);
                        }
                        Ok(coerced)
                    }
                    Err(e) => {
                        residual.attributes.insert(attr_spec.name.clone(), attr);
                        Err(HclError::Eval(format!("{e:?}")))
                    }
                }
            } else {
                let mut msg = attr_spec.name.clone();
                if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                    &attr_spec.name,
                    residual.attributes.keys().map(String::as_str),
                    2,
                ) {
                    let _ = write!(msg, ". Did you mean \"{candidate}\"?");
                }
                Err(HclError::MissingField(msg))
            }
        }
        Spec::Block(block_spec) => {
            let mut found_idx = None;
            for (idx, block) in residual.blocks.iter().enumerate() {
                if block.block_type == block_spec.type_name {
                    if found_idx.is_some() {
                        return Err(HclError::Decode(format!(
                            "Multiple blocks of type '{}' found, but only one expected",
                            block_spec.type_name
                        )));
                    }
                    found_idx = Some(idx);
                }
            }
            match found_idx {
                Some(idx) => {
                    let mut block = residual.blocks.remove(idx);
                    let val = decode_partial_internal(&mut block.body, &block_spec.body, ctx)?;
                    if val.contains_unknown()
                        || !block.body.attributes.is_empty()
                        || !block.body.blocks.is_empty()
                    {
                        residual.blocks.push(block);
                    }
                    Ok(val)
                }
                None => Err(HclError::MissingField(block_spec.type_name.clone())),
            }
        }
        Spec::BlockList(list_spec) => {
            let mut matching = Vec::new();
            let mut non_matching = Vec::new();
            for block in residual.blocks.drain(..) {
                if block.block_type == list_spec.type_name {
                    matching.push(block);
                } else {
                    non_matching.push(block);
                }
            }
            residual.blocks = non_matching;

            if let Some(min) = list_spec.min_items
                && matching.len() < min
            {
                return Err(HclError::Decode(format!(
                    "Expected at least {min} blocks of type '{}', found {}",
                    list_spec.type_name,
                    matching.len()
                )));
            }

            if let Some(max) = list_spec.max_items
                && matching.len() > max
            {
                return Err(HclError::Decode(format!(
                    "Expected at most {max} blocks of type '{}', found {}",
                    list_spec.type_name,
                    matching.len()
                )));
            }

            let mut vals = Vec::new();
            let mut types = Vec::new();
            for mut block in matching {
                let val = decode_partial_internal(&mut block.body, &list_spec.nested, ctx)?;
                if val.contains_unknown()
                    || !block.body.attributes.is_empty()
                    || !block.body.blocks.is_empty()
                {
                    residual.blocks.push(block);
                }
                types.push(val.ty().clone());
                vals.push(val);
            }
            Ok(Value::new(Type::Tuple(types), ValueData::Array(vals)))
        }
        Spec::BlockSet(set_spec) => {
            let mut matching = Vec::new();
            let mut non_matching = Vec::new();
            for block in residual.blocks.drain(..) {
                if block.block_type == set_spec.type_name {
                    matching.push(block);
                } else {
                    non_matching.push(block);
                }
            }
            residual.blocks = non_matching;

            let mut set = std::collections::BTreeSet::new();
            let mut elem_type = Type::Dynamic;
            for mut block in matching {
                let val = decode_partial_internal(&mut block.body, &set_spec.nested, ctx)?;
                if val.contains_unknown()
                    || !block.body.attributes.is_empty()
                    || !block.body.blocks.is_empty()
                {
                    residual.blocks.push(block);
                }
                elem_type = val.ty().clone();
                set.insert(val);
            }
            Ok(Value::new(
                Type::Set(Box::new(elem_type)),
                ValueData::Set(set),
            ))
        }
        Spec::BlockMap(map_spec) => {
            let mut matching = Vec::new();
            let mut non_matching = Vec::new();
            for block in residual.blocks.drain(..) {
                if block.block_type == map_spec.type_name {
                    matching.push(block);
                } else {
                    non_matching.push(block);
                }
            }
            residual.blocks = non_matching;

            build_block_map_partial(residual, matching, map_spec, ctx)
        }
        Spec::BlockAttrs(attrs_spec) => {
            let mut found_idx = None;
            for (idx, block) in residual.blocks.iter().enumerate() {
                if block.block_type == attrs_spec.type_name {
                    if found_idx.is_some() {
                        return Err(HclError::Decode(format!(
                            "Multiple blocks of type '{}' found for BlockAttrs, but only one expected",
                            attrs_spec.type_name
                        )));
                    }
                    found_idx = Some(idx);
                }
            }
            match found_idx {
                Some(idx) => {
                    let block = residual.blocks.remove(idx);
                    let mut result = BTreeMap::new();
                    let mut type_map = BTreeMap::new();
                    for (name, attr) in &block.body.attributes {
                        let (val, _) = Evaluator::new(ctx)
                            .evaluate(&attr.expr)
                            .map_err(|diags| HclError::Eval(format!("{diags:?}")))?;
                        if let Some(expected_ty) = &attrs_spec.expected_type
                            && val.ty() != expected_ty
                        {
                            return Err(HclError::Type(format!(
                                "Attribute '{}' expected type {}, got {}",
                                name,
                                expected_ty,
                                val.ty()
                            )));
                        }
                        type_map.insert(name.clone(), val.ty().clone());
                        result.insert(name.clone(), val);
                    }
                    Ok(Value::new(
                        Type::object(type_map),
                        ValueData::Object(result),
                    ))
                }
                None => Err(HclError::MissingField(attrs_spec.type_name.clone())),
            }
        }
        Spec::Literal(l) => Ok(l.value.clone()),
        Spec::Expr(e) => {
            let (val, _) = Evaluator::new(ctx)
                .evaluate(&e.expr)
                .map_err(|diags| HclError::Eval(format!("{diags:?}")))?;
            Ok(val)
        }
        Spec::Transform(t) => {
            let val = decode_partial_internal(residual, &t.spec, ctx)?;
            (t.func)(val)
        }
        Spec::Default(d) => match decode_partial_internal(residual, &d.primary, ctx) {
            Ok(v) => Ok(v),
            Err(HclError::MissingField(_)) => Ok(d.default_value.clone()),
            Err(e) => Err(e),
        },
        Spec::Optional(inner) => match decode_partial_internal(residual, inner, ctx) {
            Ok(v) => Ok(v),
            Err(HclError::MissingField(_)) => {
                let implied = inner.implied_type().unwrap_or(Type::Dynamic);
                Ok(Value::null(implied))
            }
            Err(e) => Err(e),
        },
        Spec::Required(inner) => decode_partial_internal(residual, inner, ctx),
        Spec::Tuple(t) => {
            let mut vals = Vec::new();
            let mut types = Vec::new();
            for elem in &t.elements {
                let v = decode_partial_internal(residual, elem, ctx)?;
                types.push(v.ty().clone());
                vals.push(v);
            }
            Ok(Value::new(Type::Tuple(types), ValueData::Array(vals)))
        }
        Spec::Array(inner) => match &**inner {
            Spec::Block(b) => {
                let list_spec = BlockListSpec::new(b.type_name.clone(), *b.body.clone());
                decode_partial_internal(residual, &Spec::BlockList(list_spec), ctx)
            }
            _ => Err(HclError::Decode(
                "Array spec can only be used with Block specs in this implementation".to_string(),
            )),
        },
    }
}

fn build_block_map_partial(
    residual: &mut Body,
    blocks: Vec<crate::ast::structure::Block>,
    map_spec: &crate::hcldec::spec::BlockMapSpec,
    ctx: &Context,
) -> Result<Value, HclError> {
    for block in &blocks {
        if block.labels.len() != map_spec.labels.len() {
            return Err(HclError::Decode(format!(
                "Block '{}' expected {} labels, but found {}",
                map_spec.type_name,
                map_spec.labels.len(),
                block.labels.len()
            )));
        }
    }

    if map_spec.labels.len() == 1 {
        let mut obj = BTreeMap::new();
        let mut type_map = BTreeMap::new();
        for block in blocks {
            let key = block.labels[0].clone();
            let mut block_to_decode = block.clone();
            let val = if let Ok(v) =
                decode_partial_internal(&mut block_to_decode.body, &map_spec.nested, ctx)
            {
                if v.contains_unknown() {
                    residual.blocks.push(block);
                } else if !block_to_decode.body.attributes.is_empty()
                    || !block_to_decode.body.blocks.is_empty()
                {
                    residual.blocks.push(block_to_decode);
                }
                v
            } else {
                let implied = map_spec.nested.implied_type().unwrap_or(Type::Dynamic);
                residual.blocks.push(block);
                Value::unknown(implied)
            };
            type_map.insert(key.clone(), val.ty().clone());
            obj.insert(key, val);
        }
        Ok(Value::new(Type::object(type_map), ValueData::Object(obj)))
    } else {
        let mut temp_body = Body::new(residual.span.clone());
        temp_body.blocks = blocks;
        build_block_map(&temp_body, map_spec, ctx)
    }
}

fn build_block_map(
    body: &Body,
    map_spec: &crate::hcldec::spec::BlockMapSpec,
    ctx: &Context,
) -> Result<Value, HclError> {
    let mut matching_blocks = Vec::new();
    for block in &body.blocks {
        if block.block_type == map_spec.type_name {
            if block.labels.len() != map_spec.labels.len() {
                return Err(HclError::Decode(format!(
                    "Block '{}' expected {} labels, but found {}",
                    map_spec.type_name,
                    map_spec.labels.len(),
                    block.labels.len()
                )));
            }
            matching_blocks.push(block);
        }
    }

    if map_spec.labels.len() == 1 {
        let mut obj = BTreeMap::new();
        let mut type_map = BTreeMap::new();
        for block in matching_blocks {
            let key = block.labels[0].clone();
            let val = decode_internal(&block.body, &map_spec.nested, ctx)?;
            type_map.insert(key.clone(), val.ty().clone());
            obj.insert(key, val);
        }
        Ok(Value::new(Type::object(type_map), ValueData::Object(obj)))
    } else {
        build_nested_block_map(
            matching_blocks.as_slice(),
            &map_spec.nested,
            0,
            map_spec.labels.len(),
            ctx,
        )
    }
}

fn build_nested_block_map(
    blocks: &[&crate::ast::structure::Block],
    nested_spec: &Spec,
    label_idx: usize,
    total_labels: usize,
    ctx: &Context,
) -> Result<Value, HclError> {
    if label_idx + 1 == total_labels {
        let mut obj = BTreeMap::new();
        let mut type_map = BTreeMap::new();
        for block in blocks {
            let key = block.labels[label_idx].clone();
            let val = decode_internal(&block.body, nested_spec, ctx)?;
            type_map.insert(key.clone(), val.ty().clone());
            obj.insert(key, val);
        }
        Ok(Value::new(Type::object(type_map), ValueData::Object(obj)))
    } else {
        let mut groups: BTreeMap<String, Vec<&crate::ast::structure::Block>> = BTreeMap::new();
        for block in blocks {
            let key = block.labels[label_idx].clone();
            groups.entry(key).or_default().push(*block);
        }

        let mut obj = BTreeMap::new();
        let mut type_map = BTreeMap::new();
        for (key, group) in groups {
            let val =
                build_nested_block_map(&group, nested_spec, label_idx + 1, total_labels, ctx)?;
            type_map.insert(key.clone(), val.ty().clone());
            obj.insert(key, val);
        }
        Ok(Value::new(Type::object(type_map), ValueData::Object(obj)))
    }
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

    #[test]
    fn test_do_test_blocks_print() {
        let src = "foo {\n bar = \"a\"\n }\nfoo {\n bar = \"b\"\n }";
        let body = crate::api::parse(src).unwrap();
        println!("blocks {:?}", body.blocks);
    }

    use super::*;
    use crate::ast::structure::{Block, Body};
    use crate::eval::context::Context;
    use crate::hcldec::spec::{AttrSpec, BlockSpec, DefaultSpec};
    use crate::span::Span;
    fn empty_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }
    use std::collections::HashMap;

    #[test]
    fn test_decode_missing_attr() {
        let body = Body::new(empty_span());
        let spec = Spec::Attr(AttrSpec::new("missing", Type::String));
        let _ctx = Context::new();
        assert!(decode(&body, &spec).is_err());
    }

    #[test]
    fn test_decode_required_and_optional() {
        let hcl_src = r#"
            req = "a"
        "#;
        let body = crate::api::parse(hcl_src).unwrap();

        let mut attrs = HashMap::new();
        attrs.insert(
            "req".to_string(),
            Spec::Attr(AttrSpec::new("req", Type::String)),
        );
        let spec = Spec::Object(attrs);
        let _ctx = Context::new();
        let res = decode(&body, &spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*res.data),
            std::mem::discriminant(&ValueData::Object(BTreeMap::new()))
        );
    }

    #[test]
    fn test_decode_default() {
        let hcl_src = "";
        let body = crate::api::parse(hcl_src).unwrap();
        let spec = Spec::Default(DefaultSpec {
            primary: Box::new(Spec::Attr(AttrSpec::new("missing", Type::String))),
            default_value: Value::new(Type::String, ValueData::String("default".to_string())),
        });
        let _ctx = Context::new();
        let res = decode(&body, &spec).unwrap();
        assert_eq!(*res.data, ValueData::String("default".to_string()));
    }

    #[test]
    fn test_decode_array_of_blocks() {
        let mut body = Body::new(empty_span());
        let empty_body = Body::new(empty_span());
        let block1 = Block {
            block_type: "foo".to_string(),
            labels: vec![],
            body: empty_body.clone(),

            span: empty_span(),
            type_span: empty_span(),
            label_spans: vec![],
            open_brace_span: empty_span(),
            close_brace_span: empty_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        body.blocks.push(block1.clone());
        body.blocks.push(block1);

        let block_spec = Spec::Block(BlockSpec {
            type_name: "foo".to_string(),
            body: Box::new(Spec::Object(std::collections::HashMap::new())),
        });

        let spec = Spec::Array(Box::new(block_spec));
        let res = decode(&body, &spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*res.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );
    }

    #[test]
    fn test_decode_block() {
        let hcl_src = r#"
            foo { 
bar = "a"
 }
        "#;
        let body = crate::api::parse(hcl_src).unwrap();

        let block_body = HashMap::new();

        let spec = Spec::Block(BlockSpec {
            type_name: "foo".to_string(),
            body: Box::new(Spec::Object(block_body)),
        });

        let _ctx = Context::new();
        let res = decode(&body, &spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*res.data),
            std::mem::discriminant(&ValueData::Object(BTreeMap::new()))
        );
    }

    #[test]
    fn test_hcldec_coverage_eval_expr_error() {
        let hcl_src = r"
            foo = unknown_var
        ";
        let body = crate::api::parse(hcl_src).unwrap();
        let spec = Spec::Attr(AttrSpec::new("foo", Type::String));
        let _ctx = Context::new();
        let res = decode(&body, &spec);
        assert!(res.is_err());
    }

    #[test]
    fn test_hcldec_coverage_missing_block() {
        let body = crate::ast::structure::Body::new(crate::span::Span::new(0, 0, 0, 0, 0, 0));
        let ctx = Context::new();

        let empty_attrs = std::collections::HashMap::new();
        let spec_block = Spec::Block(crate::hcldec::spec::BlockSpec {
            type_name: "missing_block".to_string(),
            body: Box::new(Spec::Object(empty_attrs)),
        });
        assert!(decode_internal(&body, &spec_block, &ctx).is_err());

        let spec_req = Spec::Required(Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
            "missing_req",
            Type::String,
        ))));
        assert!(decode_internal(&body, &spec_req, &ctx).is_err());

        let spec_opt = Spec::Optional(Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
            "missing_opt",
            Type::String,
        ))));
        let res_opt = decode_internal(&body, &spec_opt, &ctx).unwrap();
        assert_eq!(*res_opt.ty(), Type::Dynamic);

        // We need a body with an attribute to test Optional/Default presence
        let mut body_with_attr =
            crate::ast::structure::Body::new(crate::span::Span::new(0, 0, 0, 0, 0, 0));
        body_with_attr.attributes.insert(
            "opt_attr".to_string(),
            crate::ast::structure::Attribute {
                name: "opt_attr".to_string(),
                expr: crate::ast::expr::Expression::String(
                    "found".to_string(),
                    crate::span::Span::new(0, 0, 0, 0, 0, 0),
                ),
                span: crate::span::Span::new(0, 0, 0, 0, 0, 0),
                name_span: crate::span::Span::new(0, 0, 0, 0, 0, 0),
                equals_span: crate::span::Span::new(0, 0, 0, 0, 0, 0),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );

        let spec_opt_present = Spec::Optional(Box::new(Spec::Attr(
            crate::hcldec::spec::AttrSpec::new("opt_attr", Type::String),
        )));
        assert!(decode_internal(&body_with_attr, &spec_opt_present, &ctx).is_ok());

        let spec_def_present = Spec::Default(crate::hcldec::spec::DefaultSpec {
            primary: Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                "opt_attr",
                Type::String,
            ))),
            default_value: Value::new(Type::String, ValueData::String("def".to_string())),
        });
        assert!(decode_internal(&body_with_attr, &spec_def_present, &ctx).is_ok());
    }

    #[test]
    fn test_decode_eval_error() {
        let hcl_src = r#"
            foo = 1 + "a"
        "#;
        let body = crate::api::parse(hcl_src).unwrap();
        let spec = Spec::Attr(AttrSpec::new("foo", Type::String));
        let _ctx = Context::new();
        let res = decode(&body, &spec);
        assert!(res.is_err());
    }

    #[test]
    fn test_hcldec_coverage_array_spec() {
        let hcl_src = "foo = 1";
        let body = crate::api::parse(hcl_src).unwrap();
        let spec = Spec::Array(Box::new(Spec::Attr(AttrSpec::new("foo", Type::String))));
        let _ctx = Context::new();
        let res = decode(&body, &spec);
        assert!(res.is_err());
    }

    #[test]
    fn test_decode_object_and_attr() {
        let hcl_src = "foo = 1";
        let body = crate::api::parse(hcl_src).unwrap();
        let mut attrs = HashMap::new();
        attrs.insert(
            "foo".to_string(),
            Spec::Attr(AttrSpec::new("foo", Type::String)),
        );
        let spec = Spec::Object(attrs);
        let _ctx = Context::new();
        let res = decode(&body, &spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*res.data),
            std::mem::discriminant(&ValueData::Object(BTreeMap::new()))
        );
    }

    #[test]
    fn test_hcldec_coverage_decode_internal_error() {
        let hcl_src = "foo = \"bad\"";
        let body = crate::api::parse(hcl_src).unwrap();
        let mut attrs = HashMap::new();
        attrs.insert(
            "foo".to_string(),
            Spec::Array(Box::new(Spec::Attr(AttrSpec::new("foo", Type::String)))),
        );
        let spec = Spec::Object(attrs);
        let _ctx = Context::new();
        let res = decode(&body, &spec);
        assert!(res.is_err());
    }

    #[test]
    fn test_hcldec_coverage_multiple_blocks_error() {
        let mut body = Body::new(empty_span());
        let empty_body = Body::new(empty_span());
        let block1 = Block {
            block_type: "foo".to_string(),
            labels: vec![],
            body: empty_body.clone(),

            span: empty_span(),
            type_span: empty_span(),
            label_spans: vec![],
            open_brace_span: empty_span(),
            close_brace_span: empty_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        body.blocks.push(block1.clone());
        body.blocks.push(block1);

        let block_spec = Spec::Block(BlockSpec {
            type_name: "foo".to_string(),
            body: Box::new(Spec::Object(std::collections::HashMap::new())),
        });
        let res = decode(&body, &block_spec);
        assert!(res.is_err());
    }
    #[test]
    fn test_decode_array_of_blocks_with_actual_block() {
        let hcl_src = r#"
            foo { 
bar = "a"
 }
        "#;
        let body = crate::api::parse(hcl_src).unwrap();

        let block_body = HashMap::new();

        let block_spec = Spec::Block(BlockSpec {
            type_name: "foo".to_string(),
            body: Box::new(Spec::Object(block_body)),
        });

        let spec = Spec::Array(Box::new(block_spec));
        let _ctx = Context::new();
        let res = decode(&body, &spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*res.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );
    }
    #[test]
    fn test_hcldec_more_coverage() {
        // Line 65 error trigger for Multiple Blocks formatting
        let mut body = Body::new(empty_span());
        let block1 = Block {
            block_type: "foo".to_string(),
            labels: vec![],
            body: Body::new(empty_span()),
            span: empty_span(),
            type_span: empty_span(),
            label_spans: vec![],
            open_brace_span: empty_span(),
            close_brace_span: empty_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        body.blocks.push(block1.clone());
        body.blocks.push(block1);

        let block_spec = Spec::Block(BlockSpec {
            type_name: "foo".to_string(),
            body: Box::new(Spec::Object(std::collections::HashMap::new())),
        });

        // This will return an Err(HclError::Decode("Multiple blocks of type 'foo' found..."))
        let err = decode(&body, &block_spec).err().unwrap();
        assert!(err.to_string().contains("foo")); // hits line 65

        // Line 95 error trigger for Default
        // We need an error that is NOT MissingField. E.g. Eval error.
        let hcl_src = "foo = 1 + \"a\"";
        let body_eval_err = crate::api::parse(hcl_src).unwrap();
        let default_spec = Spec::Default(DefaultSpec {
            primary: Box::new(Spec::Attr(AttrSpec::new("foo", Type::String))),
            default_value: Value::new(Type::String, ValueData::Null),
        });
        assert!(decode(&body_eval_err, &default_spec).is_err()); // hits line 95

        // Line 100 Optional Eval Error
        let opt_spec = Spec::Optional(Box::new(Spec::Attr(AttrSpec::new("foo", Type::String))));
        assert!(decode(&body_eval_err, &opt_spec).is_err()); // hits Optional Err(e) => Err(e) branch
    }

    #[test]
    fn test_decode_block_list_success_and_errors() {
        let src = "item {\n  val = 1\n}\nitem {\n  val = 2\n}\n";
        let body = crate::api::parse(src).unwrap();

        let mut attrs = HashMap::new();
        attrs.insert(
            "val".to_string(),
            Spec::Attr(AttrSpec::new("val", Type::Number)),
        );
        let nested = Spec::Object(attrs);

        let list_spec = Spec::BlockList(
            crate::hcldec::spec::BlockListSpec::new("item", nested.clone())
                .with_min_items(1)
                .with_max_items(3),
        );
        let val = decode(&body, &list_spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*val.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );

        // Min/max items None
        let list_spec_no_limits = Spec::BlockList(crate::hcldec::spec::BlockListSpec::new(
            "item",
            nested.clone(),
        ));
        assert!(decode(&body, &list_spec_no_limits).is_ok());

        // Error: min_items constraint
        let list_min_err = Spec::BlockList(
            crate::hcldec::spec::BlockListSpec::new("item", nested.clone()).with_min_items(5),
        );
        assert!(decode(&body, &list_min_err).is_err());

        // Error: max_items constraint
        let list_max_err = Spec::BlockList(
            crate::hcldec::spec::BlockListSpec::new("item", nested).with_max_items(1),
        );
        assert!(decode(&body, &list_max_err).is_err());
    }

    #[test]
    fn test_decode_block_set() {
        let src = "tag {\n  name = \"web\"\n}\ntag {\n  name = \"web\"\n}\n";
        let body = crate::api::parse(src).unwrap();

        let mut attrs = HashMap::new();
        attrs.insert(
            "name".to_string(),
            Spec::Attr(AttrSpec::new("name", Type::String)),
        );
        let set_spec = Spec::BlockSet(crate::hcldec::spec::BlockSetSpec::new(
            "tag",
            Spec::Object(attrs),
        ));
        let val = decode(&body, &set_spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*val.data),
            std::mem::discriminant(&ValueData::Set(std::collections::BTreeSet::new()))
        );
    }

    #[test]
    fn test_decode_block_map_single_and_multiple_labels() {
        let src_single = "server \"web\" {\n  port = 80\n}\nserver \"api\" {\n  port = 8080\n}\n";
        let body_single = crate::api::parse(src_single).unwrap();

        let mut attrs = HashMap::new();
        attrs.insert(
            "port".to_string(),
            Spec::Attr(AttrSpec::new("port", Type::Number)),
        );
        let map_spec = Spec::BlockMap(crate::hcldec::spec::BlockMapSpec::new(
            "server",
            vec!["name".to_string()],
            Spec::Object(attrs.clone()),
        ));
        let val_single = decode(&body_single, &map_spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*val_single.data),
            std::mem::discriminant(&ValueData::Object(BTreeMap::new()))
        );
        assert!(format!("{val_single:?}").contains("web"));
        assert!(format!("{val_single:?}").contains("api"));

        // Multi-level labels
        let src_multi = "env \"prod\" \"db\" {\n  port = 5432\n}\nenv \"prod\" \"web\" {\n  port = 443\n}\nenv \"dev\" \"web\" {\n  port = 80\n}\n";
        let body_multi = crate::api::parse(src_multi).unwrap();
        let multi_map_spec = Spec::BlockMap(crate::hcldec::spec::BlockMapSpec::new(
            "env",
            vec!["env".to_string(), "role".to_string()],
            Spec::Object(attrs),
        ));
        let val_multi = decode(&body_multi, &multi_map_spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*val_multi.data),
            std::mem::discriminant(&ValueData::Object(BTreeMap::new()))
        );
        assert!(format!("{val_multi:?}").contains("prod"));
        assert!(format!("{val_multi:?}").contains("dev"));

        // Error: label count mismatch
        let bad_label_spec = Spec::BlockMap(crate::hcldec::spec::BlockMapSpec::new(
            "env",
            vec!["only_one".to_string()],
            Spec::Literal(crate::hcldec::spec::LiteralSpec::new(Value::new(
                Type::Number,
                ValueData::Number(crate::number::Number::from(1)),
            ))),
        ));
        assert!(decode(&body_multi, &bad_label_spec).is_err());
    }

    #[test]
    fn test_decode_block_attrs_success_and_errors() {
        let src = r#"
        settings {
            host = "localhost"
            port = "8080"
        }
        "#;
        let body = crate::api::parse(src).unwrap();

        let attrs_spec = Spec::BlockAttrs(
            crate::hcldec::spec::BlockAttrsSpec::new("settings").with_type(Type::String),
        );
        let val = decode(&body, &attrs_spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*val.data),
            std::mem::discriminant(&ValueData::Object(BTreeMap::new()))
        );
        assert!(format!("{val:?}").contains("localhost"));
        assert!(format!("{val:?}").contains("8080"));

        // Error: missing block
        let missing_spec =
            Spec::BlockAttrs(crate::hcldec::spec::BlockAttrsSpec::new("nonexistent"));
        assert!(decode(&body, &missing_spec).is_err());

        // Error: multiple blocks found for BlockAttrs
        let src_mult = "settings {\n  a = \"1\"\n}\nsettings {\n  b = \"2\"\n}\n";
        let body_mult = crate::api::parse(src_mult).unwrap();
        assert!(decode(&body_mult, &attrs_spec).is_err());

        // Error: type mismatch in attributes
        let src_num = r"
        settings {
            port = 8080
        }
        ";
        let body_num = crate::api::parse(src_num).unwrap();
        assert!(decode(&body_num, &attrs_spec).is_err());
    }

    #[test]
    fn test_decode_literal_and_transform_and_tuple() {
        let body = Body::new(empty_span());

        let lit_val = Value::new(Type::String, ValueData::String("const".to_string()));
        let lit_spec = Spec::Literal(crate::hcldec::spec::LiteralSpec::new(lit_val.clone()));
        let val = decode(&body, &lit_spec).unwrap();
        assert_eq!(val, lit_val);

        let transform_spec =
            Spec::Transform(crate::hcldec::spec::TransformSpec::new(lit_spec, |_val| {
                Ok(Value::new(
                    Type::String,
                    ValueData::String("const transformed".to_string()),
                ))
            }));
        let val_trans = decode(&body, &transform_spec).unwrap();
        assert_eq!(
            val_trans,
            Value::new(
                Type::String,
                ValueData::String("const transformed".to_string())
            )
        );

        let one_val = Value::new(
            Type::Number,
            ValueData::Number(crate::number::Number::from(1)),
        );
        let true_val = Value::new(Type::Bool, ValueData::Bool(true));
        let tuple_spec = Spec::Tuple(crate::hcldec::spec::TupleSpec::new(vec![
            Spec::Literal(crate::hcldec::spec::LiteralSpec::new(one_val.clone())),
            Spec::Literal(crate::hcldec::spec::LiteralSpec::new(true_val.clone())),
        ]));
        let val_tup = decode(&body, &tuple_spec).unwrap();
        assert_eq!(
            std::mem::discriminant(&*val_tup.data),
            std::mem::discriminant(&ValueData::Array(vec![]))
        );

        let ctx = Context::new();
        assert!(decode_with_context(&body, &tuple_spec, &ctx).is_ok());
    }

    #[test]
    fn test_hcldec_error_propagation() {
        let ctx = Context::new();

        // 1. Line 56: Spec::Array(Block) inner decode fails
        let src1 = "server { }";
        let body1 = crate::api::parse(src1).unwrap();
        let spec1 = Spec::Array(Box::new(Spec::Block(crate::hcldec::spec::BlockSpec {
            type_name: "server".to_string(),
            body: Box::new(Spec::Required(Box::new(Spec::Attr(
                crate::hcldec::spec::AttrSpec::new("req", Type::String),
            )))),
        })));
        assert!(decode_with_context(&body1, &spec1, &ctx).is_err());

        // 2. Line 117: Spec::BlockList inner decode fails
        let spec2 = Spec::BlockList(crate::hcldec::spec::BlockListSpec::new(
            "server",
            Spec::Required(Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                "req",
                Type::String,
            )))),
        ));
        assert!(decode_with_context(&body1, &spec2, &ctx).is_err());

        // 3. Line 128: Spec::BlockSet inner decode fails
        let spec3 = Spec::BlockSet(crate::hcldec::spec::BlockSetSpec::new(
            "server",
            Spec::Required(Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                "req",
                Type::String,
            )))),
        ));
        assert!(decode_with_context(&body1, &spec3, &ctx).is_err());

        // 4. Line 183: Spec::Transform inner decode fails
        let body_empty = Body::new(empty_span());
        let spec4 = Spec::Transform(crate::hcldec::spec::TransformSpec::new(
            Spec::Required(Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                "req",
                Type::String,
            )))),
            Ok,
        ));
        assert!(decode_with_context(&body_empty, &spec4, &ctx).is_err());

        // 5. Line 190: Spec::Tuple inner decode fails
        let spec5 = Spec::Tuple(crate::hcldec::spec::TupleSpec::new(vec![Spec::Required(
            Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                "req",
                Type::String,
            ))),
        )]));
        assert!(decode_with_context(&body_empty, &spec5, &ctx).is_err());

        // 6. Line 245: Spec::BlockMap (1 label) inner decode fails
        let src6 = "server \"web\" { }";
        let body6 = crate::api::parse(src6).unwrap();
        let spec6 = Spec::BlockMap(crate::hcldec::spec::BlockMapSpec::new(
            "server",
            vec!["name".to_string()],
            Spec::Required(Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                "req",
                Type::String,
            )))),
        ));
        assert!(decode_with_context(&body6, &spec6, &ctx).is_err());

        // 7. Lines 273 and 289: Spec::BlockMap (multi-label) inner decode fails
        let src7 = "server \"prod\" \"web\" { }";
        let body7 = crate::api::parse(src7).unwrap();
        let spec7 = Spec::BlockMap(crate::hcldec::spec::BlockMapSpec::new(
            "server",
            vec!["env".to_string(), "name".to_string()],
            Spec::Required(Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                "req",
                Type::String,
            )))),
        ));
        assert!(decode_with_context(&body7, &spec7, &ctx).is_err());
    }

    #[test]
    fn test_hcldec_coverage_block_attrs_eval_error() {
        let src = r"
        settings {
            host = var.nonexistent
        }
        ";
        let body = crate::api::parse(src).unwrap();
        let attrs_spec = Spec::BlockAttrs(crate::hcldec::spec::BlockAttrsSpec::new("settings"));
        assert!(decode(&body, &attrs_spec).is_err());
    }

    #[test]
    fn test_hcldec_unrelated_blocks_coverage() {
        let src = "unrelated \"x\" {\n  a = 1\n}\ntarget \"t\" {\n  name = \"hello\"\n}\n";
        let body = crate::api::parse(src).unwrap();

        // 1. Spec::Array with Spec::Block
        let block_spec = Spec::Block(BlockSpec {
            type_name: "target".to_string(),
            body: Box::new(Spec::Object(HashMap::new())),
        });
        assert!(decode(&body, &Spec::Array(Box::new(block_spec))).is_ok());

        // 2. Spec::Block
        let single_block_spec = Spec::Block(BlockSpec {
            type_name: "target".to_string(),
            body: Box::new(Spec::Object(HashMap::new())),
        });
        assert!(decode(&body, &single_block_spec).is_ok());

        // 3. Spec::BlockList
        let list_spec = Spec::BlockList(crate::hcldec::spec::BlockListSpec::new(
            "target",
            Spec::Object(HashMap::new()),
        ));
        assert!(decode(&body, &list_spec).is_ok());

        // 4. Spec::BlockSet
        let set_spec = Spec::BlockSet(crate::hcldec::spec::BlockSetSpec::new(
            "target",
            Spec::Object(HashMap::new()),
        ));
        assert!(decode(&body, &set_spec).is_ok());

        // 5. Spec::BlockAttrs
        let attrs_spec = Spec::BlockAttrs(crate::hcldec::spec::BlockAttrsSpec::new("target"));
        assert!(decode(&body, &attrs_spec).is_ok());

        // 6. Spec::BlockMap
        let map_spec = Spec::BlockMap(crate::hcldec::spec::BlockMapSpec::new(
            "target",
            vec!["lbl".to_string()],
            Spec::Object(HashMap::new()),
        ));
        assert!(decode(&body, &map_spec).is_ok());
    }

    #[test]
    fn test_partial_decode_multi_stage() {
        let src = r#"
service "web" {
  port = 80
}
database "primary" {
  engine = "postgres"
}
global_timeout = 30
environment = "prod"
"#;
        let body = crate::api::parse(src).unwrap();
        let mut ctx = Context::new();

        // Stage 1: Decode service block and global_timeout
        let mut stage1_specs = HashMap::new();
        stage1_specs.insert(
            "timeout".to_string(),
            Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                "global_timeout",
                Type::Number,
            )),
        );
        stage1_specs.insert(
            "web_service".to_string(),
            Spec::Block(BlockSpec {
                type_name: "service".to_string(),
                body: Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                    "port",
                    Type::Number,
                ))),
            }),
        );
        let stage1_spec = Spec::Object(stage1_specs);

        let (val1, residual1) = partial_decode(&body, &stage1_spec, &mut ctx).unwrap();

        assert!(matches!(val1.data.as_ref(), ValueData::Object(_)));
        // Residual body should no longer contain service or global_timeout
        assert!(!residual1.attributes.contains_key("global_timeout"));
        assert!(!residual1.blocks.iter().any(|b| b.block_type == "service"));
        // Residual body still contains environment and database
        assert!(residual1.attributes.contains_key("environment"));
        assert!(residual1.blocks.iter().any(|b| b.block_type == "database"));

        // Stage 2: Decode remaining items from residual1
        let mut stage2_specs = HashMap::new();
        stage2_specs.insert(
            "env".to_string(),
            Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                "environment",
                Type::String,
            )),
        );
        stage2_specs.insert(
            "db".to_string(),
            Spec::Block(BlockSpec {
                type_name: "database".to_string(),
                body: Box::new(Spec::Attr(crate::hcldec::spec::AttrSpec::new(
                    "engine",
                    Type::String,
                ))),
            }),
        );
        let stage2_spec = Spec::Object(stage2_specs);

        let (val2, residual2) = partial_decode(&residual1, &stage2_spec, &mut ctx).unwrap();
        assert!(matches!(val2.data.as_ref(), ValueData::Object(_)));
        assert!(residual2.attributes.is_empty());
        assert_eq!(residual2.blocks.len(), 0);

        // Test error propagation in partial_decode
        let bad_spec = Spec::Attr(crate::hcldec::spec::AttrSpec::new(
            "nonexistent",
            Type::String,
        ));
        assert!(partial_decode(&residual1, &bad_spec, &mut ctx).is_err());
    }

    #[test]
    fn test_hcldec_expr_spec() {
        let dummy_span = crate::span::Span::new(0, 0, 0, 0, 0, 0);
        let expr = crate::ast::expr::Expression::Number(
            crate::number::Number::from(42_i64),
            dummy_span.clone(),
        );
        let spec = Spec::Expr(crate::hcldec::spec::ExprSpec::new(expr));

        let body = Body::new(crate::span::Span::new(0, 0, 0, 0, 0, 0));
        let val = decode(&body, &spec).unwrap();
        assert_eq!(val.to_string(), "42");

        // Failing expression evaluation (line 254)
        let fail_expr =
            crate::ast::expr::Expression::Variable("nonexistent_var".to_string(), dummy_span);
        let fail_spec = Spec::Expr(crate::hcldec::spec::ExprSpec::new(fail_expr));
        assert!(decode(&body, &fail_spec).is_err());
    }

    #[test]
    fn test_partial_decode_all_spec_variants() {
        use crate::hcldec::spec::{
            AttrSpec, BlockAttrsSpec, BlockListSpec, BlockMapSpec, BlockSetSpec, DefaultSpec,
            LiteralSpec, TransformSpec, TupleSpec,
        };

        // Attribute expression evaluation failure during decode_partial (line 283)
        let fail_body = crate::api::parse("failing_attr = nonexistent_var").unwrap();
        let fail_attr_spec = Spec::Attr(AttrSpec::new("failing_attr", Type::String));
        let mut ctx = Context::new();
        assert!(partial_decode(&fail_body, &fail_attr_spec, &mut ctx).is_err());

        // BlockAttrs with unrelated block present (line 410 false branch)
        let blk_attrs_body =
            crate::api::parse("unrelated_blk {}\nsetting {\n  foo = \"bar\"\n}").unwrap();
        let blk_attrs_spec = Spec::BlockAttrs(BlockAttrsSpec::new("setting"));
        assert!(partial_decode(&blk_attrs_body, &blk_attrs_spec, &mut ctx).is_ok());

        let src = r#"
item {
  val = "item1"
}
item {
  val = "item2"
}
node "alpha" {
  ip = "10.0.0.1"
}
metadata {
  owner = "admin"
  region = "us-east-1"
}
"#;
        let body = crate::api::parse(src).unwrap();
        let mut ctx = Context::new();

        // 1. BlockList with min/max
        let mut list_spec =
            BlockListSpec::new("item", Spec::Attr(AttrSpec::new("val", Type::String)));
        list_spec.min_items = Some(1);
        list_spec.max_items = Some(5);
        let (val_list, res1) =
            partial_decode(&body, &Spec::BlockList(list_spec), &mut ctx).unwrap();
        assert!(matches!(val_list.data.as_ref(), ValueData::Array(_)));

        // 2. BlockMap
        let map_spec = BlockMapSpec::new(
            "node",
            vec!["name".to_string()],
            Spec::Attr(AttrSpec::new("ip", Type::String)),
        );
        let (val_map, res2) = partial_decode(&res1, &Spec::BlockMap(map_spec), &mut ctx).unwrap();
        assert!(matches!(val_map.data.as_ref(), ValueData::Object(_)));

        // 3. BlockAttrs
        let attrs_spec = BlockAttrsSpec::new("metadata");
        let (val_attrs, res3) =
            partial_decode(&res2, &Spec::BlockAttrs(attrs_spec), &mut ctx).unwrap();
        assert!(matches!(val_attrs.data.as_ref(), ValueData::Object(_)));
        assert_eq!(res3.blocks.len(), 0);

        // 4. BlockSet, Default, Optional, Tuple, Transform, Array
        let body_set =
            crate::api::parse("tag {\n  val = \"a\"\n}\ntag {\n  val = \"b\"\n}\n").unwrap();
        let set_spec = BlockSetSpec::new("tag", Spec::Attr(AttrSpec::new("val", Type::String)));
        let (val_set, _) = partial_decode(&body_set, &Spec::BlockSet(set_spec), &mut ctx).unwrap();
        assert!(matches!(val_set.data.as_ref(), ValueData::Set(_)));

        // Default missing fallback
        let empty_body = Body::new(crate::span::Span::new(0, 0, 0, 0, 0, 0));
        use crate::encode::EncodeValue;
        let def_spec = Spec::Default(DefaultSpec {
            primary: Box::new(Spec::Attr(AttrSpec::new("missing", Type::String))),
            default_value: "fallback".encode_value(),
        });
        let (val_def, _) = partial_decode(&empty_body, &def_spec, &mut ctx).unwrap();
        assert_eq!(val_def.to_string(), "\"fallback\"");

        // Optional missing fallback to null
        let opt_spec = Spec::Optional(Box::new(Spec::Attr(AttrSpec::new("missing", Type::Number))));
        let (val_opt, _) = partial_decode(&empty_body, &opt_spec, &mut ctx).unwrap();
        assert!(val_opt.is_null());

        // Tuple and Transform
        let tuple_spec = Spec::Tuple(TupleSpec::new(vec![
            Spec::Literal(LiteralSpec::new(10_i64.encode_value())),
            Spec::Transform(TransformSpec::new(
                Spec::Literal(LiteralSpec::new(20_i64.encode_value())),
                Ok,
            )),
        ]));
        let (val_tuple, _) = partial_decode(&empty_body, &tuple_spec, &mut ctx).unwrap();
        assert!(matches!(val_tuple.data.as_ref(), ValueData::Array(_)));

        // Array of blocks
        let body_arr = crate::api::parse("server {\n  port = 80\n}\n").unwrap();
        let arr_spec = Spec::Array(Box::new(Spec::Block(BlockSpec {
            type_name: "server".to_string(),
            body: Box::new(Spec::Attr(AttrSpec::new("port", Type::Number))),
        })));
        let (val_arr, _) = partial_decode(&body_arr, &arr_spec, &mut ctx).unwrap();
        assert!(matches!(val_arr.data.as_ref(), ValueData::Array(_)));
    }

    #[test]
    fn test_partial_decode_coverage_exhaustive() {
        use crate::encode::EncodeValue;
        use crate::hcldec::spec::*;
        let mut ctx = Context::new();
        let empty_body = Body::new(crate::span::Span::new(0, 0, 0, 0, 0, 0));

        // 1. Spec::Block: multiple blocks and missing block
        let block_spec = Spec::Block(BlockSpec {
            type_name: "server".to_string(),
            body: Box::new(Spec::Attr(AttrSpec::new("port", Type::Number))),
        });
        let body_multi_block =
            crate::api::parse("server {\n  port = 80\n}\nserver {\n  port = 81\n}\n").unwrap();
        assert!(partial_decode(&body_multi_block, &block_spec, &mut ctx).is_err());
        assert!(partial_decode(&empty_body, &block_spec, &mut ctx).is_err());

        // 2. Spec::BlockList: min_items and max_items error paths
        let list_min_spec = Spec::BlockList(
            BlockListSpec::new(
                "item",
                Spec::Literal(LiteralSpec::new(1_i64.encode_value())),
            )
            .with_min_items(2),
        );
        assert!(partial_decode(&empty_body, &list_min_spec, &mut ctx).is_err());

        let body_two_items = crate::api::parse("item {}\nitem {}\n").unwrap();
        let list_max_spec = Spec::BlockList(
            BlockListSpec::new(
                "item",
                Spec::Literal(LiteralSpec::new(1_i64.encode_value())),
            )
            .with_max_items(1),
        );
        assert!(partial_decode(&body_two_items, &list_max_spec, &mut ctx).is_err());

        // 3. Spec::BlockSet: decode error inside set block, and non-matching block retention
        let body_set_err = crate::api::parse("tag {\n  val = undefined_set_var\n}\n").unwrap();
        let set_spec = BlockSetSpec::new("tag", Spec::Attr(AttrSpec::new("val", Type::String)));
        assert!(
            partial_decode(&body_set_err, &Spec::BlockSet(set_spec.clone()), &mut ctx).is_err()
        );

        let body_mixed_set =
            crate::api::parse("tag {\n  val = \"a\"\n}\nunrelated {\n  x = 1\n}\n").unwrap();
        let (_val_set_mixed, res_mixed) =
            partial_decode(&body_mixed_set, &Spec::BlockSet(set_spec), &mut ctx).unwrap();
        assert_eq!(res_mixed.blocks.len(), 1);

        // 4. Spec::BlockAttrs: multiple blocks, eval error, type mismatch, missing
        let attrs_spec = Spec::BlockAttrs(BlockAttrsSpec::new("meta"));
        let body_multi_attrs =
            crate::api::parse("meta {\n  a = 1\n}\nmeta {\n  b = 2\n}\n").unwrap();
        assert!(partial_decode(&body_multi_attrs, &attrs_spec, &mut ctx).is_err());

        let body_bad_eval = crate::api::parse("meta {\n  a = undefined_var\n}\n").unwrap();
        assert!(partial_decode(&body_bad_eval, &attrs_spec, &mut ctx).is_err());

        let body_typed = crate::api::parse("meta {\n  a = \"string_val\"\n}\n").unwrap();
        let attrs_typed_spec =
            Spec::BlockAttrs(BlockAttrsSpec::new("meta").with_type(Type::Number));
        assert!(partial_decode(&body_typed, &attrs_typed_spec, &mut ctx).is_err());

        assert!(partial_decode(&empty_body, &attrs_spec, &mut ctx).is_err());

        // 5. Spec::Expr: success and eval error
        let expr_spec = Spec::Expr(crate::hcldec::spec::ExprSpec::new(
            crate::ast::expr::Expression::Number(
                crate::number::Number::from(42),
                crate::span::Span::new(0, 0, 0, 0, 0, 0),
            ),
        ));
        let (v_expr, _) = partial_decode(&empty_body, &expr_spec, &mut ctx).unwrap();
        assert_eq!(v_expr.to_string(), "42");

        let expr_bad = Spec::Expr(crate::hcldec::spec::ExprSpec::new(
            crate::ast::expr::Expression::Variable(
                "undefined_expr_var".to_string(),
                crate::span::Span::new(0, 0, 0, 0, 0, 0),
            ),
        ));
        assert!(partial_decode(&empty_body, &expr_bad, &mut ctx).is_err());

        // 6. Spec::Default: success and non-missing error
        let def_spec = Spec::Default(DefaultSpec {
            primary: Box::new(Spec::Attr(AttrSpec::new("present", Type::String))),
            default_value: "fallback".encode_value(),
        });
        let body_def_ok = crate::api::parse("present = \"actual\"\n").unwrap();
        let (v_def_ok, _) = partial_decode(&body_def_ok, &def_spec, &mut ctx).unwrap();
        assert_eq!(v_def_ok.to_string(), "\"actual\"");

        let body_def_err = crate::api::parse("present = [1, 2]\n").unwrap();
        assert!(partial_decode(&body_def_err, &def_spec, &mut ctx).is_err());

        // 7. Spec::Optional: success and non-missing error
        let opt_spec = Spec::Optional(Box::new(Spec::Attr(AttrSpec::new("opt", Type::Number))));
        let body_opt_ok = crate::api::parse("opt = 42\n").unwrap();
        let (v_opt_ok, _) = partial_decode(&body_opt_ok, &opt_spec, &mut ctx).unwrap();
        assert_eq!(v_opt_ok.to_string(), "42");

        let body_opt_err = crate::api::parse("opt = [1, 2]\n").unwrap();
        assert!(partial_decode(&body_opt_err, &opt_spec, &mut ctx).is_err());

        // 8. Spec::Required in partial_decode
        let req_spec = Spec::Required(Box::new(Spec::Attr(AttrSpec::new(
            "req_attr",
            Type::String,
        ))));
        let body_req = crate::api::parse("req_attr = \"req_val\"\n").unwrap();
        let (val_req, _) = partial_decode(&body_req, &req_spec, &mut ctx).unwrap();
        assert_eq!(val_req.to_string(), "\"req_val\"");

        // 9. Spec::Array: non-block error
        let bad_array_spec = Spec::Array(Box::new(Spec::Literal(LiteralSpec::new(
            1_i64.encode_value(),
        ))));
        assert!(partial_decode(&empty_body, &bad_array_spec, &mut ctx).is_err());
    }

    #[test]
    fn test_hcldec_typo_suggestions() {
        let body = crate::api::parse("usrname = \"alice\"\n").unwrap();
        let spec = Spec::Attr(AttrSpec::new("username", Type::String));
        let err = decode(&body, &spec).err().unwrap();
        assert!(err.to_string().contains("Did you mean \"usrname\"?"));

        let mut ctx = Context::new();
        let val_err = partial_decode(&body, &spec, &mut ctx).err().unwrap();
        let first_err = &val_err.errors()[0];
        assert!(first_err.to_string().contains("Did you mean \"usrname\"?"));
    }

    #[test]
    fn test_hcldec_error_propagation_branches() {
        use crate::hcldec::spec::{AttrSpec, BlockListSpec, TransformSpec, TupleSpec};

        let empty_body = Body::new(crate::span::Span::new(0, 0, 0, 0, 0, 0));
        let mut ctx = Context::new();

        // 1. Spec::Object partial decode error propagation (line 270)
        let mut obj_map = std::collections::HashMap::new();
        obj_map.insert(
            "field".to_string(),
            Spec::Required(Box::new(Spec::Attr(AttrSpec::new(
                "missing_obj_attr",
                Type::String,
            )))),
        );
        assert!(partial_decode(&empty_body, &Spec::Object(obj_map), &mut ctx).is_err());

        // 2. Spec::BlockList decode nested error propagation (line 353)
        let blk_body = crate::api::parse("item {}\n").unwrap();
        let list_spec = Spec::BlockList(BlockListSpec::new(
            "item",
            Spec::Attr(AttrSpec::new("missing_in_block", Type::String)),
        ));
        assert!(partial_decode(&blk_body, &list_spec, &mut ctx).is_err());

        // 3. Spec::Transform partial decode error propagation (line 450)
        let trans_spec = Spec::Transform(TransformSpec::new(
            Spec::Required(Box::new(Spec::Attr(AttrSpec::new(
                "missing_trans_attr",
                Type::String,
            )))),
            Ok,
        ));
        assert!(partial_decode(&empty_body, &trans_spec, &mut ctx).is_err());

        // 4. Spec::Tuple partial decode error propagation (line 471)
        let tuple_spec = Spec::Tuple(TupleSpec::new(vec![Spec::Required(Box::new(Spec::Attr(
            AttrSpec::new("missing_tuple_attr", Type::String),
        )))]));
        assert!(partial_decode(&empty_body, &tuple_spec, &mut ctx).is_err());
    }

    #[test]
    fn test_decode_tuple_and_transform_heterogeneous_and_partial() {
        use crate::encode::EncodeValue;
        use crate::hcldec::spec::{AttrSpec, LiteralSpec, TransformSpec, TupleSpec};

        let src = r#"
            name = "server-01"
            replicas = 3
            enabled = true
        "#;
        let body = crate::api::parse(src).unwrap();
        let mut ctx = Context::new();

        // Heterogeneous tuple of [name, replicas, enabled, transformed_literal]
        let tuple_spec = Spec::Tuple(TupleSpec::new(vec![
            Spec::Attr(AttrSpec::new("name", Type::String)),
            Spec::Attr(AttrSpec::new("replicas", Type::Number)),
            Spec::Attr(AttrSpec::new("enabled", Type::Bool)),
            Spec::Transform(TransformSpec::new(
                Spec::Literal(LiteralSpec::new(100_i64.encode_value())),
                |_v| {
                    Ok(Value::new(
                        Type::Number,
                        ValueData::Number(crate::number::Number::from(200)),
                    ))
                },
            )),
        ]));

        // Eager decode
        let eager_val = decode(&body, &tuple_spec).unwrap();
        assert_eq!(
            eager_val.ty(),
            &Type::Tuple(vec![Type::String, Type::Number, Type::Bool, Type::Number])
        );
        assert_eq!(eager_val.to_string(), "[\"server-01\", 3, true, 200]");

        // Partial decode
        let (partial_val, residual) = partial_decode(&body, &tuple_spec, &mut ctx).unwrap();
        assert_eq!(partial_val.ty(), eager_val.ty());
        assert_eq!(partial_val, eager_val);
        assert!(residual.attributes.is_empty());
    }

    #[test]
    fn test_decode_attr_spec_declarative_validation_in_decode() {
        use crate::hcldec::spec::AttrSpec;

        let regex = regex::Regex::new(r"^env-[a-z]+$").unwrap();

        // 1. Success case
        let src_ok = r#"
            environment = "env-production"
            port = 8080
        "#;
        let body_ok = crate::api::parse(src_ok).unwrap();

        let mut specs = HashMap::new();
        specs.insert(
            "env".to_string(),
            Spec::Attr(
                AttrSpec::new("environment", Type::String)
                    .with_regex(regex.clone())
                    .with_custom_validator(|v| {
                        if v.to_string().contains("deprecated") {
                            Err("deprecated environment".to_string())
                        } else {
                            Ok(())
                        }
                    }),
            ),
        );
        specs.insert(
            "port".to_string(),
            Spec::Attr(
                AttrSpec::new("port", Type::Number)
                    .with_min_value(crate::number::Number::from(1024))
                    .with_max_value(crate::number::Number::from(65535)),
            ),
        );
        let spec_root = Spec::Object(specs);

        let decoded = decode(&body_ok, &spec_root).unwrap();
        assert!(format!("{decoded:?}").contains("env-production"));

        // 2. Failure: regex validation mismatch in decode
        let src_regex_bad = r#"
            environment = "INVALID_ENV"
            port = 8080
        "#;
        let body_regex_bad = crate::api::parse(src_regex_bad).unwrap();
        let err_regex = decode(&body_regex_bad, &spec_root).err().unwrap();
        assert!(
            err_regex
                .to_string()
                .contains("does not match required regex pattern")
        );

        // 3. Failure: min_value validation failure in decode
        let src_port_bad = r#"
            environment = "env-staging"
            port = 80
        "#;
        let body_port_bad = crate::api::parse(src_port_bad).unwrap();
        let err_port = decode(&body_port_bad, &spec_root).err().unwrap();
        assert!(
            err_port
                .to_string()
                .contains("less than minimum allowed value")
        );

        // 4. Failure: custom validator failure in partial_decode emits Diagnostic
        let src_custom_bad = r#"
            environment = "env-deprecated"
            port = 8080
        "#;
        let body_custom_bad = crate::api::parse(src_custom_bad).unwrap();
        let mut ctx = Context::new();
        let err_diag = partial_decode(&body_custom_bad, &spec_root, &mut ctx)
            .err()
            .unwrap();
        let diag_str = format!("{err_diag:?}");
        assert!(diag_str.contains("deprecated environment"));
    }

    #[test]
    fn test_multi_stage_partial_spec_with_block_map_and_unknowns() {
        use crate::hcldec::spec::{AttrSpec, BlockMapSpec};

        let src = r#"
            service "web" {
                listen_port = 80
                upstream = "static_backend"
            }
            service "api" {
                listen_port = 8080
                upstream = dynamic_backend
            }
        "#;
        let body = crate::api::parse(src).unwrap();
        let mut ctx_stage1 = Context::new();
        // dynamic_backend is NOT known in Stage 1!

        let mut inner_specs = HashMap::new();
        inner_specs.insert(
            "port".to_string(),
            Spec::Attr(AttrSpec::new("listen_port", Type::Number)),
        );
        inner_specs.insert(
            "backend".to_string(),
            Spec::Attr(AttrSpec::new("upstream", Type::String)),
        );
        let map_spec = Spec::BlockMap(BlockMapSpec::new(
            "service",
            vec!["name".to_string()],
            Spec::Object(inner_specs.clone()),
        ));

        // Stage 1: Partial decode
        let (val1, residual1) = partial_decode(&body, &map_spec, &mut ctx_stage1).unwrap();

        // The map keys ("web", "api") are statically resolvable!
        assert!(format!("{val1:?}").contains("web"));
        assert!(format!("{val1:?}").contains("api"));
        assert!(val1.contains_unknown());

        // The "api" block is preserved in residual1 because its inner body depends on unknown variables
        assert_eq!(residual1.blocks.len(), 1);
        assert_eq!(residual1.blocks[0].labels, vec!["api".to_string()]);

        // Stage 2: Bind the missing variable and decode residual1
        let mut ctx_stage2 = Context::new();
        ctx_stage2.set_variable(
            "dynamic_backend".to_string(),
            Value::new(Type::String, ValueData::String("k8s_backend".to_string())),
        );

        let (val2, residual2) = partial_decode(&residual1, &map_spec, &mut ctx_stage2).unwrap();
        assert!(format!("{val2:?}").contains("k8s_backend"));
        assert!(!val2.contains_unknown());
        assert_eq!(residual2.blocks.len(), 0);
    }

    /// Tests coercion failures and inner block error cases in decoding.
    #[test]
    fn test_hcldec_decode_coercion_and_block_errors() {
        use crate::hcldec::spec::{AttrSpec, BlockAttrsSpec, BlockSpec};

        // 1. Eager Spec::Attr type coercion failure
        let src_attr_err = r#"port = "not_a_number""#;
        let body_attr_err = crate::api::parse(src_attr_err).unwrap();
        let spec_attr_num = Spec::Attr(AttrSpec::new("port", Type::Number));
        let _ctx = Context::new();
        assert!(decode(&body_attr_err, &spec_attr_num).is_err());

        // 2. Partial Spec::Attr type coercion failure
        let mut ctx_coercion = Context::new();
        assert!(partial_decode(&body_attr_err, &spec_attr_num, &mut ctx_coercion).is_err());

        // 3. Partial Spec::Block inner error
        let src_inner_err = r#"
            server {
                host = "not_a_num"
            }
        "#;
        let body_inner_err = crate::api::parse(src_inner_err).unwrap();
        let spec_block_err = Spec::Block(BlockSpec {
            type_name: "server".to_string(),
            body: Box::new(Spec::Attr(AttrSpec::new("host", Type::Number))),
        });
        let mut ctx_inner = Context::new();
        assert!(partial_decode(&body_inner_err, &spec_block_err, &mut ctx_inner).is_err());

        // 4. Partial Spec::BlockAttrs type mismatch
        let src_attrs_mismatch = r#"
            meta {
                tag = "not_number"
            }
        "#;
        let body_attrs_mismatch = crate::api::parse(src_attrs_mismatch).unwrap();
        let block_attrs_spec =
            Spec::BlockAttrs(BlockAttrsSpec::new("meta").with_type(Type::Number));
        let mut ctx_mismatch = Context::new();
        assert!(
            partial_decode(&body_attrs_mismatch, &block_attrs_spec, &mut ctx_mismatch).is_err()
        );

        // 5. Partial Spec::BlockAttrs with matching type
        let src_attrs_ok = r"
            meta {
                tag = 123
            }
        ";
        let body_attrs_ok = crate::api::parse(src_attrs_ok).unwrap();
        let mut ctx_ok = Context::new();
        assert!(partial_decode(&body_attrs_ok, &block_attrs_spec, &mut ctx_ok).is_ok());
    }

    /// Tests that partial decoding re-inserts items when unknowns or unhandled body items remain.
    #[test]
    fn test_hcldec_decode_partial_body_reinsertion() {
        use crate::hcldec::spec::{AttrSpec, BlockListSpec, BlockSetSpec, BlockSpec};

        // 1. Partial Spec::Attr re-insertion on unknown value
        let src_unk_attr = r"data = unk_val";
        let body_unk_attr = crate::api::parse(src_unk_attr).unwrap();
        let mut ctx_attr_unk = Context::new();
        ctx_attr_unk.set_variable("unk_val".to_string(), Value::unknown(Type::String));
        let spec_attr = Spec::Attr(AttrSpec::new("data", Type::String));
        let (val_a, res_a) = partial_decode(&body_unk_attr, &spec_attr, &mut ctx_attr_unk).unwrap();
        assert!(val_a.contains_unknown());
        assert!(res_a.attributes.contains_key("data"));

        // 2. Partial Spec::Block, BlockList, BlockSet re-insertion on leftover attributes
        let src_block_extra = r#"
            server {
                host = "localhost"
                unhandled = "extra"
            }
        "#;
        let body_block_extra = crate::api::parse(src_block_extra).unwrap();
        let mut obj_spec = HashMap::new();
        obj_spec.insert(
            "host".to_string(),
            Spec::Attr(AttrSpec::new("host", Type::String)),
        );
        let spec_block = Spec::Block(BlockSpec {
            type_name: "server".to_string(),
            body: Box::new(Spec::Object(obj_spec.clone())),
        });
        let mut ctx_blk_extra = Context::new();
        let (_val_b, res_b) =
            partial_decode(&body_block_extra, &spec_block, &mut ctx_blk_extra).unwrap();
        assert_eq!(res_b.blocks.len(), 1);
        assert!(res_b.blocks[0].body.attributes.contains_key("unhandled"));

        let spec_list_unhandled =
            Spec::BlockList(BlockListSpec::new("server", Spec::Object(obj_spec.clone())));
        let (_val_lu, res_list_unhandled) =
            partial_decode(&body_block_extra, &spec_list_unhandled, &mut ctx_blk_extra).unwrap();
        assert_eq!(res_list_unhandled.blocks.len(), 1);

        let spec_set_unhandled =
            Spec::BlockSet(BlockSetSpec::new("server", Spec::Object(obj_spec)));
        let (_val_su, res_set_unhandled) =
            partial_decode(&body_block_extra, &spec_set_unhandled, &mut ctx_blk_extra).unwrap();
        assert_eq!(res_set_unhandled.blocks.len(), 1);

        // 3. Partial Block, BlockList, BlockSet with unknown values
        let src_nested_unk = r"
            item {
                val = unk_val
            }
        ";
        let body_nested_unk = crate::api::parse(src_nested_unk).unwrap();
        let mut ctx_nest_unk = Context::new();
        ctx_nest_unk.set_variable("unk_val".to_string(), Value::unknown(Type::String));

        let spec_single_block = Spec::Block(BlockSpec {
            type_name: "item".to_string(),
            body: Box::new(Spec::Attr(AttrSpec::new("val", Type::String))),
        });
        let (val_blk_u, res_blk_u) =
            partial_decode(&body_nested_unk, &spec_single_block, &mut ctx_nest_unk).unwrap();
        assert!(val_blk_u.contains_unknown());
        assert_eq!(res_blk_u.blocks.len(), 1);

        let spec_list_item = Spec::BlockList(BlockListSpec::new(
            "item",
            Spec::Attr(AttrSpec::new("val", Type::String)),
        ));
        let (val_list_u, res_list_u) =
            partial_decode(&body_nested_unk, &spec_list_item, &mut ctx_nest_unk).unwrap();
        assert!(val_list_u.contains_unknown());
        assert_eq!(res_list_u.blocks.len(), 1);

        let spec_set_item = Spec::BlockSet(BlockSetSpec::new(
            "item",
            Spec::Attr(AttrSpec::new("val", Type::String)),
        ));
        let (val_set_u, res_set_u) =
            partial_decode(&body_nested_unk, &spec_set_item, &mut ctx_nest_unk).unwrap();
        assert!(val_set_u.contains_unknown());
        assert_eq!(res_set_u.blocks.len(), 1);

        // 4. Partial Block, BlockList, BlockSet with leftover subblocks
        let src_subblock = r#"
            item {
                val = "ok"
                child { }
            }
        "#;
        let body_subblock = crate::api::parse(src_subblock).unwrap();
        let mut ctx_sub = Context::new();

        let (_val_b_sub, leftover_block) =
            partial_decode(&body_subblock, &spec_single_block, &mut ctx_sub).unwrap();
        assert_eq!(leftover_block.blocks.len(), 1);

        let (_val_l_sub, leftover_list) =
            partial_decode(&body_subblock, &spec_list_item, &mut ctx_sub).unwrap();
        assert_eq!(leftover_list.blocks.len(), 1);

        let (_val_s_sub, leftover_set) =
            partial_decode(&body_subblock, &spec_set_item, &mut ctx_sub).unwrap();
        assert_eq!(leftover_set.blocks.len(), 1);
    }

    /// Tests edge cases in partial decoding of `BlockMap`.
    #[test]
    fn test_hcldec_decode_block_map_partial_variations() {
        use crate::hcldec::spec::{AttrSpec, BlockMapSpec};

        // 1. Multi-label fallback to build_block_map
        let src_multi = r#"
            route "web" "v1" {
                priority = 10
            }
        "#;
        let body_multi = crate::api::parse(src_multi).unwrap();
        let mut route_fields = HashMap::new();
        route_fields.insert(
            "priority".to_string(),
            Spec::Attr(AttrSpec::new("priority", Type::Number)),
        );
        let spec_multi = Spec::BlockMap(BlockMapSpec::new(
            "route",
            vec!["target".to_string(), "version".to_string()],
            Spec::Object(route_fields),
        ));
        let mut ctx_multi = Context::new();
        let (val_multi, res_multi) =
            partial_decode(&body_multi, &spec_multi, &mut ctx_multi).unwrap();
        assert!(format!("{val_multi:?}").contains("priority"));
        assert_eq!(res_multi.blocks.len(), 0);

        // 2. Single-label with leftover attributes
        let src_leftover_attr = r#"
            entry "k1" {
                count = 5
                extra = "keep_me"
            }
        "#;
        let body_leftover_attr = crate::api::parse(src_leftover_attr).unwrap();
        let mut entry_fields = HashMap::new();
        entry_fields.insert(
            "count".to_string(),
            Spec::Attr(AttrSpec::new("count", Type::Number)),
        );
        let spec_single_map = Spec::BlockMap(BlockMapSpec::new(
            "entry",
            vec!["key".to_string()],
            Spec::Object(entry_fields),
        ));
        let mut ctx_leftover = Context::new();
        let (_val_lo, res_lo) =
            partial_decode(&body_leftover_attr, &spec_single_map, &mut ctx_leftover).unwrap();
        assert_eq!(res_lo.blocks.len(), 1);
        assert!(res_lo.blocks[0].body.attributes.contains_key("extra"));

        // 3. Single-label error branch yielding unknown
        let src_error_body = r#"
            service "backend" {
                port = "invalid_not_number"
            }
        "#;
        let body_error = crate::api::parse(src_error_body).unwrap();
        let mut svc_fields = HashMap::new();
        svc_fields.insert(
            "port".to_string(),
            Spec::Attr(AttrSpec::new("port", Type::Number)),
        );
        let spec_map_error = Spec::BlockMap(BlockMapSpec::new(
            "service",
            vec!["id".to_string()],
            Spec::Object(svc_fields),
        ));
        let mut ctx_err = Context::new();
        let (val_err, res_err) =
            partial_decode(&body_error, &spec_map_error, &mut ctx_err).unwrap();
        assert!(val_err.contains_unknown());
        assert_eq!(res_err.blocks.len(), 1);

        // 4. Label count mismatch error
        let src_mismatch_lbl = r#"
            service "lbl1" "lbl2" {
                port = 80
            }
        "#;
        let body_mismatch_lbl = crate::api::parse(src_mismatch_lbl).unwrap();
        let mut ctx_mismatch_lbl = Context::new();
        let spec_map_one_lbl = Spec::BlockMap(BlockMapSpec::new(
            "service",
            vec!["id".to_string()],
            Spec::Attr(AttrSpec::new("port", Type::Number)),
        ));
        assert!(
            partial_decode(&body_mismatch_lbl, &spec_map_one_lbl, &mut ctx_mismatch_lbl).is_err()
        );

        // 5. Single-label with unknown value
        let src_unk_map = r#"
            service "api" {
                port = unk_port
            }
        "#;
        let body_unk_map = crate::api::parse(src_unk_map).unwrap();
        let mut ctx_unk_map = Context::new();
        ctx_unk_map.set_variable("unk_port".to_string(), Value::unknown(Type::Number));
        let (val_unk_m, res_unk_m) =
            partial_decode(&body_unk_map, &spec_map_one_lbl, &mut ctx_unk_map).unwrap();
        assert!(val_unk_m.contains_unknown());
        assert_eq!(res_unk_m.blocks.len(), 1);

        // 6. Single-label with leftover subblock
        let src_subblock_map = r#"
            service "api" {
                port = 80
                nested { }
            }
        "#;
        let body_subblock_map = crate::api::parse(src_subblock_map).unwrap();
        let mut ctx_sub_map = Context::new();
        let (_val_sub_m, res_sub_m) =
            partial_decode(&body_subblock_map, &spec_map_one_lbl, &mut ctx_sub_map).unwrap();
        assert_eq!(res_sub_m.blocks.len(), 1);
    }
}
