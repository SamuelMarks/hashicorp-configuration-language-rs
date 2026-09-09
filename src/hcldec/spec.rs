//! Schema Definitions for Dynamic Schemas (`hcldec`).
//!
//! Provides the `Spec` enum to describe the expected shape of HCL blocks
//! and attributes, similar to the `hcldec` package in Go.

use crate::error::HclError;
use crate::types::ty::Type;
use crate::types::val::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Details for a nested block specification.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockSpec {
    /// The block type identifier (e.g., `"resource"` or `"variable"`).
    pub type_name: String,

    /// The specification detailing the expected structure of the block's body.
    pub body: Box<Spec>,
}

/// Details for a repeated block list specification.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockListSpec {
    /// The block type identifier.
    pub type_name: String,
    /// The specification for decoding each block.
    pub nested: Box<Spec>,
    /// Optional minimum number of items required.
    pub min_items: Option<usize>,
    /// Optional maximum number of items allowed.
    pub max_items: Option<usize>,
}

impl BlockListSpec {
    /// Creates a new `BlockListSpec`.
    ///
    /// # Arguments
    /// * `type_name` - The block type identifier.
    /// * `nested` - The specification for each block's body.
    #[must_use]
    pub fn new(type_name: impl Into<String>, nested: Spec) -> Self {
        Self {
            type_name: type_name.into(),
            nested: Box::new(nested),
            min_items: None,
            max_items: None,
        }
    }

    /// Sets the minimum item count constraint.
    ///
    /// # Arguments
    /// * `min` - The minimum number of items.
    #[must_use]
    pub fn with_min_items(mut self, min: usize) -> Self {
        self.min_items = Some(min);
        self
    }

    /// Sets the maximum item count constraint.
    ///
    /// # Arguments
    /// * `max` - The maximum number of items.
    #[must_use]
    pub fn with_max_items(mut self, max: usize) -> Self {
        self.max_items = Some(max);
        self
    }
}

/// Details for a repeated block set specification.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockSetSpec {
    /// The block type identifier.
    pub type_name: String,
    /// The specification for decoding each block into a unique set element.
    pub nested: Box<Spec>,
}

impl BlockSetSpec {
    /// Creates a new `BlockSetSpec`.
    ///
    /// # Arguments
    /// * `type_name` - The block type identifier.
    /// * `nested` - The specification for each block's body.
    #[must_use]
    pub fn new(type_name: impl Into<String>, nested: Spec) -> Self {
        Self {
            type_name: type_name.into(),
            nested: Box::new(nested),
        }
    }
}

/// Details for a block map specification indexed by block labels.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockMapSpec {
    /// The block type identifier.
    pub type_name: String,
    /// The label names that serve as the map keys.
    pub labels: Vec<String>,
    /// The specification for decoding each block's body.
    pub nested: Box<Spec>,
}

impl BlockMapSpec {
    /// Creates a new `BlockMapSpec`.
    ///
    /// # Arguments
    /// * `type_name` - The block type identifier.
    /// * `labels` - The label names used as keys.
    /// * `nested` - The specification for each block's body.
    #[must_use]
    pub fn new(type_name: impl Into<String>, labels: Vec<String>, nested: Spec) -> Self {
        Self {
            type_name: type_name.into(),
            labels,
            nested: Box::new(nested),
        }
    }
}

/// Details for consuming all attributes of a block into an arbitrary key-value map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAttrsSpec {
    /// The block type identifier.
    pub type_name: String,
    /// Optional expected type for the attribute values.
    pub expected_type: Option<Type>,
}

impl BlockAttrsSpec {
    /// Creates a new `BlockAttrsSpec`.
    ///
    /// # Arguments
    /// * `type_name` - The block type identifier.
    #[must_use]
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            expected_type: None,
        }
    }

    /// Sets the expected value type for the attributes.
    ///
    /// # Arguments
    /// * `ty` - The expected type.
    #[must_use]
    pub fn with_type(mut self, ty: Type) -> Self {
        self.expected_type = Some(ty);
        self
    }
}

/// Details for injecting a literal constant value into decoded output.
#[derive(Debug, Clone, PartialEq)]
pub struct LiteralSpec {
    /// The constant value to emit.
    pub value: Value,
}

impl LiteralSpec {
    /// Creates a new `LiteralSpec`.
    ///
    /// # Arguments
    /// * `value` - The literal value.
    #[must_use]
    pub fn new(value: Value) -> Self {
        Self { value }
    }
}

/// Type alias for transformation closures.
pub type TransformFn = Arc<dyn Fn(Value) -> Result<Value, HclError> + Send + Sync>;

