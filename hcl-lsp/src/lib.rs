//! Language Server Protocol daemon for HCL (`hcl-lsp`).
//!
//! Implements LSP 3.17+ with JSON-RPC transport, document synchronization,
//! hierarchical outline symbols, hover documentation, schema-driven completions,
//! semantic token highlighting, and go-to-definition/references.
#![cfg_attr(all(coverage_nightly, test), feature(coverage_attribute))]
#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![deny(missing_docs)]
#![deny(rustdoc::missing_crate_level_docs)]
#![forbid(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

pub use error::{HclError, LspError};
pub use server::LspServer;
