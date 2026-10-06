//! Concrete Syntax Tree (CST) structures.
//!
//! The CST retains all original formatting tokens (whitespace, comments) from the lexer,
//! allowing for token-preserving mutations and formatting (like `hcl fmt`).
/// Programmatic CST builders and mutation engine (`hclwrite`).
pub mod builder;
pub mod document;
pub mod format;
pub mod parser;
pub mod splicing;
pub use builder::{
    CstAttribute, CstBlock, CstBody, CstFile, CstItem, tokens_for_expression, tokens_for_traversal,
    tokens_for_value,
};
pub use document::{Attribute, Block, Document, DocumentItem, TokenStream};
pub use format::Formatter;
pub use parser::CstParser;
