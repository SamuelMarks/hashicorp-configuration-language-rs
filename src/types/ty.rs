//! Type definitions matching `HashiCorp`'s `cty`.

use crate::error::HclError;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Represents an HCL Type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// The Dynamic pseudo-type, representing any type.
    Dynamic,
    /// String type.
    String,
    /// Number type.
    Number,
    /// Boolean type.
    Bool,
    /// A list of elements of a specific type.
    List(Box<Type>),
    /// A set of unique elements of a specific type.
    Set(Box<Type>),
    /// A map with string keys and values of a specific type.
    Map(Box<Type>),
    /// An object with a specific schema of string keys to typed values, and optional attribute names.
    Object {
        /// The types of the object attributes.
        attrs: BTreeMap<String, Type>,
        /// The set of attribute names that are optional.
        optional_attrs: BTreeSet<String>,
    },
    /// A tuple of elements, each potentially a different type.
    Tuple(Vec<Type>),
    /// An encapsulated Rust type (`cty.Capsule`).
    Capsule {
        /// The symbolic name of the encapsulated type.
        name: &'static str,
        /// The Rust type identifier.
        type_id: std::any::TypeId,
        /// Optional custom capsule operations (equality, hashing, conversions).
        ops: Option<std::sync::Arc<CapsuleOps>>,
    },
}

/// Equality function for comparing two encapsulated values.
pub type CapsuleEqualsFn =
    std::sync::Arc<dyn Fn(&dyn std::any::Any, &dyn std::any::Any) -> bool + Send + Sync>;

/// Hashing function for computing a 64-bit hash code for an encapsulated value.
pub type CapsuleHashFn = std::sync::Arc<dyn Fn(&dyn std::any::Any) -> u64 + Send + Sync>;

/// Conversion function from an encapsulated value to an HCL [`crate::types::val::Value`].
pub type CapsuleConversionToFn = std::sync::Arc<
    dyn Fn(&dyn std::any::Any, &Type) -> Option<crate::types::val::Value> + Send + Sync,
>;

/// Conversion function from an HCL [`crate::types::val::Value`] into an encapsulated type.
pub type CapsuleConversionFromFn = std::sync::Arc<
    dyn Fn(&crate::types::val::Value) -> Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>
        + Send
        + Sync,
>;

/// Binary arithmetic operation closure for capsule values.
pub type CapsuleBinaryOpFn = std::sync::Arc<
    dyn Fn(
            &dyn std::any::Any,
            &dyn std::any::Any,
        ) -> Result<Box<dyn std::any::Any + Send + Sync>, String>
        + Send
        + Sync,
>;

/// Unary arithmetic operation closure for capsule values.
pub type CapsuleUnaryOpFn = std::sync::Arc<
    dyn Fn(&dyn std::any::Any) -> Result<Box<dyn std::any::Any + Send + Sync>, String>
        + Send
        + Sync,
>;

/// Relational comparison operation closure for capsule values.
pub type CapsuleCmpFn = std::sync::Arc<
    dyn Fn(&dyn std::any::Any, &dyn std::any::Any) -> Result<std::cmp::Ordering, String>
        + Send
        + Sync,
>;

/// Indexing operation closure for capsule values.
pub type CapsuleIndexGetFn = std::sync::Arc<
    dyn Fn(
            &dyn std::any::Any,
            &crate::types::val::Value,
        ) -> Result<crate::types::val::Value, String>
        + Send
        + Sync,
>;

/// Attribute lookup operation closure for capsule values.
pub type CapsuleAttrGetFn = std::sync::Arc<
    dyn Fn(&dyn std::any::Any, &str) -> Result<crate::types::val::Value, String> + Send + Sync,
>;

/// Method execution closure for capsule values: `(capsule_any, args) -> Result<Value, HclError>`.
pub type CapsuleMethodFn = std::sync::Arc<
    dyn Fn(
            &dyn std::any::Any,
            &[crate::types::val::Value],
        ) -> Result<crate::types::val::Value, HclError>
        + Send
        + Sync,
>;

