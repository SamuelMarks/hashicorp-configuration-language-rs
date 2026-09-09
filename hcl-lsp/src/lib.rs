//! Language Server Protocol daemon for HCL (`hcl-lsp`).
//!
//! Implements LSP 3.17+ with JSON-RPC transport, document synchronization,
//! hierarchical outline symbols, hover documentation, schema-driven completions,
//! semantic token highlighting, and go-to-definition/references.

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

pub mod cache;
pub mod completion;
pub mod definition;
pub mod error;
pub mod hover;
pub mod protocol;
pub mod semantic_tokens;
pub mod server;
pub mod symbols;
pub mod transport;

pub use error::LspError;
pub use server::LspServer;
