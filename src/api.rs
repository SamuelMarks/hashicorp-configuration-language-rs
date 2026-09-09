//! API module.

//! Public API surface.

pub use crate::analysis::{Linter, ScopeSchema, TypeChecker, is_type_compatible};
pub use crate::ast::deps::{
    DependencyGraph, extract_static_references, extract_static_references_from_body,
};
pub use crate::ast::schema::{AttributeSchema, BlockHeaderSchema, BodyContent, BodySchema};
pub use crate::ast::structure::{
    Attribute, Block, Body, DynamicBlock, PostconditionBlock, PreconditionBlock, ValidationBlock,
};
pub use crate::ast::traversal::{
    AbsTraversal, RelTraversal, abs_traversal_for_expr, rel_traversal_for_expr,
};
pub use crate::ast::walk::{AstFolder, AstVisitor, walk_body, walk_expression};
pub use crate::cst::builder::{CstAttribute, CstBlock, CstBody, CstFile};
pub use crate::cst::format::format_str;
use crate::decode::DecodeBody;
pub use crate::diagnostic::{Diagnostic, DiagnosticWriter, Diagnostics, Severity};
pub use crate::eval::context::Context;
pub use crate::eval::dynblock::expand_dynamic_blocks;
pub use crate::eval::partial::{partial_eval, partial_eval_body};
pub use crate::eval::stdlib::all_functions;
pub use crate::eval::validation::{
    evaluate_all_validations, evaluate_postcondition, evaluate_precondition, evaluate_validation,
};
pub use crate::parse::file_manager::{FileManager, HclParser};
use crate::parse::parser::Parser;

/// Parse an HCL string into an AST Body.
///
/// # Errors
/// Returns diagnostics if parsing fails.
pub fn parse(input: &str) -> Result<Body, Diagnostics> {
    let mut parser = Parser::new(input);
    let body = parser.parse_body();
    if parser.errors().has_errors() {
        Err(parser.errors().clone())
    } else {
        Ok(body)
    }
}

/// Decode an HCL string directly into a Rust struct using standard library functions by default.
///
/// Automatically expands any dynamic blocks defined within the HCL configuration.
///
/// # Errors
/// Returns diagnostics if parsing, dynamic block expansion, or decoding fails.
pub fn from_str<T: DecodeBody>(input: &str) -> Result<T, Diagnostics> {
    let body = parse(input)?;
    let mut ctx = Context::with_stdlib();
    let expanded = expand_dynamic_blocks(&body, &mut ctx)?;
    T::decode_body(&expanded, &[], &mut ctx)
}

/// Decode an HCL string directly into a Rust struct without enabling standard library functions.
///
/// Automatically expands any dynamic blocks defined within the HCL configuration.
///
/// # Errors
/// Returns diagnostics if parsing, dynamic block expansion, or decoding fails.
pub fn from_str_without_stdlib<T: DecodeBody>(input: &str) -> Result<T, Diagnostics> {
    let body = parse(input)?;
    let mut ctx = Context::new();
    let expanded = expand_dynamic_blocks(&body, &mut ctx)?;
    T::decode_body(&expanded, &[], &mut ctx)
}

/// Decode an HCL string into a Rust struct using a specific context.
///
/// Automatically expands any dynamic blocks defined within the HCL configuration.
///
/// # Errors
/// Returns diagnostics if parsing, dynamic block expansion, or decoding fails.
pub fn from_str_with_context<T: DecodeBody>(
    input: &str,
    ctx: &mut Context,
) -> Result<T, Diagnostics> {
    let body = parse(input)?;
    let expanded = expand_dynamic_blocks(&body, ctx)?;
    T::decode_body(&expanded, &[], ctx)
}