/// Compares two optional Arc references by pointer equality.
///
/// # Arguments
/// * `a` - The first optional Arc reference.
/// * `b` - The second optional Arc reference.
///
/// # Returns
/// `true` if both are `None` or both point to the same memory location, `false` otherwise.
fn opt_arc_ptr_eq<T: ?Sized>(a: Option<&std::sync::Arc<T>>, b: Option<&std::sync::Arc<T>>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => std::sync::Arc::ptr_eq(x, y),
        (None, None) => true,
        _ => false,
    }
}

/// Operations and extension hooks for an encapsulated type (`cty.CapsuleWithOps`).
#[derive(Clone)]
pub struct CapsuleOps {
    /// The symbolic type name.
    pub type_name: &'static str,
    /// Custom equality function comparing two encapsulated values.
    pub equals: CapsuleEqualsFn,
    /// Custom hashing function for encapsulated values returning a 64-bit hash code.
    pub hash: CapsuleHashFn,
    /// Optional conversion from the capsule to an HCL [`crate::types::val::Value`].
    pub conversion_to: Option<CapsuleConversionToFn>,
    /// Optional conversion from an HCL [`crate::types::val::Value`] into an encapsulated type.
    pub conversion_from: Option<CapsuleConversionFromFn>,
    /// Optional addition operation hook (`+`).
    pub add: Option<CapsuleBinaryOpFn>,
    /// Optional subtraction operation hook (`-`).
    pub sub: Option<CapsuleBinaryOpFn>,
    /// Optional multiplication operation hook (`*`).
    pub mul: Option<CapsuleBinaryOpFn>,
    /// Optional division operation hook (`/`).
    pub div: Option<CapsuleBinaryOpFn>,
    /// Optional modulo operation hook (`%`).
    pub modulo: Option<CapsuleBinaryOpFn>,
    /// Optional unary negation operation hook (`-`).
    pub neg: Option<CapsuleUnaryOpFn>,
    /// Optional relational comparison operation hook (`<`, `<=`, `>`, `>=`).
    pub cmp: Option<CapsuleCmpFn>,
    /// Optional index retrieval operation hook (`capsule[index]`).
    pub index_get: Option<CapsuleIndexGetFn>,
    /// Optional attribute retrieval operation hook (`capsule.attr`).
    pub attr_get: Option<CapsuleAttrGetFn>,
    /// Named member method functions callable on capsule values (`capsule.method(...)`).
    pub methods: HashMap<String, CapsuleMethodFn>,
}

impl CapsuleOps {
    /// Creates a new `CapsuleOps` table with the required type name, equality, and hash closures.
    ///
    /// # Arguments
    /// * `type_name` - The symbolic name of the capsule type.
    /// * `equals` - The equality closure comparing two encapsulated values.
    /// * `hash` - The hashing closure computing a 64-bit hash code for an encapsulated value.
    ///
    /// # Returns
    /// A new `CapsuleOps` instance with all optional operation hooks set to `None`.
    #[must_use]
    pub fn new(type_name: &'static str, equals: CapsuleEqualsFn, hash: CapsuleHashFn) -> Self {
        Self {
            type_name,
            equals,
            hash,
            conversion_to: None,
            conversion_from: None,
            add: None,
            sub: None,
            mul: None,
            div: None,
            modulo: None,
            neg: None,
            cmp: None,
            index_get: None,
            attr_get: None,
            methods: HashMap::new(),
        }
    }

    /// Sets the conversion-to-Value closure.
    ///
    /// # Arguments
    /// * `conv` - Closure converting an encapsulated value to an HCL [`crate::types::val::Value`].
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_conversion_to(mut self, conv: CapsuleConversionToFn) -> Self {
        self.conversion_to = Some(conv);
        self
    }

