//! Function definitions, specifications, and signatures for HCL evaluation.
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::span::Span;
use crate::types::{Type, Value};
use std::sync::Arc;
/// A native function implementation closure.
pub type NativeFunc = Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;
/// Return type calculation closure for dynamic or static return types.
pub type ReturnTypeCalc = Arc<dyn Fn(&[Type]) -> Result<Type, String> + Send + Sync>;
/// Return type calculation closure based on evaluated input argument values.
pub type ValueReturnTypeCalc = Arc<dyn Fn(&[Value]) -> Result<Type, String> + Send + Sync>;
/// Specification for a function parameter.
#[derive(Clone, PartialEq, Eq)]
pub struct FunctionParamSpec {
    /// Parameter name for diagnostics and documentation.
    pub name: String,
    /// Expected parameter type.
    pub param_type: Type,
    /// Whether null is permitted for this parameter.
    pub allow_null: bool,
    /// Whether unknown values are permitted for this parameter.
    pub allow_unknown: bool,
    /// Whether dynamic types are permitted for this parameter.
    pub allow_dynamic_type: bool,
    /// Optional documentation description for this parameter.
    pub description: Option<String>,
}
impl std::fmt::Debug for FunctionParamSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionParamSpec")
            .field("name", &self.name)
            .field("param_type", &self.param_type)
            .field("allow_null", &self.allow_null)
            .field("allow_unknown", &self.allow_unknown)
            .field("allow_dynamic_type", &self.allow_dynamic_type)
            .field("description", &self.description)
            .finish()
    }
}
impl FunctionParamSpec {
    /// Creates a new parameter specification with default allowances.
    ///
    /// # Arguments
    /// * `name` - The parameter name.
    /// * `param_type` - The expected parameter type.
    #[must_use]
    pub fn new(name: impl Into<String>, param_type: Type) -> Self {
        Self {
            name: name.into(),
            param_type,
            allow_null: false,
            allow_unknown: true,
            allow_dynamic_type: true,
            description: None,
        }
    }
    /// Sets whether null values are allowed for this parameter.
    ///
    /// # Arguments
    /// * `allow` - `true` if null is permitted.
    #[must_use]
    pub fn with_allow_null(mut self, allow: bool) -> Self {
        self.allow_null = allow;
        self
    }
    /// Sets whether unknown values are allowed for this parameter.
    ///
    /// # Arguments
    /// * `allow` - `true` if unknown is permitted.
    #[must_use]
    pub fn with_allow_unknown(mut self, allow: bool) -> Self {
        self.allow_unknown = allow;
        self
    }
    /// Sets whether dynamic types are allowed for this parameter.
    ///
    /// # Arguments
    /// * `allow` - `true` if dynamic types are permitted.
    #[must_use]
    pub fn with_allow_dynamic_type(mut self, allow: bool) -> Self {
        self.allow_dynamic_type = allow;
        self
    }
    /// Sets a documentation description for this parameter.
    ///
    /// # Arguments
    /// * `desc` - The parameter description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}
/// Static signature and type-checking specification for a function.
#[derive(Clone)]
pub struct FunctionSignature {
    /// Positional fixed parameters.
    pub params: Vec<FunctionParamSpec>,
    /// Optional variadic parameter specification for trailing arguments.
    pub variadic_param: Option<FunctionParamSpec>,
    /// Return type calculator function.
    pub return_type: ReturnTypeCalc,
    /// Optional dynamic return type calculator function based on argument values.
    pub value_return_type: Option<ValueReturnTypeCalc>,
    /// Optional documentation description for this function.
    pub description: Option<String>,
    /// Whether this function is pure and deterministic.
    pub is_deterministic: bool,
}
impl std::fmt::Debug for FunctionSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionSignature")
            .field("params", &self.params)
            .field("variadic_param", &self.variadic_param)
            .field("description", &self.description)
            .field("is_deterministic", &self.is_deterministic)
            .finish_non_exhaustive()
    }
}
impl FunctionSignature {
    /// Creates a new function signature with a dynamic return type calculation function.
    ///
    /// # Arguments
    /// * `params` - Positional parameters.
    /// * `return_type` - Closure calculating the return type given argument types.
    #[must_use]
    pub fn new(params: Vec<FunctionParamSpec>, return_type: ReturnTypeCalc) -> Self {
        Self {
            params,
            variadic_param: None,
            return_type,
            value_return_type: None,
            description: None,
            is_deterministic: true,
        }
    }
    /// Creates a function signature with a constant/static return type.
    ///
    /// # Arguments
    /// * `params` - Positional parameters.
    /// * `static_return_type` - Fixed return type.
    #[must_use]
    pub fn with_static_return_type(
        params: Vec<FunctionParamSpec>,
        static_return_type: Type,
    ) -> Self {
        let ret = static_return_type;
        Self {
            params,
            variadic_param: None,
            return_type: Arc::new(move |_| Ok(ret.clone())),
            value_return_type: None,
            description: None,
            is_deterministic: true,
        }
    }
    /// Sets whether this function is pure and deterministic.
    ///
    /// # Arguments
    /// * `is_deterministic` - Whether the function is deterministic.
    #[must_use]
    pub fn with_deterministic(mut self, is_deterministic: bool) -> Self {
        self.is_deterministic = is_deterministic;
        self
    }
    /// Sets a value-dependent return type calculation function.
    ///
    /// # Arguments
    /// * `calc` - Closure calculating the return type given evaluated argument values.
    #[must_use]
    pub fn with_value_return_type(mut self, calc: ValueReturnTypeCalc) -> Self {
        self.value_return_type = Some(calc);
        self
    }
    /// Sets a variadic parameter specification.
    ///
    /// # Arguments
    /// * `variadic` - Variadic parameter specification.
    #[must_use]
    pub fn with_variadic(mut self, variadic: FunctionParamSpec) -> Self {
        self.variadic_param = Some(variadic);
        self
    }
    /// Sets a documentation description for the signature.
    ///
    /// # Arguments
    /// * `desc` - The function description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}
/// Represents a function available in an HCL evaluation context.
#[derive(Clone)]
pub struct Function {
    /// The name of the function.
    pub name: String,
    /// The native Rust implementation of the function.
    pub func: NativeFunc,
    /// Optional static signature specification.
    pub signature: Option<FunctionSignature>,
}
impl std::fmt::Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Function")
            .field("name", &self.name)
            .field("signature", &self.signature)
            .finish_non_exhaustive()
    }
}
impl Function {
    /// Creates a new function without a signature.
    ///
    /// # Arguments
    /// * `name` - Function name.
    /// * `func` - Native execution closure.
    #[must_use]
    pub fn new(name: impl Into<String>, func: NativeFunc) -> Self {
        Self {
            name: name.into(),
            func,
            signature: None,
        }
    }
    /// Attaches a static signature specification to this function.
    ///
    /// # Arguments
    /// * `signature` - Function signature.
    #[must_use]
    pub fn with_signature(mut self, signature: FunctionSignature) -> Self {
        self.signature = Some(signature);
        self
    }
    /// Returns whether this function is pure and deterministic.
    ///
    /// Impure functions (like `uuid` or `timestamp`) cannot be folded during partial evaluation.
    #[must_use]
    pub fn is_deterministic(&self) -> bool {
        if let Some(sig) = &self.signature {
            if !sig.is_deterministic {
                return false;
            }
        }
        !matches!(
            self.name.as_str(),
            "uuid" | "uuidv4" | "uuidv5" | "timestamp" | "plantimestamp" | "bcrypt"
        )
    }
    /// Performs ahead-of-time type checking and returns the inferred return type.
    ///
    /// # Arguments
    /// * `arg_types` - Types of the arguments provided.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if arity or parameter types do not conform to the function signature.
    pub fn type_check(&self, arg_types: &[Type]) -> Result<Type, Diagnostics> {
        let Some(sig) = &self.signature else {
            return Ok(Type::Dynamic);
        };
        let mut diags = Diagnostics::new();
        let expected_fixed = sig.params.len();
        if let Some(variadic) = &sig.variadic_param {
            if arg_types.len() < expected_fixed {
                diags.push(Diagnostic::error(
                    format!(
                        "Too few arguments for function '{}': expected at least {}, got {}",
                        self.name,
                        expected_fixed,
                        arg_types.len()
                    ),
                    "",
                    Span::new(0, 0, 0, 0, 0, 0),
                ));
                return Err(diags);
            }
            for (idx, arg_ty) in arg_types.iter().enumerate() {
                let spec = if idx < expected_fixed {
                    &sig.params[idx]
                } else {
                    variadic
                };
                Self::check_param_type(&self.name, spec, arg_ty, &mut diags);
            }
        } else {
            if arg_types.len() != expected_fixed {
                diags.push(Diagnostic::error(
                    format!(
                        "Wrong number of arguments for function '{}': expected {}, got {}",
                        self.name,
                        expected_fixed,
                        arg_types.len()
                    ),
                    "",
                    Span::new(0, 0, 0, 0, 0, 0),
                ));
                return Err(diags);
            }
            for (idx, arg_ty) in arg_types.iter().enumerate() {
                let spec = &sig.params[idx];
                Self::check_param_type(&self.name, spec, arg_ty, &mut diags);
            }
        }
        if diags.has_errors() {
            return Err(diags);
        }
        match (sig.return_type)(arg_types) {
            Ok(ret_ty) => Ok(ret_ty),
            Err(e) => {
                diags.push(Diagnostic::error(
                    format!("Failed to determine return type for '{}'", self.name),
                    e,
                    Span::new(0, 0, 0, 0, 0, 0),
                ));
                Err(diags)
            }
        }
    }
    fn check_param_type(
        func_name: &str,
        spec: &FunctionParamSpec,
        arg_ty: &Type,
        diags: &mut Diagnostics,
    ) {
        if !spec.allow_dynamic_type && arg_ty.is_dynamic() {
            diags.push(Diagnostic::error(
                format!(
                    "Argument '{}' for function '{}' requires exact type {}, got dynamic",
                    spec.name, func_name, spec.param_type
                ),
                "",
                Span::new(0, 0, 0, 0, 0, 0),
            ));
            return;
        }
        if spec.param_type != Type::Dynamic && !arg_ty.is_dynamic() && spec.param_type != *arg_ty {
            let can_coerce = matches!(
                (&spec.param_type, arg_ty),
                (Type::String, Type::Number | Type::Bool)
                    | (Type::Number | Type::Bool, Type::String)
            );
            if !can_coerce {
                diags.push(Diagnostic::error(
                    format!(
                        "Invalid type for argument '{}' in call to '{}': expected {}, got {}",
                        spec.name, func_name, spec.param_type, arg_ty
                    ),
                    "",
                    Span::new(0, 0, 0, 0, 0, 0),
                ));
            }
        }
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
    use crate::types::{Type, Value, ValueData};
    #[test]
    fn test_function_call() {
        let f = Function::new(
            "test_func",
            Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
        );
        let res = (f.func)(&[]);
        assert_eq!(res.unwrap(), Value::new(Type::Bool, ValueData::Bool(true)));
    }
    #[test]
    fn test_function_debug() {
        let f = Function::new(
            "test_func",
            Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
        );
        let debug_str = format!("{f:?}");
        assert!(debug_str.contains("Function"));
        assert!(debug_str.contains("test_func"));
        assert!((f.func)(&[]).is_ok());
    }
    #[test]
    fn test_param_spec_and_signature_builders() {
        let p1 = FunctionParamSpec::new("name", Type::String)
            .with_allow_null(true)
            .with_allow_unknown(false)
            .with_allow_dynamic_type(false)
            .with_description("A name parameter");
        assert_eq!(p1.name, "name");
        assert_eq!(p1.param_type, Type::String);
        assert!(p1.allow_null);
        assert!(!p1.allow_unknown);
        assert!(!p1.allow_dynamic_type);
        assert_eq!(p1.description.as_deref(), Some("A name parameter"));
        let p_var = FunctionParamSpec::new("rest", Type::Number);
        let sig = FunctionSignature::with_static_return_type(vec![p1.clone()], Type::Bool)
            .with_variadic(p_var.clone())
            .with_description("Test function description");
        assert_eq!(sig.params.len(), 1);
        assert!(sig.variadic_param.is_some());
        assert_eq!(
            sig.description.as_deref(),
            Some("Test function description")
        );
        assert!(format!("{p1:?}").contains("FunctionParamSpec"));
        assert!(format!("{sig:?}").contains("FunctionSignature"));
        assert_eq!(p1, p1.clone());
        assert_ne!(p1, p_var);
    }
    #[test]
    fn test_function_type_check() {
        let p1 = FunctionParamSpec::new("str_arg", Type::String);
        let p2 = FunctionParamSpec::new("num_arg", Type::Number).with_allow_null(false);
        let sig = FunctionSignature::new(vec![p1, p2], Arc::new(|_| Ok(Type::Bool)));
        let func = Function::new("validate_types", Arc::new(|_| Ok(Value::null(Type::Bool))))
            .with_signature(sig);
        assert!((func.func)(&[]).is_ok());
        assert_eq!(
            func.type_check(&[Type::String, Type::Number]),
            Ok(Type::Bool)
        );
        assert_eq!(
            func.type_check(&[Type::String, Type::String]),
            Ok(Type::Bool)
        );
        assert!(func.type_check(&[Type::String]).is_err());
        assert!(
            func.type_check(&[Type::String, Type::Number, Type::Bool])
                .is_err()
        );
        assert!(
            func.type_check(&[Type::String, Type::List(Box::new(Type::String))])
                .is_err()
        );
        let p_no_dyn =
            FunctionParamSpec::new("strict_str", Type::String).with_allow_dynamic_type(false);
        let sig_no_dyn = FunctionSignature::with_static_return_type(vec![p_no_dyn], Type::Bool);
        let func_no_dyn = Function::new("strict_fn", Arc::new(|_| Ok(Value::null(Type::Bool))))
            .with_signature(sig_no_dyn);
        assert!((func_no_dyn.func)(&[]).is_ok());
        assert!(func_no_dyn.type_check(&[Type::Dynamic]).is_err());
        assert_eq!(func_no_dyn.type_check(&[Type::String]), Ok(Type::Bool));
        let p_dyn_param = FunctionParamSpec::new("any_val", Type::Dynamic);
        let sig_dyn_param =
            FunctionSignature::with_static_return_type(vec![p_dyn_param], Type::Bool);
        let func_dyn_param = Function::new("any_fn", Arc::new(|_| Ok(Value::null(Type::Bool))))
            .with_signature(sig_dyn_param);
        assert!((func_dyn_param.func)(&[]).is_ok());
        assert_eq!(func_dyn_param.type_check(&[Type::Number]), Ok(Type::Bool));
        assert_eq!(
            func.type_check(&[Type::String, Type::Dynamic]),
            Ok(Type::Bool)
        );
        let sig_err_ret = FunctionSignature::new(
            vec![FunctionParamSpec::new("a", Type::String)],
            Arc::new(|_| Err("custom return type calculation failure".to_string())),
        );
        let func_err_ret = Function::new("err_ret_fn", Arc::new(|_| Ok(Value::null(Type::Bool))))
            .with_signature(sig_err_ret);
        assert!((func_err_ret.func)(&[]).is_ok());
        assert!(func_err_ret.type_check(&[Type::String]).is_err());
        let func_nosig = Function::new("nosig", Arc::new(|_| Ok(Value::null(Type::Dynamic))));
        assert!((func_nosig.func)(&[]).is_ok());
        assert_eq!(func_nosig.type_check(&[]), Ok(Type::Dynamic));
        let var_sig = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("first", Type::String)],
            Type::String,
        )
        .with_variadic(FunctionParamSpec::new("rest", Type::Number));
        let var_func = Function::new("var_fn", Arc::new(|_| Ok(Value::null(Type::String))))
            .with_signature(var_sig);
        assert!((var_func.func)(&[]).is_ok());
        assert!(var_func.type_check(&[Type::String]).is_ok());
        assert!(
            var_func
                .type_check(&[Type::String, Type::Number, Type::Number])
                .is_ok()
        );
        assert!(var_func.type_check(&[]).is_err());
        assert!(var_func.type_check(&[Type::String, Type::Bool]).is_err());
    }
    #[test]
    fn test_value_dependent_return_type() {
        use crate::ast::expr::{Expression, FuncCall};
        use crate::eval::context::Context;
        use crate::eval::evaluator::Evaluator;
        let vrt_fn = Arc::new(|args: &[Value]| {
            if args.is_empty() {
                Ok(Type::Dynamic)
            } else if args[0].to_string() == "\"num\"" {
                Ok(Type::Number)
            } else {
                Ok(Type::String)
            }
        });
        assert_eq!(vrt_fn(&[]), Ok(Type::Dynamic));
        let sig = FunctionSignature::new(
            vec![
                FunctionParamSpec::new("tag", Type::String),
                FunctionParamSpec::new("val", Type::Dynamic),
            ],
            Arc::new(|_| Ok(Type::Dynamic)),
        )
        .with_value_return_type(vrt_fn);
        assert_eq!((sig.return_type)(&[]), Ok(Type::Dynamic));
        let func =
            Function::new("cast_by_tag", Arc::new(|args| Ok(args[1].clone()))).with_signature(sig);
        let mut ctx = Context::new();
        ctx.set_function("cast_by_tag", func);
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let expr1 = Expression::FuncCall(
            Box::new(FuncCall {
                name: "cast_by_tag".into(),
                args: vec![
                    Expression::String("num".to_string(), span.clone()),
                    Expression::String("100".to_string(), span.clone()),
                ],
                expand_final: false,
            }),
            span.clone(),
        );
        let val1 = Evaluator::new(&ctx).eval_expr(&expr1);
        assert_eq!(val1.ty(), &Type::Number);
        assert_eq!(val1.to_string(), "100");
        let expr2 = Expression::FuncCall(
            Box::new(FuncCall {
                name: "cast_by_tag".into(),
                args: vec![
                    Expression::String("str".to_string(), span.clone()),
                    Expression::Number(crate::number::Number::from(500), span.clone()),
                ],
                expand_final: false,
            }),
            span,
        );
        let val2 = Evaluator::new(&ctx).eval_expr(&expr2);
        assert_eq!(val2.ty(), &Type::String);
        assert_eq!(val2.to_string(), "\"500\"");
    }
    #[test]
    fn test_function_deterministic() {
        let sig_impure = FunctionSignature::with_static_return_type(vec![], Type::String)
            .with_deterministic(false);
        let func_impure = Function::new("my_rand", Arc::new(|_| Ok(Value::null(Type::String))))
            .with_signature(sig_impure);
        assert!(!func_impure.is_deterministic());
        assert_eq!((func_impure.func)(&[]), Ok(Value::null(Type::String)));
        let func_uuid = Function::new("uuid", Arc::new(|_| Ok(Value::null(Type::String))));
        assert!(!func_uuid.is_deterministic());
        assert_eq!((func_uuid.func)(&[]), Ok(Value::null(Type::String)));
        let func_timestamp =
            Function::new("timestamp", Arc::new(|_| Ok(Value::null(Type::String))));
        assert!(!func_timestamp.is_deterministic());
        assert_eq!((func_timestamp.func)(&[]), Ok(Value::null(Type::String)));
        let func_bcrypt = Function::new("bcrypt", Arc::new(|_| Ok(Value::null(Type::String))));
        assert!(!func_bcrypt.is_deterministic());
        assert_eq!((func_bcrypt.func)(&[]), Ok(Value::null(Type::String)));
        let sig_pure = FunctionSignature::with_static_return_type(vec![], Type::String)
            .with_deterministic(true);
        let func_pure_sig = Function::new("my_pure", Arc::new(|_| Ok(Value::null(Type::String))))
            .with_signature(sig_pure);
        assert!(func_pure_sig.is_deterministic());
        assert!((func_pure_sig.func)(&[]).is_ok());
        let func_pure = Function::new("upper", Arc::new(|_| Ok(Value::null(Type::String))));
        assert!(func_pure.is_deterministic());
        assert_eq!((func_pure.func)(&[]), Ok(Value::null(Type::String)));
    }
}
