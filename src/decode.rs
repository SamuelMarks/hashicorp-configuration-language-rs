//! Decode module.

//! Structural Decoding (`gohcl` Equivalent)
//!
//! Provides the `DecodeBody` trait and related functionality for mapping HCL AST nodes
//! directly into strongly-typed Rust structures.

use crate::ast::structure::Body;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::eval::context::Context;
use crate::span::Span;
use crate::types::val::{Value, ValueData};
use std::collections::HashMap;

pub use crate::encode::{EncodeBody, EncodeValue};
pub use hcl_macros::{DecodeBody, DecodeValue};

/// Trait for decoding an HCL Body into a Rust struct.
pub trait DecodeBody: Sized {
    /// Decode the body into the struct, evaluating expressions using the given context.
    /// # Errors
    ///
    /// Returns diagnostics if decoding fails.
    fn decode_body(body: &Body, labels: &[String], ctx: &mut Context) -> Result<Self, Diagnostics>;
}

impl DecodeBody for Body {
    fn decode_body(
        body: &Body,
        _labels: &[String],
        _ctx: &mut Context,
    ) -> Result<Self, Diagnostics> {
        Ok(body.clone())
    }
}

/// Trait for decoding an HCL Expression (resolved to a `Value`) into a Rust type.
pub trait DecodeValue: Sized {
    /// Decode a value into this Rust type.
    /// # Errors
    ///
    /// Returns diagnostics if decoding fails.
    fn decode_value(value: &Value, span: Span) -> Result<Self, Diagnostics>;
}

use bigdecimal::ToPrimitive;

macro_rules! impl_decode_value_int {
    ($($t:ty => $method:ident),*) => {
        $(
            impl DecodeValue for $t {
                fn decode_value(value: &Value, span: Span) -> Result<Self, Diagnostics> {
                    if let ValueData::Number(n) = &*value.data {
                        if let Some(v) = n.0.$method() {
                            return Ok(v);
                        }
                    }
                    Err(Diagnostics::from(Diagnostic::error(
                        "Type Mismatch",
                        format!("Expected number, got {}", value.ty()),
                        span,
                    )))
                }
            }
        )*
    };
}

impl_decode_value_int!(
    i8 => to_i8, i16 => to_i16, i32 => to_i32, i64 => to_i64, i128 => to_i128, isize => to_isize,
    u8 => to_u8, u16 => to_u16, u32 => to_u32, u64 => to_u64, u128 => to_u128, usize => to_usize
);

macro_rules! impl_decode_value_float {
    ($($t:ty => $method:ident),*) => {
        $(
            impl DecodeValue for $t {
                fn decode_value(value: &Value, span: Span) -> Result<Self, Diagnostics> {
                    if let ValueData::Number(n) = &*value.data {
                        return Ok(n.0.$method().unwrap_or_default());
                    }
                    Err(Diagnostics::from(Diagnostic::error(
                        "Type Mismatch",
                        format!("Expected number, got {}", value.ty()),
                        span,
                    )))
                }
            }
        )*
    };
}

impl_decode_value_float!(f32 => to_f32, f64 => to_f64);

impl DecodeValue for bool {
    fn decode_value(value: &Value, span: Span) -> Result<Self, Diagnostics> {
        if let ValueData::Bool(b) = &*value.data {
            Ok(*b)
        } else {
            Err(Diagnostics::from(Diagnostic::error(
                "Type Mismatch",
                format!("Expected bool, got {}", value.ty()),
                span,
            )))
        }
    }
}

impl DecodeValue for String {
    fn decode_value(value: &Value, span: Span) -> Result<Self, Diagnostics> {
        if let ValueData::String(s) = &*value.data {
            Ok(s.clone())
        } else {
            Err(Diagnostics::from(Diagnostic::error(
                "Type Mismatch",
                format!("Expected string, got {}", value.ty()),
                span,
            )))
        }
    }
}

