//! Test error formatting tool for the HCL parser.

use hashicorp_configuration_language_rs::serde::ser::to_string;
use std::collections::BTreeMap;
fn main() {
    let mut bad = BTreeMap::new();
    bad.insert(vec![1, 2], "value");
    let res = to_string(&bad);
    println!("{res:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main() {
        main();
    }
}
