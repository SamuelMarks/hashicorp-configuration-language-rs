//! Core HCL expression and template evaluation engine.
use crate::ast::expr::{BinaryOp, Conditional, Expression, ForExpr, TraversalOperator, UnaryOp};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::eval::context::Context;
use crate::number::Number;
use crate::types::{Type, Value, ValueData};
use bigdecimal::ToPrimitive;
use std::collections::BTreeMap;
/// Evaluates an AST `Expression` into a `Value`.
pub struct Evaluator<'a> {
    ctx: &'a Context<'a>,
    diagnostics: Diagnostics,
    eval_callouts: Vec<crate::diagnostic::EvalCallout>,
}
impl<'a> Evaluator<'a> {
    /// Creates a new `Evaluator` with the given context.
    #[must_use]
    pub const fn new(ctx: &'a Context<'a>) -> Self {
        Self {
            ctx,
            diagnostics: Diagnostics::new(),
            eval_callouts: Vec::new(),
        }
    }
    /// Pushes a diagnostic, attaching any recorded sub-expression callouts.
    fn push_diagnostic(&mut self, mut diag: Diagnostic) {
        if diag.eval_callouts.is_empty() && !self.eval_callouts.is_empty() {
            diag.eval_callouts.clone_from(&self.eval_callouts);
        }
        self.diagnostics.push(diag);
    }
    /// Evaluates an expression, returning the result and any accumulated diagnostics.
    ///
    /// # Errors
    /// Returns accumulated diagnostics if evaluation fails.
    pub fn evaluate(mut self, expr: &Expression) -> Result<(Value, Diagnostics), Diagnostics> {
        self.eval_callouts.clear();
        let val = self.eval_expr(expr);
        if self.diagnostics.has_errors() {
            Err(self.diagnostics)
        } else {
            Ok((val, self.diagnostics))
        }
    }
    /// Partially evaluates an expression, simplifying constant sub-trees and preserving unknowns.
    ///
    /// # Arguments
    /// * `expr` - The expression to partially evaluate.
    ///
    /// # Errors
    /// Returns accumulated diagnostics if evaluation encounters an irrecoverable error.
    pub fn partial_evaluate(&self, expr: &Expression) -> Result<Expression, Diagnostics> {
        crate::eval::partial::partial_eval(expr, self.ctx)
    }
    /// Evaluates a binary operator expression on two operands.
    ///
    /// # Arguments
    /// * `op` - The binary operator.
    /// * `lhs` - Left-hand side expression.
    /// * `rhs` - Right-hand side expression.
    /// * `span` - Source code span of the operator expression.
    ///
    /// # Returns
    /// The resulting evaluated [`Value`].
    pub fn evaluate_binary_op(
        &mut self,
        op: BinaryOp,
        lhs: &Expression,
        rhs: &Expression,
        span: crate::span::Span,
    ) -> Value {
        self.eval_binary_op(op, lhs, rhs, span)
    }
    /// Evaluates a unary operator expression on an inner expression.
    ///
    /// # Arguments
    /// * `op` - The unary operator.
    /// * `inner` - The operand expression.
    /// * `span` - Source code span of the unary expression.
    ///
    /// # Returns
    /// The resulting evaluated [`Value`].
    pub fn evaluate_unary_op(
        &mut self,
        op: UnaryOp,
        inner: &Expression,
        span: crate::span::Span,
    ) -> Value {
        self.eval_unary_op(op, inner, span)
    }
    pub(crate) fn eval_expr(&mut self, expr: &Expression) -> Value {
        match expr {
            Expression::Null(_) => Value::null(Type::Dynamic),
            Expression::Bool(b, _) => Value::new(Type::Bool, ValueData::Bool(*b)),
            Expression::Number(n, _) => Value::new(Type::Number, ValueData::Number(n.clone())),
            Expression::String(s, _) => Value::new(Type::String, ValueData::String(s.clone())),
            Expression::Tuple(exprs, _) => {
                let vals: Vec<Value> = exprs.iter().map(|e| self.eval_expr(e)).collect();
                let types: Vec<Type> = vals.iter().map(|v| v.ty().clone()).collect();
                Value::new(Type::Tuple(types), ValueData::Array(vals))
            }
            Expression::Object(kvs, _span) => {
                let mut map = BTreeMap::new();
                let mut types = BTreeMap::new();
                for (k, v) in kvs {
                    let k_val = self.eval_expr(k);
                    let k_str = match &*k_val.data {
                        ValueData::String(s) => s.clone(),
                        ValueData::Unknown(_) => {
                            self.push_diagnostic(Diagnostic::error(
                                "Object key cannot be unknown",
                                "",
                                k.span(),
                            ));
                            continue;
                        }
                        _ => {
                            self.push_diagnostic(Diagnostic::error(
                                "Object key must be a string",
                                "",
                                k.span(),
                            ));
                            continue;
                        }
                    };
                    let v_val = self.eval_expr(v);
                    types.insert(k_str.clone(), v_val.ty().clone());
                    map.insert(k_str, v_val);
                }
                Value::new(Type::object(types), ValueData::Object(map))
            }
            Expression::Variable(name, span) => {
                if let Some(val) = self.ctx.get_variable(name) {
                    let display_name = if name.starts_with("var.") || name.starts_with("local.") {
                        name.clone()
                    } else {
                        format!("var.{name}")
                    };
                    self.eval_callouts
                        .push(crate::diagnostic::EvalCallout::from_value(
                            span.clone(),
                            display_name,
                            val,
                        ));
                    val.clone()
                } else {
                    let mut diag =
                        Diagnostic::error(format!("Unknown variable '{name}'"), "", span.clone());
                    let vars = self.ctx.variable_names();
                    let clean_name = name.strip_prefix("var.").unwrap_or(name);
                    if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                        clean_name,
                        vars.iter().map(|v| v.strip_prefix("var.").unwrap_or(v)),
                        2,
                    ) {
                        diag = diag.with_detail(format!("Did you mean var.{candidate}?"));
                    }
                    self.push_diagnostic(diag);
                    Value::unknown(Type::Dynamic)
                }
            }
            Expression::BinaryOp(op, lhs, rhs, span) => {
                self.eval_binary_op(*op, lhs, rhs, span.clone())
            }
            Expression::UnaryOp(op, inner, span) => self.eval_unary_op(*op, inner, span.clone()),
            Expression::Parentheses(inner, _) => self.eval_expr(inner),
            Expression::Conditional(cond, span) => self.eval_conditional(cond, span.clone()),
            Expression::Template(parts, _span) => {
                let mut out = String::new();
                let mut has_unknown = false;
                let mut marks = std::collections::BTreeSet::new();
                self.eval_template_parts(parts, &mut out, &mut has_unknown, &mut marks);
                if has_unknown {
                    let mut ref_res = crate::types::refinement::Refinement::not_null();
                    if let Some(crate::ast::expr::TemplatePart::Literal(first, _)) = parts.first() {
                        ref_res = ref_res.with_prefix(first.clone());
                    }
                    if let Some(crate::ast::expr::TemplatePart::Literal(last, _)) = parts.last() {
                        ref_res = ref_res.with_suffix(last.clone());
                    }
                    Value::unknown_refined(Type::String, ref_res).with_marks(marks)
                } else {
                    Value::new_with_marks(Type::String, ValueData::String(out), marks)
                }
            }
            Expression::Traversal(t, span) => self.eval_traversal(t, span.clone()),
            Expression::FuncCall(f, span) => self.evaluate_function_call(f, span.clone()),
            Expression::ForExpr(f, span) => self.eval_for(f, span.clone()),
        }
    }
    fn eval_try_call(&mut self, f: &crate::ast::expr::FuncCall, span: crate::span::Span) -> Value {
        if f.args.is_empty() {
            self.push_diagnostic(Diagnostic::error(
                "The 'try' function requires at least one argument",
                "",
                span,
            ));
            return Value::unknown(Type::Dynamic);
        }
        for arg_expr in &f.args {
            let mut isolated = Self::new(self.ctx);
            let val = isolated.eval_expr(arg_expr);
            if !isolated.diagnostics.has_errors() {
                return val;
            }
        }
        let detail = "All expressions provided to 'try' resulted in errors.";
        self.push_diagnostic(Diagnostic::error(
            "No alternative expression succeeded in 'try'",
            detail,
            span,
        ));
        Value::unknown(Type::Dynamic)
    }

    fn eval_can_call(&mut self, f: &crate::ast::expr::FuncCall, span: crate::span::Span) -> Value {
        if f.args.len() != 1 {
            self.push_diagnostic(Diagnostic::error(
                "The 'can' function requires exactly one argument",
                "",
                span,
            ));
            return Value::unknown(Type::Bool);
        }
        let mut isolated = Self::new(self.ctx);
        let val = isolated.eval_expr(&f.args[0]);
        if isolated.diagnostics.has_errors() {
            Value::new(Type::Bool, ValueData::Bool(false))
        } else if val.is_unknown() {
            Value::unknown(Type::Bool)
        } else {
            Value::new(Type::Bool, ValueData::Bool(true))
        }
    }

    fn expand_final_arg(
        &mut self,
        args: &mut Vec<Value>,
        span: &crate::span::Span,
    ) -> Result<(), Value> {
        let Some(final_arg) = args.pop() else {
            return Err(Value::unknown(Type::Dynamic));
        };
        if final_arg.is_unknown() {
            return Err(Value::unknown(Type::Dynamic));
        }
        match &*final_arg.data {
            ValueData::Array(arr) => {
                for item in arr {
                    let mut unpacked = item.clone();
                    unpacked.marks.extend(final_arg.marks.clone());
                    args.push(unpacked);
                }
                Ok(())
            }
            ValueData::Set(set) => {
                for item in set {
                    let mut unpacked = item.clone();
                    unpacked.marks.extend(final_arg.marks.clone());
                    args.push(unpacked);
                }
                Ok(())
            }
            _ => {
                let msg = format!(
                    "Cannot expand non-sequence value of type {} using '...'",
                    final_arg.ty()
                );
                self.push_diagnostic(Diagnostic::error(
                    "Invalid expanded argument",
                    msg,
                    span.clone(),
                ));
                Err(Value::unknown(Type::Dynamic))
            }
        }
    }

    fn validate_and_coerce_single_arg(
        &mut self,
        f: &crate::ast::expr::FuncCall,
        spec: &crate::eval::func::FunctionParamSpec,
        arg: &mut Value,
        span: &crate::span::Span,
    ) -> bool {
        if arg.is_null() && !spec.allow_null {
            let msg = format!(
                "Argument '{}' for function '{}' cannot be null",
                spec.name, f.name
            );
            self.diagnostics
                .push(Diagnostic::error(msg, "", span.clone()));
            return false;
        }
        if spec.param_type != Type::Dynamic
            && !arg.ty().is_dynamic()
            && arg.ty() != &spec.param_type
        {
            match arg.coerce(&spec.param_type) {
                Ok(coerced) => *arg = coerced,
                Err(err) => {
                    let msg = format!("Invalid argument '{}' in call to '{}'", spec.name, f.name);
                    self.push_diagnostic(Diagnostic::error(msg, err.to_string(), span.clone()));
                    return false;
                }
            }
        }
        true
    }

    fn validate_and_coerce_func_args(
        &mut self,
        f: &crate::ast::expr::FuncCall,
        sig: &crate::eval::func::FunctionSignature,
        args: &mut [Value],
        span: &crate::span::Span,
    ) -> bool {
        let expected_fixed = sig.params.len();
        if let Some(variadic) = &sig.variadic_param {
            if args.len() < expected_fixed {
                let msg = format!(
                    "Too few arguments for function '{}': expected at least {}, got {}",
                    f.name,
                    expected_fixed,
                    args.len()
                );
                self.diagnostics
                    .push(Diagnostic::error(msg, "", span.clone()));
                return false;
            }
            for (idx, arg) in args.iter_mut().enumerate() {
                let spec = if idx < expected_fixed {
                    &sig.params[idx]
                } else {
                    variadic
                };
                if !self.validate_and_coerce_single_arg(f, spec, arg, span) {
                    return false;
                }
            }
        } else {
            if args.len() != expected_fixed {
                let msg = format!(
                    "Wrong number of arguments for function '{}': expected {}, got {}",
                    f.name,
                    expected_fixed,
                    args.len()
                );
                self.diagnostics
                    .push(Diagnostic::error(msg, "", span.clone()));
                return false;
            }
            for (idx, arg) in args.iter_mut().enumerate() {
                if !self.validate_and_coerce_single_arg(f, &sig.params[idx], arg, span) {
                    return false;
                }
            }
        }
        true
    }

    /// Evaluates a function call expression, resolving built-in, registered, or standard library functions.
    ///
    /// If the function name is unknown, typo suggestions are computed against registered functions
    /// and the standard library and attached to the diagnostic detail message.
    ///
    /// # Arguments
    /// * `f` - The function call AST node.
    /// * `span` - The source span of the function call.
    pub fn evaluate_function_call(
        &mut self,
        f: &crate::ast::expr::FuncCall,
        span: crate::span::Span,
    ) -> Value {
        if f.name == "try" {
            return self.eval_try_call(f, span);
        }
        if f.name == "can" {
            return self.eval_can_call(f, span);
        }
        let mut args = Vec::new();
        let mut arg_marks = std::collections::BTreeSet::new();
        for arg_expr in &f.args {
            let evaluated_arg = self.eval_expr(arg_expr);
            arg_marks.extend(evaluated_arg.marks.clone());
            args.push(evaluated_arg);
        }
        if f.expand_final
            && let Err(unk) = self.expand_final_arg(&mut args, &span)
        {
            return unk.with_marks(arg_marks);
        }
        if let Some(func) = self.ctx.get_namespaced_func(&f.name) {
            if let Some(sig) = &func.signature
                && !self.validate_and_coerce_func_args(f, sig, &mut args, &span)
            {
                return Value::unknown(Type::Dynamic).with_marks(arg_marks);
            }
            match (func.func)(&args) {
                Ok(mut v) => {
                    if let Some(sig) = &func.signature
                        && let Some(val_calc) = &sig.value_return_type
                        && let Ok(expected_ty) = (val_calc)(&args)
                        && v.ty() != &expected_ty
                        && expected_ty != Type::Dynamic
                        && let Ok(coerced) = v.coerce(&expected_ty)
                    {
                        v = coerced;
                    }
                    if f.name == "nonsensitive" {
                        for m in arg_marks {
                            if m != crate::types::ValueMark::Sensitive {
                                v.marks.insert(m);
                            }
                        }
                    } else if f.name != "issensitive" {
                        v.marks.extend(arg_marks);
                    }
                    v
                }
                Err(e) => {
                    self.push_diagnostic(Diagnostic::error(
                        format!("Function call '{}' failed", f.name),
                        e.to_string(),
                        span,
                    ));
                    Value::unknown(Type::Dynamic).with_marks(arg_marks)
                }
            }
        } else {
            let mut diag = Diagnostic::error(format!("Unknown function '{}'", f.name), "", span);
            let mut all_funcs = self.ctx.function_names();
            for k in crate::eval::stdlib::stdlib_map().keys() {
                all_funcs.push(k.clone());
            }
            let target_str = f.name.to_string();
            if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                &target_str,
                all_funcs.iter().map(String::as_str),
                2,
            ) {
                diag = diag.with_detail(format!(
                    "Unknown function '{}'. Did you mean '{}'?",
                    f.name, candidate
                ));
            }
            self.push_diagnostic(diag);
            Value::unknown(Type::Dynamic).with_marks(arg_marks)
        }
    }
    fn eval_conditional(&mut self, cond: &Conditional, span: crate::span::Span) -> Value {
        let cond_val = self.eval_expr(&cond.cond_expr);
        let cond_marks = cond_val.marks.clone();
        if cond_val.is_unknown() {
            let t_val = self.eval_expr(&cond.true_expr);
            let f_val = self.eval_expr(&cond.false_expr);
            if let Some(unified) = crate::types::unify::unify(t_val.ty(), f_val.ty()) {
                let mut marks = cond_marks;
                marks.extend(t_val.marks);
                marks.extend(f_val.marks);
                return Value::unknown(unified).with_marks(marks);
            }
            let msg = format!(
                "True branch is {:?}, false branch is {:?}",
                t_val.ty(),
                f_val.ty()
            );
            self.push_diagnostic(Diagnostic::error(
                "Incompatible types in conditional branches",
                msg,
                span,
            ));
            return Value::unknown(Type::Dynamic).with_marks(cond_marks);
        }
        match &*cond_val.data {
            ValueData::Bool(true) => {
                let mut res = self.eval_expr(&cond.true_expr);
                res.marks.extend(cond_marks);
                res
            }
            ValueData::Bool(false) => {
                let mut res = self.eval_expr(&cond.false_expr);
                res.marks.extend(cond_marks);
                res
            }
            _ => {
                self.push_diagnostic(Diagnostic::error(
                    "Condition must be a boolean",
                    "",
                    cond.cond_expr.span(),
                ));
                Value::unknown(Type::Dynamic).with_marks(cond_marks)
            }
        }
    }
    fn eval_logical_short_circuit(
        &mut self,
        is_and: bool,
        lhs: &Expression,
        rhs: &Expression,
    ) -> Value {
        let l_val = self.eval_expr(lhs);
        if matches!(&*l_val.data, ValueData::Bool(b) if *b != is_and) {
            return Value::new_with_marks(Type::Bool, ValueData::Bool(!is_and), l_val.marks);
        }
        if l_val.is_unknown() {
            return Value::unknown(Type::Bool).with_marks(l_val.marks);
        }
        let r_val = self.eval_expr(rhs);
        let mut marks = l_val.marks;
        marks.extend(r_val.marks);
        if let ValueData::Bool(b) = &*r_val.data {
            Value::new_with_marks(Type::Bool, ValueData::Bool(*b), marks)
        } else if let ValueData::Unknown(_) = &*r_val.data {
            Value::unknown(Type::Bool).with_marks(marks)
        } else {
            let op_sym = if is_and { "&&" } else { "||" };
            self.push_diagnostic(Diagnostic::error(
                format!("Right operand of {op_sym} must be a boolean"),
                "",
                rhs.span(),
            ));
            Value::unknown(Type::Bool).with_marks(marks)
        }
    }

    fn eval_unknown_string_add(
        l_val: &Value,
        r_val: &Value,
    ) -> crate::types::refinement::Refinement {
        let mut ref_res = crate::types::refinement::Refinement::not_null();
        let l_prefix = match &*l_val.data {
            ValueData::String(s) => Some(s.clone()),
            ValueData::Unknown(_) => l_val.refinement().and_then(|r| r.string_prefix.clone()),
            _ => None,
        };
        let r_prefix = match &*r_val.data {
            ValueData::String(s) => Some(s.clone()),
            ValueData::Unknown(_) => r_val.refinement().and_then(|r| r.string_prefix.clone()),
            _ => None,
        };
        if let Some(lp) = l_prefix {
            if l_val.is_unknown() {
                ref_res = ref_res.with_prefix(lp);
            } else if let Some(rp) = r_prefix {
                ref_res = ref_res.with_prefix(format!("{lp}{rp}"));
            } else {
                ref_res = ref_res.with_prefix(lp);
            }
        }
        let l_suffix = match &*l_val.data {
            ValueData::String(s) => Some(s.clone()),
            ValueData::Unknown(_) => l_val.refinement().and_then(|r| r.string_suffix.clone()),
            _ => None,
        };
        let r_suffix = match &*r_val.data {
            ValueData::String(s) => Some(s.clone()),
            ValueData::Unknown(_) => r_val.refinement().and_then(|r| r.string_suffix.clone()),
            _ => None,
        };
        if let Some(rs) = r_suffix {
            if r_val.is_unknown() {
                ref_res = ref_res.with_suffix(rs);
            } else if let Some(ls) = l_suffix {
                ref_res = ref_res.with_suffix(format!("{ls}{rs}"));
            } else {
                ref_res = ref_res.with_suffix(rs);
            }
        }
        let l_min = match &*l_val.data {
            ValueData::String(s) => s.chars().count(),
            ValueData::Unknown(_) => l_val
                .refinement()
                .and_then(|r| r.string_length_min)
                .unwrap_or(0),
            _ => 0,
        };
        let r_min = match &*r_val.data {
            ValueData::String(s) => s.chars().count(),
            ValueData::Unknown(_) => r_val
                .refinement()
                .and_then(|r| r.string_length_min)
                .unwrap_or(0),
            _ => 0,
        };
        ref_res.string_length_min = Some(l_min + r_min);
        let l_max = match &*l_val.data {
            ValueData::String(s) => Some(s.chars().count()),
            ValueData::Unknown(_) => l_val.refinement().and_then(|r| r.string_length_max),
            _ => None,
        };
        let r_max = match &*r_val.data {
            ValueData::String(s) => Some(s.chars().count()),
            ValueData::Unknown(_) => r_val.refinement().and_then(|r| r.string_length_max),
            _ => None,
        };
        if let (Some(l_m), Some(r_m)) = (l_max, r_max) {
            ref_res.string_length_max = Some(l_m + r_m);
        }
        ref_res
    }

    fn eval_unknown_binary_op(
        op: BinaryOp,
        l_val: &Value,
        r_val: &Value,
        combined_marks: std::collections::BTreeSet<crate::types::ValueMark>,
    ) -> Value {
        match op {
            BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::Less
            | BinaryOp::LessEq
            | BinaryOp::Greater
            | BinaryOp::GreaterEq => Value::unknown(Type::Bool).with_marks(combined_marks),
            BinaryOp::Add => {
                let is_num =
                    l_val.coerce(&Type::Number).is_ok() && r_val.coerce(&Type::Number).is_ok();
                if !is_num && l_val.ty() == &Type::String && r_val.ty() == &Type::String {
                    let ref_res = Self::eval_unknown_string_add(l_val, r_val);
                    return Value::unknown_refined(Type::String, ref_res)
                        .with_marks(combined_marks);
                }
                if l_val.ty().is_capsule() {
                    return Value::unknown(l_val.ty().clone()).with_marks(combined_marks);
                }
                if r_val.ty().is_capsule() {
                    return Value::unknown(r_val.ty().clone()).with_marks(combined_marks);
                }
                Value::unknown(Type::Number).with_marks(combined_marks)
            }
            _ => {
                if l_val.ty().is_capsule() {
                    return Value::unknown(l_val.ty().clone()).with_marks(combined_marks);
                }
                if r_val.ty().is_capsule() {
                    return Value::unknown(r_val.ty().clone()).with_marks(combined_marks);
                }
                Value::unknown(Type::Number).with_marks(combined_marks)
            }
        }
    }

    fn try_eval_capsule_binary_op(
        &mut self,
        op: BinaryOp,
        l_val: &Value,
        r_val: &Value,
        combined_marks: &std::collections::BTreeSet<crate::types::ValueMark>,
        span: &crate::span::Span,
    ) -> Option<Value> {
        let capsule_cmp = match op {
            BinaryOp::Less | BinaryOp::LessEq | BinaryOp::Greater | BinaryOp::GreaterEq => l_val
                .ty()
                .capsule_ops()
                .and_then(|ops| ops.cmp.clone())
                .or_else(|| r_val.ty().capsule_ops().and_then(|ops| ops.cmp.clone())),
            _ => None,
        };
        if let Some(cmp_fn) = capsule_cmp {
            let l_any: &(dyn std::any::Any + Send + Sync) =
                l_val.as_capsule_any().map_or(l_val, |any| any);
            let r_any: &(dyn std::any::Any + Send + Sync) =
                r_val.as_capsule_any().map_or(r_val, |any| any);
            return Some(match (cmp_fn)(l_any, r_any) {
                Ok(ordering) => {
                    let result = match op {
                        BinaryOp::Less => ordering == std::cmp::Ordering::Less,
                        BinaryOp::LessEq => ordering != std::cmp::Ordering::Greater,
                        BinaryOp::Greater => ordering == std::cmp::Ordering::Greater,
                        _ => ordering != std::cmp::Ordering::Less,
                    };
                    Value::new_with_marks(
                        Type::Bool,
                        ValueData::Bool(result),
                        combined_marks.clone(),
                    )
                }
                Err(err) => {
                    self.push_diagnostic(Diagnostic::error(err.to_string(), "", span.clone()));
                    Value::unknown(Type::Bool).with_marks(combined_marks.clone())
                }
            });
        }
        let capsule_arith = match op {
            BinaryOp::Add => l_val
                .ty()
                .capsule_ops()
                .and_then(|ops| ops.add.clone())
                .or_else(|| r_val.ty().capsule_ops().and_then(|ops| ops.add.clone())),
            BinaryOp::Sub => l_val
                .ty()
                .capsule_ops()
                .and_then(|ops| ops.sub.clone())
                .or_else(|| r_val.ty().capsule_ops().and_then(|ops| ops.sub.clone())),
            BinaryOp::Mul => l_val
                .ty()
                .capsule_ops()
                .and_then(|ops| ops.mul.clone())
                .or_else(|| r_val.ty().capsule_ops().and_then(|ops| ops.mul.clone())),
            BinaryOp::Div => l_val
                .ty()
                .capsule_ops()
                .and_then(|ops| ops.div.clone())
                .or_else(|| r_val.ty().capsule_ops().and_then(|ops| ops.div.clone())),
            BinaryOp::Mod => l_val
                .ty()
                .capsule_ops()
                .and_then(|ops| ops.modulo.clone())
                .or_else(|| r_val.ty().capsule_ops().and_then(|ops| ops.modulo.clone())),
            _ => None,
        };
        if let Some(arith_fn) = capsule_arith {
            let l_any: &(dyn std::any::Any + Send + Sync) =
                l_val.as_capsule_any().map_or(l_val, |any| any);
            let r_any: &(dyn std::any::Any + Send + Sync) =
                r_val.as_capsule_any().map_or(r_val, |any| any);
            let target_ty = if l_val.ty().is_capsule() {
                l_val.ty().clone()
            } else {
                r_val.ty().clone()
            };
            return Some(match (arith_fn)(l_any, r_any) {
                Ok(boxed) => Value::new_with_marks(
                    target_ty,
                    ValueData::Capsule(std::sync::Arc::from(boxed)),
                    combined_marks.clone(),
                ),
                Err(err) => {
                    self.push_diagnostic(Diagnostic::error(err.to_string(), "", span.clone()));
                    Value::unknown(target_ty).with_marks(combined_marks.clone())
                }
            });
        }
        None
    }

    fn eval_binary_op(
        &mut self,
        op: BinaryOp,
        lhs: &Expression,
        rhs: &Expression,
        span: crate::span::Span,
    ) -> Value {
        if op == BinaryOp::And {
            return self.eval_logical_short_circuit(true, lhs, rhs);
        }
        if op == BinaryOp::Or {
            return self.eval_logical_short_circuit(false, lhs, rhs);
        }
        let l_val = self.eval_expr(lhs);
        let r_val = self.eval_expr(rhs);
        let mut combined_marks = l_val.marks.clone();
        combined_marks.extend(r_val.marks.clone());
        if l_val.is_unknown() || r_val.is_unknown() {
            return Self::eval_unknown_binary_op(op, &l_val, &r_val, combined_marks);
        }
        match op {
            BinaryOp::Eq => {
                let eq = l_val.unmark().0 == r_val.unmark().0;
                Value::new_with_marks(Type::Bool, ValueData::Bool(eq), combined_marks)
            }
            BinaryOp::NotEq => {
                let ne = l_val.unmark().0 != r_val.unmark().0;
                Value::new_with_marks(Type::Bool, ValueData::Bool(ne), combined_marks)
            }
            _ => {
                if let Some(cap_res) =
                    self.try_eval_capsule_binary_op(op, &l_val, &r_val, &combined_marks, &span)
                {
                    return cap_res;
                }
                if op == BinaryOp::Add
                    && l_val.ty() == &Type::String
                    && r_val.ty() == &Type::String
                    && (l_val.coerce(&Type::Number).is_err()
                        || r_val.coerce(&Type::Number).is_err())
                {
                    if let (ValueData::String(str1), ValueData::String(str2)) =
                        (&*l_val.data, &*r_val.data)
                    {
                        return Value::new_with_marks(
                            Type::String,
                            ValueData::String(format!("{str1}{str2}")),
                            combined_marks,
                        );
                    }
                    let msg = format!("Cannot concatenate string operands of {op:?}");
                    self.push_diagnostic(Diagnostic::error(msg, "", span));
                    return Value::unknown(Type::String).with_marks(combined_marks);
                }
                let (l_num, r_num) = if let (Ok(l_c), Ok(r_c)) =
                    (l_val.coerce(&Type::Number), r_val.coerce(&Type::Number))
                {
                    if let (ValueData::Number(ln), ValueData::Number(rn)) = (&*l_c.data, &*r_c.data)
                    {
                        (ln.clone(), rn.clone())
                    } else {
                        let msg = format!("Binary operator {op:?} requires numbers");
                        self.push_diagnostic(Diagnostic::error(msg, "", span));
                        return Value::unknown(Type::Number).with_marks(combined_marks);
                    }
                } else {
                    let msg = format!("Cannot coerce operands of {op:?} to numbers");
                    self.push_diagnostic(Diagnostic::error(msg, "", span));
                    return Value::unknown(Type::Number).with_marks(combined_marks);
                };
                self.eval_numeric_binary_op(op, l_num, r_num, combined_marks, span)
            }
        }
    }

    fn eval_numeric_binary_op(
        &mut self,
        op: BinaryOp,
        l_num: Number,
        r_num: Number,
        combined_marks: std::collections::BTreeSet<crate::types::ValueMark>,
        span: crate::span::Span,
    ) -> Value {
        match op {
            BinaryOp::Add => Value::new_with_marks(
                Type::Number,
                ValueData::Number(l_num + r_num),
                combined_marks,
            ),
            BinaryOp::Sub => Value::new_with_marks(
                Type::Number,
                ValueData::Number(l_num - r_num),
                combined_marks,
            ),
            BinaryOp::Mul => Value::new_with_marks(
                Type::Number,
                ValueData::Number(l_num * r_num),
                combined_marks,
            ),
            BinaryOp::Div => {
                if let Some(res) = l_num.checked_div(&r_num) {
                    Value::new_with_marks(Type::Number, ValueData::Number(res), combined_marks)
                } else {
                    self.diagnostics
                        .push(Diagnostic::error("Division by zero", "", span));
                    Value::unknown(Type::Number).with_marks(combined_marks)
                }
            }
            BinaryOp::Mod => {
                if let Some(res) = l_num.checked_rem(&r_num) {
                    Value::new_with_marks(Type::Number, ValueData::Number(res), combined_marks)
                } else {
                    self.push_diagnostic(Diagnostic::error("Division by zero in modulo", "", span));
                    Value::unknown(Type::Number).with_marks(combined_marks)
                }
            }
            BinaryOp::Less => {
                Value::new_with_marks(Type::Bool, ValueData::Bool(l_num < r_num), combined_marks)
            }
            BinaryOp::LessEq => {
                Value::new_with_marks(Type::Bool, ValueData::Bool(l_num <= r_num), combined_marks)
            }
            BinaryOp::Greater => {
                Value::new_with_marks(Type::Bool, ValueData::Bool(l_num > r_num), combined_marks)
            }
            _ => Value::new_with_marks(Type::Bool, ValueData::Bool(l_num >= r_num), combined_marks),
        }
    }
    fn eval_unary_op(&mut self, op: UnaryOp, inner: &Expression, span: crate::span::Span) -> Value {
        let val = self.eval_expr(inner);
        let marks = val.marks.clone();
        if val.is_unknown() {
            return match op {
                UnaryOp::Not => Value::unknown(Type::Bool).with_marks(marks),
                UnaryOp::Neg => {
                    if val.ty().is_capsule() {
                        Value::unknown(val.ty().clone()).with_marks(marks)
                    } else {
                        Value::unknown(Type::Number).with_marks(marks)
                    }
                }
            };
        }
        match op {
            UnaryOp::Not => {
                if let ValueData::Bool(b) = &*val.data {
                    Value::new_with_marks(Type::Bool, ValueData::Bool(!b), marks)
                } else {
                    self.push_diagnostic(Diagnostic::error(
                        "Operand of ! must be a boolean",
                        "",
                        inner.span(),
                    ));
                    Value::unknown(Type::Bool).with_marks(marks)
                }
            }
            UnaryOp::Neg => {
                if let (Some(any), Some(ops)) = (val.as_capsule_any(), val.ty().capsule_ops()) {
                    if let Some(ref neg_fn) = ops.neg {
                        match (neg_fn)(any) {
                            Ok(boxed) => Value::new_with_marks(
                                val.ty().clone(),
                                ValueData::Capsule(std::sync::Arc::from(boxed)),
                                marks,
                            ),
                            Err(err) => {
                                self.push_diagnostic(Diagnostic::error(err.to_string(), "", span));
                                Value::unknown(val.ty().clone()).with_marks(marks)
                            }
                        }
                    } else {
                        self.push_diagnostic(Diagnostic::error(
                            format!(
                                "Capsule type '{}' does not support unary negation",
                                ops.type_name
                            ),
                            "",
                            inner.span(),
                        ));
                        Value::unknown(val.ty().clone()).with_marks(marks)
                    }
                } else if let ValueData::Number(n) = &*val.data {
                    Value::new_with_marks(Type::Number, ValueData::Number(-(n.clone())), marks)
                } else {
                    self.push_diagnostic(Diagnostic::error(
                        "Operand of - must be a number",
                        "",
                        inner.span(),
                    ));
                    Value::unknown(Type::Number).with_marks(marks)
                }
            }
        }
    }
    fn eval_get_attr_step(
        &mut self,
        current: &Value,
        name: &str,
        op_span: &crate::span::Span,
        parent_marks: std::collections::BTreeSet<crate::types::ValueMark>,
    ) -> Value {
        if let ValueData::Object(map) = &*current.data {
            if let Some(val) = map.get(name) {
                let mut v = val.clone();
                v.marks.extend(parent_marks);
                v
            } else {
                let msg = format!("Attribute '{name}' not found");
                let mut diag = Diagnostic::error(msg, "", op_span.clone());
                if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                    name,
                    map.keys().map(String::as_str),
                    2,
                ) {
                    diag = diag.with_detail(format!("Did you mean .{candidate}?"));
                }
                self.push_diagnostic(diag);
                Value::unknown(Type::Dynamic).with_marks(parent_marks)
            }
        } else if let (Some(any), Some(ops)) =
            (current.as_capsule_any(), current.ty().capsule_ops())
        {
            if let Some(ref attr_get) = ops.attr_get {
                match (attr_get)(any, name) {
                    Ok(mut res) => {
                        res.marks.extend(parent_marks);
                        res
                    }
                    Err(err) => {
                        self.push_diagnostic(Diagnostic::error(
                            err.to_string(),
                            "",
                            op_span.clone(),
                        ));
                        Value::unknown(Type::Dynamic).with_marks(parent_marks)
                    }
                }
            } else {
                self.push_diagnostic(Diagnostic::error(
                    format!(
                        "Capsule type '{}' does not support attribute access",
                        ops.type_name
                    ),
                    "",
                    op_span.clone(),
                ));
                Value::unknown(Type::Dynamic).with_marks(parent_marks)
            }
        } else {
            self.push_diagnostic(Diagnostic::error(
                "Cannot get attribute from non-object",
                "",
                op_span.clone(),
            ));
            Value::unknown(Type::Dynamic).with_marks(parent_marks)
        }
    }

    fn eval_index_step(
        &mut self,
        current: &Value,
        idx_expr: &Expression,
        op_span: &crate::span::Span,
        parent_marks: std::collections::BTreeSet<crate::types::ValueMark>,
    ) -> Value {
        let idx_val = self.eval_expr(idx_expr);
        let mut op_marks = parent_marks;
        op_marks.extend(idx_val.marks.clone());
        if idx_val.is_unknown() {
            return Value::unknown(Type::Dynamic).with_marks(op_marks);
        }
        if let (Some(any), Some(ops)) = (current.as_capsule_any(), current.ty().capsule_ops()) {
            if let Some(ref index_get) = ops.index_get {
                match (index_get)(any, &idx_val) {
                    Ok(mut res) => {
                        res.marks.extend(op_marks);
                        res
                    }
                    Err(err) => {
                        self.push_diagnostic(Diagnostic::error(
                            err.to_string(),
                            "",
                            op_span.clone(),
                        ));
                        Value::unknown(Type::Dynamic).with_marks(op_marks)
                    }
                }
            } else {
                self.push_diagnostic(Diagnostic::error(
                    format!("Capsule type '{}' does not support indexing", ops.type_name),
                    "",
                    op_span.clone(),
                ));
                Value::unknown(Type::Dynamic).with_marks(op_marks)
            }
        } else {
            match (&*current.data, &*idx_val.data) {
                (ValueData::Array(arr), ValueData::Number(n)) => {
                    if let Some(i) = n.0.to_usize() {
                        if i < arr.len() {
                            let mut res = arr[i].clone();
                            res.marks.extend(op_marks);
                            res
                        } else {
                            self.push_diagnostic(Diagnostic::error(
                                "Index out of bounds",
                                "",
                                op_span.clone(),
                            ));
                            Value::unknown(Type::Dynamic).with_marks(op_marks)
                        }
                    } else {
                        self.push_diagnostic(Diagnostic::error(
                            "Invalid array index",
                            "",
                            op_span.clone(),
                        ));
                        Value::unknown(Type::Dynamic).with_marks(op_marks)
                    }
                }
                (ValueData::Object(map), ValueData::String(s)) => {
                    if let Some(val) = map.get(s) {
                        let mut res = val.clone();
                        res.marks.extend(op_marks);
                        res
                    } else {
                        let msg = format!("Key '{s}' not found");
                        let mut diag = Diagnostic::error(msg, "", op_span.clone());
                        if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                            s,
                            map.keys().map(String::as_str),
                            2,
                        ) {
                            diag = diag.with_detail(format!("Did you mean .{candidate}?"));
                        }
                        self.push_diagnostic(diag);
                        Value::unknown(Type::Dynamic).with_marks(op_marks)
                    }
                }
                _ => {
                    self.push_diagnostic(Diagnostic::error(
                        "Invalid index operation",
                        "",
                        op_span.clone(),
                    ));
                    Value::unknown(Type::Dynamic).with_marks(op_marks)
                }
            }
        }
    }

    fn eval_single_step(&mut self, current: &Value, op: &TraversalOperator) -> Value {
        if current.is_unknown() {
            return Value::unknown(Type::Dynamic).with_marks(current.marks.clone());
        }
        if current.is_null() {
            let marks = current.marks.clone();
            self.push_diagnostic(Diagnostic::error(
                "Attempt to traverse null value",
                "",
                op.span(),
            ));
            return Value::unknown(Type::Dynamic).with_marks(marks);
        }
        let parent_marks = current.marks.clone();
        match op {
            TraversalOperator::GetAttr(name, op_span) => {
                self.eval_get_attr_step(current, name, op_span, parent_marks)
            }
            TraversalOperator::Index(idx_expr, op_span) => {
                self.eval_index_step(current, idx_expr, op_span, parent_marks)
            }
            TraversalOperator::LegacyIndex(idx, op_span) => {
                if let (Some(any), Some(ops)) =
                    (current.as_capsule_any(), current.ty().capsule_ops())
                {
                    if let Some(ref index_get) = ops.index_get {
                        let idx_val = Value::new(
                            Type::Number,
                            ValueData::Number(crate::number::Number::from(*idx)),
                        );
                        match (index_get)(any, &idx_val) {
                            Ok(mut res) => {
                                res.marks.extend(parent_marks);
                                res
                            }
                            Err(err) => {
                                self.push_diagnostic(Diagnostic::error(
                                    err.to_string(),
                                    "",
                                    op_span.clone(),
                                ));
                                Value::unknown(Type::Dynamic).with_marks(parent_marks)
                            }
                        }
                    } else {
                        self.push_diagnostic(Diagnostic::error(
                            format!("Capsule type '{}' does not support indexing", ops.type_name),
                            "",
                            op_span.clone(),
                        ));
                        Value::unknown(Type::Dynamic).with_marks(parent_marks)
                    }
                } else if let ValueData::Array(arr) = &*current.data {
                    if usize::try_from(*idx).is_ok_and(|i| i < arr.len()) {
                        let mut res = arr[usize::try_from(*idx).unwrap_or_default()].clone();
                        res.marks.extend(parent_marks);
                        res
                    } else {
                        self.push_diagnostic(Diagnostic::error(
                            "Index out of bounds",
                            "",
                            op_span.clone(),
                        ));
                        Value::unknown(Type::Dynamic).with_marks(parent_marks)
                    }
                } else {
                    self.push_diagnostic(Diagnostic::error(
                        "Legacy index on non-array",
                        "",
                        op_span.clone(),
                    ));
                    Value::unknown(Type::Dynamic).with_marks(parent_marks)
                }
            }
            TraversalOperator::AttrSplat(op_span) | TraversalOperator::FullSplat(op_span) => {
                self.push_diagnostic(Diagnostic::error(
                    "Unexpected nested splat operator in single step",
                    "",
                    op_span.clone(),
                ));
                Value::unknown(Type::Dynamic).with_marks(parent_marks)
            }
        }
    }

    fn eval_attr_splat_step(
        &mut self,
        current: &mut Value,
        operators: &[TraversalOperator],
        op_idx: &mut usize,
    ) {
        let parent_marks = current.marks.clone();
        if current.is_unknown() {
            *current = Value::unknown(Type::List(Box::new(Type::Dynamic))).with_marks(parent_marks);
            *op_idx += 1;
            return;
        }
        if current.is_null() {
            *current = Value::new(
                Type::List(Box::new(Type::Dynamic)),
                ValueData::Array(Vec::new()),
            )
            .with_marks(parent_marks);
            *op_idx += 1;
            while *op_idx < operators.len() {
                if let TraversalOperator::GetAttr(..) = &operators[*op_idx] {
                    *op_idx += 1;
                } else {
                    break;
                }
            }
            return;
        }
        let elements = match &*current.data {
            ValueData::Array(arr) => arr.clone(),
            ValueData::Set(set) => set.iter().cloned().collect(),
            _ => vec![current.clone()],
        };
        *op_idx += 1;
        let mut sub_attrs = Vec::new();
        while *op_idx < operators.len() {
            if let TraversalOperator::GetAttr(name, s) = &operators[*op_idx] {
                sub_attrs.push((name.clone(), s.clone()));
                *op_idx += 1;
            } else {
                break;
            }
        }
        if elements.is_empty() {
            *current = Value::new(
                Type::List(Box::new(Type::Dynamic)),
                ValueData::Array(Vec::new()),
            )
            .with_marks(parent_marks);
            return;
        }
        let mut results = Vec::with_capacity(elements.len());
        for mut elem in elements {
            for (attr_name, attr_span) in &sub_attrs {
                elem = self.eval_single_step(
                    &elem,
                    &TraversalOperator::GetAttr(attr_name.clone(), attr_span.clone()),
                );
            }
            results.push(elem);
        }
        let elem_type = results.first().map_or(Type::Dynamic, |v| v.ty().clone());
        *current = Value::new(Type::List(Box::new(elem_type)), ValueData::Array(results))
            .with_marks(parent_marks);
    }

    fn eval_traversal(
        &mut self,
        traversal: &crate::ast::expr::Traversal,
        span: crate::span::Span,
    ) -> Value {
        let mut current = self.eval_expr(&traversal.expr);
        let mut op_idx = 0;
        while op_idx < traversal.operators.len() {
            let op = &traversal.operators[op_idx];
            match op {
                TraversalOperator::AttrSplat(_op_span) => {
                    self.eval_attr_splat_step(&mut current, &traversal.operators, &mut op_idx);
                }
                TraversalOperator::FullSplat(op_span) => {
                    let parent_marks = current.marks.clone();
                    if current.is_unknown() {
                        current = Value::unknown(Type::List(Box::new(Type::Dynamic)))
                            .with_marks(parent_marks);
                        op_idx += 1;
                        continue;
                    }
                    if current.is_null() {
                        current = Value::new(
                            Type::List(Box::new(Type::Dynamic)),
                            ValueData::Array(Vec::new()),
                        )
                        .with_marks(parent_marks);
                        op_idx += 1;
                        while op_idx < traversal.operators.len() {
                            if traversal.operators[op_idx].is_splat() {
                                break;
                            }
                            op_idx += 1;
                        }
                        continue;
                    }
                    let elements = match &*current.data {
                        ValueData::Array(arr) => arr.clone(),
                        ValueData::Set(set) => set.iter().cloned().collect(),
                        _ => {
                            self.push_diagnostic(Diagnostic::error(
                                "Cannot apply full splat operator to non-sequence value",
                                "",
                                op_span.clone(),
                            ));
                            return Value::unknown(Type::Dynamic).with_marks(parent_marks);
                        }
                    };
                    op_idx += 1;
                    let mut sub_ops = Vec::new();
                    while op_idx < traversal.operators.len() {
                        if traversal.operators[op_idx].is_splat() {
                            break;
                        }
                        sub_ops.push(traversal.operators[op_idx].clone());
                        op_idx += 1;
                    }
                    if elements.is_empty() {
                        current = Value::new(
                            Type::List(Box::new(Type::Dynamic)),
                            ValueData::Array(Vec::new()),
                        )
                        .with_marks(parent_marks);
                        continue;
                    }
                    let mut results = Vec::with_capacity(elements.len());
                    for mut elem in elements {
                        for sub_op in &sub_ops {
                            elem = self.eval_single_step(&elem, sub_op);
                        }
                        results.push(elem);
                    }
                    let elem_type = results.first().map_or(Type::Dynamic, |v| v.ty().clone());
                    current =
                        Value::new(Type::List(Box::new(elem_type)), ValueData::Array(results))
                            .with_marks(parent_marks);
                }
                _ => {
                    current = self.eval_single_step(&current, op);
                    op_idx += 1;
                }
            }
        }
        let trav_text = format!("{traversal}");
        self.eval_callouts
            .push(crate::diagnostic::EvalCallout::from_value(
                span, trav_text, &current,
            ));
        current
    }

    fn eval_for_item(
        &mut self,
        for_expr: &ForExpr,
        k_val: Value,
        v_val: Value,
        out_array: &mut Vec<Value>,
        out_map: &mut BTreeMap<String, Value>,
        out_groups: &mut BTreeMap<String, Vec<Value>>,
    ) {
        let mut iter_ctx = Context::new_child(self.ctx);
        if let Some(ref k_name) = for_expr.key_var {
            iter_ctx.set_variable(k_name, k_val);
        }
        iter_ctx.set_variable(&for_expr.val_var, v_val);
        let mut sub_eval = Evaluator::new(&iter_ctx);
        if let Some(ref cond_expr) = for_expr.cond_expr {
            let cond_res = sub_eval.eval_expr(cond_expr);
            match &*cond_res.data {
                ValueData::Bool(true) => {}
                ValueData::Bool(false) => {
                    return;
                }
                _ => {
                    for e in sub_eval.diagnostics.errors() {
                        self.push_diagnostic(e.clone());
                    }
                    self.push_diagnostic(Diagnostic::error(
                        "For expression condition must be a boolean",
                        "",
                        cond_expr.span(),
                    ));
                    return;
                }
            }
        }
        let res_v = sub_eval.eval_expr(&for_expr.val_expr);
        if let Some(ref key_expr) = for_expr.key_expr {
            let res_k = sub_eval.eval_expr(key_expr);
            match &*res_k.data {
                ValueData::String(s) => {
                    if for_expr.grouping {
                        out_groups.entry(s.clone()).or_default().push(res_v);
                    } else {
                        out_map.insert(s.clone(), res_v);
                    }
                }
                _ => {
                    self.push_diagnostic(Diagnostic::error(
                        "For expression key must evaluate to string",
                        "",
                        key_expr.span(),
                    ));
                }
            }
        } else {
            out_array.push(res_v);
        }
        for e in sub_eval.diagnostics.errors() {
            self.push_diagnostic(e.clone());
        }
    }

    fn eval_for(&mut self, for_expr: &ForExpr, _span: crate::span::Span) -> Value {
        let coll_val = self.eval_expr(&for_expr.collection);
        if coll_val.is_unknown() {
            return Value::unknown(Type::Dynamic).with_marks(coll_val.marks);
        }
        let coll_marks = coll_val.marks.clone();
        let items: Vec<(Value, Value)> = match &*coll_val.data {
            ValueData::Array(arr) => arr
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    (
                        Value::new(
                            Type::Number,
                            ValueData::Number(Number::new(bigdecimal::BigDecimal::from(i as u64))),
                        ),
                        v.clone(),
                    )
                })
                .collect(),
            ValueData::Object(map) => map
                .iter()
                .map(|(k, v)| {
                    (
                        Value::new(Type::String, ValueData::String(k.clone())),
                        v.clone(),
                    )
                })
                .collect(),
            _ => {
                self.push_diagnostic(Diagnostic::error(
                    "For expression collection must be a tuple or object",
                    "",
                    for_expr.collection.span(),
                ));
                return Value::unknown(Type::Dynamic).with_marks(coll_marks);
            }
        };
        let mut out_array = Vec::new();
        let mut out_map = BTreeMap::new();
        let mut out_groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (k_val, v_val) in items {
            self.eval_for_item(
                for_expr,
                k_val,
                v_val,
                &mut out_array,
                &mut out_map,
                &mut out_groups,
            );
        }
        if for_expr.grouping {
            let mut final_map = BTreeMap::new();
            let mut types = BTreeMap::new();
            for (k, group) in out_groups {
                let val = Value::new(Type::Tuple(vec![]), ValueData::Array(group));
                types.insert(k.clone(), val.ty().clone());
                final_map.insert(k, val);
            }
            Value::new_with_marks(
                Type::object(types),
                ValueData::Object(final_map),
                coll_marks,
            )
        } else if for_expr.key_expr.is_some() {
            let mut types = BTreeMap::new();
            for (k, v) in &out_map {
                types.insert(k.clone(), v.ty().clone());
            }
            Value::new_with_marks(Type::object(types), ValueData::Object(out_map), coll_marks)
        } else {
            let types = out_array.iter().map(|v| v.ty().clone()).collect();
            Value::new_with_marks(Type::Tuple(types), ValueData::Array(out_array), coll_marks)
        }
    }
    /// Evaluates a slice of template parts, appending literal text and interpolated values to `out`.
    ///
    /// # Arguments
    /// * `parts` - The slice of [`TemplatePart`](crate::ast::expr::TemplatePart) items to evaluate.
    /// * `out` - Target string buffer accumulating rendered template output.
    /// * `has_unknown` - Flag set to `true` if any sub-expression yields an unknown value during evaluation.
    /// * `marks` - Target set accumulating marks from interpolated values and control directives.
    fn eval_template_parts(
        &mut self,
        parts: &[crate::ast::expr::TemplatePart],
        out: &mut String,
        has_unknown: &mut bool,
        marks: &mut std::collections::BTreeSet<crate::types::ValueMark>,
    ) {
        for part in parts {
            match part {
                crate::ast::expr::TemplatePart::Literal(s, _) => out.push_str(s),
                crate::ast::expr::TemplatePart::Interpolation(e, span) => {
                    let val = self.eval_expr(e);
                    marks.extend(val.marks.clone());
                    if val.is_unknown() {
                        *has_unknown = true;
                        continue;
                    }
                    match &*val.data {
                        ValueData::String(s) => out.push_str(s),
                        ValueData::Number(n) => out.push_str(&n.0.to_string()),
                        ValueData::Bool(b) => out.push_str(&b.to_string()),
                        ValueData::Null => {}
                        _ => {
                            self.push_diagnostic(Diagnostic::error(
                                "Cannot stringify complex value in interpolation",
                                "",
                                span.clone(),
                            ));
                        }
                    }
                }
                crate::ast::expr::TemplatePart::Directive(directive, span) => {
                    self.eval_directive(directive, span.clone(), out, has_unknown, marks);
                }
            }
        }
    }
    /// Evaluates a template control directive (`if` condition or `for` loop).
    ///
    /// # Arguments
    /// * `directive` - The control directive to evaluate.
    /// * `span` - Source span of the directive for error reporting.
    /// * `out` - Target string buffer accumulating rendered template output.
    /// * `has_unknown` - Flag set to `true` if an evaluated condition or collection is unknown.
    /// * `marks` - Target set accumulating marks from conditions, collections, and inner template parts.
    fn eval_directive(
        &mut self,
        directive: &crate::ast::expr::Directive,
        _span: crate::span::Span,
        out: &mut String,
        has_unknown: &mut bool,
        marks: &mut std::collections::BTreeSet<crate::types::ValueMark>,
    ) {
        match directive {
            crate::ast::expr::Directive::If {
                cond,
                true_expr,
                else_ifs,
                false_expr,
            } => {
                let cond_val = self.eval_expr(cond);
                marks.extend(cond_val.marks.clone());
                if cond_val.is_unknown() {
                    *has_unknown = true;
                    return;
                }
                match &*cond_val.data {
                    ValueData::Bool(true) => {
                        self.eval_template_parts(true_expr, out, has_unknown, marks);
                    }
                    ValueData::Bool(false) => {
                        for (elif_cond, elif_parts) in else_ifs {
                            let elif_val = self.eval_expr(elif_cond);
                            marks.extend(elif_val.marks.clone());
                            if elif_val.is_unknown() {
                                *has_unknown = true;
                                return;
                            }
                            match &*elif_val.data {
                                ValueData::Bool(true) => {
                                    self.eval_template_parts(elif_parts, out, has_unknown, marks);
                                    return;
                                }
                                ValueData::Bool(false) => {}
                                _ => {
                                    self.push_diagnostic(Diagnostic::error(
                                        "Condition in %{ else if } must evaluate to a boolean",
                                        "",
                                        elif_cond.span(),
                                    ));
                                    return;
                                }
                            }
                        }
                        if let Some(false_parts) = false_expr {
                            self.eval_template_parts(false_parts, out, has_unknown, marks);
                        }
                    }
                    _ => {
                        self.push_diagnostic(Diagnostic::error(
                            "Condition in %{ if } must evaluate to a boolean",
                            "",
                            cond.span(),
                        ));
                    }
                }
            }
            crate::ast::expr::Directive::For {
                key_var,
                val_var,
                collection,
                body,
            } => {
                self.eval_for_directive(
                    (key_var.as_deref(), val_var, collection, body),
                    out,
                    has_unknown,
                    marks,
                );
            }
            crate::ast::expr::Directive::Strip { .. } => {}
        }
    }

    fn eval_for_directive(
        &mut self,
        (key_var, val_var, collection, body): (
            Option<&str>,
            &str,
            &Expression,
            &[crate::ast::expr::TemplatePart],
        ),
        out: &mut String,
        has_unknown: &mut bool,
        marks: &mut std::collections::BTreeSet<crate::types::ValueMark>,
    ) {
        let coll_val = self.eval_expr(collection);
        marks.extend(coll_val.marks.clone());
        if coll_val.is_unknown() {
            *has_unknown = true;
            return;
        }
        if coll_val.is_null() {
            let msg = "A null value cannot be used as the collection in a for directive";
            self.push_diagnostic(Diagnostic::error(
                "Iteration over null value in %{ for } directive",
                msg,
                collection.span(),
            ));
            return;
        }
        let items: Vec<(Value, Value)> = match &*coll_val.data {
            ValueData::Array(arr) => arr
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    (
                        Value::new(
                            Type::Number,
                            ValueData::Number(Number::new(bigdecimal::BigDecimal::from(i as u64))),
                        ),
                        v.clone(),
                    )
                })
                .collect(),
            ValueData::Set(set) => set.iter().map(|v| (v.clone(), v.clone())).collect(),
            ValueData::Object(obj) => obj
                .iter()
                .map(|(k, v)| {
                    (
                        Value::new(Type::String, ValueData::String(k.clone())),
                        v.clone(),
                    )
                })
                .collect(),
            _ => {
                self.push_diagnostic(Diagnostic::error(
                    "Collection in %{ for } directive must be a tuple, list, set, or object",
                    "",
                    collection.span(),
                ));
                return;
            }
        };
        for (k_val, v_val) in items {
            let mut iter_ctx = Context::new_child(self.ctx);
            if let Some(k_name) = key_var {
                iter_ctx.set_variable(k_name, k_val);
            }
            iter_ctx.set_variable(val_var, v_val);
            let mut sub_eval = Evaluator::new(&iter_ctx);
            sub_eval.eval_template_parts(body, out, has_unknown, marks);
            for e in sub_eval.diagnostics.errors() {
                self.push_diagnostic(e.clone());
            }
        }
    }
}
#[cfg(test)]
#[path = "evaluator_tests.rs"]
mod tests;
