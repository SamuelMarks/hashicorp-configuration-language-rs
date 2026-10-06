//! Structural Encoding (`gohcl.EncodeIntoBody` Equivalent).
//!
//! Provides [`EncodeBody`] and [`EncodeValue`] traits for serializing strongly-typed
//! Rust structures and values directly into HCL Concrete Syntax Tree ([`CstBody`]) structures.
use crate::cst::builder::{CstBlock, CstBody};
use crate::diagnostic::Diagnostics;
use crate::number::Number;
use crate::types::ty::Type;
use crate::types::val::{Value, ValueData};
pub use hcl_macros::{EncodeBody, EncodeValue};
use std::collections::{BTreeMap, HashMap};
/// Trait for encoding a Rust struct into an HCL CST body.
pub trait EncodeBody {
    /// Encodes this struct into the provided CST body.
    ///
    /// # Arguments
    /// * `body` - The target CST body to append attributes and blocks into.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if encoding fails.
    fn encode_into_body(&self, body: &mut CstBody) -> Result<(), Diagnostics>;
    /// Returns the labels associated with this struct when encoded as a block.
    #[must_use]
    fn extract_labels(&self) -> Vec<String> {
        Vec::new()
    }
    /// Encodes this struct into a new `CstBlock` with the given block type and labels.
    ///
    /// # Arguments
    /// * `block_type` - The block type identifier.
    /// * `labels` - The labels for the block.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if encoding fails.
    fn encode_into_block(
        &self,
        block_type: impl Into<String>,
        labels: Vec<String>,
    ) -> Result<CstBlock, Diagnostics> {
        let mut block = CstBlock::new(block_type, labels);
        self.encode_into_body(&mut block.body)?;
        Ok(block)
    }
}
impl EncodeBody for crate::ast::structure::Body {
    fn encode_into_body(&self, body: &mut CstBody) -> Result<(), Diagnostics> {
        for (name, attr) in &self.attributes {
            let toks = crate::cst::builder::tokens_for_expression(&attr.expr);
            let mut cst_attr = crate::cst::builder::CstAttribute::new(name, toks);
            for comment in &attr.leading_comments {
                cst_attr.with_leading_comment(comment);
            }
            if let Some(ref trailing) = attr.trailing_comment {
                cst_attr.with_trailing_inline_comment(trailing);
            }
            body.items
                .push(crate::cst::builder::CstItem::Attribute(cst_attr.clone()));
            body.attributes.push(cst_attr);
        }
        for block in &self.blocks {
            let mut cst_block = CstBlock::new(&block.block_type, block.labels.clone());
            for comment in &block.leading_comments {
                cst_block.with_leading_comment(comment);
            }
            if let Some(ref trailing) = block.trailing_comment {
                cst_block.with_trailing_inline_comment(trailing);
            }
            let _ = block.body.encode_into_body(&mut cst_block.body);
            body.append_block(cst_block);
        }
        Ok(())
    }
}
/// Trait for encoding a Rust type into an HCL [`Value`].
pub trait EncodeValue {
    /// Encodes this value into an HCL [`Value`].
    #[must_use]
    fn encode_value(&self) -> Value;
}
impl EncodeValue for bool {
    fn encode_value(&self) -> Value {
        Value::new(Type::Bool, ValueData::Bool(*self))
    }
}
impl EncodeValue for String {
    fn encode_value(&self) -> Value {
        Value::new(Type::String, ValueData::String(self.clone()))
    }
}
impl EncodeValue for &str {
    fn encode_value(&self) -> Value {
        Value::new(Type::String, ValueData::String((*self).to_string()))
    }
}
impl EncodeValue for Number {
    fn encode_value(&self) -> Value {
        Value::new(Type::Number, ValueData::Number(self.clone()))
    }
}
macro_rules! impl_encode_value_signed {
    ($($t:ty),*) => {
        $(impl EncodeValue for $t { fn encode_value(& self) -> Value {
        Value::new(Type::Number,
        ValueData::Number(Number(bigdecimal::BigDecimal::from(i64::from(* self)))),) }
        })*
    };
}
impl_encode_value_signed!(i8, i16, i32, i64);
impl EncodeValue for isize {
    fn encode_value(&self) -> Value {
        Value::new(
            Type::Number,
            ValueData::Number(Number(bigdecimal::BigDecimal::from(*self as i64))),
        )
    }
}
impl EncodeValue for i128 {
    fn encode_value(&self) -> Value {
        Value::new(
            Type::Number,
            ValueData::Number(Number(bigdecimal::BigDecimal::from(*self))),
        )
    }
}
macro_rules! impl_encode_value_unsigned {
    ($($t:ty),*) => {
        $(impl EncodeValue for $t { fn encode_value(& self) -> Value {
        Value::new(Type::Number,
        ValueData::Number(Number(bigdecimal::BigDecimal::from(u64::from(* self)))),) }
        })*
    };
}
impl_encode_value_unsigned!(u8, u16, u32, u64);
impl EncodeValue for usize {
    fn encode_value(&self) -> Value {
        Value::new(
            Type::Number,
            ValueData::Number(Number(bigdecimal::BigDecimal::from(*self as u64))),
        )
    }
}
impl EncodeValue for u128 {
    fn encode_value(&self) -> Value {
        Value::new(
            Type::Number,
            ValueData::Number(Number(bigdecimal::BigDecimal::from(*self))),
        )
    }
}
macro_rules! impl_encode_value_float {
    ($($t:ty),*) => {
        $(impl EncodeValue for $t { fn encode_value(& self) -> Value { use
        std::str::FromStr; let s = format!("{self}"); let n = Number::from_str(& s)
        .unwrap_or_else(| _ | Number::from(0_i64)); Value::new(Type::Number,
        ValueData::Number(n)) } })*
    };
}
impl_encode_value_float!(f32, f64);
impl<T: EncodeValue> EncodeValue for Vec<T> {
    fn encode_value(&self) -> Value {
        let mut vals = Vec::with_capacity(self.len());
        let mut elem_ty = Type::Dynamic;
        for item in self {
            let v = item.encode_value();
            elem_ty = v.ty().clone();
            vals.push(v);
        }
        Value::new(Type::List(Box::new(elem_ty)), ValueData::Array(vals))
    }
}
impl<T: EncodeValue> EncodeValue for Option<T> {
    fn encode_value(&self) -> Value {
        match self {
            Some(val) => val.encode_value(),
            None => Value::null(Type::Dynamic),
        }
    }
}
impl<T: EncodeValue, S: std::hash::BuildHasher + Default> EncodeValue for HashMap<String, T, S> {
    fn encode_value(&self) -> Value {
        let mut map = BTreeMap::new();
        let mut val_ty = Type::Dynamic;
        for (k, v) in self {
            let encoded = v.encode_value();
            val_ty = encoded.ty().clone();
            map.insert(k.clone(), encoded);
        }
        Value::new(Type::Map(Box::new(val_ty)), ValueData::Object(map))
    }
}
impl<T: EncodeValue> EncodeValue for BTreeMap<String, T> {
    fn encode_value(&self) -> Value {
        let mut map = BTreeMap::new();
        let mut val_ty = Type::Dynamic;
        for (k, v) in self {
            let encoded = v.encode_value();
            val_ty = encoded.ty().clone();
            map.insert(k.clone(), encoded);
        }
        Value::new(Type::Map(Box::new(val_ty)), ValueData::Object(map))
    }
}
impl EncodeValue for Value {
    fn encode_value(&self) -> Value {
        self.clone()
    }
}
/// Encodes a struct implementing [`EncodeBody`] into a formatted HCL string.
///
/// # Arguments
/// * `val` - The value to encode into HCL.
///
/// # Errors
/// Returns [`Diagnostics`] if encoding fails.
pub fn encode_to_string<T: EncodeBody>(val: &T) -> Result<String, Diagnostics> {
    let mut body = CstBody::new();
    val.encode_into_body(&mut body)?;
    let mut out = String::new();
    body.render(&mut out);
    Ok(out)
}
mod encode_tests;