    /// Sets the conversion-from-Value closure.
    ///
    /// # Arguments
    /// * `conv` - Closure converting an HCL [`crate::types::val::Value`] into an encapsulated type.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_conversion_from(mut self, conv: CapsuleConversionFromFn) -> Self {
        self.conversion_from = Some(conv);
        self
    }

    /// Sets the addition (`+`) operator closure.
    ///
    /// # Arguments
    /// * `op` - Addition operation closure.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_add(mut self, op: CapsuleBinaryOpFn) -> Self {
        self.add = Some(op);
        self
    }

    /// Sets the subtraction (`-`) operator closure.
    ///
    /// # Arguments
    /// * `op` - Subtraction operation closure.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_sub(mut self, op: CapsuleBinaryOpFn) -> Self {
        self.sub = Some(op);
        self
    }

    /// Sets the multiplication (`*`) operator closure.
    ///
    /// # Arguments
    /// * `op` - Multiplication operation closure.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_mul(mut self, op: CapsuleBinaryOpFn) -> Self {
        self.mul = Some(op);
        self
    }

    /// Sets the division (`/`) operator closure.
    ///
    /// # Arguments
    /// * `op` - Division operation closure.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_div(mut self, op: CapsuleBinaryOpFn) -> Self {
        self.div = Some(op);
        self
    }

    /// Sets the modulo (`%`) operator closure.
    ///
    /// # Arguments
    /// * `op` - Modulo operation closure.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_modulo(mut self, op: CapsuleBinaryOpFn) -> Self {
        self.modulo = Some(op);
        self
    }

    /// Sets the unary negation (`-`) operator closure.
    ///
    /// # Arguments
    /// * `op` - Unary negation operation closure.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_neg(mut self, op: CapsuleUnaryOpFn) -> Self {
        self.neg = Some(op);
        self
    }

    /// Sets the relational comparison (`<`, `<=`, `>`, `>=`) operator closure.
    ///
    /// # Arguments
    /// * `op` - Relational comparison operation closure.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_cmp(mut self, op: CapsuleCmpFn) -> Self {
        self.cmp = Some(op);
        self
    }

    /// Sets the indexing (`capsule[index]`) operator closure.
    ///
    /// # Arguments
    /// * `op` - Indexing operation closure.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_index_get(mut self, op: CapsuleIndexGetFn) -> Self {
        self.index_get = Some(op);
        self
    }

    /// Sets the attribute lookup (`capsule.attr`) operator closure.
    ///
    /// # Arguments
    /// * `op` - Attribute retrieval operation closure.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_attr_get(mut self, op: CapsuleAttrGetFn) -> Self {
        self.attr_get = Some(op);
        self
    }

    /// Registers a named method on the capsule type (`capsule.method(...)`).
    ///
    /// # Arguments
    /// * `name` - The method name.
    /// * `method` - Method execution closure.
    ///
    /// # Returns
    /// The updated `CapsuleOps` instance.
    #[must_use]
    pub fn with_method(mut self, name: impl Into<String>, method: CapsuleMethodFn) -> Self {
        self.methods.insert(name.into(), method);
        self
    }
}

impl std::fmt::Debug for CapsuleOps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapsuleOps")
            .field("type_name", &self.type_name)
            .finish_non_exhaustive()
    }
}

impl PartialEq for CapsuleOps {
    fn eq(&self, other: &Self) -> bool {
        self.type_name == other.type_name
            && std::sync::Arc::ptr_eq(&self.equals, &other.equals)
            && std::sync::Arc::ptr_eq(&self.hash, &other.hash)
            && opt_arc_ptr_eq(self.conversion_to.as_ref(), other.conversion_to.as_ref())
            && opt_arc_ptr_eq(
                self.conversion_from.as_ref(),
                other.conversion_from.as_ref(),
            )
            && opt_arc_ptr_eq(self.add.as_ref(), other.add.as_ref())
            && opt_arc_ptr_eq(self.sub.as_ref(), other.sub.as_ref())
            && opt_arc_ptr_eq(self.mul.as_ref(), other.mul.as_ref())
            && opt_arc_ptr_eq(self.div.as_ref(), other.div.as_ref())
            && opt_arc_ptr_eq(self.modulo.as_ref(), other.modulo.as_ref())
            && opt_arc_ptr_eq(self.neg.as_ref(), other.neg.as_ref())
            && opt_arc_ptr_eq(self.cmp.as_ref(), other.cmp.as_ref())
            && opt_arc_ptr_eq(self.index_get.as_ref(), other.index_get.as_ref())
            && opt_arc_ptr_eq(self.attr_get.as_ref(), other.attr_get.as_ref())
            && self.methods.len() == other.methods.len()
            && self.methods.iter().all(|(k, v)| {
                other
                    .methods
                    .get(k)
                    .is_some_and(|ov| std::sync::Arc::ptr_eq(v, ov))
            })
    }
}

impl Eq for CapsuleOps {}

