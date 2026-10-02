//! Grounding tool for the HCL parser.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]

/// Entry point for `grounding` binary.
fn main() {
    let mut parser =
        hashicorp_configuration_language_rs::parse::parser::Parser::new("a = \"unclosed");
    let body = parser.parse_body();
    println!("Body attributes: {}", body.attributes.len());
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_main() {
        main();
    }
}
