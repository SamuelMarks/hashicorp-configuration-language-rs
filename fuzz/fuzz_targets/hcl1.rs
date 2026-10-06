//! Fuzz target for HCL1 lexing, parsing, and migration.
#![cfg_attr(not(test), no_main)]
use hashicorp_configuration_language_rs::hcl1::{Hcl1Lexer, Hcl1Parser, migrate_hcl1_to_hcl2};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let mut lexer = Hcl1Lexer::new(s);
        if let Ok(tokens) = lexer.tokenize() {
            let mut parser = Hcl1Parser::new(tokens);
            if let Ok(body) = parser.parse_body() {
                let _ = migrate_hcl1_to_hcl2(&body);
            }
        }
    }
});
#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::pedantic,
        clippy::nursery
    )]
    #[allow(improper_ctypes)]
    unsafe extern "C" {
        fn rust_fuzzer_test_input(bytes: &[u8]) -> i32;
    }
    /// Tests fuzz target execution on valid HCL1, parser errors, lexer errors, and invalid UTF-8.
    #[test]
    fn test_fuzz_target_execution() {
        assert_eq!(
            unsafe {
                rust_fuzzer_test_input(
                    b"foo = 1
",
                )
            },
            0
        );
        assert_eq!(unsafe { rust_fuzzer_test_input(b"foo = ") }, 0);
        assert_eq!(unsafe { rust_fuzzer_test_input(b"\"un") }, 0);
        assert_eq!(unsafe { rust_fuzzer_test_input(&[0xff, 0xff]) }, 0);
    }
}
