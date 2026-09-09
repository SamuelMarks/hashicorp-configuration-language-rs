//! The HCL type system (`cty` equivalent).
//!
//! This module defines the strict type definitions and value semantics used by HCL.

pub mod json;
pub mod msgpack;
pub mod path;
pub mod refinement;
pub mod ty;
pub mod unify;
pub mod val;

pub use hcl_macros::CapsuleType;
pub use json::{
    decode_type_from_json, decode_typed_value, decode_value_from_json, encode_type_to_json,
    encode_typed_value, encode_value_to_json,
};
pub use msgpack::{decode_type, decode_value, encode_type, encode_value};
pub use path::{Path, PathStep};
pub use refinement::Refinement;
pub use ty::Type;
pub use val::{Value, ValueData, ValueMark};
