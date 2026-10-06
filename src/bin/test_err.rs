#![allow(clippy::use_self)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::suspicious_operation_groupings)]
#![allow(clippy::needless_collect)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::iter_on_single_items)]
#![allow(clippy::trivial_regex)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::similar_names)]
#![allow(clippy::if_not_else)]
#![allow(clippy::format_push_string)]
#![allow(clippy::unused_self)]
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::equatable_if_let)]
#![allow(clippy::branches_sharing_code)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::useless_let_if_seq)]
#![allow(clippy::collection_is_never_read)]
#![allow(clippy::literal_string_with_formatting_args)]
#![allow(clippy::string_lit_as_bytes)]
//! Test error formatting tool for the HCL parser.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::nursery
)]
use hashicorp_configuration_language_rs::serde::ser::to_string;
use std::collections::BTreeMap;
/// Entry point for `test-err` binary.
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
