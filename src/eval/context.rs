use crate::types::Value;
use std::collections::HashMap;
/// A context or scope for evaluating HCL expressions.
///
/// Contains variables and functions that can be resolved during evaluation.
/// Supports a hierarchical structure via an optional parent context.
#[derive(Debug, Clone)]
pub struct Context<'a> {
    /// Parent context for fallback resolution.
    pub parent: Option<&'a Context<'a>>,
    /// Variables defined in this scope.
    pub variables: HashMap<String, Value>,
    /// Functions defined in this scope.
    pub functions: HashMap<String, crate::eval::func::Function>,
    /// Namespaced functions defined in this scope, keyed by (`namespace_segments`, name).
    pub namespaced_functions: HashMap<(Vec<String>, String), crate::eval::func::Function>,
    /// Whether standard library functions are enabled as fallback.
    pub stdlib_enabled: bool,
    /// Pluggable virtual filesystem implementation.
    pub fs: std::sync::Arc<dyn crate::eval::fs::FileSystem>,
}
impl Default for Context<'_> {
    fn default() -> Self {
        Self::new()
    }
}
impl<'a> Context<'a> {
    /// Create a new, empty Context without standard library functions.
    #[must_use]
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
            functions: HashMap::new(),
            namespaced_functions: HashMap::new(),
            stdlib_enabled: false,
            fs: std::sync::Arc::new(crate::eval::fs::OsFileSystem),
        }
    }
    /// Create a new Context with all standard library functions enabled.
    #[must_use]
    pub fn with_stdlib() -> Self {
        let mut ctx = Self {
            parent: None,
            variables: HashMap::new(),
            functions: HashMap::new(),
            namespaced_functions: HashMap::new(),
            stdlib_enabled: true,
            fs: std::sync::Arc::new(crate::eval::fs::OsFileSystem),
        };
        ctx.register_filesystem_functions();
        ctx
    }
    /// Create a new Context with a parent, inheriting the parent's stdlib settings and filesystem.
    #[must_use]
    pub fn new_child(parent: &'a Context<'a>) -> Self {
        Self {
            parent: Some(parent),
            variables: HashMap::new(),
            functions: HashMap::new(),
            namespaced_functions: HashMap::new(),
            stdlib_enabled: parent.stdlib_enabled,
            fs: parent.fs.clone(),
        }
    }
    /// Configures this context with a custom virtual filesystem implementation.
    ///
    /// # Arguments
    /// * `fs` - The virtual filesystem implementation.
    #[must_use]
    pub fn with_filesystem(mut self, fs: std::sync::Arc<dyn crate::eval::fs::FileSystem>) -> Self {
        self.fs = fs;
        self.register_filesystem_functions();
        self
    }
    /// Returns a reference to the active virtual filesystem.
    #[must_use]
    pub fn filesystem(&self) -> &std::sync::Arc<dyn crate::eval::fs::FileSystem> {
        &self.fs
    }
    /// Enable standard library functions using builder style.
    #[must_use]
    pub fn enable_stdlib(mut self) -> Self {
        self.stdlib_enabled = true;
        self.register_filesystem_functions();
        self
    }
    /// Enable standard library functions on this context in-place.
    pub fn register_stdlib(&mut self) {
        self.stdlib_enabled = true;
        self.register_filesystem_functions();
    }
    /// Enable collection functions on this context using builder style.
    #[must_use]
    pub fn with_collection_functions(mut self) -> Self {
        self.register_collection_functions();
        self
    }
    /// Register all collection standard library functions in this context.
    pub fn register_collection_functions(&mut self) {
        for func in crate::eval::stdlib::collection::functions() {
            self.set_function(func.name.clone(), func);
        }
    }
    /// Enable conversion functions on this context using builder style.
    #[must_use]
    pub fn with_conversion_functions(mut self) -> Self {
        self.register_conversion_functions();
        self
    }
    /// Register all conversion standard library functions in this context.
    pub fn register_conversion_functions(&mut self) {
        for func in crate::eval::stdlib::conversion::functions() {
            self.set_function(func.name.clone(), func);
        }
    }
    /// Enable crypto functions on this context using builder style.
    #[must_use]
    pub fn with_crypto_functions(mut self) -> Self {
        self.register_crypto_functions();
        self
    }
    /// Register all crypto standard library functions in this context.
    pub fn register_crypto_functions(&mut self) {
        for func in crate::eval::stdlib::crypto::functions() {
            self.set_function(func.name.clone(), func);
        }
    }
    /// Enable datetime functions on this context using builder style.
    #[must_use]
    pub fn with_datetime_functions(mut self) -> Self {
        self.register_datetime_functions();
        self
    }
    /// Register all datetime standard library functions in this context.
    pub fn register_datetime_functions(&mut self) {
        for func in crate::eval::stdlib::datetime::functions() {
            self.set_function(func.name.clone(), func);
        }
    }
    /// Enable encoding functions on this context using builder style.
    #[must_use]
    pub fn with_encoding_functions(mut self) -> Self {
        self.register_encoding_functions();
        self
    }
    /// Register all encoding standard library functions in this context.
    pub fn register_encoding_functions(&mut self) {
        for func in crate::eval::stdlib::encoding::functions() {
            self.set_function(func.name.clone(), func);
        }
    }
    /// Enable filesystem and template functions on this context using builder style.
    #[must_use]
    pub fn with_filesystem_functions(mut self) -> Self {
        self.register_filesystem_functions();
        self
    }
    /// Register all filesystem and template standard library functions in this context.
    pub fn register_filesystem_functions(&mut self) {
        for func in crate::eval::stdlib::filesystem::functions_with_fs(self.fs.clone()) {
            self.set_function(func.name.clone(), func);
        }
    }
    /// Enable network functions on this context using builder style.
    #[must_use]
    pub fn with_network_functions(mut self) -> Self {
        self.register_network_functions();
        self
    }
    /// Register all network standard library functions in this context.
    pub fn register_network_functions(&mut self) {
        for func in crate::eval::stdlib::network::functions() {
            self.set_function(func.name.clone(), func);
        }
    }
    /// Enable numeric functions on this context using builder style.
    #[must_use]
    pub fn with_numeric_functions(mut self) -> Self {
        self.register_numeric_functions();
        self
    }
    /// Register all numeric standard library functions in this context.
    pub fn register_numeric_functions(&mut self) {
        for func in crate::eval::stdlib::numeric::functions() {
            self.set_function(func.name.clone(), func);
        }
    }
    /// Enable math functions on this context (alias for numeric functions) using builder style.
    #[must_use]
    pub fn with_math_functions(self) -> Self {
        self.with_numeric_functions()
    }
    /// Register all math standard library functions in this context (alias for numeric functions).
    pub fn register_math_functions(&mut self) {
        self.register_numeric_functions();
    }
    /// Enable string functions on this context using builder style.
    #[must_use]
    pub fn with_string_functions(mut self) -> Self {
        self.register_string_functions();
        self
    }
    /// Register all string standard library functions in this context.
    pub fn register_string_functions(&mut self) {
        for func in crate::eval::stdlib::string::functions() {
            self.set_function(func.name.clone(), func);
        }
    }
    /// Retrieve a variable by name from this context or its parents.
    #[must_use]
    pub fn get_variable(&self, name: &str) -> Option<&Value> {
        if let Some(val) = self.variables.get(name) {
            Some(val)
        } else if let Some(parent) = self.parent {
            parent.get_variable(name)
        } else {
            None
        }
    }
    /// Set a variable in the current context.
    pub fn set_variable(&mut self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }
    /// Sets a variable within a named namespace object (such as `"var"`, `"local"`, or `"path"`).
    ///
    /// If the namespace object does not exist in `variables`, a new object is initialized.
    ///
    /// # Arguments
    /// * `namespace` - The namespace name (e.g. `"var"`, `"local"`).
    /// * `name` - The attribute name within the namespace.
    /// * `value` - The value to store.
    pub fn set_namespace_variable(
        &mut self,
        namespace: &str,
        name: impl Into<String>,
        value: Value,
    ) {
        let name_str = name.into();
        if let Some(ns_val) = self.variables.get_mut(namespace) {
            if let crate::types::ValueData::Object(map) = &mut *ns_val.data {
                map.insert(name_str, value);
                return;
            }
        }
        let mut map = std::collections::BTreeMap::new();
        map.insert(name_str, value);
        self.variables.insert(
            namespace.to_string(),
            Value::new(
                crate::types::Type::object(std::collections::BTreeMap::new()),
                crate::types::ValueData::Object(map),
            ),
        );
    }
    /// Sets a variable in the `"var"` namespace scope (`var.<name>`).
    ///
    /// # Arguments
    /// * `name` - The variable name.
    /// * `value` - The variable value.
    pub fn set_var(&mut self, name: impl Into<String>, value: Value) {
        self.set_namespace_variable("var", name, value);
    }
    /// Sets a local variable in the `"local"` namespace scope (`local.<name>`).
    ///
    /// # Arguments
    /// * `name` - The local variable name.
    /// * `value` - The local value.
    pub fn set_local(&mut self, name: impl Into<String>, value: Value) {
        self.set_namespace_variable("local", name, value);
    }
    /// Configures the `"path"` namespace scope with standard filesystem paths (`path.root`, `path.cwd`).
    ///
    /// # Arguments
    /// * `root` - The root directory of the configuration.
    /// * `cwd` - The current working directory.
    pub fn set_path_scopes(&mut self, root: impl Into<String>, cwd: impl Into<String>) {
        self.set_namespace_variable(
            "path",
            "root",
            Value::new(
                crate::types::Type::String,
                crate::types::ValueData::String(root.into()),
            ),
        );
        self.set_namespace_variable(
            "path",
            "cwd",
            Value::new(
                crate::types::Type::String,
                crate::types::ValueData::String(cwd.into()),
            ),
        );
    }
    /// Sets a deeply nested variable path (e.g. `["source", "qemu", "vm"]`).
    ///
    /// # Arguments
    /// * `segments` - The slice of path segment identifiers.
    /// * `value` - The value to assign at the leaf position.
    pub fn set_nested_scope(&mut self, segments: &[&str], value: Value) {
        if segments.is_empty() {
            return;
        }
        if segments.len() == 1 {
            self.set_variable(segments[0], value);
            return;
        }
        let mut curr = value;
        for &seg in segments[1..].iter().rev() {
            let mut map = std::collections::BTreeMap::new();
            map.insert(seg.to_string(), curr);
            curr = Value::new(
                crate::types::Type::object(std::collections::BTreeMap::new()),
                crate::types::ValueData::Object(map),
            );
        }
        let root = segments[0];
        if let Some(existing) = self.variables.get_mut(root) {
            merge_nested_values(existing, curr);
        } else {
            self.variables.insert(root.to_string(), curr);
        }
    }
    /// Returns the names of all variables accessible from this context and its parents.
    #[must_use]
    pub fn variable_names(&self) -> Vec<String> {
        let mut names = std::collections::BTreeSet::new();
        if let Some(parent) = self.parent {
            names.extend(parent.variable_names());
        }
        names.extend(self.variables.keys().cloned());
        names.into_iter().collect()
    }
    /// Retrieve a function by name from this context, its parents, or stdlib if enabled.
    ///
    /// If the name contains `::`, delegates to [`get_namespaced_func`](Self::get_namespaced_func).
    #[must_use]
    pub fn get_function(&self, name: &str) -> Option<&crate::eval::func::Function> {
        if name.contains("::") {
            let ident = crate::ast::expr::NamespacedIdent::parse(
                name,
                crate::span::Span::new(0, 0, 0, 0, 0, 0),
            );
            return self.get_namespaced_func(&ident);
        }
        if let Some(func) = self.functions.get(name) {
            Some(func)
        } else if let Some(parent) = self.parent {
            parent.get_function(name)
        } else if self.stdlib_enabled {
            crate::eval::stdlib::get_stdlib_function(name)
        } else {
            None
        }
    }
    /// Register a namespaced function within this context.
    ///
    /// # Arguments
    /// * `namespace` - Sequence of namespace segments (e.g. `&["provider", "aws"]`).
    /// * `name` - The base function name.
    /// * `func` - The [`Function`](crate::eval::func::Function) implementation.
    ///
    /// # Examples
    /// ```rust
    /// use hashicorp_configuration_language_rs::eval::context::Context;
    /// use hashicorp_configuration_language_rs::eval::func::Function;
    /// use hashicorp_configuration_language_rs::types::{Type, Value, ValueData};
    /// use std::sync::Arc;
    ///
    /// let mut ctx = Context::new();
    /// let f = Function {
    ///     name: "arn_parse".to_string(),
    ///     func: Arc::new(|_| Ok(Value::new(Type::String, ValueData::String("parsed".into())))),
    ///     signature: None,
    /// };
    /// ctx.set_namespaced_func(&["provider", "aws"], "arn_parse", f);
    /// assert!(ctx.has_namespaced_func(&["provider", "aws"], "arn_parse"));
    /// ```
    pub fn set_namespaced_func(
        &mut self,
        namespace: &[&str],
        name: &str,
        func: crate::eval::func::Function,
    ) {
        let ns: Vec<String> = namespace.iter().map(|s| (*s).to_string()).collect();
        self.namespaced_functions
            .insert((ns, name.to_string()), func);
    }
    /// Retrieve a namespaced function from this context or parent scopes.
    ///
    /// If the identifier has no namespace segments, falls back to unqualified [`get_function`](Self::get_function).
    ///
    /// # Arguments
    /// * `ident` - The namespaced identifier to look up.
    #[must_use]
    pub fn get_namespaced_func(
        &self,
        ident: &crate::ast::expr::NamespacedIdent,
    ) -> Option<&crate::eval::func::Function> {
        if ident.namespace.is_empty() {
            return self.get_function(&ident.name);
        }
        let key = (ident.namespace.clone(), ident.name.clone());
        if let Some(func) = self.namespaced_functions.get(&key) {
            Some(func)
        } else if let Some(parent) = self.parent {
            parent.get_namespaced_func(ident)
        } else {
            None
        }
    }
    /// Check whether a namespaced function is defined in this context or its parent scopes.
    ///
    /// # Arguments
    /// * `namespace` - Sequence of namespace segments.
    /// * `name` - Base function name.
    #[must_use]
    pub fn has_namespaced_func(&self, namespace: &[&str], name: &str) -> bool {
        let ident = crate::ast::expr::NamespacedIdent::new(
            namespace.iter().map(|s| (*s).to_string()).collect(),
            name,
            crate::span::Span::new(0, 0, 0, 0, 0, 0),
        );
        self.get_namespaced_func(&ident).is_some()
    }
    /// Check whether a function is available in this context, its parents, or stdlib if enabled.
    #[must_use]
    pub fn has_function(&self, name: &str) -> bool {
        self.get_function(name).is_some()
    }
    /// Returns the names of all functions accessible from this context.
    #[must_use]
    pub fn function_names(&self) -> Vec<String> {
        let mut names = std::collections::BTreeSet::new();
        if self.stdlib_enabled {
            names.extend(crate::eval::stdlib::stdlib_map().keys().cloned());
        }
        if let Some(parent) = self.parent {
            names.extend(parent.function_names());
        }
        names.extend(self.functions.keys().cloned());
        for (ns, name) in self.namespaced_functions.keys() {
            names.insert(format!("{}::{}", ns.join("::"), name));
        }
        names.into_iter().collect()
    }
    /// Set a function in the current context.
    pub fn set_function(&mut self, name: impl Into<String>, func: crate::eval::func::Function) {
        self.functions.insert(name.into(), func);
    }
    /// Registers a user-defined function block in this evaluation context,
    /// supporting positional parameters, optional variadic parameter, and return type constraint.
    ///
    /// # Arguments
    /// * `fb` - The [`FunctionBlock`](crate::ast::user_func::FunctionBlock) defining the function.
    pub fn register_function_block(&mut self, fb: &crate::ast::user_func::FunctionBlock) {
        let name = fb.name.clone();
        let params = fb.params.clone();
        let variadic_param = fb.variadic_param.clone();
        let return_type = fb.return_type.clone();
        let body_expr = fb.body.clone();
        let func = crate::eval::func::Function {
            name: name.clone(),
            func: std::sync::Arc::new(move |args: &[Value]| -> Result<Value, String> {
                let min_args = params.len();
                if variadic_param.is_none() {
                    if args.len() != min_args {
                        return Err(format!(
                            "Function '{name}' requires {min_args} arguments, got {}",
                            args.len()
                        ));
                    }
                } else if args.len() < min_args {
                    return Err(format!(
                        "Function '{name}' requires at least {min_args} arguments, got {}",
                        args.len()
                    ));
                }
                let mut sub_ctx = Context::with_stdlib();
                for (i, param) in params.iter().enumerate() {
                    let arg = &args[i];
                    if let Some(ref te) = param.type_expr {
                        let expected_ty = crate::eval::type_expr::eval_type_expr(te);
                        if let Err(e) = arg.coerce(&expected_ty) {
                            return Err(format!(
                                "type mismatch for parameter '{}': expected {expected_ty}, got {}: {e}",
                                param.name,
                                arg.ty()
                            ));
                        }
                    }
                    sub_ctx.set_variable(&param.name, arg.clone());
                }
                if let Some(ref vp) = variadic_param {
                    let rest_args = if args.len() > min_args {
                        args[min_args..].to_vec()
                    } else {
                        Vec::new()
                    };
                    if let Some(ref te) = vp.type_expr {
                        let expected_ty = crate::eval::type_expr::eval_type_expr(te);
                        for arg in &rest_args {
                            if let Err(e) = arg.coerce(&expected_ty) {
                                return Err(format!(
                                    "type mismatch for variadic parameter '{}': expected {expected_ty}, got {}: {e}",
                                    vp.name,
                                    arg.ty()
                                ));
                            }
                        }
                    }
                    let elem_types: Vec<crate::types::ty::Type> =
                        rest_args.iter().map(|v| v.ty().clone()).collect();
                    let variadic_val = Value::new(
                        crate::types::ty::Type::Tuple(elem_types),
                        crate::types::val::ValueData::Array(rest_args),
                    );
                    sub_ctx.set_variable(&vp.name, variadic_val);
                }
                let (result_val, _diags) = crate::eval::evaluator::Evaluator::new(&sub_ctx)
                    .evaluate(&body_expr)
                    .map_err(|d| format!("Evaluation failed: {:?}", d.errors()))?;
                if let Some(ref rt) = return_type {
                    let expected_return_ty = crate::eval::type_expr::eval_type_expr(rt);
                    result_val
                        .coerce(&expected_return_ty)
                        .map_err(|e| format!("Return type mismatch: {e}"))
                } else {
                    Ok(result_val)
                }
            }),
            signature: None,
        };
        self.set_function(fb.name.clone(), func);
    }
}
fn merge_nested_values(dest: &mut Value, src: Value) {
    if let (crate::types::ValueData::Object(d_map), crate::types::ValueData::Object(s_map)) =
        (&mut *dest.data, &*src.data)
    {
        for (k, v) in s_map.clone() {
            if let Some(existing_child) = d_map.get_mut(&k) {
                merge_nested_values(existing_child, v);
            } else {
                d_map.insert(k, v);
            }
        }
    } else {
        *dest = src;
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
    use crate::types::{Type, ValueData};
    #[test]
    fn test_context_variables() {
        let mut ctx = Context::new();
        ctx.set_variable("foo", Value::new(Type::Bool, ValueData::Bool(true)));
        ctx.set_variable(
            String::from("foo_str"),
            Value::new(Type::Bool, ValueData::Bool(true)),
        );
        assert!(ctx.get_variable("foo").is_some());
        assert!(ctx.get_variable("foo_str").is_some());
        assert!(ctx.get_variable("bar").is_none());
        let mut child = Context::new_child(&ctx);
        assert!(child.get_variable("foo").is_some());
        child.set_variable("bar", Value::new(Type::Bool, ValueData::Bool(false)));
        assert!(child.get_variable("bar").is_some());
        assert!(ctx.get_variable("bar").is_none());
        child.set_variable("foo", Value::unknown(Type::String));
        let shadowed = child.get_variable("foo").unwrap();
        assert!(shadowed.is_unknown());
        let original = ctx.get_variable("foo").unwrap();
        assert!(!original.is_unknown());
    }
    #[test]
    fn test_context_variables_fallback() {
        let mut ctx = Context::new();
        assert_eq!(ctx.variable_names(), Vec::<String>::new());
        ctx.set_variable("foo", Value::new(Type::Bool, ValueData::Bool(true)));
        assert_eq!(ctx.variable_names(), vec!["foo".to_string()]);
        let child = Context::new_child(&ctx);
        assert_eq!(child.get_variable("missing_everywhere"), None);
        let mut child2 = Context::new_child(&child);
        assert!(child2.get_variable("foo").is_some());
        assert!(child2.get_variable("bar").is_none());
        assert_eq!(child2.get_variable("missing_everywhere"), None);
        child2.set_variable("bar", Value::new(Type::Bool, ValueData::Bool(false)));
        assert!(child2.get_variable("bar").is_some());
    }
    #[test]
    fn test_context_scoped_namespaces() {
        let mut ctx = Context::new();
        ctx.set_var(
            "my_var",
            Value::new(Type::String, ValueData::String("val".into())),
        );
        ctx.set_var("second_var", Value::new(Type::Bool, ValueData::Bool(true)));
        ctx.set_local(
            "my_local",
            Value::new(Type::Number, ValueData::Number(42.into())),
        );
        ctx.set_path_scopes("/root/dir", "/cwd/dir");
        let var_val = ctx.get_variable("var").unwrap();
        assert!(var_val.to_string().contains("my_var"));
        assert!(var_val.to_string().contains("second_var"));
        let local_val = ctx.get_variable("local").unwrap();
        assert!(local_val.to_string().contains("my_local"));
        let path_val = ctx.get_variable("path").unwrap();
        assert!(path_val.to_string().contains("/root/dir"));
        assert!(path_val.to_string().contains("/cwd/dir"));
        ctx.set_nested_scope(&[], Value::new(Type::Bool, ValueData::Bool(false)));
        ctx.set_nested_scope(&["single"], Value::new(Type::Bool, ValueData::Bool(true)));
        assert!(ctx.get_variable("single").is_some());
        ctx.set_nested_scope(
            &["source", "qemu", "vm"],
            Value::new(Type::String, ValueData::String("qemu_img".into())),
        );
        ctx.set_nested_scope(
            &["source", "qemu", "arch"],
            Value::new(Type::String, ValueData::String("x86_64".into())),
        );
        let source_val = ctx.get_variable("source").unwrap();
        assert!(source_val.to_string().contains("qemu_img"));
        assert!(source_val.to_string().contains("x86_64"));
        ctx.set_nested_scope(
            &["overwrite_prim"],
            Value::new(Type::Number, ValueData::Number(10.into())),
        );
        ctx.set_nested_scope(
            &["overwrite_prim", "child"],
            Value::new(Type::Bool, ValueData::Bool(true)),
        );
        let prim_obj = ctx.get_variable("overwrite_prim").unwrap();
        assert!(prim_obj.to_string().contains("child"));
        ctx.set_variable("scalar_ns", Value::new(Type::Bool, ValueData::Bool(true)));
        ctx.set_namespace_variable(
            "scalar_ns",
            "key",
            Value::new(Type::String, ValueData::String("val".into())),
        );
        let scalar_obj = ctx.get_variable("scalar_ns").unwrap();
        assert!(scalar_obj.to_string().contains("key"));
    }
    #[test]
    fn test_context_functions_fallback() {
        use crate::eval::func::Function;
        use std::sync::Arc;
        let mut ctx = Context::new();
        let func = Function {
            name: "test_func".to_string(),
            func: Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
            signature: None,
        };
        ctx.set_function("test_func", func);
        let child = Context::new_child(&ctx);
        let child2 = Context::new_child(&child);
        let found = child2.get_function("test_func").unwrap();
        assert!((found.func)(&[]).is_ok());
        assert!(child2.get_function("missing_func").is_none());
        assert!(ctx.get_function("missing_func").is_none());
        let stdlib_ctx = Context::with_stdlib();
        assert!(stdlib_ctx.get_function("missing_stdlib_func").is_none());
        let mut no_stdlib_ctx = Context::new();
        no_stdlib_ctx.stdlib_enabled = false;
        assert!(no_stdlib_ctx.get_function("missing_func").is_none());
    }
    #[test]
    fn test_context_functions_call() {
        use crate::eval::func::Function;
        use std::sync::Arc;
        let mut ctx = Context::new();
        let func = Function {
            name: "test_func".to_string(),
            func: Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
            signature: None,
        };
        ctx.set_function("test_func", func);
        let f = ctx.get_function("test_func").unwrap();
        let _ = (f.func)(&[]);
        let func2 = Function {
            name: "other_func".to_string(),
            func: Arc::new(|_| Ok(Value::new(Type::String, ValueData::String("a".to_string())))),
            signature: None,
        };
        ctx.set_function("other_func", func2);
        let f2 = ctx.get_function("other_func").unwrap();
        let _ = (f2.func)(&[]);
    }
    #[test]
    fn test_context_functions() {
        use crate::eval::func::Function;
        use std::sync::Arc;
        let mut ctx = Context::new();
        let func = Function {
            name: "test_func".to_string(),
            func: Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
            signature: None,
        };
        ctx.set_function("test_func", func);
        let func_str = Function {
            name: "test_func_str".to_string(),
            func: Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
            signature: None,
        };
        ctx.set_function(String::from("test_func_str"), func_str);
        let f = ctx.get_function("test_func").unwrap();
        assert!((f.func)(&[]).is_ok());
        let f_str = ctx.get_function("test_func_str").unwrap();
        assert!((f_str.func)(&[]).is_ok());
        assert!(ctx.get_function("other_func").is_none());
        let mut child = Context::new_child(&ctx);
        assert!(child.get_function("test_func").is_some());
        assert!(child.get_function("other_func").is_none());
        let func2 = Function {
            name: "other_func".to_string(),
            func: Arc::new(|_| Ok(Value::new(Type::String, ValueData::String("a".to_string())))),
            signature: None,
        };
        child.set_function("other_func", func2);
        let f2 = child.get_function("other_func").unwrap();
        assert!((f2.func)(&[]).is_ok());
        assert!(ctx.get_function("other_func").is_none());
    }
    #[test]
    fn test_context_with_stdlib() {
        let ctx = Context::with_stdlib();
        assert!(ctx.stdlib_enabled);
        assert!(ctx.has_function("upper"));
        assert!(ctx.has_function("abs"));
        assert!(ctx.has_function("md5"));
        assert!(!ctx.has_function("nonexistent_fn_xyz"));
        let child = Context::new_child(&ctx);
        assert!(child.stdlib_enabled);
        assert!(child.has_function("upper"));
        let mut child_shadow = Context::new_child(&ctx);
        use crate::eval::func::Function;
        use std::sync::Arc;
        child_shadow.set_function(
            "upper",
            Function {
                name: "upper".to_string(),
                func: Arc::new(|_| {
                    Ok(Value::new(Type::String, ValueData::String("custom".into())))
                }),
                signature: None,
            },
        );
        let custom_fn = child_shadow.get_function("upper").unwrap();
        let res = (custom_fn.func)(&[]).unwrap();
        assert_eq!(*res.data, ValueData::String("custom".into()));
    }
    #[test]
    fn test_context_enable_and_register_stdlib() {
        let mut ctx = Context::new();
        assert!(!ctx.stdlib_enabled);
        assert!(!ctx.has_function("lower"));
        ctx.register_stdlib();
        assert!(ctx.stdlib_enabled);
        assert!(ctx.has_function("lower"));
        let ctx2 = Context::new().enable_stdlib();
        assert!(ctx2.stdlib_enabled);
        assert!(ctx2.has_function("lower"));
    }
    #[test]
    fn test_context_category_loaders() {
        let c_coll = Context::new().with_collection_functions();
        assert!(c_coll.has_function("flatten"));
        assert!(!c_coll.has_function("md5"));
        let c_conv = Context::new().with_conversion_functions();
        assert!(c_conv.has_function("tostring"));
        assert!(!c_conv.has_function("flatten"));
        let c_crypto = Context::new().with_crypto_functions();
        assert!(c_crypto.has_function("md5"));
        assert!(!c_crypto.has_function("formatdate"));
        let c_dt = Context::new().with_datetime_functions();
        assert!(c_dt.has_function("formatdate"));
        assert!(!c_dt.has_function("md5"));
        let c_enc = Context::new().with_encoding_functions();
        assert!(c_enc.has_function("base64encode"));
        assert!(!c_enc.has_function("cidrhost"));
        let c_fs = Context::new().with_filesystem_functions();
        assert!(c_fs.has_function("file"));
        assert!(!c_fs.has_function("cidrhost"));
        let c_net = Context::new().with_network_functions();
        assert!(c_net.has_function("cidrhost"));
        assert!(!c_net.has_function("abs"));
        let c_num = Context::new().with_numeric_functions();
        assert!(c_num.has_function("abs"));
        assert!(!c_num.has_function("upper"));
        let c_math = Context::new().with_math_functions();
        assert!(c_math.has_function("abs"));
        let c_str = Context::new().with_string_functions();
        assert!(c_str.has_function("upper"));
        assert!(!c_str.has_function("abs"));
    }
    #[test]
    fn test_context_category_registers() {
        let mut ctx = Context::new();
        ctx.register_collection_functions();
        assert!(ctx.has_function("flatten"));
        ctx.register_conversion_functions();
        assert!(ctx.has_function("tostring"));
        ctx.register_crypto_functions();
        assert!(ctx.has_function("md5"));
        ctx.register_datetime_functions();
        assert!(ctx.has_function("formatdate"));
        ctx.register_encoding_functions();
        assert!(ctx.has_function("base64encode"));
        ctx.register_filesystem_functions();
        assert!(ctx.has_function("file"));
        ctx.register_network_functions();
        assert!(ctx.has_function("cidrhost"));
        ctx.register_numeric_functions();
        assert!(ctx.has_function("abs"));
        ctx.register_math_functions();
        assert!(ctx.has_function("ceil"));
        ctx.register_string_functions();
        assert!(ctx.has_function("lower"));
    }
    #[test]
    fn test_context_function_names() {
        let mut ctx = Context::new();
        assert_eq!(ctx.function_names(), Vec::<String>::new());
        use crate::eval::func::Function;
        use std::sync::Arc;
        ctx.set_function(
            "my_fn",
            Function {
                name: "my_fn".to_string(),
                func: Arc::new(|_| Ok(Value::null(Type::Dynamic))),
                signature: None,
            },
        );
        let f_my = ctx.get_function("my_fn").unwrap();
        assert!((f_my.func)(&[]).is_ok());
        let names = ctx.function_names();
        assert_eq!(names, vec!["my_fn".to_string()]);
        let child = Context::new_child(&ctx);
        let child_names = child.function_names();
        assert_eq!(child_names, vec!["my_fn".to_string()]);
        let stdlib_ctx = Context::with_stdlib();
        let all_names = stdlib_ctx.function_names();
        assert!(all_names.len() >= 70);
        assert!(all_names.contains(&"upper".to_string()));
    }
    #[test]
    fn test_register_function_block_coverage() {
        use crate::ast::expr::Expression;
        use crate::span::Span;
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let mut ctx = Context::new();
        let fixed_param = crate::ast::user_func::FunctionParam {
            name: "prefix".to_string(),
            type_expr: Some(crate::ast::type_expr::TypeExpr::Primitive(
                Type::String,
                span.clone(),
            )),
            span: span.clone(),
        };
        let var_param = crate::ast::user_func::FunctionParam {
            name: "items".to_string(),
            type_expr: Some(crate::ast::type_expr::TypeExpr::Primitive(
                Type::String,
                span.clone(),
            )),
            span: span.clone(),
        };
        let body_expr = Expression::Variable("prefix".to_string(), span.clone());
        let fb_variadic = crate::ast::user_func::FunctionBlock {
            name: "custom_var".to_string(),
            params: vec![fixed_param],
            variadic_param: Some(var_param),
            return_type: Some(crate::ast::type_expr::TypeExpr::Primitive(
                Type::String,
                span.clone(),
            )),
            body: body_expr,
            span: span.clone(),
        };
        ctx.register_function_block(&fb_variadic);
        let f = ctx.get_function("custom_var").unwrap();
        assert!((f.func)(&[]).is_err());
        let res_min = (f.func)(&[Value::new(Type::String, ValueData::String("p".into()))]);
        assert_eq!(
            res_min,
            Ok(Value::new(Type::String, ValueData::String("p".into())))
        );
        let bad_var = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        let res_bad_var = (f.func)(&[
            Value::new(Type::String, ValueData::String("p".into())),
            bad_var,
        ]);
        assert!(res_bad_var.is_err());
        let fb_ret_mismatch = crate::ast::user_func::FunctionBlock {
            name: "ret_mismatch".to_string(),
            params: vec![],
            variadic_param: None,
            return_type: Some(crate::ast::type_expr::TypeExpr::Primitive(
                Type::Number,
                span.clone(),
            )),
            body: Expression::String("not_a_num".to_string(), span.clone()),
            span: span.clone(),
        };
        ctx.register_function_block(&fb_ret_mismatch);
        let f_mismatch = ctx.get_function("ret_mismatch").unwrap();
        assert!((f_mismatch.func)(&[]).is_err());
        assert!((f_mismatch.func)(&[Value::null(Type::Dynamic)]).is_err());
        let fb_eval_fail = crate::ast::user_func::FunctionBlock {
            name: "eval_fail".to_string(),
            params: vec![],
            variadic_param: None,
            return_type: None,
            body: Expression::Variable("nonexistent".to_string(), span.clone()),
            span,
        };
        ctx.register_function_block(&fb_eval_fail);
        let f_fail = ctx.get_function("eval_fail").unwrap();
        assert!((f_fail.func)(&[]).is_err());
    }
    #[test]
    fn test_context_coverage_gaps() {
        let ctx_def: Context = Context::default();
        assert_eq!(ctx_def.variable_names().len(), 0);
        let mem_fs: std::sync::Arc<dyn crate::eval::fs::FileSystem> =
            std::sync::Arc::new(crate::eval::fs::MemFileSystem::new());
        let ctx = Context::new()
            .with_filesystem(mem_fs.clone())
            .enable_stdlib();
        assert!(std::sync::Arc::ptr_eq(ctx.filesystem(), &mem_fs));
        assert!(ctx.has_function("upper"));
        let bare_ctx = Context::new();
        assert!(!bare_ctx.has_function("nonexistent"));
        assert!(bare_ctx.get_function("nonexistent").is_none());
        let dummy_func = crate::eval::func::Function {
            name: "dummy".to_string(),
            func: std::sync::Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
            signature: None,
        };
        assert_eq!(
            (dummy_func.func)(&[]).ok(),
            Some(Value::new(Type::Bool, ValueData::Bool(true)))
        );
        let mut parent_ctx = Context::new();
        parent_ctx.set_variable("p_var", Value::new(Type::Bool, ValueData::Bool(true)));
        parent_ctx.set_namespaced_func(&["my", "ns"], "dummy", dummy_func.clone());
        assert!(parent_ctx.has_namespaced_func(&["my", "ns"], "dummy"));
        assert!(!parent_ctx.has_namespaced_func(&["my", "ns"], "missing"));
        assert!(!parent_ctx.has_namespaced_func(&["other"], "dummy"));
        let child_ctx = Context::new_child(&parent_ctx);
        assert!(child_ctx.variable_names().contains(&"p_var".to_string()));
        assert!(child_ctx.has_namespaced_func(&["my", "ns"], "dummy"));
        assert!(child_ctx.get_function("my::ns::dummy").is_some());
        assert!(child_ctx.get_function("my::ns::missing").is_none());
        let mut empty_ns_ctx = Context::new();
        empty_ns_ctx.set_function("plain_func", dummy_func);
        let empty_ns_ident = crate::ast::expr::NamespacedIdent::new(
            vec![],
            "plain_func",
            crate::span::Span::new(0, 0, 0, 0, 0, 0),
        );
        assert!(empty_ns_ctx.get_namespaced_func(&empty_ns_ident).is_some());
        let mut full_ctx = Context::new_child(&parent_ctx);
        full_ctx.register_stdlib();
        let local_fn = crate::eval::func::Function {
            name: "fn".to_string(),
            func: std::sync::Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
            signature: None,
        };
        assert_eq!(
            (local_fn.func)(&[]).ok(),
            Some(Value::new(Type::Bool, ValueData::Bool(true)))
        );
        full_ctx.set_namespaced_func(&["local"], "fn", local_fn);
        let fnames = full_ctx.function_names();
        assert!(fnames.contains(&"my::ns::dummy".to_string()));
        assert!(fnames.contains(&"local::fn".to_string()));
        assert!(fnames.contains(&"upper".to_string()));
    }
}
