//! AST Body Transformation Engine (`ext/transform` parity).
//!
//! Provides the `BodyTransformer` trait and standard transformers for rewriting
//! `Body` structures before they are decoded.

use crate::ast::structure::{Attribute, Block, Body, DynamicBlock};
use crate::error::HclError;
use std::ops::{Deref, DerefMut};

/// A trait for rewriting HCL `Body` elements (attributes, blocks, and dynamic blocks).
pub trait BodyTransformer {
    /// Transforms an attribute in-place.
    ///
    /// # Arguments
    /// * `attr` - The attribute to transform.
    ///
    /// # Errors
    /// Returns an `HclError` if the transformation fails.
    fn transform_attribute(&mut self, attr: &mut Attribute) -> Result<(), HclError> {
        let _ = attr;
        Ok(())
    }

    /// Transforms a block in-place.
    ///
    /// # Arguments
    /// * `block` - The block to transform.
    ///
    /// # Errors
    /// Returns an `HclError` if the transformation fails.
    fn transform_block(&mut self, block: &mut Block) -> Result<(), HclError> {
        let _ = block;
        Ok(())
    }

    /// Transforms a dynamic block in-place.
    ///
    /// # Arguments
    /// * `dyn_block` - The dynamic block to transform.
    ///
    /// # Errors
    /// Returns an `HclError` if the transformation fails.
    fn transform_dynamic_block(&mut self, dyn_block: &mut DynamicBlock) -> Result<(), HclError> {
        let _ = dyn_block;
        Ok(())
    }
}

/// A wrapper around a `Body` that applies a `BodyTransformer`.
#[derive(Debug, Clone)]
pub struct TransformedBody {
    /// The inner transformed body.
    pub body: Body,
}

impl TransformedBody {
    /// Creates a new `TransformedBody` by applying a transformer to the given body.
    ///
    /// # Arguments
    /// * `mut body` - The body to transform.
    /// * `transformer` - The transformer to apply.
    ///
    /// # Errors
    /// Returns an `HclError` if any step of the transformation fails.
    pub fn new<T: BodyTransformer + ?Sized>(
        mut body: Body,
        transformer: &mut T,
    ) -> Result<Self, HclError> {
        let mut new_attrs = std::collections::HashMap::new();
        for (_k, mut attr) in body.attributes {
            transformer.transform_attribute(&mut attr)?;
            new_attrs.insert(attr.name.clone(), attr);
        }
        body.attributes = new_attrs;

        let mut transformed_blocks = Vec::with_capacity(body.blocks.len());
        for mut block in body.blocks {
            transformer.transform_block(&mut block)?;
            // Apply recursively
            let inner_transformed = TransformedBody::new(block.body, transformer)?;
            block.body = inner_transformed.body;
            transformed_blocks.push(block);
        }
        body.blocks = transformed_blocks;

        let mut transformed_dyn = Vec::with_capacity(body.dynamic_blocks.len());
        for mut dyn_block in body.dynamic_blocks {
            transformer.transform_dynamic_block(&mut dyn_block)?;
            let inner_transformed = TransformedBody::new(dyn_block.content, transformer)?;
            dyn_block.content = inner_transformed.body;
            transformed_dyn.push(dyn_block);
        }
        body.dynamic_blocks = transformed_dyn;

        Ok(Self { body })
    }
}

impl Deref for TransformedBody {
    type Target = Body;

    fn deref(&self) -> &Self::Target {
        &self.body
    }
}

impl DerefMut for TransformedBody {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.body
    }
}

impl AsRef<Body> for TransformedBody {
    fn as_ref(&self) -> &Body {
        &self.body
    }
}

impl AsMut<Body> for TransformedBody {
    fn as_mut(&mut self) -> &mut Body {
        &mut self.body
    }
}

impl<T: BodyTransformer + ?Sized> BodyTransformer for &mut T {
    fn transform_attribute(&mut self, attr: &mut Attribute) -> Result<(), HclError> {
        (**self).transform_attribute(attr)
    }
    fn transform_block(&mut self, block: &mut Block) -> Result<(), HclError> {
        (**self).transform_block(block)
    }
    fn transform_dynamic_block(&mut self, dyn_block: &mut DynamicBlock) -> Result<(), HclError> {
        (**self).transform_dynamic_block(dyn_block)
    }
}

/// A transformer that renames attribute keys.
pub struct RenameAttributeTransformer<F>
where
    F: Fn(&str) -> Option<String>,
{
    renamer: F,
}

