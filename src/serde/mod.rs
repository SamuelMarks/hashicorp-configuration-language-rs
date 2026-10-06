//! Serde module.
pub mod de;
pub mod ser;
pub use de::from_str;
pub use ser::to_string;
mod de_tests;
