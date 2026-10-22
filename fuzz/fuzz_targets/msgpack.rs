//! Fuzz target for `MessagePack` type and value decoding.
#![cfg_attr(not(test), no_main)]
#![forbid(clippy::all, clippy::pedantic, clippy::nursery)]
#![forbid(missing_docs)]
#![forbid(rustdoc::missing_crate_level_docs)]
#![forbid(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::types::Type;
use hashicorp_configuration_language_rs::types::msgpack::{decode_type, decode_value};
use libfuzzer_sys::fuzz_target;

/// Executes the `MessagePack` fuzz target on a raw byte slice.
pub fn run_fuzz_input(data: &[u8]) {
    if let Ok(ty) = decode_type(data) {
        let _ = decode_value(data, &ty);
    }
    let _ = decode_value(data, &Type::Dynamic);
}

fuzz_target!(|data: &[u8]| {
    run_fuzz_input(data);
});

#[cfg(test)]
mod tests {
    use super::run_fuzz_input;

    /// Tests fuzz target execution on valid types, invalid types, and dynamic values.
    #[test]
    fn test_fuzz_target_execution() {
        run_fuzz_input(b"\xa1S");
        run_fuzz_input(&[0xc1]);
    }
}