/// Details for applying a transformation function to decoded sub-values.
#[derive(Clone)]
pub struct TransformSpec {
    /// The underlying specification to decode before transformation.
    pub spec: Box<Spec>,
    /// The transformation closure.
    pub func: TransformFn,
}

impl fmt::Debug for TransformSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransformSpec")
            .field("spec", &self.spec)
            .finish_non_exhaustive()
    }
}

impl PartialEq for TransformSpec {
    fn eq(&self, other: &Self) -> bool {
        self.spec == other.spec && Arc::ptr_eq(&self.func, &other.func)
    }
}

impl TransformSpec {
    /// Creates a new `TransformSpec`.
    ///
    /// # Arguments
    /// * `spec` - The underlying specification.
    /// * `func` - The transformation function.
    #[must_use]
    pub fn new<F>(spec: Spec, func: F) -> Self
    where
        F: Fn(Value) -> Result<Value, HclError> + Send + Sync + 'static,
    {
        Self {
            spec: Box::new(spec),
            func: Arc::new(func),
        }
    }
}

/// Details for decoding an ordered heterogeneous tuple.
#[derive(Debug, Clone, PartialEq)]
pub struct TupleSpec {
    /// Element specifications in order.
    pub elements: Vec<Spec>,
}

impl TupleSpec {
    /// Creates a new `TupleSpec`.
    ///
    /// # Arguments
    /// * `elements` - Element specifications.
    #[must_use]
    pub fn new(elements: Vec<Spec>) -> Self {
        Self { elements }
    }
}

/// Details for an arbitrary expression specification.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprSpec {
    /// The expression AST node.
    pub expr: crate::ast::expr::Expression,
}

impl ExprSpec {
    /// Creates a new `ExprSpec`.
    ///
    /// # Arguments
    /// * `expr` - The expression AST node.
    #[must_use]
    pub fn new(expr: crate::ast::expr::Expression) -> Self {
        Self { expr }
    }
}

/// Type alias for custom attribute validation closures.
pub type AttrValidatorFn = Arc<dyn Fn(&Value) -> Result<(), String> + Send + Sync>;

/// Details for an attribute specification.
#[derive(Clone)]
pub struct AttrSpec {
    /// The name of the attribute.
    pub name: String,

    /// The expected type of the attribute's evaluated value.
    pub expected_type: Type,

    /// Optional regex pattern that string attribute values must match.
    pub regex_pattern: Option<regex::Regex>,

    /// Optional minimum numeric value constraint.
    pub min_value: Option<crate::number::Number>,

    /// Optional maximum numeric value constraint.
    pub max_value: Option<crate::number::Number>,

    /// Optional custom validation closure.
    pub custom_validator: Option<AttrValidatorFn>,
}

impl fmt::Debug for AttrSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttrSpec")
            .field("name", &self.name)
            .field("expected_type", &self.expected_type)
            .field(
                "regex_pattern",
                &self.regex_pattern.as_ref().map(regex::Regex::as_str),
            )
            .field("min_value", &self.min_value)
            .field("max_value", &self.max_value)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AttrSpec {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.expected_type == other.expected_type
            && self.regex_pattern.as_ref().map(regex::Regex::as_str)
                == other.regex_pattern.as_ref().map(regex::Regex::as_str)
            && self.min_value == other.min_value
            && self.max_value == other.max_value
            && match (&self.custom_validator, &other.custom_validator) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}

impl AttrSpec {
    /// Creates a new `AttrSpec`.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    /// * `expected_type` - The expected evaluated type.
    #[must_use]
    pub fn new(name: impl Into<String>, expected_type: Type) -> Self {
        Self {
            name: name.into(),
            expected_type,
            regex_pattern: None,
            min_value: None,
            max_value: None,
            custom_validator: None,
        }
    }

    /// Sets a regex pattern validation constraint for string attribute values.
    ///
    /// # Arguments
    /// * `regex` - The regular expression to match against.
    #[must_use]
    pub fn with_regex(mut self, regex: regex::Regex) -> Self {
        self.regex_pattern = Some(regex);
        self
    }

    /// Sets a minimum numeric value constraint.
    ///
    /// # Arguments
    /// * `min` - The minimum allowed value.
    #[must_use]
    pub fn with_min_value(mut self, min: crate::number::Number) -> Self {
        self.min_value = Some(min);
        self
    }

    /// Sets a maximum numeric value constraint.
    ///
    /// # Arguments
    /// * `max` - The maximum allowed value.
    #[must_use]
    pub fn with_max_value(mut self, max: crate::number::Number) -> Self {
        self.max_value = Some(max);
        self
    }