impl<F> RenameAttributeTransformer<F>
where
    F: Fn(&str) -> Option<String>,
{
    /// Creates a new `RenameAttributeTransformer`.
    ///
    /// # Arguments
    /// * `renamer` - A closure that optionally returns a new name for an attribute.
    pub fn new(renamer: F) -> Self {
        Self { renamer }
    }
}

impl<F> BodyTransformer for RenameAttributeTransformer<F>
where
    F: Fn(&str) -> Option<String>,
{
    fn transform_attribute(&mut self, attr: &mut Attribute) -> Result<(), HclError> {
        if let Some(new_name) = (self.renamer)(&attr.name) {
            attr.name = new_name;
        }
        Ok(())
    }
}

/// A transformer that renames block type identifiers or labels.
pub struct RenameBlockTransformer<F>
where
    F: Fn(&str, &[String]) -> Option<(String, Vec<String>)>,
{
    renamer: F,
}

impl<F> RenameBlockTransformer<F>
where
    F: Fn(&str, &[String]) -> Option<(String, Vec<String>)>,
{
    /// Creates a new `RenameBlockTransformer`.
    ///
    /// # Arguments
    /// * `renamer` - A closure that optionally returns a new block type and labels.
    pub fn new(renamer: F) -> Self {
        Self { renamer }
    }
}

impl<F> BodyTransformer for RenameBlockTransformer<F>
where
    F: Fn(&str, &[String]) -> Option<(String, Vec<String>)>,
{
    fn transform_block(&mut self, block: &mut Block) -> Result<(), HclError> {
        if let Some((new_type, new_labels)) = (self.renamer)(&block.block_type, &block.labels) {
            block.block_type = new_type;
            block.labels = new_labels;
        }
        Ok(())
    }
}

/// A transformer that conditionally removes attributes or blocks.
pub struct FilterTransformer<A, B, D>
where
    A: Fn(&Attribute) -> bool,
    B: Fn(&Block) -> bool,
    D: Fn(&DynamicBlock) -> bool,
{
    attr_filter: A,
    block_filter: B,
    dyn_block_filter: D,
}

impl<A, B, D> FilterTransformer<A, B, D>
where
    A: Fn(&Attribute) -> bool,
    B: Fn(&Block) -> bool,
    D: Fn(&DynamicBlock) -> bool,
{
    /// Creates a new `FilterTransformer`.
    ///
    /// # Arguments
    /// * `attr_filter` - Closure returning true if the attribute should be kept.
    /// * `block_filter` - Closure returning true if the block should be kept.
    /// * `dyn_block_filter` - Closure returning true if the dynamic block should be kept.
    pub fn new(attr_filter: A, block_filter: B, dyn_block_filter: D) -> Self {
        Self {
            attr_filter,
            block_filter,
            dyn_block_filter,
        }
    }
}

// FilterTransformer requires slightly different handling in TransformedBody,
// as BodyTransformer's trait only permits in-place mutation, not removal.
// We can achieve removal by hooking into the TransformedBody construction directly,
// or by marking items for removal (e.g. emptying their names or using a specific trait extension).
// Let's implement an extension to TransformedBody to handle filtering.

impl TransformedBody {
    /// Applies a `FilterTransformer` to remove elements.
    ///
    /// # Errors
    /// Returns an `HclError` if any step fails.
    pub fn filter<A, B, D>(
        mut body: Body,
        filter_transformer: &FilterTransformer<A, B, D>,
    ) -> Result<Self, HclError>
    where
        A: Fn(&Attribute) -> bool,
        B: Fn(&Block) -> bool,
        D: Fn(&DynamicBlock) -> bool,
    {
        body.attributes
            .retain(|_, attr| (filter_transformer.attr_filter)(attr));
        body.blocks
            .retain(|block| (filter_transformer.block_filter)(block));
        body.dynamic_blocks
            .retain(|dyn_block| (filter_transformer.dyn_block_filter)(dyn_block));

        let mut transformed_blocks = Vec::with_capacity(body.blocks.len());
        for mut block in body.blocks {
            let inner_transformed = TransformedBody::filter(block.body, filter_transformer)?;
            block.body = inner_transformed.body;
            transformed_blocks.push(block);
        }
        body.blocks = transformed_blocks;

        let mut transformed_dyn = Vec::with_capacity(body.dynamic_blocks.len());
        for mut dyn_block in body.dynamic_blocks {
            let inner_transformed = TransformedBody::filter(dyn_block.content, filter_transformer)?;
            dyn_block.content = inner_transformed.body;
            transformed_dyn.push(dyn_block);
        }
        body.dynamic_blocks = transformed_dyn;

        Ok(Self { body })
    }
}