/// Parse and evaluate a single HCL expression string.
///
/// If `ctx` is None, a default context with all standard library functions is used.
///
/// # Errors
/// Returns diagnostics if expression parsing or evaluation fails.
pub fn evaluate_expr(
    input: &str,
    ctx: Option<&Context>,
) -> Result<crate::types::Value, Diagnostics> {
    let mut parser = Parser::new(input);
    let expr = parser.parse_expression();
    if parser.errors().has_errors() {
        return Err(parser.errors().clone());
    }
    let Some(expr) = expr else {
        let mut diags = Diagnostics::new();
        diags.push(crate::diagnostic::Diagnostic::new(
            crate::error::HclError::Parse("empty or invalid expression".into()),
            crate::span::Span::new(0, 0, 0, 0, 0, 0),
        ));
        return Err(diags);
    };
    let default_ctx;
    let eval_ctx = if let Some(c) = ctx {
        c
    } else {
        default_ctx = Context::with_stdlib();
        &default_ctx
    };
    let eval = crate::eval::evaluator::Evaluator::new(eval_ctx);
    let (val, _) = eval.evaluate(&expr)?;
    Ok(val)
}

/// Parse an HCL string and expand dynamic blocks using the provided context (or stdlib if None).
///
/// # Errors
/// Returns diagnostics if parsing or dynamic block expansion fails.
pub fn evaluate(input: &str, ctx: Option<&mut Context>) -> Result<Body, Diagnostics> {
    let body = parse(input)?;
    let mut default_ctx;
    let eval_ctx = if let Some(c) = ctx {
        c
    } else {
        default_ctx = Context::with_stdlib();
        &mut default_ctx
    };
    expand_dynamic_blocks(&body, eval_ctx)
}

/// Decode an HCL file from disk directly into a Rust struct using standard library functions by default.
///
/// Automatically caches the source file in [`FileManager`] to provide source context for diagnostics
/// and expands dynamic blocks.
///
/// # Arguments
/// * `path` - The filesystem path to the HCL configuration file.
///
/// # Errors
/// Returns [`Diagnostics`] if file reading, parsing, dynamic block expansion, or decoding fails.
pub fn from_file<T: DecodeBody>(path: impl AsRef<std::path::Path>) -> Result<T, Diagnostics> {
    let mut ctx = Context::with_stdlib();
    from_file_with_context(path, &mut ctx)
}

/// Decode an HCL file from disk into a Rust struct using a specific [`Context`].
///
/// Automatically caches the source file in [`FileManager`] to provide source context for diagnostics
/// and expands dynamic blocks.
///
/// # Arguments
/// * `path` - The filesystem path to the HCL configuration file.
/// * `ctx` - The evaluation context.
///
/// # Errors
/// Returns [`Diagnostics`] if file reading, parsing, dynamic block expansion, or decoding fails.
pub fn from_file_with_context<T: DecodeBody>(
    path: impl AsRef<std::path::Path>,
    ctx: &mut Context,
) -> Result<T, Diagnostics> {
    let p_str = path.as_ref().to_string_lossy();
    let mut file_manager = FileManager::new();
    let body = file_manager.parse_hcl_file(&p_str)?.clone();
    let expanded = expand_dynamic_blocks(&body, ctx)?;
    T::decode_body(&expanded, &[], ctx)
}

/// Parses an HCL file from disk and expands dynamic blocks using the provided context (or stdlib if None).
///
/// # Arguments
/// * `path` - The filesystem path to the HCL configuration file.
/// * `ctx` - Optional evaluation context.
///
/// # Errors
/// Returns [`Diagnostics`] if file reading, parsing, or dynamic block expansion fails.
pub fn evaluate_file(
    path: impl AsRef<std::path::Path>,
    ctx: Option<&mut Context>,
) -> Result<Body, Diagnostics> {
    let p_str = path.as_ref().to_string_lossy();
    let mut file_manager = FileManager::new();
    let body = file_manager.parse_hcl_file(&p_str)?.clone();
    if let Some(c) = ctx {
        expand_dynamic_blocks(&body, c)
    } else {
        let mut default_ctx = Context::with_stdlib();
        expand_dynamic_blocks(&body, &mut default_ctx)
    }
}

mod api_tests;
