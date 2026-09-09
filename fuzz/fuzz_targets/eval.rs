#![no_main]
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