/// A transformer that adds a namespace prefix to attributes and blocks.
pub struct PrefixTransformer {
    prefix: String,
}

impl PrefixTransformer {
    /// Creates a new `PrefixTransformer`.
    ///
    /// # Arguments
    /// * `prefix` - The prefix to prepend.
    #[must_use]
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

impl BodyTransformer for PrefixTransformer {
    fn transform_attribute(&mut self, attr: &mut Attribute) -> Result<(), HclError> {
        attr.name = format!("{}{}", self.prefix, attr.name);
        Ok(())
    }

    fn transform_block(&mut self, block: &mut Block) -> Result<(), HclError> {
        block.block_type = format!("{}{}", self.prefix, block.block_type);
        Ok(())
    }

    fn transform_dynamic_block(&mut self, dyn_block: &mut DynamicBlock) -> Result<(), HclError> {
        dyn_block.block_type = format!("{}{}", self.prefix, dyn_block.block_type);
        Ok(())
    }
}

/// Chains multiple `BodyTransformer` implementations in sequence.
pub struct CompositeTransformer<'a> {
    transformers: Vec<&'a mut dyn BodyTransformer>,
}

impl<'a> CompositeTransformer<'a> {
    /// Creates a new, empty `CompositeTransformer`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            transformers: Vec::new(),
        }
    }

    /// Adds a transformer to the chain.
    ///
    /// # Arguments
    /// * `transformer` - The transformer to add.
    pub fn add(&mut self, transformer: &'a mut dyn BodyTransformer) {
        self.transformers.push(transformer);
    }
}