    /// Sets a custom validation function.
    ///
    /// # Arguments
    /// * `validator` - The custom validation function returning `Ok(())` or `Err(String)`.
    #[must_use]
    pub fn with_custom_validator<F>(mut self, validator: F) -> Self
    where
        F: Fn(&Value) -> Result<(), String> + Send + Sync + 'static,
    {
        self.custom_validator = Some(Arc::new(validator));
        self
    }

    /// Validates an evaluated value against the declarative rules on this attribute specification.
    ///
    /// # Arguments
    /// * `val` - The evaluated value.
    /// * `span` - Source span of the attribute expression.
    ///
    /// # Errors
    /// Returns [`HclError::Validation`] if regex, min/max, or custom validator fails.
    pub fn validate_value(&self, val: &Value, _span: crate::span::Span) -> Result<(), HclError> {
        if let Some(ref regex) = self.regex_pattern
            && let crate::types::val::ValueData::String(ref s) = *val.data
            && !regex.is_match(s)
        {
            return Err(HclError::Validation(format!(
                "Attribute '{}' with value {:?} does not match required regex pattern '{}'",
                self.name,
                s,
                regex.as_str()
            )));
        }

        if let Some(ref min) = self.min_value
            && let crate::types::val::ValueData::Number(ref num) = *val.data
            && num < min
        {
            return Err(HclError::Validation(format!(
                "Attribute '{}' with value {} is less than minimum allowed value {}",
                self.name, num, min
            )));
        }

        if let Some(ref max) = self.max_value
            && let crate::types::val::ValueData::Number(ref num) = *val.data
            && num > max
        {
            return Err(HclError::Validation(format!(
                "Attribute '{}' with value {} is greater than maximum allowed value {}",
                self.name, num, max
            )));
        }

        if let Some(ref validator) = self.custom_validator
            && let Err(msg) = validator(val)
        {
            return Err(HclError::Validation(format!(
                "Validation failed for attribute '{}': {}",
                self.name, msg
            )));
        }

        Ok(())
    }
}

/// Details for a default fallback specification.
#[derive(Debug, Clone, PartialEq)]
pub struct DefaultSpec {
    /// The primary specification to evaluate.
    pub primary: Box<Spec>,

    /// The fallback value to use if the primary specification is unsatisfied.
    pub default_value: Value,
}

/// A specification for an HCL attribute or block structure.
///
/// This enum is used to dynamically define the expected structural layout
/// of an HCL body. It is recursively composed to represent full configuration
/// schemas.
#[derive(Debug, Clone, PartialEq)]
pub enum Spec {
    /// Describes an object where the keys are strings and the values
    /// conform to the inner specifications. Corresponds to an HCL object value
    /// or a collection of mapped attributes.
    Object(HashMap<String, Spec>),

    /// Describes an array of elements that conform to the inner specification.
    /// Corresponds to an HCL tuple or list value, or repeated blocks.
    Array(Box<Spec>),

    /// Describes a nested block with the given type name, requiring its body
    /// to match the inner specification. Corresponds to an HCL nested block.
    Block(BlockSpec),

    /// Describes a list of repeated nested blocks decoded as a list.
    BlockList(BlockListSpec),

    /// Describes a set of repeated nested blocks decoded into a unique set.
    BlockSet(BlockSetSpec),

    /// Describes repeated blocks indexed into a map by their label values.
    BlockMap(BlockMapSpec),

    /// Consumes all attributes of a block into an arbitrary key-value map.
    BlockAttrs(BlockAttrsSpec),

    /// Injects a constant literal value into decoded output.
    Literal(LiteralSpec),

    /// Applies a custom transformation function to decoded sub-values.
    Transform(TransformSpec),

    /// Describes an ordered heterogeneous tuple.
    Tuple(TupleSpec),

    /// Describes an expected attribute of a certain primitive or composed type.
    /// Corresponds to an HCL attribute assignment (`name = value`).
    Attr(AttrSpec),

    /// Wraps another specification and provides a fallback value to use
    /// if the structure is missing from the configuration.
    Default(DefaultSpec),

    /// Asserts that the inner specification must be present in the configuration.
    /// Used to mark required attributes or blocks.
    Required(Box<Spec>),

    /// States that the inner specification is optional, and can be omitted.
    /// If omitted, the resulting value will typically be null.
    Optional(Box<Spec>),

    /// Evaluates an arbitrary HCL expression directly in the evaluation context.
    Expr(ExprSpec),
}

impl Spec {
    /// Returns the implied HCL [`Type`] produced by decoding with this specification.
    ///
    /// # Errors
    /// Returns [`HclError`] if a nested specification cannot statically infer a type.
    pub fn implied_type(&self) -> Result<Type, HclError> {
        Ok(self.compute_implied_type())
    }

