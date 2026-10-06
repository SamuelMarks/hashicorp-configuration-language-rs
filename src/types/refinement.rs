//! Type refinements on unknown values (`cty.Refinement`).
//!
//! Refinements allow tracking partial constraints known about an unknown value,
//! such as nullability, string length bounds, string prefix/suffix, collection length bounds,
//! number value bounds, and object attribute constraints.
use crate::number::Number;
use crate::types::val::{Value, ValueData};
use std::collections::BTreeMap;
/// Constraints and refinements known about an unknown value.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Refinement {
    /// Whether the value is guaranteed not to be null.
    pub not_null: bool,
    /// Minimum string length in characters, if known.
    pub string_length_min: Option<usize>,
    /// Maximum string length in characters, if known.
    pub string_length_max: Option<usize>,
    /// Known prefix of the string, if any.
    pub string_prefix: Option<String>,
    /// Known suffix of the string, if any.
    pub string_suffix: Option<String>,
    /// Minimum number of elements in a collection, if known.
    pub collection_length_min: Option<usize>,
    /// Maximum number of elements in a collection, if known.
    pub collection_length_max: Option<usize>,
    /// Minimum number value, if known.
    pub number_min: Option<Number>,
    /// Maximum number value, if known.
    pub number_max: Option<Number>,
    /// Known refinements for object attributes.
    pub object_attrs: BTreeMap<String, Refinement>,
}
impl std::hash::Hash for Refinement {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.not_null.hash(state);
        self.string_length_min.hash(state);
        self.string_length_max.hash(state);
        self.string_prefix.hash(state);
        self.string_suffix.hash(state);
        self.collection_length_min.hash(state);
        self.collection_length_max.hash(state);
        if let Some(ref min) = self.number_min {
            state.write_u8(1);
            min.to_string().hash(state);
        } else {
            state.write_u8(0);
        }
        if let Some(ref max) = self.number_max {
            state.write_u8(1);
            max.to_string().hash(state);
        } else {
            state.write_u8(0);
        }
        self.object_attrs.hash(state);
    }
}
impl Refinement {
    /// Creates a new empty [`Refinement`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Creates a new [`Refinement`] requiring the value to be non-null.
    #[must_use]
    pub fn not_null() -> Self {
        Self {
            not_null: true,
            ..Default::default()
        }
    }
    /// Sets whether the value is guaranteed not to be null.
    ///
    /// # Arguments
    /// * `not_null` - `true` if the value is non-null.
    #[must_use]
    pub fn with_not_null(mut self, not_null: bool) -> Self {
        self.not_null = not_null;
        self
    }
    /// Sets the minimum and maximum string length constraints.
    ///
    /// # Arguments
    /// * `min` - The optional minimum length in characters.
    /// * `max` - The optional maximum length in characters.
    ///
    /// # Errors
    /// Returns an error message if `min` exceeds `max`.
    pub fn with_string_length(
        mut self,
        min: impl Into<Option<usize>>,
        max: impl Into<Option<usize>>,
    ) -> Result<Self, String> {
        let min = min.into();
        let max = max.into();
        if let Some((mi, ma)) = min.zip(max).filter(|(mi, ma)| mi > ma) {
            return Err(format!(
                "minimum string length ({mi}) cannot exceed maximum ({ma})"
            ));
        }
        self.string_length_min = min;
        self.string_length_max = max;
        Ok(self)
    }
    /// Sets the minimum and maximum collection length constraints.
    ///
    /// # Arguments
    /// * `min` - The optional minimum collection length.
    /// * `max` - The optional maximum collection length.
    ///
    /// # Errors
    /// Returns an error message if `min` exceeds `max`.
    pub fn with_collection_length(
        mut self,
        min: impl Into<Option<usize>>,
        max: impl Into<Option<usize>>,
    ) -> Result<Self, String> {
        let min = min.into();
        let max = max.into();
        if let Some((mi, ma)) = min.zip(max).filter(|(mi, ma)| mi > ma) {
            return Err(format!(
                "minimum collection length ({mi}) cannot exceed maximum ({ma})"
            ));
        }
        self.collection_length_min = min;
        self.collection_length_max = max;
        Ok(self)
    }
    /// Sets the minimum and maximum numeric range constraints.
    ///
    /// # Arguments
    /// * `min` - The optional minimum number value.
    /// * `max` - The optional maximum number value.
    ///
    /// # Errors
    /// Returns an error message if `min` exceeds `max`.
    pub fn with_number_range(
        mut self,
        min: impl Into<Option<Number>>,
        max: impl Into<Option<Number>>,
    ) -> Result<Self, String> {
        let min = min.into();
        let max = max.into();
        if let Some((mi, ma)) = min.as_ref().zip(max.as_ref()).filter(|(mi, ma)| mi > ma) {
            return Err(format!(
                "minimum number ({mi:?}) cannot exceed maximum ({ma:?})"
            ));
        }
        self.number_min = min;
        self.number_max = max;
        Ok(self)
    }
    /// Sets a known prefix constraint for string values.
    ///
    /// # Arguments
    /// * `prefix` - The expected prefix string.
    #[must_use]
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        let p = prefix.into();
        let char_count = p.chars().count();
        if let Some(min) = self.string_length_min {
            self.string_length_min = Some(min.max(char_count));
        } else {
            self.string_length_min = Some(char_count);
        }
        self.string_prefix = Some(p);
        self
    }
    /// Sets a known suffix constraint for string values.
    ///
    /// # Arguments
    /// * `suffix` - The expected suffix string.
    #[must_use]
    pub fn with_suffix(mut self, suffix: impl Into<String>) -> Self {
        let s = suffix.into();
        let char_count = s.chars().count();
        if let Some(min) = self.string_length_min {
            self.string_length_min = Some(min.max(char_count));
        } else {
            self.string_length_min = Some(char_count);
        }
        self.string_suffix = Some(s);
        self
    }
    /// Sets a refinement constraint for a named object attribute.
    ///
    /// # Arguments
    /// * `attr` - The attribute name.
    /// * `refinement` - The refinement constraints for the attribute.
    #[must_use]
    pub fn with_object_attr(mut self, attr: impl Into<String>, refinement: Self) -> Self {
        self.object_attrs.insert(attr.into(), refinement);
        self
    }
    /// Returns `true` if this refinement has no constraints set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.not_null
            && self.string_length_min.is_none()
            && self.string_length_max.is_none()
            && self.string_prefix.is_none()
            && self.string_suffix.is_none()
            && self.collection_length_min.is_none()
            && self.collection_length_max.is_none()
            && self.number_min.is_none()
            && self.number_max.is_none()
            && self.object_attrs.is_empty()
    }
    /// Intersects this refinement with another, combining their constraints.
    ///
    /// # Arguments
    /// * `other` - The other refinement to combine with.
    ///
    /// # Errors
    /// Returns an error message if the combined constraints are contradictory.
    pub fn intersect(&self, other: &Self) -> Result<Self, String> {
        let not_null = self.not_null || other.not_null;
        let mut string_length_min = match (self.string_length_min, other.string_length_min) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        let string_length_max = match (self.string_length_max, other.string_length_max) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        if let Some((min, max)) = string_length_min
            .zip(string_length_max)
            .filter(|(mi, ma)| mi > ma)
        {
            return Err(format!(
                "conflicting string length constraints: min ({min}) > max ({max})"
            ));
        }
        let string_prefix = match (&self.string_prefix, &other.string_prefix) {
            (Some(p1), Some(p2)) => {
                if p1.starts_with(p2) {
                    Some(p1.clone())
                } else if p2.starts_with(p1) {
                    Some(p2.clone())
                } else {
                    return Err(format!("conflicting string prefixes: {p1:?} and {p2:?}"));
                }
            }
            (Some(p), None) | (None, Some(p)) => Some(p.clone()),
            (None, None) => None,
        };
        if let (Some(p), Some(max)) = (&string_prefix, string_length_max) {
            let count = p.chars().count();
            if count > max {
                return Err(format!(
                    "prefix length ({count}) exceeds maximum string length ({max})"
                ));
            }
        }
        if let Some(p) = &string_prefix {
            let count = p.chars().count();
            string_length_min = Some(string_length_min.map_or(count, |m| m.max(count)));
        }
        let string_suffix = match (&self.string_suffix, &other.string_suffix) {
            (Some(s1), Some(s2)) => {
                if s1.ends_with(s2) {
                    Some(s1.clone())
                } else if s2.ends_with(s1) {
                    Some(s2.clone())
                } else {
                    return Err(format!("conflicting string suffixes: {s1:?} and {s2:?}"));
                }
            }
            (Some(s), None) | (None, Some(s)) => Some(s.clone()),
            (None, None) => None,
        };
        if let (Some(s), Some(max)) = (&string_suffix, string_length_max) {
            let count = s.chars().count();
            if count > max {
                return Err(format!(
                    "suffix length ({count}) exceeds maximum string length ({max})"
                ));
            }
        }
        if let Some(s) = &string_suffix {
            let count = s.chars().count();
            string_length_min = Some(string_length_min.map_or(count, |m| m.max(count)));
        }
        let collection_length_min = match (self.collection_length_min, other.collection_length_min)
        {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        let collection_length_max = match (self.collection_length_max, other.collection_length_max)
        {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        if let Some((min, max)) = collection_length_min
            .zip(collection_length_max)
            .filter(|(mi, ma)| mi > ma)
        {
            return Err(format!(
                "conflicting collection length constraints: min ({min}) > max ({max})"
            ));
        }
        let number_min = match (&self.number_min, &other.number_min) {
            (Some(a), Some(b)) => Some(if a >= b { a.clone() } else { b.clone() }),
            (Some(a), None) | (None, Some(a)) => Some(a.clone()),
            (None, None) => None,
        };
        let number_max = match (&self.number_max, &other.number_max) {
            (Some(a), Some(b)) => Some(if a <= b { a.clone() } else { b.clone() }),
            (Some(a), None) | (None, Some(a)) => Some(a.clone()),
            (None, None) => None,
        };
        if let Some((min, max)) = number_min
            .as_ref()
            .zip(number_max.as_ref())
            .filter(|(mi, ma)| mi > ma)
        {
            return Err(format!(
                "conflicting number bounds: min ({min:?}) > max ({max:?})"
            ));
        }
        let mut object_attrs = BTreeMap::new();
        for (k, v) in &self.object_attrs {
            if let Some(other_v) = other.object_attrs.get(k) {
                let intersected = v.intersect(other_v)?;
                object_attrs.insert(k.clone(), intersected);
            } else {
                object_attrs.insert(k.clone(), v.clone());
            }
        }
        for (k, v) in &other.object_attrs {
            if !self.object_attrs.contains_key(k) {
                object_attrs.insert(k.clone(), v.clone());
            }
        }
        Ok(Self {
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
    /// Verifies if a concrete [`Value`] satisfies all constraints of this refinement.
    ///
    /// # Arguments
    /// * `val` - The value to check against constraints.
    ///
    /// # Errors
    /// Returns an error message describing the violation if the value fails to satisfy any constraint.
    pub fn satisfies(&self, val: &Value) -> Result<(), String> {
        if self.not_null && val.is_null() {
            return Err("expected non-null value, got null".to_string());
        }
        match &*val.data {
            ValueData::String(s) => {
                let char_count = s.chars().count();
                if let Some(min) = self.string_length_min.filter(|&m| char_count < m) {
                    return Err(format!(
                        "string length {char_count} is less than minimum {min}"
                    ));
                }
                if let Some(max) = self.string_length_max.filter(|&m| char_count > m) {
                    return Err(format!("string length {char_count} exceeds maximum {max}"));
                }
                if let Some(prefix) = self.string_prefix.as_ref().filter(|p| !s.starts_with(*p)) {
                    return Err(format!(
                        "string does not start with expected prefix {prefix:?}"
                    ));
                }
                if let Some(suffix) = self.string_suffix.as_ref().filter(|suf| !s.ends_with(*suf)) {
                    return Err(format!(
                        "string does not end with expected suffix {suffix:?}"
                    ));
                }
            }
            ValueData::Array(arr) => {
                let len = arr.len();
                if let Some(min) = self.collection_length_min.filter(|&m| len < m) {
                    return Err(format!(
                        "collection length {len} is less than minimum {min}"
                    ));
                }
                if let Some(max) = self.collection_length_max.filter(|&m| len > m) {
                    return Err(format!("collection length {len} exceeds maximum {max}"));
                }
            }
            ValueData::Set(set) => {
                let len = set.len();
                if let Some(min) = self.collection_length_min.filter(|&m| len < m) {
                    return Err(format!(
                        "collection length {len} is less than minimum {min}"
                    ));
                }
                if let Some(max) = self.collection_length_max.filter(|&m| len > m) {
                    return Err(format!("collection length {len} exceeds maximum {max}"));
                }
            }
            ValueData::Object(obj) => {
                let len = obj.len();
                if let Some(min) = self.collection_length_min.filter(|&m| len < m) {
                    return Err(format!(
                        "object attribute count {len} is less than minimum {min}"
                    ));
                }
                if let Some(max) = self.collection_length_max.filter(|&m| len > m) {
                    return Err(format!(
                        "object attribute count {len} exceeds maximum {max}"
                    ));
                }
                for (k, ref_attr) in &self.object_attrs {
                    if let Some(val_attr) = obj.get(k) {
                        ref_attr.satisfies(val_attr)?;
                    } else if ref_attr.not_null {
                        return Err(format!("missing required object attribute {k:?}"));
                    }
                }
            }
            ValueData::Number(n) => {
                if let Some(min) = self.number_min.as_ref().filter(|m| n < *m) {
                    return Err(format!("number {n:?} is less than minimum {min:?}"));
                }
                if let Some(max) = self.number_max.as_ref().filter(|m| n > *m) {
                    return Err(format!("number {n:?} exceeds maximum {max:?}"));
                }
            }
            _ => {}
        }
        Ok(())
    }
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
    use crate::types::Type;
    use std::collections::{BTreeMap, BTreeSet};
    #[test]
    fn test_refinement_builders_and_empty() {
        let empty = Refinement::new();
        assert!(empty.is_empty());
        let non_null = Refinement::not_null();
        assert!(!non_null.is_empty());
        assert!(non_null.not_null);
        let toggle = non_null.with_not_null(false);
        assert!(toggle.is_empty());
        let str_len = Refinement::new().with_string_length(2, 10).unwrap();
        assert_eq!(str_len.string_length_min, Some(2));
        assert_eq!(str_len.string_length_max, Some(10));
        let str_err = Refinement::new().with_string_length(10, 2);
        assert!(str_err.is_err());
        let col_len = Refinement::new().with_collection_length(1, 5).unwrap();
        assert_eq!(col_len.collection_length_min, Some(1));
        assert_eq!(col_len.collection_length_max, Some(5));
        let col_err = Refinement::new().with_collection_length(5, 1);
        assert!(col_err.is_err());
        let num_range = Refinement::new()
            .with_number_range(Number::from(1), Number::from(10))
            .unwrap();
        assert_eq!(num_range.number_min, Some(Number::from(1)));
        assert_eq!(num_range.number_max, Some(Number::from(10)));
        let num_err = Refinement::new().with_number_range(Number::from(10), Number::from(1));
        assert!(num_err.is_err());
        let with_p = Refinement::new().with_prefix("pre-");
        assert_eq!(with_p.string_prefix.as_deref(), Some("pre-"));
        assert_eq!(with_p.string_length_min, Some(4));
        let with_s = Refinement::new().with_suffix("-suf");
        assert_eq!(with_s.string_suffix.as_deref(), Some("-suf"));
        assert_eq!(with_s.string_length_min, Some(4));
        let with_obj = Refinement::new().with_object_attr("nested", Refinement::not_null());
        assert!(with_obj.object_attrs.contains_key("nested"));
    }
    #[test]
    fn test_refinement_intersection_success() {
        let r1 = Refinement::not_null()
            .with_string_length(2, 10)
            .unwrap()
            .with_prefix("ab")
            .with_suffix("yz")
            .with_collection_length(1, 10)
            .unwrap()
            .with_number_range(Number::from(2), Number::from(10))
            .unwrap();
        let r2 = Refinement::new()
            .with_string_length(4, 8)
            .unwrap()
            .with_prefix("abcd")
            .with_suffix("xyz")
            .with_collection_length(3, 8)
            .unwrap()
            .with_number_range(Number::from(4), Number::from(8))
            .unwrap();
        let inter = r1.intersect(&r2).unwrap();
        assert!(inter.not_null);
        assert_eq!(inter.string_length_min, Some(4));
        assert_eq!(inter.string_length_max, Some(8));
        assert_eq!(inter.string_prefix.as_deref(), Some("abcd"));
        assert_eq!(inter.string_suffix.as_deref(), Some("xyz"));
        assert_eq!(inter.collection_length_min, Some(3));
        assert_eq!(inter.collection_length_max, Some(8));
        assert_eq!(inter.number_min, Some(Number::from(4)));
        assert_eq!(inter.number_max, Some(Number::from(8)));
        let r_a = Refinement::new().with_prefix("xyz");
        let r_b = Refinement::new().with_prefix("xy");
        let inter_ab = r_a.intersect(&r_b).unwrap();
        assert_eq!(inter_ab.string_prefix.as_deref(), Some("xyz"));
        let suf_a = Refinement::new().with_suffix("123");
        let suf_b = Refinement::new().with_suffix("23");
        let inter_suf = suf_a.intersect(&suf_b).unwrap();
        assert_eq!(inter_suf.string_suffix.as_deref(), Some("123"));
        let r_o1 = Refinement::new()
            .with_object_attr("attr1", Refinement::new().with_string_length(1, 5).unwrap());
        let r_o2 = Refinement::new()
            .with_object_attr(
                "attr1",
                Refinement::new().with_string_length(3, 10).unwrap(),
            )
            .with_object_attr("attr2", Refinement::not_null());
        let inter_o = r_o1.intersect(&r_o2).unwrap();
        assert_eq!(
            inter_o
                .object_attrs
                .get("attr1")
                .and_then(|r| r.string_length_min),
            Some(3)
        );
        assert_eq!(
            inter_o
                .object_attrs
                .get("attr1")
                .and_then(|r| r.string_length_max),
            Some(5)
        );
        assert!(inter_o.object_attrs.contains_key("attr2"));
    }
    #[test]
    fn test_refinement_intersection_contradictions() {
        let r1 = Refinement::new().with_string_length(5, None).unwrap();
        let r2 = Refinement::new().with_string_length(None, 3).unwrap();
        assert!(r1.intersect(&r2).is_err());
        let r3 = Refinement::new().with_prefix("foo");
        let r4 = Refinement::new().with_prefix("bar");
        assert!(r3.intersect(&r4).is_err());
        let r5 = Refinement::new().with_prefix("long_prefix");
        let r6 = Refinement::new().with_string_length(None, 4).unwrap();
        assert!(r5.intersect(&r6).is_err());
        let r7 = Refinement::new().with_suffix("foo");
        let r8 = Refinement::new().with_suffix("bar");
        assert!(r7.intersect(&r8).is_err());
        let r9 = Refinement::new().with_suffix("long_suffix");
        let r10 = Refinement::new().with_string_length(None, 4).unwrap();
        assert!(r9.intersect(&r10).is_err());
        let r11 = Refinement::new().with_collection_length(5, None).unwrap();
        let r12 = Refinement::new().with_collection_length(None, 3).unwrap();
        assert!(r11.intersect(&r12).is_err());
        let r13 = Refinement::new()
            .with_number_range(Number::from(10), None)
            .unwrap();
        let r14 = Refinement::new()
            .with_number_range(None, Number::from(5))
            .unwrap();
        assert!(r13.intersect(&r14).is_err());
        let r15 = Refinement::new().with_object_attr("k", r3);
        let r16 = Refinement::new().with_object_attr("k", r4);
        assert!(r15.intersect(&r16).is_err());
    }
    #[test]
    fn test_refinement_satisfies() {
        let str_ref = Refinement::not_null()
            .with_string_length(3, 10)
            .unwrap()
            .with_prefix("go-")
            .with_suffix(".rs");
        let ok_str = Value::new(Type::String, ValueData::String("go-test.rs".to_string()));
        assert!(str_ref.satisfies(&ok_str).is_ok());
        let null_val = Value::null(Type::String);
        assert!(str_ref.satisfies(&null_val).is_err());
        let short_str = Value::new(Type::String, ValueData::String("g".to_string()));
        assert!(str_ref.satisfies(&short_str).is_err());
        let long_str = Value::new(
            Type::String,
            ValueData::String("go-a_very_long_string.rs".to_string()),
        );
        assert!(str_ref.satisfies(&long_str).is_err());
        let bad_pre = Value::new(Type::String, ValueData::String("rs-test.rs".to_string()));
        assert!(str_ref.satisfies(&bad_pre).is_err());
        let bad_suf = Value::new(Type::String, ValueData::String("go-test.go".to_string()));
        assert!(str_ref.satisfies(&bad_suf).is_err());
        let col_ref = Refinement::new().with_collection_length(2, 4).unwrap();
        let ok_arr = Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(vec![
                Value::new(Type::Number, ValueData::Number(Number::from(1))),
                Value::new(Type::Number, ValueData::Number(Number::from(2))),
            ]),
        );
        assert!(col_ref.satisfies(&ok_arr).is_ok());
        let short_arr = Value::new(Type::List(Box::new(Type::Number)), ValueData::Array(vec![]));
        assert!(col_ref.satisfies(&short_arr).is_err());
        let mut long_vec = Vec::new();
        for i in 0..5 {
            long_vec.push(Value::new(Type::Number, ValueData::Number(Number::from(i))));
        }
        let long_arr = Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(long_vec),
        );
        assert!(col_ref.satisfies(&long_arr).is_err());
        let mut set = BTreeSet::new();
        set.insert(Value::new(Type::Number, ValueData::Number(Number::from(1))));
        set.insert(Value::new(Type::Number, ValueData::Number(Number::from(2))));
        let ok_set = Value::new(Type::Set(Box::new(Type::Number)), ValueData::Set(set));
        assert!(col_ref.satisfies(&ok_set).is_ok());
        let empty_set = Value::new(
            Type::Set(Box::new(Type::Number)),
            ValueData::Set(BTreeSet::new()),
        );
        assert!(col_ref.satisfies(&empty_set).is_err());
        let mut obj_attrs = BTreeMap::new();
        obj_attrs.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("sample".to_string())),
        );
        let obj_val = Value::new(
            Type::object(BTreeMap::from([("name".to_string(), Type::String)])),
            ValueData::Object(obj_attrs),
        );
        let obj_ref = Refinement::new()
            .with_collection_length(1, 2)
            .unwrap()
            .with_object_attr("name", Refinement::not_null().with_prefix("sam"));
        assert!(obj_ref.satisfies(&obj_val).is_ok());
        let missing_attr_ref =
            Refinement::new().with_object_attr("missing", Refinement::not_null());
        assert!(missing_attr_ref.satisfies(&obj_val).is_err());
        let num_ref = Refinement::new()
            .with_number_range(Number::from(5), Number::from(10))
            .unwrap();
        let ok_num = Value::new(Type::Number, ValueData::Number(Number::from(7)));
        assert!(num_ref.satisfies(&ok_num).is_ok());
        let low_num = Value::new(Type::Number, ValueData::Number(Number::from(2)));
        assert!(num_ref.satisfies(&low_num).is_err());
        let high_num = Value::new(Type::Number, ValueData::Number(Number::from(12)));
        assert!(num_ref.satisfies(&high_num).is_err());
        let bool_val = Value::new(Type::Bool, ValueData::Bool(true));
        assert!(Refinement::new().satisfies(&bool_val).is_ok());
    }
    #[test]
    fn test_refinement_traits_and_hashing() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let r1 = Refinement::not_null()
            .with_prefix("pre")
            .with_suffix("suf")
            .with_number_range(Number::from(1), Number::from(10))
            .unwrap();
        let r2 = r1.clone();
        assert_eq!(r1, r2);
        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();
        r1.hash(&mut h1);
        r2.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
        let debug_s = format!("{r1:?}");
        assert!(debug_s.contains("Refinement"));
    }
    #[test]
    fn test_value_refinement_integration() {
        let r = Refinement::not_null().with_prefix("data-");
        let val = Value::unknown_refined(Type::String, r.clone());
        assert!(val.is_unknown());
        assert_eq!(val.refinement(), Some(&r));
        assert!(val.refinement_arc().is_some());
        let mut cloned = Value::unknown(Type::String);
        cloned.clone_from(&val);
        assert_eq!(cloned.refinement(), Some(&r));
        let unrefined = Value::unknown(Type::String);
        assert_eq!(unrefined.refinement(), None);
        assert!(unrefined.refinement_arc().is_none());
    }
    #[test]
    fn test_refinement_complete_coverage() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hash;
        let r_empty = Refinement::new();
        let mut h = DefaultHasher::new();
        r_empty.hash(&mut h);
        let r_none = Refinement::new();
        let r_str_min = Refinement::new()
            .with_string_length(Some(2), None)
            .unwrap_or_default();
        assert!(r_str_min.intersect(&r_none).is_ok());
        assert!(r_none.intersect(&r_str_min).is_ok());
        let r_str_max = Refinement::new()
            .with_string_length(None, Some(10))
            .unwrap_or_default();
        assert!(r_str_max.intersect(&r_none).is_ok());
        assert!(r_none.intersect(&r_str_max).is_ok());
        let r_pre = Refinement::new().with_prefix("pre");
        assert!(r_pre.intersect(&r_none).is_ok());
        assert!(r_none.intersect(&r_pre).is_ok());
        let r_suf = Refinement::new().with_suffix("suf");
        assert!(r_suf.intersect(&r_none).is_ok());
        assert!(r_none.intersect(&r_suf).is_ok());
        let r_col_min = Refinement::new()
            .with_collection_length(Some(2), None)
            .unwrap_or_default();
        assert!(r_col_min.intersect(&r_none).is_ok());
        assert!(r_none.intersect(&r_col_min).is_ok());
        let r_col_max = Refinement::new()
            .with_collection_length(None, Some(10))
            .unwrap_or_default();
        assert!(r_col_max.intersect(&r_none).is_ok());
        assert!(r_none.intersect(&r_col_max).is_ok());
        let r_nmin5 = Refinement::new()
            .with_number_range(Number::from(5), Number::from(20))
            .unwrap_or_default();
        let r_nmin10 = Refinement::new()
            .with_number_range(Number::from(10), Number::from(15))
            .unwrap_or_default();
        assert!(r_nmin5.intersect(&r_none).is_ok());
        assert!(r_none.intersect(&r_nmin5).is_ok());
        assert!(r_nmin10.intersect(&r_nmin5).is_ok());
        assert!(r_nmin5.intersect(&r_nmin10).is_ok());
        let r_raw_pre = Refinement {
            string_prefix: Some("abcdef".into()),
            ..Default::default()
        };
        let r_short_max = Refinement {
            string_length_max: Some(3),
            ..Default::default()
        };
        assert!(r_raw_pre.intersect(&r_short_max).is_err());
        let r_raw_suf = Refinement {
            string_suffix: Some("abcdef".into()),
            ..Default::default()
        };
        assert!(r_raw_suf.intersect(&r_short_max).is_err());
        let r_pre5 = Refinement::new().with_prefix("12345");
        let r_max4 = Refinement::new()
            .with_string_length(None, Some(4))
            .unwrap_or_default();
        assert!(r_pre5.intersect(&r_max4).is_err());
        let r_obj_a = Refinement::new().with_object_attr("a", Refinement::not_null());
        let r_obj_b = Refinement::new().with_object_attr("b", Refinement::not_null());
        let r_obj_both = r_obj_a.intersect(&r_obj_b).unwrap_or_default();
        assert_eq!(r_obj_both.object_attrs.len(), 2);
        let mut set_data = std::collections::BTreeSet::new();
        set_data.insert(Value::new(Type::String, ValueData::String("1".into())));
        set_data.insert(Value::new(Type::String, ValueData::String("2".into())));
        let val_set = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_data));
        let r_set_max = Refinement::new()
            .with_collection_length(None, Some(1))
            .unwrap_or_default();
        assert!(r_set_max.satisfies(&val_set).is_err());
        let mut obj_data = std::collections::BTreeMap::new();
        obj_data.insert(
            "x".to_string(),
            Value::new(Type::String, ValueData::String("val".into())),
        );
        let obj_type = Type::Object {
            attrs: std::collections::BTreeMap::new(),
            optional_attrs: std::collections::BTreeSet::new(),
        };
        let val_obj = Value::new(obj_type.clone(), ValueData::Object(obj_data));
        let r_obj_min = Refinement::new()
            .with_collection_length(Some(2), None)
            .unwrap_or_default();
        assert!(r_obj_min.satisfies(&val_obj).is_err());
        let r_obj_max = Refinement::new()
            .with_collection_length(None, Some(0))
            .unwrap_or_default();
        assert!(r_obj_max.satisfies(&val_obj).is_err());
        let mut obj_with_null = std::collections::BTreeMap::new();
        obj_with_null.insert("k".to_string(), Value::null(Type::String));
        let val_obj_null = Value::new(obj_type.clone(), ValueData::Object(obj_with_null));
        let r_obj_req = Refinement::new().with_object_attr("k", Refinement::not_null());
        assert!(r_obj_req.satisfies(&val_obj_null).is_err());
        let empty_obj = Value::new(
            obj_type,
            ValueData::Object(std::collections::BTreeMap::new()),
        );
        let r_obj_opt = Refinement::new().with_object_attr("optional_k", Refinement::new());
        assert!(r_obj_opt.satisfies(&empty_obj).is_ok());
    }
    /// Tests edge cases and all combinations of `is_empty`, `with_prefix`, and range validations.
    #[test]
    fn test_refinement_branch_coverage_gaps() {
        assert!(
            !Refinement {
                string_length_min: Some(1),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !Refinement {
                string_length_max: Some(5),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !Refinement {
                string_prefix: Some("pre".to_string()),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !Refinement {
                string_suffix: Some("suf".to_string()),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !Refinement {
                collection_length_min: Some(1),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !Refinement {
                collection_length_max: Some(5),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !Refinement {
                number_min: Some(Number::from(1)),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !Refinement {
                number_max: Some(Number::from(10)),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !Refinement {
                object_attrs: [("k".to_string(), Refinement::new())].into_iter().collect(),
                ..Default::default()
            }
            .is_empty()
        );
        let r_pre = Refinement::new()
            .with_string_length(Some(2), None)
            .unwrap()
            .with_prefix("a");
        assert_eq!(r_pre.string_length_min, Some(2));
        let r_suf = Refinement::new()
            .with_string_length(Some(3), None)
            .unwrap()
            .with_suffix("b");
        assert_eq!(r_suf.string_length_min, Some(3));
        assert!(
            Refinement::new()
                .with_string_length(Some(10), Some(2))
                .is_err()
        );
        assert!(Refinement::new().with_string_length(Some(10), 2).is_err());
        assert!(Refinement::new().with_string_length(10, Some(2)).is_err());
        assert!(
            Refinement::new()
                .with_collection_length(Some(10), Some(2))
                .is_err()
        );
        assert!(
            Refinement::new()
                .with_collection_length(Some(10), 2)
                .is_err()
        );
        assert!(
            Refinement::new()
                .with_collection_length(10, Some(2))
                .is_err()
        );
        assert!(
            Refinement::new()
                .with_number_range(Some(Number::from(10)), Some(Number::from(2)))
                .is_err()
        );
        assert!(
            Refinement::new()
                .with_number_range(Some(Number::from(10)), Number::from(2))
                .is_err()
        );
        assert!(
            Refinement::new()
                .with_number_range(Number::from(10), Some(Number::from(2)))
                .is_err()
        );
    }
}
