#![no_main]
use hashicorp_configuration_language_rs::hclsimple;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = hclsimple::decode_to_value("fuzz.hcl", s, None);
        let _ = hclsimple::decode_to_value("fuzz.json", s, None);
    }
});
