//! Fuzz target for AST body transformations (`ext::transform`).
#![cfg_attr(not(test), no_main)]
#![forbid(clippy::all, clippy::pedantic, clippy::nursery)]
#![forbid(missing_docs)]
#![forbid(rustdoc::missing_crate_level_docs)]
#![forbid(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::ext::transform::{PrefixTransformer, TransformedBody};
use hashicorp_configuration_language_rs::parse::parser::Parser;
use libfuzzer_sys::fuzz_target;

/// Executes the AST transform fuzz target on a raw byte slice.
pub fn run_fuzz_input(data: &[u8]) {
    if let Ok(s) = std::str::from_utf8(data) {
        let mut parser = Parser::new(s);
        let body = parser.parse_body();
        let mut tx = PrefixTransformer::new("fuzz_");
        let _ = TransformedBody::new(body, &mut tx);
    }
}

fuzz_target!(|data: &[u8]| {
    run_fuzz_input(data);
});
