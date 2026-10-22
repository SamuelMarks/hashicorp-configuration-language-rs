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
            Self::Null => write!(f, "Null"),
            Self::Unknown(None) => write!(f, "Unknown"),
            Self::Unknown(Some(r)) => write!(f, "Unknown({r:?})"),
            Self::Bool(b) => f.debug_tuple("Bool").field(b).finish(),
            Self::Number(n) => f.debug_tuple("Number").field(n).finish(),
            Self::String(s) => f.debug_tuple("String").field(s).finish(),
            Self::Array(a) => f.debug_tuple("Array").field(a).finish(),
            Self::Set(s) => f.debug_tuple("Set").field(s).finish(),
            Self::Object(o) => f.debug_tuple("Object").field(o).finish(),
            Self::Capsule(c) => write!(f, "Capsule({:p})", Arc::as_ptr(c)),
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
                    (Arc::as_ptr(&self.0).cast::<()>() as usize)
                        .cmp(&(Arc::as_ptr(&other.0).cast::<()>() as usize))
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
        other
            .as_any()
            .downcast_ref::<T>()
            .is_some_and(|other_concrete| self == other_concrete)
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
    /// Indicates that a value is ephemeral and cannot be persisted into long-lived state or files.
    Ephemeral,
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
            Self::Ephemeral => write!(f, "ephemeral"),
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
            (Self::Null, Self::Null) => true,
            (Self::Unknown(a), Self::Unknown(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => a == b,
            (Self::Set(a), Self::Set(b)) => a == b,
            (Self::Object(a), Self::Object(b)) => a == b,
            (Self::Capsule(a), Self::Capsule(b)) => Arc::ptr_eq(a, b),
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
        if let (ValueData::Capsule(a), ValueData::Capsule(b)) = (&*self.data, &*other.data) {
            if let Some(ops) = self.ty.capsule_ops() {
                return (ops.equals)(&**a, &**b);
            }
        }
        self.data == other.data
    }
}
impl Eq for Value {}
impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ty.hash(state);
        self.marks.hash(state);
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
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ty
            .is_dynamic()
            .cmp(&other.ty.is_dynamic())
            .then_with(|| self.marks.cmp(&other.marks))
            .then_with(|| match (&*self.data, &*other.data) {
                (ValueData::String(a), ValueData::String(b)) => a.cmp(b),
                (ValueData::Number(a), ValueData::Number(b)) => a.cmp(b),
                (ValueData::Capsule(a), ValueData::Capsule(b)) => self
                    .ty
                    .capsule_ops()
                    .and_then(|ops| ops.cmp.as_ref())
                    .and_then(|cmp_fn| (cmp_fn)(&**a, &**b).ok())
                    .unwrap_or(std::cmp::Ordering::Equal),
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
    pub const fn marks(&self) -> &BTreeSet<ValueMark> {
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

    /// Checks if this value is marked as ephemeral.
    #[must_use]
    pub fn is_ephemeral(&self) -> bool {
        self.has_mark(&ValueMark::Ephemeral)
    }

    /// Returns a new value with the `Ephemeral` mark attached.
    #[must_use]
    pub fn mark_ephemeral(&self) -> Self {
        self.mark(ValueMark::Ephemeral)
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
    pub fn call_method(&self, name: &str, args: &[Self]) -> Result<Self, crate::error::HclError> {
        let (Some(any), Some(ops)) = (self.as_capsule_any(), self.ty().capsule_ops()) else {
            return Err(crate::error::HclError::Capsule(format!(
                "cannot call method '{name}' on non-capsule type {}",
                self.ty()
            )));
        };
        if args.iter().any(Self::is_unknown) {
            return Ok(Self::unknown(Type::Dynamic));
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
            return Ok(self.unmark());
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
                let ValueData::Number(n) = &*idx_val.data else {
                    return Err(HclError::PathError {
                        path: full_path.to_string(),
                        message: "Index step must evaluate to a number".to_string(),
                    });
                };
                let idx = n.0.to_usize().ok_or_else(|| HclError::PathError {
                    path: full_path.to_string(),
                    message: "Index must be a valid integer".to_string(),
                })?;
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
                let ValueData::String(key) = &*key_val.data else {
                    return Err(HclError::PathError {
                        path: full_path.to_string(),
                        message: "Key step must evaluate to a string".to_string(),
                    });
                };
                if let ValueData::Object(map) = &*self.data {
                    let child = map.get(key.as_str()).ok_or_else(|| HclError::PathError {
                        path: full_path.to_string(),
                        message: format!("Key '{key}' not found"),
                    })?;
                    let (updated_child, removed_marks) =
                        child.unmark_path_steps(rest, full_path)?;
                    let mut new_map = map.clone();
                    new_map.insert(key.clone(), updated_child);
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
    pub const fn ty(&self) -> &Type {
        &self.ty
    }
    /// Returns true if this value is unknown.
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
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
    pub const fn is_null(&self) -> bool {
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
        let Some(any) = self.as_capsule_any() else {
            return Err(HclError::CapsuleDowncast {
                expected: std::any::type_name::<T>(),
                actual: self.ty.to_string(),
            });
        };
        any.downcast_ref::<T>()
            .ok_or_else(|| HclError::CapsuleDowncast {
                expected: std::any::type_name::<T>(),
                actual: self.ty.to_string(),
            })
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
    pub fn get_path(&self, path: &crate::types::path::Path) -> Result<Self, HclError> {
        let mut curr = self.clone();
        for step in path.steps() {
            curr = match step {
                crate::types::path::PathStep::GetAttr(attr) => match &*curr.data {
                    ValueData::Object(map) => {
                        if let Some(val) = map.get(attr) {
                            val.clone()
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
                                val.clone()
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
                                val.clone()
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
            };
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
    pub fn transform<F>(&self, f: &mut F) -> Result<Self, HclError>
    where
        F: FnMut(&crate::types::path::Path, &Self) -> Result<Self, HclError>,
    {
        let mut path = crate::types::path::Path::empty();
        self.transform_internal(&mut path, f)
    }
    fn transform_internal<F>(
        &self,
        path: &mut crate::types::path::Path,
        f: &mut F,
    ) -> Result<Self, HclError>
    where
        F: FnMut(&crate::types::path::Path, &Self) -> Result<Self, HclError>,
    {
        match &*self.data {
            ValueData::Array(arr) => {
                let mut new_arr = Vec::with_capacity(arr.len());
                for (idx, elem) in arr.iter().enumerate() {
                    let idx_val = Self::new(
                        Type::Number,
                        ValueData::Number(crate::number::Number::from(
                            u64::try_from(idx).unwrap_or(u64::MAX),
                        )),
                    );
                    path.push(crate::types::path::PathStep::Index(idx_val));
                    let transformed_elem = elem.transform_internal(path, f)?;
                    path.pop();
                    new_arr.push(transformed_elem);
                }
                let intermediate = Self::new(self.ty().clone(), ValueData::Array(new_arr));
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
                let intermediate = Self::new(self.ty().clone(), ValueData::Object(new_map));
                f(path, &intermediate)
            }
            ValueData::Set(set) => {
                let mut new_set = std::collections::BTreeSet::new();
                for elem in set {
                    let transformed_elem = elem.transform_internal(path, f)?;
                    new_set.insert(transformed_elem);
                }
                let intermediate = Self::new(self.ty().clone(), ValueData::Set(new_set));
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
        F: FnMut(&crate::types::path::Path, &Self) -> Result<bool, HclError>,
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
        F: FnMut(&crate::types::path::Path, &Self) -> Result<bool, HclError>,
    {
        if !f(path, self)? {
            return Ok(());
        }
        match &*self.data {
            ValueData::Array(arr) => {
                for (idx, elem) in arr.iter().enumerate() {
                    let idx_val = Self::new(
                        Type::Number,
                        ValueData::Number(crate::number::Number::from(
                            u64::try_from(idx).unwrap_or(u64::MAX),
                        )),
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
    /// Coerces an unknown value to the target type, validating and adjusting refinements.
    fn coerce_unknown(&self, target: &Type) -> Result<Self, HclError> {
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
                            return Err(HclError::Type(format!(
                                "cannot coerce string with prefix {p:?} to number"
                            )));
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
            return Ok(Self::unknown_refined(target.clone(), new_r).with_marks(self.marks.clone()));
        }
        Ok(Self::unknown(target.clone()).with_marks(self.marks.clone()))
    }
    /// Attempts to coerce this value to the given target type.
    ///
    /// # Arguments
    /// * `target` - The target [`Type`] to coerce to.
    ///
    /// # Errors
    /// Returns [`HclError::Type`] on failure.
    pub fn coerce(&self, target: &Type) -> Result<Self, HclError> {
        if &self.ty == target || self.ty.is_dynamic() || target.is_dynamic() {
            return Ok(self.clone());
        }
        if let Some(ops) = self.ty.capsule_ops() {
            if let ValueData::Capsule(a) = &*self.data {
                if let Some(ref conv) = ops.conversion_to {
                    if let Some(val) = (conv)(&**a, target) {
                        return Ok(val.with_marks(self.marks.clone()));
                    }
                }
            }
        }
        if let Some(ops) = target.capsule_ops() {
            if let Some(ref conv) = ops.conversion_from {
                if let Some(arc) = (conv)(self) {
                    return Ok(Self::new(target.clone(), ValueData::Capsule(arc))
                        .with_marks(self.marks.clone()));
                }
            }
        }
        if self.ty.is_capsule() || target.is_capsule() {
            return Err(HclError::Type(format!(
                "cannot coerce {} to {}",
                self.ty, target
            )));
        }
        if self.is_unknown() {
            return self.coerce_unknown(target);
        }
        if self.is_null() {
            return Ok(Self::null(target.clone()).with_marks(self.marks.clone()));
        }
        let mut coerced = match (&self.ty, target) {
            (Type::String, Type::Number) => {
                if let ValueData::String(s) = &*self.data {
                    use std::str::FromStr;
                    let num = crate::number::Number::from_str(s)
                        .map_err(|_| HclError::Type(format!("cannot parse '{s}' as number")))?;
                    Ok(Self::new(Type::Number, ValueData::Number(num)))
                } else {
                    Err(HclError::Type(
                        "Mismatched type and data for string".to_string(),
                    ))
                }
            }
            (Type::Number, Type::String) => {
                if let ValueData::Number(n) = &*self.data {
                    Ok(Self::new(Type::String, ValueData::String(n.0.to_string())))
                } else {
                    Err(HclError::Type(
                        "Mismatched type and data for number".to_string(),
                    ))
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
                    Err(HclError::Type(
                        "Mismatched type and data for bool".to_string(),
                    ))
                }
            }
            (Type::String, Type::Bool) => {
                if let ValueData::String(s) = &*self.data {
                    match s.as_str() {
                        "true" => Ok(Self::new(Type::Bool, ValueData::Bool(true))),
                        "false" => Ok(Self::new(Type::Bool, ValueData::Bool(false))),
                        _ => Err(HclError::Type(format!("cannot parse '{s}' as bool"))),
                    }
                } else {
                    Err(HclError::Type(
                        "Mismatched type and data for string".to_string(),
                    ))
                }
            }
            _ => Err(HclError::Type(format!(
                "cannot coerce {} to {}",
                self.ty, target
            ))),
        }?;
        coerced.marks.extend(self.marks.clone());
        Ok(coerced)
    }
}
#[cfg(test)]
#[path = "val_tests.rs"]
mod tests;
