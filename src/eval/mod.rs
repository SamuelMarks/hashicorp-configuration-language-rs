//! The Evaluation Engine for HCL expressions.

/// Context definitions.
pub mod context;
/// Topological DAG dependency resolver.
pub mod dag;
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
/// CLI variable ingestion and var-file management.
pub mod vars;

pub use dag::{DagResolver, resolve_definitions};
pub use dynblock::expand_dynamic_blocks;
pub use fs::{ArchiveFileSystem, FileSystem, MemFileSystem, OsFileSystem, SandboxedFileSystem};
pub use lazy::LazyBody;
pub use partial::{partial_eval, partial_eval_body, value_to_expression};
pub use validation::{
    evaluate_all_validations, evaluate_postcondition, evaluate_precondition, evaluate_validation,
};
pub use vars::{VarManager, parse_var_file, parse_var_flag, parse_var_value};

#[cfg(test)]
mod func_tests;
#[cfg(test)]
mod partial_tests;
