#![no_main]
use hashicorp_configuration_language_rs::types::Type;
use hashicorp_configuration_language_rs::types::msgpack::{decode_type, decode_value};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(ty) = decode_type(data) {
        let _ = decode_value(data, &ty);
    }
    let _ = decode_value(data, &Type::Dynamic);
});
