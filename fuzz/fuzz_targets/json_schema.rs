#![no_main]
use hashicorp_configuration_language_rs::ast::schema::BodySchema;
use hashicorp_configuration_language_rs::parse::json::parse_json_with_schema;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let schema = BodySchema::default();
        let _ = parse_json_with_schema(s, &schema);
    }
});
