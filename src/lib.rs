//! The core HCL-RS crate.
//!
//! Provides parsing, evaluation, and serialization of the `HashiCorp` Configuration Language.

#![deny(clippy::all, clippy::pedantic)]
#![allow(clippy::too_many_lines)]
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::items_after_statements,
    clippy::cast_possible_truncation,
    clippy::format_push_string
)]
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod analysis;
pub mod api;
pub mod ast;
pub mod cst;
pub mod decode;
pub mod diagnostic;
pub mod encode;
pub mod error;
pub mod eval;
pub mod hcl1;
pub mod hcldec;
pub mod lex;
pub mod number;
pub mod parse;
pub mod serde;
pub mod span;
pub mod types;
