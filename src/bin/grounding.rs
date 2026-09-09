//! Grounding tool for the HCL parser.

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
