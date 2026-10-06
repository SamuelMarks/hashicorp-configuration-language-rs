//! Legacy HCL 1.0 backward compatibility and migration layer.
//!
//! Provides parsing for HCL 1.0 syntax (bare object keys, block assignments with `=`,
//! comma-less lists) and migration translation into canonical HCL 2.0 AST structures.
pub mod lex;
pub mod migrate;
pub mod parser;
pub mod token;
pub use lex::Hcl1Lexer;
pub use migrate::{MigrationDiagnostic, migrate_hcl1_to_hcl2};
pub use parser::{Hcl1Attribute, Hcl1Block, Hcl1Body, Hcl1Expression, Hcl1Item, Hcl1Parser};
pub use token::{Hcl1Token, Hcl1TokenKind};
#[cfg(test)]
mod tests;
