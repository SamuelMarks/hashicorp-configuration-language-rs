//! Dynamic schemas for structural typing and validation (`hcldec`).
//!
//! Provides the ability to define specifications for expected HCL structures
//! and dynamically decode and validate HCL bodies against those specifications.

pub mod decode;
pub mod spec;

pub use decode::{decode, decode_with_context, partial_decode};
pub use spec::{
    AttrSpec, BlockAttrsSpec, BlockListSpec, BlockMapSpec, BlockSetSpec, BlockSpec, DefaultSpec,
    ExprSpec, LiteralSpec, Spec, TransformSpec, TupleSpec,
};