impl<T: DecodeValue> DecodeValue for Vec<T> {
    fn decode_value(value: &Value, span: Span) -> Result<Self, Diagnostics> {
        match &*value.data {
            ValueData::Array(items) => {
                let mut vec = Vec::with_capacity(items.len());
                let mut diags = Diagnostics::new();
                for item in items {
                    match T::decode_value(item, span.clone()) {
                        Ok(v) => vec.push(v),
                        Err(d) => diags.extend(d),
                    }
                }
                if diags.has_errors() {
                    Err(diags)
                } else {
                    Ok(vec)
                }
            }
            ValueData::Set(items) => {
                let mut vec = Vec::with_capacity(items.len());
                let mut diags = Diagnostics::new();
                for item in items {
                    match T::decode_value(item, span.clone()) {
                        Ok(v) => vec.push(v),
                        Err(d) => diags.extend(d),
                    }
                }
                if diags.has_errors() {
                    Err(diags)
                } else {
                    Ok(vec)
                }
            }
            _ => Err(Diagnostics::from(Diagnostic::error(
                "Type Mismatch",
                format!("Expected list, tuple, or set, got {}", value.ty()),
                span,
            ))),
        }
    }
}

impl<T: DecodeValue, S: std::hash::BuildHasher + Default> DecodeValue for HashMap<String, T, S> {
    fn decode_value(value: &Value, span: Span) -> Result<Self, Diagnostics> {
        match &*value.data {
            ValueData::Object(map) => {
                let mut hash_map = HashMap::with_capacity_and_hasher(map.len(), Default::default());
                let mut diags = Diagnostics::new();
                for (k, v) in map {
                    match T::decode_value(v, span.clone()) {
                        Ok(decoded) => {
                            hash_map.insert(k.clone(), decoded);
                        }
                        Err(d) => diags.extend(d),
                    }
                }
                if diags.has_errors() {
                    Err(diags)
                } else {
                    Ok(hash_map)
                }
            }
            _ => Err(Diagnostics::from(Diagnostic::error(
                "Type Mismatch",
                format!("Expected object, got {}", value.ty()),
                span,
            ))),
        }
    }
}

impl DecodeValue for Value {
    fn decode_value(value: &Value, _span: Span) -> Result<Self, Diagnostics> {
        Ok(value.clone())
    }
}

impl<T: DecodeValue> DecodeValue for std::collections::BTreeMap<String, T> {
    fn decode_value(value: &Value, span: Span) -> Result<Self, Diagnostics> {
        match &*value.data {
            ValueData::Object(map) => {
                let mut btree_map = std::collections::BTreeMap::new();
                let mut diags = Diagnostics::new();
                for (k, v) in map {
                    match T::decode_value(v, span.clone()) {
                        Ok(decoded) => {
                            btree_map.insert(k.clone(), decoded);
                        }
                        Err(d) => diags.extend(d),
                    }
                }
                if diags.has_errors() {
                    Err(diags)
                } else {
                    Ok(btree_map)
                }
            }
            _ => Err(Diagnostics::from(Diagnostic::error(
                "Type Mismatch",
                format!("Expected object, got {}", value.ty()),
                span,
            ))),
        }
    }
}

impl<T: DecodeValue> DecodeValue for Option<T> {
    fn decode_value(value: &Value, span: Span) -> Result<Self, Diagnostics> {
        if let ValueData::Null = &*value.data {
            Ok(None)
        } else if let ValueData::Unknown(_) = &*value.data {
            T::decode_value(value, span).map(Some)
        } else {
            T::decode_value(value, span).map(Some)
        }
    }
}

/// Decodes an HCL [`crate::ast::expr::Expression`] into a strongly-typed Rust value by evaluating
/// the expression with the provided [`Context`].
///
/// # Arguments
/// * `expr` - The expression AST node to evaluate and decode.
/// * `ctx` - The evaluation context holding variables and functions.
///
/// # Errors
/// Returns [`Diagnostics`] if expression evaluation or type decoding fails.
pub fn decode_expression<T: DecodeValue>(
    expr: &crate::ast::expr::Expression,
    ctx: &mut Context,
) -> Result<T, Diagnostics> {
    let eval = crate::eval::evaluator::Evaluator::new(ctx);
    let (val, mut diags) = eval.evaluate(expr)?;
    let res = T::decode_value(&val, expr.span()).map_err(|d| {
        diags.extend(d);
        diags
    })?;
    Ok(res)
}

/// Decodes an evaluated [`Value`] into a strongly-typed Rust value using context information.
///
/// # Arguments
/// * `val` - The value to decode.
/// * `span` - The source span of the value.
/// * `_ctx` - The context (reserved for contextual type lookups).
///
/// # Errors
/// Returns [`Diagnostics`] if value decoding fails.
pub fn decode_value_with_context<T: DecodeValue>(
    val: &Value,
    span: Span,
    _ctx: &Context,
) -> Result<T, Diagnostics> {
    T::decode_value(val, span)
}

mod decode_tests;