    fn compute_implied_type(&self) -> Type {
        match self {
            Self::Attr(a) => a.expected_type.clone(),
            Self::Block(b) => b.body.compute_implied_type(),
            Self::BlockList(b) => Type::List(Box::new(b.nested.compute_implied_type())),
            Self::BlockSet(b) => Type::Set(Box::new(b.nested.compute_implied_type())),
            Self::BlockMap(b) => Type::Map(Box::new(b.nested.compute_implied_type())),
            Self::BlockAttrs(b) => {
                let elem_ty = b.expected_type.clone().unwrap_or(Type::Dynamic);
                Type::Map(Box::new(elem_ty))
            }
            Self::Literal(l) => l.value.ty().clone(),
            Self::Default(d) => d.primary.compute_implied_type(),
            Self::Transform(t) => t.spec.compute_implied_type(),
            Self::Tuple(t) => {
                let mut elem_types = Vec::with_capacity(t.elements.len());
                for elem in &t.elements {
                    elem_types.push(elem.compute_implied_type());
                }
                Type::Tuple(elem_types)
            }
            Self::Array(a) => Type::List(Box::new(a.compute_implied_type())),
            Self::Object(obj) => {
                let mut attrs = std::collections::BTreeMap::new();
                let mut optional_attrs = std::collections::BTreeSet::new();
                for (k, spec) in obj {
                    if let Self::Optional(inner) = spec {
                        optional_attrs.insert(k.clone());
                        attrs.insert(k.clone(), inner.compute_implied_type());
                    } else {
                        attrs.insert(k.clone(), spec.compute_implied_type());
                    }
                }
                Type::Object {
                    attrs,
                    optional_attrs,
                }
            }
            Self::Required(inner) | Self::Optional(inner) => inner.compute_implied_type(),
            Self::Expr(_) => Type::Dynamic,
        }
    }

    /// Extracts all variable traversal references from expressions contained within this specification.
    #[must_use]
    pub fn variables(&self) -> Vec<crate::ast::traversal::AbsTraversal> {
        let mut vars = Vec::new();
        self.collect_variables(&mut vars);
        vars
    }

    fn collect_variables(&self, vars: &mut Vec<crate::ast::traversal::AbsTraversal>) {
        match self {
            Self::Expr(e) => {
                vars.extend(
                    crate::ast::deps::extract_static_references(&e.expr).unwrap_or_default(),
                );
            }
            Self::Object(map) => {
                for inner in map.values() {
                    inner.collect_variables(vars);
                }
            }
            Self::Array(inner) | Self::Required(inner) | Self::Optional(inner) => {
                inner.collect_variables(vars);
            }
            Self::Block(b) => {
                b.body.collect_variables(vars);
            }
            Self::BlockList(b) => {
                b.nested.collect_variables(vars);
            }
            Self::BlockSet(b) => {
                b.nested.collect_variables(vars);
            }
            Self::BlockMap(b) => {
                b.nested.collect_variables(vars);
            }
            Self::Transform(t) => {
                t.spec.collect_variables(vars);
            }
            Self::Default(d) => {
                d.primary.collect_variables(vars);
            }
            Self::Tuple(t) => {
                for elem in &t.elements {
                    elem.collect_variables(vars);
                }
            }
            Self::BlockAttrs(_) | Self::Literal(_) | Self::Attr(_) => {}
        }
    }

    /// Recursively traverses this specification node and all child specifications using a visitor function.
    ///
    /// # Arguments
    /// * `f` - Visitor closure receiving each `&Spec` node. Return `true` to continue traversing child nodes,
    ///   or `false` to skip visiting children of the current node.
    pub fn walk<F>(&self, mut f: F)
    where
        F: FnMut(&Spec) -> bool,
    {
        self.walk_inner(&mut f);
    }

