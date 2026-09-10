#![allow(clippy::all, clippy::pedantic)]

//! HCL values mapping to types.

use crate::error::HclError;
use crate::number::Number;
use crate::types::Type;
use crate::types::path::{Path, PathStep};
use crate::types::refinement::Refinement;
use bigdecimal::num_traits::ToPrimitive;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// The internal representation of a typed HCL Value.
#[derive(Clone)]
pub enum ValueData {
    /// A null value of a specific type.
    Null,
    /// An unknown value of a specific type (used during planning phases),
    /// optionally carrying type refinements (`cty.Refinement`).
    Unknown(Option<Arc<Refinement>>),
    /// A boolean value.
    Bool(bool),
    /// A number value.
    Number(Number),
    /// A string value.
    String(String),
    /// A list or tuple of values.
    Array(Vec<Value>),
    /// A set of values.
    Set(BTreeSet<Value>),
    /// A map or object of string keys to values.
    Object(BTreeMap<String, Value>),
    /// An encapsulated Rust value (`cty.Capsule`).
    Capsule(Arc<dyn std::any::Any + Send + Sync>),
}

impl std::fmt::Debug for ValueData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueData::Null => write!(f, "Null"),
            ValueData::Unknown(None) => write!(f, "Unknown"),
            ValueData::Unknown(Some(r)) => write!(f, "Unknown({r:?})"),
            ValueData::Bool(b) => f.debug_tuple("Bool").field(b).finish(),
            ValueData::Number(n) => f.debug_tuple("Number").field(n).finish(),
            ValueData::String(s) => f.debug_tuple("String").field(s).finish(),
            ValueData::Array(a) => f.debug_tuple("Array").field(a).finish(),
            ValueData::Set(s) => f.debug_tuple("Set").field(s).finish(),
            ValueData::Object(o) => f.debug_tuple("Object").field(o).finish(),
            ValueData::Capsule(c) => write!(f, "Capsule({:p})", Arc::as_ptr(c)),
        }
    }
}

/// Trait for user-defined, strongly-typed marks that can be attached to a [`Value`].
pub trait AnyValueMark: std::any::Any + Send + Sync + std::fmt::Debug + std::fmt::Display {
    /// Returns the mark as a trait object with `'static` lifetime for downcasting.
    fn as_any(&self) -> &dyn std::any::Any;
    /// Compares equality with another `AnyValueMark`.
    fn dyn_eq(&self, other: &dyn AnyValueMark) -> bool;
    /// Feeds this mark into the given hasher.
    fn dyn_hash(&self, state: &mut dyn std::hash::Hasher);
    /// Returns the type name or identifier of this mark.
    fn mark_name(&self) -> &'static str;
}

/// An open-ended, type-erased container holding an arbitrary user-defined mark.
#[derive(Clone)]
pub struct TypedMark(pub Arc<dyn AnyValueMark>);

impl TypedMark {
    /// Creates a new `TypedMark` wrapping the given typed mark.
    ///
    /// # Arguments
    /// * `mark` - The mark implementing `AnyValueMark`.
    #[must_use]
    pub fn new<M: AnyValueMark + Clone + 'static>(mark: M) -> Self {
        Self(Arc::new(mark))
    }
}

impl std::fmt::Debug for TypedMark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypedMark({:?})", self.0)
    }
}

impl std::fmt::Display for TypedMark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialEq for TypedMark {
    fn eq(&self, other: &Self) -> bool {
        self.0.dyn_eq(&*other.0)
    }
}

impl Eq for TypedMark {}

impl std::hash::Hash for TypedMark {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_any().type_id().hash(state);
        self.0.dyn_hash(state);
    }
}

impl PartialOrd for TypedMark {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TypedMark {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.0.dyn_eq(&*other.0) {
            return std::cmp::Ordering::Equal;
        }
        let type_id_a = self.0.as_any().type_id();
        let type_id_b = other.0.as_any().type_id();
        type_id_a.cmp(&type_id_b).then_with(|| {
            self.0.mark_name().cmp(other.0.mark_name()).then_with(|| {
                let s_a = self.0.to_string();
                let s_b = other.0.to_string();
                s_a.cmp(&s_b).then_with(|| {
                    (Arc::as_ptr(&self.0) as *const () as usize)
                        .cmp(&(Arc::as_ptr(&other.0) as *const () as usize))
                })
            })
        })
    }
}

impl<T> AnyValueMark for T
where
    T: std::any::Any
        + Send
        + Sync
        + std::fmt::Debug
        + std::fmt::Display
        + Eq
        + std::hash::Hash
        + 'static,
{
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn dyn_eq(&self, other: &dyn AnyValueMark) -> bool {
        if let Some(other_concrete) = other.as_any().downcast_ref::<T>() {
            self == other_concrete
        } else {
            false
        }
    }
    fn dyn_hash(&self, mut state: &mut dyn std::hash::Hasher) {
        std::hash::Hash::hash(self, &mut state);
    }
    fn mark_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}

/// A mark associated with a [`Value`].
///
/// Marks allow tracking metadata such as sensitivity through expression evaluation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueMark {
    /// Indicates that a value is sensitive and should not be displayed in logs or diagnostics.
    Sensitive,
    /// A custom mark with an identifying string.
    Custom(String),
    /// An open-ended, type-erased custom mark.
    Typed(TypedMark),
}

impl ValueMark {
    /// Creates a new custom mark.
    ///
    /// # Arguments
    /// * `name` - The string identifier for the mark.
    #[must_use]
    pub fn custom(name: impl Into<String>) -> Self {
        Self::Custom(name.into())
    }

    /// Creates a new typed mark wrapping an arbitrary user-defined type.
    ///
    /// # Arguments
    /// * `mark` - The mark implementing `AnyValueMark`.
    #[must_use]
    pub fn typed<M: AnyValueMark + Clone + 'static>(mark: M) -> Self {
        Self::Typed(TypedMark::new(mark))
    }
}

impl std::fmt::Display for ValueMark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sensitive => write!(f, "sensitive"),
            Self::Custom(s) => write!(f, "{s}"),
            Self::Typed(t) => write!(f, "{t}"),
        }
    }
}

/// A value in the HCL language. Every value has a defined `Type`.
#[derive(Debug)]
pub struct Value {
    /// The type of this value.
    ty: Type,
    /// The marks associated with this value.
    pub marks: BTreeSet<ValueMark>,
    /// The underlying data.
    pub data: Box<ValueData>,
}

impl Clone for Value {
    fn clone(&self) -> Self {
        Self {
            ty: self.ty.clone(),
            marks: self.marks.clone(),
            data: self.data.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.ty.clone_from(&source.ty);
        self.marks.clone_from(&source.marks);
        self.data.clone_from(&source.data);
    }
}

impl PartialEq for ValueData {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ValueData::Null, ValueData::Null) => true,
            (ValueData::Unknown(a), ValueData::Unknown(b)) => a == b,
            (ValueData::Bool(a), ValueData::Bool(b)) => a == b,
            (ValueData::Number(a), ValueData::Number(b)) => a == b,
            (ValueData::String(a), ValueData::String(b)) => a == b,
            (ValueData::Array(a), ValueData::Array(b)) => a == b,
            (ValueData::Set(a), ValueData::Set(b)) => a == b,
            (ValueData::Object(a), ValueData::Object(b)) => a == b,
            (ValueData::Capsule(a), ValueData::Capsule(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
impl Eq for ValueData {}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        if self.ty != other.ty || self.marks != other.marks {
            return false;
        }
        if let (ValueData::Capsule(a), ValueData::Capsule(b)) = (&*self.data, &*other.data)
            && let Some(ops) = self.ty.capsule_ops()
        {
            return (ops.equals)(&**a, &**b);
        }
        self.data == other.data
    }
}
impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ty.hash(state);
        self.marks.hash(state);
        // We only hash simple types for set compatibility.
        // HCL Sets of complex types may require specialized hashing rules later.
        match &*self.data {
            ValueData::Null => state.write_u8(0),
            ValueData::Unknown(r) => {
                state.write_u8(1);
                r.hash(state);
            }
            ValueData::Bool(b) => {
                state.write_u8(2);
                b.hash(state);
            }
            ValueData::String(s) => {
                state.write_u8(3);
                s.hash(state);
            }
            // Number hashing requires bigdecimal hash support which we might need to implement manually
            ValueData::Number(_) => state.write_u8(4),
            ValueData::Array(_) => state.write_u8(5),
            ValueData::Set(_) => state.write_u8(6),
            ValueData::Object(_) => state.write_u8(7),
            ValueData::Capsule(c) => {
                state.write_u8(8);
                if let Some(ops) = self.ty.capsule_ops() {
                    state.write_u64((ops.hash)(&**c));
                } else {
                    Arc::as_ptr(c).hash(state);
                }
            }
        }
    }
}
impl PartialOrd for Value {
    fn partial_cmp(&self, _other: &Self) -> Option<std::cmp::Ordering> {
        None
    }
}
impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Sets require total ordering, we fallback to string/number comparison if available.
        self.ty
            .is_dynamic()
            .cmp(&other.ty.is_dynamic())
            .then_with(|| self.marks.cmp(&other.marks))
            .then_with(|| match (&*self.data, &*other.data) {
                (ValueData::String(a), ValueData::String(b)) => a.cmp(b),
                (ValueData::Number(a), ValueData::Number(b)) => a.cmp(b),
                (ValueData::Capsule(a), ValueData::Capsule(b)) => {
                    if let Some(ops) = self.ty.capsule_ops()
                        && let Some(ref cmp_fn) = ops.cmp
                        && let Ok(ord) = (cmp_fn)(&**a, &**b)
                    {
                        ord
                    } else {
                        std::cmp::Ordering::Equal
                    }
                }
                _ => std::cmp::Ordering::Equal,
            })
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.has_mark(&ValueMark::Sensitive) {
            return write!(f, "(sensitive value)");
        }
        match &*self.data {
            ValueData::Null => write!(f, "null"),
            ValueData::Unknown(_) => write!(f, "(known after apply)"),
            ValueData::Bool(b) => write!(f, "{b}"),
            ValueData::Number(n) => write!(f, "{}", n.0),
            ValueData::String(s) => write!(f, "\"{s}\""),
            ValueData::Array(arr) => {
                write!(f, "[")?;
                for (i, elem) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{elem}")?;
                }
                write!(f, "]")
            }
            ValueData::Set(set) => {
                write!(f, "[")?;
                for (i, elem) in set.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{elem}")?;
                }
                write!(f, "]")
            }
            ValueData::Object(obj) => {
                write!(f, "{{")?;
                for (i, (k, val)) in obj.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{k}\" = {val}")?;
                }
                write!(f, "}}")
            }
            ValueData::Capsule(arc) => write!(f, "capsule({:p})", Arc::as_ptr(arc)),
        }
    }
}

impl Value {
    /// Create a new Value.
    #[must_use]
    pub fn new(ty: Type, data: ValueData) -> Self {
        Self {
            ty,
            marks: BTreeSet::new(),
            data: Box::new(data),
        }
    }

    /// Create a new `Value` with the specified marks.
    ///
    /// # Arguments
    /// * `ty` - The type of the value.
    /// * `data` - The underlying value data.
    /// * `marks` - The initial set of marks.
    #[must_use]
    pub fn new_with_marks(ty: Type, data: ValueData, marks: BTreeSet<ValueMark>) -> Self {
        Self {
            ty,
            marks,
            data: Box::new(data),
        }
    }

    /// Sets the marks on this value, returning the updated value.
    ///
    /// # Arguments
    /// * `marks` - The new set of marks.
    #[must_use]
    pub fn with_marks(mut self, marks: BTreeSet<ValueMark>) -> Self {
        self.marks = marks;
        self
    }

    /// Returns a new [`Value`] with the specified mark added.
    ///
    /// # Arguments
    /// * `mark` - The mark to attach to the value.
    #[must_use]
    pub fn mark(&self, mark: ValueMark) -> Self {
        let mut new_val = self.clone();
        new_val.marks.insert(mark);
        new_val
    }

    /// Returns the marks associated with this value.
    #[must_use]
    pub fn marks(&self) -> &BTreeSet<ValueMark> {
        &self.marks
    }

    /// Checks if this value has the specified mark.
    ///
    /// # Arguments
    /// * `mark` - The mark to check for.
    #[must_use]
    pub fn has_mark(&self, mark: &ValueMark) -> bool {
        self.marks.contains(mark)
    }

