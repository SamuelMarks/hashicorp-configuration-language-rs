//! The core HCL-RS crate.
//!
//! Provides parsing, evaluation, and serialization of the `HashiCorp` Configuration Language.
#![cfg_attr(all(coverage_nightly, test), feature(coverage_attribute))]
#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![deny(missing_docs)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub mod analysis;
pub mod api;
pub mod ast;
pub mod cst;
pub mod decode;
pub mod diagnostic;
pub mod encode;
pub mod error;
pub mod eval;
pub mod ext;
pub mod hcl1;
pub mod hcldec;
pub mod hclsimple;
pub mod lex;
pub mod number;
pub mod parse;
pub mod serde;
pub mod span;
pub mod types;
