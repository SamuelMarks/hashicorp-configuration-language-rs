//! `MessagePack` serialization matching `zclconf/go-cty/cty/msgpack`.
//!
//! Provides canonical binary serialization for `cty` types and values, including
//! arbitrary-precision numbers, unknown values with refinements, nulls, and marks.
use crate::error::HclError;
use crate::number::Number;
use crate::types::refinement::Refinement;
use crate::types::ty::Type;
use crate::types::val::{Value, ValueData, ValueMark};
use bigdecimal::BigDecimal;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
/// Low-level `MessagePack` binary writer.
#[derive(Default, Debug)]
pub struct MsgPackWriter {
    buf: Vec<u8>,
}
impl MsgPackWriter {
    /// Creates a new empty [`MsgPackWriter`].
    #[must_use]
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }
    /// Consumes the writer and returns the accumulated byte buffer.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
    /// Writes a `nil` token (`0xc0`).
    pub fn write_nil(&mut self) {
        self.buf.push(0xc0);
    }
    /// Writes a boolean token (`0xc2` for false, `0xc3` for true).
    ///
    /// # Arguments
    /// * `b` - The boolean value to write.
    pub fn write_bool(&mut self, b: bool) {
        if b {
            self.buf.push(0xc3);
        } else {
            self.buf.push(0xc2);
        }
    }
    /// Writes a signed 64-bit integer.
    ///
    /// # Arguments
    /// * `n` - The signed integer to write.
    pub fn write_int(&mut self, n: i64) {
        if (-32..=127).contains(&n) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            self.buf.push(n as u8);
        } else if (0..=255).contains(&n) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                self.buf.push(0xcc);
                self.buf.push(n as u8);
            }
        } else if (0..=65535).contains(&n) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                self.buf.push(0xcd);
                self.buf.extend_from_slice(&(n as u16).to_be_bytes());
            }
        } else if (0..=i64::from(u32::MAX)).contains(&n) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                self.buf.push(0xce);
                self.buf.extend_from_slice(&(n as u32).to_be_bytes());
            }
        } else if n >= 0 {
            #[allow(clippy::cast_sign_loss)]
            {
                self.buf.push(0xcf);
                self.buf.extend_from_slice(&(n as u64).to_be_bytes());
            }
        } else if n >= -128 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                self.buf.push(0xd0);
                self.buf.push(n as i8 as u8);
            }
        } else if n >= -32768 {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xd1);
                self.buf.extend_from_slice(&(n as i16).to_be_bytes());
            }
        } else if n >= i64::from(i32::MIN) {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xd2);
                self.buf.extend_from_slice(&(n as i32).to_be_bytes());
            }
        } else {
            self.buf.push(0xd3);
            self.buf.extend_from_slice(&n.to_be_bytes());
        }
    }
    /// Writes an unsigned 64-bit integer.
    ///
    /// # Arguments
    /// * `n` - The unsigned integer to write.
    pub fn write_uint(&mut self, n: u64) {
        if n <= 127 {
            #[allow(clippy::cast_possible_truncation)]
            self.buf.push(n as u8);
        } else if n <= 255 {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xcc);
                self.buf.push(n as u8);
            }
        } else if n <= 65535 {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xcd);
                self.buf.extend_from_slice(&(n as u16).to_be_bytes());
            }
        } else if u32::try_from(n).is_ok() {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xce);
                self.buf.extend_from_slice(&(n as u32).to_be_bytes());
            }
        } else {
            self.buf.push(0xcf);
            self.buf.extend_from_slice(&n.to_be_bytes());
        }
    }
    /// Writes a 64-bit IEEE floating-point number.
    ///
    /// # Arguments
    /// * `f` - The 64-bit float to write.
    pub fn write_float(&mut self, f: f64) {
        self.buf.push(0xcb);
        self.buf.extend_from_slice(&f.to_be_bytes());
    }
    /// Writes a UTF-8 string.
    ///
    /// # Arguments
    /// * `s` - The string slice to write.
    pub fn write_str(&mut self, s: &str) {
        let len = s.len();
        if len < 32 {
            #[allow(clippy::cast_possible_truncation)]
            self.buf.push(0xa0 | (len as u8));
        } else if len < 256 {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xd9);
                self.buf.push(len as u8);
            }
        } else if len < 65536 {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xda);
                self.buf.extend_from_slice(&(len as u16).to_be_bytes());
            }
        } else {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xdb);
                self.buf.extend_from_slice(&(len as u32).to_be_bytes());
            }
        }
        self.buf.extend_from_slice(s.as_bytes());
    }
    /// Writes a binary payload.
    ///
    /// # Arguments
    /// * `bytes` - The byte slice to write.
    pub fn write_bin(&mut self, bytes: &[u8]) {
        let len = bytes.len();
        if len < 256 {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xc4);
                self.buf.push(len as u8);
            }
        } else if len < 65536 {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xc5);
                self.buf.extend_from_slice(&(len as u16).to_be_bytes());
            }
        } else {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xc6);
                self.buf.extend_from_slice(&(len as u32).to_be_bytes());
            }
        }
        self.buf.extend_from_slice(bytes);
    }
    /// Writes an array header with the specified number of elements.
    ///
    /// # Arguments
    /// * `len` - The number of elements in the array.
    pub fn write_array_header(&mut self, len: usize) {
        if len < 16 {
            #[allow(clippy::cast_possible_truncation)]
            self.buf.push(0x90 | (len as u8));
        } else if len < 65536 {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xdc);
                self.buf.extend_from_slice(&(len as u16).to_be_bytes());
            }
        } else {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xdd);
                self.buf.extend_from_slice(&(len as u32).to_be_bytes());
            }
        }
    }
    /// Writes a map header with the specified number of key-value pairs.
    ///
    /// # Arguments
    /// * `len` - The number of key-value pairs in the map.
    pub fn write_map_header(&mut self, len: usize) {
        if len < 16 {
            #[allow(clippy::cast_possible_truncation)]
            self.buf.push(0x80 | (len as u8));
        } else if len < 65536 {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xde);
                self.buf.extend_from_slice(&(len as u16).to_be_bytes());
            }
        } else {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.buf.push(0xdf);
                self.buf.extend_from_slice(&(len as u32).to_be_bytes());
            }
        }
    }
    /// Writes a `MessagePack` extension value.
    ///
    /// # Arguments
    /// * `type_code` - The signed 8-bit extension type code.
    /// * `data` - The extension payload bytes.
    pub fn write_ext(&mut self, type_code: i8, data: &[u8]) {
        #[allow(clippy::cast_sign_loss)]
        let type_byte = type_code as u8;
        let len = data.len();
        match len {
            1 => {
                self.buf.push(0xd4);
                self.buf.push(type_byte);
            }
            2 => {
                self.buf.push(0xd5);
                self.buf.push(type_byte);
            }
            4 => {
                self.buf.push(0xd6);
                self.buf.push(type_byte);
            }
            8 => {
                self.buf.push(0xd7);
                self.buf.push(type_byte);
            }
            16 => {
                self.buf.push(0xd8);
                self.buf.push(type_byte);
            }
            0..=255 => {
                self.buf.push(0xc7);
                self.buf.push(len as u8);
                self.buf.push(type_byte);
            }
            256..=65535 => {
                self.buf.push(0xc8);
                self.buf.extend_from_slice(&(len as u16).to_be_bytes());
                self.buf.push(type_byte);
            }
            _ => {
                self.buf.push(0xc9);
                self.buf.extend_from_slice(&(len as u32).to_be_bytes());
                self.buf.push(type_byte);
            }
        }
        self.buf.extend_from_slice(data);
    }
}
/// A parsed low-level `MessagePack` token.
#[derive(Debug, Clone, PartialEq)]
pub enum MsgPackToken<'a> {
    /// A `nil` value.
    Nil,
    /// A boolean value.
    Bool(bool),
    /// A signed integer.
    Int(i64),
    /// An unsigned integer.
    Uint(u64),
    /// A floating point value.
    Float(f64),
    /// A string slice.
    Str(&'a str),
    /// A binary byte slice.
    Bin(&'a [u8]),
    /// An array header with length.
    ArrayHeader(usize),
    /// A map header with pair count.
    MapHeader(usize),
    /// An extension token with type code and payload slice.
    Ext(i8, &'a [u8]),
}
/// Low-level `MessagePack` zero-copy byte decoder.
#[derive(Debug)]
pub struct MsgPackReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> MsgPackReader<'a> {
    /// Creates a new [`MsgPackReader`] over the given slice.
    ///
    /// # Arguments
    /// * `bytes` - The input bytes to read.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    /// Returns the number of unconsumed bytes remaining.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }
    fn read_u8(&mut self) -> Result<u8, HclError> {
        if self.pos >= self.bytes.len() {
            return Err(HclError::MsgPackDecode(
                "unexpected end of buffer".to_string(),
            ));
        }
        let b = self.bytes[self.pos];
        self.pos += 1;
        Ok(b)
    }
    fn peek_u8(&self) -> Result<u8, HclError> {
        if self.pos >= self.bytes.len() {
            return Err(HclError::MsgPackDecode(
                "unexpected end of buffer".to_string(),
            ));
        }
        Ok(self.bytes[self.pos])
    }
    fn read_exact(&mut self, n: usize) -> Result<&'a [u8], HclError> {
        if self.pos + n > self.bytes.len() {
            return Err(HclError::MsgPackDecode(
                "unexpected end of buffer".to_string(),
            ));
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }
    fn read_2_bytes(&mut self) -> Result<[u8; 2], HclError> {
        let slice = self.read_exact(2)?;
        let mut arr = [0u8; 2];
        arr.copy_from_slice(slice);
        Ok(arr)
    }
    fn read_4_bytes(&mut self) -> Result<[u8; 4], HclError> {
        let slice = self.read_exact(4)?;
        let mut arr = [0u8; 4];
        arr.copy_from_slice(slice);
        Ok(arr)
    }
    fn read_8_bytes(&mut self) -> Result<[u8; 8], HclError> {
        let slice = self.read_exact(8)?;
        let mut arr = [0u8; 8];
        arr.copy_from_slice(slice);
        Ok(arr)
    }
    /// Reads and decodes the next [`MsgPackToken`].
    ///
    /// # Errors
    /// Returns [`HclError::MsgPackDecode`] if the bytes are malformed or buffer is truncated.
    pub fn read_token(&mut self) -> Result<MsgPackToken<'a>, HclError> {
        let b = self.read_u8()?;
        match b {
            0x00..=0x7f => Ok(MsgPackToken::Uint(u64::from(b))),
            0x80..=0x8f => Ok(MsgPackToken::MapHeader(usize::from(b & 0x0f))),
            0x90..=0x9f => Ok(MsgPackToken::ArrayHeader(usize::from(b & 0x0f))),
            0xa0..=0xbf => {
                let len = usize::from(b & 0x1f);
                let slice = self.read_exact(len)?;
                let s = std::str::from_utf8(slice)
                    .map_err(|e| HclError::MsgPackDecode(format!("invalid utf8 string: {e}")))?;
                Ok(MsgPackToken::Str(s))
            }
            0xc0 => Ok(MsgPackToken::Nil),
            0xc2 => Ok(MsgPackToken::Bool(false)),
            0xc3 => Ok(MsgPackToken::Bool(true)),
            0xc4 => {
                let len = usize::from(self.read_u8()?);
                Ok(MsgPackToken::Bin(self.read_exact(len)?))
            }
            0xc5 => {
                let len = usize::from(u16::from_be_bytes(self.read_2_bytes()?));
                Ok(MsgPackToken::Bin(self.read_exact(len)?))
            }
            0xc6 => {
                #[allow(clippy::cast_possible_truncation)]
                let len = u32::from_be_bytes(self.read_4_bytes()?) as usize;
                Ok(MsgPackToken::Bin(self.read_exact(len)?))
            }
            0xc7 => {
                let len = usize::from(self.read_u8()?);
                #[allow(clippy::cast_possible_wrap)]
                let code = self.read_u8()? as i8;
                Ok(MsgPackToken::Ext(code, self.read_exact(len)?))
            }
            0xc8 => {
                let len = usize::from(u16::from_be_bytes(self.read_2_bytes()?));
                #[allow(clippy::cast_possible_wrap)]
                let code = self.read_u8()? as i8;
                Ok(MsgPackToken::Ext(code, self.read_exact(len)?))
            }
            0xc9 => {
                #[allow(clippy::cast_possible_truncation)]
                let len = u32::from_be_bytes(self.read_4_bytes()?) as usize;
                #[allow(clippy::cast_possible_wrap)]
                let code = self.read_u8()? as i8;
                Ok(MsgPackToken::Ext(code, self.read_exact(len)?))
            }
            0xca => {
                let f_bytes = self.read_4_bytes()?;
                Ok(MsgPackToken::Float(f64::from(f32::from_be_bytes(f_bytes))))
            }
            0xcb => {
                let f_bytes = self.read_8_bytes()?;
                Ok(MsgPackToken::Float(f64::from_be_bytes(f_bytes)))
            }
            0xcc => {
                let val = self.read_u8()?;
                Ok(MsgPackToken::Uint(u64::from(val)))
            }
            0xcd => {
                let val = u16::from_be_bytes(self.read_2_bytes()?);
                Ok(MsgPackToken::Uint(u64::from(val)))
            }
            0xce => {
                let val = u32::from_be_bytes(self.read_4_bytes()?);
                Ok(MsgPackToken::Uint(u64::from(val)))
            }
            0xcf => {
                let val = u64::from_be_bytes(self.read_8_bytes()?);
                Ok(MsgPackToken::Uint(val))
            }
            0xd0 => {
                #[allow(clippy::cast_possible_wrap)]
                let val = self.read_u8()? as i8;
                Ok(MsgPackToken::Int(i64::from(val)))
            }
            0xd1 => {
                let val = i16::from_be_bytes(self.read_2_bytes()?);
                Ok(MsgPackToken::Int(i64::from(val)))
            }
            0xd2 => {
                let val = i32::from_be_bytes(self.read_4_bytes()?);
                Ok(MsgPackToken::Int(i64::from(val)))
            }
            0xd3 => {
                let val = i64::from_be_bytes(self.read_8_bytes()?);
                Ok(MsgPackToken::Int(val))
            }
            0xd4 => {
                #[allow(clippy::cast_possible_wrap)]
                let code = self.read_u8()? as i8;
                Ok(MsgPackToken::Ext(code, self.read_exact(1)?))
            }
            0xd5 => {
                #[allow(clippy::cast_possible_wrap)]
                let code = self.read_u8()? as i8;
                Ok(MsgPackToken::Ext(code, self.read_exact(2)?))
            }
            0xd6 => {
                #[allow(clippy::cast_possible_wrap)]
                let code = self.read_u8()? as i8;
                Ok(MsgPackToken::Ext(code, self.read_exact(4)?))
            }
            0xd7 => {
                #[allow(clippy::cast_possible_wrap)]
                let code = self.read_u8()? as i8;
                Ok(MsgPackToken::Ext(code, self.read_exact(8)?))
            }
            0xd8 => {
                #[allow(clippy::cast_possible_wrap)]
                let code = self.read_u8()? as i8;
                Ok(MsgPackToken::Ext(code, self.read_exact(16)?))
            }
            0xd9 => {
                let len = usize::from(self.read_u8()?);
                let slice = self.read_exact(len)?;
                let s = std::str::from_utf8(slice)
                    .map_err(|e| HclError::MsgPackDecode(format!("invalid utf8 string: {e}")))?;
                Ok(MsgPackToken::Str(s))
            }
            0xda => {
                let len = usize::from(u16::from_be_bytes(self.read_2_bytes()?));
                let slice = self.read_exact(len)?;
                let s = std::str::from_utf8(slice)
                    .map_err(|e| HclError::MsgPackDecode(format!("invalid utf8 string: {e}")))?;
                Ok(MsgPackToken::Str(s))
            }
            0xdb => {
                #[allow(clippy::cast_possible_truncation)]
                let len = u32::from_be_bytes(self.read_4_bytes()?) as usize;
                let slice = self.read_exact(len)?;
                let s = std::str::from_utf8(slice)
                    .map_err(|e| HclError::MsgPackDecode(format!("invalid utf8 string: {e}")))?;
                Ok(MsgPackToken::Str(s))
            }
            0xdc => {
                let len = usize::from(u16::from_be_bytes(self.read_2_bytes()?));
                Ok(MsgPackToken::ArrayHeader(len))
            }
            0xdd => {
                #[allow(clippy::cast_possible_truncation)]
                let len = u32::from_be_bytes(self.read_4_bytes()?) as usize;
                Ok(MsgPackToken::ArrayHeader(len))
            }
            0xde => {
                let len = usize::from(u16::from_be_bytes(self.read_2_bytes()?));
                Ok(MsgPackToken::MapHeader(len))
            }
            0xdf => {
                #[allow(clippy::cast_possible_truncation)]
                let len = u32::from_be_bytes(self.read_4_bytes()?) as usize;
                Ok(MsgPackToken::MapHeader(len))
            }
            0xe0..=0xff => {
                #[allow(clippy::cast_possible_wrap)]
                let val = b as i8;
                Ok(MsgPackToken::Int(i64::from(val)))
            }
            _ => Err(HclError::MsgPackDecode(format!(
                "unsupported messagepack byte: 0x{b:02x}"
            ))),
        }
    }
}
fn encode_type_internal(w: &mut MsgPackWriter, ty: &Type) {
    match ty {
        Type::Number => w.write_str("N"),
        Type::String => w.write_str("S"),
        Type::Bool => w.write_str("B"),
        Type::Dynamic => w.write_str("?"),
        Type::List(elem) => {
            w.write_array_header(2);
            w.write_str("L");
            encode_type_internal(w, elem);
        }
        Type::Set(elem) => {
            w.write_array_header(2);
            w.write_str("Z");
            encode_type_internal(w, elem);
        }
        Type::Map(elem) => {
            w.write_array_header(2);
            w.write_str("M");
            encode_type_internal(w, elem);
        }
        Type::Tuple(elems) => {
            w.write_array_header(2);
            w.write_str("T");
            w.write_array_header(elems.len());
            for elem in elems {
                encode_type_internal(w, elem);
            }
        }
        Type::Object { attrs, .. } => {
            w.write_array_header(2);
            w.write_str("O");
            w.write_map_header(attrs.len());
            for (k, v) in attrs {
                w.write_str(k);
                encode_type_internal(w, v);
            }
        }
        Type::Capsule { name, .. } => {
            w.write_array_header(2);
            w.write_str("C");
            w.write_str(name);
        }
    }
}
fn decode_type_internal(reader: &mut MsgPackReader<'_>) -> Result<Type, HclError> {
    let token = reader.read_token()?;
    match token {
        MsgPackToken::Str("N") => Ok(Type::Number),
        MsgPackToken::Str("S") => Ok(Type::String),
        MsgPackToken::Str("B") => Ok(Type::Bool),
        MsgPackToken::Str("?") => Ok(Type::Dynamic),
        MsgPackToken::ArrayHeader(2) => {
            let kind_token = reader.read_token()?;
            match kind_token {
                MsgPackToken::Str("L") => {
                    let elem = decode_type_internal(reader)?;
                    Ok(Type::List(Box::new(elem)))
                }
                MsgPackToken::Str("Z") => {
                    let elem = decode_type_internal(reader)?;
                    Ok(Type::Set(Box::new(elem)))
                }
                MsgPackToken::Str("M") => {
                    let elem = decode_type_internal(reader)?;
                    Ok(Type::Map(Box::new(elem)))
                }
                MsgPackToken::Str("T") => {
                    let tup_hdr = reader.read_token()?;
                    let MsgPackToken::ArrayHeader(count) = tup_hdr else {
                        return Err(HclError::MsgPackDecode(
                            "expected array header for tuple types".to_string(),
                        ));
                    };
                    let mut elems = Vec::with_capacity(count);
                    for _ in 0..count {
                        elems.push(decode_type_internal(reader)?);
                    }
                    Ok(Type::Tuple(elems))
                }
                MsgPackToken::Str("O") => {
                    let map_hdr = reader.read_token()?;
                    let MsgPackToken::MapHeader(count) = map_hdr else {
                        return Err(HclError::MsgPackDecode(
                            "expected map header for object types".to_string(),
                        ));
                    };
                    let mut attrs = BTreeMap::new();
                    for _ in 0..count {
                        let k_tok = reader.read_token()?;
                        let MsgPackToken::Str(s) = k_tok else {
                            return Err(HclError::MsgPackDecode(
                                "object type attribute name must be string".to_string(),
                            ));
                        };
                        let k = s.to_string();
                        let v = decode_type_internal(reader)?;
                        attrs.insert(k, v);
                    }
                    Ok(Type::object(attrs))
                }
                MsgPackToken::Str("C") => {
                    let name_tok = reader.read_token()?;
                    let MsgPackToken::Str(name) = name_tok else {
                        return Err(HclError::MsgPackDecode(
                            "capsule type name must be string".to_string(),
                        ));
                    };
                    Ok(Type::capsule::<()>(Box::leak(
                        name.to_string().into_boxed_str(),
                    )))
                }
                other => Err(HclError::MsgPackDecode(format!(
                    "unknown compound type tag: {other:?}"
                ))),
            }
        }
        other => Err(HclError::MsgPackDecode(format!(
            "unexpected token when decoding cty.Type: {other:?}"
        ))),
    }
}
/// Serializes a [`Type`] into canonical `MessagePack` binary format.
///
/// # Arguments
/// * `ty` - The type to serialize.
///
/// # Errors
/// Returns [`HclError::MsgPackEncode`] on serialization failure.
pub fn encode_type(ty: &Type) -> Result<Vec<u8>, HclError> {
    let mut writer = MsgPackWriter::new();
    encode_type_internal(&mut writer, ty);
    Ok(writer.into_bytes())
}
/// Deserializes a [`Type`] from canonical `MessagePack` binary format.
///
/// # Arguments
/// * `bytes` - The `MessagePack` bytes to decode.
///
/// # Errors
/// Returns [`HclError::MsgPackDecode`] if the bytes are malformed.
pub fn decode_type(bytes: &[u8]) -> Result<Type, HclError> {
    let mut reader = MsgPackReader::new(bytes);
    let ty = decode_type_internal(&mut reader)?;
    if reader.remaining() > 0 {
        return Err(HclError::MsgPackDecode(
            "trailing unread bytes after type decode".to_string(),
        ));
    }
    Ok(ty)
}
fn encode_refinement(w: &mut MsgPackWriter, r: &Refinement) {
    w.write_array_header(10);
    w.write_bool(r.not_null);
    if let Some(min) = r.string_length_min {
        #[allow(clippy::cast_possible_wrap)]
        w.write_int(min as i64);
    } else {
        w.write_nil();
    }
    if let Some(max) = r.string_length_max {
        #[allow(clippy::cast_possible_wrap)]
        w.write_int(max as i64);
    } else {
        w.write_nil();
    }
    if let Some(ref p) = r.string_prefix {
        w.write_str(p);
    } else {
        w.write_nil();
    }
    if let Some(ref s) = r.string_suffix {
        w.write_str(s);
    } else {
        w.write_nil();
    }
    if let Some(min) = r.collection_length_min {
        #[allow(clippy::cast_possible_wrap)]
        w.write_int(min as i64);
    } else {
        w.write_nil();
    }
    if let Some(max) = r.collection_length_max {
        #[allow(clippy::cast_possible_wrap)]
        w.write_int(max as i64);
    } else {
        w.write_nil();
    }
    if let Some(ref min_n) = r.number_min {
        w.write_str(&min_n.to_string());
    } else {
        w.write_nil();
    }
    if let Some(ref max_n) = r.number_max {
        w.write_str(&max_n.to_string());
    } else {
        w.write_nil();
    }
    w.write_map_header(r.object_attrs.len());
    for (k, attr_ref) in &r.object_attrs {
        w.write_str(k);
        encode_refinement(w, attr_ref);
    }
}
fn decode_refinement(reader: &mut MsgPackReader<'_>) -> Result<Refinement, HclError> {
    let tok = reader.read_token()?;
    let MsgPackToken::ArrayHeader(count) = tok else {
        return Err(HclError::MsgPackDecode(
            "expected array header for refinement".to_string(),
        ));
    };
    if count < 10 {
        return Err(HclError::MsgPackDecode(
            "refinement array must have at least 10 fields".to_string(),
        ));
    }
    let not_null = match reader.read_token()? {
        MsgPackToken::Bool(b) => b,
        _ => false,
    };
    let string_length_min = match reader.read_token()? {
        MsgPackToken::Uint(u) => usize::try_from(u).ok(),
        MsgPackToken::Int(i) if i >= 0 => usize::try_from(i).ok(),
        _ => None,
    };
    let string_length_max = match reader.read_token()? {
        MsgPackToken::Uint(u) => usize::try_from(u).ok(),
        MsgPackToken::Int(i) if i >= 0 => usize::try_from(i).ok(),
        _ => None,
    };
    let string_prefix = match reader.read_token()? {
        MsgPackToken::Str(s) => Some(s.to_string()),
        _ => None,
    };
    let string_suffix = match reader.read_token()? {
        MsgPackToken::Str(s) => Some(s.to_string()),
        _ => None,
    };
    let collection_length_min = match reader.read_token()? {
        MsgPackToken::Uint(u) => usize::try_from(u).ok(),
        MsgPackToken::Int(i) if i >= 0 => usize::try_from(i).ok(),
        _ => None,
    };
    let collection_length_max = match reader.read_token()? {
        MsgPackToken::Uint(u) => usize::try_from(u).ok(),
        MsgPackToken::Int(i) if i >= 0 => usize::try_from(i).ok(),
        _ => None,
    };
    let number_min = match reader.read_token()? {
        MsgPackToken::Str(s) => Number::from_str(s).ok(),
        MsgPackToken::Int(i) => Some(Number::from(i)),
        MsgPackToken::Uint(u) => Some(Number::from(u)),
        _ => None,
    };
    let number_max = match reader.read_token()? {
        MsgPackToken::Str(s) => Number::from_str(s).ok(),
        MsgPackToken::Int(i) => Some(Number::from(i)),
        MsgPackToken::Uint(u) => Some(Number::from(u)),
        _ => None,
    };
    let map_tok = reader.read_token()?;
    let attr_count = match map_tok {
        MsgPackToken::MapHeader(n) => n,
        _ => 0,
    };
    let mut object_attrs = BTreeMap::new();
    for _ in 0..attr_count {
        let k = match reader.read_token()? {
            MsgPackToken::Str(s) => s.to_string(),
            _ => continue,
        };
        let attr_r = decode_refinement(reader)?;
        object_attrs.insert(k, attr_r);
    }
    Ok(Refinement {
        not_null,
        string_length_min,
        string_length_max,
        string_prefix,
        string_suffix,
        collection_length_min,
        collection_length_max,
        number_min,
        number_max,
        object_attrs,
    })
}
fn encode_value_data(w: &mut MsgPackWriter, val: &Value) {
    match &*val.data {
        ValueData::Null => w.write_nil(),
        ValueData::Unknown(maybe_ref) => {
            if let Some(r) = maybe_ref {
                let mut ref_writer = MsgPackWriter::new();
                encode_refinement(&mut ref_writer, r);
                w.write_ext(0, &ref_writer.into_bytes());
            } else {
                w.write_ext(0, &[0x00]);
            }
        }
        ValueData::Bool(b) => w.write_bool(*b),
        ValueData::Number(n) => {
            use bigdecimal::num_traits::ToPrimitive;
            if n.0.is_integer() {
                if let Some(i) = n.0.to_i64() {
                    w.write_int(i);
                } else if let Some(u) = n.0.to_u64() {
                    w.write_uint(u);
                } else {
                    w.write_str(&n.to_string());
                }
            } else {
                w.write_str(&n.to_string());
            }
        }
        ValueData::String(s) => w.write_str(s),
        ValueData::Array(arr) => {
            w.write_array_header(arr.len());
            for item in arr {
                encode_value_internal(w, item);
            }
        }
        ValueData::Set(set) => {
            w.write_array_header(set.len());
            for item in set {
                encode_value_internal(w, item);
            }
        }
        ValueData::Object(obj) => {
            w.write_map_header(obj.len());
            for (k, v) in obj {
                w.write_str(k);
                encode_value_internal(w, v);
            }
        }
        ValueData::Capsule(_) => {
            let cap_name = val.ty().capsule_name().unwrap_or("custom_capsule");
            w.write_ext(2, cap_name.as_bytes());
        }
    }
}
fn encode_value_internal(w: &mut MsgPackWriter, val: &Value) {
    if val.marks.is_empty() {
        encode_value_data(w, val);
    } else {
        w.write_array_header(3);
        w.write_uint(1);
        encode_value_data(w, val);
        w.write_array_header(val.marks.len());
        for m in &val.marks {
            match m {
                ValueMark::Sensitive => w.write_str("sensitive"),
                ValueMark::Custom(s) => w.write_str(s),
                ValueMark::Typed(t) => w.write_str(&t.to_string()),
            }
        }
    }
}
fn decode_value_internal(
    reader: &mut MsgPackReader<'_>,
    expected_type: &Type,
) -> Result<Value, HclError> {
    let peek_b = reader.peek_u8()?;
    if peek_b == 0x93 {
        let mut lookahead = MsgPackReader::new(&reader.bytes[reader.pos..]);
        let is_marked_envelope = matches!(
            (lookahead.read_token(), lookahead.read_token()),
            (
                Ok(MsgPackToken::ArrayHeader(3)),
                Ok(MsgPackToken::Uint(1) | MsgPackToken::Int(1))
            )
        );
        if is_marked_envelope {
            let _ = reader.read_token();
            let _ = reader.read_token();
            let mut inner_val = decode_value_data(reader, expected_type)?;
            let marks_hdr = reader.read_token()?;
            let marks_count = match marks_hdr {
                MsgPackToken::ArrayHeader(n) => n,
                _ => 0,
            };
            for _ in 0..marks_count {
                if let Ok(MsgPackToken::Str(m_str)) = reader.read_token() {
                    if m_str == "sensitive" {
                        inner_val.marks.insert(ValueMark::Sensitive);
                    } else {
                        inner_val.marks.insert(ValueMark::Custom(m_str.to_string()));
                    }
                }
            }
            return Ok(inner_val);
        }
    }
    decode_value_data(reader, expected_type)
}
fn decode_value_data(
    reader: &mut MsgPackReader<'_>,
    expected_type: &Type,
) -> Result<Value, HclError> {
    let token = reader.read_token()?;
    match token {
        MsgPackToken::Nil => Ok(Value::null(expected_type.clone())),
        MsgPackToken::Ext(0, payload) => {
            if payload.is_empty() || payload == [0x00] {
                Ok(Value::unknown(expected_type.clone()))
            } else {
                let mut ref_reader = MsgPackReader::new(payload);
                let refinement = decode_refinement(&mut ref_reader)?;
                Ok(Value::unknown_refined(expected_type.clone(), refinement))
            }
        }
        MsgPackToken::Ext(2, _payload) => {
            let name = expected_type.capsule_name().unwrap_or("capsule");
            Ok(Value::capsule(name, ()))
        }
        MsgPackToken::Bool(b) => Ok(Value::new(Type::Bool, ValueData::Bool(b))),
        MsgPackToken::Int(i) => Ok(Value::new(Type::Number, ValueData::Number(Number::from(i)))),
        MsgPackToken::Uint(u) => Ok(Value::new(Type::Number, ValueData::Number(Number::from(u)))),
        MsgPackToken::Float(f) => {
            let bd = BigDecimal::from_str(&f.to_string()).unwrap_or_default();
            Ok(Value::new(Type::Number, ValueData::Number(Number::new(bd))))
        }
        MsgPackToken::Str(s) => {
            if expected_type == &Type::Number {
                let bd = BigDecimal::from_str(s).map_err(|e| {
                    HclError::MsgPackDecode(format!("failed to parse number string: {e}"))
                })?;
                Ok(Value::new(Type::Number, ValueData::Number(Number::new(bd))))
            } else {
                Ok(Value::new(Type::String, ValueData::String(s.to_string())))
            }
        }
        MsgPackToken::Bin(bytes) => {
            let s = std::str::from_utf8(bytes)
                .map_err(|e| HclError::MsgPackDecode(format!("invalid utf8 binary data: {e}")))?;
            Ok(Value::new(Type::String, ValueData::String(s.to_string())))
        }
        MsgPackToken::ArrayHeader(count) => match expected_type {
            Type::List(elem_ty) => {
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(decode_value_internal(reader, elem_ty)?);
                }
                Ok(Value::new(expected_type.clone(), ValueData::Array(items)))
            }
            Type::Set(elem_ty) => {
                let mut items = BTreeSet::new();
                for _ in 0..count {
                    items.insert(decode_value_internal(reader, elem_ty)?);
                }
                Ok(Value::new(expected_type.clone(), ValueData::Set(items)))
            }
            Type::Tuple(elem_tys) => {
                let mut items = Vec::with_capacity(count);
                for i in 0..count {
                    let elem_ty = elem_tys.get(i).unwrap_or(&Type::Dynamic);
                    items.push(decode_value_internal(reader, elem_ty)?);
                }
                Ok(Value::new(expected_type.clone(), ValueData::Array(items)))
            }
            _ => {
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(decode_value_internal(reader, &Type::Dynamic)?);
                }
                Ok(Value::new(
                    Type::Tuple(vec![Type::Dynamic; count]),
                    ValueData::Array(items),
                ))
            }
        },
        MsgPackToken::MapHeader(count) => match expected_type {
            Type::Map(elem_ty) => {
                let mut map = BTreeMap::new();
                for _ in 0..count {
                    let k = match reader.read_token()? {
                        MsgPackToken::Str(s) => s.to_string(),
                        _ => {
                            return Err(HclError::MsgPackDecode(
                                "map key must be a string".to_string(),
                            ));
                        }
                    };
                    let v = decode_value_internal(reader, elem_ty)?;
                    map.insert(k, v);
                }
                Ok(Value::new(expected_type.clone(), ValueData::Object(map)))
            }
            Type::Object { attrs, .. } => {
                let mut map = BTreeMap::new();
                for _ in 0..count {
                    let k = match reader.read_token()? {
                        MsgPackToken::Str(s) => s.to_string(),
                        _ => {
                            return Err(HclError::MsgPackDecode(
                                "object key must be a string".to_string(),
                            ));
                        }
                    };
                    let attr_ty = attrs.get(&k).unwrap_or(&Type::Dynamic);
                    let v = decode_value_internal(reader, attr_ty)?;
                    map.insert(k, v);
                }
                Ok(Value::new(expected_type.clone(), ValueData::Object(map)))
            }
            _ => {
                let mut map = BTreeMap::new();
                let mut attr_tys = BTreeMap::new();
                for _ in 0..count {
                    let k = match reader.read_token()? {
                        MsgPackToken::Str(s) => s.to_string(),
                        _ => {
                            return Err(HclError::MsgPackDecode(
                                "object key must be a string".to_string(),
                            ));
                        }
                    };
                    let v = decode_value_internal(reader, &Type::Dynamic)?;
                    attr_tys.insert(k.clone(), v.ty().clone());
                    map.insert(k, v);
                }
                Ok(Value::new(Type::object(attr_tys), ValueData::Object(map)))
            }
        },
        MsgPackToken::Ext(code, _) => Err(HclError::MsgPackDecode(format!(
            "unexpected extension code {code} for cty.Value"
        ))),
    }
}
/// Serializes a [`Value`] into canonical `MessagePack` binary format.
///
/// # Arguments
/// * `val` - The value to serialize.
///
/// # Errors
/// Returns [`HclError::MsgPackEncode`] if the value cannot be encoded.
pub fn encode_value(val: &Value) -> Result<Vec<u8>, HclError> {
    let mut writer = MsgPackWriter::new();
    encode_value_internal(&mut writer, val);
    Ok(writer.into_bytes())
}
/// Deserializes a [`Value`] from canonical `MessagePack` binary format given an expected [`Type`].
///
/// # Arguments
/// * `bytes` - The `MessagePack` bytes to decode.
/// * `expected_type` - The target type schema to conform to.
///
/// # Errors
/// Returns [`HclError::MsgPackDecode`] if decoding fails or the payload is malformed.
pub fn decode_value(bytes: &[u8], expected_type: &Type) -> Result<Value, HclError> {
    let mut reader = MsgPackReader::new(bytes);
    let val = decode_value_internal(&mut reader, expected_type)?;
    if reader.remaining() > 0 {
        return Err(HclError::MsgPackDecode(
            "trailing unread bytes after value decode".to_string(),
        ));
    }
    Ok(val)
}
#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::pedantic,
        clippy::nursery
    )]
    use super::*;
    use crate::number::Number;
    use crate::types::refinement::Refinement;
    use crate::types::ty::Type;
    use crate::types::val::{Value, ValueData, ValueMark};
    use std::collections::{BTreeMap, BTreeSet};
    #[test]
    fn test_msgpack_primitive_types_roundtrip() {
        let types = [Type::Number, Type::String, Type::Bool, Type::Dynamic];
        for ty in &types {
            let encoded = ty.to_msgpack().unwrap();
            let decoded = Type::from_msgpack(&encoded).unwrap();
            assert_eq!(ty, &decoded);
        }
        assert_eq!(Type::Number.to_msgpack().unwrap(), vec![0xa1, b'N']);
        assert_eq!(Type::String.to_msgpack().unwrap(), vec![0xa1, b'S']);
        assert_eq!(Type::Bool.to_msgpack().unwrap(), vec![0xa1, b'B']);
        assert_eq!(Type::Dynamic.to_msgpack().unwrap(), vec![0xa1, b'?']);
    }
    #[test]
    fn test_msgpack_compound_types_roundtrip() {
        let list_ty = Type::List(Box::new(Type::String));
        let set_ty = Type::Set(Box::new(Type::Number));
        let map_ty = Type::Map(Box::new(Type::Bool));
        let tup_ty = Type::Tuple(vec![Type::String, Type::Number, Type::Bool]);
        let mut obj_attrs = BTreeMap::new();
        obj_attrs.insert("host".to_string(), Type::String);
        obj_attrs.insert("port".to_string(), Type::Number);
        let obj_ty = Type::object(obj_attrs);
        let cap_ty = Type::capsule::<()>("my_custom_capsule");
        let compound = [list_ty, set_ty, map_ty, tup_ty, obj_ty, cap_ty];
        for ty in &compound {
            let encoded = ty.to_msgpack().unwrap();
            let decoded = Type::from_msgpack(&encoded).unwrap();
            assert_eq!(ty, &decoded);
        }
    }
    #[test]
    fn test_msgpack_primitive_values_roundtrip() {
        let null_str = Value::null(Type::String);
        let null_enc = null_str.to_msgpack().unwrap();
        assert_eq!(null_enc, vec![0xc0]);
        let null_dec = Value::from_msgpack(&null_enc, &Type::String).unwrap();
        assert_eq!(null_str, null_dec);
        let t_val = Value::new(Type::Bool, ValueData::Bool(true));
        let f_val = Value::new(Type::Bool, ValueData::Bool(false));
        assert_eq!(t_val.to_msgpack().unwrap(), vec![0xc3]);
        assert_eq!(f_val.to_msgpack().unwrap(), vec![0xc2]);
        assert_eq!(Value::from_msgpack(&[0xc3], &Type::Bool).unwrap(), t_val);
        assert_eq!(Value::from_msgpack(&[0xc2], &Type::Bool).unwrap(), f_val);
        let str_val = Value::new(Type::String, ValueData::String("hello world".to_string()));
        let str_enc = str_val.to_msgpack().unwrap();
        let str_dec = Value::from_msgpack(&str_enc, &Type::String).unwrap();
        assert_eq!(str_val, str_dec);
        let num_int = Value::new(Type::Number, ValueData::Number(Number::from(42)));
        let num_enc = num_int.to_msgpack().unwrap();
        let num_dec = Value::from_msgpack(&num_enc, &Type::Number).unwrap();
        assert_eq!(num_int, num_dec);
        let num_neg = Value::new(Type::Number, ValueData::Number(Number::from(-100)));
        let neg_enc = num_neg.to_msgpack().unwrap();
        let neg_dec = Value::from_msgpack(&neg_enc, &Type::Number).unwrap();
        assert_eq!(num_neg, neg_dec);
        let num_big = Value::new(
            Type::Number,
            ValueData::Number(
                Number::from_str("123456789012345678901234567890.123456789").unwrap(),
            ),
        );
        let big_enc = num_big.to_msgpack().unwrap();
        let big_dec = Value::from_msgpack(&big_enc, &Type::Number).unwrap();
        assert_eq!(num_big, big_dec);
    }
    #[test]
    fn test_msgpack_collections_roundtrip() {
        let list_val = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(vec![
                Value::new(Type::String, ValueData::String("a".to_string())),
                Value::new(Type::String, ValueData::String("b".to_string())),
            ]),
        );
        let list_enc = list_val.to_msgpack().unwrap();
        let list_dec = Value::from_msgpack(&list_enc, list_val.ty()).unwrap();
        assert_eq!(list_val, list_dec);
        let mut set = BTreeSet::new();
        set.insert(Value::new(Type::Number, ValueData::Number(Number::from(1))));
        set.insert(Value::new(Type::Number, ValueData::Number(Number::from(2))));
        let set_val = Value::new(Type::Set(Box::new(Type::Number)), ValueData::Set(set));
        let set_enc = set_val.to_msgpack().unwrap();
        let set_dec = Value::from_msgpack(&set_enc, set_val.ty()).unwrap();
        assert_eq!(set_val, set_dec);
        let tup_val = Value::new(
            Type::Tuple(vec![Type::String, Type::Number]),
            ValueData::Array(vec![
                Value::new(Type::String, ValueData::String("x".to_string())),
                Value::new(Type::Number, ValueData::Number(Number::from(99))),
            ]),
        );
        let tup_enc = tup_val.to_msgpack().unwrap();
        let tup_dec = Value::from_msgpack(&tup_enc, tup_val.ty()).unwrap();
        assert_eq!(tup_val, tup_dec);
        let mut obj = BTreeMap::new();
        obj.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("alpha".to_string())),
        );
        obj.insert(
            "count".to_string(),
            Value::new(Type::Number, ValueData::Number(Number::from(10))),
        );
        let obj_val = Value::new(
            Type::object(BTreeMap::from([
                ("name".to_string(), Type::String),
                ("count".to_string(), Type::Number),
            ])),
            ValueData::Object(obj),
        );
        let obj_enc = obj_val.to_msgpack().unwrap();
        let obj_dec = Value::from_msgpack(&obj_enc, obj_val.ty()).unwrap();
        assert_eq!(obj_val, obj_dec);
    }
    #[test]
    fn test_msgpack_unknown_and_refinements_roundtrip() {
        let unk_val = Value::unknown(Type::String);
        let unk_enc = unk_val.to_msgpack().unwrap();
        assert_eq!(unk_enc, vec![0xd4, 0x00, 0x00]);
        let unk_dec = Value::from_msgpack(&unk_enc, &Type::String).unwrap();
        assert!(unk_dec.is_unknown());
        assert_eq!(unk_dec.refinement(), None);
        let r = Refinement::not_null()
            .with_prefix("server-")
            .with_suffix(".internal")
            .with_string_length(10, 40)
            .unwrap()
            .with_collection_length(1, 100)
            .unwrap()
            .with_number_range(Number::from(1), Number::from(100))
            .unwrap()
            .with_object_attr("ip", Refinement::not_null().with_prefix("10."));
        let ref_val = Value::unknown_refined(Type::String, r.clone());
        let ref_enc = ref_val.to_msgpack().unwrap();
        let ref_dec = Value::from_msgpack(&ref_enc, &Type::String).unwrap();
        assert!(ref_dec.is_unknown());
        let decoded_ref = ref_dec.refinement().unwrap();
        assert!(decoded_ref.not_null);
        assert_eq!(decoded_ref.string_prefix.as_deref(), Some("server-"));
        assert_eq!(decoded_ref.string_suffix.as_deref(), Some(".internal"));
        assert_eq!(decoded_ref.string_length_min, Some(10));
        assert_eq!(decoded_ref.string_length_max, Some(40));
        assert_eq!(decoded_ref.collection_length_min, Some(1));
        assert_eq!(decoded_ref.collection_length_max, Some(100));
        assert_eq!(decoded_ref.number_min, Some(Number::from(1)));
        assert_eq!(decoded_ref.number_max, Some(Number::from(100)));
        assert!(decoded_ref.object_attrs.contains_key("ip"));
        let mut w_empty = MsgPackWriter::new();
        w_empty.write_ext(0, &[]);
        let empty_unk = Value::from_msgpack(&w_empty.into_bytes(), &Type::String).unwrap();
        assert!(empty_unk.is_unknown());
        let mut ref_payload = MsgPackWriter::new();
        ref_payload.write_array_header(10);
        ref_payload.write_bool(false);
        ref_payload.write_int(-1);
        ref_payload.write_int(-2);
        ref_payload.write_nil();
        ref_payload.write_nil();
        ref_payload.write_int(-3);
        ref_payload.write_int(-4);
        ref_payload.write_nil();
        ref_payload.write_nil();
        ref_payload.write_map_header(0);
        let mut w_ref = MsgPackWriter::new();
        w_ref.write_ext(0, &ref_payload.into_bytes());
        let dec_ref = Value::from_msgpack(&w_ref.into_bytes(), &Type::String).unwrap();
        assert!(dec_ref.is_unknown());
        let r = dec_ref.refinement().unwrap();
        assert_eq!(r.string_length_min, None);
        assert_eq!(r.string_length_max, None);
        assert_eq!(r.collection_length_min, None);
        assert_eq!(r.collection_length_max, None);
    }
    #[test]
    fn test_msgpack_marked_values_roundtrip() {
        let mut val = Value::new(Type::String, ValueData::String("secret".to_string()));
        val.marks.insert(ValueMark::Sensitive);
        val.marks.insert(ValueMark::custom("encrypted"));
        val.marks.insert(ValueMark::typed(42u32));
        let enc = val.to_msgpack().unwrap();
        let dec = Value::from_msgpack(&enc, &Type::String).unwrap();
        assert!(dec.has_mark(&ValueMark::Sensitive));
        assert!(dec.has_mark(&ValueMark::custom("encrypted")));
        assert!(dec.has_mark(&ValueMark::custom("42")));
    }
    #[test]
    fn test_msgpack_capsule_roundtrip() {
        let cap_ty = Type::capsule::<()>("my_resource");
        let cap_val = Value::capsule("my_resource", ());
        let enc = cap_val.to_msgpack().unwrap();
        let dec = Value::from_msgpack(&enc, &cap_ty).unwrap();
        assert_eq!(dec.ty().capsule_name(), Some("my_resource"));
    }
    #[test]
    fn test_msgpack_error_handling() {
        assert!(Type::from_msgpack(&[]).is_err());
        assert!(Value::from_msgpack(&[], &Type::String).is_err());
        assert!(Type::from_msgpack(&[0x92, 0xa1]).is_err());
        assert!(Type::from_msgpack(&[0xa1, b'S', 0x00]).is_err());
        assert!(Value::from_msgpack(&[0xc3, 0x00], &Type::Bool).is_err());
        assert!(Type::from_msgpack(&[0xa1, b'X']).is_err());
        assert!(Type::from_msgpack(&[0x92, 0xa1, b'X']).is_err());
        assert!(Value::from_msgpack(&[0xc1], &Type::Dynamic).is_err());
    }
    /// Tests all integer encoding and decoding representations in `MsgPack`.
    #[test]
    fn test_msgpack_writer_reader_all_int_sizes() {
        let mut w = MsgPackWriter::new();
        let sample_signed = [
            -32,
            -1,
            0,
            1,
            127,
            200,
            1000,
            100_000,
            10_000_000_000,
            -50,
            -1000,
            -100_000,
            -10_000_000_000,
        ];
        for &n in &sample_signed {
            w.write_int(n);
        }
        let sample_unsigned = [50_u64, 200_u64, 1000_u64, 100_000_u64, 10_000_000_000_u64];
        for &u in &sample_unsigned {
            w.write_uint(u);
        }
        let bytes = w.into_bytes();
        let mut r = MsgPackReader::new(&bytes);
        for &n in &sample_signed {
            let tok = r.read_token();
            if n >= 0 {
                let u = u64::try_from(n).unwrap_or_default();
                assert_eq!(tok.ok(), Some(MsgPackToken::Uint(u)));
            } else {
                assert_eq!(tok.ok(), Some(MsgPackToken::Int(n)));
            }
        }
        for &expected_u in &sample_unsigned {
            assert_eq!(r.read_token().ok(), Some(MsgPackToken::Uint(expected_u)));
        }
        assert_eq!(r.remaining(), 0);
    }
    /// Tests string and binary encodings across all supported sizes.
    #[test]
    fn test_msgpack_writer_reader_all_string_bin_sizes() {
        let mut w = MsgPackWriter::new();
        let s_fix = "hello";
        let s_8 = "a".repeat(100);
        let s_16 = "b".repeat(500);
        let s_32 = "c".repeat(70_000);
        w.write_str(s_fix);
        w.write_str(&s_8);
        w.write_str(&s_16);
        w.write_str(&s_32);
        let bin_8 = vec![1_u8; 50];
        let bin_16 = vec![2_u8; 500];
        let bin_32 = vec![3_u8; 70_000];
        w.write_bin(&bin_8);
        w.write_bin(&bin_16);
        w.write_bin(&bin_32);
        let bytes = w.into_bytes();
        let mut r = MsgPackReader::new(&bytes);
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Str(s_fix)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Str(&s_8)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Str(&s_16)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Str(&s_32)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Bin(&bin_8)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Bin(&bin_16)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Bin(&bin_32)));
        assert_eq!(r.remaining(), 0);
    }
    /// Tests array headers, map headers, and extension tokens across all supported sizes.
    #[test]
    fn test_msgpack_writer_reader_headers_and_ext() {
        let mut w = MsgPackWriter::new();
        w.write_array_header(5);
        w.write_array_header(50);
        w.write_array_header(70_000);
        w.write_map_header(5);
        w.write_map_header(50);
        w.write_map_header(70_000);
        w.write_ext(1, &[0xaa]);
        w.write_ext(2, &[0xbb, 0xcc]);
        w.write_ext(4, &[1, 2, 3, 4]);
        w.write_ext(8, &[1, 2, 3, 4, 5, 6, 7, 8]);
        let ext16 = vec![9_u8; 16];
        w.write_ext(16, &ext16);
        let ext8_data = vec![5_u8; 20];
        w.write_ext(5, &ext8_data);
        let ext16_data = vec![6_u8; 300];
        w.write_ext(6, &ext16_data);
        let ext32_data = vec![7_u8; 70_000];
        w.write_ext(7, &ext32_data);
        let bytes = w.into_bytes();
        let mut r = MsgPackReader::new(&bytes);
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::ArrayHeader(5)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::ArrayHeader(50)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::ArrayHeader(70_000)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::MapHeader(5)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::MapHeader(50)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::MapHeader(70_000)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Ext(1, &[0xaa])));
        assert_eq!(
            r.read_token().ok(),
            Some(MsgPackToken::Ext(2, &[0xbb, 0xcc]))
        );
        assert_eq!(
            r.read_token().ok(),
            Some(MsgPackToken::Ext(4, &[1, 2, 3, 4]))
        );
        assert_eq!(
            r.read_token().ok(),
            Some(MsgPackToken::Ext(8, &[1, 2, 3, 4, 5, 6, 7, 8]))
        );
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Ext(16, &ext16)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Ext(5, &ext8_data)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Ext(6, &ext16_data)));
        assert_eq!(r.read_token().ok(), Some(MsgPackToken::Ext(7, &ext32_data)));
        assert_eq!(r.remaining(), 0);
    }
    /// Tests float encoding and decoding and reader truncation/error conditions.
    #[test]
    fn test_msgpack_writer_reader_floats_and_errors() {
        let mut w = MsgPackWriter::new();
        w.write_float(1234.5678);
        let bytes = w.into_bytes();
        let mut r = MsgPackReader::new(&bytes);
        assert_eq!(r.read_token(), Ok(MsgPackToken::Float(1234.5678)));
        let f32_bytes = [0xca, 0x40, 0x49, 0x0f, 0xdb];
        let mut r_f32 = MsgPackReader::new(&f32_bytes);
        let expected_f32 = f64::from(f32::from_be_bytes([0x40, 0x49, 0x0f, 0xdb]));
        assert_eq!(r_f32.read_token(), Ok(MsgPackToken::Float(expected_f32)));
        let mut empty_r = MsgPackReader::new(&[]);
        assert_eq!(empty_r.remaining(), 0);
        assert!(empty_r.peek_u8().is_err());
        assert!(empty_r.read_u8().is_err());
        assert!(empty_r.read_exact(1).is_err());
        let truncated_cases: &[&[u8]] = &[
            &[0xcc],
            &[0xcd, 0x01],
            &[0xce, 0x01, 0x02],
            &[0xcf, 0x01, 0x02, 0x03],
            &[0xd0],
            &[0xd1, 0x01],
            &[0xd2, 0x01, 0x02],
            &[0xd3, 0x01, 0x02, 0x03],
            &[0xca, 0x01, 0x02],
            &[0xcb, 0x01, 0x02],
            &[0xc4],
            &[0xc4, 0x05, 0x01],
            &[0xc5, 0x01],
            &[0xc5, 0x00, 0x05, 0x01],
            &[0xc6, 0x00, 0x01],
            &[0xc6, 0x00, 0x00, 0x00, 0x05, 0x01],
            &[0xc7],
            &[0xc7, 0x05],
            &[0xc7, 0x02, 0x01, 0xaa],
            &[0xc8, 0x01],
            &[0xc8, 0x00, 0x05],
            &[0xc8, 0x00, 0x05, 0x01],
            &[0xc9, 0x00, 0x01],
            &[0xc9, 0x00, 0x00, 0x00, 0x05],
            &[0xc9, 0x00, 0x00, 0x00, 0x05, 0x01],
            &[0xd4],
            &[0xd4, 0x01],
            &[0xd5],
            &[0xd5, 0x01],
            &[0xd6],
            &[0xd6, 0x01],
            &[0xd7],
            &[0xd7, 0x01],
            &[0xd8],
            &[0xd8, 0x01],
            &[0xd9],
            &[0xd9, 0x05, 0x61],
            &[0xda, 0x01],
            &[0xda, 0x00, 0x05, 0x61],
            &[0xdb, 0x00, 0x01],
            &[0xdb, 0x00, 0x00, 0x00, 0x05, 0x61],
            &[0xdc, 0x01],
            &[0xdd, 0x00, 0x01],
            &[0xde, 0x01],
            &[0xdf, 0x00, 0x01],
        ];
        for &case in truncated_cases {
            let mut r = MsgPackReader::new(case);
            assert!(r.read_token().is_err(), "case {case:?} should fail");
        }
        let invalid_utf8_str = [0xa2, 0xff, 0xff];
        assert!(MsgPackReader::new(&invalid_utf8_str).read_token().is_err());
        let invalid_utf8_str8 = [0xd9, 0x02, 0xff, 0xff];
        assert!(MsgPackReader::new(&invalid_utf8_str8).read_token().is_err());
        let invalid_utf8_str16 = [0xda, 0x00, 0x02, 0xff, 0xff];
        assert!(
            MsgPackReader::new(&invalid_utf8_str16)
                .read_token()
                .is_err()
        );
        let invalid_utf8_str32 = [0xdb, 0x00, 0x00, 0x00, 0x02, 0xff, 0xff];
        assert!(
            MsgPackReader::new(&invalid_utf8_str32)
                .read_token()
                .is_err()
        );
    }
    /// Tests error paths during compound type decoding.
    #[test]
    fn test_msgpack_type_decoding_error_paths() {
        let bad_tuple = [0x92, 0xa1, b'T', 0xc0];
        assert!(decode_type(&bad_tuple).is_err());
        let bad_object = [0x92, 0xa1, b'O', 0xc0];
        assert!(decode_type(&bad_object).is_err());
        let bad_obj_key = [0x92, 0xa1, b'O', 0x81, 0x01, 0xa1, b'S'];
        assert!(decode_type(&bad_obj_key).is_err());
        let bad_capsule = [0x92, 0xa1, b'C', 0x01];
        assert!(decode_type(&bad_capsule).is_err());
        let unknown_tag = [0x92, 0xa1, b'X'];
        assert!(decode_type(&unknown_tag).is_err());
        assert!(decode_type(&[0x92, 0xa1, b'L']).is_err());
        assert!(decode_type(&[0x92, 0xa1, b'Z']).is_err());
        assert!(decode_type(&[0x92, 0xa1, b'M']).is_err());
        assert!(decode_type(&[0x92, 0xa1, b'T']).is_err());
        assert!(decode_type(&[0x92, 0xa1, b'T', 0x91]).is_err());
        assert!(decode_type(&[0x92, 0xa1, b'O']).is_err());
        assert!(decode_type(&[0x92, 0xa1, b'O', 0x81]).is_err());
        assert!(decode_type(&[0x92, 0xa1, b'O', 0x81, 0xa1, b'a']).is_err());
        assert!(decode_type(&[0x92, 0xa1, b'C']).is_err());
        assert!(decode_type(&[0xc0]).is_err());
    }
    /// Tests value decoding edge cases, dynamic fallbacks, numbers, and errors.
    #[test]
    fn test_msgpack_value_decoding_comprehensive() {
        let dynamic_arr_bytes = [0x92, 0xa5, b'h', b'e', b'l', b'l', b'o', 0x0a];
        let val_arr = decode_value(&dynamic_arr_bytes, &Type::Dynamic).ok();
        assert!(matches!(
            val_arr.as_ref().map(Value::ty),
            Some(Type::Tuple(_))
        ));
        let dynamic_map_bytes = [0x81, 0xa3, b'f', b'o', b'o', 0x05];
        let val_map = decode_value(&dynamic_map_bytes, &Type::Dynamic).ok();
        assert!(matches!(
            val_map.as_ref().map(Value::ty),
            Some(Type::Object { .. })
        ));
        let non_str_key_map = [0x81, 0x01, 0x02];
        assert!(decode_value(&non_str_key_map, &Type::Map(Box::new(Type::Number))).is_err());
        assert!(
            decode_value(
                &non_str_key_map,
                &Type::object(BTreeMap::from([("k".to_string(), Type::Number)]))
            )
            .is_err()
        );
        assert!(decode_value(&non_str_key_map, &Type::Dynamic).is_err());
        let float_token_bytes = [0xcb, 0x40, 0x09, 0x21, 0xfb, 0x54, 0x44, 0x2d, 0x18];
        let val_float = decode_value(&float_token_bytes, &Type::Number).ok();
        assert_eq!(val_float.as_ref().map(Value::ty), Some(&Type::Number));
        let bin_str_bytes = [0xc4, 0x04, b't', b'e', b'x', b't'];
        let val_bin_str = decode_value(&bin_str_bytes, &Type::String).ok();
        assert_eq!(
            val_bin_str.as_ref().map(|v| v.data.as_ref()),
            Some(&ValueData::String("text".to_string()))
        );
        let bin_invalid_utf8 = [0xc4, 0x01, 0xff];
        assert!(decode_value(&bin_invalid_utf8, &Type::String).is_err());
        let valid_num_str = [0xa3, b'1', b'.', b'5'];
        let val_num = decode_value(&valid_num_str, &Type::Number).ok();
        assert_eq!(val_num.as_ref().map(Value::ty), Some(&Type::Number));
        let invalid_num_str = [0xa3, b'b', b'a', b'd'];
        assert!(decode_value(&invalid_num_str, &Type::Number).is_err());
        let unexpected_ext = [0xd4, 99, 0x00];
        assert!(decode_value(&unexpected_ext, &Type::Dynamic).is_err());
        let u64_max_num = Value::new(Type::Number, ValueData::Number(Number::from(u64::MAX)));
        let enc_u64 = encode_value(&u64_max_num).unwrap_or_default();
        assert_eq!(
            decode_value(&enc_u64, &Type::Number).ok(),
            Some(u64_max_num)
        );
        let huge_n =
            Number(bigdecimal::BigDecimal::from(u64::MAX) + bigdecimal::BigDecimal::from(100));
        let huge_num = Value::new(Type::Number, ValueData::Number(huge_n));
        let enc_huge = encode_value(&huge_num).unwrap_or_default();
        assert_eq!(decode_value(&enc_huge, &Type::Number).ok(), Some(huge_num));
        let dec_n =
            Number(bigdecimal::BigDecimal::from(314_159) / bigdecimal::BigDecimal::from(100_000));
        let dec_num = Value::new(Type::Number, ValueData::Number(dec_n));
        let enc_dec = encode_value(&dec_num).unwrap_or_default();
        assert_eq!(decode_value(&enc_dec, &Type::Number).ok(), Some(dec_num));
        let bad_ref_hdr = [0xd4, 0x00, 0xc0];
        assert!(decode_value(&bad_ref_hdr, &Type::String).is_err());
        let short_ref_array = [0xd4, 0x00, 0x95];
        assert!(decode_value(&short_ref_array, &Type::String).is_err());
        let mut map_data = BTreeMap::new();
        map_data.insert(
            "key".to_string(),
            Value::new(Type::String, ValueData::String("val".to_string())),
        );
        let map_val = Value::new(
            Type::Map(Box::new(Type::String)),
            ValueData::Object(map_data),
        );
        let map_enc = encode_value(&map_val).unwrap_or_default();
        assert_eq!(decode_value(&map_enc, map_val.ty()).ok(), Some(map_val));
        let empty_ref = Refinement {
            not_null: false,
            string_length_min: None,
            string_length_max: None,
            string_prefix: None,
            string_suffix: None,
            collection_length_min: None,
            collection_length_max: None,
            number_min: None,
            number_max: None,
            object_attrs: BTreeMap::new(),
        };
        let empty_ref_val = Value::unknown_refined(Type::String, empty_ref);
        let enc_empty_ref = encode_value(&empty_ref_val).unwrap_or_default();
        let dec_empty_ref =
            decode_value(&enc_empty_ref, &Type::String).unwrap_or(Value::null(Type::Dynamic));
        assert!(dec_empty_ref.is_unknown());
        let default_ref = Refinement::new();
        let r = dec_empty_ref.refinement().unwrap_or(&default_ref);
        assert!(!r.not_null);
        assert_eq!(r.string_length_min, None);
        assert_eq!(r.string_prefix, None);
        let raw_signed_ref = [
            0x9a, 0xc0, 0xd0, 5, 0xd0, 20, 0xc0, 0xc0, 0xd0, 1, 0xd0, 10, 0xd0, 100, 0xd0, 120,
            0xc0,
        ];
        let mut enc_signed_ref = MsgPackWriter::new();
        enc_signed_ref.write_ext(0, &raw_signed_ref);
        let val_signed_ref = decode_value(&enc_signed_ref.into_bytes(), &Type::String)
            .unwrap_or(Value::null(Type::Dynamic));
        let res_signed_ref = val_signed_ref.refinement().unwrap_or(&default_ref);
        assert_eq!(res_signed_ref.string_length_min, Some(5));
        assert_eq!(res_signed_ref.string_length_max, Some(20));
        assert_eq!(res_signed_ref.collection_length_min, Some(1));
        assert_eq!(res_signed_ref.collection_length_max, Some(10));
        assert_eq!(res_signed_ref.number_min, Some(Number::from(100)));
        assert_eq!(res_signed_ref.number_max, Some(Number::from(120)));
        let raw_unsigned_ref = [
            0x9a, 0xc3, 0xcc, 6, 0xcc, 22, 0xc0, 0xc0, 0xcc, 2, 0xcc, 12, 0xcc, 50, 0xcc, 90, 0xc0,
        ];
        let mut enc_unsigned_ref = MsgPackWriter::new();
        enc_unsigned_ref.write_ext(0, &raw_unsigned_ref);
        let val_unsigned_ref = decode_value(&enc_unsigned_ref.into_bytes(), &Type::String)
            .unwrap_or(Value::null(Type::Dynamic));
        let res_unsigned_ref = val_unsigned_ref.refinement().unwrap_or(&default_ref);
        assert!(res_unsigned_ref.not_null);
        assert_eq!(res_unsigned_ref.string_length_min, Some(6));
        assert_eq!(res_unsigned_ref.string_length_max, Some(22));
        assert_eq!(res_unsigned_ref.collection_length_min, Some(2));
        assert_eq!(res_unsigned_ref.collection_length_max, Some(12));
        assert_eq!(res_unsigned_ref.number_min, Some(Number::from(50)));
        assert_eq!(res_unsigned_ref.number_max, Some(Number::from(90)));
        let mut ref_w_bad_key = MsgPackWriter::new();
        ref_w_bad_key.write_array_header(10);
        ref_w_bad_key.write_bool(true);
        ref_w_bad_key.write_nil();
        ref_w_bad_key.write_nil();
        ref_w_bad_key.write_nil();
        ref_w_bad_key.write_nil();
        ref_w_bad_key.write_nil();
        ref_w_bad_key.write_nil();
        ref_w_bad_key.write_nil();
        ref_w_bad_key.write_nil();
        ref_w_bad_key.write_map_header(1);
        ref_w_bad_key.write_int(999);
        let mut ext_w_bad = MsgPackWriter::new();
        ext_w_bad.write_ext(0, &ref_w_bad_key.into_bytes());
        let dec_bad_attr_val = decode_value(&ext_w_bad.into_bytes(), &Type::String)
            .unwrap_or(Value::null(Type::Dynamic));
        assert!(dec_bad_attr_val.is_unknown());
        let tuple_3 = Value::new(
            Type::Tuple(vec![Type::String, Type::String, Type::String]),
            ValueData::Array(vec![
                Value::new(Type::String, ValueData::String("x".into())),
                Value::new(Type::String, ValueData::String("y".into())),
                Value::new(Type::String, ValueData::String("z".into())),
            ]),
        );
        let enc_tup3 = encode_value(&tuple_3).unwrap_or_default();
        assert_eq!(decode_value(&enc_tup3, tuple_3.ty()).ok(), Some(tuple_3));
        let mut marked_bad_hdr = MsgPackWriter::new();
        marked_bad_hdr.write_array_header(3);
        marked_bad_hdr.write_uint(1);
        marked_bad_hdr.write_str("content");
        marked_bad_hdr.write_nil();
        let dec_bad_hdr = decode_value(&marked_bad_hdr.into_bytes(), &Type::String)
            .unwrap_or(Value::null(Type::Dynamic));
        assert!(dec_bad_hdr.marks.is_empty());
        let mut marked_bad_mark = MsgPackWriter::new();
        marked_bad_mark.write_array_header(3);
        marked_bad_mark.write_uint(1);
        marked_bad_mark.write_str("content");
        marked_bad_mark.write_array_header(1);
        marked_bad_mark.write_uint(42);
        let dec_bad_mark = decode_value(&marked_bad_mark.into_bytes(), &Type::String)
            .unwrap_or(Value::null(Type::Dynamic));
        assert!(dec_bad_mark.marks.is_empty());
        let bad_ref_payloads: &[&[u8]] = &[
            &[0xc1],
            &[0x9a],
            &[0x9a, 0xc3],
            &[0x9a, 0xc3, 0x01],
            &[0x9a, 0xc3, 0x01, 0x02],
            &[0x9a, 0xc3, 0x01, 0x02, 0xc0],
            &[0x9a, 0xc3, 0x01, 0x02, 0xc0, 0xc0],
            &[0x9a, 0xc3, 0x01, 0x02, 0xc0, 0xc0, 0x01],
            &[0x9a, 0xc3, 0x01, 0x02, 0xc0, 0xc0, 0x01, 0x02],
            &[0x9a, 0xc3, 0x01, 0x02, 0xc0, 0xc0, 0x01, 0x02, 0x01],
            &[0x9a, 0xc3, 0x01, 0x02, 0xc0, 0xc0, 0x01, 0x02, 0x01, 0x02],
            &[
                0x9a, 0xc3, 0x01, 0x02, 0xc0, 0xc0, 0x01, 0x02, 0x01, 0x02, 0x81,
            ],
            &[
                0x9a, 0xc3, 0x01, 0x02, 0xc0, 0xc0, 0x01, 0x02, 0x01, 0x02, 0x81, 0xa1, b'k', 0xc0,
            ],
        ];
        for bad_payload in bad_ref_payloads {
            let mut w = MsgPackWriter::new();
            w.write_ext(0, bad_payload);
            let bytes = w.into_bytes();
            let res = decode_value(&bytes, &Type::String);
            assert!(
                res.is_err(),
                "payload {bad_payload:?} unexpectedly succeeded: {res:?}"
            );
        }
        assert!(decode_value(&[0x93, 0x01], &Type::String).is_err());
        assert!(
            decode_value(
                &[0x93, 0x01, 0xa5, b'h', b'e', b'l', b'l', b'o'],
                &Type::String
            )
            .is_err()
        );
        assert!(decode_value(&[0x91], &Type::List(Box::new(Type::String))).is_err());
        assert!(decode_value(&[0x91], &Type::Set(Box::new(Type::String))).is_err());
        assert!(decode_value(&[0x91], &Type::Tuple(vec![Type::String])).is_err());
        assert!(decode_value(&[0x91], &Type::Dynamic).is_err());
        assert!(decode_value(&[0x81], &Type::Map(Box::new(Type::String))).is_err());
        assert!(decode_value(&[0x81, 0xa1, b'k'], &Type::Map(Box::new(Type::String))).is_err());
        assert!(
            decode_value(
                &[0x81],
                &Type::object(BTreeMap::from([("k".into(), Type::String)]))
            )
            .is_err()
        );
        assert!(
            decode_value(
                &[0x81, 0xa1, b'k'],
                &Type::object(BTreeMap::from([("k".into(), Type::String)]))
            )
            .is_err()
        );
        assert!(decode_value(&[0x81], &Type::Dynamic).is_err());
        assert!(decode_value(&[0x81, 0xa1, b'k'], &Type::Dynamic).is_err());
    }
}
