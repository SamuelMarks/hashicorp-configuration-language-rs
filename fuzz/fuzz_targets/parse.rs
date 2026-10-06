//! Fuzz target for HCL parsing.
#![cfg_attr(not(test), no_main)]
use hashicorp_configuration_language_rs::api::parse;
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = parse(s);
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
    /// Tests fuzz target execution on valid and invalid UTF-8.
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
        assert_eq!(unsafe { rust_fuzzer_test_input(&[0xff, 0xff]) }, 0);
    }
}
