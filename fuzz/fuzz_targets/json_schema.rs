//! Fuzz target for schema-driven JSON configuration parsing.
#![cfg_attr(not(test), no_main)]
#![forbid(clippy::all, clippy::pedantic, clippy::nursery)]
#![forbid(missing_docs)]
#![forbid(rustdoc::missing_crate_level_docs)]
#![forbid(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::ast::schema::BodySchema;
use hashicorp_configuration_language_rs::parse::json::parse_json_with_schema;
use libfuzzer_sys::fuzz_target;

/// Executes the JSON schema parsing fuzz target on a raw byte slice.
pub fn run_fuzz_input(data: &[u8]) {
    if let Ok(s) = std::str::from_utf8(data) {
        let schema = BodySchema::default();
        let _ = parse_json_with_schema(s, &schema);
    }
}

fuzz_target!(|data: &[u8]| {
    run_fuzz_input(data);
});
