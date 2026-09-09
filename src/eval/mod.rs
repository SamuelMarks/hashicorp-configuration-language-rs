//! The Evaluation Engine for HCL expressions.

/// Context definitions.
pub mod context;
/// Dynamic block expansion.
pub mod dynblock;
/// Evaluator implementations.
pub mod evaluator;
/// Pluggable virtual filesystem abstraction.
pub mod fs;
/// Function representations.
pub mod func;
/// Lazy on-demand body evaluation with memoization.
pub mod lazy;
/// Partial AST evaluation and expression reduction.
pub mod partial;
/// Standard library functions.
pub mod stdlib;
/// Type expression evaluation.
pub mod type_expr;
/// Declarative validation and assertion evaluation.
pub mod validation;

pub use dynblock::expand_dynamic_blocks;
pub use fs::{ArchiveFileSystem, FileSystem, MemFileSystem, OsFileSystem, SandboxedFileSystem};
pub use lazy::LazyBody;
pub use partial::{partial_eval, partial_eval_body, value_to_expression};
pub use validation::{
    evaluate_all_validations, evaluate_postcondition, evaluate_precondition, evaluate_validation,
};

#[cfg(test)]
mod func_tests;
#[cfg(test)]
mod partial_tests;
