//! Static analysis, type checking, and linting engines.
//!
//! Provides ahead-of-time (AOT) type checking against structural schemas and scopes,
//! as well as linting passes for detecting unreachable templates, tautologies, and unused definitions.
pub mod lint;
pub mod type_check;
pub use lint::Linter;
pub use type_check::{ScopeSchema, TypeChecker, is_type_compatible};
