//! Environment and system standard library functions.
//!
//! Provides the `env(var_name)` built-in standard library function along with the
//! [`Environment`] abstraction for isolated and mockable process environment variable lookups.
use crate::eval::func::Function;
use crate::types::ty::Type;
use crate::types::val::{Value, ValueData};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
/// An abstraction over environment variable lookup.
pub trait Environment: std::fmt::Debug + Send + Sync {
    /// Look up an environment variable by name.
    ///
    /// # Arguments
    /// * `name` - The environment variable name.
    fn get_var(&self, name: &str) -> Option<String>;
}
/// Default system environment provider reading from `std::env::var`.
#[derive(Debug)]
pub struct SystemEnvironment;
impl Environment for SystemEnvironment {
    fn get_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}
/// In-memory mock environment provider for unit tests and sandboxed evaluation.
#[derive(Debug)]
pub struct MockEnvironment {
    vars: RwLock<HashMap<String, String>>,
}
impl MockEnvironment {
    /// Creates a new, empty `MockEnvironment`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            vars: RwLock::new(HashMap::new()),
        }
    }
    /// Sets an environment variable in the mock store.
    ///
    /// # Arguments
    /// * `name` - The environment variable name.
    /// * `value` - The value to associate with the variable.
    pub fn set_var(&self, name: impl Into<String>, value: impl Into<String>) {
        let mut lock = self
            .vars
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lock.insert(name.into(), value.into());
    }
    /// Removes an environment variable from the mock store.
    ///
    /// # Arguments
    /// * `name` - The environment variable name to remove.
    pub fn remove_var(&self, name: &str) {
        let mut lock = self
            .vars
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lock.remove(name);
    }
}
impl Default for MockEnvironment {
    fn default() -> Self {
        Self::new()
    }
}
impl Environment for MockEnvironment {
    fn get_var(&self, name: &str) -> Option<String> {
        let lock = self
            .vars
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lock.get(name).cloned()
    }
}
/// Returns standard library system and environment functions using the default system provider.
#[must_use]
pub fn functions() -> Vec<Function> {
    vec![env_func()]
}
/// Returns the standard `env` function using the operating system environment.
#[must_use]
pub fn env_func() -> Function {
    env_func_with_provider(Arc::new(SystemEnvironment))
}
/// Constructs the `env` function backed by a specific [`Environment`] provider.
///
/// # Arguments
/// * `provider` - The environment provider implementation.
#[must_use]
pub fn env_func_with_provider(provider: Arc<dyn Environment>) -> Function {
    Function {
        name: "env".to_string(),
        func: Arc::new(move |args: &[Value]| -> Result<Value, String> {
            if args.is_empty() || args.len() > 2 {
                return Err(format!("env expects 1 or 2 arguments, got {}", args.len()));
            }
            if args[0].is_unknown() {
                return Ok(Value::unknown(Type::String));
            }
            let var_name = match &*args[0].data {
                ValueData::String(s) => s.as_str(),
                _ => {
                    return Err(format!(
                        "env expects a string variable name, got {}",
                        args[0].ty()
                    ));
                }
            };
            let fallback = if args.len() == 2 {
                if args[1].is_unknown() {
                    return Ok(Value::unknown(Type::String));
                }
                match &*args[1].data {
                    ValueData::String(s) => s.clone(),
                    _ => {
                        return Err(format!(
                            "env fallback must be a string, got {}",
                            args[1].ty()
                        ));
                    }
                }
            } else {
                String::new()
            };
            let result = provider.get_var(var_name).unwrap_or(fallback);
            Ok(Value::new(Type::String, ValueData::String(result)))
        }),
        signature: None,
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
    #[test]
    fn test_system_and_mock_environment() {
        assert_eq!(functions().len(), 1);
        let sys = SystemEnvironment;
        let _ = format!("{sys:?}");
        let mock_def = MockEnvironment::default();
        let _ = format!("{mock_def:?}");
        let mock = Arc::new(MockEnvironment::new());
        let _ = format!("{mock:?}");
        mock.set_var("FOO", "bar");
        assert_eq!(mock.get_var("FOO"), Some("bar".to_string()));
        assert_eq!(mock.get_var("MISSING"), None);
        let env_fn = env_func_with_provider(mock.clone());
        let res =
            (env_fn.func)(&[Value::new(Type::String, ValueData::String("FOO".into()))]).unwrap();
        assert_eq!(res.data.as_ref(), &ValueData::String("bar".to_string()));
        let res_existing_fb = (env_fn.func)(&[
            Value::new(Type::String, ValueData::String("FOO".into())),
            Value::new(Type::String, ValueData::String("ignored".into())),
        ])
        .unwrap();
        assert_eq!(
            res_existing_fb.data.as_ref(),
            &ValueData::String("bar".to_string())
        );
        let res_missing = (env_fn.func)(&[Value::new(
            Type::String,
            ValueData::String("MISSING".into()),
        )])
        .unwrap();
        assert_eq!(res_missing.data.as_ref(), &ValueData::String(String::new()));
        let res_fb = (env_fn.func)(&[
            Value::new(Type::String, ValueData::String("MISSING".into())),
            Value::new(Type::String, ValueData::String("fallback_val".into())),
        ])
        .unwrap();
        assert_eq!(
            res_fb.data.as_ref(),
            &ValueData::String("fallback_val".to_string())
        );
        mock.remove_var("FOO");
        assert_eq!(mock.get_var("FOO"), None);
        let res_unk = (env_fn.func)(&[Value::unknown(Type::String)]).unwrap();
        assert!(res_unk.is_unknown());
        let res_unk_fb = (env_fn.func)(&[
            Value::new(Type::String, ValueData::String("VAR".into())),
            Value::unknown(Type::String),
        ])
        .unwrap();
        assert!(res_unk_fb.is_unknown());
        assert!((env_fn.func)(&[]).is_err());
        assert!(
            (env_fn.func)(&[
                Value::new(Type::String, ValueData::String("A".into())),
                Value::new(Type::String, ValueData::String("B".into())),
                Value::new(Type::String, ValueData::String("C".into())),
            ])
            .is_err()
        );
        assert!((env_fn.func)(&[Value::new(Type::Number, ValueData::Bool(true))]).is_err());
        assert!(
            (env_fn.func)(&[
                Value::new(Type::String, ValueData::String("A".into())),
                Value::new(Type::Bool, ValueData::Bool(true)),
            ])
            .is_err()
        );
        let sys_fn = env_func();
        let _ = (sys_fn.func)(&[Value::new(Type::String, ValueData::String("PATH".into()))]);
    }
}
