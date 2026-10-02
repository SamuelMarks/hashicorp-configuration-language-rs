//! Main executable for the HCL parser.

use bigdecimal::{BigDecimal, ToPrimitive};
use std::str::FromStr;

/// # Errors
/// Returns an error if the arguments cannot be parsed.
pub fn main_with_args(arg1: &str, arg2: &str) -> Result<(), String> {
    let bd = BigDecimal::from_str(arg1).map_err(|e| e.to_string())?;
    println!("1e10000: {:?}", bd.to_f32());
    println!("1e10000 to_f64: {:?}", bd.to_f64());

    // what about really small numbers?
    let bd = BigDecimal::from_str(arg2).map_err(|e| e.to_string())?;
    println!("1e-10000: {:?}", bd.to_f32());
    Ok(())
}

fn main() -> Result<(), String> {
    main_with_args("1e10000", "1e-10000")
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::pedantic,
        clippy::nursery,
        clippy::collection_is_never_read
    )]

    use super::*;

    #[test]
    fn test_main() {
        let _ = main();
    }

    #[test]
    fn test_main_err() {
        let _ = main_with_args("invalid", "1e-10000");
        let _ = main_with_args("1e10000", "invalid");
    }
}