    fn walk_inner<F>(&self, f: &mut F)
    where
        F: FnMut(&Spec) -> bool,
    {
        if !f(self) {
            return;
        }

        match self {
            Self::Object(map) => {
                for inner in map.values() {
                    inner.walk_inner(f);
                }
            }
            Self::Array(inner) | Self::Required(inner) | Self::Optional(inner) => {
                inner.walk_inner(f);
            }
            Self::Block(b) => {
                b.body.walk_inner(f);
            }
            Self::BlockList(b) => {
                b.nested.walk_inner(f);
            }
            Self::BlockSet(b) => {
                b.nested.walk_inner(f);
            }
            Self::BlockMap(b) => {
                b.nested.walk_inner(f);
            }
            Self::Transform(t) => {
                t.spec.walk_inner(f);
            }
            Self::Default(d) => {
                d.primary.walk_inner(f);
            }
            Self::Tuple(t) => {
                for elem in &t.elements {
                    elem.walk_inner(f);
                }
            }
            Self::BlockAttrs(_) | Self::Literal(_) | Self::Attr(_) | Self::Expr(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::number::Number;
    use crate::types::val::ValueData;
    use std::str::FromStr;

    #[test]
    fn test_spec_builders_and_equality() {
        let bl = BlockListSpec::new(
            "tag",
            Spec::Literal(LiteralSpec::new(Value::new(
                Type::Bool,
                ValueData::Bool(true),
            ))),
        )
        .with_min_items(1)
        .with_max_items(5);
        assert_eq!(bl.type_name, "tag");
        assert_eq!(bl.min_items, Some(1));
        assert_eq!(bl.max_items, Some(5));

        let bs = BlockSetSpec::new(
            "item",
            Spec::Literal(LiteralSpec::new(Value::new(
                Type::String,
                ValueData::String("val".to_string()),
            ))),
        );
        assert_eq!(bs.type_name, "item");

        let bm = BlockMapSpec::new(
            "server",
            vec!["name".to_string()],
            Spec::Literal(LiteralSpec::new(Value::new(
                Type::Number,
                ValueData::Number(Number::from_str("1").expect("num")),
            ))),
        );
        assert_eq!(bm.type_name, "server");
        assert_eq!(bm.labels, vec!["name"]);

        let ba = BlockAttrsSpec::new("extra").with_type(Type::String);
        assert_eq!(ba.type_name, "extra");
        assert_eq!(ba.expected_type, Some(Type::String));

        let lit = LiteralSpec::new(Value::new(
            Type::String,
            ValueData::String("constant".to_string()),
        ));
        assert_eq!(
            lit.value,
            Value::new(Type::String, ValueData::String("constant".to_string()))
        );

        let tr = TransformSpec::new(
            Spec::Literal(LiteralSpec::new(Value::new(
                Type::String,
                ValueData::String("x".to_string()),
            ))),
            Ok,
        );
        assert_eq!(tr, tr.clone());
        let tr2 = TransformSpec::new(
            Spec::Literal(LiteralSpec::new(Value::new(
                Type::String,
                ValueData::String("x".to_string()),
            ))),
            Ok,
        );
        assert_ne!(tr, tr2);
        let tr3 = TransformSpec::new(
            Spec::Literal(LiteralSpec::new(Value::new(
                Type::String,
                ValueData::String("y".to_string()),
            ))),
            Ok,
        );
        assert_ne!(tr, tr3);
        assert!(format!("{tr:?}").contains("TransformSpec"));

        let tup = TupleSpec::new(vec![Spec::Literal(LiteralSpec::new(Value::new(
            Type::Number,
            ValueData::Number(Number::from_str("1").expect("num")),
        )))]);
        assert_eq!(tup.elements.len(), 1);
    }

    #[test]
    fn test_spec_implied_type_and_variables() {
        use crate::ast::expr::Expression;
        use crate::encode::EncodeValue;
        use crate::span::Span;

        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);

        // 1. Test implied_type on nested specs
        let mut obj_specs = HashMap::new();
        obj_specs.insert(
            "name".to_string(),
            Spec::Attr(AttrSpec::new("name", Type::String)),
        );
        obj_specs.insert(
            "count".to_string(),
            Spec::Optional(Box::new(Spec::Attr(AttrSpec::new("count", Type::Number)))),
        );
        let obj_spec = Spec::Object(obj_specs);
        let implied = obj_spec.implied_type().expect("implied object type");
        assert!(matches!(implied, Type::Object { .. }));

        let block_list_spec = Spec::BlockList(BlockListSpec::new(
            "items",
            Spec::Attr(AttrSpec::new("val", Type::Bool)),
        ));
        assert_eq!(
            block_list_spec.implied_type().expect("implied list"),
            Type::List(Box::new(Type::Bool))
        );

        let block_set_spec = Spec::BlockSet(BlockSetSpec::new(
            "tags",
            Spec::Literal(LiteralSpec::new("tag1".encode_value())),
        ));
        assert_eq!(
            block_set_spec.implied_type().expect("implied set"),
            Type::Set(Box::new(Type::String))
        );

        let block_map_spec = Spec::BlockMap(BlockMapSpec::new(
            "env",
            vec!["name".to_string()],
            Spec::Literal(LiteralSpec::new("val".encode_value())),
        ));
        assert_eq!(
            block_map_spec.implied_type().expect("implied map"),
            Type::Map(Box::new(Type::String))
        );

        let block_attrs_spec = Spec::BlockAttrs(BlockAttrsSpec::new("extra"));
        assert_eq!(
            block_attrs_spec
                .implied_type()
                .expect("implied block attrs"),
            Type::Map(Box::new(Type::Dynamic))
        );

        let tuple_spec = Spec::Tuple(TupleSpec::new(vec![
            Spec::Literal(LiteralSpec::new(10_i64.encode_value())),
            Spec::Literal(LiteralSpec::new("ok".encode_value())),
        ]));
        assert_eq!(
            tuple_spec.implied_type().expect("implied tuple"),
            Type::Tuple(vec![Type::Number, Type::String])
        );

        let default_spec = Spec::Default(DefaultSpec {
            primary: Box::new(Spec::Attr(AttrSpec::new("foo", Type::String))),
            default_value: "bar".encode_value(),
        });
        assert_eq!(
            default_spec.implied_type().expect("implied default"),
            Type::String
        );

        let transform_spec = Spec::Transform(TransformSpec::new(
            Spec::Attr(AttrSpec::new("baz", Type::Bool)),
            Ok,
        ));
        assert_eq!(
            transform_spec.implied_type().expect("implied transform"),
            Type::Bool
        );

        let expr_spec = Spec::Expr(ExprSpec::new(Expression::Variable(
            "var.sample".to_string(),
            dummy_span.clone(),
        )));
        assert_eq!(
            expr_spec.implied_type().expect("implied expr"),
            Type::Dynamic
        );

        // 2. Test variables extraction
        let mut complex_obj = HashMap::new();
        complex_obj.insert(
            "computed".to_string(),
            Spec::Expr(ExprSpec::new(Expression::Variable(
                "var.foo".to_string(),
                dummy_span.clone(),
            ))),
        );
        complex_obj.insert(
            "nested_block".to_string(),
            Spec::Block(BlockSpec {
                type_name: "sub".to_string(),
                body: Box::new(Spec::Expr(ExprSpec::new(Expression::Variable(
                    "local.bar".to_string(),
                    dummy_span,
                )))),
            }),
        );
        let complex_spec = Spec::Object(complex_obj);
        let vars = complex_spec.variables();
        let var_names: Vec<String> = vars.iter().map(std::string::ToString::to_string).collect();
        assert!(var_names.iter().any(|s| s.contains("var.foo")));
        assert!(var_names.iter().any(|s| s.contains("local.bar")));
    }

    #[test]
    fn test_spec_implied_type_and_variables_exhaustive() {
        use crate::ast::expr::Expression;
        use crate::encode::EncodeValue;
        use crate::span::Span;

        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);

        // Test implied_type for Block, Array, Required, Optional, and BlockAttrs with type
        let block_spec = Spec::Block(BlockSpec {
            type_name: "service".to_string(),
            body: Box::new(Spec::Attr(AttrSpec::new("port", Type::Number))),
        });
        assert_eq!(
            block_spec.implied_type().expect("implied block"),
            Type::Number
        );

        let array_spec = Spec::Array(Box::new(Spec::Attr(AttrSpec::new("elem", Type::String))));
        assert_eq!(
            array_spec.implied_type().expect("implied array"),
            Type::List(Box::new(Type::String))
        );

        let req_spec = Spec::Required(Box::new(Spec::Attr(AttrSpec::new("req", Type::Bool))));
        assert_eq!(
            req_spec.implied_type().expect("implied required"),
            Type::Bool
        );

        let opt_spec = Spec::Optional(Box::new(Spec::Attr(AttrSpec::new("opt", Type::Number))));
        assert_eq!(
            opt_spec.implied_type().expect("implied optional"),
            Type::Number
        );

        let ba_typed = Spec::BlockAttrs(BlockAttrsSpec::new("tags").with_type(Type::String));
        assert_eq!(
            ba_typed.implied_type().expect("implied block attrs typed"),
            Type::Map(Box::new(Type::String))
        );

        // Test variables extraction for all remaining variants
        let make_var_expr = |name: &str| {
            Spec::Expr(ExprSpec::new(Expression::Variable(
                name.to_string(),
                dummy_span.clone(),
            )))
        };

        let array_var = Spec::Array(Box::new(make_var_expr("var.arr")));
        assert_eq!(array_var.variables().len(), 1);

        let req_var = Spec::Required(Box::new(make_var_expr("var.req")));
        assert_eq!(req_var.variables().len(), 1);

        let opt_var = Spec::Optional(Box::new(make_var_expr("var.opt")));
        assert_eq!(opt_var.variables().len(), 1);

        let block_list_var = Spec::BlockList(BlockListSpec::new("bl", make_var_expr("var.bl")));
        assert_eq!(block_list_var.variables().len(), 1);

        let block_set_var = Spec::BlockSet(BlockSetSpec::new("bs", make_var_expr("var.bs")));
        assert_eq!(block_set_var.variables().len(), 1);

        let block_map_var = Spec::BlockMap(BlockMapSpec::new(
            "bm",
            vec!["label".to_string()],
            make_var_expr("var.bm"),
        ));
        assert_eq!(block_map_var.variables().len(), 1);

        let tr_var = Spec::Transform(TransformSpec::new(make_var_expr("var.tr"), Ok));
        assert_eq!(tr_var.variables().len(), 1);

        let def_var = Spec::Default(DefaultSpec {
            primary: Box::new(make_var_expr("var.def")),
            default_value: "fallback".encode_value(),
        });
        assert_eq!(def_var.variables().len(), 1);

        let tup_var = Spec::Tuple(TupleSpec::new(vec![
            make_var_expr("var.tup1"),
            make_var_expr("var.tup2"),
        ]));
        assert_eq!(tup_var.variables().len(), 2);

        // Terminal branches without expressions
        let ba_leaf = Spec::BlockAttrs(BlockAttrsSpec::new("empty"));
        assert_eq!(ba_leaf.variables().len(), 0);

        let lit_leaf = Spec::Literal(LiteralSpec::new(1.encode_value()));
        assert_eq!(lit_leaf.variables().len(), 0);

        let attr_leaf = Spec::Attr(AttrSpec::new("leaf", Type::Bool));
        assert_eq!(attr_leaf.variables().len(), 0);
    }

    #[test]
    fn test_spec_walk_all_variants_and_pruning() {
        use crate::ast::expr::Expression;
        use crate::encode::EncodeValue;
        use crate::span::Span;

        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);

        let inner_tuple = Spec::Tuple(TupleSpec::new(vec![
            Spec::BlockList(BlockListSpec::new(
                "bl",
                Spec::Attr(AttrSpec::new("attr", Type::String)),
            )),
            Spec::BlockSet(BlockSetSpec::new(
                "bs",
                Spec::Literal(LiteralSpec::new("lit".encode_value())),
            )),
            Spec::BlockMap(BlockMapSpec::new(
                "bm",
                vec!["k".to_string()],
                Spec::BlockAttrs(BlockAttrsSpec::new("ba")),
            )),
            Spec::Expr(ExprSpec::new(Expression::Number(
                crate::number::Number::from(1),
                dummy_span,
            ))),
        ]));

        let transform = Spec::Transform(TransformSpec::new(inner_tuple, Ok));
        let default_spec = Spec::Default(DefaultSpec {
            primary: Box::new(transform),
            default_value: "def".encode_value(),
        });
        let opt_spec = Spec::Optional(Box::new(default_spec));
        let req_spec = Spec::Required(Box::new(opt_spec));
        let block_spec = Spec::Block(BlockSpec {
            type_name: "b".to_string(),
            body: Box::new(req_spec),
        });
        let array_spec = Spec::Array(Box::new(block_spec.clone()));

        let mut m = HashMap::new();
        m.insert("arr".to_string(), array_spec);
        let full_tree = Spec::Object(m);

        // 1. Visit count with early return pruning on last element
        let mut count = 0;
        full_tree.walk(|_s| {
            count += 1;
            count < 15
        });
        assert_eq!(count, 15);

        // 2. Prune at Array
        let mut visited_types = Vec::new();
        full_tree.walk(|s| {
            visited_types.push(std::mem::discriminant(s));
            !matches!(s, Spec::Array(_))
        });
        assert!(!visited_types.contains(&std::mem::discriminant(&block_spec)));
    }

