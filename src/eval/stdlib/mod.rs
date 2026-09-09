//! Standard library functions for HCL evaluation.
//!
//! This module contains implementations of the built-in functions
//! available in HCL configuration files.

use crate::eval::func::Function;
use std::collections::HashMap;
use std::sync::LazyLock;

mod all_tests;
/// Collection functions.
pub mod collection;
/// Conversion functions.
pub mod conversion;
/// Crypto functions.
pub mod crypto;
/// Date time functions.
pub mod datetime;
/// Encoding and decoding functions.
pub mod encoding;
/// Filesystem and template functions.
pub mod filesystem;
/// Network functions.
pub mod network;
/// Numeric functions.
pub mod numeric;
/// String manipulation functions.
pub mod string;

/// Returns all built-in standard library functions aggregated across all categories.
#[must_use]
pub fn all_functions() -> Vec<Function> {
    let mut funcs = Vec::new();
    funcs.extend(collection::functions());
    funcs.extend(conversion::functions());
    funcs.extend(crypto::functions());
    funcs.extend(datetime::functions());
    funcs.extend(encoding::functions());
    funcs.extend(filesystem::functions());
    funcs.extend(network::functions());
    funcs.extend(numeric::functions());
    funcs.extend(string::functions());
    funcs
}

static STDLIB_FUNCTIONS: LazyLock<HashMap<String, Function>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for func in all_functions() {
        map.insert(func.name.clone(), func);
    }
    map
});

/// Look up a built-in standard library function by name from the global static registry.
///
/// Returns `Some(&Function)` if the function exists in the standard library, or `None` otherwise.
#[must_use]
pub fn get_stdlib_function(name: &str) -> Option<&'static Function> {
    STDLIB_FUNCTIONS.get(name)
}

/// Returns a reference to the global static map of all standard library functions.
#[must_use]
pub fn stdlib_map() -> &'static HashMap<String, Function> {
    &STDLIB_FUNCTIONS
}
