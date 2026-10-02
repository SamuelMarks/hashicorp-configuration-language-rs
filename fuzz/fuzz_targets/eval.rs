//! Fuzz target for HCL evaluation.

#![cfg_attr(not(test), no_main)]

use hashicorp_configuration_language_rs::api::parse;
use hashicorp_configuration_language_rs::eval::context::Context;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data)
        && let Ok(body) = parse(s)
    {
        let ctx = Context::new();
        for attr in body.attributes.values() {
            let evaluator =
                hashicorp_configuration_language_rs::eval::evaluator::Evaluator::new(&ctx);
            let _ = evaluator.evaluate(&attr.expr);
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

    /// Tests fuzz target execution on valid evaluations, parse failures, and invalid UTF-8.
    #[test]
    fn test_fuzz_target_execution() {
        assert_eq!(
            unsafe {
                rust_fuzzer_test_input(
                    b"a = 1 + 2
",
                )
            },
            0
        );
        assert_eq!(unsafe { rust_fuzzer_test_input(b"") }, 0);
        assert_eq!(unsafe { rust_fuzzer_test_input(b"bad {{") }, 0);
        assert_eq!(unsafe { rust_fuzzer_test_input(&[0xff, 0xff]) }, 0);
    }
}
