//! Schema definitions and content extraction for HCL bodies.
//!
//! Provides [`BodySchema`], [`AttributeSchema`], [`BlockHeaderSchema`], and [`BodyContent`]
//! for validating and partitioning HCL bodies against declarative structural schemas.
use crate::ast::structure::{Attribute, Block, Body};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::error::HclError;
use crate::span::Span;
pub use hcl_macros::ImpliedBodySchema;
use std::collections::HashMap;
/// Specification for an expected attribute in an HCL body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeSchema {
    /// The attribute name.
    pub name: String,
    /// Whether the attribute must be defined in the body.
    pub required: bool,
    /// Optional documentation description for this attribute.
    pub description: Option<String>,
    /// Optional expected static type for this attribute.
    pub expected_type: Option<crate::types::Type>,
}
impl AttributeSchema {
    /// Creates a new `AttributeSchema`.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    /// * `required` - Whether the attribute is required.
    #[must_use]
    pub fn new(name: impl Into<String>, required: bool) -> Self {
        Self {
            name: name.into(),
            required,
            description: None,
            expected_type: None,
        }
    }
    /// Creates a required `AttributeSchema`.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    #[must_use]
    pub fn required(name: impl Into<String>) -> Self {
        Self::new(name, true)
    }
    /// Creates an optional `AttributeSchema`.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    #[must_use]
    pub fn optional(name: impl Into<String>) -> Self {
        Self::new(name, false)
    }
    /// Sets the expected static type for this attribute.
    ///
    /// # Arguments
    /// * `ty` - The expected HCL type.
    #[must_use]
    pub fn with_type(mut self, ty: crate::types::Type) -> Self {
        self.expected_type = Some(ty);
        self
    }
    /// Sets documentation comments or description for the attribute schema.
    ///
    /// # Arguments
    /// * `desc` - The description string.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}
/// Specification for an expected block header in an HCL body.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockHeaderSchema {
    /// The block type identifier (e.g. `"resource"`, `"variable"`).
    pub block_type: String,
    /// The expected label names (e.g. `["type", "name"]`).
    pub label_names: Vec<String>,
    /// Optional expected schema for the body of this block.
    pub body_schema: Option<BodySchema>,
    /// Optional documentation description for this block.
    pub description: Option<String>,
}
impl BlockHeaderSchema {
    /// Creates a new `BlockHeaderSchema`.
    ///
    /// # Arguments
    /// * `block_type` - The block type identifier.
    /// * `label_names` - The names of labels expected on the block.
    #[must_use]
    pub fn new(block_type: impl Into<String>, label_names: Vec<String>) -> Self {
        Self {
            block_type: block_type.into(),
            label_names,
            body_schema: None,
            description: None,
        }
    }
    /// Sets the expected inner body schema for this block.
    ///
    /// # Arguments
    /// * `schema` - The inner body schema.
    #[must_use]
    pub fn with_body_schema(mut self, schema: BodySchema) -> Self {
        self.body_schema = Some(schema);
        self
    }
    /// Sets documentation comments or description for this block schema.
    ///
    /// # Arguments
    /// * `desc` - The description string.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}
/// A declarative schema describing the attributes and blocks expected in an HCL body.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BodySchema {
    /// Expected attributes mapped by name.
    pub attributes: HashMap<String, AttributeSchema>,
    /// Expected blocks mapped by block type.
    pub blocks: HashMap<String, BlockHeaderSchema>,
}
impl BodySchema {
    /// Creates a new, empty `BodySchema`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Adds an expected attribute specification to the schema.
    ///
    /// # Arguments
    /// * `attr` - The attribute schema to add.
    #[must_use]
    pub fn with_attribute(mut self, attr: AttributeSchema) -> Self {
        self.attributes.insert(attr.name.clone(), attr);
        self
    }
    /// Adds an expected block specification to the schema.
    ///
    /// # Arguments
    /// * `block` - The block schema to add.
    #[must_use]
    pub fn with_block(mut self, block: BlockHeaderSchema) -> Self {
        self.blocks.insert(block.block_type.clone(), block);
        self
    }
}
/// A trait for types that can automatically derive an implied [`BodySchema`].
pub trait ImpliedBodySchema {
    /// Returns the implied schema describing attributes and blocks for this type.
    #[must_use]
    fn implied_body_schema() -> BodySchema;
    /// Returns the names of labels defined on this type (if any).
    #[must_use]
    fn implied_label_names() -> Vec<String> {
        Vec::new()
    }
}
impl ImpliedBodySchema for crate::ast::structure::Body {
    fn implied_body_schema() -> BodySchema {
        BodySchema::new()
    }
}
/// The content extracted from an HCL body after matching against a [`BodySchema`].
#[derive(Debug, Clone, PartialEq)]
pub struct BodyContent {
    /// Attributes that matched the schema.
    pub attributes: HashMap<String, Attribute>,
    /// Blocks that matched the schema.
    pub blocks: Vec<Block>,
    /// The source span of the content.
    pub span: Span,
}
impl BodyContent {
    /// Creates a new `BodyContent`.
    ///
    /// # Arguments
    /// * `attributes` - The matched attributes.
    /// * `blocks` - The matched blocks.
    /// * `span` - The source span.
    #[must_use]
    pub fn new(attributes: HashMap<String, Attribute>, blocks: Vec<Block>, span: Span) -> Self {
        Self {
            attributes,
            blocks,
            span,
        }
    }
    /// Retrieves an attribute by name if present.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    #[must_use]
    pub fn get_attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes.get(name)
    }
    /// Finds all blocks of a given type.
    ///
    /// # Arguments
    /// * `block_type` - The block type to filter by.
    #[must_use]
    pub fn find_blocks(&self, block_type: &str) -> Vec<&Block> {
        self.blocks
            .iter()
            .filter(|b| b.block_type == block_type)
            .collect()
    }
}
/// Validates an AST [`Body`] against a [`BodySchema`], returning [`BodyContent`] or accumulated [`Diagnostics`].
///
/// If unexpected attributes or block types are encountered, typo suggestions are provided in the
/// diagnostic detail if matching schema definitions exist within an edit distance of 2.
///
/// # Arguments
/// * `body` - The AST body to validate.
/// * `schema` - The schema specification to match against.
///
/// # Errors
/// Returns [`Diagnostics`] containing validation errors and warnings.
pub fn validate_body(body: &Body, schema: &BodySchema) -> Result<BodyContent, Diagnostics> {
    body.content(schema)
}
impl Body {
    /// Matches and validates this body against a [`BodySchema`].
    ///
    /// Emits diagnostic errors for any unexpected attributes, missing required attributes,
    /// unexpected blocks, or block label count mismatches.
    ///
    /// # Arguments
    /// * `schema` - The schema to validate against.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if validation fails.
    pub fn content(&self, schema: &BodySchema) -> Result<BodyContent, Diagnostics> {
        let mut diags = Diagnostics::new();
        for (name, attr) in &self.attributes {
            if !schema.attributes.contains_key(name) {
                let mut diag = Diagnostic::new(
                    HclError::Schema(format!("An argument named {name:?} is not expected here.")),
                    attr.name_span.clone(),
                );
                if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                    name,
                    schema.attributes.keys().map(String::as_str),
                    2,
                ) {
                    diag = diag.with_detail(format!("Did you mean \"{candidate}\"?"));
                }
                diags.push(diag);
            }
        }
        for attr_schema in schema.attributes.values() {
            if attr_schema.required && !self.attributes.contains_key(&attr_schema.name) {
                diags.push(Diagnostic::new(
                    HclError::Schema(format!(
                        "The argument {:?} is required, but no definition was found.",
                        attr_schema.name
                    )),
                    self.span.clone(),
                ));
            }
        }
        for block in &self.blocks {
            if let Some(block_schema) = schema.blocks.get(&block.block_type) {
                let expected_count = block_schema.label_names.len();
                let actual_count = block.labels.len();
                if actual_count != expected_count {
                    diags.push(Diagnostic::new(
                        HclError::Schema(format!(
                            "Wrong number of labels for block {:?}: expected {}, got {}.",
                            block.block_type, expected_count, actual_count
                        )),
                        block.type_span.clone(),
                    ));
                }
                if let Some(inner_schema) = &block_schema.body_schema {
                    if let Err(inner_diags) = block.body.content(inner_schema) {
                        diags.extend(inner_diags);
                    }
                }
            } else {
                let mut diag = Diagnostic::new(
                    HclError::Schema(format!(
                        "Blocks of type {:?} are not expected here.",
                        block.block_type
                    )),
                    block.type_span.clone(),
                );
                if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                    &block.block_type,
                    schema.blocks.keys().map(String::as_str),
                    2,
                ) {
                    diag = diag.with_detail(format!("Did you mean \"{candidate}\"?"));
                }
                diags.push(diag);
            }
        }
        if diags.has_errors() {
            Err(diags)
        } else {
            Ok(BodyContent::new(
                self.attributes.clone(),
                self.blocks.clone(),
                self.span.clone(),
            ))
        }
    }
    /// Partitions this body into recognized [`BodyContent`] and leftover [`Body`] (`remain`).
    ///
    /// Recognized attributes and blocks are matched according to the provided [`BodySchema`].
    /// Any unrecognized attributes or blocks are placed in the returned `remain` body without error.
    /// Required attributes in the schema must still be present.
    ///
    /// # Arguments
    /// * `schema` - The schema identifying the recognized content.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if a required attribute is missing or if recognized blocks have label count errors.
    pub fn partial_content(&self, schema: &BodySchema) -> Result<(BodyContent, Body), Diagnostics> {
        let mut diags = Diagnostics::new();
        let mut matched_attributes = HashMap::new();
        let mut remain_attributes = HashMap::new();
        for (name, attr) in &self.attributes {
            if schema.attributes.contains_key(name) {
                matched_attributes.insert(name.clone(), attr.clone());
            } else {
                remain_attributes.insert(name.clone(), attr.clone());
            }
        }
        for attr_schema in schema.attributes.values() {
            if attr_schema.required && !matched_attributes.contains_key(&attr_schema.name) {
                diags.push(Diagnostic::new(
                    HclError::Schema(format!(
                        "The argument {:?} is required, but no definition was found.",
                        attr_schema.name
                    )),
                    self.span.clone(),
                ));
            }
        }
        let mut matched_blocks = Vec::new();
        let mut remain_blocks = Vec::new();
        for block in &self.blocks {
            if let Some(block_schema) = schema.blocks.get(&block.block_type) {
                let expected_count = block_schema.label_names.len();
                let actual_count = block.labels.len();
                if actual_count != expected_count {
                    diags.push(Diagnostic::new(
                        HclError::Schema(format!(
                            "Wrong number of labels for block {:?}: expected {}, got {}.",
                            block.block_type, expected_count, actual_count
                        )),
                        block.type_span.clone(),
                    ));
                }
                if let Some(inner_schema) = &block_schema.body_schema {
                    if let Err(inner_diags) = block.body.content(inner_schema) {
                        diags.extend(inner_diags);
                    }
                }
                matched_blocks.push(block.clone());
            } else {
                remain_blocks.push(block.clone());
            }
        }
        let mut remain_body = Body::new(self.span.clone());
        remain_body.attributes = remain_attributes;
        remain_body.blocks = remain_blocks;
        remain_body.functions.clone_from(&self.functions);
        remain_body.dynamic_blocks.clone_from(&self.dynamic_blocks);
        remain_body.validations.clone_from(&self.validations);
        remain_body.preconditions.clone_from(&self.preconditions);
        remain_body.postconditions.clone_from(&self.postconditions);
        if diags.has_errors() {
            Err(diags)
        } else {
            Ok((
                BodyContent::new(matched_attributes, matched_blocks, self.span.clone()),
                remain_body,
            ))
        }
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
    use crate::span::Span;
    #[test]
    fn test_schema_builders_and_properties() {
        let attr = AttributeSchema::required("name").with_description("The resource name");
        assert_eq!(attr.name, "name");
        assert!(attr.required);
        assert_eq!(attr.description.as_deref(), Some("The resource name"));
        let opt_attr = AttributeSchema::optional("enabled").with_description("Enable toggle");
        assert_eq!(opt_attr.name, "enabled");
        assert!(!opt_attr.required);
        let inner_schema = BodySchema::new().with_attribute(attr.clone());
        let block_schema = BlockHeaderSchema::new("service", vec!["name".to_string()])
            .with_body_schema(inner_schema)
            .with_description("Service definition");
        assert_eq!(block_schema.block_type, "service");
        assert_eq!(block_schema.label_names, vec!["name"]);
        assert!(block_schema.body_schema.is_some());
        assert_eq!(
            block_schema.description.as_deref(),
            Some("Service definition")
        );
        let body_schema = BodySchema::new()
            .with_attribute(opt_attr)
            .with_block(block_schema);
        assert_eq!(body_schema.attributes.len(), 1);
        assert_eq!(body_schema.blocks.len(), 1);
    }
    #[test]
    fn test_body_content_methods() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut attrs = HashMap::new();
        attrs.insert(
            "foo".to_string(),
            Attribute {
                name: "foo".to_string(),
                name_span: span.clone(),
                equals_span: span.clone(),
                expr: Expression::Bool(true, span.clone()),
                span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        let blocks = vec![Block {
            block_type: "item".to_string(),
            type_span: span.clone(),
            labels: vec![],
            label_spans: vec![],
            body: Body::new(span.clone()),
            span: span.clone(),
            open_brace_span: span.clone(),
            close_brace_span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        }];
        let content = BodyContent::new(attrs, blocks, span);
        assert!(content.get_attribute("foo").is_some());
        assert!(content.get_attribute("bar").is_none());
        assert_eq!(content.find_blocks("item").len(), 1);
        assert_eq!(content.find_blocks("missing").len(), 0);
    }
    #[test]
    fn test_body_content_success_and_errors() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut body = Body::new(span.clone());
        body.attributes.insert(
            "name".to_string(),
            Attribute {
                name: "name".to_string(),
                name_span: span.clone(),
                equals_span: span.clone(),
                expr: Expression::String("test".to_string(), span.clone()),
                span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        let schema = BodySchema::new()
            .with_attribute(AttributeSchema::required("name"))
            .with_attribute(AttributeSchema::required("count"));
        let err = body.content(&schema);
        assert!(err.is_err());
        assert!(
            err.unwrap_err().errors()[0]
                .error
                .to_string()
                .contains(r#"The argument "count" is required"#)
        );
        let mut body2 = Body::new(span.clone());
        body2.attributes.insert(
            "unexpected".to_string(),
            Attribute {
                name: "unexpected".to_string(),
                name_span: span.clone(),
                equals_span: span.clone(),
                expr: Expression::Bool(true, span.clone()),
                span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        let empty_schema = BodySchema::new();
        let err2 = body2.content(&empty_schema);
        assert!(err2.is_err());
        assert!(
            err2.unwrap_err().errors()[0]
                .error
                .to_string()
                .contains(r#"An argument named "unexpected" is not expected here."#)
        );
        let mut body3 = Body::new(span.clone());
        body3.blocks.push(Block {
            block_type: "server".to_string(),
            type_span: span.clone(),
            labels: vec!["alpha".to_string(), "beta".to_string()],
            label_spans: vec![span.clone(), span.clone()],
            body: Body::new(span.clone()),
            span: span.clone(),
            open_brace_span: span.clone(),
            close_brace_span: span,
            leading_comments: Vec::new(),
            trailing_comment: None,
        });
        let schema3 = BodySchema::new()
            .with_block(BlockHeaderSchema::new("server", vec!["name".to_string()]));
        let err3 = body3.content(&schema3);
        assert!(err3.is_err());
        assert!(
            err3.unwrap_err().errors()[0]
                .error
                .to_string()
                .contains("Wrong number of labels")
        );
        let err_unexp_block = body3.content(&empty_schema);
        assert!(err_unexp_block.is_err());
        assert!(
            err_unexp_block.unwrap_err().errors()[0]
                .error
                .to_string()
                .contains(r#"Blocks of type "server" are not expected here."#)
        );
    }
    #[test]
    fn test_body_partial_content_partitioning() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut body = Body::new(span.clone());
        body.attributes.insert(
            "known_attr".to_string(),
            Attribute {
                name: "known_attr".to_string(),
                name_span: span.clone(),
                equals_span: span.clone(),
                expr: Expression::Bool(true, span.clone()),
                span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        body.attributes.insert(
            "unknown_attr".to_string(),
            Attribute {
                name: "unknown_attr".to_string(),
                name_span: span.clone(),
                equals_span: span.clone(),
                expr: Expression::String("remain".to_string(), span.clone()),
                span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        body.blocks.push(Block {
            block_type: "known_block".to_string(),
            type_span: span.clone(),
            labels: vec!["lbl1".to_string()],
            label_spans: vec![span.clone()],
            body: Body::new(span.clone()),
            span: span.clone(),
            open_brace_span: span.clone(),
            close_brace_span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        });
        body.blocks.push(Block {
            block_type: "unknown_block".to_string(),
            type_span: span.clone(),
            labels: vec![],
            label_spans: vec![],
            body: Body::new(span.clone()),
            span: span.clone(),
            open_brace_span: span.clone(),
            close_brace_span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        });
        let schema = BodySchema::new()
            .with_attribute(AttributeSchema::required("known_attr"))
            .with_block(BlockHeaderSchema::new(
                "known_block",
                vec!["label".to_string()],
            ));
        let (content, remain) = body.partial_content(&schema).unwrap();
        assert_eq!(content.attributes.len(), 1);
        assert!(content.attributes.contains_key("known_attr"));
        assert_eq!(content.blocks.len(), 1);
        assert_eq!(content.blocks[0].block_type, "known_block");
        assert_eq!(remain.attributes.len(), 1);
        assert!(remain.attributes.contains_key("unknown_attr"));
        assert_eq!(remain.blocks.len(), 1);
        assert_eq!(remain.blocks[0].block_type, "unknown_block");
        assert_eq!(remain.span, span);
        let schema2 = BodySchema::new()
            .with_attribute(AttributeSchema::required("unknown_attr"))
            .with_block(BlockHeaderSchema::new("unknown_block", vec![]));
        let (content2, remain2) = remain.partial_content(&schema2).unwrap();
        assert_eq!(content2.attributes.len(), 1);
        assert_eq!(content2.blocks.len(), 1);
        assert_eq!(remain2.attributes.len(), 0);
        assert_eq!(remain2.blocks, [] as [Block; 0]);
    }
    #[test]
    fn test_body_nested_body_schema_validation() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut inner_body = Body::new(span.clone());
        inner_body.attributes.insert(
            "bad_attr".to_string(),
            Attribute {
                name: "bad_attr".to_string(),
                name_span: span.clone(),
                equals_span: span.clone(),
                expr: Expression::Bool(false, span.clone()),
                span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        let mut root_body = Body::new(span.clone());
        root_body.blocks.push(Block {
            block_type: "outer".to_string(),
            type_span: span.clone(),
            labels: vec![],
            label_spans: vec![],
            body: inner_body,
            span: span.clone(),
            open_brace_span: span.clone(),
            close_brace_span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        });
        let inner_schema =
            BodySchema::new().with_attribute(AttributeSchema::required("expected_attr"));
        let root_schema = BodySchema::new()
            .with_block(BlockHeaderSchema::new("outer", vec![]).with_body_schema(inner_schema));
        let err = root_body.content(&root_schema);
        assert!(err.is_err());
        let errs = err.unwrap_err();
        assert!(errs.errors().len() >= 2);
        let err_partial = root_body.partial_content(&root_schema);
        assert!(err_partial.is_err());
    }
    #[test]
    fn test_schema_coverage_comprehensive() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut inner_body = Body::new(span.clone());
        inner_body.attributes.insert(
            "nested_attr".to_string(),
            Attribute {
                name: "nested_attr".to_string(),
                name_span: span.clone(),
                equals_span: span.clone(),
                expr: Expression::Bool(true, span.clone()),
                span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        let mut body = Body::new(span.clone());
        body.attributes.insert(
            "req_attr".to_string(),
            Attribute {
                name: "req_attr".to_string(),
                name_span: span.clone(),
                equals_span: span.clone(),
                expr: Expression::String("value".to_string(), span.clone()),
                span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        body.blocks.push(Block {
            block_type: "service".to_string(),
            type_span: span.clone(),
            labels: vec!["api".to_string()],
            label_spans: vec![span.clone()],
            body: inner_body,
            span: span.clone(),
            open_brace_span: span.clone(),
            close_brace_span: span,
            leading_comments: Vec::new(),
            trailing_comment: None,
        });
        let inner_schema =
            BodySchema::new().with_attribute(AttributeSchema::required("nested_attr"));
        let schema = BodySchema::new()
            .with_attribute(AttributeSchema::required("req_attr"))
            .with_attribute(AttributeSchema::optional("opt_attr"))
            .with_block(
                BlockHeaderSchema::new("service", vec!["name".to_string()])
                    .with_body_schema(inner_schema.clone()),
            );
        let content = body.content(&schema).unwrap();
        assert_eq!(content.attributes.len(), 1);
        assert_eq!(content.blocks.len(), 1);
        let bad_schema = BodySchema::new().with_attribute(AttributeSchema::required("missing_req"));
        let err_partial = body.partial_content(&bad_schema);
        assert!(err_partial.is_err());
        let bad_block_schema = BodySchema::new().with_block(BlockHeaderSchema::new(
            "service",
            vec!["l1".to_string(), "l2".to_string()],
        ));
        let err_labels = body.partial_content(&bad_block_schema);
        assert!(err_labels.is_err());
        let (p_content, remain) = body.partial_content(&schema).unwrap();
        assert_eq!(p_content.attributes.len(), 1);
        assert_eq!(p_content.blocks.len(), 1);
        assert_eq!(remain.attributes.len(), 0);
        assert_eq!(remain.blocks.len(), 0);
    }
    #[derive(Debug, PartialEq, ImpliedBodySchema)]
    struct SubBlock {
        #[hcl(label)]
        id: String,
        count: u32,
    }
    #[derive(Debug, PartialEq, ImpliedBodySchema)]
    struct AppConfig {
        name: String,
        description: Option<String>,
        #[hcl(block)]
        sub: SubBlock,
        #[hcl(block)]
        replicas: Vec<SubBlock>,
    }
    #[test]
    fn test_implied_body_schema_derive_and_json_integration() {
        let schema = AppConfig::implied_body_schema();
        assert_eq!(schema.attributes.len(), 2);
        assert!(schema.attributes["name"].required);
        assert!(!schema.attributes["description"].required);
        assert_eq!(schema.blocks.len(), 2);
        let sub_schema = &schema.blocks["sub"];
        assert_eq!(sub_schema.label_names, vec!["id"]);
        let inner = sub_schema.body_schema.as_ref().unwrap();
        assert!(inner.attributes["count"].required);
        let replicas_schema = &schema.blocks["replicas"];
        assert_eq!(replicas_schema.label_names, vec!["id"]);
        let json_str = r#"{
            "name": "my-app",
            "sub": {
                "sub-primary": {
                    "count": 5
                }
            },
            "replicas": [
                {
                    "rep-1": {
                        "count": 10
                    }
                },
                {
                    "rep-2": {
                        "count": 20
                    }
                }
            ]
        }"#;
        let body = crate::parse::json::parse_json_with_schema(json_str, &schema).unwrap();
        assert!(body.attributes.contains_key("name"));
        assert_eq!(body.blocks.len(), 3);
        let sub_block = body.blocks.iter().find(|b| b.block_type == "sub").unwrap();
        assert_eq!(sub_block.labels, vec!["sub-primary"]);
        assert!(sub_block.body.attributes.contains_key("count"));
        let replica_blocks: Vec<_> = body
            .blocks
            .iter()
            .filter(|b| b.block_type == "replicas")
            .collect();
        assert_eq!(replica_blocks.len(), 2);
        assert_eq!(replica_blocks[0].labels, vec!["rep-1"]);
        assert_eq!(replica_blocks[1].labels, vec!["rep-2"]);
    }
    #[test]
    fn test_implied_label_names_default() {
        struct OnlySchema;
        impl ImpliedBodySchema for OnlySchema {
            fn implied_body_schema() -> BodySchema {
                BodySchema::new()
            }
        }
        let schema = OnlySchema::implied_body_schema();
        assert!(schema.attributes.is_empty());
        let labels = OnlySchema::implied_label_names();
        assert_eq!(labels, Vec::<String>::new());
    }
    #[test]
    fn test_attribute_and_block_typo_suggestions() {
        let schema = BodySchema::new()
            .with_attribute(AttributeSchema::new("username", true))
            .with_attribute(AttributeSchema::new("password", false))
            .with_block(BlockHeaderSchema::new("service", vec![]));
        let src = r#"
            usrname = "alice"
            servce {
            }
        "#;
        let body = crate::api::parse(src).unwrap();
        let err = validate_body(&body, &schema).err().unwrap();
        let attr_err = err
            .errors()
            .iter()
            .find(|d| d.summary_str().contains("usrname"))
            .unwrap();
        assert_eq!(
            attr_err.detail.as_deref(),
            Some("Did you mean \"username\"?")
        );
        let block_err = err
            .errors()
            .iter()
            .find(|d| d.summary_str().contains("servce"))
            .unwrap();
        assert_eq!(
            block_err.detail.as_deref(),
            Some("Did you mean \"service\"?")
        );
    }
    #[test]
    fn test_implied_body_schema_for_body() {
        let s = <crate::ast::structure::Body as ImpliedBodySchema>::implied_body_schema();
        assert_eq!(s, BodySchema::new());
    }
}
