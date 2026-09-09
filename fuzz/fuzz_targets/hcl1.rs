#![no_main]
use hashicorp_configuration_language_rs::hcl1::{Hcl1Lexer, Hcl1Parser, migrate_hcl1_to_hcl2};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let mut lexer = Hcl1Lexer::new(s);
        if let Ok(tokens) = lexer.tokenize() {
            let mut parser = Hcl1Parser::new(tokens);
            if let Ok(body) = parser.parse_body() {
                let _ = migrate_hcl1_to_hcl2(&body);
            }
        }
    }
});