impl Default for CompositeTransformer<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl BodyTransformer for CompositeTransformer<'_> {
    fn transform_attribute(&mut self, attr: &mut Attribute) -> Result<(), HclError> {
        for t in &mut self.transformers {
            t.transform_attribute(attr)?;
        }
        Ok(())
    }

    fn transform_block(&mut self, block: &mut Block) -> Result<(), HclError> {
        for t in &mut self.transformers {
            t.transform_block(block)?;
        }
        Ok(())
    }

    fn transform_dynamic_block(&mut self, dyn_block: &mut DynamicBlock) -> Result<(), HclError> {
        for t in &mut self.transformers {
            t.transform_dynamic_block(dyn_block)?;
        }
        Ok(())
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
    use super::*;
    use crate::ast::expr::Expression;
    use crate::ast::structure::Body;
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 1, 1, 1, 1)
    }

    #[test]
    fn test_rename_attribute() {
        let mut body = Body::new(dummy_span());
        body.attributes.insert(
            "untouched_key".to_string(),
            Attribute {
                name: "untouched_key".to_string(),
                expr: crate::ast::expr::Expression::Null(dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );
        body.attributes.insert(
            "old_key".to_string(),
            Attribute {
                name: "old_key".to_string(),
                expr: Expression::String("val".to_string(), dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );

        let mut renamer = RenameAttributeTransformer::new(|name| {
            if name == "old_key" {
                Some("new_key".to_string())
            } else {
                None
            }
        });

        let tb = TransformedBody::new(body, &mut renamer).unwrap();
        assert!(tb.body.attributes.contains_key("new_key"));
        assert!(!tb.body.attributes.contains_key("old_key"));
    }

    #[test]
    fn test_rename_block() {
        let mut body = Body::new(dummy_span());
        body.blocks.push(Block {
            block_type: "untouched_block".to_string(),
            labels: vec![],
            body: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
            label_spans: vec![],
            open_brace_span: dummy_span(),
            close_brace_span: dummy_span(),
            leading_comments: vec![],
            trailing_comment: None,
        });
        body.blocks.push(Block {
            block_type: "resource".to_string(),
            labels: vec!["aws_instance".to_string(), "web".to_string()],
            body: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
            label_spans: vec![dummy_span(), dummy_span()],
            open_brace_span: dummy_span(),
            close_brace_span: dummy_span(),
            leading_comments: vec![],
            trailing_comment: None,
        });

        let mut renamer = RenameBlockTransformer::new(|ty, labels| {
            if ty == "resource" && labels.len() == 2 && labels[0] == "aws_instance" {
                Some((
                    "compute_instance".to_string(),
                    vec!["gcp_instance".to_string(), labels[1].clone()],
                ))
            } else {
                None
            }
        });

        let tb = TransformedBody::new(body, &mut renamer).unwrap();
        assert_eq!(tb.body.blocks[0].block_type, "untouched_block");
        assert_eq!(tb.body.blocks[1].block_type, "compute_instance");
        assert_eq!(
            tb.body.blocks[1].labels,
            vec!["gcp_instance".to_string(), "web".to_string()]
        );
    }

    #[test]
    fn test_filter_transformer() {
        let mut body = Body::new(dummy_span());
        let mut db = crate::ast::structure::DynamicBlock {
            iterator: None,
            labels: None,
            content: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
            for_each: crate::ast::expr::Expression::Null(dummy_span()),
            block_type: "b".into(),
        };
        db.content.attributes.insert(
            "keep".into(),
            Attribute {
                name: "keep".into(),
                expr: crate::ast::expr::Expression::Null(dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );
        db.content.attributes.insert(
            "drop".into(),
            Attribute {
                name: "drop".into(),
                expr: crate::ast::expr::Expression::Null(dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );
        body.dynamic_blocks.push(db);

        body.attributes.insert(
            "keep".to_string(),
            Attribute {
                name: "keep".to_string(),
                expr: Expression::String("".to_string(), dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );
        body.attributes.insert(
            "drop".to_string(),
            Attribute {
                name: "drop".to_string(),
                expr: Expression::String("".to_string(), dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );

        let mut block_keep = Block {
            block_type: "keep_block".to_string(),
            labels: vec![],
            body: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
            label_spans: vec![],
            open_brace_span: dummy_span(),
            close_brace_span: dummy_span(),
            leading_comments: vec![],
            trailing_comment: None,
        };
        block_keep.body.attributes.insert(
            "drop".to_string(),
            Attribute {
                name: "drop".to_string(),
                expr: Expression::String("".to_string(), dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );
        body.blocks.push(block_keep);

        let block_drop = Block {
            block_type: "drop_block".to_string(),
            labels: vec![],
            body: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
            label_spans: vec![],
            open_brace_span: dummy_span(),
            close_brace_span: dummy_span(),
            leading_comments: vec![],
            trailing_comment: None,
        };
        body.blocks.push(block_drop);

        let filter = FilterTransformer::new(
            |attr| attr.name != "drop",
            |block| block.block_type != "drop_block",
            |_dyn_block| true,
        );

        let tb = TransformedBody::filter(body, &filter).unwrap();
        assert!(tb.body.attributes.contains_key("keep"));
        assert!(!tb.body.attributes.contains_key("drop"));
        assert_eq!(tb.body.blocks.len(), 1);
        assert_eq!(tb.body.blocks[0].block_type, "keep_block");
        assert!(!tb.body.blocks[0].body.attributes.contains_key("drop"));
    }

    #[test]
    fn test_prefix_transformer() {
        let mut body = Body::new(dummy_span());
        body.attributes.insert(
            "my_attr".to_string(),
            Attribute {
                name: "my_attr".to_string(),
                expr: Expression::String("".to_string(), dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );
        body.blocks.push(Block {
            block_type: "my_block".to_string(),
            labels: vec![],
            body: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
            label_spans: vec![],
            open_brace_span: dummy_span(),
            close_brace_span: dummy_span(),
            leading_comments: vec![],
            trailing_comment: None,
        });

        let dyn_block = DynamicBlock {
            block_type: "my_dyn".to_string(),
            for_each: Expression::String("".to_string(), dummy_span()),
            iterator: None,
            labels: None,
            content: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
        };
        body.dynamic_blocks.push(dyn_block);

        let mut prefixer = PrefixTransformer::new("test_");
        let tb = TransformedBody::new(body, &mut prefixer).unwrap();

        assert!(tb.body.attributes.contains_key("test_my_attr"));
        assert_eq!(tb.body.blocks[0].block_type, "test_my_block");
        assert_eq!(tb.body.dynamic_blocks[0].block_type, "test_my_dyn");
    }

    #[test]
    fn test_transformed_body_traits() {
        let mut body = Body::new(dummy_span());
        body.attributes.insert(
            "k".into(),
            Attribute {
                name: "k".into(),
                expr: crate::ast::expr::Expression::Null(dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );
        let mut tb = TransformedBody::new(body, &mut PrefixTransformer::new("pre_")).unwrap();
        use std::ops::{Deref, DerefMut};
        assert_eq!(tb.deref().attributes.keys().next().unwrap(), "pre_k");
        tb.deref_mut().attributes.clear();
        assert!(tb.as_ref().attributes.is_empty());
        tb.as_mut().attributes.clear();
    }

    #[test]
    fn test_composite_transformer() {
        let mut body = Body::new(dummy_span());
        body.attributes.insert(
            "attr1".to_string(),
            Attribute {
                name: "attr1".to_string(),
                expr: Expression::String("".to_string(), dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );
        body.blocks.push(Block {
            block_type: "block1".to_string(),
            labels: vec![],
            body: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
            label_spans: vec![],
            open_brace_span: dummy_span(),
            close_brace_span: dummy_span(),
            leading_comments: vec![],
            trailing_comment: None,
        });

        let dyn_block = DynamicBlock {
            block_type: "dyn1".to_string(),
            for_each: Expression::String("".to_string(), dummy_span()),
            iterator: None,
            labels: None,
            content: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
        };
        body.dynamic_blocks.push(dyn_block);

        let mut prefix1 = PrefixTransformer::new("a_");
        let mut prefix2 = PrefixTransformer::new("b_");

        let mut composite = CompositeTransformer::default();
        composite.add(&mut prefix1);
        composite.add(&mut prefix2);

        let tb = TransformedBody::new(body, &mut composite).unwrap();

        assert!(tb.body.attributes.contains_key("b_a_attr1"));
        assert_eq!(tb.body.blocks[0].block_type, "b_a_block1");
        assert_eq!(tb.body.dynamic_blocks[0].block_type, "b_a_dyn1");
    }

    struct ErrorTransformer;
    impl BodyTransformer for ErrorTransformer {
        fn transform_attribute(&mut self, _attr: &mut Attribute) -> Result<(), HclError> {
            Err(HclError::Parse("error".to_string()))
        }
        fn transform_block(&mut self, _block: &mut Block) -> Result<(), HclError> {
            Err(HclError::Parse("error".to_string()))
        }
        fn transform_dynamic_block(
            &mut self,
            _dyn_block: &mut DynamicBlock,
        ) -> Result<(), HclError> {
            Err(HclError::Parse("error".to_string()))
        }
    }

    #[test]
    fn test_error_propagation() {
        let mut body1 = Body::new(dummy_span());
        body1.attributes.insert(
            "a".to_string(),
            Attribute {
                name: "a".to_string(),
                expr: Expression::String("".to_string(), dummy_span()),
                span: dummy_span(),
                name_span: dummy_span(),
                equals_span: dummy_span(),
                leading_comments: vec![],
                trailing_comment: None,
            },
        );

        let mut body2 = Body::new(dummy_span());
        body2.blocks.push(Block {
            block_type: "b".to_string(),
            labels: vec![],
            body: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
            label_spans: vec![],
            open_brace_span: dummy_span(),
            close_brace_span: dummy_span(),
            leading_comments: vec![],
            trailing_comment: None,
        });

        let mut body3 = Body::new(dummy_span());
        body3.dynamic_blocks.push(DynamicBlock {
            block_type: "d".to_string(),
            for_each: Expression::String("".to_string(), dummy_span()),
            iterator: None,
            labels: None,
            content: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
        });

        assert!(TransformedBody::new(body1, &mut ErrorTransformer).is_err());
        assert!(TransformedBody::new(body2, &mut ErrorTransformer).is_err());
        assert!(TransformedBody::new(body3, &mut ErrorTransformer).is_err());
    }

    #[test]
    fn test_body_transformer_default_methods() {
        struct DummyTransformer;
        impl BodyTransformer for DummyTransformer {}

        let mut d = DummyTransformer;
        let mut attr = Attribute {
            name: "a".to_string(),
            expr: Expression::String("".to_string(), dummy_span()),
            span: dummy_span(),
            name_span: dummy_span(),
            equals_span: dummy_span(),
            leading_comments: vec![],
            trailing_comment: None,
        };
        assert!(d.transform_attribute(&mut attr).is_ok());

        let mut block = Block {
            block_type: "b".to_string(),
            labels: vec![],
            body: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
            label_spans: vec![],
            open_brace_span: dummy_span(),
            close_brace_span: dummy_span(),
            leading_comments: vec![],
            trailing_comment: None,
        };
        assert!(d.transform_block(&mut block).is_ok());

        let mut dyn_block = DynamicBlock {
            block_type: "d".to_string(),
            for_each: Expression::String("".to_string(), dummy_span()),
            iterator: None,
            labels: None,
            content: Body::new(dummy_span()),
            span: dummy_span(),
            type_span: dummy_span(),
        };
        assert!(d.transform_dynamic_block(&mut dyn_block).is_ok());
    }
}
