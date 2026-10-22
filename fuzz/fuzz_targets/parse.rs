//! Fuzz target for HCL parsing.
#![cfg_attr(not(test), no_main)]
#![forbid(clippy::all, clippy::pedantic, clippy::nursery)]
#![forbid(missing_docs)]
#![forbid(rustdoc::missing_crate_level_docs)]
#![forbid(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::api::parse;
use libfuzzer_sys::fuzz_target;

/// Executes the parse fuzz target on a raw byte slice.
pub fn run_fuzz_input(data: &[u8]) {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = parse(s);
    }
}

fuzz_target!(|data: &[u8]| {
    run_fuzz_input(data);
});

#[cfg(test)]
mod tests {
    use super::run_fuzz_input;

    /// Tests fuzz target execution on valid and invalid UTF-8.
    #[test]
    fn test_fuzz_target_execution() {
        run_fuzz_input(
            b"foo = 1
",
        );
        run_fuzz_input(&[0xff, 0xff]);
    }
}
