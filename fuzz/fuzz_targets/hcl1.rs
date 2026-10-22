//! Fuzz target for HCL1 lexing, parsing, and migration.
#![cfg_attr(not(test), no_main)]
#![forbid(clippy::all, clippy::pedantic, clippy::nursery)]
#![forbid(missing_docs)]
#![forbid(rustdoc::missing_crate_level_docs)]
#![forbid(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::hcl1::{Hcl1Lexer, Hcl1Parser, migrate_hcl1_to_hcl2};
use libfuzzer_sys::fuzz_target;

/// Executes the HCL1 fuzz target on a raw byte slice.
pub fn run_fuzz_input(data: &[u8]) {
    if let Ok(s) = std::str::from_utf8(data) {
        let mut lexer = Hcl1Lexer::new(s);
        if let Ok(tokens) = lexer.tokenize() {
            let mut parser = Hcl1Parser::new(tokens);
            if let Ok(body) = parser.parse_body() {
                let _ = migrate_hcl1_to_hcl2(&body);
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    run_fuzz_input(data);
});

#[cfg(test)]
mod tests {
    use super::run_fuzz_input;

    /// Tests fuzz target execution on valid HCL1, parser errors, lexer errors, and invalid UTF-8.
    #[test]
    fn test_fuzz_target_execution() {
        run_fuzz_input(b"foo = 1\n");
        run_fuzz_input(b"foo = ");
        run_fuzz_input(br#""un"#);
        run_fuzz_input(&[0xff, 0xff]);
    }
}