impl std::hash::Hash for CapsuleOps {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.type_name.hash(state);
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Dynamic => write!(f, "dynamic"),
            Type::String => write!(f, "string"),
            Type::Number => write!(f, "number"),
            Type::Bool => write!(f, "bool"),
            Type::Capsule { name, .. } => write!(f, "capsule({name})"),
            Type::List(inner) => write!(f, "list({inner})"),
            Type::Set(inner) => write!(f, "set({inner})"),
            Type::Map(inner) => write!(f, "map({inner})"),
            Type::Tuple(elements) => {
                write!(f, "tuple([")?;
                for (i, el) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{el}")?;
                }
                write!(f, "])")
            }
            Type::Object {
                attrs,
                optional_attrs,
            } => {
                write!(f, "object({{")?;
                let mut first = true;
                for (k, v) in attrs {
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    if optional_attrs.contains(k) {
                        write!(f, "{k} = optional({v})")?;
                    } else {
                        write!(f, "{k} = {v}")?;
                    }
                }
                write!(f, "}})")
            }
        }
    }
}

impl Type {
    /// Creates an object type with no optional attributes.
    ///
    /// # Arguments
    /// * `attrs` - Mapping of attribute names to their types.
    #[must_use]
    pub fn object(attrs: BTreeMap<String, Type>) -> Self {
        Self::Object {
            attrs,
            optional_attrs: BTreeSet::new(),
        }
    }

    /// Creates an object type with optional attribute names.
    ///
    /// # Arguments
    /// * `attrs` - Mapping of attribute names to their types.
    /// * `optional_attrs` - Set of attribute names that are optional.
    #[must_use]
    pub fn object_with_optional(
        attrs: BTreeMap<String, Type>,
        optional_attrs: BTreeSet<String>,
    ) -> Self {
        Self::Object {
            attrs,
            optional_attrs,
        }
    }

    /// If this is an object type, returns the map of attribute names to types.
    #[must_use]
    pub fn object_attrs(&self) -> Option<&BTreeMap<String, Type>> {
        match self {
            Self::Object { attrs, .. } => Some(attrs),
            _ => None,
        }
    }

    /// If this is an object type, returns the set of optional attribute names.
    #[must_use]
    pub fn optional_attrs(&self) -> Option<&BTreeSet<String>> {
        match self {
            Self::Object { optional_attrs, .. } => Some(optional_attrs),
            _ => None,
        }
    }

    /// Checks if a given attribute name is optional on this object type.
    ///
    /// Returns `false` if this is not an object type or if the attribute is required.
    ///
    /// # Arguments
    /// * `attr` - The attribute name to check.
    #[must_use]
    pub fn is_attr_optional(&self, attr: &str) -> bool {
        match self {
            Self::Object { optional_attrs, .. } => optional_attrs.contains(attr),
            _ => false,
        }
    }

