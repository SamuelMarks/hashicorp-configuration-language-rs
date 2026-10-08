#![no_main]
use hashicorp_configuration_language_rs::ext::transform::{PrefixTransformer, TransformedBody};
use hashicorp_configuration_language_rs::parse::parser::Parser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let mut parser = Parser::new(s);
        let body = parser.parse_body();
        let mut tx = PrefixTransformer::new("fuzz_");
        let _ = TransformedBody::new(body, &mut tx);
    }
});
