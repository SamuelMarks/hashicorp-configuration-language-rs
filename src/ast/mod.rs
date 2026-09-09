//! The Abstract Syntax Tree (AST) for HCL.

/// Static reference extraction and dependency graph analysis.
pub mod deps;

/// HCL Expression AST nodes.
pub mod expr;

/// Schema definitions and content decoding.
pub mod schema;

/// HCL Structural AST nodes (Body, Attribute, Block).
pub mod structure;

/// Traversal primitives (AbsTraversal, RelTraversal).
pub mod traversal;

/// HCL Type Expressions AST nodes.
pub mod type_expr;

/// User-Defined Functions AST nodes.
pub mod user_func;

/// AST Walker and Visitor infrastructure.
pub mod walk;

pub use deps::{DependencyGraph, extract_static_references, extract_static_references_from_body};
pub use schema::{AttributeSchema, BlockHeaderSchema, BodyContent, BodySchema};
pub use traversal::{AbsTraversal, RelTraversal, abs_traversal_for_expr, rel_traversal_for_expr};
pub use walk::{AstFolder, AstVisitor, walk_body, walk_expression};
