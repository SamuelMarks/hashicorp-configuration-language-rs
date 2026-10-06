//! Dynamic block expansion engine.
//!
//! Implements HCL dynamic block expansion (`dynamic "<type>" { for_each = ... content { ... } }`).
//! Iterates over collections (lists, sets, maps, objects), binding iterator variables
//! (key and value) into child scopes and generating repeated nested AST blocks.
use crate::ast::structure::{Block, Body, DynamicBlock};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::error::HclError;
use crate::eval::context::Context;
use crate::eval::evaluator::Evaluator;
use crate::number::Number;
use crate::types::ty::Type;
use crate::types::val::{Value, ValueData};
use bigdecimal::BigDecimal;
use std::collections::BTreeMap;
/// Expands all `dynamic` blocks within a [`Body`] using the provided evaluation context.
///
/// Recursively processes regular blocks and nested dynamic blocks inside `content`.
///
/// # Arguments
/// * `body` - The AST body containing possible dynamic blocks.
/// * `ctx` - The evaluation context for resolving expressions.
///
/// # Errors
/// Returns [`Diagnostics`] if evaluating the `for_each` expression or label expressions fails,
/// or if an invalid type is encountered.
pub fn expand_dynamic_blocks(body: &Body, ctx: &mut Context) -> Result<Body, Diagnostics> {
    let mut new_body = Body::new(body.span.clone());
    new_body.attributes.clone_from(&body.attributes);
    new_body.functions.clone_from(&body.functions);
    new_body.validations.clone_from(&body.validations);
    new_body.preconditions.clone_from(&body.preconditions);
    new_body.postconditions.clone_from(&body.postconditions);
    for block in &body.blocks {
        let expanded_inner = expand_dynamic_blocks(&block.body, ctx)?;
        let mut new_block = block.clone();
        new_block.body = expanded_inner;
        new_body.blocks.push(new_block);
    }
    for dyn_block in &body.dynamic_blocks {
        expand_single_dynamic_block(dyn_block, ctx, &mut new_body.blocks)?;
    }
    Ok(new_body)
}
fn expand_single_dynamic_block(
    dyn_block: &DynamicBlock,
    ctx: &mut Context,
    out_blocks: &mut Vec<Block>,
) -> Result<(), Diagnostics> {
    let evaluator = Evaluator::new(ctx);
    let (for_each_val, _) = evaluator.evaluate(&dyn_block.for_each)?;
    if for_each_val.is_null() {
        return Ok(());
    }
    if for_each_val.is_unknown() {
        let block = Block {
            block_type: dyn_block.block_type.clone(),
            type_span: dyn_block.type_span.clone(),
            labels: Vec::new(),
            label_spans: Vec::new(),
            body: Body::new(dyn_block.span.clone()),
            span: dyn_block.span.clone(),
            open_brace_span: dyn_block.span.clone(),
            close_brace_span: dyn_block.span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        out_blocks.push(block);
        return Ok(());
    }
    let iterator_name = dyn_block
        .iterator
        .as_deref()
        .unwrap_or(&dyn_block.block_type);
    let items: Vec<(Value, Value)> = match &*for_each_val.data {
        ValueData::Array(arr) => arr
            .iter()
            .enumerate()
            .map(|(idx, val)| {
                let k = Value::new(
                    Type::Number,
                    ValueData::Number(Number::new(BigDecimal::from(idx as i64))),
                );
                (k, val.clone())
            })
            .collect(),
        ValueData::Set(set) => set.iter().map(|val| (val.clone(), val.clone())).collect(),
        ValueData::Object(obj) => obj
            .iter()
            .map(|(k, v)| {
                let key_val = Value::new(Type::String, ValueData::String(k.clone()));
                (key_val, v.clone())
            })
            .collect(),
        _ => {
            let diag = Diagnostic::new(
                HclError::DynamicBlock(format!(
                    "The `for_each` value must be a list, set, or map, not {}",
                    for_each_val.ty()
                )),
                dyn_block.for_each.span(),
            );
            return Err(Diagnostics::from(diag));
        }
    };
    for (key, val) in items {
        let mut child_ctx = Context::new_child(ctx);
        let mut iter_obj = BTreeMap::new();
        iter_obj.insert("key".to_string(), key);
        iter_obj.insert("value".to_string(), val);
        let iter_val = Value::new(Type::object(BTreeMap::new()), ValueData::Object(iter_obj));
        child_ctx.set_variable(iterator_name.to_string(), iter_val);
        let mut evaluated_labels = Vec::new();
        let mut label_spans = Vec::new();
        if let Some(labels_exprs) = &dyn_block.labels {
            for lbl_expr in labels_exprs {
                let eval = Evaluator::new(&child_ctx);
                let (label_val, _) = eval.evaluate(lbl_expr)?;
                match &*label_val.data {
                    ValueData::String(s) => evaluated_labels.push(s.clone()),
                    ValueData::Number(n) => evaluated_labels.push(n.0.to_string()),
                    _ => {
                        let diag = Diagnostic::new(
                            HclError::DynamicBlock(
                                "Dynamic block label expression must evaluate to a string or number."
                                    .to_string(),
                            ),
                            lbl_expr.span(),
                        );
                        return Err(Diagnostics::from(diag));
                    }
                }
                label_spans.push(lbl_expr.span());
            }
        }
        let expanded_content = expand_dynamic_blocks(&dyn_block.content, &mut child_ctx)?;
        let block = Block {
            block_type: dyn_block.block_type.clone(),
            type_span: dyn_block.type_span.clone(),
            labels: evaluated_labels,
            label_spans,
            body: expanded_content,
            span: dyn_block.span.clone(),
            open_brace_span: dyn_block.span.clone(),
            close_brace_span: dyn_block.span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        out_blocks.push(block);
    }
    Ok(())
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
    use crate::ast::expr::Expression;
    use crate::ast::structure::Attribute;
    use crate::span::Span;
    use std::collections::BTreeSet;
    use std::str::FromStr;
    #[test]
    fn test_expand_dynamic_blocks_array() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut root_body = Body::new(span.clone());
        let mut content = Body::new(span.clone());
        content.attributes.insert(
            "name".to_string(),
            Attribute {
                name: "name".to_string(),
                name_span: span.clone(),
                expr: Expression::Variable("item".to_string(), span.clone()),
                span: span.clone(),
                equals_span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        let dyn_block = DynamicBlock::new(
            "tag".to_string(),
            Expression::Tuple(
                vec![
                    Expression::String("web".to_string(), span.clone()),
                    Expression::String("db".to_string(), span.clone()),
                ],
                span.clone(),
            ),
            Some("custom_iter".to_string()),
            Some(vec![Expression::String("lbl".to_string(), span.clone())]),
            content,
            span.clone(),
            span,
        );
        root_body.dynamic_blocks.push(dyn_block);
        let mut ctx = Context::new();
        let expanded = expand_dynamic_blocks(&root_body, &mut ctx).unwrap();
        assert_eq!(expanded.blocks.len(), 2);
        assert_eq!(expanded.blocks[0].block_type, "tag");
        assert_eq!(expanded.blocks[0].labels, vec!["lbl"]);
        assert_eq!(expanded.blocks[1].block_type, "tag");
        assert_eq!(expanded.blocks[1].labels, vec!["lbl"]);
    }
    #[test]
    fn test_expand_dynamic_blocks_object_and_iterator() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut root_body = Body::new(span.clone());
        let dyn_block = DynamicBlock::new(
            "setting".to_string(),
            Expression::Object(
                vec![(
                    Expression::String("env".to_string(), span.clone()),
                    Expression::String("prod".to_string(), span.clone()),
                )],
                span.clone(),
            ),
            None,
            None,
            Body::new(span.clone()),
            span.clone(),
            span,
        );
        root_body.dynamic_blocks.push(dyn_block);
        let mut ctx = Context::new();
        let expanded = expand_dynamic_blocks(&root_body, &mut ctx).unwrap();
        assert_eq!(expanded.blocks.len(), 1);
        assert_eq!(expanded.blocks[0].block_type, "setting");
        assert_eq!(expanded.blocks[0].labels, [] as [String; 0]);
    }
    #[test]
    fn test_expand_dynamic_blocks_with_existing_regular_blocks() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut root_body = Body::new(span.clone());
        let regular_block = Block {
            block_type: "resource".to_string(),
            labels: vec!["aws_s3_bucket".to_string()],
            body: Body::new(span.clone()),
            span: span.clone(),
            type_span: span.clone(),
            label_spans: vec![span.clone()],
            open_brace_span: span.clone(),
            close_brace_span: span,
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        root_body.blocks.push(regular_block);
        let mut ctx = Context::new();
        let expanded = expand_dynamic_blocks(&root_body, &mut ctx).unwrap();
        assert_eq!(expanded.blocks.len(), 1);
        assert_eq!(expanded.blocks[0].block_type, "resource");
    }
    #[test]
    fn test_expand_dynamic_blocks_null_and_unknown() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut root_body = Body::new(span.clone());
        let dyn_null = DynamicBlock::new(
            "null_block".to_string(),
            Expression::Null(span.clone()),
            None,
            None,
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        );
        let dyn_unknown = DynamicBlock::new(
            "unknown_block".to_string(),
            Expression::Variable("unknown_var".to_string(), span.clone()),
            None,
            None,
            Body::new(span.clone()),
            span.clone(),
            span,
        );
        root_body.dynamic_blocks.push(dyn_null);
        root_body.dynamic_blocks.push(dyn_unknown);
        let mut ctx = Context::new();
        ctx.set_variable("unknown_var".to_string(), Value::unknown(Type::Dynamic));
        let expanded = expand_dynamic_blocks(&root_body, &mut ctx).unwrap();
        assert_eq!(expanded.blocks.len(), 1);
        assert_eq!(expanded.blocks[0].block_type, "unknown_block");
    }
    #[test]
    fn test_expand_dynamic_blocks_set() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut root_body = Body::new(span.clone());
        let dyn_block = DynamicBlock::new(
            "item".to_string(),
            Expression::Variable("my_set".to_string(), span.clone()),
            None,
            None,
            Body::new(span.clone()),
            span.clone(),
            span,
        );
        root_body.dynamic_blocks.push(dyn_block);
        let mut ctx = Context::new();
        let mut set = BTreeSet::new();
        set.insert(Value::new(
            Type::String,
            ValueData::String("elem".to_string()),
        ));
        ctx.set_variable(
            "my_set".to_string(),
            Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set)),
        );
        let expanded = expand_dynamic_blocks(&root_body, &mut ctx).unwrap();
        assert_eq!(expanded.blocks.len(), 1);
        assert_eq!(expanded.blocks[0].block_type, "item");
    }
    #[test]
    fn test_expand_dynamic_blocks_nested() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut root_body = Body::new(span.clone());
        let mut inner_content = Body::new(span.clone());
        let inner_dyn = DynamicBlock::new(
            "nested".to_string(),
            Expression::Tuple(
                vec![Expression::Number(
                    Number::from_str("1").unwrap(),
                    span.clone(),
                )],
                span.clone(),
            ),
            None,
            Some(vec![Expression::Number(
                Number::from_str("100").unwrap(),
                span.clone(),
            )]),
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        );
        inner_content.dynamic_blocks.push(inner_dyn);
        let outer_dyn = DynamicBlock::new(
            "outer".to_string(),
            Expression::Tuple(
                vec![Expression::String("a".to_string(), span.clone())],
                span.clone(),
            ),
            None,
            None,
            inner_content,
            span.clone(),
            span,
        );
        root_body.dynamic_blocks.push(outer_dyn);
        let mut ctx = Context::new();
        let expanded = expand_dynamic_blocks(&root_body, &mut ctx).unwrap();
        assert_eq!(expanded.blocks.len(), 1);
        assert_eq!(expanded.blocks[0].block_type, "outer");
        assert_eq!(expanded.blocks[0].body.blocks.len(), 1);
        assert_eq!(expanded.blocks[0].body.blocks[0].block_type, "nested");
        assert_eq!(expanded.blocks[0].body.blocks[0].labels, vec!["100"]);
    }
    #[test]
    fn test_expand_dynamic_blocks_errors() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut root_body = Body::new(span.clone());
        let dyn_bad_coll = DynamicBlock::new(
            "bad".to_string(),
            Expression::Number(Number::from_str("42").unwrap(), span.clone()),
            None,
            None,
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        );
        root_body.dynamic_blocks.push(dyn_bad_coll);
        let mut ctx = Context::new();
        let err = expand_dynamic_blocks(&root_body, &mut ctx);
        assert!(err.is_err());
        let mut root_body2 = Body::new(span.clone());
        let dyn_bad_lbl = DynamicBlock::new(
            "bad_lbl".to_string(),
            Expression::Tuple(
                vec![Expression::String("val".to_string(), span.clone())],
                span.clone(),
            ),
            None,
            Some(vec![Expression::Tuple(vec![], span.clone())]),
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        );
        root_body2.dynamic_blocks.push(dyn_bad_lbl);
        let err2 = expand_dynamic_blocks(&root_body2, &mut ctx);
        assert!(err2.is_err());
        let mut root_for_each_err = Body::new(span.clone());
        let dyn_for_each_err = DynamicBlock::new(
            "err".to_string(),
            Expression::Variable("undefined_variable_123".to_string(), span.clone()),
            None,
            None,
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        );
        root_for_each_err.dynamic_blocks.push(dyn_for_each_err);
        assert!(expand_dynamic_blocks(&root_for_each_err, &mut ctx).is_err());
        let mut root_lbl_eval_err = Body::new(span.clone());
        let dyn_lbl_eval_err = DynamicBlock::new(
            "err".to_string(),
            Expression::Tuple(
                vec![Expression::String("ok".to_string(), span.clone())],
                span.clone(),
            ),
            None,
            Some(vec![Expression::Variable(
                "undefined_lbl_var".to_string(),
                span.clone(),
            )]),
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        );
        root_lbl_eval_err.dynamic_blocks.push(dyn_lbl_eval_err);
        assert!(expand_dynamic_blocks(&root_lbl_eval_err, &mut ctx).is_err());
        let mut root_block_err = Body::new(span.clone());
        let mut inner_block_body = Body::new(span.clone());
        let bad_dyn = DynamicBlock::new(
            "bad".to_string(),
            Expression::Number(Number::from_str("42").unwrap(), span.clone()),
            None,
            None,
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        );
        inner_block_body.dynamic_blocks.push(bad_dyn);
        let regular_block = Block {
            block_type: "resource".to_string(),
            type_span: span.clone(),
            labels: vec![],
            label_spans: vec![],
            body: inner_block_body,
            span: span.clone(),
            open_brace_span: span.clone(),
            close_brace_span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        root_block_err.blocks.push(regular_block);
        assert!(expand_dynamic_blocks(&root_block_err, &mut ctx).is_err());
        let mut root_content_err = Body::new(span.clone());
        let mut bad_content = Body::new(span.clone());
        let bad_nested_dyn = DynamicBlock::new(
            "bad_nested".to_string(),
            Expression::Number(Number::from_str("42").unwrap(), span.clone()),
            None,
            None,
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        );
        bad_content.dynamic_blocks.push(bad_nested_dyn);
        let outer_dyn = DynamicBlock::new(
            "outer".to_string(),
            Expression::Tuple(
                vec![Expression::String("ok".to_string(), span.clone())],
                span.clone(),
            ),
            None,
            None,
            bad_content,
            span.clone(),
            span,
        );
        root_content_err.dynamic_blocks.push(outer_dyn);
        assert!(expand_dynamic_blocks(&root_content_err, &mut ctx).is_err());
    }
}