    /// Attaches an arbitrary strongly-typed mark to this value.
    ///
    /// # Arguments
    /// * `mark` - The strongly typed mark to attach.
    #[must_use]
    pub fn mark_typed<M: AnyValueMark + Clone + 'static>(&self, mark: M) -> Self {
        let mut new_val = self.clone();
        new_val.marks.insert(ValueMark::Typed(TypedMark::new(mark)));
        new_val
    }

    /// Retrieves a reference to a strongly-typed mark attached to this value if present.
    #[must_use]
    pub fn get_mark<M: AnyValueMark + 'static>(&self) -> Option<&M> {
        for m in &self.marks {
            if let ValueMark::Typed(tm) = m {
                if let Some(concrete) = tm.0.as_any().downcast_ref::<M>() {
                    return Some(concrete);
                }
            }
        }
        None
    }

    /// Returns `true` if this value has a strongly-typed mark of type `M`.
    #[must_use]
    pub fn has_mark_type<M: AnyValueMark + 'static>(&self) -> bool {
        self.get_mark::<M>().is_some()
    }

    /// Calls a registered method on this capsule value.
    ///
    /// If any argument is unknown, returns an unknown [`Value`].
    /// Marks present on this value are propagated to the result.
    ///
    /// # Arguments
    /// * `name` - The method name.
    /// * `args` - The arguments passed to the method.
    ///
    /// # Errors
    /// Returns [`crate::error::HclError::Capsule`] if this value is not a capsule,
    /// [`crate::error::HclError::CapsuleMethodNotFound`] if the method does not exist,
    /// or the error returned by the method.
    pub fn call_method(&self, name: &str, args: &[Value]) -> Result<Value, crate::error::HclError> {
        let (any, ops) = match (self.as_capsule_any(), self.ty().capsule_ops()) {
            (Some(any), Some(ops)) => (any, ops),
            _ => {
                return Err(crate::error::HclError::Capsule(format!(
                    "cannot call method '{name}' on non-capsule type {}",
                    self.ty()
                )));
            }
        };

        if args.iter().any(Value::is_unknown) {
            return Ok(Value::unknown(Type::Dynamic));
        }

        let method =
            ops.methods
                .get(name)
                .ok_or_else(|| crate::error::HclError::CapsuleMethodNotFound {
                    capsule_type: ops.type_name,
                    method: name.to_string(),
                })?;

        let mut res = (method)(any, args)?;
        res.marks.extend(self.marks.clone());
        Ok(res)
    }

    /// Checks if this value or any nested element is marked as [`ValueMark::Sensitive`].
    #[must_use]
    pub fn is_sensitive(&self) -> bool {
        if self.has_mark(&ValueMark::Sensitive) {
            return true;
        }
        match &*self.data {
            ValueData::Array(arr) => arr.iter().any(Self::is_sensitive),
            ValueData::Set(set) => set.iter().any(Self::is_sensitive),
            ValueData::Object(map) => map.values().any(Self::is_sensitive),
            _ => false,
        }
    }

    /// Unmarks this value, returning a copy without marks and the set of marks removed.
    #[must_use]
    pub fn unmark(&self) -> (Self, BTreeSet<ValueMark>) {
        let mut unmarked = self.clone();
        let marks = std::mem::take(&mut unmarked.marks);
        (unmarked, marks)
    }

    /// Recursively unmarks this value and all nested collection elements,
    /// returning the stripped value and the combined set of all marks found.
    #[must_use]
    pub fn unmark_deep(&self) -> (Self, BTreeSet<ValueMark>) {
        let mut marks = self.marks.clone();
        let new_data = match &*self.data {
            ValueData::Array(arr) => {
                let mut new_arr = Vec::with_capacity(arr.len());
                for elem in arr {
                    let (unmarked_elem, elem_marks) = elem.unmark_deep();
                    marks.extend(elem_marks);
                    new_arr.push(unmarked_elem);
                }
                ValueData::Array(new_arr)
            }
            ValueData::Set(set) => {
                let mut new_set = BTreeSet::new();
                for elem in set {
                    let (unmarked_elem, elem_marks) = elem.unmark_deep();
                    marks.extend(elem_marks);
                    new_set.insert(unmarked_elem);
                }
                ValueData::Set(new_set)
            }
            ValueData::Object(obj) => {
                let mut new_obj = BTreeMap::new();
                for (k, v) in obj {
                    let (unmarked_val, val_marks) = v.unmark_deep();
                    marks.extend(val_marks);
                    new_obj.insert(k.clone(), unmarked_val);
                }
                ValueData::Object(new_obj)
            }
            other => other.clone(),
        };
        let unmarked_val = Self {
            ty: self.ty.clone(),
            marks: BTreeSet::new(),
            data: Box::new(new_data),
        };
        (unmarked_val, marks)
    }

    /// Recursively strips [`ValueMark::Sensitive`] from this value and all nested elements.
    #[must_use]
    pub fn unmark_sensitive(&self) -> Self {
        let mut new_marks = self.marks.clone();
        new_marks.remove(&ValueMark::Sensitive);
        let new_data = match &*self.data {
            ValueData::Array(arr) => {
                ValueData::Array(arr.iter().map(Self::unmark_sensitive).collect())
            }
            ValueData::Set(set) => ValueData::Set(set.iter().map(Self::unmark_sensitive).collect()),
            ValueData::Object(obj) => {
                let mut new_obj = BTreeMap::new();
                for (k, v) in obj {
                    new_obj.insert(k.clone(), v.unmark_sensitive());
                }
                ValueData::Object(new_obj)
            }
            other => other.clone(),
        };
        Self {
            ty: self.ty.clone(),
            marks: new_marks,
            data: Box::new(new_data),
        }
    }

    /// Attaches a mark to the nested element located at the specified [`Path`].
    ///
    /// # Arguments
    /// * `path` - The path to the element to mark.
    /// * `mark` - The mark to attach.
    ///
    /// # Returns
    /// A new [`Value`] with the element at `path` marked.
    ///
    /// # Errors
    /// Returns [`HclError::PathError`] if the path cannot be resolved within this value.
    pub fn mark_path(&self, path: &Path, mark: ValueMark) -> Result<Self, HclError> {
        self.mark_path_steps(path.steps(), mark, path)
    }

    fn mark_path_steps(
        &self,
        steps: &[PathStep],
        mark: ValueMark,
        full_path: &Path,
    ) -> Result<Self, HclError> {
        if steps.is_empty() {
            return Ok(self.mark(mark));
        }

        let first = &steps[0];
        let rest = &steps[1..];

        match first {
            PathStep::GetAttr(name) => {
                if let ValueData::Object(map) = &*self.data {
                    let child = map.get(name).ok_or_else(|| HclError::PathError {
                        path: full_path.to_string(),
                        message: format!("Attribute '{name}' not found"),
                    })?;
                    let updated_child = child.mark_path_steps(rest, mark, full_path)?;
                    let mut new_map = map.clone();
                    new_map.insert(name.clone(), updated_child);
                    Ok(Self {
                        ty: self.ty.clone(),
                        marks: self.marks.clone(),
                        data: Box::new(ValueData::Object(new_map)),
                    })
                } else {
                    Err(HclError::PathError {
                        path: full_path.to_string(),
                        message: format!("Cannot get attribute '{name}' from non-object"),
                    })
                }
            }
            PathStep::Index(idx_val) => {
                let idx = match &*idx_val.data {
                    ValueData::Number(n) => n.0.to_usize().ok_or_else(|| HclError::PathError {
                        path: full_path.to_string(),
                        message: "Index must be a valid integer".to_string(),
                    })?,
                    _ => {
                        return Err(HclError::PathError {
                            path: full_path.to_string(),
                            message: "Index step must evaluate to a number".to_string(),
                        });
                    }
                };

                if let ValueData::Array(arr) = &*self.data {
                    if idx >= arr.len() {
                        return Err(HclError::PathError {
                            path: full_path.to_string(),
                            message: format!("Index {idx} out of bounds (len {})", arr.len()),
                        });
                    }
                    let updated_child = arr[idx].mark_path_steps(rest, mark, full_path)?;
                    let mut new_arr = arr.clone();
                    new_arr[idx] = updated_child;
                    Ok(Self {
                        ty: self.ty.clone(),
                        marks: self.marks.clone(),
                        data: Box::new(ValueData::Array(new_arr)),
                    })
                } else {
                    Err(HclError::PathError {
                        path: full_path.to_string(),
                        message: "Cannot index into non-array".to_string(),
                    })
                }
            }
            PathStep::Key(key_val) => {
                let key = match &*key_val.data {
                    ValueData::String(s) => s.as_str(),
                    _ => {
                        return Err(HclError::PathError {
                            path: full_path.to_string(),
                            message: "Key step must evaluate to a string".to_string(),
                        });
                    }
                };

                if let ValueData::Object(map) = &*self.data {
                    let child = map.get(key).ok_or_else(|| HclError::PathError {
                        path: full_path.to_string(),
                        message: format!("Key '{key}' not found"),
                    })?;
                    let updated_child = child.mark_path_steps(rest, mark, full_path)?;
                    let mut new_map = map.clone();
                    new_map.insert(key.to_string(), updated_child);
                    Ok(Self {
                        ty: self.ty.clone(),
                        marks: self.marks.clone(),
                        data: Box::new(ValueData::Object(new_map)),
                    })
                } else {
                    Err(HclError::PathError {
                        path: full_path.to_string(),
                        message: format!("Cannot lookup key '{key}' in non-object"),
                    })
                }
            }
        }
    }

    /// Removes all marks from the nested element located at the specified [`Path`].
    ///
    /// # Arguments
    /// * `path` - The path to the element to unmark.
    ///
    /// # Returns
    /// A tuple of the updated [`Value`] and the set of marks removed from the targeted element.
    ///
    /// # Errors
    /// Returns [`HclError::PathError`] if the path cannot be resolved within this value.
    pub fn unmark_path(&self, path: &Path) -> Result<(Self, BTreeSet<ValueMark>), HclError> {
        self.unmark_path_steps(path.steps(), path)
    }

    fn unmark_path_steps(
        &self,
        steps: &[PathStep],
        full_path: &Path,
    ) -> Result<(Self, BTreeSet<ValueMark>), HclError> {
        if steps.is_empty() {
            let (unmarked, marks) = self.unmark();
            return Ok((unmarked, marks));
        }

        let first = &steps[0];
        let rest = &steps[1..];

        match first {
            PathStep::GetAttr(name) => {
                if let ValueData::Object(map) = &*self.data {
                    let child = map.get(name).ok_or_else(|| HclError::PathError {
                        path: full_path.to_string(),
                        message: format!("Attribute '{name}' not found"),
                    })?;
                    let (updated_child, removed_marks) =
                        child.unmark_path_steps(rest, full_path)?;
                    let mut new_map = map.clone();
                    new_map.insert(name.clone(), updated_child);
                    Ok((
                        Self {
                            ty: self.ty.clone(),
                            marks: self.marks.clone(),
                            data: Box::new(ValueData::Object(new_map)),
                        },
                        removed_marks,
                    ))
                } else {
                    Err(HclError::PathError {
                        path: full_path.to_string(),
                        message: format!("Cannot get attribute '{name}' from non-object"),
                    })
                }
            }
            PathStep::Index(idx_val) => {
                let idx = match &*idx_val.data {
                    ValueData::Number(n) => n.0.to_usize().ok_or_else(|| HclError::PathError {
                        path: full_path.to_string(),
                        message: "Index must be a valid integer".to_string(),
                    })?,
                    _ => {
                        return Err(HclError::PathError {
                            path: full_path.to_string(),
                            message: "Index step must evaluate to a number".to_string(),
                        });
                    }
                };

                if let ValueData::Array(arr) = &*self.data {
                    if idx >= arr.len() {
                        return Err(HclError::PathError {
                            path: full_path.to_string(),
                            message: format!("Index {idx} out of bounds (len {})", arr.len()),
                        });
                    }
                    let (updated_child, removed_marks) =
                        arr[idx].unmark_path_steps(rest, full_path)?;
                    let mut new_arr = arr.clone();
                    new_arr[idx] = updated_child;
                    Ok((
                        Self {
                            ty: self.ty.clone(),
                            marks: self.marks.clone(),
                            data: Box::new(ValueData::Array(new_arr)),
                        },
                        removed_marks,
                    ))
                } else {
                    Err(HclError::PathError {
                        path: full_path.to_string(),
                        message: "Cannot index into non-array".to_string(),
                    })
                }
            }
            PathStep::Key(key_val) => {
                let key = match &*key_val.data {
                    ValueData::String(s) => s.as_str(),
                    _ => {
                        return Err(HclError::PathError {
                            path: full_path.to_string(),
                            message: "Key step must evaluate to a string".to_string(),
                        });
                    }
                };

                if let ValueData::Object(map) = &*self.data {
                    let child = map.get(key).ok_or_else(|| HclError::PathError {
                        path: full_path.to_string(),
                        message: format!("Key '{key}' not found"),
                    })?;
                    let (updated_child, removed_marks) =
                        child.unmark_path_steps(rest, full_path)?;
                    let mut new_map = map.clone();
                    new_map.insert(key.to_string(), updated_child);
                    Ok((
                        Self {
                            ty: self.ty.clone(),
                            marks: self.marks.clone(),
                            data: Box::new(ValueData::Object(new_map)),
                        },
                        removed_marks,
                    ))
                } else {
                    Err(HclError::PathError {
                        path: full_path.to_string(),
                        message: format!("Cannot lookup key '{key}' in non-object"),
                    })
                }
            }
        }
    }

    /// Returns `true` if any child or descendant element contains one or more marks.
    ///
    /// # Returns
    /// `true` if at least one nested descendant element has a non-empty mark set.
    #[must_use]
    pub fn has_marked_children(&self) -> bool {
        match &*self.data {
            ValueData::Array(arr) => arr
                .iter()
                .any(|elem| !elem.marks.is_empty() || elem.has_marked_children()),
            ValueData::Set(set) => set
                .iter()
                .any(|elem| !elem.marks.is_empty() || elem.has_marked_children()),
            ValueData::Object(map) => map
                .values()
                .any(|val| !val.marks.is_empty() || val.has_marked_children()),
            _ => false,
        }
    }

    /// Returns all paths within this value that are tagged with the specified [`ValueMark`].
    ///
    /// # Arguments
    /// * `mark` - The mark to search for.
    ///
    /// # Returns
    /// A vector of [`Path`] instances identifying every matching location.
    #[must_use]
    pub fn paths_with_mark(&self, mark: &ValueMark) -> Vec<Path> {
        let mut results = Vec::new();
        self.collect_paths_with_mark(mark, &Path::empty(), &mut results);
        results
    }

    fn collect_paths_with_mark(
        &self,
        mark: &ValueMark,
        current_path: &Path,
        results: &mut Vec<Path>,
    ) {
        if self.has_mark(mark) {
            results.push(current_path.clone());
        }

        match &*self.data {
            ValueData::Array(arr) => {
                for (idx, elem) in arr.iter().enumerate() {
                    let step = PathStep::Index(Self::new(
                        Type::Number,
                        ValueData::Number(crate::number::Number::from(idx as u64)),
                    ));
                    let next_path = current_path.with_step(step);
                    elem.collect_paths_with_mark(mark, &next_path, results);
                }
            }
            ValueData::Set(set) => {
                for elem in set {
                    let step = PathStep::Index(elem.clone());
                    let next_path = current_path.with_step(step);
                    elem.collect_paths_with_mark(mark, &next_path, results);
                }
            }
            ValueData::Object(map) => {
                for (k, v) in map {
                    let step = PathStep::GetAttr(k.clone());
                    let next_path = current_path.with_step(step);
                    v.collect_paths_with_mark(mark, &next_path, results);
                }
            }
            _ => {}
        }
    }

    /// Create an `Unknown` value of a specific type.
    #[must_use]
    pub fn unknown(ty: Type) -> Self {
        Self::new(ty, ValueData::Unknown(None))
    }

    /// Create an `Unknown` value of a specific type with type refinements.
    ///
    /// # Arguments
    /// * `ty` - The type of the unknown value.
    /// * `refinement` - The refinement constraints on the unknown value.
    #[must_use]
    pub fn unknown_refined(ty: Type, refinement: Refinement) -> Self {
        Self::new(ty, ValueData::Unknown(Some(Arc::new(refinement))))
    }

    /// Returns a reference to the type refinements on this value, if it is unknown and has refinements.
    #[must_use]
    pub fn refinement(&self) -> Option<&Refinement> {
        if let ValueData::Unknown(Some(ref r)) = *self.data {
            Some(r.as_ref())
        } else {
            None
        }
    }

    /// Returns an `Arc` reference to the type refinements on this value, if it is unknown and has refinements.
    #[must_use]
    pub fn refinement_arc(&self) -> Option<Arc<Refinement>> {
        if let ValueData::Unknown(ref r) = *self.data {
            r.clone()
        } else {
            None
        }
    }

    /// Create a `Null` value of a specific type.
    #[must_use]
    pub fn null(ty: Type) -> Self {
        Self::new(ty, ValueData::Null)
    }

    /// Returns the type of this value.
    #[must_use]
    pub fn ty(&self) -> &Type {
        &self.ty
    }

    /// Returns true if this value is unknown.
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        matches!(*self.data, ValueData::Unknown(_))
    }

    /// Returns true if this value or any nested child value is unknown.
    #[must_use]
    pub fn contains_unknown(&self) -> bool {
        if self.is_unknown() {
            return true;
        }
        match *self.data {
            ValueData::Array(ref elems) => elems.iter().any(Self::contains_unknown),
            ValueData::Set(ref elems) => elems.iter().any(Self::contains_unknown),
            ValueData::Object(ref map) => map.values().any(Self::contains_unknown),
            _ => false,
        }
    }

    /// Returns true if this value is null.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(*self.data, ValueData::Null)
    }

    /// Creates a new encapsulated [`Value`] holding an arbitrary Rust type.
    ///
    /// # Arguments
    /// * `name` - The capsule type name.
    /// * `val` - The value to encapsulate.
    #[must_use]
    pub fn capsule<T: 'static + Send + Sync>(name: &'static str, val: T) -> Self {
        let ty = Type::capsule::<T>(name);
        Self::new(ty, ValueData::Capsule(Arc::new(val)))
    }

    /// Creates a new encapsulated [`Value`] holding an arbitrary Rust type with custom operations.
    ///
    /// # Arguments
    /// * `name` - The capsule type name.
    /// * `ops` - Custom capsule operations.
    /// * `val` - The value to encapsulate.
    #[must_use]
    pub fn capsule_with_ops<T: 'static + Send + Sync>(
        name: &'static str,
        ops: Arc<crate::types::ty::CapsuleOps>,
        val: T,
    ) -> Self {
        let ty = Type::capsule_with_ops::<T>(name, ops);
        Self::new(ty, ValueData::Capsule(Arc::new(val)))
    }

    /// Creates a new encapsulated [`Value`] using the provided [`crate::types::ty::CapsuleOps`] operations table.
    ///
    /// # Arguments
    /// * `val` - The value to encapsulate.
    /// * `ops` - Custom capsule operations defining equality, hashing, and conversions.
    #[must_use]
    pub fn from_capsule<T: 'static + Send + Sync>(
        val: T,
        ops: Arc<crate::types::ty::CapsuleOps>,
    ) -> Self {
        let name = ops.type_name;
        let ty = Type::capsule_with_ops::<T>(name, ops);
        Self::new(ty, ValueData::Capsule(Arc::new(val)))
    }

    /// Returns a reference to the encapsulated dynamic value if this value is a capsule.
    #[must_use]
    pub fn as_capsule_any(&self) -> Option<&(dyn std::any::Any + Send + Sync)> {
        if let ValueData::Capsule(ref arc) = *self.data {
            Some(&**arc)
        } else {
            None
        }
    }

    /// Attempts to downcast a reference to the encapsulated value of type `T`.
    ///
    /// Returns `Some(&T)` if this value is a capsule containing `T`, or `None` otherwise.
    #[must_use]
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        let any = self.as_capsule_any()?;
        any.downcast_ref::<T>()
    }

    /// Attempts to downcast a reference to the encapsulated value of type `T`.
    ///
    /// # Errors
    /// Returns [`HclError::CapsuleDowncast`] if the value is not a capsule or if downcasting to `T` fails.
    pub fn as_capsule<T: 'static>(&self) -> Result<&T, HclError> {
        self.downcast::<T>()
    }

    /// Attempts to downcast a reference to the encapsulated value of type `T`,
    /// returning a typed [`HclError::CapsuleDowncast`] on failure.
    ///
    /// # Errors
    /// Returns [`HclError::CapsuleDowncast`] if the value is not a capsule or if downcasting to `T` fails.
    pub fn downcast<T: 'static>(&self) -> Result<&T, HclError> {
        let any = match self.as_capsule_any() {
            Some(a) => a,
            None => {
                return Err(HclError::CapsuleDowncast {
                    expected: std::any::type_name::<T>(),
                    actual: self.ty.to_string(),
                });
            }
        };

        if let Some(val) = any.downcast_ref::<T>() {
            Ok(val)
        } else {
            Err(HclError::CapsuleDowncast {
                expected: std::any::type_name::<T>(),
                actual: self.ty.to_string(),
            })
        }
    }

    /// Serializes this [`Value`] into canonical `MessagePack` binary format.
    ///
    /// # Errors
    /// Returns [`HclError::MsgPackEncode`] if the value cannot be serialized.
    pub fn to_msgpack(&self) -> Result<Vec<u8>, HclError> {
        crate::types::msgpack::encode_value(self)
    }

    /// Deserializes a [`Value`] from canonical `MessagePack` binary format using an expected [`Type`].
    ///
    /// # Arguments
    /// * `bytes` - The `MessagePack` bytes to decode.
    /// * `expected_type` - The target type schema to conform to.
    ///
    /// # Errors
    /// Returns [`HclError::MsgPackDecode`] if decoding fails or the payload is invalid.
    pub fn from_msgpack(bytes: &[u8], expected_type: &Type) -> Result<Self, HclError> {
        crate::types::msgpack::decode_value(bytes, expected_type)
    }

    /// Retrieves a nested value by following a [`crate::types::path::Path`].
    ///
    /// # Arguments
    /// * `path` - The path to follow.
    ///
    /// # Errors
    /// Returns [`HclError::PathError`] if an attribute or index along the path cannot be resolved.
    pub fn get_path(&self, path: &crate::types::path::Path) -> Result<Value, HclError> {
        let mut curr = self.clone();
        for step in path.steps() {
            match step {
                crate::types::path::PathStep::GetAttr(attr) => match &*curr.data {
                    ValueData::Object(map) => {
                        if let Some(val) = map.get(attr) {
                            curr = val.clone();
                        } else {
                            return Err(HclError::PathError {
                                path: path.to_string(),
                                message: format!("Attribute '{attr}' not found"),
                            });
                        }
                    }
                    _ => {
                        return Err(HclError::PathError {
                            path: path.to_string(),
                            message: format!(
                                "Cannot access attribute '{attr}' on type {}",
                                curr.ty()
                            ),
                        });
                    }
                },
                crate::types::path::PathStep::Index(idx_val) => {
                    let idx_num = match &*idx_val.data {
                        ValueData::Number(n) => n.as_i64().and_then(|i| usize::try_from(i).ok()),
                        _ => None,
                    };
                    let Some(idx) = idx_num else {
                        return Err(HclError::PathError {
                            path: path.to_string(),
                            message: format!("Index must be a non-negative integer, got {idx_val}"),
                        });
                    };
                    match &*curr.data {
                        ValueData::Array(arr) => {
                            if let Some(val) = arr.get(idx) {
                                curr = val.clone();
                            } else {
                                return Err(HclError::PathError {
                                    path: path.to_string(),
                                    message: format!(
                                        "Index {idx} out of bounds for array of length {}",
                                        arr.len()
                                    ),
                                });
                            }
                        }
                        _ => {
                            return Err(HclError::PathError {
                                path: path.to_string(),
                                message: format!("Cannot index non-array type {}", curr.ty()),
                            });
                        }
                    }
                }
                crate::types::path::PathStep::Key(key_val) => {
                    let key_str = match &*key_val.data {
                        ValueData::String(s) => s.clone(),
                        _ => key_val.to_string(),
                    };
                    match &*curr.data {
                        ValueData::Object(map) => {
                            if let Some(val) = map.get(&key_str) {
                                curr = val.clone();
                            } else {
                                return Err(HclError::PathError {
                                    path: path.to_string(),
                                    message: format!("Key '{key_str}' not found in object"),
                                });
                            }
                        }
                        _ => {
                            return Err(HclError::PathError {
                                path: path.to_string(),
                                message: format!(
                                    "Cannot index key '{key_str}' on type {}",
                                    curr.ty()
                                ),
                            });
                        }
                    }
                }
            }
        }
        Ok(curr)
    }

    /// Recursively transforms this value using a path-aware mapping closure.
    ///
    /// # Arguments
    /// * `f` - A closure taking the current path and value, and returning a transformed value or error.
    ///
    /// # Errors
    /// Returns [`HclError`] if the transformation closure fails.
    pub fn transform<F>(&self, f: &mut F) -> Result<Value, HclError>
    where
        F: FnMut(&crate::types::path::Path, &Value) -> Result<Value, HclError>,
    {
        let mut path = crate::types::path::Path::empty();
        self.transform_internal(&mut path, f)
    }

    fn transform_internal<F>(
        &self,
        path: &mut crate::types::path::Path,
        f: &mut F,
    ) -> Result<Value, HclError>
    where
        F: FnMut(&crate::types::path::Path, &Value) -> Result<Value, HclError>,
    {
        match &*self.data {
            ValueData::Array(arr) => {
                let mut new_arr = Vec::with_capacity(arr.len());
                for (idx, elem) in arr.iter().enumerate() {
                    let idx_val = Value::new(
                        Type::Number,
                        ValueData::Number(crate::number::Number::from(idx as i64)),
                    );
                    path.push(crate::types::path::PathStep::Index(idx_val));
                    let transformed_elem = elem.transform_internal(path, f)?;
                    path.pop();
                    new_arr.push(transformed_elem);
                }
                let intermediate = Value::new(self.ty().clone(), ValueData::Array(new_arr));
                f(path, &intermediate)
            }
            ValueData::Object(map) => {
                let mut new_map = BTreeMap::new();
                for (k, v) in map {
                    path.push(crate::types::path::PathStep::GetAttr(k.clone()));
                    let transformed_v = v.transform_internal(path, f)?;
                    path.pop();
                    new_map.insert(k.clone(), transformed_v);
                }
                let intermediate = Value::new(self.ty().clone(), ValueData::Object(new_map));
                f(path, &intermediate)
            }
            ValueData::Set(set) => {
                let mut new_set = std::collections::BTreeSet::new();
                for elem in set {
                    let transformed_elem = elem.transform_internal(path, f)?;
                    new_set.insert(transformed_elem);
                }
                let intermediate = Value::new(self.ty().clone(), ValueData::Set(new_set));
                f(path, &intermediate)
            }
            _ => f(path, self),
        }
    }

    /// Recursively walks this value using a path-aware visitor closure.
    ///
    /// # Arguments
    /// * `f` - A closure taking the current path and value, returning `Ok(true)` to continue
    ///   descending or `Ok(false)` to prune the subtree.
    ///
    /// # Errors
    /// Returns [`HclError`] if the visitor closure fails.
    pub fn walk<F>(&self, f: &mut F) -> Result<(), HclError>
    where
        F: FnMut(&crate::types::path::Path, &Value) -> Result<bool, HclError>,
    {
        let mut path = crate::types::path::Path::empty();
        self.walk_internal(&mut path, f)
    }

    fn walk_internal<F>(
        &self,
        path: &mut crate::types::path::Path,
        f: &mut F,
    ) -> Result<(), HclError>
    where
        F: FnMut(&crate::types::path::Path, &Value) -> Result<bool, HclError>,
    {
        if !f(path, self)? {
            return Ok(());
        }
        match &*self.data {
            ValueData::Array(arr) => {
                for (idx, elem) in arr.iter().enumerate() {
                    let idx_val = Value::new(
                        Type::Number,
                        ValueData::Number(crate::number::Number::from(idx as i64)),
                    );
                    path.push(crate::types::path::PathStep::Index(idx_val));
                    elem.walk_internal(path, f)?;
                    path.pop();
                }
            }
            ValueData::Object(map) => {
                for (k, v) in map {
                    path.push(crate::types::path::PathStep::GetAttr(k.clone()));
                    v.walk_internal(path, f)?;
                    path.pop();
                }
            }
            ValueData::Set(set) => {
                for elem in set {
                    elem.walk_internal(path, f)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Attempts to coerce this value to the given target type.
    /// Returns an error message on failure.
    pub fn coerce(&self, target: &Type) -> Result<Self, String> {
        if &self.ty == target || self.ty.is_dynamic() || target.is_dynamic() {
            return Ok(self.clone());
        }

        if let Some(ops) = self.ty.capsule_ops()
            && let ValueData::Capsule(a) = &*self.data
            && let Some(ref conv) = ops.conversion_to
            && let Some(val) = (conv)(&**a, target)
        {
            return Ok(val.with_marks(self.marks.clone()));
        }

        if let Some(ops) = target.capsule_ops()
            && let Some(ref conv) = ops.conversion_from
            && let Some(arc) = (conv)(self)
        {
            return Ok(
                Self::new(target.clone(), ValueData::Capsule(arc)).with_marks(self.marks.clone())
            );
        }

        if self.ty.is_capsule() || target.is_capsule() {
            return Err(format!("cannot coerce {} to {}", self.ty, target));
        }

        if self.is_unknown() {
            if let Some(r) = self.refinement() {
                crate::types::unify::validate_unknown_refinement(target, r)?;
                if self.ty == Type::String && target == &Type::Number {
                    if let Some(ref p) = r.string_prefix {
                        if !p.is_empty() {
                            let first_char = p.chars().next().unwrap_or(' ');
                            if !first_char.is_ascii_digit()
                                && first_char != '-'
                                && first_char != '+'
                                && first_char != '.'
                            {
                                return Err(format!(
                                    "cannot coerce string with prefix {p:?} to number"
                                ));
                            }
                        }
                    }
                }
                let mut new_r = r.clone();
                if target == &Type::Number {
                    new_r.string_length_min = None;
                    new_r.string_length_max = None;
                    new_r.string_prefix = None;
                    new_r.string_suffix = None;
                } else if target == &Type::String {
                    new_r.number_min = None;
                    new_r.number_max = None;
                }
                return Ok(
                    Self::unknown_refined(target.clone(), new_r).with_marks(self.marks.clone())
                );
            }
            return Ok(Self::unknown(target.clone()).with_marks(self.marks.clone()));
        }
        if self.is_null() {
            return Ok(Self::null(target.clone()).with_marks(self.marks.clone()));
        }

        let mut coerced = match (&self.ty, target) {
            (Type::String, Type::Number) => {
                if let ValueData::String(s) = &*self.data {
                    use std::str::FromStr;
                    let num = crate::number::Number::from_str(s)
                        .map_err(|_| format!("cannot parse '{}' as number", s))?;
                    Ok(Self::new(Type::Number, ValueData::Number(num)))
                } else {
                    Err("Mismatched type and data for string".to_string())
                }
            }
            (Type::Number, Type::String) => {
                if let ValueData::Number(n) = &*self.data {
                    Ok(Self::new(Type::String, ValueData::String(n.0.to_string())))
                } else {
                    Err("Mismatched type and data for number".to_string())
                }
            }
            (Type::Bool, Type::String) => {
                if let ValueData::Bool(b) = &*self.data {
                    Ok(Self::new(
                        Type::String,
                        ValueData::String(if *b {
                            "true".to_string()
                        } else {
                            "false".to_string()
                        }),
                    ))
                } else {
                    Err("Mismatched type and data for bool".to_string())
                }
            }
            (Type::String, Type::Bool) => {
                if let ValueData::String(s) = &*self.data {
                    match s.as_str() {
                        "true" => Ok(Self::new(Type::Bool, ValueData::Bool(true))),
                        "false" => Ok(Self::new(Type::Bool, ValueData::Bool(false))),
                        _ => Err(format!("cannot parse '{}' as bool", s)),
                    }
                } else {
                    Err("Mismatched type and data for string".to_string())
                }
            }
            // Add tuple/list coercion here if needed
            _ => Err(format!("cannot coerce {} to {}", self.ty, target)),
        }?;
        coerced.marks.extend(self.marks.clone());
        Ok(coerced)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::BigDecimal;
    use std::str::FromStr;

    #[test]
    fn test_value_creation() {
        let v_unk = Value::unknown(Type::String);
        assert!(v_unk.is_unknown());
        assert!(!v_unk.is_null());
        assert_eq!(v_unk.ty(), &Type::String);

        let v_null = Value::null(Type::Number);
        assert!(!v_null.is_unknown());
        assert!(v_null.is_null());
        assert_eq!(v_null.ty(), &Type::Number);

        let v_bool = Value::new(Type::Bool, ValueData::Bool(true));
        assert_eq!(v_bool.ty(), &Type::Bool);
    }

    #[test]
    fn test_value_eq() {
        let v1 = Value::new(Type::Bool, ValueData::Bool(true));
        let v2 = Value::new(Type::Bool, ValueData::Bool(true));
        let v3 = Value::new(Type::Bool, ValueData::Bool(false));

        assert_eq!(v1, v2);
        assert_ne!(v1, v3);

        let v_unk1 = Value::unknown(Type::String);
        let v_unk2 = Value::unknown(Type::String);
        assert_eq!(v_unk1, v_unk2);

        let n1 = Value::new(
            Type::Number,
            ValueData::Number(Number::new(
                BigDecimal::from_str("1.0").expect("expected value"),
            )),
        );
        let n2 = Value::new(
            Type::Number,
            ValueData::Number(Number::new(
                BigDecimal::from_str("1.0").expect("expected value"),
            )),
        );
        let n3 = Value::new(
            Type::Number,
            ValueData::Number(Number::new(
                BigDecimal::from_str("2.0").expect("expected value"),
            )),
        );
        assert_eq!(n1, n2);
        assert_ne!(n1, n3);

        let s1 = Value::new(Type::String, ValueData::String("a".to_string()));
        let s2 = Value::new(Type::String, ValueData::String("a".to_string()));
        let s3 = Value::new(Type::String, ValueData::String("b".to_string()));
        assert_eq!(s1, s2);
        assert_ne!(s1, s3);

        let arr1 = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(vec![s1.clone()]),
        );
        let arr2 = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(vec![s2.clone()]),
        );
        let arr3 = Value::new(Type::List(Box::new(Type::String)), ValueData::Array(vec![]));
        assert_eq!(arr1, arr2);
        assert_ne!(arr1, arr3);

        let mut set_data = BTreeSet::new();
        set_data.insert(s1.clone());
        let set1 = Value::new(
            Type::Set(Box::new(Type::String)),
            ValueData::Set(set_data.clone()),
        );
        let set2 = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_data));
        assert_eq!(set1, set2);

        let mut map_data = BTreeMap::new();
        map_data.insert("k".to_string(), s1.clone());
        let map1 = Value::new(
            Type::Map(Box::new(Type::String)),
            ValueData::Object(map_data.clone()),
        );
        let map2 = Value::new(
            Type::Map(Box::new(Type::String)),
            ValueData::Object(map_data),
        );
        assert_eq!(map1, map2);

        assert_ne!(v1, v_unk1);

        assert_ne!(
            Value::new(Type::Bool, ValueData::Bool(true)),
            Value::new(Type::Bool, ValueData::Null)
        );

        // Test ValueData::eq directly for unmatched variants
        assert_ne!(ValueData::Bool(true), ValueData::Null);

        // Test derived Debug for Value
        let v_debug = format!("{:?}", v1);
        assert!(v_debug.contains("Bool"));
    }

    #[test]
    fn test_value_hash() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;

        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();

        let v1 = Value::new(Type::Bool, ValueData::Bool(true));
        v1.hash(&mut h1);
        v1.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());

        let mut h3 = DefaultHasher::new();
        let v2 = Value::unknown(Type::String);
        v2.hash(&mut h3);
        assert_ne!(h1.finish(), h3.finish());

        let mut h4 = DefaultHasher::new();
        let v3 = Value::null(Type::String);
        v3.hash(&mut h4);

        let mut h5 = DefaultHasher::new();
        let v4 = Value::new(Type::String, ValueData::String("test".to_string()));
        v4.hash(&mut h5);

        let mut h6 = DefaultHasher::new();
        let v5 = Value::new(
            Type::Number,
            ValueData::Number(Number::new(
                BigDecimal::from_str("1.0").expect("expected value"),
            )),
        );
        v5.hash(&mut h6);

        let mut h7 = DefaultHasher::new();
        let v6 = Value::new(Type::List(Box::new(Type::String)), ValueData::Array(vec![]));
        v6.hash(&mut h7);

        let mut h8 = DefaultHasher::new();
        let v7 = Value::new(
            Type::Set(Box::new(Type::String)),
            ValueData::Set(BTreeSet::new()),
        );
        v7.hash(&mut h8);

        let mut h9 = DefaultHasher::new();
        let v8 = Value::new(
            Type::Map(Box::new(Type::String)),
            ValueData::Object(BTreeMap::new()),
        );
        v8.hash(&mut h9);
    }

    #[test]
    fn test_value_ord() {
        let v1 = Value::new(Type::Bool, ValueData::Bool(true));
        let v2 = Value::new(Type::Bool, ValueData::Bool(false));

        assert_eq!(v1.partial_cmp(&v2), None);
        assert_eq!(v1.cmp(&v2), std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_value_coerce() {
        let val_unk = Value::unknown(Type::String);
        let coerced_unk = val_unk.coerce(&Type::Number).expect("expected value");
        assert!(coerced_unk.is_unknown());
        assert_eq!(coerced_unk.ty(), &Type::Number);

        let val_null = Value::null(Type::String);
        let coerced_null = val_null.coerce(&Type::Number).expect("expected value");
        assert!(coerced_null.is_null());
        assert_eq!(coerced_null.ty(), &Type::Number);

        let val_str = Value::new(Type::String, ValueData::String("1".to_string()));
        let coerced_num = val_str.coerce(&Type::Number).expect("expected value");
        assert_eq!(coerced_num.ty(), &Type::Number);

        let val_num = Value::new(
            Type::Number,
            ValueData::Number(Number::from_str("1").expect("expected value")),
        );
        let coerced_str = val_num.coerce(&Type::String).expect("expected value");
        assert_eq!(coerced_str.ty(), &Type::String);

        let val_bool_true = Value::new(Type::Bool, ValueData::Bool(true));
        let coerced_str_true = val_bool_true.coerce(&Type::String).expect("expected value");
        assert_eq!(
            coerced_str_true.data.as_ref(),
            &ValueData::String("true".to_string())
        );

        let val_bool_false = Value::new(Type::Bool, ValueData::Bool(false));
        let coerced_str_false = val_bool_false
            .coerce(&Type::String)
            .expect("expected value");
        assert_eq!(
            coerced_str_false.data.as_ref(),
            &ValueData::String("false".to_string())
        );

        let val_str_true = Value::new(Type::String, ValueData::String("true".to_string()));
        let coerced_bool_true = val_str_true.coerce(&Type::Bool).expect("expected value");
        assert_eq!(coerced_bool_true.data.as_ref(), &ValueData::Bool(true));

        let val_str_false = Value::new(Type::String, ValueData::String("false".to_string()));
        let coerced_bool_false = val_str_false.coerce(&Type::Bool).expect("expected value");
        assert_eq!(coerced_bool_false.data.as_ref(), &ValueData::Bool(false));

        let val_str_bad = Value::new(Type::String, ValueData::String("bad".to_string()));
        assert!(val_str_bad.coerce(&Type::Bool).is_err());
        assert!(val_str_bad.coerce(&Type::Number).is_err());

        // Mismatched type and data tests (crafting an invalid value to test the internal error paths)
        let invalid_str = Value::new(Type::String, ValueData::Bool(true));
        assert!(invalid_str.coerce(&Type::Number).is_err());
        assert!(invalid_str.coerce(&Type::Bool).is_err());

        let invalid_num = Value::new(Type::Number, ValueData::Bool(true));
        assert!(invalid_num.coerce(&Type::String).is_err());

        let invalid_bool = Value::new(Type::Bool, ValueData::String("true".to_string()));
        assert!(invalid_bool.coerce(&Type::String).is_err());

        // Dynamic target and self type coercion
        let coerced_dyn = val_num.coerce(&Type::Dynamic).expect("expected value");
        assert_eq!(coerced_dyn, val_num);

        let val_dyn = Value::new(Type::Dynamic, ValueData::Null);
        let coerced_from_dyn = val_dyn.coerce(&Type::Number).expect("expected value");
        assert_eq!(coerced_from_dyn, val_dyn);

        // Unsupported coercion fallback (e.g. Number to Bool)
        assert!(val_num.coerce(&Type::Bool).is_err());
    }

    #[derive(Debug, PartialEq, Eq)]
    struct CustomResource {
        id: String,
        count: usize,
    }

    #[test]
    fn test_capsule_value_operations() {
        let res = CustomResource {
            id: "res-123".to_string(),
            count: 42,
        };
        let cap_val = Value::capsule("custom_resource", res);

        assert!(cap_val.ty().is_capsule());
        assert_eq!(cap_val.ty().to_string(), "capsule(custom_resource)");

        // Successful downcast_ref
        let downcasted_ref = cap_val.downcast_ref::<CustomResource>();
        assert_eq!(
            downcasted_ref,
            Some(&CustomResource {
                id: "res-123".to_string(),
                count: 42,
            })
        );

        // Successful downcast
        let downcasted = cap_val.downcast::<CustomResource>();
        assert!(downcasted.is_ok());
        assert_eq!(downcasted.expect("ok").id, "res-123");

        // Wrong target type downcast (using another capsule type)
        let cap_u32 = Value::capsule("other", 100_u32);
        assert_eq!(cap_u32.downcast_ref::<CustomResource>(), None);
        let err = cap_u32
            .downcast::<CustomResource>()
            .expect_err("should fail");
        assert!(err.to_string().contains("Capsule downcast error"));

        // Non-capsule downcast
        let non_cap = Value::new(Type::String, ValueData::String("test".into()));
        assert_eq!(non_cap.downcast_ref::<CustomResource>(), None);
        let non_cap_err = non_cap
            .downcast::<CustomResource>()
            .expect_err("should fail");
        assert!(non_cap_err.to_string().contains("Capsule downcast error"));
        assert!(non_cap.as_capsule_any().is_none());

        // Equality (same Arc vs different Arc)
        let cap_clone = cap_val.clone();
        assert_eq!(cap_val, cap_clone);

        let cap_other = Value::capsule(
            "custom_resource",
            CustomResource {
                id: "res-123".to_string(),
                count: 42,
            },
        );
        assert_ne!(cap_val, cap_other);

        // Debug formatting
        assert!(format!("{:?}", ValueData::Null).contains("Null"));
        assert!(format!("{:?}", ValueData::Unknown(None)).contains("Unknown"));
        let r_debug = Refinement::not_null().with_prefix("pre");
        assert!(format!("{:?}", ValueData::Unknown(Some(Arc::new(r_debug)))).contains("Unknown"));
        assert!(format!("{:?}", ValueData::Bool(true)).contains("Bool"));
        assert!(format!("{:?}", ValueData::Number(10.into())).contains("Number"));
        assert!(format!("{:?}", ValueData::String("s".into())).contains("String"));
        assert!(format!("{:?}", ValueData::Array(vec![])).contains("Array"));
        assert!(format!("{:?}", ValueData::Set(BTreeSet::new())).contains("Set"));
        assert!(format!("{:?}", ValueData::Object(BTreeMap::new())).contains("Object"));

        let debug_str = format!("{:?}", cap_val.data);
        assert!(debug_str.starts_with("Capsule("));

        // Hashing
        use std::collections::hash_map::DefaultHasher;
        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();
        cap_val.hash(&mut h1);
        cap_clone.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());

        // Coerce
        assert_eq!(
            cap_val.coerce(cap_val.ty()).expect("expected value"),
            cap_val
        );
        assert!(cap_val.coerce(&Type::String).is_err());
        assert!(
            Value::new(Type::String, ValueData::String("x".into()))
                .coerce(cap_val.ty())
                .is_err()
        );

        // Native function interoperability
        let func = crate::eval::func::Function {
            name: "bump_count".to_string(),
            func: Arc::new(|args| {
                let resource = args[0]
                    .downcast::<CustomResource>()
                    .map_err(|e| e.to_string())?;
                let updated = CustomResource {
                    id: resource.id.clone(),
                    count: resource.count + 1,
                };
                Ok(Value::capsule("custom_resource", updated))
            }),
            signature: None,
        };

        let result = (func.func)(&[cap_val]).expect("function succeeded");
        assert_eq!(
            result.downcast_ref::<CustomResource>(),
            Some(&CustomResource {
                id: "res-123".to_string(),
                count: 43,
            })
        );
        assert!((func.func)(&[non_cap]).is_err());
    }

    #[test]
    fn test_value_marks_comprehensive() {
        // ValueMark creation & display
        let sens = ValueMark::Sensitive;
        assert_eq!(sens.to_string(), "sensitive");
        let custom = ValueMark::custom("origin:env");
        assert_eq!(custom.to_string(), "origin:env");
        assert_ne!(sens, custom);

        // Value new_with_marks & with_marks
        let mut initial_marks = BTreeSet::new();
        initial_marks.insert(sens.clone());
        let val = Value::new_with_marks(
            Type::String,
            ValueData::String("secret".into()),
            initial_marks.clone(),
        );
        assert_eq!(val.marks(), &initial_marks);
        assert!(val.has_mark(&ValueMark::Sensitive));
        assert!(!val.has_mark(&custom));
        assert!(val.is_sensitive());

        // Display with sensitive mark
        assert_eq!(val.to_string(), "(sensitive value)");

        // Add custom mark
        let marked = val.mark(custom.clone());
        assert!(marked.has_mark(&custom));
        assert!(marked.has_mark(&ValueMark::Sensitive));
        assert_eq!(marked.marks().len(), 2);

        // Unmark
        let (unmarked, removed) = marked.unmark();
        assert_eq!(unmarked.marks().len(), 0);
        assert_eq!(removed.len(), 2);
        assert!(!unmarked.is_sensitive());
        assert_eq!(unmarked.to_string(), "\"secret\"");

        // with_marks
        let reset = unmarked.with_marks(initial_marks);
        assert!(reset.is_sensitive());

        // unmark_deep on nested structures (Array, Set, Object)
        let s1 = Value::new(Type::String, ValueData::String("public".into()));
        let s2 =
            Value::new(Type::String, ValueData::String("hidden".into())).mark(ValueMark::Sensitive);
        let s3 = Value::new(Type::String, ValueData::String("tagged".into())).mark(custom.clone());

        // Array
        let arr_val = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(vec![s1.clone(), s2.clone()]),
        );
        assert!(arr_val.is_sensitive());
        let (unmarked_arr, arr_marks) = arr_val.unmark_deep();
        assert!(arr_marks.contains(&ValueMark::Sensitive));
        assert!(!unmarked_arr.is_sensitive());

        // Set
        let mut set_data = BTreeSet::new();
        set_data.insert(s1.clone());
        set_data.insert(s2.clone());
        let set_val = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_data));
        assert!(set_val.is_sensitive());
        let (unmarked_set, set_marks) = set_val.unmark_deep();
        assert!(set_marks.contains(&ValueMark::Sensitive));
        assert!(!unmarked_set.is_sensitive());

        // Object
        let mut obj_data = BTreeMap::new();
        obj_data.insert("a".to_string(), s1.clone());
        obj_data.insert("b".to_string(), s2.clone());
        obj_data.insert("c".to_string(), s3.clone());
        let obj_val = Value::new(Type::Dynamic, ValueData::Object(obj_data));
        assert!(obj_val.is_sensitive());
        let (unmarked_obj, obj_marks) = obj_val.unmark_deep();
        assert!(obj_marks.contains(&ValueMark::Sensitive));
        assert!(obj_marks.contains(&custom));
        assert!(!unmarked_obj.is_sensitive());

        // unmark_sensitive
        let obj_no_sens = obj_val.unmark_sensitive();
        assert!(!obj_no_sens.is_sensitive());
        assert_eq!(obj_no_sens.paths_with_mark(&custom).len(), 1);

        // unmark_deep on primitive
        let (prim_unmark, prim_marks) = s2.unmark_deep();
        assert_eq!(prim_marks.len(), 1);
        assert_eq!(prim_unmark.marks().len(), 0);

        // unmark_sensitive on array and set
        let arr_no_sens = arr_val.unmark_sensitive();
        assert!(!arr_no_sens.is_sensitive());
        let set_no_sens = set_val.unmark_sensitive();
        assert!(!set_no_sens.is_sensitive());

        // Display for other variants
        assert_eq!(Value::null(Type::Dynamic).to_string(), "null");
        assert_eq!(
            Value::unknown(Type::Dynamic).to_string(),
            "(known after apply)"
        );
        assert_eq!(
            Value::new(Type::Bool, ValueData::Bool(true)).to_string(),
            "true"
        );
        assert_eq!(
            Value::new(Type::Number, ValueData::Number(42_i32.into())).to_string(),
            "42"
        );
        assert_eq!(s1.to_string(), "\"public\"");
        assert_eq!(unmarked_arr.to_string(), "[\"public\", \"hidden\"]");
        let rendered_obj = unmarked_obj.to_string();
        assert!(rendered_obj.contains("\"a\" = \"public\""));
        let cap = Value::capsule("res", 123_u32);
        assert!(cap.to_string().starts_with("capsule("));

        // Coerce preserves marks
        let sens_str =
            Value::new(Type::String, ValueData::String("42".into())).mark(ValueMark::Sensitive);
        let coerced_num = sens_str.coerce(&Type::Number).expect("coerce succeeds");
        assert!(coerced_num.has_mark(&ValueMark::Sensitive));

        let unk_sens = Value::unknown(Type::String).mark(ValueMark::Sensitive);
        assert!(
            unk_sens
                .coerce(&Type::Number)
                .expect("ok")
                .has_mark(&ValueMark::Sensitive)
        );

        let null_sens = Value::null(Type::String).mark(ValueMark::Sensitive);
        assert!(
            null_sens
                .coerce(&Type::Number)
                .expect("ok")
                .has_mark(&ValueMark::Sensitive)
        );
    }

    #[test]
    fn test_value_set_display_and_refinement_arc() {
        // Set Display formatting: empty and multi-element
        let empty_set = Value::new(
            Type::Set(Box::new(Type::Number)),
            ValueData::Set(BTreeSet::new()),
        );
        assert_eq!(empty_set.to_string(), "[]");

        let multi_set = Value::new(
            Type::Set(Box::new(Type::Number)),
            ValueData::Set(BTreeSet::from([
                Value::new(Type::Number, ValueData::Number(1_i32.into())),
                Value::new(Type::Number, ValueData::Number(2_i32.into())),
            ])),
        );
        assert_eq!(multi_set.to_string(), "[1, 2]");

        // refinement_arc on non-unknown value
        let non_unk = Value::new(Type::Bool, ValueData::Bool(true));
        assert!(non_unk.refinement_arc().is_none());
    }

    #[test]
    fn test_value_unknown_refined_coercion() {
        use crate::types::refinement::Refinement;

        // String to Number with valid prefix digit
        let ref_digit = Refinement::new()
            .with_prefix("100")
            .with_string_length(3, 10)
            .expect("ok");
        let unk_str_digit = Value::unknown_refined(Type::String, ref_digit);
        let coerced_num = unk_str_digit.coerce(&Type::Number).expect("ok");
        assert_eq!(coerced_num.ty, Type::Number);
        let ref_after = coerced_num.refinement().expect("has refinement");
        assert!(ref_after.string_prefix.is_none());
        assert!(ref_after.string_length_min.is_none());

        // String to Number with minus, plus, dot, and empty prefixes
        for prefix in ["-42", "+42", ".5", ""] {
            let r = Refinement::new().with_prefix(prefix);
            let unk_str = Value::unknown_refined(Type::String, r);
            assert!(unk_str.coerce(&Type::Number).is_ok());
        }

        // String to Number with prefix == None
        let ref_no_prefix = Refinement::new().with_string_length(1, 10).expect("ok");
        let unk_str_no_prefix = Value::unknown_refined(Type::String, ref_no_prefix);
        assert!(unk_str_no_prefix.coerce(&Type::Number).is_ok());

        // String to Number with invalid prefix -> Err
        let ref_alpha = Refinement::new().with_prefix("abc");
        let unk_str_alpha = Value::unknown_refined(Type::String, ref_alpha);
        assert!(unk_str_alpha.coerce(&Type::Number).is_err());

        // Number to String clears number_min/max
        let ref_num = Refinement::new()
            .with_number_range(
                crate::number::Number::from(1),
                crate::number::Number::from(50),
            )
            .expect("ok");
        let unk_num = Value::unknown_refined(Type::Number, ref_num);
        let coerced_str = unk_num.coerce(&Type::String).expect("ok");
        assert_eq!(coerced_str.ty, Type::String);
        let ref_str_after = coerced_str.refinement().expect("has refinement");
        assert!(ref_str_after.number_min.is_none());
        assert!(ref_str_after.number_max.is_none());

        // Coerce to another type (neither Number nor String)
        let ref_not_null = Refinement::not_null();
        let unk_val_other = Value::unknown_refined(Type::Set(Box::new(Type::String)), ref_not_null);
        let target_list = Type::List(Box::new(Type::String));
        let coerced_other = unk_val_other.coerce(&target_list).expect("ok");
        assert_eq!(coerced_other.ty, target_list);

        // Unknown refinement validation error in coerce
        let mut src_schema = BTreeMap::new();
        src_schema.insert("foo".to_string(), Type::String);
        let src_ty = Type::object(src_schema);

        let mut dst_schema = BTreeMap::new();
        dst_schema.insert("bar".to_string(), Type::String);
        let dst_ty = Type::object(dst_schema);

        let unk_bad_ref = Value::unknown_refined(
            src_ty,
            Refinement::new().with_object_attr("not_allowed", Refinement::not_null()),
        );
        assert!(unk_bad_ref.coerce(&dst_ty).is_err());
    }

    struct CountdownWriter(usize);
    impl std::fmt::Write for CountdownWriter {
        fn write_str(&mut self, _s: &str) -> std::fmt::Result {
            if self.0 == 0 {
                Err(std::fmt::Error)
            } else {
                self.0 -= 1;
                Ok(())
            }
        }
    }

    #[test]
    fn test_value_display_error_paths() {
        use std::fmt::Write;

        let arr = Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(vec![
                Value::new(Type::Number, ValueData::Number(1_i32.into())),
                Value::new(Type::Number, ValueData::Number(2_i32.into())),
            ]),
        );
        for count in 0..6 {
            let mut writer = CountdownWriter(count);
            let _ = write!(writer, "{arr}");
        }

        let set = Value::new(
            Type::Set(Box::new(Type::Number)),
            ValueData::Set(BTreeSet::from([
                Value::new(Type::Number, ValueData::Number(1_i32.into())),
                Value::new(Type::Number, ValueData::Number(2_i32.into())),
            ])),
        );
        for count in 0..6 {
            let mut writer = CountdownWriter(count);
            let _ = write!(writer, "{set}");
        }

        let mut map = BTreeMap::new();
        map.insert(
            "k1".to_string(),
            Value::new(Type::Number, ValueData::Number(1_i32.into())),
        );
        map.insert(
            "k2".to_string(),
            Value::new(Type::Number, ValueData::Number(2_i32.into())),
        );
        let obj = Value::new(Type::object(BTreeMap::new()), ValueData::Object(map));
        for count in 0..8 {
            let mut writer = CountdownWriter(count);
            let _ = write!(writer, "{obj}");
        }
    }

    #[test]
    fn test_value_path_traversal_and_transform() {
        use crate::encode::EncodeValue;
        use crate::types::path::{Path, PathStep};

        // Construct nested data: { cluster = { servers = [ { host = "node-1" }, { host = "node-2" } ] } }
        let mut node1 = BTreeMap::new();
        node1.insert("host".to_string(), "node-1".encode_value());

        let mut node2 = BTreeMap::new();
        node2.insert("host".to_string(), "node-2".encode_value());

        let servers = Value::new(
            Type::List(Box::new(Type::Dynamic)),
            ValueData::Array(vec![
                Value::new(Type::Dynamic, ValueData::Object(node1)),
                Value::new(Type::Dynamic, ValueData::Object(node2)),
            ]),
        );

        let mut cluster = BTreeMap::new();
        cluster.insert("servers".to_string(), servers);

        let mut root_map = BTreeMap::new();
        root_map.insert(
            "cluster".to_string(),
            Value::new(Type::Dynamic, ValueData::Object(cluster)),
        );
        let root = Value::new(Type::Dynamic, ValueData::Object(root_map));

        // 1. get_path success
        let path = Path::empty()
            .with_step(PathStep::GetAttr("cluster".to_string()))
            .with_step(PathStep::GetAttr("servers".to_string()))
            .with_step(PathStep::Index(1_i64.encode_value()))
            .with_step(PathStep::Key("host".encode_value()));

        let val = root.get_path(&path).expect("resolved path");
        assert_eq!(val.to_string(), "\"node-2\"");

        // 2. get_path error branches
        let bad_attr_path = Path::empty().with_step(PathStep::GetAttr("nonexistent".to_string()));
        assert!(root.get_path(&bad_attr_path).is_err());

        let bad_type_attr_path = Path::empty()
            .with_step(PathStep::GetAttr("cluster".to_string()))
            .with_step(PathStep::GetAttr("servers".to_string()))
            .with_step(PathStep::GetAttr("invalid_on_array".to_string()));
        assert!(root.get_path(&bad_type_attr_path).is_err());

        let out_of_bounds_path = Path::empty()
            .with_step(PathStep::GetAttr("cluster".to_string()))
            .with_step(PathStep::GetAttr("servers".to_string()))
            .with_step(PathStep::Index(99_i64.encode_value()));
        assert!(root.get_path(&out_of_bounds_path).is_err());

        let bad_index_val_path = Path::empty()
            .with_step(PathStep::GetAttr("cluster".to_string()))
            .with_step(PathStep::GetAttr("servers".to_string()))
            .with_step(PathStep::Index("not_a_num".encode_value()));
        assert!(root.get_path(&bad_index_val_path).is_err());

        // 3. walk
        let mut visited_paths = Vec::new();
        root.walk(&mut |p, _val| {
            visited_paths.push(p.to_string());
            Ok(true)
        })
        .expect("walk succeeds");
        assert!(
            visited_paths
                .iter()
                .any(|p| p.contains("cluster.servers[0].host"))
        );

        // 4. transform
        let transformed = root
            .transform(&mut |p, val| {
                if p.to_string().ends_with(".host") {
                    Ok("REDACTED".encode_value())
                } else {
                    Ok(val.clone())
                }
            })
            .expect("transform succeeds");

        let redacted_val = transformed.get_path(&path).expect("redacted host");
        assert_eq!(redacted_val.to_string(), "\"REDACTED\"");
    }

    #[test]
    fn test_capsule_ops_custom_hooks() {
        use crate::types::ty::CapsuleOps;
        use std::collections::hash_map::DefaultHasher;

        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        struct CustomOpaque {
            id: u64,
            payload: String,
        }

        let ops = Arc::new(
            CapsuleOps::new(
                "custom_opaque",
                Arc::new(|a, b| {
                    if let (Some(x), Some(y)) = (
                        a.downcast_ref::<CustomOpaque>(),
                        b.downcast_ref::<CustomOpaque>(),
                    ) {
                        x == y
                    } else {
                        false
                    }
                }),
                Arc::new(|a| {
                    if let Some(x) = a.downcast_ref::<CustomOpaque>() {
                        let mut s = DefaultHasher::new();
                        x.hash(&mut s);
                        s.finish()
                    } else {
                        0
                    }
                }),
            )
            .with_conversion_to(Arc::new(|a, target| {
                if target == &Type::String
                    && let Some(x) = a.downcast_ref::<CustomOpaque>()
                {
                    use crate::encode::EncodeValue;
                    Some(format!("opaque-{}:{}", x.id, x.payload).encode_value())
                } else {
                    None
                }
            }))
            .with_conversion_from(Arc::new(|val| {
                if let ValueData::String(ref s) = *val.data
                    && let Some((id_str, payload)) =
                        s.strip_prefix("opaque-").and_then(|r| r.split_once(':'))
                    && let Ok(id) = id_str.parse::<u64>()
                {
                    Some(Arc::new(CustomOpaque {
                        id,
                        payload: payload.to_string(),
                    }))
                } else {
                    None
                }
            })),
        );

        // Test equality via ops
        let obj1 = CustomOpaque {
            id: 100,
            payload: "alpha".to_string(),
        };
        let obj2 = CustomOpaque {
            id: 100,
            payload: "alpha".to_string(),
        };
        let obj3 = CustomOpaque {
            id: 200,
            payload: "beta".to_string(),
        };

        let cap1 = Value::capsule_with_ops("custom_opaque", ops.clone(), obj1);
        let cap2 = Value::capsule_with_ops("custom_opaque", ops.clone(), obj2);
        let cap3 = Value::capsule_with_ops("custom_opaque", ops.clone(), obj3);

        // Value::eq uses CapsuleOps::equals
        assert_eq!(cap1, cap2);
        assert_ne!(cap1, cap3);

        // Hashing uses CapsuleOps::hash
        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();
        cap1.hash(&mut h1);
        cap2.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());

        // Coercion from capsule to String
        let coerced_str = cap1.coerce(&Type::String);
        assert_eq!(
            coerced_str.as_ref().ok().map(ToString::to_string),
            Some("\"opaque-100:alpha\"".to_string())
        );

        // Coercion from String to capsule
        use crate::encode::EncodeValue;
        let str_source = "opaque-300:gamma".encode_value();
        let coerced_cap = str_source.coerce(cap1.ty());
        let downcasted = coerced_cap
            .as_ref()
            .ok()
            .and_then(|c| c.downcast::<CustomOpaque>().ok());
        assert_eq!(downcasted.as_ref().map(|d| d.id), Some(300));
        assert_eq!(
            downcasted.as_ref().map(|d| d.payload.as_str()),
            Some("gamma")
        );

        // Fallback branches in custom capsule ops closures
        let bad_any: Arc<dyn std::any::Any + Send + Sync> = Arc::new(123_i32);
        assert!(!(ops.equals)(&bad_any, &bad_any));
        assert_eq!((ops.hash)(&bad_any), 0);
        assert!(
            ops.conversion_to
                .as_ref()
                .is_some_and(|conv| (conv)(&bad_any, &Type::Number).is_none())
        );
        assert!(
            ops.conversion_to
                .as_ref()
                .is_some_and(|conv| (conv)(&bad_any, &Type::String).is_none())
        );
        let non_string_val = 123_i64.encode_value();
        assert!(
            ops.conversion_from
                .as_ref()
                .is_some_and(|conv| (conv)(&non_string_val).is_none())
        );
        let non_opaque_val = "non_opaque".encode_value();
        assert!(
            ops.conversion_from
                .as_ref()
                .is_some_and(|conv| (conv)(&non_opaque_val).is_none())
        );
        let malformed_opaque = "opaque-notanumber:xyz".encode_value();
        assert!(
            ops.conversion_from
                .as_ref()
                .is_some_and(|conv| (conv)(&malformed_opaque).is_none())
        );
        let malformed_no_colon = "opaque-123".encode_value();
        assert!(
            ops.conversion_from
                .as_ref()
                .is_some_and(|conv| (conv)(&malformed_no_colon).is_none())
        );
        let malformed_empty = "opaque-".encode_value();
        assert!(
            ops.conversion_from
                .as_ref()
                .is_some_and(|conv| (conv)(&malformed_empty).is_none())
        );

        // from_capsule constructor, downcast_ref, and downcast error branches
        let from_cap = Value::from_capsule(
            CustomOpaque {
                id: 400,
                payload: "delta".to_string(),
            },
            ops.clone(),
        );
        assert_eq!(
            from_cap.downcast_ref::<CustomOpaque>().map(|c| c.id),
            Some(400)
        );
        assert!(from_cap.downcast_ref::<String>().is_none());
        assert!(
            42_i64
                .encode_value()
                .downcast_ref::<CustomOpaque>()
                .is_none()
        );

        assert_eq!(
            from_cap.as_capsule::<CustomOpaque>().ok().map(|c| c.id),
            Some(400)
        );
        assert!(from_cap.downcast::<CustomOpaque>().is_ok());
        assert!(from_cap.downcast_ref::<CustomOpaque>().is_some());
        assert!(from_cap.as_capsule::<String>().is_err());
        assert!(42_i64.encode_value().as_capsule::<CustomOpaque>().is_err());

        assert!(from_cap.downcast::<String>().is_err());
        assert!(42_i64.encode_value().downcast::<CustomOpaque>().is_err());

        let from_cap_str = Value::capsule("str_capsule", "hello".to_string());
        assert!(from_cap_str.as_capsule::<String>().is_ok());
        assert!(from_cap_str.downcast::<String>().is_ok());
        assert!(from_cap_str.downcast_ref::<String>().is_some());
        assert!(42_i64.encode_value().as_capsule::<String>().is_err());
        assert!(42_i64.encode_value().downcast::<String>().is_err());
        assert!(42_i64.encode_value().downcast_ref::<String>().is_none());
        assert!(from_cap_str.as_capsule::<CustomOpaque>().is_err());
        assert!(from_cap_str.downcast::<CustomOpaque>().is_err());
        assert!(from_cap_str.downcast_ref::<CustomOpaque>().is_none());

        // ValueMark::custom constructor
        assert_eq!(
            ValueMark::custom("audit"),
            ValueMark::Custom("audit".to_string())
        );

        // Diagnostic with path rendering
        let dummy_span = crate::span::Span::new(0, 0, 1, 1, 1, 10);
        let p = crate::types::path::Path::empty()
            .with_step(crate::types::path::PathStep::GetAttr(
                "settings".to_string(),
            ))
            .with_step(crate::types::path::PathStep::Index(0_i64.encode_value()));
        let diag = crate::diagnostic::Diagnostic::error("Type Error", "Invalid schema", dummy_span)
            .with_path(p);
        let writer = crate::diagnostic::DiagnosticWriter::new(false);
        let rendered = writer.format_diagnostic(&diag, "test.hcl", "settings = [123]\n");
        assert!(rendered.contains("at path: settings[0]"));
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash, crate::types::CapsuleType)]
    #[hcl(
        type_name = "managed_client",
        convert_to = "client_to_string",
        convert_from = "client_from_string"
    )]
    struct ManagedClient {
        endpoint: String,
        retries: u32,
    }

    fn client_to_string(client: &ManagedClient, target: &Type) -> Option<Value> {
        if target == &Type::String {
            Some(Value::new(
                Type::String,
                ValueData::String(format!("client://{}:{}", client.endpoint, client.retries)),
            ))
        } else {
            None
        }
    }

    fn client_from_string(val: &Value) -> Option<ManagedClient> {
        if let ValueData::String(ref s) = *val.data
            && let Some((endpoint, retries_str)) =
                s.strip_prefix("client://").and_then(|r| r.split_once(':'))
            && let Ok(retries) = retries_str.parse::<u32>()
        {
            Some(ManagedClient {
                endpoint: endpoint.to_string(),
                retries,
            })
        } else {
            None
        }
    }

    #[test]
    fn test_derive_capsule_type_integration() {
        assert!(client_from_string(&"not_client_protocol".encode_value()).is_none());
        assert!(client_from_string(&"client://endpoint:bad_int".encode_value()).is_none());
        assert!(client_from_string(&123_i64.encode_value()).is_none());

        let client1 = ManagedClient {
            endpoint: "api.example.com".to_string(),
            retries: 3,
        };
        let client2 = ManagedClient {
            endpoint: "api.example.com".to_string(),
            retries: 3,
        };
        let client3 = ManagedClient {
            endpoint: "api.other.com".to_string(),
            retries: 1,
        };

        // Convert into Value via From<T>
        let val1: Value = client1.clone().into();
        let val2: Value = client2.into();
        let val3: Value = client3.into();

        assert_eq!(val1.ty().to_string(), "capsule(managed_client)");
        assert_eq!(val1, val2);
        assert_ne!(val1, val3);

        // as_capsule downcast
        assert_eq!(
            val1.as_capsule::<ManagedClient>()
                .ok()
                .map(|c| c.endpoint.as_str()),
            Some("api.example.com")
        );
        assert_eq!(
            val1.as_capsule::<ManagedClient>().ok().map(|c| c.retries),
            Some(3)
        );
        assert!(val1.downcast::<ManagedClient>().is_ok());
        assert!(val1.downcast_ref::<ManagedClient>().is_some());
        let non_cap_mc = 42_i64.encode_value();
        assert!(non_cap_mc.as_capsule::<ManagedClient>().is_err());
        assert!(non_cap_mc.downcast::<ManagedClient>().is_err());
        assert!(non_cap_mc.downcast_ref::<ManagedClient>().is_none());
        let wrong_cap_mc = Value::capsule("other", 100_u32);
        assert!(wrong_cap_mc.as_capsule::<ManagedClient>().is_err());
        assert!(wrong_cap_mc.downcast::<ManagedClient>().is_err());
        assert!(wrong_cap_mc.downcast_ref::<ManagedClient>().is_none());

        // Coerce via convert_to hook
        let coerced = val1.coerce(&Type::String);
        assert_eq!(
            coerced.as_ref().ok().map(ToString::to_string),
            Some("\"client://api.example.com:3\"".to_string())
        );

        // Coerce back via convert_from hook
        let cap_ty =
            Type::capsule_with_ops::<ManagedClient>("managed_client", ManagedClient::capsule_ops());
        let back = coerced.as_ref().map(|c| c.coerce(&cap_ty));
        assert_eq!(
            back.as_ref()
                .ok()
                .and_then(|r| r.as_ref().ok())
                .and_then(|v| v.as_capsule::<ManagedClient>().ok()),
            Some(&client1)
        );

        // Pass through HCL evaluation
        let mut ctx = crate::eval::context::Context::new();
        ctx.set_variable("client", val1.clone());
        let expr = crate::ast::expr::Expression::Variable(
            "client".to_string(),
            crate::span::Span::default(),
        );
        let mut eval = crate::eval::evaluator::Evaluator::new(&ctx);
        let eval_val = eval.eval_expr(&expr);
        assert_eq!(eval_val, val1);

        // Exercise client_to_string with non-String target and client_from_string fallback branches
        use crate::encode::EncodeValue;
        assert!(client_to_string(&client1, &Type::Number).is_none());
        assert!(client_from_string(&"not_client".encode_value()).is_none());
        assert!(client_from_string(&"client://onlyendpoint".encode_value()).is_none());
        assert!(client_from_string(&"client://endpoint:badretries".encode_value()).is_none());
    }

    #[test]
    fn test_value_coverage_exhaustive() {
        use crate::encode::EncodeValue;
        use crate::error::HclError;
        use crate::types::path::{Path, PathStep};

        // 1. get_path: PathStep::Index on non-array
        let non_arr = "not_array".encode_value();
        let p_idx = Path::empty().with_step(PathStep::Index(0_i64.encode_value()));
        assert!(non_arr.get_path(&p_idx).is_err());

        // 2. get_path: PathStep::Key errors
        let mut obj_map = std::collections::BTreeMap::new();
        obj_map.insert("foo".to_string(), 100_i64.encode_value());
        let obj = Value::new(
            Type::object(std::collections::BTreeMap::new()),
            ValueData::Object(obj_map),
        );

        // Key not found in object
        let p_missing = Path::empty().with_step(PathStep::Key("missing".encode_value()));
        assert!(obj.get_path(&p_missing).is_err());

        // Non-string key value (e.g. integer 123)
        let p_num_key = Path::empty().with_step(PathStep::Key(123_i64.encode_value()));
        assert!(obj.get_path(&p_num_key).is_err());

        // PathStep::Key on non-object
        let non_obj = 42_i64.encode_value();
        let p_key = Path::empty().with_step(PathStep::Key("foo".encode_value()));
        assert!(non_obj.get_path(&p_key).is_err());

        // 3. Set transform
        let mut s = std::collections::BTreeSet::new();
        s.insert(1_i64.encode_value());
        s.insert(2_i64.encode_value());
        let set_val = Value::new(Type::Set(Box::new(Type::Number)), ValueData::Set(s));
        let transformed_set = set_val.transform(&mut |_p, v| {
            if let ValueData::Number(n) = &*v.data {
                Ok(Value::new(
                    Type::Number,
                    ValueData::Number(n.clone() + crate::number::Number::from(10)),
                ))
            } else {
                Ok(v.clone())
            }
        });
        assert_eq!(
            transformed_set.as_ref().ok().map(Value::ty),
            Some(&Type::Set(Box::new(Type::Number)))
        );

        // 4. Set walk and walk pruning
        let mut count = 0;
        let walk_res = set_val.walk(&mut |_p, _v| {
            count += 1;
            Ok(true)
        });
        assert!(walk_res.is_ok());
        assert_eq!(count, 3); // 1 set + 2 elements

        let mut pruned_count = 0;
        let prune_res = set_val.walk(&mut |_p, _v| {
            pruned_count += 1;
            Ok(false)
        });
        assert!(prune_res.is_ok());
        assert_eq!(pruned_count, 1);

        let mut should_descend = true;
        let mut toggle_count = 0;
        let mut toggle_visitor = |_p: &Path, _v: &Value| {
            toggle_count += 1;
            let res = should_descend;
            should_descend = false;
            Ok(res)
        };
        assert!(set_val.walk(&mut toggle_visitor).is_ok());
        assert!(toggle_count >= 2);

        // 5. Transform and walk error branches (lines 704, 715, 725, 759, 770, 777, 783)
        let arr_val = Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(vec![1_i64.encode_value()]),
        );
        // Array transform error (704)
        assert!(
            arr_val
                .transform(&mut |_p, _v| Err(HclError::Type("fail".into())))
                .is_err()
        );
        // Object transform error (715)
        assert!(
            obj.transform(&mut |_p, _v| Err(HclError::Type("fail".into())))
                .is_err()
        );
        // Set transform error (725)
        assert!(
            set_val
                .transform(&mut |_p, _v| Err(HclError::Type("fail".into())))
                .is_err()
        );
        // Walk root visitor error (759)
        assert!(
            arr_val
                .walk(&mut |_p, _v| Err(HclError::Type("fail root".into())))
                .is_err()
        );
        // Walk array child error (770)
        assert!(
            arr_val
                .walk(&mut |_p, v| {
                    if matches!(v.ty(), Type::Number) {
                        Err(HclError::Type("fail child".into()))
                    } else {
                        Ok(true)
                    }
                })
                .is_err()
        );
        // Walk object child error (777)
        assert!(
            obj.walk(&mut |_p, v| {
                if matches!(v.ty(), Type::Number) {
                    Err(HclError::Type("fail child".into()))
                } else {
                    Ok(true)
                }
            })
            .is_err()
        );
        // Walk set child error (783)
        assert!(
            set_val
                .walk(&mut |_p, v| {
                    if matches!(v.ty(), Type::Number) {
                        Err(HclError::Type("fail child".into()))
                    } else {
                        Ok(true)
                    }
                })
                .is_err()
        );

        // 6. Coerce to capsule error
        let cap_ty = Type::capsule::<()>("opaque_cap");
        assert!("hello".encode_value().coerce(&cap_ty).is_err());
    }

    #[test]
    fn test_granular_path_based_marks() {
        use crate::encode::EncodeValue;

        // Build nested configuration:
        // {
        //   database = {
        //     host = "localhost"
        //     password = "supersecret"
        //   }
        //   servers = ["web-1", "web-2"]
        //   tags = {
        //     "env" = "production"
        //   }
        // }
        let mut db = BTreeMap::new();
        db.insert("host".to_string(), "localhost".encode_value());
        db.insert("password".to_string(), "supersecret".encode_value());

        let servers = vec!["web-1".encode_value(), "web-2".encode_value()];

        let mut tags = BTreeMap::new();
        tags.insert("env".to_string(), "production".encode_value());

        let mut root_map = BTreeMap::new();
        root_map.insert(
            "database".to_string(),
            Value::new(Type::Dynamic, ValueData::Object(db)),
        );
        root_map.insert(
            "servers".to_string(),
            Value::new(Type::Dynamic, ValueData::Array(servers)),
        );
        root_map.insert(
            "tags".to_string(),
            Value::new(Type::Dynamic, ValueData::Object(tags)),
        );

        let root_val = Value::new(Type::Dynamic, ValueData::Object(root_map));

        assert!(!root_val.has_marked_children());
        assert!(root_val.paths_with_mark(&ValueMark::Sensitive).is_empty());

        // 1. Mark nested attribute: database.password
        let path_pass = Path::new(vec![
            PathStep::GetAttr("database".to_string()),
            PathStep::GetAttr("password".to_string()),
        ]);
        let marked_root = root_val
            .mark_path(&path_pass, ValueMark::Sensitive)
            .expect("mark database.password");

        assert!(marked_root.has_marked_children());
        let sens_paths = marked_root.paths_with_mark(&ValueMark::Sensitive);
        assert_eq!(sens_paths.len(), 1);
        assert_eq!(sens_paths[0].to_string(), "database.password");

        // 2. Mark nested array element: servers[1]
        let path_srv = Path::new(vec![
            PathStep::GetAttr("servers".to_string()),
            PathStep::Index(Value::new(Type::Number, ValueData::Number(1.into()))),
        ]);
        let marked_both = marked_root
            .mark_path(&path_srv, ValueMark::Sensitive)
            .expect("mark servers[1]");

        let both_paths = marked_both.paths_with_mark(&ValueMark::Sensitive);
        assert_eq!(both_paths.len(), 2);

        // 3. Mark map key: tags["env"]
        let path_tag = Path::new(vec![
            PathStep::GetAttr("tags".to_string()),
            PathStep::Key("env".encode_value()),
        ]);
        let custom_mark = ValueMark::custom("env:prod");
        let marked_custom = marked_both
            .mark_path(&path_tag, custom_mark.clone())
            .expect("mark tags.env");

        assert_eq!(marked_custom.paths_with_mark(&custom_mark).len(), 1);

        // 4. Mark root path directly
        let marked_root_self = marked_custom
            .mark_path(&Path::empty(), ValueMark::Sensitive)
            .expect("mark root");
        assert!(marked_root_self.has_mark(&ValueMark::Sensitive));

        // 5. Unmark path
        let (unmarked_pass, removed_marks) = marked_both
            .unmark_path(&path_pass)
            .expect("unmark database.password");
        assert!(removed_marks.contains(&ValueMark::Sensitive));
        let remaining = unmarked_pass.paths_with_mark(&ValueMark::Sensitive);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].to_string(), "servers[1]");

        // Unmark root
        let (unmarked_all_root, root_rem) = marked_root_self
            .unmark_path(&Path::empty())
            .expect("unmark root");
        assert!(root_rem.contains(&ValueMark::Sensitive));
        assert!(!unmarked_all_root.has_mark(&ValueMark::Sensitive));

        // 6. Test Set paths_with_mark and has_marked_children
        let mut test_set = BTreeSet::new();
        test_set.insert("item1".encode_value().mark(ValueMark::Sensitive));
        let set_val = Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(test_set));
        assert!(set_val.has_marked_children());
        assert_eq!(set_val.paths_with_mark(&ValueMark::Sensitive).len(), 1);

        // 7. Error handling for mark_path and unmark_path
        let bad_attr = Path::new(vec![PathStep::GetAttr("nonexistent".to_string())]);
        assert!(root_val.mark_path(&bad_attr, ValueMark::Sensitive).is_err());
        assert!(root_val.unmark_path(&bad_attr).is_err());

        let non_obj = "hello".encode_value();
        assert!(non_obj.mark_path(&bad_attr, ValueMark::Sensitive).is_err());
        assert!(non_obj.unmark_path(&bad_attr).is_err());

        let bad_idx_val = Path::new(vec![
            PathStep::GetAttr("servers".to_string()),
            PathStep::Index("not_a_num".encode_value()),
        ]);
        assert!(
            root_val
                .mark_path(&bad_idx_val, ValueMark::Sensitive)
                .is_err()
        );
        assert!(root_val.unmark_path(&bad_idx_val).is_err());

        let oob_idx = Path::new(vec![
            PathStep::GetAttr("servers".to_string()),
            PathStep::Index(Value::new(Type::Number, ValueData::Number(99.into()))),
        ]);
        assert!(root_val.mark_path(&oob_idx, ValueMark::Sensitive).is_err());
        assert!(root_val.unmark_path(&oob_idx).is_err());

        let idx_on_non_arr = Path::new(vec![
            PathStep::GetAttr("database".to_string()),
            PathStep::Index(Value::new(Type::Number, ValueData::Number(0.into()))),
        ]);
        assert!(
            root_val
                .mark_path(&idx_on_non_arr, ValueMark::Sensitive)
                .is_err()
        );
        assert!(root_val.unmark_path(&idx_on_non_arr).is_err());

        let bad_key_type = Path::new(vec![
            PathStep::GetAttr("tags".to_string()),
            PathStep::Key(123.encode_value()),
        ]);
        assert!(
            root_val
                .mark_path(&bad_key_type, ValueMark::Sensitive)
                .is_err()
        );
        assert!(root_val.unmark_path(&bad_key_type).is_err());

        let missing_key = Path::new(vec![
            PathStep::GetAttr("tags".to_string()),
            PathStep::Key("not_found".encode_value()),
        ]);
        assert!(
            root_val
                .mark_path(&missing_key, ValueMark::Sensitive)
                .is_err()
        );
        assert!(root_val.unmark_path(&missing_key).is_err());

        let key_on_non_obj = Path::new(vec![
            PathStep::GetAttr("servers".to_string()),
            PathStep::Key("any_key".encode_value()),
        ]);
        assert!(
            root_val
                .mark_path(&key_on_non_obj, ValueMark::Sensitive)
                .is_err()
        );
        assert!(root_val.unmark_path(&key_on_non_obj).is_err());

        // 8. Traversal evaluation preserving path marks
        let mut eval_ctx = crate::eval::context::Context::new();
        eval_ctx.set_variable("cfg", marked_both);

        fn empty_span() -> crate::span::Span {
            crate::span::Span::default()
        }

        let expr_pass = crate::ast::expr::Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(crate::ast::expr::Expression::Variable(
                    "cfg".to_string(),
                    empty_span(),
                )),
                operators: vec![
                    crate::ast::expr::TraversalOperator::GetAttr(
                        "database".to_string(),
                        empty_span(),
                    ),
                    crate::ast::expr::TraversalOperator::GetAttr(
                        "password".to_string(),
                        empty_span(),
                    ),
                ],
            }),
            empty_span(),
        );
        let (val_pass, _) = crate::eval::evaluator::Evaluator::new(&eval_ctx)
            .evaluate(&expr_pass)
            .expect("eval database.password");
        assert!(val_pass.has_mark(&ValueMark::Sensitive));

        let expr_host = crate::ast::expr::Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(crate::ast::expr::Expression::Variable(
                    "cfg".to_string(),
                    empty_span(),
                )),
                operators: vec![
                    crate::ast::expr::TraversalOperator::GetAttr(
                        "database".to_string(),
                        empty_span(),
                    ),
                    crate::ast::expr::TraversalOperator::GetAttr("host".to_string(), empty_span()),
                ],
            }),
            empty_span(),
        );
        let (val_host, _) = crate::eval::evaluator::Evaluator::new(&eval_ctx)
            .evaluate(&expr_host)
            .expect("eval database.host");
        assert!(!val_host.has_mark(&ValueMark::Sensitive));

        let expr_s1 = crate::ast::expr::Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(crate::ast::expr::Expression::Variable(
                    "cfg".to_string(),
                    empty_span(),
                )),
                operators: vec![
                    crate::ast::expr::TraversalOperator::GetAttr(
                        "servers".to_string(),
                        empty_span(),
                    ),
                    crate::ast::expr::TraversalOperator::Index(
                        crate::ast::expr::Expression::Number(1.into(), empty_span()),
                        empty_span(),
                    ),
                ],
            }),
            empty_span(),
        );
        let (val_s1, _) = crate::eval::evaluator::Evaluator::new(&eval_ctx)
            .evaluate(&expr_s1)
            .expect("eval servers[1]");
        assert!(val_s1.has_mark(&ValueMark::Sensitive));

        let expr_s0 = crate::ast::expr::Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(crate::ast::expr::Expression::Variable(
                    "cfg".to_string(),
                    empty_span(),
                )),
                operators: vec![
                    crate::ast::expr::TraversalOperator::GetAttr(
                        "servers".to_string(),
                        empty_span(),
                    ),
                    crate::ast::expr::TraversalOperator::Index(
                        crate::ast::expr::Expression::Number(0.into(), empty_span()),
                        empty_span(),
                    ),
                ],
            }),
            empty_span(),
        );
        let (val_s0, _) = crate::eval::evaluator::Evaluator::new(&eval_ctx)
            .evaluate(&expr_s0)
            .expect("eval servers[0]");
        assert!(!val_s0.has_mark(&ValueMark::Sensitive));
    }

    /// Tests edge cases, error paths, and remaining branches for `Value`.
    #[test]
    fn test_value_coverage_exhaustive_gaps() {
        use std::collections::{BTreeMap, BTreeSet};

        // 1. Capsule comparisons via Ord::cmp
        let ops_with_ok_cmp = std::sync::Arc::new(
            crate::types::ty::CapsuleOps::new(
                "cap_test",
                std::sync::Arc::new(|_, _| true),
                std::sync::Arc::new(|_| 1),
            )
            .with_cmp(std::sync::Arc::new(|first, second| {
                let x = match first.downcast_ref::<i32>() {
                    Some(&v) => v,
                    None => 0,
                };
                let y = match second.downcast_ref::<i32>() {
                    Some(&v) => v,
                    None => 0,
                };
                Ok(x.cmp(&y))
            })),
        );
        let cap_ok1 = Value::capsule_with_ops("cap_test", ops_with_ok_cmp.clone(), 10_i32);
        let cap_ok2 = Value::capsule_with_ops("cap_test", ops_with_ok_cmp.clone(), 20_i32);
        assert_eq!(cap_ok1.cmp(&cap_ok2), std::cmp::Ordering::Less);

        let cap_str = Value::capsule_with_ops("cap_test", ops_with_ok_cmp, "not_i32".to_string());
        assert_eq!(cap_ok1.cmp(&cap_str), std::cmp::Ordering::Greater);
        assert_eq!(cap_str.cmp(&cap_ok1), std::cmp::Ordering::Less);

        let ops_with_err_cmp = std::sync::Arc::new(
            crate::types::ty::CapsuleOps::new(
                "cap_test",
                std::sync::Arc::new(|_, _| true),
                std::sync::Arc::new(|_| 1),
            )
            .with_cmp(std::sync::Arc::new(|_, _| Err("cmp error".to_string()))),
        );
        let cap_err1 = Value::capsule_with_ops("cap_test", ops_with_err_cmp.clone(), 10_i32);
        let cap_err2 = Value::capsule_with_ops("cap_test", ops_with_err_cmp, 20_i32);
        assert_eq!(cap_err1.cmp(&cap_err2), std::cmp::Ordering::Equal);

        let ops_no_cmp = std::sync::Arc::new(crate::types::ty::CapsuleOps::new(
            "cap_test",
            std::sync::Arc::new(|_, _| true),
            std::sync::Arc::new(|_| 1),
        ));
        let cap_none1 = Value::capsule_with_ops("cap_test", ops_no_cmp.clone(), 10_i32);
        let cap_none2 = Value::capsule_with_ops("cap_test", ops_no_cmp.clone(), 20_i32);
        assert_eq!(cap_none1.cmp(&cap_none2), std::cmp::Ordering::Equal);

        let cap_raw1 = Value::new(
            Type::Capsule {
                name: "raw_test",
                type_id: std::any::TypeId::of::<i32>(),
                ops: None,
            },
            ValueData::Capsule(std::sync::Arc::new(10_i32)),
        );
        let cap_raw2 = Value::new(
            Type::Capsule {
                name: "raw_test",
                type_id: std::any::TypeId::of::<i32>(),
                ops: None,
            },
            ValueData::Capsule(std::sync::Arc::new(20_i32)),
        );
        assert_eq!(cap_raw1.cmp(&cap_raw2), std::cmp::Ordering::Equal);

        use crate::encode::EncodeValue;

        // 2. mark_path_steps error paths
        let base_arr = Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(vec![100_i64.encode_value()]),
        );
        let neg_idx = &[PathStep::Index((-1_i64).encode_value())];
        assert!(
            base_arr
                .mark_path_steps(neg_idx, ValueMark::Sensitive, &Path::empty())
                .is_err()
        );

        let str_idx = &[PathStep::Index("invalid".encode_value())];
        assert!(
            base_arr
                .mark_path_steps(str_idx, ValueMark::Sensitive, &Path::empty())
                .is_err()
        );

        let num_key = &[PathStep::Key(42_i64.encode_value())];
        let mut obj_fields = BTreeMap::new();
        obj_fields.insert("k".to_string(), 1_i64.encode_value());
        let base_obj = Value::new(Type::object(BTreeMap::new()), ValueData::Object(obj_fields));
        assert!(
            base_obj
                .mark_path_steps(num_key, ValueMark::Sensitive, &Path::empty())
                .is_err()
        );

        let single_idx = &[PathStep::Index(0_i64.encode_value())];
        let plain_val = "string_not_array".encode_value();
        assert!(
            plain_val
                .mark_path_steps(single_idx, ValueMark::Sensitive, &Path::empty())
                .is_err()
        );

        let single_key = &[PathStep::Key("k".encode_value())];
        assert!(
            plain_val
                .mark_path_steps(single_key, ValueMark::Sensitive, &Path::empty())
                .is_err()
        );

        // Recursive child error paths for mark_path_steps
        let recurse_idx_err = &[
            PathStep::Index(0_i64.encode_value()),
            PathStep::Index((-1_i64).encode_value()),
        ];
        assert!(
            base_arr
                .mark_path_steps(recurse_idx_err, ValueMark::Sensitive, &Path::empty())
                .is_err()
        );

        let recurse_key_err = &[
            PathStep::Key("k".encode_value()),
            PathStep::Index((-1_i64).encode_value()),
        ];
        assert!(
            base_obj
                .mark_path_steps(recurse_key_err, ValueMark::Sensitive, &Path::empty())
                .is_err()
        );

        // 3. unmark_path_steps error paths
        assert!(base_arr.unmark_path_steps(neg_idx, &Path::empty()).is_err());
        assert!(base_arr.unmark_path_steps(str_idx, &Path::empty()).is_err());
        assert!(
            base_arr
                .unmark_path_steps(&[PathStep::Index(99_i64.encode_value())], &Path::empty())
                .is_err()
        );
        assert!(base_obj.unmark_path_steps(num_key, &Path::empty()).is_err());
        assert!(
            plain_val
                .unmark_path_steps(single_idx, &Path::empty())
                .is_err()
        );
        assert!(
            plain_val
                .unmark_path_steps(single_key, &Path::empty())
                .is_err()
        );

        // Recursive child error paths for unmark_path_steps
        assert!(
            base_arr
                .unmark_path_steps(recurse_idx_err, &Path::empty())
                .is_err()
        );
        assert!(
            base_obj
                .unmark_path_steps(recurse_key_err, &Path::empty())
                .is_err()
        );

        // Successful unmark_path_steps on array and object
        let (unmarked_arr_step, _) = base_arr
            .unmark_path_steps(&[PathStep::Index(0_i64.encode_value())], &Path::empty())
            .expect("unmark array step");
        assert_eq!(unmarked_arr_step, base_arr);

        let (unmarked_obj_step, _) = base_obj
            .unmark_path_steps(&[PathStep::Key("k".encode_value())], &Path::empty())
            .expect("unmark object step");
        assert_eq!(unmarked_obj_step, base_obj);

        // 4. has_marked_children recursive for Set and Array
        let leaf_marked = "secret".encode_value().mark(ValueMark::Sensitive);
        let mut inner_obj = BTreeMap::new();
        inner_obj.insert("unmarked_field".to_string(), 10_i64.encode_value());
        inner_obj.insert("prop".to_string(), leaf_marked);
        let mid_obj_val = Value::new(Type::object(BTreeMap::new()), ValueData::Object(inner_obj));

        let parent_arr = Value::new(
            Type::List(Box::new(Type::Dynamic)),
            ValueData::Array(vec![1_i64.encode_value(), mid_obj_val]),
        );
        assert!(!parent_arr.has_mark(&ValueMark::Sensitive));
        assert!(parent_arr.has_marked_children());

        let parent_set = Value::new(
            Type::Set(Box::new(parent_arr.ty().clone())),
            ValueData::Set(BTreeSet::from([2_i64.encode_value(), parent_arr])),
        );
        assert!(parent_set.has_marked_children());

        // 5. Traits and methods coverage
        let val_a = 1_i64.encode_value();
        let val_b = 2_i64.encode_value();
        assert_eq!(val_a.partial_cmp(&val_b), None);
        assert!(!(val_a < val_b));
        assert_eq!(val_a.cmp(&val_b), std::cmp::Ordering::Less);

        let mut v_clone = 10_i64.encode_value();
        v_clone.clone_from(&val_b);
        assert_eq!(v_clone, val_b);

        assert!(!format!("{:?}", ValueData::Null).is_empty());
        assert_eq!(format!("{}", ValueMark::Sensitive), "sensitive");
        assert_eq!(
            format!("{}", 42_i64.encode_value().mark(ValueMark::Sensitive)),
            "(sensitive value)"
        );
        assert!(
            42_i64
                .encode_value()
                .mark(ValueMark::Sensitive)
                .is_sensitive()
        );

        let multi_arr_disp = Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(vec![1_i64.encode_value(), 2_i64.encode_value()]),
        );
        assert_eq!(format!("{multi_arr_disp}"), "[1, 2]");

        let multi_set_disp = Value::new(
            Type::Set(Box::new(Type::Number)),
            ValueData::Set(BTreeSet::from([1_i64.encode_value(), 2_i64.encode_value()])),
        );
        assert_eq!(format!("{multi_set_disp}"), "[1, 2]");

        let mut multi_obj_fields = BTreeMap::new();
        multi_obj_fields.insert("a".to_string(), 1_i64.encode_value());
        multi_obj_fields.insert("b".to_string(), 2_i64.encode_value());
        let multi_obj_disp = Value::new(
            Type::object(BTreeMap::new()),
            ValueData::Object(multi_obj_fields),
        );
        assert!(format!("{multi_obj_disp}").contains("\"a\" = 1"));

        let marked_set = BTreeSet::from([ValueMark::Sensitive]);
        let val_with_m = 42_i64.encode_value().with_marks(marked_set);
        assert!(val_with_m.has_mark(&ValueMark::Sensitive));
        assert_ne!(val_with_m, 42_i64.encode_value());

        let (unmarked_deep, removed_deep) = val_with_m.unmark_deep();
        assert!(!unmarked_deep.has_mark(&ValueMark::Sensitive));
        assert_eq!(removed_deep.len(), 1);

        let unmarked_sens = val_with_m.unmark_sensitive();
        assert!(!unmarked_sens.has_mark(&ValueMark::Sensitive));

        assert!(val_with_m.refinement().is_none());
        assert!(val_with_m.refinement_arc().is_none());

        let msgpack_bytes = val_with_m.to_msgpack().expect("serialize msgpack");
        let deserialized_val =
            Value::from_msgpack(&msgpack_bytes, val_with_m.ty()).expect("deserialize msgpack");
        assert_eq!(deserialized_val, val_with_m);

        assert!(val_with_m.as_capsule_any().is_none());
        assert!(cap_ok1.as_capsule::<String>().is_err());

        // Directly marked array element
        let directly_marked_arr = Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(vec![1_i64.encode_value().mark(ValueMark::Sensitive)]),
        );
        assert!(directly_marked_arr.has_marked_children());

        // Capsule conversions returning None in coerce
        let cap_conv_ops = std::sync::Arc::new(
            crate::types::ty::CapsuleOps::new(
                "test_conv_capsule",
                std::sync::Arc::new(|_, _| true),
                std::sync::Arc::new(|_| 1),
            )
            .with_conversion_to(std::sync::Arc::new(|_, target| {
                if target == &Type::String {
                    Some("converted_str".encode_value())
                } else {
                    None
                }
            }))
            .with_conversion_from(std::sync::Arc::new(|val| {
                if let ValueData::String(ref s) = *val.data
                    && s == "valid_token"
                {
                    Some(std::sync::Arc::new(777_i32))
                } else {
                    None
                }
            })),
        );
        let cap_val_conv = Value::capsule_with_ops("test_conv_capsule", cap_conv_ops, 123_i32);
        assert!(cap_val_conv.coerce(&Type::String).is_ok());
        assert!(cap_val_conv.coerce(&Type::Number).is_err());
        assert!(
            "valid_token"
                .encode_value()
                .coerce(cap_val_conv.ty())
                .is_ok()
        );
        assert!(
            "invalid_token"
                .encode_value()
                .coerce(cap_val_conv.ty())
                .is_err()
        );
        assert!(100_i64.encode_value().coerce(cap_val_conv.ty()).is_err());

        // Capsule ops without conversion_from hook
        let target_no_conv = Type::capsule_with_ops::<i32>("cap_no_conv", ops_no_cmp);
        assert!(10_i64.encode_value().coerce(&target_no_conv).is_err());

        // Unknown string with refinement coerced to Type::Bool (target != Type::Number)
        let unk_str_ref = Value::unknown_refined(
            Type::String,
            crate::types::refinement::Refinement::new().with_prefix("prefix"),
        );
        assert!(unk_str_ref.coerce(&Type::Bool).is_ok());
    }

    /// Tests comprehensive branch, region, and line coverage for `walk_internal` and `transform_internal`
    /// across all structural variants (primitive, array, object, set) and error/pruning paths.
    #[test]
    fn test_walk_transform_instantiation_full_coverage() {
        use crate::encode::EncodeValue;
        use crate::types::path::Path;

        let val_prim = 42_i64.encode_value();
        let val_arr = Value::new(
            Type::List(Box::new(Type::Number)),
            ValueData::Array(vec![1_i64.encode_value(), 2_i64.encode_value()]),
        );
        let mut obj_map = BTreeMap::new();
        obj_map.insert("key".to_string(), 10_i64.encode_value());
        let val_obj = Value::new(Type::Dynamic, ValueData::Object(obj_map));

        let mut set_data = std::collections::BTreeSet::new();
        set_data.insert(100_i64.encode_value());
        let val_set = Value::new(Type::Set(Box::new(Type::Number)), ValueData::Set(set_data));

        let val_err_arr = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(vec!["trigger_err".encode_value()]),
        );
        let mut err_obj_map = BTreeMap::new();
        err_obj_map.insert("sub".to_string(), "trigger_err".encode_value());
        let val_err_obj = Value::new(Type::Dynamic, ValueData::Object(err_obj_map));

        let mut err_set_data = std::collections::BTreeSet::new();
        err_set_data.insert("trigger_err".encode_value());
        let val_err_set = Value::new(
            Type::Set(Box::new(Type::String)),
            ValueData::Set(err_set_data),
        );

        let val_prune = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(vec!["prune".encode_value()]),
        );

        // Visitor closure covering 100% of walk_internal
        let mut visitor = |_path: &Path, val: &Value| -> Result<bool, HclError> {
            match &*val.data {
                ValueData::String(s) if s == "trigger_err" => {
                    Err(HclError::Type("intentional_walk_err".into()))
                }
                ValueData::String(s) if s == "prune" => Ok(false),
                _ => Ok(true),
            }
        };

        assert!(val_prim.walk(&mut visitor).is_ok());
        assert!("normal_str".encode_value().walk(&mut visitor).is_ok());
        assert!(val_prune.walk(&mut visitor).is_ok());
        assert!(val_arr.walk(&mut visitor).is_ok());
        assert!(val_err_arr.walk(&mut visitor).is_err());
        assert!(val_obj.walk(&mut visitor).is_ok());
        assert!(val_err_obj.walk(&mut visitor).is_err());
        assert!(val_set.walk(&mut visitor).is_ok());
        assert!(val_err_set.walk(&mut visitor).is_err());
        assert!("trigger_err".encode_value().walk(&mut visitor).is_err());

        // Transformer closure covering 100% of transform_internal
        let mut transformer = |_path: &Path, val: &Value| -> Result<Value, HclError> {
            match &*val.data {
                ValueData::String(s) if s == "trigger_err" => {
                    Err(HclError::Type("intentional_transform_err".into()))
                }
                _ => Ok(val.clone()),
            }
        };

        assert!(val_prim.transform(&mut transformer).is_ok());
        assert!(
            "normal_str"
                .encode_value()
                .transform(&mut transformer)
                .is_ok()
        );
        assert!(val_arr.transform(&mut transformer).is_ok());
        assert!(val_err_arr.transform(&mut transformer).is_err());
        assert!(val_obj.transform(&mut transformer).is_ok());
        assert!(val_err_obj.transform(&mut transformer).is_err());
        assert!(val_set.transform(&mut transformer).is_ok());
        assert!(val_err_set.transform(&mut transformer).is_err());
        assert!(
            "trigger_err"
                .encode_value()
                .transform(&mut transformer)
                .is_err()
        );
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    struct EphemeralMark {
        generation: u32,
    }

    impl std::fmt::Display for EphemeralMark {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "ephemeral:{}", self.generation)
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    struct TaintMark;

    impl std::fmt::Display for TaintMark {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "tainted")
        }
    }

    #[test]
    fn test_typed_value_marks() {
        let val = Value::new(Type::String, ValueData::String("secure_data".into()));
        assert!(!val.has_mark_type::<EphemeralMark>());
        assert!(val.get_mark::<EphemeralMark>().is_none());

        let marked_eph = val.mark_typed(EphemeralMark { generation: 42 });
        assert!(marked_eph.has_mark_type::<EphemeralMark>());
        assert_eq!(
            marked_eph.get_mark::<EphemeralMark>(),
            Some(&EphemeralMark { generation: 42 })
        );
        assert!(!marked_eph.has_mark_type::<TaintMark>());
        assert!(marked_eph.get_mark::<TaintMark>().is_none());

        // ValueMark::typed constructor and display
        let custom_tm = ValueMark::typed(TaintMark);
        assert_eq!(format!("{custom_tm}"), "tainted");

        let marked_both = marked_eph.mark(custom_tm.clone());
        assert!(marked_both.has_mark_type::<EphemeralMark>());
        assert!(marked_both.has_mark_type::<TaintMark>());
        assert!(marked_both.has_mark(&custom_tm));

        let sensitive_val = val
            .mark(ValueMark::Sensitive)
            .mark(ValueMark::custom("tag"));
        assert!(sensitive_val.get_mark::<EphemeralMark>().is_none());
        assert!(!sensitive_val.has_mark_type::<EphemeralMark>());

        // TypedMark equality, hashing, display, debug, ord
        let tm1 = TypedMark::new(EphemeralMark { generation: 1 });
        let tm2 = TypedMark::new(EphemeralMark { generation: 1 });
        let tm3 = TypedMark::new(EphemeralMark { generation: 2 });
        let tm_other = TypedMark::new(TaintMark);

        assert_eq!(tm1, tm2);
        assert_ne!(tm1, tm3);
        assert_ne!(tm1, tm_other);
        assert_eq!(format!("{tm1}"), "ephemeral:1");
        assert!(format!("{tm1:?}").contains("EphemeralMark"));

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();
        tm1.hash(&mut h1);
        tm2.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());

        let mut h_other = DefaultHasher::new();
        tm_other.hash(&mut h_other);
        assert_ne!(h_other.finish(), 0);

        // AnyValueMark dynamic dispatch coverage
        let any_mark: Box<dyn AnyValueMark> = Box::new(TaintMark);
        let mut h_any = DefaultHasher::new();
        any_mark.dyn_hash(&mut h_any);
        assert_eq!(any_mark.mark_name(), std::any::type_name::<TaintMark>());
        assert!(any_mark.dyn_eq(&TaintMark));
        assert!(!any_mark.dyn_eq(&EphemeralMark { generation: 1 }));

        // Fallback pointer ordering when Display strings match but Eq is false
        #[derive(Clone, Debug)]
        struct SameDisplayDiffEq {
            name: String,
            id: u32,
        }
        impl PartialEq for SameDisplayDiffEq {
            fn eq(&self, other: &Self) -> bool {
                self.id == other.id
            }
        }
        impl Eq for SameDisplayDiffEq {}
        impl std::hash::Hash for SameDisplayDiffEq {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.id.hash(state);
            }
        }
        impl std::fmt::Display for SameDisplayDiffEq {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.name)
            }
        }

        let m1 = TypedMark::new(SameDisplayDiffEq {
            name: "same".into(),
            id: 1,
        });
        let m2 = TypedMark::new(SameDisplayDiffEq {
            name: "same".into(),
            id: 2,
        });
        assert_ne!(m1, m2);
        assert_ne!(m1.cmp(&m2), std::cmp::Ordering::Equal);

        let mut h_m1 = DefaultHasher::new();
        m1.hash(&mut h_m1);
        let mut h_m2 = DefaultHasher::new();
        m2.hash(&mut h_m2);
        assert_ne!(h_m1.finish(), h_m2.finish());

        // Ord and PartialOrd ordering
        assert_eq!(tm1.partial_cmp(&tm2), Some(std::cmp::Ordering::Equal));
        assert_eq!(tm1.cmp(&tm3), std::cmp::Ordering::Less);
        assert!(tm1 < tm3);
        assert!(tm1.cmp(&tm2).is_eq());

        // Unmarking
        let (unmarked, removed) = marked_both.unmark();
        assert!(!unmarked.has_mark_type::<EphemeralMark>());
        assert!(!unmarked.has_mark_type::<TaintMark>());
        assert_eq!(removed.len(), 2);
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Counter(i32);

    #[test]
    fn test_capsule_methods() {
        let eq_fn: crate::types::ty::CapsuleEqualsFn = std::sync::Arc::new(|a, b| {
            let c1 = a.downcast_ref::<Counter>().expect("counter");
            let c2 = b.downcast_ref::<Counter>().expect("counter");
            c1 == c2
        });
        let hash_fn: crate::types::ty::CapsuleHashFn = std::sync::Arc::new(|a| {
            let c = a.downcast_ref::<Counter>().expect("counter");
            c.0 as u64
        });

        let mut ops = crate::types::ty::CapsuleOps::new("Counter", eq_fn, hash_fn);
        let inc_method: crate::types::ty::CapsuleMethodFn = std::sync::Arc::new(|any, args| {
            let c = any
                .downcast_ref::<Counter>()
                .ok_or_else(|| crate::error::HclError::Capsule("invalid counter".into()))?;
            let delta = if args.is_empty() {
                1
            } else {
                match &*args[0].data {
                    ValueData::Number(n) => {
                        use bigdecimal::num_traits::ToPrimitive;
                        n.0.to_i32()
                            .ok_or_else(|| crate::error::HclError::Capsule("overflow".into()))?
                    }
                    _ => return Err(crate::error::HclError::Capsule("expected number".into())),
                }
            };
            Ok(Value::new(
                Type::Number,
                ValueData::Number((c.0 + delta).into()),
            ))
        });
        ops = ops.with_method("increment", inc_method);

        let c1 = Counter(10);
        let c2 = Counter(10);
        let c3 = Counter(20);
        assert!((ops.equals)(&c1, &c2));
        assert!(!(ops.equals)(&c1, &c3));
        assert_eq!((ops.hash)(&c1), (ops.hash)(&c2));

        let ops_arc = std::sync::Arc::new(ops);
        let cap_ty = Type::capsule_with_ops::<Counter>("Counter", ops_arc.clone());
        let cap_val = Value::new(cap_ty, ValueData::Capsule(std::sync::Arc::new(Counter(10))));

        // 1. Valid method invocation
        let delta_val = Value::new(Type::Number, ValueData::Number(5.into()));
        let res = cap_val
            .call_method("increment", &[delta_val])
            .expect("method call ok");
        assert_eq!(res.to_string(), "15");

        // 2. Unknown argument propagation
        let unk_arg = Value::unknown(Type::Number);
        let unk_res = cap_val
            .call_method("increment", &[unk_arg])
            .expect("unknown propagation ok");
        assert!(unk_res.is_unknown());

        // 3. Mark propagation
        let marked_cap = cap_val.mark(ValueMark::Sensitive);
        let marked_res = marked_cap
            .call_method("increment", &[])
            .expect("call with default arg ok");
        assert!(marked_res.is_sensitive());
        assert_eq!(marked_res.to_string(), "(sensitive value)");
        assert_eq!(marked_res.unmark().0.to_string(), "11");

        // 4. Method not found error
        let err = cap_val
            .call_method("decrement", &[])
            .expect_err("should not find decrement");
        assert_eq!(
            err,
            crate::error::HclError::CapsuleMethodNotFound {
                capsule_type: "Counter",
                method: "decrement".to_string(),
            }
        );

        // 5. Calling method on non-capsule type
        let str_val = Value::new(Type::String, ValueData::String("text".into()));
        assert!(str_val.call_method("trim", &[]).is_err());

        // 6. Calling method on capsule without ops
        let no_ops_ty = Type::capsule::<Counter>("Counter");
        let no_ops_val = Value::new(
            no_ops_ty,
            ValueData::Capsule(std::sync::Arc::new(Counter(10))),
        );
        assert!(no_ops_val.call_method("increment", &[]).is_err());

        // 7. Method execution error paths
        let str_arg = Value::new(Type::String, ValueData::String("not_a_num".into()));
        let err_bad_arg = cap_val.call_method("increment", &[str_arg]);
        assert!(err_bad_arg.is_err());

        let huge_num = Value::new(
            Type::Number,
            ValueData::Number(
                crate::number::Number::from(i64::MAX) * crate::number::Number::from(i64::MAX),
            ),
        );
        let err_overflow = cap_val.call_method("increment", &[huge_num]);
        assert!(err_overflow.is_err());

        let bad_any = "not a counter";
        assert!((ops_arc.methods["increment"])(&bad_any, &[]).is_err());
    }
}