    #[test]
    fn test_attr_spec_declarative_validation_and_equality() {
        use crate::span::Span;
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);

        let regex = regex::Regex::new(r"^prod-[0-9]+$").expect("valid regex");
        let min_num = crate::number::Number::from(10);
        let max_num = crate::number::Number::from(100);

        let spec = AttrSpec::new("cluster_id", Type::String)
            .with_regex(regex.clone())
            .with_min_value(min_num.clone())
            .with_max_value(max_num.clone())
            .with_custom_validator(|val| {
                if val.to_string().contains("999") {
                    Err("cluster 999 is reserved".to_string())
                } else {
                    Ok(())
                }
            });

        // Debug and PartialEq
        let dbg_str = format!("{spec:?}");
        assert!(dbg_str.contains("cluster_id"));
        assert!(dbg_str.contains("prod-[0-9]+"));

        let spec_clone = spec.clone();
        assert_eq!(spec, spec_clone);

        let spec_diff_name = AttrSpec::new("diff", Type::String);
        assert_ne!(spec, spec_diff_name);

        let spec_diff_type = AttrSpec::new("cluster_id", Type::Number);
        assert_ne!(spec, spec_diff_type);

        let spec_no_validator = AttrSpec::new("cluster_id", Type::String);
        assert_ne!(spec, spec_no_validator);

