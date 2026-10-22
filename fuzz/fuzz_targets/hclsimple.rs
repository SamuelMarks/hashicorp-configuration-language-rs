//! Fuzz target for `hclsimple` single-step decoding of HCL and JSON inputs.
#![cfg_attr(not(test), no_main)]
#![forbid(clippy::all, clippy::pedantic, clippy::nursery)]
#![forbid(missing_docs)]
#![forbid(rustdoc::missing_crate_level_docs)]
#![forbid(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hashicorp_configuration_language_rs::hclsimple;
use libfuzzer_sys::fuzz_target;

/// Executes the `hclsimple` fuzz target on a raw byte slice.
pub fn run_fuzz_input(data: &[u8]) {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = hclsimple::decode_to_value("fuzz.hcl", s, None);
        let _ = hclsimple::decode_to_value("fuzz.json", s, None);
    }
}

fuzz_target!(|data: &[u8]| {
    run_fuzz_input(data);
});