    /// Returns true if this type is `Dynamic`.
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        matches!(self, Type::Dynamic)
    }

    /// Returns true if this is a collection type (List, Set, Map).
    #[must_use]
    pub fn is_collection(&self) -> bool {
        matches!(self, Type::List(_) | Type::Set(_) | Type::Map(_))
    }

    /// Returns true if this is a structural type (Object, Tuple).
    #[must_use]
    pub fn is_structural(&self) -> bool {
        matches!(self, Type::Object { .. } | Type::Tuple(_))
    }

    /// Creates a new [`Type::Capsule`] for the provided Rust type `T`.
    ///
    /// # Arguments
    /// * `name` - The descriptive name of the capsule type.
    #[must_use]
    pub fn capsule<T: 'static>(name: &'static str) -> Self {
        Type::Capsule {
            name,
            type_id: std::any::TypeId::of::<T>(),
            ops: None,
        }
    }

    /// Creates a new [`Type::Capsule`] for the provided Rust type `T` with custom operations.
    ///
    /// # Arguments
    /// * `name` - The descriptive name of the capsule type.
    /// * `ops` - Custom capsule operations (equality, hashing, conversions).
    #[must_use]
    pub fn capsule_with_ops<T: 'static>(
        name: &'static str,
        ops: std::sync::Arc<CapsuleOps>,
    ) -> Self {
        Type::Capsule {
            name,
            type_id: std::any::TypeId::of::<T>(),
            ops: Some(ops),
        }
    }

    /// Returns true if this is a capsule type.
    #[must_use]
    pub fn is_capsule(&self) -> bool {
        matches!(self, Type::Capsule { .. })
    }

    /// Returns the capsule type name if this is a capsule type.
    #[must_use]
    pub fn capsule_name(&self) -> Option<&'static str> {
        match self {
            Self::Capsule { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Returns the capsule operations table if this is a capsule type with operations configured.
    #[must_use]
    pub fn capsule_ops(&self) -> Option<&std::sync::Arc<CapsuleOps>> {
        match self {
            Self::Capsule { ops, .. } => ops.as_ref(),
            _ => None,
        }
    }

    /// Serializes this [`Type`] into canonical `MessagePack` binary format.
    ///
    /// # Errors
    /// Returns [`crate::error::HclError::MsgPackEncode`] on serialization failure.
    pub fn to_msgpack(&self) -> Result<Vec<u8>, crate::error::HclError> {
        crate::types::msgpack::encode_type(self)
    }

    /// Deserializes a [`Type`] from canonical `MessagePack` binary format.
    ///
    /// # Arguments
    /// * `bytes` - The `MessagePack` bytes to decode.
    ///
    /// # Errors
    /// Returns [`crate::error::HclError::MsgPackDecode`] if the bytes are malformed.
    pub fn from_msgpack(bytes: &[u8]) -> Result<Self, crate::error::HclError> {
        crate::types::msgpack::decode_type(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_checks() {
        assert!(Type::Dynamic.is_dynamic());
        assert!(!Type::String.is_dynamic());

        assert!(Type::List(Box::new(Type::String)).is_collection());
        assert!(Type::Set(Box::new(Type::Number)).is_collection());
        assert!(Type::Map(Box::new(Type::Bool)).is_collection());
        assert!(!Type::String.is_collection());

        assert!(Type::object(BTreeMap::new()).is_structural());
        assert!(Type::Tuple(vec![]).is_structural());
        assert!(!Type::Number.is_structural());

        let cap = Type::capsule::<u64>("custom_u64");
        assert!(cap.is_capsule());
        assert!(!cap.is_collection());
        assert!(!cap.is_structural());
        assert!(!cap.is_dynamic());
        assert_eq!(cap, Type::capsule::<u64>("custom_u64"));
        assert_ne!(cap, Type::capsule::<String>("custom_string"));
    }

    #[test]
    fn test_type_display() {
        assert!(Type::Dynamic.to_string().contains("dynamic"));
        assert!(Type::String.to_string().contains("string"));
        assert!(Type::Number.to_string().contains("number"));
        assert!(Type::Bool.to_string().contains("bool"));

        let cap = Type::capsule::<i32>("my_int");
        assert_eq!(cap.to_string(), "capsule(my_int)");
        assert_eq!(cap.capsule_name(), Some("my_int"));
        assert_eq!(Type::String.capsule_name(), None);

        assert_eq!(
            Type::List(Box::new(Type::String)).to_string(),
            "list(string)"
        );
        assert!(
            Type::Set(Box::new(Type::Number))
                .to_string()
                .contains("set(number)")
        );
        assert!(
            Type::Map(Box::new(Type::Bool))
                .to_string()
                .contains("map(bool)")
        );

        assert!(Type::Tuple(vec![]).to_string().contains("tuple([])"));
        assert_eq!(Type::Tuple(vec![Type::Bool]).to_string(), "tuple([bool])");
        assert_eq!(
            Type::Tuple(vec![Type::String, Type::Number]).to_string(),
            "tuple([string, number])"
        );

        let mut obj = BTreeMap::new();
        assert!(Type::object(obj.clone()).to_string().contains("object({})"));

        obj.insert("a".to_string(), Type::String);
        assert_eq!(
            Type::object(obj.clone()).to_string(),
            "object({a = string})"
        );

        obj.insert("b".to_string(), Type::Number);
        assert_eq!(
            Type::object(obj.clone()).to_string(),
            "object({a = string, b = number})"
        );

        let mut opt_set = BTreeSet::new();
        opt_set.insert("b".to_string());
        let obj_opt = Type::object_with_optional(obj, opt_set);
        assert_eq!(
            obj_opt.to_string(),
            "object({a = string, b = optional(number)})"
        );
        assert!(obj_opt.is_attr_optional("b"));
        assert!(!obj_opt.is_attr_optional("a"));
        assert_eq!(obj_opt.object_attrs().map(BTreeMap::len), Some(2));
        assert_eq!(obj_opt.optional_attrs().map(BTreeSet::len), Some(1));

        assert_eq!(Type::String.object_attrs(), None);
        assert_eq!(Type::String.optional_attrs(), None);
        assert!(!Type::String.is_attr_optional("a"));

        let mut single_opt = BTreeMap::new();
        single_opt.insert("x".to_string(), Type::Bool);
        let mut single_opt_set = BTreeSet::new();
        single_opt_set.insert("x".to_string());
        let single_opt_ty = Type::object_with_optional(single_opt, single_opt_set);
        assert_eq!(single_opt_ty.to_string(), "object({x = optional(bool)})");
    }

    struct StepWriter {
        remaining: usize,
    }

    impl std::fmt::Write for StepWriter {
        fn write_str(&mut self, _: &str) -> std::fmt::Result {
            if self.remaining == 0 {
                Err(std::fmt::Error)
            } else {
                self.remaining = self.remaining.saturating_sub(1);
                Ok(())
            }
        }
    }

    #[test]
    fn test_type_display_error_paths() {
        let tuple_ty = Type::Tuple(vec![Type::String, Type::Number]);
        for r in 0..10 {
            let mut w = StepWriter { remaining: r };
            let _ = std::fmt::write(&mut w, format_args!("{tuple_ty}"));
        }

        let mut attrs_non_opt = BTreeMap::new();
        attrs_non_opt.insert("a".to_string(), Type::String);
        attrs_non_opt.insert("b".to_string(), Type::Number);
        let obj_non_opt = Type::object(attrs_non_opt);
        for r in 0..15 {
            let mut w = StepWriter { remaining: r };
            let _ = std::fmt::write(&mut w, format_args!("{obj_non_opt}"));
        }

        let mut attrs_opt = BTreeMap::new();
        attrs_opt.insert("a".to_string(), Type::String);
        attrs_opt.insert("b".to_string(), Type::Number);
        let mut opt_set = BTreeSet::new();
        opt_set.insert("a".to_string());
        opt_set.insert("b".to_string());
        let obj_opt = Type::object_with_optional(attrs_opt, opt_set);
        for r in 0..15 {
            let mut w = StepWriter { remaining: r };
            let _ = std::fmt::write(&mut w, format_args!("{obj_opt}"));
        }
    }

    #[test]
    fn test_type_msgpack_roundtrip() {
        let ty = Type::List(Box::new(Type::String));
        let bytes = ty.to_msgpack().expect("encoded");
        let decoded = Type::from_msgpack(&bytes).expect("decoded");
        assert_eq!(decoded, ty);

        assert!(Type::from_msgpack(&[]).is_err());
    }

    #[test]
    fn test_capsule_ops_traits() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let eq_fn: CapsuleEqualsFn = std::sync::Arc::new(|_, _| true);
        let hash_fn: CapsuleHashFn = std::sync::Arc::new(|_| 42);

        let ops1 = CapsuleOps::new("test_ops", eq_fn.clone(), hash_fn.clone());
        let ops2 = CapsuleOps::new("test_ops", eq_fn.clone(), hash_fn.clone());
        let ops3 = CapsuleOps::new("diff_ops", eq_fn.clone(), hash_fn.clone());

        assert_eq!(ops1, ops2);
        assert_ne!(ops1, ops3);

        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();
        ops1.hash(&mut h1);
        ops2.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());

        assert!(format!("{ops1:?}").contains("test_ops"));

        // Test all builder methods and equality checks
        let add_fn: CapsuleBinaryOpFn = std::sync::Arc::new(|_, _| Ok(Box::new(0_i32)));
        let sub_fn: CapsuleBinaryOpFn = std::sync::Arc::new(|_, _| Ok(Box::new(0_i32)));
        let mul_fn: CapsuleBinaryOpFn = std::sync::Arc::new(|_, _| Ok(Box::new(0_i32)));
        let div_fn: CapsuleBinaryOpFn = std::sync::Arc::new(|_, _| Ok(Box::new(0_i32)));
        let mod_fn: CapsuleBinaryOpFn = std::sync::Arc::new(|_, _| Ok(Box::new(0_i32)));
        let neg_fn: CapsuleUnaryOpFn = std::sync::Arc::new(|_| Ok(Box::new(0_i32)));
        let cmp_fn: CapsuleCmpFn = std::sync::Arc::new(|_, _| Ok(std::cmp::Ordering::Equal));
        let idx_fn: CapsuleIndexGetFn =
            std::sync::Arc::new(|_, _| Ok(crate::types::val::Value::null(Type::Dynamic)));
        let attr_fn: CapsuleAttrGetFn =
            std::sync::Arc::new(|_, _| Ok(crate::types::val::Value::null(Type::Dynamic)));
        let conv_to: CapsuleConversionToFn = std::sync::Arc::new(|_, _| None);
        let conv_from: CapsuleConversionFromFn = std::sync::Arc::new(|_| None);

        let ops_full = CapsuleOps::new(
            "full_ops",
            std::sync::Arc::new(|_, _| true),
            std::sync::Arc::new(|_| 1),
        )
        .with_conversion_to(conv_to.clone())
        .with_conversion_from(conv_from.clone())
        .with_add(add_fn.clone())
        .with_sub(sub_fn.clone())
        .with_mul(mul_fn.clone())
        .with_div(div_fn.clone())
        .with_modulo(mod_fn.clone())
        .with_neg(neg_fn.clone())
        .with_cmp(cmp_fn.clone())
        .with_index_get(idx_fn.clone())
        .with_attr_get(attr_fn.clone());

        assert_eq!(ops_full, ops_full);
        assert_ne!(ops1, ops_full);

        // Execute all closures in ops_full
        assert!((ops_full.equals)(&(), &()));
        assert_eq!((ops_full.hash)(&()), 1);
        assert!(ops_full.conversion_to.as_ref().expect("conv_to")(&(), &Type::Dynamic).is_none());
        assert!(
            ops_full.conversion_from.as_ref().expect("conv_from")(&crate::types::val::Value::null(
                Type::Dynamic
            ))
            .is_none()
        );
        assert!(ops_full.add.as_ref().expect("add")(&(), &()).is_ok());
        assert!(ops_full.sub.as_ref().expect("sub")(&(), &()).is_ok());
        assert!(ops_full.mul.as_ref().expect("mul")(&(), &()).is_ok());
        assert!(ops_full.div.as_ref().expect("div")(&(), &()).is_ok());
        assert!(ops_full.modulo.as_ref().expect("modulo")(&(), &()).is_ok());
        assert!(ops_full.neg.as_ref().expect("neg")(&()).is_ok());
        assert_eq!(
            ops_full.cmp.as_ref().expect("cmp")(&(), &()),
            Ok(std::cmp::Ordering::Equal)
        );
        assert!(
            ops_full.index_get.as_ref().expect("index_get")(
                &(),
                &crate::types::val::Value::null(Type::Dynamic)
            )
            .is_ok()
        );
        assert!(ops_full.attr_get.as_ref().expect("attr_get")(&(), "attr").is_ok());

        // Test difference in equals and hash when type names match
        let eq_alt: CapsuleEqualsFn = std::sync::Arc::new(|_, _| false);
        assert!(!(eq_alt)(&(), &()));
        let hash_alt: CapsuleHashFn = std::sync::Arc::new(|_| 999);
        assert_eq!((hash_alt)(&()), 999);

        let ops_diff_eq = CapsuleOps::new("test_ops", eq_alt, hash_fn.clone());
        assert_ne!(ops1, ops_diff_eq);

        let ops_diff_hash = CapsuleOps::new("test_ops", eq_fn.clone(), hash_alt);
        assert_ne!(ops1, ops_diff_hash);

        // Test opt_arc_ptr_eq with Some vs None and None vs Some
        let ops_none_conv = ops1.clone();
        let ops_some_conv = ops1.clone().with_conversion_to(conv_to);
        assert_ne!(ops_none_conv, ops_some_conv);
        assert_ne!(ops_some_conv, ops_none_conv);

        // Check opt_arc_ptr_eq differences and invoke replacement closures
        let diff_bin: CapsuleBinaryOpFn = std::sync::Arc::new(|_, _| Ok(Box::new(1_i32)));
        assert!(diff_bin(&(), &()).is_ok());
        let diff_unary: CapsuleUnaryOpFn = std::sync::Arc::new(|_| Ok(Box::new(1_i32)));
        assert!(diff_unary(&()).is_ok());
        let diff_cmp: CapsuleCmpFn = std::sync::Arc::new(|_, _| Ok(std::cmp::Ordering::Less));
        assert_eq!(diff_cmp(&(), &()), Ok(std::cmp::Ordering::Less));
        let diff_idx: CapsuleIndexGetFn =
            std::sync::Arc::new(|_, _| Ok(crate::types::val::Value::null(Type::Dynamic)));
        assert!(diff_idx(&(), &crate::types::val::Value::null(Type::Dynamic)).is_ok());
        let diff_attr: CapsuleAttrGetFn =
            std::sync::Arc::new(|_, _| Ok(crate::types::val::Value::null(Type::Dynamic)));
        assert!(diff_attr(&(), "x").is_ok());
        let diff_conv_to: CapsuleConversionToFn = std::sync::Arc::new(|_, _| None);
        assert!(diff_conv_to(&(), &Type::Dynamic).is_none());
        let diff_conv_from: CapsuleConversionFromFn = std::sync::Arc::new(|_| None);
        assert!(diff_conv_from(&crate::types::val::Value::null(Type::Dynamic)).is_none());
        let diff_method: CapsuleMethodFn =
            std::sync::Arc::new(|_, _| Ok(crate::types::val::Value::null(Type::Dynamic)));
        assert!(diff_method(&(), &[]).is_ok());

        let ops_diff_add = ops_full.clone().with_add(diff_bin.clone());
        assert_ne!(ops_full, ops_diff_add);
        let ops_diff_sub = ops_full.clone().with_sub(diff_bin.clone());
        assert_ne!(ops_full, ops_diff_sub);
        let ops_diff_mul = ops_full.clone().with_mul(diff_bin.clone());
        assert_ne!(ops_full, ops_diff_mul);
        let ops_diff_div = ops_full.clone().with_div(diff_bin.clone());
        assert_ne!(ops_full, ops_diff_div);
        let ops_diff_mod = ops_full.clone().with_modulo(diff_bin);
        assert_ne!(ops_full, ops_diff_mod);
        let ops_diff_neg = ops_full.clone().with_neg(diff_unary);
        assert_ne!(ops_full, ops_diff_neg);
        let ops_diff_cmp = ops_full.clone().with_cmp(diff_cmp);
        assert_ne!(ops_full, ops_diff_cmp);
        let ops_diff_idx = ops_full.clone().with_index_get(diff_idx);
        assert_ne!(ops_full, ops_diff_idx);
        let ops_diff_attr = ops_full.clone().with_attr_get(diff_attr);
        assert_ne!(ops_full, ops_diff_attr);
        let ops_diff_conv_to = ops_full.clone().with_conversion_to(diff_conv_to);
        assert_ne!(ops_full, ops_diff_conv_to);
        let ops_diff_conv_from = ops_full.clone().with_conversion_from(diff_conv_from);
        assert_ne!(ops_full, ops_diff_conv_from);
        let ops_diff_method = ops_full.clone().with_method("test_m", diff_method.clone());
        assert_ne!(ops_full, ops_diff_method);

        // Debug formatting
        assert!(format!("{ops_full:?}").contains("CapsuleOps"));

        // Compare ops with equal methods
        let ops_same_method = ops_full.clone().with_method("m1", diff_method.clone());
        let ops_same_method_2 = ops_full.clone().with_method("m1", diff_method.clone());
        assert_eq!(ops_same_method, ops_same_method_2);

        // Compare ops with different method names
        let ops_diff_mname = ops_full.clone().with_method("m2", diff_method);
        assert_ne!(ops_same_method, ops_diff_mname);

        let cap_with_ops =
            Type::capsule_with_ops::<i32>("cap_ops", std::sync::Arc::new(ops_full.clone()));
        assert_eq!(
            cap_with_ops.capsule_ops(),
            Some(&std::sync::Arc::new(ops_full))
        );
        assert_eq!(Type::String.capsule_ops(), None);
    }
}