        // Test AttrSpec equality with regex mismatch
        let a_regex1 = AttrSpec::new("test", Type::String)
            .with_regex(regex::Regex::new("^a$").expect("valid regex"));
        let a_regex2 = AttrSpec::new("test", Type::String)
            .with_regex(regex::Regex::new("^b$").expect("valid regex"));
        assert_ne!(a_regex1, a_regex2);

        // Test AttrSpec equality with min/max mismatch
        let a_min1 = AttrSpec::new("test", Type::Number).with_min_value(min_num.clone());
        let a_min2 = AttrSpec::new("test", Type::Number).with_min_value(max_num.clone());
        assert_ne!(a_min1, a_min2);

        let a_max1 = AttrSpec::new("test", Type::Number).with_max_value(min_num.clone());
        let a_max2 = AttrSpec::new("test", Type::Number).with_max_value(max_num.clone());
        assert_ne!(a_max1, a_max2);

        // Test AttrSpec equality with custom_validator branches
        let a_none1 = AttrSpec::new("test", Type::String);
        let a_none2 = AttrSpec::new("test", Type::String);
        assert_eq!(a_none1, a_none2);

        let val_fn1: AttrValidatorFn = std::sync::Arc::new(|_| Ok(()));
        let val_fn2: AttrValidatorFn = std::sync::Arc::new(|_| Ok(()));

