//! Fuzz target for MessagePack type and value decoding.

#![cfg_attr(not(test), no_main)]

use hashicorp_configuration_language_rs::types::Type;
use hashicorp_configuration_language_rs::types::msgpack::{decode_type, decode_value};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(ty) = decode_type(data) {
        let _ = decode_value(data, &ty);
    }
    let _ = decode_value(data, &Type::Dynamic);
});

#[cfg(test)]
mod tests {
    #[allow(improper_ctypes)]
    unsafe extern "C" {
        fn rust_fuzzer_test_input(bytes: &[u8]) -> i32;
    }

    /// Tests fuzz target execution on valid types, invalid types, and dynamic values.
    #[test]
    fn test_fuzz_target_execution() {
        assert_eq!(unsafe { rust_fuzzer_test_input(b"\xa1S") }, 0);
        assert_eq!(unsafe { rust_fuzzer_test_input(&[0xc1]) }, 0);
    }
}
