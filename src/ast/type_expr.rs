//! Type Expressions AST.
//!
//! Provides the AST nodes for HCL type constraints (e.g., `list(string)`, `object({ name = string })`).
use crate::ast::expr::Expression;
use crate::span::Span;
use crate::types::ty::Type;
use std::collections::HashMap;
/// An attribute definition within an object type expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectAttrType {
    /// The type expression for this attribute.
    pub ty: TypeExpr,
    /// Whether this attribute is optional.
    pub optional: bool,
    /// An optional default expression evaluated if the attribute is omitted or null.
    pub default_val: Option<Expression>,
}
impl ObjectAttrType {
    /// Creates a required object attribute with the given type expression.
    ///
    /// # Arguments
    /// * `ty` - The type expression for the attribute.
    #[must_use]
    pub fn required(ty: TypeExpr) -> Self {
        Self {
            ty,
            optional: false,
            default_val: None,
        }
    }
    /// Creates an optional object attribute with the given type expression and optional default expression.
    ///
    /// # Arguments
    /// * `ty` - The type expression for the attribute.
    /// * `default_val` - The optional default expression.
    #[must_use]
    pub fn optional(ty: TypeExpr, default_val: Option<Expression>) -> Self {
        Self {
            ty,
            optional: true,
            default_val,
        }
    }
}
/// A type of collection for a type expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionType {
    /// A list collection type.
    List,
    /// A set collection type.
    Set,
    /// A map collection type.
    Map,
}
/// An HCL Type Expression AST node.
///
/// Represents type constraints parsed from HCL configurations.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    /// A primitive type (string, number, bool) or dynamic.
    Primitive(Type, Span),
    /// A collection type wrapping another type expression.
    Collection(CollectionType, Box<TypeExpr>, Span),
    /// An object type with strongly typed attributes.
    Object(HashMap<String, ObjectAttrType>, Span),
    /// A tuple type with an ordered sequence of type expressions.
    Tuple(Vec<TypeExpr>, Span),
}
impl TypeExpr {
    /// Returns the source span for this type expression.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Primitive(_, span)
            | TypeExpr::Collection(_, _, span)
            | TypeExpr::Object(_, span)
            | TypeExpr::Tuple(_, span) => span.clone(),
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
    #[test]
    fn test_type_expr_span() {
        let span = Span::new(0, 1, 1, 1, 1, 2);
        let prim = TypeExpr::Primitive(Type::String, span.clone());
        assert_eq!(prim.span(), span);
        let coll = TypeExpr::Collection(CollectionType::List, Box::new(prim.clone()), span.clone());
        assert_eq!(coll.span(), span);
        let req_attr = ObjectAttrType::required(prim.clone());
        assert!(!req_attr.optional);
        assert_eq!(req_attr.default_val, None);
        let opt_attr = ObjectAttrType::optional(prim, None);
        assert!(opt_attr.optional);
        let mut attrs = HashMap::new();
        attrs.insert("field".to_string(), req_attr);
        let obj = TypeExpr::Object(attrs, span.clone());
        assert_eq!(obj.span(), span);
        let tup = TypeExpr::Tuple(vec![], span.clone());
        assert_eq!(tup.span(), span);
    }
}