        let mut a_some1 = AttrSpec::new("test", Type::String);
        a_some1.custom_validator = Some(val_fn1.clone());
        let mut a_some2 = AttrSpec::new("test", Type::String);
        a_some2.custom_validator = Some(val_fn2.clone());

        assert_ne!(a_none1, a_some1);
        assert_ne!(a_some1, a_none1);
        assert_ne!(a_some1, a_some2); // Different Arcs

        let mut a_shared2 = AttrSpec::new("test", Type::String);
        a_shared2.custom_validator = Some(val_fn1.clone());
        assert_eq!(a_some1, a_shared2);

        // Validation - Success string
        let valid_str = Value::new(
            Type::String,
            crate::types::val::ValueData::String("prod-42".to_string()),
        );
        assert!(spec.validate_value(&valid_str, dummy_span.clone()).is_ok());

        // Execute validator closures
        let _ = (val_fn1)(&valid_str);
        let _ = (val_fn2)(&valid_str);

        // Validation - Failure regex
        let invalid_str = Value::new(
            Type::String,
            crate::types::val::ValueData::String("dev-42".to_string()),
        );
        assert_eq!(
            spec.validate_value(&invalid_str, dummy_span.clone()),
            Err(HclError::Validation(format!(
                "Attribute 'cluster_id' with value \"dev-42\" does not match required regex pattern '{}'",
                regex.as_str()
            )))
        );

        // Validation - Numeric min/max
        let num_spec = AttrSpec::new("scale", Type::Number)
            .with_min_value(min_num)
            .with_max_value(max_num);

        let valid_num = Value::new(
            Type::Number,
            crate::types::val::ValueData::Number(crate::number::Number::from(50)),
        );
        assert!(
            num_spec
                .validate_value(&valid_num, dummy_span.clone())
                .is_ok()
        );

        // Validate value with type mismatch against specs (non-string against regex, non-number against min/max)
        assert!(spec.validate_value(&valid_num, dummy_span.clone()).is_ok());
        assert!(
            num_spec
                .validate_value(&valid_str, dummy_span.clone())
                .is_ok()
        );

        let too_small = Value::new(
            Type::Number,
            crate::types::val::ValueData::Number(crate::number::Number::from(5)),
        );
        assert_eq!(
            num_spec.validate_value(&too_small, dummy_span.clone()),
            Err(HclError::Validation(
                "Attribute 'scale' with value 5 is less than minimum allowed value 10".to_string()
            ))
        );

        let too_big = Value::new(
            Type::Number,
            crate::types::val::ValueData::Number(crate::number::Number::from(150)),
        );
        assert_eq!(
            num_spec.validate_value(&too_big, dummy_span.clone()),
            Err(HclError::Validation(
                "Attribute 'scale' with value 150 is greater than maximum allowed value 100"
                    .to_string()
            ))
        );

        // Validation - Custom validator
        let forbidden_val = Value::new(
            Type::String,
            crate::types::val::ValueData::String("prod-999".to_string()),
        );
        assert_eq!(
            spec.validate_value(&forbidden_val, dummy_span.clone()),
            Err(HclError::Validation(
                "Validation failed for attribute 'cluster_id': cluster 999 is reserved".to_string()
            ))
        );

        // Validation - Plain spec with None for regex, min_value, max_value, and custom_validator
        let plain_spec = AttrSpec::new("name", Type::String);
        assert!(plain_spec.validate_value(&valid_str, dummy_span).is_ok());
    }
}
