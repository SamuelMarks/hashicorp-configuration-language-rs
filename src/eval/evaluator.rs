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
    pub fn new(ctx: &'a Context<'a>) -> Self {
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
                            // Coerce to string if possible, for now just err if not string
                            // HCL keys are strings.

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
            if f.args.is_empty() {
                self.push_diagnostic(Diagnostic::error(
                    "The 'try' function requires at least one argument",
                    "",
                    span,
                ));
                return Value::unknown(Type::Dynamic);
            }

            for arg_expr in &f.args {
                let mut isolated = Evaluator::new(self.ctx);
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
            return Value::unknown(Type::Dynamic);
        }

        if f.name == "can" {
            if f.args.len() != 1 {
                self.push_diagnostic(Diagnostic::error(
                    "The 'can' function requires exactly one argument",
                    "",
                    span,
                ));
                return Value::unknown(Type::Bool);
            }

            let mut isolated = Evaluator::new(self.ctx);
            let val = isolated.eval_expr(&f.args[0]);
            return if isolated.diagnostics.has_errors() {
                Value::new(Type::Bool, ValueData::Bool(false))
            } else if val.is_unknown() {
                Value::unknown(Type::Bool)
            } else {
                Value::new(Type::Bool, ValueData::Bool(true))
            };
        }

        let mut args = Vec::new();
        let mut arg_marks = std::collections::BTreeSet::new();
        for arg_expr in &f.args {
            let evaluated_arg = self.eval_expr(arg_expr);
            arg_marks.extend(evaluated_arg.marks.clone());
            args.push(evaluated_arg);
        }

        if f.expand_final {
            let Some(final_arg) = args.pop() else {
                return Value::unknown(Type::Dynamic).with_marks(arg_marks);
            };
            if final_arg.is_unknown() {
                return Value::unknown(Type::Dynamic).with_marks(arg_marks);
            }
            match &*final_arg.data {
                ValueData::Array(arr) => {
                    for item in arr {
                        let mut unpacked = item.clone();
                        unpacked.marks.extend(final_arg.marks.clone());
                        args.push(unpacked);
                    }
                }
                ValueData::Set(set) => {
                    for item in set {
                        let mut unpacked = item.clone();
                        unpacked.marks.extend(final_arg.marks.clone());
                        args.push(unpacked);
                    }
                }
                _ => {
                    let msg = format!(
                        "Cannot expand non-sequence value of type {} using '...'",
                        final_arg.ty()
                    );
                    self.push_diagnostic(Diagnostic::error("Invalid expanded argument", msg, span));
                    return Value::unknown(Type::Dynamic).with_marks(arg_marks);
                }
            }
        }

        if let Some(func) = self.ctx.get_namespaced_func(&f.name) {
            if let Some(sig) = &func.signature {
                let expected_fixed = sig.params.len();
                if let Some(variadic) = &sig.variadic_param {
                    if args.len() < expected_fixed {
                        let msg = format!(
                            "Too few arguments for function '{}': expected at least {}, got {}",
                            f.name,
                            expected_fixed,
                            args.len()
                        );
                        self.diagnostics.push(Diagnostic::error(msg, "", span));
                        return Value::unknown(Type::Dynamic).with_marks(arg_marks);
                    }
                    for (idx, arg) in args.iter_mut().enumerate() {
                        let spec = if idx < expected_fixed {
                            &sig.params[idx]
                        } else {
                            variadic
                        };
                        if arg.is_null() && !spec.allow_null {
                            let msg = format!(
                                "Argument '{}' for function '{}' cannot be null",
                                spec.name, f.name
                            );
                            self.diagnostics.push(Diagnostic::error(msg, "", span));
                            return Value::unknown(Type::Dynamic).with_marks(arg_marks);
                        }
                        if spec.param_type != Type::Dynamic
                            && !arg.ty().is_dynamic()
                            && arg.ty() != &spec.param_type
                        {
                            match arg.coerce(&spec.param_type) {
                                Ok(coerced) => *arg = coerced,
                                Err(err) => {
                                    let msg = format!(
                                        "Invalid argument '{}' in call to '{}'",
                                        spec.name, f.name
                                    );
                                    self.push_diagnostic(Diagnostic::error(msg, err, span));
                                    return Value::unknown(Type::Dynamic).with_marks(arg_marks);
                                }
                            }
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
                        self.diagnostics.push(Diagnostic::error(msg, "", span));
                        return Value::unknown(Type::Dynamic).with_marks(arg_marks);
                    }
                    for (idx, arg) in args.iter_mut().enumerate() {
                        let spec = &sig.params[idx];
                        if arg.is_null() && !spec.allow_null {
                            let msg = format!(
                                "Argument '{}' for function '{}' cannot be null",
                                spec.name, f.name
                            );
                            self.diagnostics.push(Diagnostic::error(msg, "", span));
                            return Value::unknown(Type::Dynamic).with_marks(arg_marks);
                        }
                        if spec.param_type != Type::Dynamic
                            && !arg.ty().is_dynamic()
                            && arg.ty() != &spec.param_type
                        {
                            match arg.coerce(&spec.param_type) {
                                Ok(coerced) => *arg = coerced,
                                Err(err) => {
                                    let msg = format!(
                                        "Invalid argument '{}' in call to '{}'",
                                        spec.name, f.name
                                    );
                                    self.push_diagnostic(Diagnostic::error(msg, err, span));
                                    return Value::unknown(Type::Dynamic).with_marks(arg_marks);
                                }
                            }
                        }
                    }
                }
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
                        e,
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
            // If condition is unknown, the result is unknown, but we must unify the types
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

    fn eval_binary_op(
        &mut self,
        op: BinaryOp,
        lhs: &Expression,
        rhs: &Expression,
        span: crate::span::Span,
    ) -> Value {
        if op == BinaryOp::And {
            let l_val = self.eval_expr(lhs);
            if let ValueData::Bool(false) = &*l_val.data {
                return Value::new_with_marks(
                    Type::Bool,
                    ValueData::Bool(false),
                    l_val.marks.clone(),
                );
            }
            if l_val.is_unknown() {
                return Value::unknown(Type::Bool).with_marks(l_val.marks.clone());
            }
            let r_val = self.eval_expr(rhs);
            let mut marks = l_val.marks.clone();
            marks.extend(r_val.marks.clone());
            return if let ValueData::Bool(b) = &*r_val.data {
                Value::new_with_marks(Type::Bool, ValueData::Bool(*b), marks)
            } else if let ValueData::Unknown(_) = &*r_val.data {
                Value::unknown(Type::Bool).with_marks(marks)
            } else {
                self.push_diagnostic(Diagnostic::error(
                    "Right operand of && must be a boolean",
                    "",
                    rhs.span(),
                ));
                Value::unknown(Type::Bool).with_marks(marks)
            };
        }

        if op == BinaryOp::Or {
            let l_val = self.eval_expr(lhs);
            if let ValueData::Bool(true) = &*l_val.data {
                return Value::new_with_marks(
                    Type::Bool,
                    ValueData::Bool(true),
                    l_val.marks.clone(),
                );
            }
            if l_val.is_unknown() {
                return Value::unknown(Type::Bool).with_marks(l_val.marks.clone());
            }
            let r_val = self.eval_expr(rhs);
            let mut marks = l_val.marks.clone();
            marks.extend(r_val.marks.clone());
            return if let ValueData::Bool(b) = &*r_val.data {
                Value::new_with_marks(Type::Bool, ValueData::Bool(*b), marks)
            } else if let ValueData::Unknown(_) = &*r_val.data {
                Value::unknown(Type::Bool).with_marks(marks)
            } else {
                self.push_diagnostic(Diagnostic::error(
                    "Right operand of || must be a boolean",
                    "",
                    rhs.span(),
                ));
                Value::unknown(Type::Bool).with_marks(marks)
            };
        }

        let l_val = self.eval_expr(lhs);
        let r_val = self.eval_expr(rhs);

        let mut combined_marks = l_val.marks.clone();
        combined_marks.extend(r_val.marks.clone());

        if l_val.is_unknown() || r_val.is_unknown() {
            match op {
                BinaryOp::Eq
                | BinaryOp::NotEq
                | BinaryOp::Less
                | BinaryOp::LessEq
                | BinaryOp::Greater
                | BinaryOp::GreaterEq => {
                    return Value::unknown(Type::Bool).with_marks(combined_marks);
                }
                BinaryOp::Add => {
                    let is_num =
                        l_val.coerce(&Type::Number).is_ok() && r_val.coerce(&Type::Number).is_ok();
                    if !is_num && (l_val.ty() == &Type::String && r_val.ty() == &Type::String) {
                        let mut ref_res = crate::types::refinement::Refinement::not_null();
                        let l_prefix = match &*l_val.data {
                            ValueData::String(s) => Some(s.clone()),
                            ValueData::Unknown(_) => {
                                l_val.refinement().and_then(|r| r.string_prefix.clone())
                            }
                            _ => None,
                        };
                        let r_prefix = match &*r_val.data {
                            ValueData::String(s) => Some(s.clone()),
                            ValueData::Unknown(_) => {
                                r_val.refinement().and_then(|r| r.string_prefix.clone())
                            }
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
                            ValueData::Unknown(_) => {
                                l_val.refinement().and_then(|r| r.string_suffix.clone())
                            }
                            _ => None,
                        };
                        let r_suffix = match &*r_val.data {
                            ValueData::String(s) => Some(s.clone()),
                            ValueData::Unknown(_) => {
                                r_val.refinement().and_then(|r| r.string_suffix.clone())
                            }
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
                            ValueData::Unknown(_) => {
                                l_val.refinement().and_then(|r| r.string_length_max)
                            }
                            _ => None,
                        };
                        let r_max = match &*r_val.data {
                            ValueData::String(s) => Some(s.chars().count()),
                            ValueData::Unknown(_) => {
                                r_val.refinement().and_then(|r| r.string_length_max)
                            }
                            _ => None,
                        };
                        if let (Some(l_m), Some(r_m)) = (l_max, r_max) {
                            ref_res.string_length_max = Some(l_m + r_m);
                        }

                        return Value::unknown_refined(Type::String, ref_res)
                            .with_marks(combined_marks);
                    }
                    if l_val.ty().is_capsule() {
                        return Value::unknown(l_val.ty().clone()).with_marks(combined_marks);
                    }
                    if r_val.ty().is_capsule() {
                        return Value::unknown(r_val.ty().clone()).with_marks(combined_marks);
                    }
                    return Value::unknown(Type::Number).with_marks(combined_marks);
                }
                _ => {
                    if l_val.ty().is_capsule() {
                        return Value::unknown(l_val.ty().clone()).with_marks(combined_marks);
                    }
                    if r_val.ty().is_capsule() {
                        return Value::unknown(r_val.ty().clone()).with_marks(combined_marks);
                    }
                    return Value::unknown(Type::Number).with_marks(combined_marks);
                }
            }
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
                // Capsule relational comparison dispatch
                let capsule_cmp = match op {
                    BinaryOp::Less | BinaryOp::LessEq | BinaryOp::Greater | BinaryOp::GreaterEq => {
                        l_val
                            .ty()
                            .capsule_ops()
                            .and_then(|ops| ops.cmp.clone())
                            .or_else(|| r_val.ty().capsule_ops().and_then(|ops| ops.cmp.clone()))
                    }
                    _ => None,
                };
                if let Some(cmp_fn) = capsule_cmp {
                    let l_any: &(dyn std::any::Any + Send + Sync) = match l_val.as_capsule_any() {
                        Some(any) => any,
                        None => &l_val,
                    };
                    let r_any: &(dyn std::any::Any + Send + Sync) = match r_val.as_capsule_any() {
                        Some(any) => any,
                        None => &r_val,
                    };
                    match (cmp_fn)(l_any, r_any) {
                        Ok(ordering) => {
                            let res = match op {
                                BinaryOp::Less => ordering == std::cmp::Ordering::Less,
                                BinaryOp::LessEq => ordering != std::cmp::Ordering::Greater,
                                BinaryOp::Greater => ordering == std::cmp::Ordering::Greater,
                                _ => ordering != std::cmp::Ordering::Less,
                            };
                            return Value::new_with_marks(
                                Type::Bool,
                                ValueData::Bool(res),
                                combined_marks,
                            );
                        }
                        Err(err) => {
                            self.push_diagnostic(Diagnostic::error(err, "", span));
                            return Value::unknown(Type::Bool).with_marks(combined_marks);
                        }
                    }
                }

                // Capsule arithmetic dispatch
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
                    let l_any: &(dyn std::any::Any + Send + Sync) = match l_val.as_capsule_any() {
                        Some(any) => any,
                        None => &l_val,
                    };
                    let r_any: &(dyn std::any::Any + Send + Sync) = match r_val.as_capsule_any() {
                        Some(any) => any,
                        None => &r_val,
                    };
                    let target_ty = if l_val.ty().is_capsule() {
                        l_val.ty().clone()
                    } else {
                        r_val.ty().clone()
                    };
                    match (arith_fn)(l_any, r_any) {
                        Ok(boxed) => {
                            return Value::new_with_marks(
                                target_ty,
                                ValueData::Capsule(std::sync::Arc::from(boxed)),
                                combined_marks,
                            );
                        }
                        Err(err) => {
                            self.push_diagnostic(Diagnostic::error(err, "", span));
                            return Value::unknown(target_ty).with_marks(combined_marks);
                        }
                    }
                }

                if op == BinaryOp::Add
                    && (l_val.ty() == &Type::String && r_val.ty() == &Type::String)
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
                            Value::new_with_marks(
                                Type::Number,
                                ValueData::Number(res),
                                combined_marks,
                            )
                        } else {
                            self.diagnostics
                                .push(Diagnostic::error("Division by zero", "", span));
                            Value::unknown(Type::Number).with_marks(combined_marks)
                        }
                    }
                    BinaryOp::Mod => {
                        if let Some(res) = l_num.checked_rem(&r_num) {
                            Value::new_with_marks(
                                Type::Number,
                                ValueData::Number(res),
                                combined_marks,
                            )
                        } else {
                            self.push_diagnostic(Diagnostic::error(
                                "Division by zero in modulo",
                                "",
                                span,
                            ));
                            Value::unknown(Type::Number).with_marks(combined_marks)
                        }
                    }
                    BinaryOp::Less => Value::new_with_marks(
                        Type::Bool,
                        ValueData::Bool(l_num < r_num),
                        combined_marks,
                    ),
                    BinaryOp::LessEq => Value::new_with_marks(
                        Type::Bool,
                        ValueData::Bool(l_num <= r_num),
                        combined_marks,
                    ),
                    BinaryOp::Greater => Value::new_with_marks(
                        Type::Bool,
                        ValueData::Bool(l_num > r_num),
                        combined_marks,
                    ),
                    _ => Value::new_with_marks(
                        Type::Bool,
                        ValueData::Bool(l_num >= r_num),
                        combined_marks,
                    ),
                }
            }
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
                                self.push_diagnostic(Diagnostic::error(err, "", span));
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
                                self.push_diagnostic(Diagnostic::error(err, "", op_span.clone()));
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
            TraversalOperator::Index(idx_expr, op_span) => {
                let idx_val = self.eval_expr(idx_expr);
                let mut op_marks = parent_marks.clone();
                op_marks.extend(idx_val.marks.clone());

                if idx_val.is_unknown() {
                    return Value::unknown(Type::Dynamic).with_marks(op_marks);
                }

                if let (Some(any), Some(ops)) =
                    (current.as_capsule_any(), current.ty().capsule_ops())
                {
                    if let Some(ref index_get) = ops.index_get {
                        match (index_get)(any, &idx_val) {
                            Ok(mut res) => {
                                res.marks.extend(op_marks);
                                res
                            }
                            Err(err) => {
                                self.push_diagnostic(Diagnostic::error(err, "", op_span.clone()));
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
                                if let Some(candidate) =
                                    crate::diagnostic::suggestion::suggest_closest_name(
                                        s,
                                        map.keys().map(String::as_str),
                                        2,
                                    )
                                {
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
                                self.push_diagnostic(Diagnostic::error(err, "", op_span.clone()));
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
                            if let TraversalOperator::GetAttr(..) = &traversal.operators[op_idx] {
                                op_idx += 1;
                            } else {
                                break;
                            }
                        }
                        continue;
                    }

                    let elements = match &*current.data {
                        ValueData::Array(arr) => arr.clone(),
                        ValueData::Set(set) => set.iter().cloned().collect(),
                        _ => vec![current.clone()],
                    };

                    op_idx += 1;
                    let mut sub_attrs = Vec::new();
                    while op_idx < traversal.operators.len() {
                        if let TraversalOperator::GetAttr(name, s) = &traversal.operators[op_idx] {
                            sub_attrs.push((name.clone(), s.clone()));
                            op_idx += 1;
                        } else {
                            break;
                        }
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
                        for (attr_name, attr_span) in &sub_attrs {
                            elem = self.eval_single_step(
                                &elem,
                                &TraversalOperator::GetAttr(attr_name.clone(), attr_span.clone()),
                            );
                        }
                        results.push(elem);
                    }
                    let elem_type = results.first().map_or(Type::Dynamic, |v| v.ty().clone());
                    current =
                        Value::new(Type::List(Box::new(elem_type)), ValueData::Array(results))
                            .with_marks(parent_marks);
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

    fn eval_for(&mut self, for_expr: &ForExpr, _span: crate::span::Span) -> Value {
        let coll_val = self.eval_expr(&for_expr.collection);
        if coll_val.is_unknown() {
            return Value::unknown(Type::Dynamic).with_marks(coll_val.marks.clone());
        }

        let coll_marks = coll_val.marks.clone();

        // Extract items to iterate over
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
            let mut iter_ctx = Context::new_child(self.ctx);
            if let Some(ref k_name) = for_expr.key_var {
                iter_ctx.set_variable(k_name, k_val.clone());
            }
            iter_ctx.set_variable(&for_expr.val_var, v_val.clone());

            // Sub-evaluator for the iteration
            let mut sub_eval = Evaluator::new(&iter_ctx);

            // Filter
            if let Some(ref cond_expr) = for_expr.cond_expr {
                let cond_res = sub_eval.eval_expr(cond_expr);
                match &*cond_res.data {
                    ValueData::Bool(true) => {}
                    ValueData::Bool(false) => {
                        continue;
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
                        continue;
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
            // Need to construct the Type map
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
                        let mut matched = false;
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
                                    matched = true;
                                    break;
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
                        if !matched && let Some(false_parts) = false_expr {
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
                                    ValueData::Number(Number::new(bigdecimal::BigDecimal::from(
                                        i as u64,
                                    ))),
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
                            "Collection in %{ for } directive must be a tuple, list, set, or object", "", collection.span(),
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
            crate::ast::expr::Directive::Strip { .. } => {}
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

    #[test]
    fn test_eval_binary_logical_right_errors() {
        let ctx = Context::new();
        let evaluator = Evaluator::new(&ctx);

        let expr = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::Number(
                Number::from_str("1").unwrap(),
                empty_span(),
            )),
            empty_span(),
        );
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("must be a boolean")
        );

        let evaluator = Evaluator::new(&ctx);
        let expr2 = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Bool(false, empty_span())),
            Box::new(Expression::Number(
                Number::from_str("1").unwrap(),
                empty_span(),
            )),
            empty_span(),
        );
        let errs = evaluator.evaluate(&expr2).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("must be a boolean")
        );
    }

    #[test]
    fn test_eval_binary_math_unknown_rhs_all_ops() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::Dynamic));

        let ops = vec![
            BinaryOp::NotEq,
            BinaryOp::Less,
            BinaryOp::LessEq,
            BinaryOp::Greater,
            BinaryOp::GreaterEq,
            BinaryOp::Sub,
            BinaryOp::Mul,
            BinaryOp::Div,
            BinaryOp::Mod,
        ];

        for op in ops {
            let expr = Expression::BinaryOp(
                op,
                Box::new(Expression::Number(
                    Number::from_str("1").unwrap(),
                    empty_span(),
                )),
                Box::new(Expression::Variable("unk".to_string(), empty_span())),
                empty_span(),
            );
            let val = Evaluator::new(&ctx).evaluate(&expr).unwrap().0;
            assert!(val.is_unknown());
        }
    }
    #[test]
    fn test_eval_binary_logical_right_unknown() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::Dynamic));

        let expr = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let val = Evaluator::new(&ctx).evaluate(&expr).unwrap().0;
        assert_eq!(val.ty(), &Type::Bool);
        assert!(val.is_unknown());

        let expr2 = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Bool(false, empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let val2 = Evaluator::new(&ctx).evaluate(&expr2).unwrap().0;
        assert_eq!(val2.ty(), &Type::Bool);
        assert!(val2.is_unknown());
    }

    #[test]
    fn test_eval_binary_math_unknown_rhs2() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::Dynamic));

        let expr3 = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Number(
                Number::from_str("1").unwrap(),
                empty_span(),
            )),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let val3 = Evaluator::new(&ctx).evaluate(&expr3).unwrap().0;
        assert_eq!(val3.ty(), &Type::Number);
        assert!(val3.is_unknown());

        let expr4 = Expression::BinaryOp(
            BinaryOp::Eq,
            Box::new(Expression::Number(
                Number::from_str("1").unwrap(),
                empty_span(),
            )),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let val4 = Evaluator::new(&ctx).evaluate(&expr4).unwrap().0;
        assert_eq!(val4.ty(), &Type::Bool);
        assert!(val4.is_unknown());
    }

    use super::*;
    use crate::number::Number;
    use crate::span::Span;
    use bigdecimal::BigDecimal;
    use std::collections::BTreeSet;
    use std::str::FromStr;

    fn empty_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    #[test]
    fn test_eval_literal() {
        let ctx = Context::new();
        let evaluator = Evaluator::new(&ctx);

        let expr = Expression::Null(empty_span());
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert!(val.is_null());

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::Bool(true, empty_span());
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));

        let evaluator = Evaluator::new(&ctx);
        let n = Number::from_str("42").unwrap();
        let expr = Expression::Number(n.clone(), empty_span());
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Number, ValueData::Number(n)));

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::String("hello".to_string(), empty_span());
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(
            val,
            Value::new(Type::String, ValueData::String("hello".to_string()))
        );
    }

    #[test]
    fn test_eval_variable() {
        let mut ctx = Context::new();
        ctx.set_variable("foo", Value::new(Type::Bool, ValueData::Bool(true)));

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::Variable("foo".to_string(), empty_span());
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));

        // Local variable
        ctx.set_variable(
            "local.bar",
            Value::new(
                Type::Number,
                ValueData::Number(crate::number::Number::from(42)),
            ),
        );
        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::Variable("local.bar".to_string(), empty_span());
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(
            val,
            Value::new(
                Type::Number,
                ValueData::Number(crate::number::Number::from(42))
            )
        );

        // Missing variable
        let _evaluator = Evaluator::new(&ctx);
        let expr = Expression::Variable("bar".to_string(), empty_span());
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert_eq!(errs.errors().len(), 1);
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Unknown variable 'bar'")
        );
    }

    #[test]
    fn test_eval_tuple() {
        let ctx = Context::new();
        let evaluator = Evaluator::new(&ctx);

        let expr = Expression::Tuple(
            vec![
                Expression::Bool(true, empty_span()),
                Expression::String("x".to_string(), empty_span()),
            ],
            empty_span(),
        );

        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val.ty(), &Type::Tuple(vec![Type::Bool, Type::String]));
    }

    #[test]
    fn test_eval_object() {
        let ctx = Context::new();
        let evaluator = Evaluator::new(&ctx);

        let k1 = Expression::String("k1".to_string(), empty_span());
        let v1 = Expression::Bool(true, empty_span());

        let expr = Expression::Object(vec![(k1, v1)], empty_span());

        let (val, _) = evaluator.evaluate(&expr).unwrap();
        let mut expected_map = BTreeMap::new();
        expected_map.insert("k1".to_string(), Type::Bool);
        assert_eq!(val.ty(), &Type::object(expected_map));
    }

    #[test]
    fn test_eval_unary() {
        let ctx = Context::new();
        let evaluator = Evaluator::new(&ctx);

        let expr = Expression::UnaryOp(
            UnaryOp::Not,
            Box::new(Expression::Bool(true, empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(false)));

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Number(
                Number::from_str("10").unwrap(),
                empty_span(),
            )),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(
            val,
            Value::new(
                Type::Number,
                ValueData::Number(Number::from_str("-10").unwrap())
            )
        );
    }

    #[test]
    fn test_eval_binary() {
        let ctx = Context::new();
        let evaluator = Evaluator::new(&ctx);

        let expr = Expression::BinaryOp(
            BinaryOp::Eq,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::Bool(false, empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(false)));

        // Short-circuit AND
        let evaluator = Evaluator::new(&ctx);
        let expr2 = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Bool(false, empty_span())),
            // Missing variable would error if evaluated, but short circuits
            Box::new(Expression::Variable("missing".to_string(), empty_span())),
            empty_span(),
        );
        let (val2, _) = evaluator.evaluate(&expr2).unwrap();
        assert_eq!(val2, Value::new(Type::Bool, ValueData::Bool(false)));
    }

    #[test]
    fn test_eval_conditional() {
        let ctx = Context::new();

        let evaluator = Evaluator::new(&ctx);
        let cond = Conditional {
            cond_expr: Expression::Bool(true, empty_span()),
            true_expr: Expression::Number(Number::from_str("1").unwrap(), empty_span()),
            false_expr: Expression::Number(Number::from_str("2").unwrap(), empty_span()),
        };
        let expr = Expression::Conditional(Box::new(cond), empty_span());

        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(
            val,
            Value::new(
                Type::Number,
                ValueData::Number(Number::from_str("1").unwrap())
            )
        );
    }

    #[test]
    fn test_eval_binary_math_logic_unknown() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::String));
        let evaluator = Evaluator::new(&ctx);

        // Eq between unk and unk -> Unknown Bool
        let expr = Expression::BinaryOp(
            BinaryOp::Eq,
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val.ty(), &Type::Bool);

        let evaluator = Evaluator::new(&ctx);
        let expr2 = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (val2, _) = evaluator.evaluate(&expr2).unwrap();
        assert_eq!(val2.ty(), &Type::Number);
        assert!(val2.is_unknown());
    }

    #[test]
    fn test_eval_binary_unsupported() {
        let ctx = Context::new();
        let expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::Bool(false, empty_span())),
            empty_span(),
        );
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert_eq!(errs.errors().len(), 1);
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Cannot coerce operands of Add to numbers")
        );
    }

    #[test]
    fn test_eval_logical_errors() {
        let ctx = Context::new();

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::Number(
                Number::from_str("1").unwrap(),
                empty_span(),
            )),
            empty_span(),
        );
        let _errs = evaluator.evaluate(&expr).err().unwrap();

        let evaluator = Evaluator::new(&ctx);
        let expr2 = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Bool(false, empty_span())),
            Box::new(Expression::Number(
                Number::from_str("1").unwrap(),
                empty_span(),
            )),
            empty_span(),
        );
        let _errs = evaluator.evaluate(&expr2).err().unwrap();
    }

    #[test]
    fn test_eval_logical_unknown() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::Bool));

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            Box::new(Expression::Bool(true, empty_span())),
            empty_span(),
        );
        let (_val, _) = evaluator.evaluate(&expr).unwrap();

        let evaluator = Evaluator::new(&ctx);
        let expr2 = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (_val, _) = evaluator.evaluate(&expr2).unwrap();

        let evaluator = Evaluator::new(&ctx);
        let expr3 = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            Box::new(Expression::Bool(false, empty_span())),
            empty_span(),
        );
        let (_val, _) = evaluator.evaluate(&expr3).unwrap();

        let evaluator = Evaluator::new(&ctx);
        let expr4 = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Bool(false, empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (_val, _) = evaluator.evaluate(&expr4).unwrap();
    }
    #[test]
    fn test_eval_unary_errors_and_unknown() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::Bool));

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::UnaryOp(
            UnaryOp::Not,
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val.ty(), &Type::Bool);

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val.ty(), &Type::Number);

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::UnaryOp(
            UnaryOp::Not,
            Box::new(Expression::Number(
                Number::from_str("1").unwrap(),
                empty_span(),
            )),
            empty_span(),
        );
        let _errs = evaluator.evaluate(&expr).err().unwrap();

        let _evaluator = Evaluator::new(&ctx);
        let expr = Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Bool(true, empty_span())),
            empty_span(),
        );
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("must be a number")
        );
    }

    #[test]
    fn test_eval_cond_unknown_and_errors() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::Bool));

        let evaluator = Evaluator::new(&ctx);
        let cond = Conditional {
            cond_expr: Expression::Variable("unk".to_string(), empty_span()),
            true_expr: Expression::Number(Number::from_str("1").unwrap(), empty_span()),
            false_expr: Expression::Number(Number::from_str("2").unwrap(), empty_span()),
        };
        let expr = Expression::Conditional(Box::new(cond), empty_span());
        let (_val, _) = evaluator.evaluate(&expr).unwrap();

        // Unknown with bad types
        let _evaluator = Evaluator::new(&ctx);
        let cond2 = Conditional {
            cond_expr: Expression::Variable("unk".to_string(), empty_span()),
            true_expr: Expression::Number(Number::from_str("1").unwrap(), empty_span()),
            false_expr: Expression::Bool(false, empty_span()),
        };
        let expr2 = Expression::Conditional(Box::new(cond2), empty_span());
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr2).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Incompatible types")
        );

        // Bad cond
        let _evaluator = Evaluator::new(&ctx);
        let cond3 = Conditional {
            cond_expr: Expression::Number(Number::from_str("1").unwrap(), empty_span()),
            true_expr: Expression::Number(Number::from_str("1").unwrap(), empty_span()),
            false_expr: Expression::Number(Number::from_str("2").unwrap(), empty_span()),
        };
        let expr3 = Expression::Conditional(Box::new(cond3), empty_span());
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr3).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("must be a boolean")
        );
    }

    #[test]
    fn test_eval_stubs() {
        let ctx = Context::new();

        let exprs = vec![Expression::Template(vec![], empty_span())];

        for expr in exprs {
            let evaluator = Evaluator::new(&ctx);
            let val = evaluator.evaluate(&expr).unwrap().0;
            assert_eq!(
                val,
                Value::new(Type::String, ValueData::String(String::new()))
            );
        }

        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator
            .evaluate(&Expression::FuncCall(
                Box::new(crate::ast::expr::FuncCall {
                    name: "foo".into(),
                    args: vec![],
                    expand_final: false,
                }),
                empty_span(),
            ))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Unknown function")
        );
    }
    #[test]
    fn test_eval_object_errors() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::String));

        let _evaluator = Evaluator::new(&ctx);
        let expr = Expression::Object(
            vec![(
                Expression::Variable("unk".to_string(), empty_span()),
                Expression::Bool(true, empty_span()),
            )],
            empty_span(),
        );
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Object key cannot be unknown")
        );

        let _evaluator = Evaluator::new(&ctx);
        let expr2 = Expression::Object(
            vec![(
                Expression::Number(Number::from_str("1").unwrap(), empty_span()),
                Expression::Bool(true, empty_span()),
            )],
            empty_span(),
        );
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr2).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Object key must be a string")
        );
    }

    #[test]
    fn test_eval_binary_math_logic_ops() {
        let ctx = Context::new();

        let cases = vec![
            (BinaryOp::Less, "1", "2", true),
            (BinaryOp::LessEq, "2", "2", true),
            (BinaryOp::Greater, "3", "2", true),
            (BinaryOp::GreaterEq, "2", "2", true),
        ];

        for (op, lhs, rhs, expected) in cases {
            let evaluator = Evaluator::new(&ctx);
            let expr = Expression::BinaryOp(
                op,
                Box::new(Expression::Number(
                    Number::from_str(lhs).unwrap(),
                    empty_span(),
                )),
                Box::new(Expression::Number(
                    Number::from_str(rhs).unwrap(),
                    empty_span(),
                )),
                empty_span(),
            );
            let (val, _) = evaluator.evaluate(&expr).unwrap();
            assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(expected)));
        }

        // Eq on unequal types
        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::BinaryOp(
            BinaryOp::Eq,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::String("true".to_string(), empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(false)));

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::BinaryOp(
            BinaryOp::NotEq,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::String("true".to_string(), empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));
    }

    #[test]
    fn test_eval_logical_right_operand_unknown() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::Bool));
        ctx.set_variable("unk_num", Value::unknown(Type::Number));

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::Variable("unk_num".to_string(), empty_span())),
            empty_span(),
        );
        let (_val, _) = evaluator.evaluate(&expr).unwrap();

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Bool(false, empty_span())),
            Box::new(Expression::Variable("unk_num".to_string(), empty_span())),
            empty_span(),
        );
        let (_val, _) = evaluator.evaluate(&expr).unwrap();
    }

    #[test]
    fn test_eval_object_key_unknown_or_bad() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::String));

        let _evaluator = Evaluator::new(&ctx);
        let expr = Expression::Object(
            vec![(
                Expression::Variable("unk".to_string(), empty_span()),
                Expression::Bool(true, empty_span()),
            )],
            empty_span(),
        );
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Object key cannot be unknown")
        );
    }

    #[test]
    fn test_eval_splats() {
        let mut ctx = Context::new();

        // 1. Attribute splat on scalar: auto-wraps into a single-element list
        let t1 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Bool(true, empty_span())),
            operators: vec![TraversalOperator::AttrSplat(empty_span())],
        };
        let (val, _) = Evaluator::new(&ctx)
            .evaluate(&Expression::Traversal(Box::new(t1), empty_span()))
            .unwrap();
        assert_eq!(
            *val.data,
            ValueData::Array(vec![Value::new(Type::Bool, ValueData::Bool(true))])
        );

        // 2. Full splat on non-sequence scalar: should reject auto-wrapping and emit diagnostic error
        let t2 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Bool(true, empty_span())),
            operators: vec![TraversalOperator::FullSplat(empty_span())],
        };
        let errs = Evaluator::new(&ctx)
            .evaluate(&Expression::Traversal(Box::new(t2), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Cannot apply full splat operator to non-sequence value")
        );

        // 3. Attribute splat on list of objects
        let mut obj1 = BTreeMap::new();
        obj1.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("server1".to_string())),
        );
        obj1.insert(
            "port".to_string(),
            Value::new(
                Type::Number,
                ValueData::Number(Number::new(BigDecimal::from(80))),
            ),
        );

        let mut obj2 = BTreeMap::new();
        obj2.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("server2".to_string())),
        );
        obj2.insert(
            "port".to_string(),
            Value::new(
                Type::Number,
                ValueData::Number(Number::new(BigDecimal::from(443))),
            ),
        );

        let list_val = Value::new(
            Type::List(Box::new(Type::Dynamic)),
            ValueData::Array(vec![
                Value::new(Type::Dynamic, ValueData::Object(obj1)),
                Value::new(Type::Dynamic, ValueData::Object(obj2)),
            ]),
        );
        ctx.set_variable("servers", list_val);

        let t3 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("servers".to_string(), empty_span())),
            operators: vec![
                TraversalOperator::AttrSplat(empty_span()),
                TraversalOperator::GetAttr("name".to_string(), empty_span()),
            ],
        };
        let (val_names, _) = Evaluator::new(&ctx)
            .evaluate(&Expression::Traversal(Box::new(t3), empty_span()))
            .unwrap();
        assert_eq!(
            *val_names.data,
            ValueData::Array(vec![
                Value::new(Type::String, ValueData::String("server1".to_string())),
                Value::new(Type::String, ValueData::String("server2".to_string())),
            ])
        );

        // 4. Null short-circuiting: null.* returns empty list without error
        let t_null = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Null(empty_span())),
            operators: vec![
                TraversalOperator::AttrSplat(empty_span()),
                TraversalOperator::GetAttr("any".to_string(), empty_span()),
            ],
        };
        let (val_null, _) = Evaluator::new(&ctx)
            .evaluate(&Expression::Traversal(Box::new(t_null), empty_span()))
            .unwrap();
        assert_eq!(*val_null.data, ValueData::Array(Vec::new()));

        // 5. Empty list short-circuiting: [].* returns empty list
        let t_empty = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Tuple(vec![], empty_span())),
            operators: vec![
                TraversalOperator::AttrSplat(empty_span()),
                TraversalOperator::GetAttr("foo".to_string(), empty_span()),
            ],
        };
        let (val_empty, _) = Evaluator::new(&ctx)
            .evaluate(&Expression::Traversal(Box::new(t_empty), empty_span()))
            .unwrap();
        assert_eq!(*val_empty.data, ValueData::Array(Vec::new()));

        // 6. Chained full splat with indexing: clusters[*].nodes[0].ip
        let mut node0 = BTreeMap::new();
        node0.insert(
            "ip".to_string(),
            Value::new(Type::String, ValueData::String("10.0.0.1".to_string())),
        );
        let mut cluster0 = BTreeMap::new();
        cluster0.insert(
            "nodes".to_string(),
            Value::new(
                Type::List(Box::new(Type::Dynamic)),
                ValueData::Array(vec![Value::new(Type::Dynamic, ValueData::Object(node0))]),
            ),
        );

        let mut node1 = BTreeMap::new();
        node1.insert(
            "ip".to_string(),
            Value::new(Type::String, ValueData::String("10.0.0.2".to_string())),
        );
        let mut cluster1 = BTreeMap::new();
        cluster1.insert(
            "nodes".to_string(),
            Value::new(
                Type::List(Box::new(Type::Dynamic)),
                ValueData::Array(vec![Value::new(Type::Dynamic, ValueData::Object(node1))]),
            ),
        );

        ctx.set_variable(
            "clusters",
            Value::new(
                Type::List(Box::new(Type::Dynamic)),
                ValueData::Array(vec![
                    Value::new(Type::Dynamic, ValueData::Object(cluster0)),
                    Value::new(Type::Dynamic, ValueData::Object(cluster1)),
                ]),
            ),
        );

        let eval_clusters = Evaluator::new(&ctx);
        let t_chained_full = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("clusters".to_string(), empty_span())),
            operators: vec![
                TraversalOperator::FullSplat(empty_span()),
                TraversalOperator::GetAttr("nodes".to_string(), empty_span()),
                TraversalOperator::Index(
                    Expression::Number(Number::new(BigDecimal::from(0)), empty_span()),
                    empty_span(),
                ),
                TraversalOperator::GetAttr("ip".to_string(), empty_span()),
            ],
        };
        let (val_ips, _) = eval_clusters
            .evaluate(&Expression::Traversal(
                Box::new(t_chained_full),
                empty_span(),
            ))
            .unwrap();
        assert_eq!(
            *val_ips.data,
            ValueData::Array(vec![
                Value::new(Type::String, ValueData::String("10.0.0.1".to_string())),
                Value::new(Type::String, ValueData::String("10.0.0.2".to_string())),
            ])
        );

        // 7. Mark propagation through splat
        let mut marked_list = Value::new(
            Type::List(Box::new(Type::Dynamic)),
            ValueData::Array(vec![Value::new(
                Type::String,
                ValueData::String("m1".to_string()),
            )]),
        );
        marked_list
            .marks
            .insert(crate::types::val::ValueMark::Sensitive);
        ctx.set_variable("marked_list", marked_list);

        let eval_marked = Evaluator::new(&ctx);
        let t_marked = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable(
                "marked_list".to_string(),
                empty_span(),
            )),
            operators: vec![TraversalOperator::AttrSplat(empty_span())],
        };
        let (val_marked, _) = eval_marked
            .evaluate(&Expression::Traversal(Box::new(t_marked), empty_span()))
            .unwrap();
        assert!(
            val_marked
                .marks
                .contains(&crate::types::val::ValueMark::Sensitive)
        );

        // 8. Unknown propagation
        ctx.set_variable(
            "unknown_seq",
            Value::unknown(Type::List(Box::new(Type::String))),
        );
        let eval_unk = Evaluator::new(&ctx);
        let t_unk = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable(
                "unknown_seq".to_string(),
                empty_span(),
            )),
            operators: vec![TraversalOperator::FullSplat(empty_span())],
        };
        let (val_unk, _) = eval_unk
            .evaluate(&Expression::Traversal(Box::new(t_unk), empty_span()))
            .unwrap();
        assert!(val_unk.is_unknown());
    }

    #[test]
    fn test_eval_func_call_expand_final() {
        let mut ctx = Context::new().enable_stdlib();

        // 1. Calling format with expand_final on a list: format("%s-%d", ["host", 1]...)
        let expr = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "format".into(),
                args: vec![
                    Expression::String("%s-%d".to_string(), empty_span()),
                    Expression::Tuple(
                        vec![
                            Expression::String("host".to_string(), empty_span()),
                            Expression::Number(Number::new(BigDecimal::from(1)), empty_span()),
                        ],
                        empty_span(),
                    ),
                ],
                expand_final: true,
            }),
            empty_span(),
        );
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
        assert_eq!(*val.data, ValueData::String("host-1".to_string()));

        // 2. Calling concat with expand_final on a list of lists: concat([["a"], ["b", "c"]]...)
        let expr_concat = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "concat".into(),
                args: vec![Expression::Tuple(
                    vec![
                        Expression::Tuple(
                            vec![Expression::String("a".to_string(), empty_span())],
                            empty_span(),
                        ),
                        Expression::Tuple(
                            vec![
                                Expression::String("b".to_string(), empty_span()),
                                Expression::String("c".to_string(), empty_span()),
                            ],
                            empty_span(),
                        ),
                    ],
                    empty_span(),
                )],
                expand_final: true,
            }),
            empty_span(),
        );
        let (val_concat, _) = Evaluator::new(&ctx).evaluate(&expr_concat).unwrap();
        assert_eq!(
            *val_concat.data,
            ValueData::Array(vec![
                Value::new(Type::String, ValueData::String("a".to_string())),
                Value::new(Type::String, ValueData::String("b".to_string())),
                Value::new(Type::String, ValueData::String("c".to_string())),
            ])
        );

        // 3. Mark preservation on unpacked arguments
        let mut sensitive_list = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(vec![Value::new(
                Type::String,
                ValueData::String("val".to_string()),
            )]),
        );
        sensitive_list
            .marks
            .insert(crate::types::val::ValueMark::Sensitive);
        ctx.set_variable("sec_args", sensitive_list);

        ctx.set_function(
            "check_mark",
            crate::eval::func::Function {
                name: "check_mark".to_string(),
                func: std::sync::Arc::new(|args| {
                    if args.first().is_some_and(|first| {
                        first
                            .marks
                            .contains(&crate::types::val::ValueMark::Sensitive)
                    }) {
                        return Ok(Value::new(Type::Bool, ValueData::Bool(true)));
                    }
                    Ok(Value::new(Type::Bool, ValueData::Bool(false)))
                }),
                signature: None,
            },
        );

        let expr_mark = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "check_mark".into(),
                args: vec![Expression::Variable("sec_args".to_string(), empty_span())],
                expand_final: true,
            }),
            empty_span(),
        );
        let (val_mark, _) = Evaluator::new(&ctx).evaluate(&expr_mark).unwrap();
        assert_eq!(*val_mark.data, ValueData::Bool(true));

        let expr_unmark = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "check_mark".into(),
                args: vec![Expression::String("plain".to_string(), empty_span())],
                expand_final: false,
            }),
            empty_span(),
        );
        let (val_unmark, _) = Evaluator::new(&ctx).evaluate(&expr_unmark).unwrap();
        assert_eq!(*val_unmark.data, ValueData::Bool(false));

        // 4. Expanding a set
        let mut set_data = BTreeSet::new();
        set_data.insert(Value::new(
            Type::String,
            ValueData::String("s1".to_string()),
        ));
        ctx.set_variable(
            "my_set",
            Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_data)),
        );

        ctx.set_function(
            "count_args",
            crate::eval::func::Function {
                name: "count_args".to_string(),
                func: std::sync::Arc::new(|args| {
                    let len = u64::try_from(args.len()).unwrap_or_default();
                    Ok(Value::new(
                        Type::Number,
                        ValueData::Number(Number::new(BigDecimal::from(len))),
                    ))
                }),
                signature: None,
            },
        );

        let expr_set = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "count_args".into(),
                args: vec![Expression::Variable("my_set".to_string(), empty_span())],
                expand_final: true,
            }),
            empty_span(),
        );
        let (val_count, _) = Evaluator::new(&ctx).evaluate(&expr_set).unwrap();
        assert_eq!(
            *val_count.data,
            ValueData::Number(Number::new(BigDecimal::from(1)))
        );

        // 5. Expanding unknown sequence
        ctx.set_variable(
            "unk_args",
            Value::unknown(Type::List(Box::new(Type::String))),
        );
        let expr_unk = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "format".into(),
                args: vec![
                    Expression::String("%s".to_string(), empty_span()),
                    Expression::Variable("unk_args".to_string(), empty_span()),
                ],
                expand_final: true,
            }),
            empty_span(),
        );
        let (val_unk, _) = Evaluator::new(&ctx).evaluate(&expr_unk).unwrap();
        assert!(val_unk.is_unknown());

        // 6. Expanding non-sequence (diagnostic error)
        let expr_bad = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "format".into(),
                args: vec![Expression::Number(
                    Number::new(BigDecimal::from(42)),
                    empty_span(),
                )],
                expand_final: true,
            }),
            empty_span(),
        );
        let errs = Evaluator::new(&ctx).evaluate(&expr_bad).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Cannot expand non-sequence")
        );
    }

    #[test]
    fn test_eval_lazy_try_and_can() {
        let mut ctx = Context::new();

        let mut map = BTreeMap::new();
        map.insert(
            "existing".to_string(),
            Value::new(Type::String, ValueData::String("found".to_string())),
        );
        ctx.set_variable(
            "obj",
            Value::new(Type::object(BTreeMap::new()), ValueData::Object(map)),
        );

        ctx.set_variable(
            "arr",
            Value::new(
                Type::List(Box::new(Type::Number)),
                ValueData::Array(vec![Value::new(
                    Type::Number,
                    ValueData::Number(Number::new(BigDecimal::from(10))),
                )]),
            ),
        );

        // 1. try(obj.missing, "fallback") -> returns "fallback" without error
        let expr_try_fallback = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "try".into(),
                args: vec![
                    Expression::Traversal(
                        Box::new(crate::ast::expr::Traversal {
                            expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                            operators: vec![TraversalOperator::GetAttr(
                                "missing".to_string(),
                                empty_span(),
                            )],
                        }),
                        empty_span(),
                    ),
                    Expression::String("fallback".to_string(), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let (val_try1, _) = Evaluator::new(&ctx).evaluate(&expr_try_fallback).unwrap();
        assert_eq!(*val_try1.data, ValueData::String("fallback".to_string()));

        // 2. try(obj.existing, "fallback") -> returns "found" immediately
        let expr_try_found = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "try".into(),
                args: vec![
                    Expression::Traversal(
                        Box::new(crate::ast::expr::Traversal {
                            expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                            operators: vec![TraversalOperator::GetAttr(
                                "existing".to_string(),
                                empty_span(),
                            )],
                        }),
                        empty_span(),
                    ),
                    Expression::String("fallback".to_string(), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let (val_try2, _) = Evaluator::new(&ctx).evaluate(&expr_try_found).unwrap();
        assert_eq!(*val_try2.data, ValueData::String("found".to_string()));

        // 3. Cascading try: try(arr[100], obj.missing, 42) -> 42
        let expr_try_cascade = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "try".into(),
                args: vec![
                    Expression::Traversal(
                        Box::new(crate::ast::expr::Traversal {
                            expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
                            operators: vec![TraversalOperator::Index(
                                Expression::Number(
                                    Number::new(BigDecimal::from(100)),
                                    empty_span(),
                                ),
                                empty_span(),
                            )],
                        }),
                        empty_span(),
                    ),
                    Expression::Traversal(
                        Box::new(crate::ast::expr::Traversal {
                            expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                            operators: vec![TraversalOperator::GetAttr(
                                "missing".to_string(),
                                empty_span(),
                            )],
                        }),
                        empty_span(),
                    ),
                    Expression::Number(Number::new(BigDecimal::from(42)), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let (val_try3, _) = Evaluator::new(&ctx).evaluate(&expr_try_cascade).unwrap();
        assert_eq!(
            *val_try3.data,
            ValueData::Number(Number::new(BigDecimal::from(42)))
        );

        // 4. try() with 0 arguments emits diagnostic
        let expr_try_zero = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "try".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        let err_try_zero = Evaluator::new(&ctx).evaluate(&expr_try_zero).err().unwrap();
        assert!(
            err_try_zero.errors()[0]
                .error
                .to_string()
                .contains("requires at least one argument")
        );

        // 5. try() when all alternatives fail emits diagnostic
        let expr_try_all_fail = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "try".into(),
                args: vec![
                    Expression::Traversal(
                        Box::new(crate::ast::expr::Traversal {
                            expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                            operators: vec![TraversalOperator::GetAttr(
                                "missing1".to_string(),
                                empty_span(),
                            )],
                        }),
                        empty_span(),
                    ),
                    Expression::Traversal(
                        Box::new(crate::ast::expr::Traversal {
                            expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                            operators: vec![TraversalOperator::GetAttr(
                                "missing2".to_string(),
                                empty_span(),
                            )],
                        }),
                        empty_span(),
                    ),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let err_try_fail = Evaluator::new(&ctx)
            .evaluate(&expr_try_all_fail)
            .err()
            .unwrap();
        assert!(
            err_try_fail.errors()[0]
                .error
                .to_string()
                .contains("No alternative expression succeeded in 'try'")
        );

        // 6. can(obj.missing) -> false
        let expr_can_false = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "can".into(),
                args: vec![Expression::Traversal(
                    Box::new(crate::ast::expr::Traversal {
                        expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                        operators: vec![TraversalOperator::GetAttr(
                            "missing".to_string(),
                            empty_span(),
                        )],
                    }),
                    empty_span(),
                )],
                expand_final: false,
            }),
            empty_span(),
        );
        let (val_can1, _) = Evaluator::new(&ctx).evaluate(&expr_can_false).unwrap();
        assert_eq!(*val_can1.data, ValueData::Bool(false));

        // 7. can(obj.existing) -> true
        let expr_can_true = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "can".into(),
                args: vec![Expression::Traversal(
                    Box::new(crate::ast::expr::Traversal {
                        expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
                        operators: vec![TraversalOperator::GetAttr(
                            "existing".to_string(),
                            empty_span(),
                        )],
                    }),
                    empty_span(),
                )],
                expand_final: false,
            }),
            empty_span(),
        );
        let (val_can2, _) = Evaluator::new(&ctx).evaluate(&expr_can_true).unwrap();
        assert_eq!(*val_can2.data, ValueData::Bool(true));

        // 8. can(arr[0] / 0) division by zero -> false
        let expr_can_div_zero = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "can".into(),
                args: vec![Expression::BinaryOp(
                    BinaryOp::Div,
                    Box::new(Expression::Number(
                        Number::new(BigDecimal::from(10)),
                        empty_span(),
                    )),
                    Box::new(Expression::Number(
                        Number::new(BigDecimal::from(0)),
                        empty_span(),
                    )),
                    empty_span(),
                )],
                expand_final: false,
            }),
            empty_span(),
        );
        let (val_can_div, _) = Evaluator::new(&ctx).evaluate(&expr_can_div_zero).unwrap();
        assert_eq!(*val_can_div.data, ValueData::Bool(false));

        // 9. can(unknown_expr) -> unknown bool
        ctx.set_variable("unk", Value::unknown(Type::String));
        let expr_can_unk = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "can".into(),
                args: vec![Expression::Variable("unk".to_string(), empty_span())],
                expand_final: false,
            }),
            empty_span(),
        );
        let (val_can_unk, _) = Evaluator::new(&ctx).evaluate(&expr_can_unk).unwrap();
        assert!(val_can_unk.is_unknown());
        assert_eq!(val_can_unk.ty(), &Type::Bool);

        // 10. can() with arity != 1 emits diagnostic error
        let expr_can_bad_arity = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "can".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        let err_can_bad = Evaluator::new(&ctx)
            .evaluate(&expr_can_bad_arity)
            .err()
            .unwrap();
        assert!(
            err_can_bad.errors()[0]
                .error
                .to_string()
                .contains("requires exactly one argument")
        );
    }

    #[test]
    fn test_eval_function_signatures_and_type_checking() {
        use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
        use std::sync::Arc;

        let mut ctx = Context::new();

        // 1. Function with fixed parameters and automatic coercion
        let sig_calc = FunctionSignature::with_static_return_type(
            vec![
                FunctionParamSpec::new("num1", Type::Number),
                FunctionParamSpec::new("num2", Type::Number),
            ],
            Type::Number,
        );
        let fn_add = Function::new(
            "add_nums",
            Arc::new(|args| {
                let n1 = match *args[0].data {
                    ValueData::Number(ref n) => n.clone(),
                    _ => return Err("expected number".to_string()),
                };
                let n2 = match *args[1].data {
                    ValueData::Number(ref n) => n.clone(),
                    _ => return Err("expected number".to_string()),
                };
                Ok(Value::new(
                    Type::Number,
                    ValueData::Number(Number::new(&n1.0 + &n2.0)),
                ))
            }),
        )
        .with_signature(sig_calc);
        assert!(
            (fn_add.func)(&[
                Value::new(Type::Bool, ValueData::Bool(true)),
                Value::new(Type::Bool, ValueData::Bool(true)),
            ])
            .is_err()
        );
        assert!(
            (fn_add.func)(&[
                Value::new(
                    Type::Number,
                    ValueData::Number(Number::new(BigDecimal::from(1)))
                ),
                Value::new(Type::Bool, ValueData::Bool(true)),
            ])
            .is_err()
        );
        ctx.set_function("add_nums", fn_add);

        // Success with coercion: add_nums("10", 20) -> 30
        let expr_add_ok = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "add_nums".into(),
                args: vec![
                    Expression::String("10".to_string(), empty_span()),
                    Expression::Number(Number::new(BigDecimal::from(20)), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let (val_add, _) = Evaluator::new(&ctx).evaluate(&expr_add_ok).unwrap();
        assert_eq!(
            *val_add.data,
            ValueData::Number(Number::new(BigDecimal::from(30)))
        );

        // Arity mismatch: too few arguments (1 instead of 2)
        let expr_add_few = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "add_nums".into(),
                args: vec![Expression::Number(
                    Number::new(BigDecimal::from(10)),
                    empty_span(),
                )],
                expand_final: false,
            }),
            empty_span(),
        );
        let err_few = Evaluator::new(&ctx).evaluate(&expr_add_few).err().unwrap();
        assert!(
            err_few.errors()[0]
                .error
                .to_string()
                .contains("Wrong number of arguments")
        );

        // Arity mismatch: too many arguments (3 instead of 2)
        let expr_add_many = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "add_nums".into(),
                args: vec![
                    Expression::Number(Number::new(BigDecimal::from(1)), empty_span()),
                    Expression::Number(Number::new(BigDecimal::from(2)), empty_span()),
                    Expression::Number(Number::new(BigDecimal::from(3)), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let err_many = Evaluator::new(&ctx).evaluate(&expr_add_many).err().unwrap();
        assert!(
            err_many.errors()[0]
                .error
                .to_string()
                .contains("Wrong number of arguments")
        );

        // Type mismatch: uncoercible bool to number
        let expr_add_incompat = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "add_nums".into(),
                args: vec![
                    Expression::Bool(true, empty_span()),
                    Expression::Number(Number::new(BigDecimal::from(2)), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let err_incompat = Evaluator::new(&ctx)
            .evaluate(&expr_add_incompat)
            .err()
            .unwrap();
        assert!(
            err_incompat.errors()[0]
                .error
                .to_string()
                .contains("Invalid argument")
        );

        // Null check: passing null when allow_null is false
        let expr_add_null = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "add_nums".into(),
                args: vec![
                    Expression::Null(empty_span()),
                    Expression::Number(Number::new(BigDecimal::from(2)), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let err_null = Evaluator::new(&ctx).evaluate(&expr_add_null).err().unwrap();
        assert!(
            err_null.errors()[0]
                .error
                .to_string()
                .contains("cannot be null")
        );

        // 2. Variadic function signature
        let sig_join_custom = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("sep", Type::String)],
            Type::String,
        )
        .with_variadic(FunctionParamSpec::new("items", Type::String));
        let fn_join_custom = Function::new(
            "custom_join",
            Arc::new(|args| {
                let sep = match *args[0].data {
                    ValueData::String(ref s) => s.clone(),
                    _ => return Err("expected string".to_string()),
                };
                let mut parts = Vec::new();
                for arg in &args[1..] {
                    if let ValueData::String(ref s) = *arg.data {
                        parts.push(s.clone());
                    }
                }
                Ok(Value::new(
                    Type::String,
                    ValueData::String(parts.join(&sep)),
                ))
            }),
        )
        .with_signature(sig_join_custom);
        assert!((fn_join_custom.func)(&[Value::new(Type::Bool, ValueData::Bool(true))]).is_err());
        let _ = (fn_join_custom.func)(&[
            Value::new(Type::String, ValueData::String(":".to_string())),
            Value::new(Type::Bool, ValueData::Bool(true)),
        ]);
        ctx.set_function("custom_join", fn_join_custom);

        // Success: custom_join(":", "a", "b", "c") -> "a:b:c"
        let expr_join_ok = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "custom_join".into(),
                args: vec![
                    Expression::String(":".to_string(), empty_span()),
                    Expression::String("a".to_string(), empty_span()),
                    Expression::String("b".to_string(), empty_span()),
                    Expression::String("c".to_string(), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let (val_join, _) = Evaluator::new(&ctx).evaluate(&expr_join_ok).unwrap();
        assert_eq!(*val_join.data, ValueData::String("a:b:c".to_string()));

        // Variadic arity error: missing fixed parameter
        let expr_join_few = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "custom_join".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        let err_join_few = Evaluator::new(&ctx).evaluate(&expr_join_few).err().unwrap();
        assert!(
            err_join_few.errors()[0]
                .error
                .to_string()
                .contains("Too few arguments")
        );
    }

    #[test]
    fn test_eval_traversal_errors() {
        let mut ctx = Context::new();
        let evaluator = Evaluator::new(&ctx);
        // traverse null
        let t1 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Null(empty_span())),
            operators: vec![TraversalOperator::GetAttr("a".to_string(), empty_span())],
        };
        let errs = evaluator
            .evaluate(&Expression::Traversal(Box::new(t1), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Attempt to traverse null value")
        );

        let evaluator = Evaluator::new(&ctx);
        // traverse non-object attr
        let t2 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Bool(true, empty_span())),
            operators: vec![TraversalOperator::GetAttr("a".to_string(), empty_span())],
        };
        let errs = evaluator
            .evaluate(&Expression::Traversal(Box::new(t2), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Cannot get attribute from non-object")
        );

        // traverse missing attr
        let mut map = BTreeMap::new();
        map.insert(
            "a".to_string(),
            Value::new(Type::Bool, ValueData::Bool(true)),
        );
        let obj = Value::new(Type::object(BTreeMap::new()), ValueData::Object(map));
        ctx.set_variable("obj", obj);
        let evaluator = Evaluator::new(&ctx);
        let t3 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
            operators: vec![TraversalOperator::GetAttr("b".to_string(), empty_span())],
        };
        let errs = evaluator
            .evaluate(&Expression::Traversal(Box::new(t3), empty_span()))
            .err()
            .unwrap();
        assert!(errs.errors()[0].error.to_string().contains("not found"));

        // Index array with non-number
        let arr = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        ctx.set_variable("arr", arr);
        let evaluator = Evaluator::new(&ctx);
        let t4 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::Bool(true, empty_span()),
                empty_span(),
            )],
        };
        let errs = evaluator
            .evaluate(&Expression::Traversal(Box::new(t4), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Invalid index operation")
        );

        // Index array out of bounds
        let evaluator = Evaluator::new(&ctx);
        let t4 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::Number(Number::from_str("10").unwrap(), empty_span()),
                empty_span(),
            )],
        };
        let errs = evaluator
            .evaluate(&Expression::Traversal(Box::new(t4), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Index out of bounds")
        );

        // Legacy index out of bounds
        let evaluator = Evaluator::new(&ctx);
        let t4 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
            operators: vec![TraversalOperator::LegacyIndex(10, empty_span())],
        };
        let errs = evaluator
            .evaluate(&Expression::Traversal(Box::new(t4), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Index out of bounds")
        );

        // Legacy index on non-array
        let evaluator = Evaluator::new(&ctx);
        let t4 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Bool(true, empty_span())),
            operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
        };
        let errs = evaluator
            .evaluate(&Expression::Traversal(Box::new(t4), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Legacy index on non-array")
        );

        // Index object missing key
        let evaluator = Evaluator::new(&ctx);
        let t5 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::String("b".to_string(), empty_span()),
                empty_span(),
            )],
        };
        let errs = evaluator
            .evaluate(&Expression::Traversal(Box::new(t5), empty_span()))
            .err()
            .unwrap();
        assert!(errs.errors()[0].error.to_string().contains("not found"));

        // Index unknown index
        let mut ctx2 = Context::new();
        ctx2.set_variable(
            "arr",
            Value::new(Type::Tuple(vec![]), ValueData::Array(vec![])),
        );
        ctx2.set_variable("unk", Value::unknown(Type::Number));
        let evaluator = Evaluator::new(&ctx2);
        let t5 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::Variable("unk".to_string(), empty_span()),
                empty_span(),
            )],
        };
        let (_val, _) = evaluator
            .evaluate(&Expression::Traversal(Box::new(t5), empty_span()))
            .unwrap();
    }
    #[test]
    fn test_eval_for_expr_errors() {
        let mut ctx = Context::new();

        // non-tuple/object collection
        let evaluator = Evaluator::new(&ctx);
        let f = ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Bool(true, empty_span())),
            key_expr: None,
            val_expr: Box::new(Expression::Bool(true, empty_span())),
            cond_expr: None,
            grouping: false,
        };
        let errs = evaluator
            .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("collection must be a tuple")
        );

        // Unreachable object group mock
        // Handled through AST generation edge case not easily hit.
        ctx.set_variable("unk", Value::unknown(Type::Tuple(vec![])));
        let evaluator = Evaluator::new(&ctx);
        let f = ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Variable("unk".to_string(), empty_span())),
            key_expr: None,
            val_expr: Box::new(Expression::Bool(true, empty_span())),
            cond_expr: None,
            grouping: false,
        };
        let (_val, _) = evaluator
            .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
            .unwrap();

        // bad cond
        let arr = Value::new(
            Type::Tuple(vec![]),
            ValueData::Array(vec![Value::new(Type::Bool, ValueData::Bool(true))]),
        );
        ctx.set_variable("arr", arr);
        let evaluator = Evaluator::new(&ctx);
        let f = ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Variable("arr".to_string(), empty_span())),
            key_expr: None,
            val_expr: Box::new(Expression::Bool(true, empty_span())),
            cond_expr: Some(Box::new(Expression::Number(
                Number::from_str("1").unwrap(),
                empty_span(),
            ))),
            grouping: false,
        };
        let errs = evaluator
            .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("condition must be a boolean")
        );

        // filter false
        let evaluator = Evaluator::new(&ctx);
        let f = ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Variable("arr".to_string(), empty_span())),
            key_expr: None,
            val_expr: Box::new(Expression::Bool(true, empty_span())),
            cond_expr: Some(Box::new(Expression::Bool(false, empty_span()))),
            grouping: false,
        };
        let (val, _) = evaluator
            .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
            .unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::Array(vec![]));

        // map bad key
        let evaluator = Evaluator::new(&ctx);
        let f = ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Variable("arr".to_string(), empty_span())),
            key_expr: Some(Box::new(Expression::Bool(true, empty_span()))),
            val_expr: Box::new(Expression::Bool(true, empty_span())),
            cond_expr: None,
            grouping: false,
        };
        let errs = evaluator
            .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("key must evaluate to string")
        );
    }

    #[test]
    fn test_eval_for_expr_grouping() {
        let mut ctx = Context::new();
        let arr = Value::new(
            Type::Tuple(vec![]),
            ValueData::Array(vec![
                Value::new(Type::Bool, ValueData::Bool(true)),
                Value::new(Type::Bool, ValueData::Bool(false)),
            ]),
        );
        ctx.set_variable("arr", arr);
        let evaluator = Evaluator::new(&ctx);

        let f = ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Variable("arr".to_string(), empty_span())),
            key_expr: Some(Box::new(Expression::String(
                "fixed".to_string(),
                empty_span(),
            ))),
            val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
            cond_expr: None,
            grouping: true,
        };
        let (val, _) = evaluator
            .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
            .unwrap();

        let mut expected_map = BTreeMap::new();
        expected_map.insert(
            "fixed".to_string(),
            Value::new(
                Type::Tuple(vec![]),
                ValueData::Array(vec![
                    Value::new(Type::Bool, ValueData::Bool(true)),
                    Value::new(Type::Bool, ValueData::Bool(false)),
                ]),
            ),
        );
        assert_eq!(val.data.as_ref(), &ValueData::Object(expected_map));
    }

    #[test]
    fn test_eval_traversal() {
        let mut ctx = Context::new();

        let mut map = BTreeMap::new();
        map.insert(
            "a".to_string(),
            Value::new(
                Type::Number,
                ValueData::Number(Number::from_str("1").unwrap()),
            ),
        );
        let obj = Value::new(Type::object(BTreeMap::new()), ValueData::Object(map));
        ctx.set_variable("obj", obj);

        let arr = Value::new(
            Type::Tuple(vec![]),
            ValueData::Array(vec![Value::new(Type::Bool, ValueData::Bool(true))]),
        );
        ctx.set_variable("arr", arr);

        let evaluator = Evaluator::new(&ctx);
        let t1 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("obj".to_string(), empty_span())),
            operators: vec![TraversalOperator::GetAttr("a".to_string(), empty_span())],
        };
        let (val, _) = evaluator
            .evaluate(&Expression::Traversal(Box::new(t1), empty_span()))
            .unwrap();
        assert_eq!(
            val,
            Value::new(
                Type::Number,
                ValueData::Number(Number::from_str("1").unwrap())
            )
        );

        let evaluator = Evaluator::new(&ctx);
        let t2 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
            operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
        };
        let (val2, _) = evaluator
            .evaluate(&Expression::Traversal(Box::new(t2), empty_span()))
            .unwrap();
        assert_eq!(val2, Value::new(Type::Bool, ValueData::Bool(true)));
    }

    #[test]
    fn test_eval_for_expr_tuple() {
        let ctx = Context::new();
        let evaluator = Evaluator::new(&ctx);

        let f = ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![
                    Expression::Number(Number::from_str("1").unwrap(), empty_span()),
                    Expression::Number(Number::from_str("2").unwrap(), empty_span()),
                ],
                empty_span(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::UnaryOp(
                UnaryOp::Neg,
                Box::new(Expression::Variable("v".to_string(), empty_span())),
                empty_span(),
            )),
            cond_expr: None,
            grouping: false,
        };

        let (val, _) = evaluator
            .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
            .unwrap();

        let expected_arr = vec![
            Value::new(
                Type::Number,
                ValueData::Number(Number::from_str("-1").unwrap()),
            ),
            Value::new(
                Type::Number,
                ValueData::Number(Number::from_str("-2").unwrap()),
            ),
        ];
        assert_eq!(val.data.as_ref(), &ValueData::Array(expected_arr));
    }

    #[test]
    fn test_eval_for_expr_object() {
        let ctx = Context::new();
        let evaluator = Evaluator::new(&ctx);

        let k1 = Expression::String("a".to_string(), empty_span());
        let v1 = Expression::Number(Number::from_str("1").unwrap(), empty_span());

        let f = ForExpr {
            key_var: Some("k".to_string()),
            val_var: "v".to_string(),
            collection: Box::new(Expression::Object(vec![(k1, v1)], empty_span())),
            key_expr: Some(Box::new(Expression::Variable(
                "k".to_string(),
                empty_span(),
            ))),
            val_expr: Box::new(Expression::UnaryOp(
                UnaryOp::Neg,
                Box::new(Expression::Variable("v".to_string(), empty_span())),
                empty_span(),
            )),
            cond_expr: None,
            grouping: false,
        };

        let (val, _) = evaluator
            .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
            .unwrap();
        let mut expected_map = BTreeMap::new();
        expected_map.insert(
            "a".to_string(),
            Value::new(
                Type::Number,
                ValueData::Number(Number::from_str("-1").unwrap()),
            ),
        );
        assert_eq!(val.data.as_ref(), &ValueData::Object(expected_map));
    }

    #[test]
    fn test_eval_cov() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::object(BTreeMap::new())));

        let evaluator = Evaluator::new(&ctx);
        let t1 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("unk".to_string(), empty_span())),
            operators: vec![TraversalOperator::GetAttr("a".to_string(), empty_span())],
        };
        let (_val, _) = evaluator
            .evaluate(&Expression::Traversal(Box::new(t1), empty_span()))
            .unwrap();

        let arr = Value::new(Type::Tuple(vec![]), ValueData::Array(vec![]));
        ctx.set_variable("arr", arr);
        let evaluator = Evaluator::new(&ctx);
        let t2 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("arr".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::Number(Number::from_str("-1").unwrap(), empty_span()),
                empty_span(),
            )],
        };
        let errs = evaluator
            .evaluate(&Expression::Traversal(Box::new(t2), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Invalid array index")
        );

        let evaluator = Evaluator::new(&ctx);
        let f = ForExpr {
            key_var: None,
            val_var: "v".to_string(),
            collection: Box::new(Expression::Tuple(
                vec![Expression::Bool(true, empty_span())],
                empty_span(),
            )),
            key_expr: None,
            val_expr: Box::new(Expression::Bool(true, empty_span())),
            cond_expr: Some(Box::new(Expression::Number(
                Number::from_str("1").unwrap(),
                empty_span(),
            ))),
            grouping: false,
        };
        let errs = evaluator
            .evaluate(&Expression::ForExpr(Box::new(f), empty_span()))
            .err()
            .unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("must be a boolean")
        );

        let evaluator = Evaluator::new(&ctx);
        let t3 = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("unk".to_string(), empty_span())),
            operators: vec![TraversalOperator::Index(
                Expression::Variable("unk".to_string(), empty_span()),
                empty_span(),
            )],
        };
        let (_val, _) = evaluator
            .evaluate(&Expression::Traversal(Box::new(t3), empty_span()))
            .unwrap();
    }

    #[test]
    fn test_eval_template_parts() {
        let mut ctx = Context::new();
        ctx.set_variable(
            "num",
            Value::new(
                Type::Number,
                ValueData::Number(crate::number::Number::new(
                    std::str::FromStr::from_str("42.0").unwrap(),
                )),
            ),
        );
        ctx.set_variable("bool", Value::new(Type::Bool, ValueData::Bool(true)));
        ctx.set_variable(
            "obj",
            Value::new(
                Type::object(std::collections::BTreeMap::new()),
                ValueData::Object(std::collections::BTreeMap::new()),
            ),
        );
        ctx.set_variable("null", Value::null(Type::Dynamic));

        let evaluator = Evaluator::new(&ctx);
        let span = empty_span();
        let expr = Expression::Template(
            vec![
                crate::ast::expr::TemplatePart::Literal("literal ".to_string(), span.clone()),
                crate::ast::expr::TemplatePart::Interpolation(
                    Expression::Variable("num".to_string(), span.clone()),
                    span.clone(),
                ),
                crate::ast::expr::TemplatePart::Literal(" ".to_string(), span.clone()),
                crate::ast::expr::TemplatePart::Interpolation(
                    Expression::Variable("bool".to_string(), span.clone()),
                    span.clone(),
                ),
                crate::ast::expr::TemplatePart::Literal(" ".to_string(), span.clone()),
                crate::ast::expr::TemplatePart::Interpolation(
                    Expression::Variable("null".to_string(), span.clone()),
                    span.clone(),
                ),
                crate::ast::expr::TemplatePart::Literal(" ".to_string(), span.clone()),
                crate::ast::expr::TemplatePart::Interpolation(
                    Expression::Variable("obj".to_string(), span.clone()),
                    span.clone(),
                ),
                crate::ast::expr::TemplatePart::Directive(
                    crate::ast::expr::Directive::If {
                        cond: Expression::Bool(true, span.clone()),
                        true_expr: vec![],
                        else_ifs: vec![],
                        false_expr: None,
                    },
                    span.clone(),
                ),
            ],
            span,
        );

        let result = evaluator.evaluate(&expr);
        assert!(result.is_err());
        let diags = result.err().unwrap();
        assert_eq!(diags.errors().len(), 1); // One for object interpolation
        assert!(
            diags.errors()[0]
                .error
                .to_string()
                .contains("Cannot stringify complex value")
        );
    }

    #[test]
    fn test_eval_binary_math_unknown_rhs() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::Number));
        let evaluator = Evaluator::new(&ctx);

        let expr = Expression::BinaryOp(
            crate::ast::expr::BinaryOp::Add,
            Box::new(Expression::Number(
                crate::number::Number::new(std::str::FromStr::from_str("1.0").unwrap()),
                empty_span(),
            )),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );

        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert!(val.is_unknown());
        assert_eq!(*val.ty(), Type::Number);

        let expr2 = Expression::BinaryOp(
            crate::ast::expr::BinaryOp::Eq,
            Box::new(Expression::Number(
                crate::number::Number::new(std::str::FromStr::from_str("1.0").unwrap()),
                empty_span(),
            )),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let evaluator2 = Evaluator::new(&ctx);
        let (val2, _) = evaluator2.evaluate(&expr2).unwrap();
        assert!(val2.is_unknown());
        assert_eq!(*val2.ty(), Type::Bool);
    }

    #[test]
    fn test_eval_template_interpolation_null() {
        let mut ctx = Context::new();
        ctx.set_variable("null_var", Value::null(Type::String));
        let evaluator = Evaluator::new(&ctx);

        let expr = Expression::Template(
            vec![crate::ast::expr::TemplatePart::Interpolation(
                Expression::Variable("null_var".to_string(), empty_span()),
                empty_span(),
            )],
            empty_span(),
        );

        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val.ty(), &Type::String);
    }

    #[test]
    fn test_eval_binary_math_unknown() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::Number));
        let evaluator = Evaluator::new(&ctx);

        let expr = Expression::BinaryOp(
            crate::ast::expr::BinaryOp::Add,
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            Box::new(Expression::Bool(false, empty_span())),
            empty_span(),
        );

        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert!(val.is_unknown());
        assert_eq!(*val.ty(), Type::Number);
    }

    #[test]
    fn test_eval_cov2() {
        let mut ctx = Context::new();
        ctx.set_variable("unk", Value::unknown(Type::Bool));
        let evaluator = Evaluator::new(&ctx);

        let expr = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Bool(false, empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(false)));

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));

        // Also Splat stub hit
        let _evaluator = Evaluator::new(&ctx);
        let expr = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Bool(true, empty_span())),
                operators: vec![TraversalOperator::FullSplat(empty_span())],
            }),
            empty_span(),
        );
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .to_lowercase()
                .contains("splat")
        );

        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::Template(vec![], empty_span());
        let val = evaluator.evaluate(&expr).unwrap().0;
        assert_eq!(
            val,
            Value::new(Type::String, ValueData::String(String::new()))
        );

        // test FuncCall error
        let expr = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "test_func_err".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        let mut ctx = Context::new();
        ctx.set_function(
            "test_func_err",
            crate::eval::func::Function {
                name: "test_func_err".to_string(),
                func: std::sync::Arc::new(|_| Err("func error".to_string())),
                signature: None,
            },
        );
        let _evaluator = Evaluator::new(&ctx);
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Function call 'test_func_err' failed")
        );

        // test Conditional non-bool condition
        let expr = Expression::Conditional(
            Box::new(crate::ast::expr::Conditional {
                cond_expr: Expression::String("not bool".to_string(), empty_span()),
                true_expr: Expression::Bool(true, empty_span()),
                false_expr: Expression::Bool(false, empty_span()),
            }),
            empty_span(),
        );
        let _evaluator = Evaluator::new(&ctx);
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Condition must be a boolean")
        );

        // test BinaryOp::And / Or non-bool RHS
        let expr = Expression::BinaryOp(
            crate::ast::expr::BinaryOp::And,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::String("not bool".to_string(), empty_span())),
            empty_span(),
        );
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Right operand of && must be a boolean")
        );

        let expr = Expression::BinaryOp(
            crate::ast::expr::BinaryOp::Or,
            Box::new(Expression::Bool(false, empty_span())),
            Box::new(Expression::String("not bool".to_string(), empty_span())),
            empty_span(),
        );
        let evaluator = Evaluator::new(&ctx);
        let errs = evaluator.evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Right operand of || must be a boolean")
        );

        // test index out of bounds
        let expr = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Tuple(vec![], empty_span())),
                operators: vec![crate::ast::expr::TraversalOperator::Index(
                    Expression::Number(crate::number::Number::from_str("1").unwrap(), empty_span()),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let errs = Evaluator::new(&ctx).evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Index out of bounds")
        );

        // test index missing key
        let expr = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Object(vec![], empty_span())),
                operators: vec![crate::ast::expr::TraversalOperator::Index(
                    Expression::String("missing".to_string(), empty_span()),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let errs = Evaluator::new(&ctx).evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Key 'missing' not found")
        );

        // test for expr cond not bool
        let expr = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Tuple(
                    vec![Expression::Bool(true, empty_span())],
                    empty_span(),
                )),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
                cond_expr: Some(Box::new(Expression::String(
                    "not bool".to_string(),
                    empty_span(),
                ))),
                grouping: false,
            }),
            empty_span(),
        );
        let errs = Evaluator::new(&ctx).evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("For expression condition must be a boolean")
        );

        // test for expr error in sub eval loop (error in cond)
        let expr = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Tuple(
                    vec![Expression::Bool(true, empty_span())],
                    empty_span(),
                )),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
                cond_expr: Some(Box::new(Expression::FuncCall(
                    Box::new(crate::ast::expr::FuncCall {
                        name: "test_func_err".into(),
                        args: vec![],
                        expand_final: false,
                    }),
                    empty_span(),
                ))),
                grouping: false,
            }),
            empty_span(),
        );
        let errs = Evaluator::new(&ctx).evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Function call 'test_func_err' failed")
        );
        assert!(
            errs.errors()[1]
                .error
                .to_string()
                .contains("For expression condition must be a boolean")
        );

        // test for expr error in value eval loop
        let expr = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Tuple(
                    vec![Expression::Bool(true, empty_span())],
                    empty_span(),
                )),
                key_expr: None,
                val_expr: Box::new(Expression::FuncCall(
                    Box::new(crate::ast::expr::FuncCall {
                        name: "test_func_err".into(),
                        args: vec![],
                        expand_final: false,
                    }),
                    empty_span(),
                )),
                cond_expr: None,
                grouping: false,
            }),
            empty_span(),
        );
        let errs = Evaluator::new(&ctx).evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Function call 'test_func_err' failed")
        );

        // test for expr error in loop cond == false
        let _expr = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Tuple(
                    vec![Expression::Bool(true, empty_span())],
                    empty_span(),
                )),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
                cond_expr: Some(Box::new(Expression::BinaryOp(
                    crate::ast::expr::BinaryOp::And,
                    Box::new(Expression::Bool(false, empty_span())),
                    Box::new(Expression::FuncCall(
                        Box::new(crate::ast::expr::FuncCall {
                            name: "test_func_err".into(),
                            args: vec![],
                            expand_final: false,
                        }),
                        empty_span(),
                    )),
                    empty_span(),
                ))),
                grouping: false,
            }),
            empty_span(),
        );
        // Wait, And short-circuits. Let's do Or
        let expr = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Tuple(
                    vec![Expression::Bool(true, empty_span())],
                    empty_span(),
                )),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
                cond_expr: Some(Box::new(Expression::BinaryOp(
                    crate::ast::expr::BinaryOp::Or,
                    Box::new(Expression::Bool(false, empty_span())),
                    Box::new(Expression::FuncCall(
                        Box::new(crate::ast::expr::FuncCall {
                            name: "test_func_err".into(),
                            args: vec![],
                            expand_final: false,
                        }),
                        empty_span(),
                    )),
                    empty_span(),
                ))),
                grouping: false,
            }),
            empty_span(),
        );
        let errs = Evaluator::new(&ctx).evaluate(&expr).err().unwrap();
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("Function call 'test_func_err' failed")
        );

        // test Parentheses
        let expr =
            Expression::Parentheses(Box::new(Expression::Bool(true, empty_span())), empty_span());
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));

        // test Conditional false branch
        let expr = Expression::Conditional(
            Box::new(crate::ast::expr::Conditional {
                cond_expr: Expression::Bool(false, empty_span()),
                true_expr: Expression::String("true".to_string(), empty_span()),
                false_expr: Expression::String("false".to_string(), empty_span()),
            }),
            empty_span(),
        );
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
        assert_eq!(
            val,
            Value::new(Type::String, ValueData::String("false".to_string()))
        );

        // test BinaryOp::And evaluating RHS
        let expr = Expression::BinaryOp(
            crate::ast::expr::BinaryOp::And,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::Bool(true, empty_span())),
            empty_span(),
        );
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(true)));

        // test BinaryOp::Or evaluating RHS
        let expr = Expression::BinaryOp(
            crate::ast::expr::BinaryOp::Or,
            Box::new(Expression::Bool(false, empty_span())),
            Box::new(Expression::Bool(false, empty_span())),
            empty_span(),
        );
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
        assert_eq!(val, Value::new(Type::Bool, ValueData::Bool(false)));

        // test Index success
        let expr = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Tuple(
                    vec![Expression::String("a".to_string(), empty_span())],
                    empty_span(),
                )),
                operators: vec![crate::ast::expr::TraversalOperator::Index(
                    Expression::Number(crate::number::Number::from_str("0").unwrap(), empty_span()),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
        assert_eq!(
            val,
            Value::new(Type::String, ValueData::String("a".to_string()))
        );

        // test Object Index success
        let expr = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Object(
                    vec![(
                        Expression::String("k".to_string(), empty_span()),
                        Expression::String("v".to_string(), empty_span()),
                    )],
                    empty_span(),
                )),
                operators: vec![crate::ast::expr::TraversalOperator::Index(
                    Expression::String("k".to_string(), empty_span()),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
        assert_eq!(
            val,
            Value::new(Type::String, ValueData::String("v".to_string()))
        );

        // test for expr cond == true
        let expr = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Tuple(
                    vec![Expression::Bool(true, empty_span())],
                    empty_span(),
                )),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), empty_span())),
                cond_expr: Some(Box::new(Expression::Bool(true, empty_span()))),
                grouping: false,
            }),
            empty_span(),
        );
        let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
        assert_eq!(
            val.data.as_ref(),
            &ValueData::Array(vec![Value::new(Type::Bool, ValueData::Bool(true))])
        );

        // test for expr cond == false with diagnostic (we need the condition to evaluate to false but push an error)
        // A condition can't easily return false AND push an error unless it's evaluating a sub-expression that pushes an error
        // but wait, if it pushes an error, the return value is Unknown! Not Bool(false)!
        // Wait, how can cond_expr evaluate to Bool(false) but have errors?
        // Ah, if `cond_expr` is `false && func_err()` it short-circuits and returns `false`, but doesn't run the RHS!
        // What if we do an error in `collection`? No, we need it in `cond_expr`.
        // I will just ignore 488-489 branch because it's practically unreachable or I'll just write it.
    }

    #[test]
    fn test_eval_extra_cov() {
        let mut ctx = Context::new();
        ctx.set_variable(
            "str_var",
            Value::new(Type::String, ValueData::String("s".to_string())),
        );
        let evaluator = Evaluator::new(&ctx);
        let expr = Expression::Template(
            vec![crate::ast::expr::TemplatePart::Interpolation(
                Expression::Variable("str_var".to_string(), empty_span()),
                empty_span(),
            )],
            empty_span(),
        );
        let (val, _) = evaluator.evaluate(&expr).unwrap();
        assert_eq!(
            val,
            Value::new(Type::String, ValueData::String("s".to_string()))
        );

        let evaluator = Evaluator::new(&ctx);
        let expr_and = Expression::BinaryOp(
            crate::ast::expr::BinaryOp::And,
            Box::new(Expression::Bool(true, empty_span())),
            Box::new(Expression::String("not bool".to_string(), empty_span())),
            empty_span(),
        );
        assert!(evaluator.evaluate(&expr_and).is_err());

        let evaluator = Evaluator::new(&ctx);
        let expr_or = Expression::BinaryOp(
            crate::ast::expr::BinaryOp::Or,
            Box::new(Expression::Bool(false, empty_span())),
            Box::new(Expression::String("not bool".to_string(), empty_span())),
            empty_span(),
        );
        assert!(evaluator.evaluate(&expr_or).is_err());

        ctx.set_variable("unk", Value::unknown(Type::Number));
        for op in [
            crate::ast::expr::BinaryOp::NotEq,
            crate::ast::expr::BinaryOp::Less,
            crate::ast::expr::BinaryOp::LessEq,
            crate::ast::expr::BinaryOp::Greater,
            crate::ast::expr::BinaryOp::GreaterEq,
        ] {
            let evaluator = Evaluator::new(&ctx);
            let expr = Expression::BinaryOp(
                op,
                Box::new(Expression::Variable("unk".to_string(), empty_span())),
                Box::new(Expression::Variable("unk".to_string(), empty_span())),
                empty_span(),
            );
            let (val, _) = evaluator.evaluate(&expr).unwrap();
            assert_eq!(*val.ty(), Type::Bool);
        }
    }

    #[test]
    fn test_eval_binary_right_operand_errors_covered() {
        let input = r"
            a = true && 123
            b = false || 123
        ";
        let ctx = Context::new();
        let body = crate::api::parse(input).unwrap();
        let res_a = Evaluator::new(&ctx).evaluate(&body.attributes["a"].expr);
        let res_b = Evaluator::new(&ctx).evaluate(&body.attributes["b"].expr);
        assert!(res_a.is_err());
        assert!(res_b.is_err());
    }

    #[test]
    fn test_eval_template_directives_exhaustive() {
        use crate::ast::expr::{Directive, Expression, TemplatePart};
        let span = empty_span();

        // 1. Directive::If branches
        let mut ctx = Context::new();
        ctx.set_variable("admin", Value::new(Type::Bool, ValueData::Bool(true)));
        let expr_if_true = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::If {
                    cond: Expression::Variable("admin".to_string(), span.clone()),
                    true_expr: vec![TemplatePart::Literal("Admin".to_string(), span.clone())],
                    else_ifs: vec![],
                    false_expr: Some(vec![TemplatePart::Literal(
                        "User".to_string(),
                        span.clone(),
                    )]),
                },
                span.clone(),
            )],
            span.clone(),
        );
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_if_true).unwrap();
        assert_eq!(
            v,
            Value::new(Type::String, ValueData::String("Admin".to_string()))
        );

        // If false with else
        ctx.set_variable("admin", Value::new(Type::Bool, ValueData::Bool(false)));
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_if_true).unwrap();
        assert_eq!(
            v,
            Value::new(Type::String, ValueData::String("User".to_string()))
        );

        // If false without else
        let expr_no_else = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::If {
                    cond: Expression::Bool(false, span.clone()),
                    true_expr: vec![TemplatePart::Literal("Admin".to_string(), span.clone())],
                    else_ifs: vec![],
                    false_expr: None,
                },
                span.clone(),
            )],
            span.clone(),
        );
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_no_else).unwrap();
        assert_eq!(
            v,
            Value::new(Type::String, ValueData::String(String::new()))
        );

        // If false with else_if branches
        let expr_elif = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::If {
                    cond: Expression::Bool(false, span.clone()),
                    true_expr: vec![TemplatePart::Literal("A".to_string(), span.clone())],
                    else_ifs: vec![
                        (
                            Expression::Bool(false, span.clone()),
                            vec![TemplatePart::Literal("B".to_string(), span.clone())],
                        ),
                        (
                            Expression::Bool(true, span.clone()),
                            vec![TemplatePart::Literal("C".to_string(), span.clone())],
                        ),
                    ],
                    false_expr: Some(vec![TemplatePart::Literal("D".to_string(), span.clone())]),
                },
                span.clone(),
            )],
            span.clone(),
        );
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_elif).unwrap();
        assert_eq!(
            v,
            Value::new(Type::String, ValueData::String("C".to_string()))
        );

        // If condition unknown
        let expr_unk_cond = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::If {
                    cond: Expression::Variable("unk".to_string(), span.clone()),
                    true_expr: vec![TemplatePart::Literal("A".to_string(), span.clone())],
                    else_ifs: vec![],
                    false_expr: None,
                },
                span.clone(),
            )],
            span.clone(),
        );
        ctx.set_variable("unk", Value::unknown(Type::Bool));
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_unk_cond).unwrap();
        assert!(v.is_unknown());

        // If condition not boolean
        let expr_bad_cond = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::If {
                    cond: Expression::Number(Number::from_str("123").unwrap(), span.clone()),
                    true_expr: vec![],
                    else_ifs: vec![],
                    false_expr: None,
                },
                span.clone(),
            )],
            span.clone(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_bad_cond).is_err());

        // Else if condition unknown
        let expr_elif_unk = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::If {
                    cond: Expression::Bool(false, span.clone()),
                    true_expr: vec![],
                    else_ifs: vec![(
                        Expression::Variable("unk".to_string(), span.clone()),
                        vec![],
                    )],
                    false_expr: None,
                },
                span.clone(),
            )],
            span.clone(),
        );
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_elif_unk).unwrap();
        assert!(v.is_unknown());

        // Else if condition not boolean
        let expr_elif_bad = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::If {
                    cond: Expression::Bool(false, span.clone()),
                    true_expr: vec![],
                    else_ifs: vec![(
                        Expression::String("not bool".to_string(), span.clone()),
                        vec![],
                    )],
                    false_expr: None,
                },
                span.clone(),
            )],
            span.clone(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_elif_bad).is_err());

        // 2. Directive::For loops
        // Array with val_var
        let expr_for_arr = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::For {
                    key_var: None,
                    val_var: "item".to_string(),
                    collection: Expression::Tuple(
                        vec![
                            Expression::String("a".to_string(), span.clone()),
                            Expression::String("b".to_string(), span.clone()),
                        ],
                        span.clone(),
                    ),
                    body: vec![
                        TemplatePart::Literal("[".to_string(), span.clone()),
                        TemplatePart::Interpolation(
                            Expression::Variable("item".to_string(), span.clone()),
                            span.clone(),
                        ),
                        TemplatePart::Literal("]".to_string(), span.clone()),
                    ],
                },
                span.clone(),
            )],
            span.clone(),
        );
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_for_arr).unwrap();
        assert_eq!(
            v,
            Value::new(Type::String, ValueData::String("[a][b]".to_string()))
        );

        // Array with key_var and val_var
        let expr_for_key_val = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::For {
                    key_var: Some("idx".to_string()),
                    val_var: "item".to_string(),
                    collection: Expression::Tuple(
                        vec![
                            Expression::String("x".to_string(), span.clone()),
                            Expression::String("y".to_string(), span.clone()),
                        ],
                        span.clone(),
                    ),
                    body: vec![
                        TemplatePart::Interpolation(
                            Expression::Variable("idx".to_string(), span.clone()),
                            span.clone(),
                        ),
                        TemplatePart::Literal(":".to_string(), span.clone()),
                        TemplatePart::Interpolation(
                            Expression::Variable("item".to_string(), span.clone()),
                            span.clone(),
                        ),
                        TemplatePart::Literal(";".to_string(), span.clone()),
                    ],
                },
                span.clone(),
            )],
            span.clone(),
        );
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_for_key_val).unwrap();
        assert_eq!(
            v,
            Value::new(Type::String, ValueData::String("0:x;1:y;".to_string()))
        );

        // Set iteration
        let mut set_vals = std::collections::BTreeSet::new();
        set_vals.insert(Value::new(
            Type::String,
            ValueData::String("set_item".to_string()),
        ));
        ctx.set_variable(
            "my_set",
            Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_vals)),
        );
        let expr_for_set = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::For {
                    key_var: None,
                    val_var: "item".to_string(),
                    collection: Expression::Variable("my_set".to_string(), span.clone()),
                    body: vec![TemplatePart::Interpolation(
                        Expression::Variable("item".to_string(), span.clone()),
                        span.clone(),
                    )],
                },
                span.clone(),
            )],
            span.clone(),
        );
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_for_set).unwrap();
        assert_eq!(
            v,
            Value::new(Type::String, ValueData::String("set_item".to_string()))
        );

        // Object iteration
        let mut obj_vals = BTreeMap::new();
        obj_vals.insert(
            "k1".to_string(),
            Value::new(Type::String, ValueData::String("v1".to_string())),
        );
        ctx.set_variable(
            "my_obj",
            Value::new(Type::object(BTreeMap::new()), ValueData::Object(obj_vals)),
        );
        let expr_for_obj = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::For {
                    key_var: Some("k".to_string()),
                    val_var: "v".to_string(),
                    collection: Expression::Variable("my_obj".to_string(), span.clone()),
                    body: vec![
                        TemplatePart::Interpolation(
                            Expression::Variable("k".to_string(), span.clone()),
                            span.clone(),
                        ),
                        TemplatePart::Literal("=".to_string(), span.clone()),
                        TemplatePart::Interpolation(
                            Expression::Variable("v".to_string(), span.clone()),
                            span.clone(),
                        ),
                    ],
                },
                span.clone(),
            )],
            span.clone(),
        );
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_for_obj).unwrap();
        assert_eq!(
            v,
            Value::new(Type::String, ValueData::String("k1=v1".to_string()))
        );

        // For collection unknown
        let expr_for_unk = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::For {
                    key_var: None,
                    val_var: "x".to_string(),
                    collection: Expression::Variable("unk".to_string(), span.clone()),
                    body: vec![],
                },
                span.clone(),
            )],
            span.clone(),
        );
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_for_unk).unwrap();
        assert!(v.is_unknown());

        // For collection null
        let expr_for_null = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::For {
                    key_var: None,
                    val_var: "x".to_string(),
                    collection: Expression::Null(span.clone()),
                    body: vec![TemplatePart::Literal("never".to_string(), span.clone())],
                },
                span.clone(),
            )],
            span.clone(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_for_null).is_err());

        // For collection invalid type (e.g. number)
        let expr_for_bad = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::For {
                    key_var: None,
                    val_var: "x".to_string(),
                    collection: Expression::Number(Number::from_str("42").unwrap(), span.clone()),
                    body: vec![],
                },
                span.clone(),
            )],
            span.clone(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_for_bad).is_err());

        // 3. Directive::Strip
        let expr_strip = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::Strip {
                    strip_left: true,
                    strip_right: true,
                },
                span.clone(),
            )],
            span,
        );
        let (v, _) = Evaluator::new(&ctx).evaluate(&expr_strip).unwrap();
        assert_eq!(
            v,
            Value::new(Type::String, ValueData::String(String::new()))
        );

        // 4. End-to-end HCL string with parser and evaluator
        let input = r#"
            greeting = "Hello, %{ for name in names }${name}%{ if name == \"Alice\" } (boss)%{ endif }, %{ endfor }bye!"
        "#;
        let mut full_ctx = Context::new();
        full_ctx.set_variable(
            "names",
            Value::new(
                Type::Tuple(vec![]),
                ValueData::Array(vec![
                    Value::new(Type::String, ValueData::String("Alice".to_string())),
                    Value::new(Type::String, ValueData::String("Bob".to_string())),
                ]),
            ),
        );
        let body = crate::api::parse(input).unwrap();
        let (val, _) = Evaluator::new(&full_ctx)
            .evaluate(&body.attributes["greeting"].expr)
            .unwrap();
        assert_eq!(
            val,
            Value::new(
                Type::String,
                ValueData::String("Hello, Alice (boss), Bob, bye!".to_string())
            )
        );
    }

    #[test]
    fn test_mark_propagation_comprehensive() {
        use crate::types::ValueMark;

        let mut ctx = Context::with_stdlib();
        let sens_num =
            Value::new(Type::Number, ValueData::Number(10_i32.into())).mark(ValueMark::Sensitive);
        let sens_bool = Value::new(Type::Bool, ValueData::Bool(true)).mark(ValueMark::Sensitive);
        let sens_str =
            Value::new(Type::String, ValueData::String("vault".into())).mark(ValueMark::Sensitive);

        ctx.set_variable("sens_num", sens_num);
        ctx.set_variable("sens_bool", sens_bool);
        ctx.set_variable("sens_str", sens_str);

        let parse_expr = |s: &str| {
            let body = crate::api::parse(&format!("v = {s}")).unwrap();
            body.attributes["v"].expr.clone()
        };

        // Arithmetic
        let cases = [
            ("sens_num + 5", Type::Number),
            ("20 - sens_num", Type::Number),
            ("sens_num * 3", Type::Number),
            ("sens_num / 2", Type::Number),
            ("sens_num % 3", Type::Number),
        ];
        for (expr_str, expected_ty) in cases {
            let expr = parse_expr(expr_str);
            let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
            assert_eq!(val.ty(), &expected_ty);
            assert!(
                val.has_mark(&ValueMark::Sensitive),
                "Expression {expr_str} should retain Sensitive mark"
            );
        }

        // Division & modulo by zero with marked numbers
        let div_zero = parse_expr("sens_num / 0");
        let mut sub_eval = Evaluator::new(&ctx);
        let val = sub_eval.eval_expr(&div_zero);
        assert!(val.is_unknown());
        assert!(val.has_mark(&ValueMark::Sensitive));
        assert!(sub_eval.diagnostics.errors().iter().any(|d| {
            d.summary
                .as_deref()
                .unwrap_or("")
                .contains("Division by zero")
        }));

        let mod_zero = parse_expr("sens_num % 0");
        let mut sub_eval2 = Evaluator::new(&ctx);
        let val = sub_eval2.eval_expr(&mod_zero);
        assert!(val.is_unknown());
        assert!(val.has_mark(&ValueMark::Sensitive));
        assert!(sub_eval2.diagnostics.errors().iter().any(|d| {
            d.summary
                .as_deref()
                .unwrap_or("")
                .contains("Division by zero in modulo")
        }));

        // Comparisons & Equality
        let cmp_cases = [
            "sens_num == 10",
            "sens_num != 5",
            "sens_num > 5",
            "sens_num >= 10",
            "sens_num < 20",
            "sens_num <= 10",
        ];
        for expr_str in cmp_cases {
            let expr = parse_expr(expr_str);
            let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
            assert_eq!(val.ty(), &Type::Bool);
            assert_eq!(val.data.as_ref(), &ValueData::Bool(true));
            assert!(
                val.has_mark(&ValueMark::Sensitive),
                "Expression {expr_str} should retain Sensitive mark"
            );
        }

        // Logical ops & short-circuiting with marks
        let log_cases = [
            "sens_bool && true",
            "true && sens_bool",
            "sens_bool || false",
            "false || sens_bool",
        ];
        for expr_str in log_cases {
            let expr = parse_expr(expr_str);
            let (val, _) = Evaluator::new(&ctx).evaluate(&expr).unwrap();
            assert_eq!(val.data.as_ref(), &ValueData::Bool(true));
            assert!(
                val.has_mark(&ValueMark::Sensitive),
                "Expression {expr_str} should retain Sensitive mark"
            );
        }

        // Unary ops
        let not_expr = parse_expr("!sens_bool");
        let (val, _) = Evaluator::new(&ctx).evaluate(&not_expr).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::Bool(false));
        assert!(val.has_mark(&ValueMark::Sensitive));

        let neg_expr = parse_expr("-sens_num");
        let (val, _) = Evaluator::new(&ctx).evaluate(&neg_expr).unwrap();
        assert!(val.has_mark(&ValueMark::Sensitive));

        // Conditionals
        let cond_expr = parse_expr("sens_bool ? \"yes\" : \"no\"");
        let (val, _) = Evaluator::new(&ctx).evaluate(&cond_expr).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::String("yes".into()));
        assert!(val.has_mark(&ValueMark::Sensitive));

        // Traversals
        let mut obj_map = BTreeMap::new();
        obj_map.insert(
            "secret".to_string(),
            Value::new(Type::String, ValueData::String("pass123".into())),
        );
        let marked_obj =
            Value::new(Type::Dynamic, ValueData::Object(obj_map)).mark(ValueMark::Sensitive);
        ctx.set_variable("marked_obj", marked_obj);

        let t_attr = parse_expr("marked_obj.secret");
        let (val, _) = Evaluator::new(&ctx).evaluate(&t_attr).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::String("pass123".into()));
        assert!(val.has_mark(&ValueMark::Sensitive));

        let t_idx = parse_expr("marked_obj[\"secret\"]");
        let (val, _) = Evaluator::new(&ctx).evaluate(&t_idx).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::String("pass123".into()));
        assert!(val.has_mark(&ValueMark::Sensitive));

        let marked_arr = Value::new(
            Type::List(Box::new(Type::String)),
            ValueData::Array(vec![Value::new(
                Type::String,
                ValueData::String("item0".into()),
            )]),
        )
        .mark(ValueMark::Sensitive);
        ctx.set_variable("marked_arr", marked_arr);

        let t_arr_idx = parse_expr("marked_arr[0]");
        let (val, _) = Evaluator::new(&ctx).evaluate(&t_arr_idx).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::String("item0".into()));
        assert!(val.has_mark(&ValueMark::Sensitive));

        let t_arr_legacy = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("marked_arr".to_string(), empty_span())),
                operators: vec![crate::ast::expr::TraversalOperator::LegacyIndex(
                    0,
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let (val, _) = Evaluator::new(&ctx).evaluate(&t_arr_legacy).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::String("item0".into()));
        assert!(val.has_mark(&ValueMark::Sensitive));

        // String templates & interpolation
        let tpl_expr = parse_expr("\"prefix-${sens_str}-suffix\"");
        let (val, _) = Evaluator::new(&ctx).evaluate(&tpl_expr).unwrap();
        assert_eq!(
            val.data.as_ref(),
            &ValueData::String("prefix-vault-suffix".into())
        );
        assert!(val.has_mark(&ValueMark::Sensitive));

        let tpl_if = parse_expr("\"result: %{ if sens_bool }visible%{ endif }\"");
        let (val, _) = Evaluator::new(&ctx).evaluate(&tpl_if).unwrap();
        assert_eq!(
            val.data.as_ref(),
            &ValueData::String("result: visible".into())
        );
        assert!(val.has_mark(&ValueMark::Sensitive));

        let tpl_for = parse_expr("\"items: %{ for x in marked_arr }${x}%{ endfor }\"");
        let (val, _) = Evaluator::new(&ctx).evaluate(&tpl_for).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::String("items: item0".into()));
        assert!(val.has_mark(&ValueMark::Sensitive));

        // For-expression
        let for_tuple = Expression::ForExpr(
            Box::new(crate::ast::expr::ForExpr {
                key_var: None,
                val_var: "x".to_string(),
                collection: Box::new(Expression::Variable("marked_arr".to_string(), empty_span())),
                key_expr: None,
                val_expr: Box::new(parse_expr("upper(x)")),
                cond_expr: None,
                grouping: false,
            }),
            empty_span(),
        );
        let (val, _) = Evaluator::new(&ctx).evaluate(&for_tuple).unwrap();
        assert!(val.has_mark(&ValueMark::Sensitive));

        // Functions: standard library marks propagation
        let func_upper = parse_expr("upper(sens_str)");
        let (val, _) = Evaluator::new(&ctx).evaluate(&func_upper).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::String("VAULT".into()));
        assert!(val.has_mark(&ValueMark::Sensitive));

        // Functions: sensitive, issensitive, nonsensitive
        let is_sens = parse_expr("issensitive(sens_str)");
        let (val, _) = Evaluator::new(&ctx).evaluate(&is_sens).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::Bool(true));
        assert!(
            !val.has_mark(&ValueMark::Sensitive),
            "issensitive result must NOT be marked sensitive"
        );

        let is_not_sens = parse_expr("issensitive(\"plain\")");
        let (val, _) = Evaluator::new(&ctx).evaluate(&is_not_sens).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::Bool(false));
        assert!(!val.has_mark(&ValueMark::Sensitive));

        let non_sens = parse_expr("nonsensitive(sens_str)");
        let (val, _) = Evaluator::new(&ctx).evaluate(&non_sens).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::String("vault".into()));
        assert!(
            !val.has_mark(&ValueMark::Sensitive),
            "nonsensitive result must NOT be marked sensitive"
        );

        let make_sens = parse_expr("sensitive(\"raw\")");
        let (val, _) = Evaluator::new(&ctx).evaluate(&make_sens).unwrap();
        assert_eq!(val.data.as_ref(), &ValueData::String("raw".into()));
        assert!(val.has_mark(&ValueMark::Sensitive));
    }

    #[test]
    fn test_evaluator_coverage_exhaustive() {
        use crate::types::ValueMark;
        use crate::types::refinement::Refinement;

        let mut ctx = Context::new();

        // 1. expand_final with empty args (line 175)
        let expr_empty_expand = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "foo".into(),
                args: vec![],
                expand_final: true,
            }),
            empty_span(),
        );
        let res_empty = Evaluator::new(&ctx).eval_expr(&expr_empty_expand);
        assert!(res_empty.is_unknown());

        // 2. Variadic function signatures: null errors and coercion errors
        let sig = crate::eval::func::FunctionSignature::with_static_return_type(
            vec![crate::eval::func::FunctionParamSpec::new(
                "p1",
                Type::String,
            )],
            Type::Dynamic,
        )
        .with_variadic(crate::eval::func::FunctionParamSpec::new(
            "rest",
            Type::Number,
        ));

        ctx.set_function(
            "test_var",
            crate::eval::func::Function::new(
                "test_var",
                std::sync::Arc::new(|_| Ok(Value::new(Type::Bool, ValueData::Bool(true)))),
            )
            .with_signature(sig),
        );

        // Fixed param null when !allow_null
        let expr_null_fixed = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "test_var".into(),
                args: vec![
                    Expression::Null(empty_span()),
                    Expression::Number(1.into(), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let mut evaluator_null_fixed = Evaluator::new(&ctx);
        let res_null_fixed = evaluator_null_fixed.eval_expr(&expr_null_fixed);
        assert!(res_null_fixed.is_unknown());
        assert!(evaluator_null_fixed.diagnostics.has_errors());

        // Variadic param null when !allow_null
        let expr_null_variadic = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "test_var".into(),
                args: vec![
                    Expression::String("a".to_string(), empty_span()),
                    Expression::Null(empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let mut evaluator_null_variadic = Evaluator::new(&ctx);
        let res_null_variadic = evaluator_null_variadic.eval_expr(&expr_null_variadic);
        assert!(res_null_variadic.is_unknown());
        assert!(evaluator_null_variadic.diagnostics.has_errors());

        // Fixed param coercion failure
        let expr_bad_fixed = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "test_var".into(),
                args: vec![
                    Expression::Tuple(vec![], empty_span()),
                    Expression::Number(1.into(), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let mut evaluator_bad_fixed = Evaluator::new(&ctx);
        let res_bad_fixed = evaluator_bad_fixed.eval_expr(&expr_bad_fixed);
        assert!(res_bad_fixed.is_unknown());
        assert!(evaluator_bad_fixed.diagnostics.has_errors());

        // Variadic param coercion failure
        let expr_bad_variadic = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "test_var".into(),
                args: vec![
                    Expression::String("a".to_string(), empty_span()),
                    Expression::Tuple(vec![], empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let mut evaluator_bad_variadic = Evaluator::new(&ctx);
        let res_bad_variadic = evaluator_bad_variadic.eval_expr(&expr_bad_variadic);
        assert!(res_bad_variadic.is_unknown());
        assert!(evaluator_bad_variadic.diagnostics.has_errors());

        // Variadic call with successful coercion
        let expr_coerce_success = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "test_var".into(),
                args: vec![
                    Expression::String("a".to_string(), empty_span()),
                    Expression::String("42".to_string(), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let res_coerce_success = Evaluator::new(&ctx).eval_expr(&expr_coerce_success);
        assert_eq!(*res_coerce_success.data, ValueData::Bool(true));

        // 3. nonsensitive preserving non-sensitive marks
        let mut ctx_std = Context::with_stdlib();
        let mut custom_marked = Value::new(Type::String, ValueData::String("secret".to_string()));
        custom_marked
            .marks
            .insert(ValueMark::Custom("tag".to_string()));
        custom_marked.marks.insert(ValueMark::Sensitive);
        ctx_std.set_variable("marked_val", custom_marked);

        let expr_non_sens = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "nonsensitive".into(),
                args: vec![Expression::Variable("marked_val".to_string(), empty_span())],
                expand_final: false,
            }),
            empty_span(),
        );
        let res_non_sens = Evaluator::new(&ctx_std).eval_expr(&expr_non_sens);
        assert!(!res_non_sens.marks.contains(&ValueMark::Sensitive));
        assert!(
            res_non_sens
                .marks
                .contains(&ValueMark::Custom("tag".to_string()))
        );

        // 4. BinaryOp::And and BinaryOp::Or non-boolean left operand
        let expr_and = Expression::BinaryOp(
            BinaryOp::And,
            Box::new(Expression::Number(1.into(), empty_span())),
            Box::new(Expression::Bool(true, empty_span())),
            empty_span(),
        );
        let res_and = Evaluator::new(&ctx).eval_expr(&expr_and);
        assert_eq!(*res_and.data, ValueData::Bool(true));

        let expr_or = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Number(1.into(), empty_span())),
            Box::new(Expression::Bool(false, empty_span())),
            empty_span(),
        );
        let res_or = Evaluator::new(&ctx).eval_expr(&expr_or);
        assert_eq!(*res_or.data, ValueData::Bool(false));

        // 5. Refinements in BinaryOp::Add with unknown strings
        let mut ref_l = Refinement::not_null();
        ref_l.string_prefix = Some("pre_".to_string());
        ref_l.string_suffix = Some("_suf".to_string());
        ref_l.string_length_min = Some(10);
        ref_l.string_length_max = Some(20);
        let unk_left = Value::unknown_refined(Type::String, ref_l);

        let mut ref_r = Refinement::not_null();
        ref_r.string_prefix = Some("start_".to_string());
        ref_r.string_suffix = Some("_end".to_string());
        ref_r.string_length_min = Some(5);
        ref_r.string_length_max = Some(15);
        let unk_right = Value::unknown_refined(Type::String, ref_r);

        let known_mid = Value::new(Type::String, ValueData::String("middle".to_string()));

        ctx.set_variable("unk_left", unk_left);
        ctx.set_variable("unk_right", unk_right);
        ctx.set_variable("known_mid", known_mid);

        let expr_both_unknown = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("unk_left".to_string(), empty_span())),
            Box::new(Expression::Variable("unk_right".to_string(), empty_span())),
            empty_span(),
        );
        let res_both_unknown = Evaluator::new(&ctx).eval_expr(&expr_both_unknown);
        assert!(res_both_unknown.is_unknown());
        assert_eq!(
            res_both_unknown
                .refinement()
                .and_then(|r| r.string_prefix.clone())
                .as_deref(),
            Some("pre_")
        );
        assert_eq!(
            res_both_unknown
                .refinement()
                .and_then(|r| r.string_suffix.clone())
                .as_deref(),
            Some("_end")
        );
        assert_eq!(
            res_both_unknown
                .refinement()
                .and_then(|r| r.string_length_min),
            Some(15)
        );
        assert_eq!(
            res_both_unknown
                .refinement()
                .and_then(|r| r.string_length_max),
            Some(35)
        );

        let expr_known_left = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("known_mid".to_string(), empty_span())),
            Box::new(Expression::Variable("unk_right".to_string(), empty_span())),
            empty_span(),
        );
        let res_known_left = Evaluator::new(&ctx).eval_expr(&expr_known_left);
        assert!(res_known_left.is_unknown());
        assert_eq!(
            res_known_left
                .refinement()
                .and_then(|r| r.string_prefix.clone())
                .as_deref(),
            Some("middlestart_")
        );
        assert_eq!(
            res_known_left
                .refinement()
                .and_then(|r| r.string_suffix.clone())
                .as_deref(),
            Some("_end")
        );

        let expr_known_right = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("unk_left".to_string(), empty_span())),
            Box::new(Expression::Variable("known_mid".to_string(), empty_span())),
            empty_span(),
        );
        let res_known_right = Evaluator::new(&ctx).eval_expr(&expr_known_right);
        assert!(res_known_right.is_unknown());
        assert_eq!(
            res_known_right
                .refinement()
                .and_then(|r| r.string_prefix.clone())
                .as_deref(),
            Some("pre_")
        );
        assert_eq!(
            res_known_right
                .refinement()
                .and_then(|r| r.string_suffix.clone())
                .as_deref(),
            Some("_sufmiddle")
        );

        let unk_bare = Value::unknown(Type::String);
        ctx.set_variable("unk_bare", unk_bare);

        let expr_known_bare = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("known_mid".to_string(), empty_span())),
            Box::new(Expression::Variable("unk_bare".to_string(), empty_span())),
            empty_span(),
        );
        let res_known_bare = Evaluator::new(&ctx).eval_expr(&expr_known_bare);
        assert!(res_known_bare.is_unknown());
        assert_eq!(
            res_known_bare
                .refinement()
                .and_then(|r| r.string_prefix.clone())
                .as_deref(),
            Some("middle")
        );

        let expr_bare_known = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("unk_bare".to_string(), empty_span())),
            Box::new(Expression::Variable("known_mid".to_string(), empty_span())),
            empty_span(),
        );
        let res_bare_known = Evaluator::new(&ctx).eval_expr(&expr_bare_known);
        assert!(res_bare_known.is_unknown());
        assert_eq!(
            res_bare_known
                .refinement()
                .and_then(|r| r.string_suffix.clone())
                .as_deref(),
            Some("middle")
        );

        let corrupted_str = Value::new(Type::String, ValueData::Bool(true));
        ctx.set_variable("corrupted_str", corrupted_str);

        let expr_corrupt_left = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable(
                "corrupted_str".to_string(),
                empty_span(),
            )),
            Box::new(Expression::Variable("unk_bare".to_string(), empty_span())),
            empty_span(),
        );
        let res_corrupt_left = Evaluator::new(&ctx).eval_expr(&expr_corrupt_left);
        assert!(res_corrupt_left.is_unknown());

        let expr_corrupt_right = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("unk_bare".to_string(), empty_span())),
            Box::new(Expression::Variable(
                "corrupted_str".to_string(),
                empty_span(),
            )),
            empty_span(),
        );
        let res_corrupt_right = Evaluator::new(&ctx).eval_expr(&expr_corrupt_right);
        assert!(res_corrupt_right.is_unknown());

        // 6. BinaryOp::Add string concatenation
        let expr_str_concat = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::String("foo_".to_string(), empty_span())),
            Box::new(Expression::String("bar".to_string(), empty_span())),
            empty_span(),
        );
        let res_str_concat = Evaluator::new(&ctx).eval_expr(&expr_str_concat);
        assert_eq!(
            *res_str_concat.data,
            ValueData::String("foo_bar".to_string())
        );

        let expr_num_str = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::String("10".to_string(), empty_span())),
            Box::new(Expression::String("foo".to_string(), empty_span())),
            empty_span(),
        );
        let res_num_str = Evaluator::new(&ctx).eval_expr(&expr_num_str);
        assert_eq!(*res_num_str.data, ValueData::String("10foo".to_string()));

        let expr_num_num = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::String("10".to_string(), empty_span())),
            Box::new(Expression::String("20".to_string(), empty_span())),
            empty_span(),
        );
        let res_num_num = Evaluator::new(&ctx).eval_expr(&expr_num_num);
        assert_eq!(*res_num_num.data, ValueData::Number(30.into()));

        let bad_str_a = Value::new(Type::String, ValueData::Bool(true));
        let bad_str_b = Value::new(Type::String, ValueData::String("ok".to_string()));
        ctx.set_variable("bad_a", bad_str_a);
        ctx.set_variable("bad_b", bad_str_b);
        let expr_bad_str = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("bad_a".to_string(), empty_span())),
            Box::new(Expression::Variable("bad_b".to_string(), empty_span())),
            empty_span(),
        );
        let mut eval_bad_s = Evaluator::new(&ctx);
        let res_bad_s = eval_bad_s.eval_expr(&expr_bad_str);
        assert!(res_bad_s.is_unknown());
        assert!(eval_bad_s.diagnostics.has_errors());

        // 7. Binary operator requires numbers error
        let bad_num_val = Value::new(Type::Number, ValueData::Bool(true));
        ctx.set_variable("bad_num_val", bad_num_val);
        let expr_bad_num = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable(
                "bad_num_val".to_string(),
                empty_span(),
            )),
            Box::new(Expression::Number(1.into(), empty_span())),
            empty_span(),
        );
        let mut eval_bad_n = Evaluator::new(&ctx);
        let res_bad_n = eval_bad_n.eval_expr(&expr_bad_num);
        assert!(res_bad_n.is_unknown());
        assert!(eval_bad_n.diagnostics.has_errors());

        // 8. eval_single_step with AttrSplat and FullSplat
        let base_bool_val = Value::new(Type::Bool, ValueData::Bool(true));
        let mut eval_step_attr = Evaluator::new(&ctx);
        let step_attr_res = eval_step_attr
            .eval_single_step(&base_bool_val, &TraversalOperator::AttrSplat(empty_span()));
        assert!(step_attr_res.is_unknown());
        assert!(eval_step_attr.diagnostics.has_errors());

        let mut eval_step_full = Evaluator::new(&ctx);
        let step_full_res = eval_step_full
            .eval_single_step(&base_bool_val, &TraversalOperator::FullSplat(empty_span()));
        assert!(step_full_res.is_unknown());
        assert!(eval_step_full.diagnostics.has_errors());

        // 9. AttrSplat branches
        let t_null_splat = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Null(empty_span())),
            operators: vec![
                TraversalOperator::AttrSplat(empty_span()),
                TraversalOperator::GetAttr("a".to_string(), empty_span()),
                TraversalOperator::Index(Expression::Number(0.into(), empty_span()), empty_span()),
            ],
        };
        let mut eval_null_splat = Evaluator::new(&ctx);
        let res_null_splat = eval_null_splat.eval_traversal(&t_null_splat, empty_span());
        assert!(res_null_splat.is_unknown());

        let mut set_map = BTreeSet::new();
        let mut map_item = BTreeMap::new();
        map_item.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("set_item".to_string())),
        );
        set_map.insert(Value::new(Type::Dynamic, ValueData::Object(map_item)));
        ctx.set_variable(
            "my_set_obj",
            Value::new(Type::Set(Box::new(Type::Dynamic)), ValueData::Set(set_map)),
        );

        let t_set_traversal = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("my_set_obj".to_string(), empty_span())),
            operators: vec![
                TraversalOperator::AttrSplat(empty_span()),
                TraversalOperator::GetAttr("name".to_string(), empty_span()),
            ],
        };
        let res_set_splat = Evaluator::new(&ctx).eval_traversal(&t_set_traversal, empty_span());
        assert_eq!(
            *res_set_splat.data,
            ValueData::Array(vec![Value::new(
                Type::String,
                ValueData::String("set_item".to_string())
            )])
        );

        let t_arr_idx = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Tuple(
                vec![Expression::Tuple(
                    vec![Expression::Number(42.into(), empty_span())],
                    empty_span(),
                )],
                empty_span(),
            )),
            operators: vec![
                TraversalOperator::AttrSplat(empty_span()),
                TraversalOperator::Index(Expression::Number(0.into(), empty_span()), empty_span()),
            ],
        };
        let res_arr_idx = Evaluator::new(&ctx).eval_traversal(&t_arr_idx, empty_span());
        assert_eq!(
            *res_arr_idx.data,
            ValueData::Array(vec![Value::new(Type::Number, ValueData::Number(42.into()))])
        );

        let t_unk_attr = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("unk_bare".to_string(), empty_span())),
            operators: vec![TraversalOperator::AttrSplat(empty_span())],
        };
        let res_unk_attr = Evaluator::new(&ctx).eval_traversal(&t_unk_attr, empty_span());
        assert!(res_unk_attr.is_unknown());

        // 10. FullSplat branches
        let t_null_full = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Null(empty_span())),
            operators: vec![
                TraversalOperator::FullSplat(empty_span()),
                TraversalOperator::FullSplat(empty_span()),
            ],
        };
        let res_null_full = Evaluator::new(&ctx).eval_traversal(&t_null_full, empty_span());
        assert_eq!(*res_null_full.data, ValueData::Array(vec![]));

        let t_null_full_chain = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Null(empty_span())),
            operators: vec![
                TraversalOperator::FullSplat(empty_span()),
                TraversalOperator::GetAttr("foo".to_string(), empty_span()),
                TraversalOperator::FullSplat(empty_span()),
            ],
        };
        let res_null_chain = Evaluator::new(&ctx).eval_traversal(&t_null_full_chain, empty_span());
        assert_eq!(*res_null_chain.data, ValueData::Array(vec![]));

        let mut set_raw = BTreeSet::new();
        set_raw.insert(Value::new(
            Type::String,
            ValueData::String("item".to_string()),
        ));
        ctx.set_variable(
            "simple_set",
            Value::new(Type::Set(Box::new(Type::String)), ValueData::Set(set_raw)),
        );

        let t_full_set = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Variable("simple_set".to_string(), empty_span())),
            operators: vec![TraversalOperator::FullSplat(empty_span())],
        };
        let res_full_set = Evaluator::new(&ctx).eval_traversal(&t_full_set, empty_span());
        assert_eq!(
            *res_full_set.data,
            ValueData::Array(vec![Value::new(
                Type::String,
                ValueData::String("item".to_string())
            )])
        );

        let t_splat_splat = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Tuple(
                vec![Expression::Tuple(
                    vec![Expression::String("nested".to_string(), empty_span())],
                    empty_span(),
                )],
                empty_span(),
            )),
            operators: vec![
                TraversalOperator::FullSplat(empty_span()),
                TraversalOperator::FullSplat(empty_span()),
            ],
        };
        let res_splat_splat = Evaluator::new(&ctx).eval_traversal(&t_splat_splat, empty_span());
        assert_eq!(
            *res_splat_splat.data,
            ValueData::Array(vec![Value::new(
                Type::Tuple(vec![Type::String]),
                ValueData::Array(vec![Value::new(
                    Type::String,
                    ValueData::String("nested".to_string())
                )])
            )])
        );

        let t_empty_full = crate::ast::expr::Traversal {
            expr: Box::new(Expression::Tuple(vec![], empty_span())),
            operators: vec![TraversalOperator::FullSplat(empty_span())],
        };
        let res_empty_full = Evaluator::new(&ctx).eval_traversal(&t_empty_full, empty_span());
        assert_eq!(*res_empty_full.data, ValueData::Array(vec![]));

        // 11. Template for-directive error propagation
        let expr_template_err = Expression::Template(
            vec![crate::ast::expr::TemplatePart::Directive(
                crate::ast::expr::Directive::For {
                    key_var: None,
                    val_var: "item".to_string(),
                    collection: Expression::Tuple(
                        vec![Expression::Number(1.into(), empty_span())],
                        empty_span(),
                    ),
                    body: vec![crate::ast::expr::TemplatePart::Interpolation(
                        Expression::BinaryOp(
                            BinaryOp::Div,
                            Box::new(Expression::Number(1.into(), empty_span())),
                            Box::new(Expression::Number(0.into(), empty_span())),
                            empty_span(),
                        ),
                        empty_span(),
                    )],
                },
                empty_span(),
            )],
            empty_span(),
        );
        let mut evaluator_template_err = Evaluator::new(&ctx);
        let _ = evaluator_template_err.eval_expr(&expr_template_err);
        assert!(evaluator_template_err.diagnostics.has_errors());
    }

    #[test]
    fn test_func_value_return_type_coercion() {
        use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
        use std::sync::Arc;

        let mut ctx = Context::new();
        let sig = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("arg", Type::String)],
            Type::Dynamic,
        )
        .with_value_return_type(Arc::new(|_args| Ok(Type::Number)));

        let func = Function::new("return_coerced", Arc::new(|args| Ok(args[0].clone())))
            .with_signature(sig);

        ctx.set_function("return_coerced", func);

        let expr = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "return_coerced".into(),
                args: vec![Expression::String("42".to_string(), empty_span())],
                expand_final: false,
            }),
            empty_span(),
        );

        let mut evaluator = Evaluator::new(&ctx);
        let val = evaluator.eval_expr(&expr);
        assert_eq!(val.ty(), &Type::Number);
        assert_eq!(val.to_string(), "42");
    }

    #[test]
    fn test_evaluator_subexpression_callouts_on_error() {
        use crate::types::ValueMark;

        let mut ctx = Context::new();
        ctx.set_variable(
            "count",
            Value::new(Type::Number, ValueData::Number(Number::from(10))),
        );
        ctx.set_variable(
            "secret_token",
            Value::new(Type::String, ValueData::String("token123".into()))
                .mark(ValueMark::Sensitive),
        );

        // Expression: var.count + var.secret_token (cannot add number and string)
        let expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("count".to_string(), empty_span())),
            Box::new(Expression::Variable(
                "secret_token".to_string(),
                empty_span(),
            )),
            empty_span(),
        );

        let evaluator = Evaluator::new(&ctx);
        let diags = evaluator.evaluate(&expr).err().unwrap();

        assert!(diags.has_errors());
        let diag = &diags.errors()[0];
        assert_ne!(diag.eval_callouts.len(), 0);

        // Check that var.count was recorded
        let count_callout = diag
            .eval_callouts
            .iter()
            .find(|c| c.expression_text == "var.count")
            .unwrap();
        assert_eq!(count_callout.evaluated_value, "10");

        // Check that var.secret_token was recorded and masked
        let secret_callout = diag
            .eval_callouts
            .iter()
            .find(|c| c.expression_text == "var.secret_token")
            .unwrap();
        assert_eq!(secret_callout.evaluated_value, "(sensitive value)");
    }

    #[test]
    fn test_variable_and_attribute_typo_suggestions() {
        let mut parent_ctx = Context::new();
        parent_ctx.set_variable(
            "instance_type",
            Value::new(Type::String, ValueData::String("t3.micro".into())),
        );

        let mut child_ctx = Context::new_child(&parent_ctx);
        child_ctx.set_variable(
            "user_count",
            Value::new(Type::Number, ValueData::Number(Number::from(5))),
        );

        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "hostname".to_string(),
            Value::new(Type::String, ValueData::String("localhost".into())),
        );
        map.insert(
            "port_number".to_string(),
            Value::new(Type::Number, ValueData::Number(Number::from(8080))),
        );
        child_ctx.set_variable(
            "server",
            Value::new(
                Type::object(std::collections::BTreeMap::new()),
                ValueData::Object(map),
            ),
        );

        // 1. Misspelled variable in child scope: "user_cont" instead of "user_count"
        let expr_var = Expression::Variable("user_cont".to_string(), empty_span());
        let err_var = Evaluator::new(&child_ctx)
            .evaluate(&expr_var)
            .err()
            .unwrap();
        let diag_var = &err_var.errors()[0];
        assert_eq!(
            diag_var.detail.as_deref(),
            Some("Did you mean var.user_count?")
        );

        // 2. Misspelled variable from parent scope: "var.instance_typ" instead of "var.instance_type"
        let expr_parent = Expression::Variable("var.instance_typ".to_string(), empty_span());
        let err_parent = Evaluator::new(&child_ctx)
            .evaluate(&expr_parent)
            .err()
            .unwrap();
        let diag_parent = &err_parent.errors()[0];
        assert_eq!(
            diag_parent.detail.as_deref(),
            Some("Did you mean var.instance_type?")
        );

        // 3. Misspelled object attribute: server.hostnam
        let expr_attr = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("server".to_string(), empty_span())),
                operators: vec![TraversalOperator::GetAttr(
                    "hostnam".to_string(),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let err_attr = Evaluator::new(&child_ctx)
            .evaluate(&expr_attr)
            .err()
            .unwrap();
        let diag_attr = &err_attr.errors()[0];
        assert_eq!(diag_attr.detail.as_deref(), Some("Did you mean .hostname?"));

        // 4. Misspelled index key: server["port_numbr"]
        let expr_idx = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("server".to_string(), empty_span())),
                operators: vec![TraversalOperator::Index(
                    Expression::String("port_numbr".to_string(), empty_span()),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let err_idx = Evaluator::new(&child_ctx)
            .evaluate(&expr_idx)
            .err()
            .unwrap();
        let diag_idx = &err_idx.errors()[0];
        assert_eq!(
            diag_idx.detail.as_deref(),
            Some("Did you mean .port_number?")
        );
    }

    #[test]
    fn test_capsule_operator_overloading_and_hooks() {
        use crate::types::ty::CapsuleOps;
        use std::sync::Arc;

        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        struct Matrix2x2(i64, i64, i64, i64);

        let ops = Arc::new(
            CapsuleOps::new(
                "matrix2x2",
                Arc::new(|a, b| {
                    if let (Some(m1), Some(m2)) =
                        (a.downcast_ref::<Matrix2x2>(), b.downcast_ref::<Matrix2x2>())
                    {
                        m1 == m2
                    } else {
                        false
                    }
                }),
                Arc::new(|_| 0),
            )
            .with_add(Arc::new(|a, b| {
                let m1 = a.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                let m2 = b.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                Ok(Box::new(Matrix2x2(
                    m1.0 + m2.0,
                    m1.1 + m2.1,
                    m1.2 + m2.2,
                    m1.3 + m2.3,
                )))
            }))
            .with_sub(Arc::new(|a, b| {
                let m1 = a.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                let m2 = b.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                Ok(Box::new(Matrix2x2(
                    m1.0 - m2.0,
                    m1.1 - m2.1,
                    m1.2 - m2.2,
                    m1.3 - m2.3,
                )))
            }))
            .with_mul(Arc::new(|a, b| {
                let m1 = a.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                let m2 = b.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                Ok(Box::new(Matrix2x2(
                    m1.0 * m2.0 + m1.1 * m2.2,
                    m1.0 * m2.1 + m1.1 * m2.3,
                    m1.2 * m2.0 + m1.3 * m2.2,
                    m1.2 * m2.1 + m1.3 * m2.3,
                )))
            }))
            .with_div(Arc::new(|a, b| {
                let m1 = a.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                let m2 = b.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                if m2.0 == 0 || m2.3 == 0 {
                    return Err("matrix division by singular matrix".to_string());
                }
                Ok(Box::new(Matrix2x2(
                    m1.0 / m2.0,
                    m1.1 / m2.1,
                    m1.2 / m2.2,
                    m1.3 / m2.3,
                )))
            }))
            .with_modulo(Arc::new(|a, b| {
                let m1 = a.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                let m2 = b.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                if m2.0 == 0 {
                    return Err("matrix modulo by zero".to_string());
                }
                Ok(Box::new(Matrix2x2(
                    m1.0 % m2.0,
                    m1.1 % m2.1,
                    m1.2 % m2.2,
                    m1.3 % m2.3,
                )))
            }))
            .with_neg(Arc::new(|a| {
                let m = a.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                Ok(Box::new(Matrix2x2(-m.0, -m.1, -m.2, -m.3)))
            }))
            .with_cmp(Arc::new(|a, b| {
                let m1 = a.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                let m2 = b.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                let det1 = m1.0 * m1.3 - m1.1 * m1.2;
                let det2 = m2.0 * m2.3 - m2.1 * m2.2;
                Ok(det1.cmp(&det2))
            }))
            .with_attr_get(Arc::new(|a, name| {
                let m = a.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                match name {
                    "a" => Ok(Value::new(Type::Number, ValueData::Number(m.0.into()))),
                    "b" => Ok(Value::new(Type::Number, ValueData::Number(m.1.into()))),
                    "c" => Ok(Value::new(Type::Number, ValueData::Number(m.2.into()))),
                    "d" => Ok(Value::new(Type::Number, ValueData::Number(m.3.into()))),
                    _ => Err(format!("unknown matrix field '{name}'")),
                }
            }))
            .with_index_get(Arc::new(|a, idx| {
                let m = a.downcast_ref::<Matrix2x2>().ok_or("expected matrix")?;
                if let ValueData::Number(n) = &*idx.data {
                    match n.0.to_usize() {
                        Some(0) => Ok(Value::new(Type::Number, ValueData::Number(m.0.into()))),
                        Some(1) => Ok(Value::new(Type::Number, ValueData::Number(m.1.into()))),
                        Some(2) => Ok(Value::new(Type::Number, ValueData::Number(m.2.into()))),
                        Some(3) => Ok(Value::new(Type::Number, ValueData::Number(m.3.into()))),
                        _ => Err("matrix index out of bounds".to_string()),
                    }
                } else {
                    Err("matrix index must be an integer".to_string())
                }
            })),
        );

        let m1_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(1, 2, 3, 4));
        let m2_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(5, 6, 7, 8));

        let mut ctx = Context::new();
        ctx.set_variable("m1", m1_val.clone());
        ctx.set_variable("m2", m2_val.clone());

        // 1. Arithmetic: Add, Sub, Mul, Div, Mod
        let expr_add = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("m2".to_string(), empty_span())),
            empty_span(),
        );
        let (res_add, _) = Evaluator::new(&ctx).evaluate(&expr_add).unwrap();
        assert_eq!(
            res_add.downcast_ref::<Matrix2x2>(),
            Some(&Matrix2x2(6, 8, 10, 12))
        );

        let expr_sub = Expression::BinaryOp(
            BinaryOp::Sub,
            Box::new(Expression::Variable("m2".to_string(), empty_span())),
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            empty_span(),
        );
        let (res_sub, _) = Evaluator::new(&ctx).evaluate(&expr_sub).unwrap();
        assert_eq!(
            res_sub.downcast_ref::<Matrix2x2>(),
            Some(&Matrix2x2(4, 4, 4, 4))
        );

        let expr_mul = Expression::BinaryOp(
            BinaryOp::Mul,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("m2".to_string(), empty_span())),
            empty_span(),
        );
        let (res_mul, _) = Evaluator::new(&ctx).evaluate(&expr_mul).unwrap();
        assert_eq!(
            res_mul.downcast_ref::<Matrix2x2>(),
            Some(&Matrix2x2(19, 22, 43, 50))
        );

        let expr_div = Expression::BinaryOp(
            BinaryOp::Div,
            Box::new(Expression::Variable("m2".to_string(), empty_span())),
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            empty_span(),
        );
        let (res_div, _) = Evaluator::new(&ctx).evaluate(&expr_div).unwrap();
        assert_eq!(
            res_div.downcast_ref::<Matrix2x2>(),
            Some(&Matrix2x2(5, 3, 2, 2))
        );

        let expr_mod = Expression::BinaryOp(
            BinaryOp::Mod,
            Box::new(Expression::Variable("m2".to_string(), empty_span())),
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            empty_span(),
        );
        let (res_mod, _) = Evaluator::new(&ctx).evaluate(&expr_mod).unwrap();
        assert_eq!(
            res_mod.downcast_ref::<Matrix2x2>(),
            Some(&Matrix2x2(0, 0, 1, 0))
        );

        // 2. Unary negation
        let expr_neg = Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            empty_span(),
        );
        let (res_neg, _) = Evaluator::new(&ctx).evaluate(&expr_neg).unwrap();
        assert_eq!(
            res_neg.downcast_ref::<Matrix2x2>(),
            Some(&Matrix2x2(-1, -2, -3, -4))
        );

        // 3. Relational comparisons: Less, LessEq, Greater, GreaterEq, Eq, NotEq
        // det(m1) = 1*4 - 2*3 = -2
        // det(m2) = 5*8 - 6*7 = -2
        // det(m3) = 1*1 - 0*0 = 1
        let m3_val = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(1, 0, 0, 1));
        ctx.set_variable("m3", m3_val.clone());

        let expr_lt = Expression::BinaryOp(
            BinaryOp::Less,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("m3".to_string(), empty_span())),
            empty_span(),
        );
        let (res_lt, _) = Evaluator::new(&ctx).evaluate(&expr_lt).unwrap();
        assert_eq!(*res_lt.data, ValueData::Bool(true));

        let expr_lte = Expression::BinaryOp(
            BinaryOp::LessEq,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("m2".to_string(), empty_span())),
            empty_span(),
        );
        let (res_lte, _) = Evaluator::new(&ctx).evaluate(&expr_lte).unwrap();
        assert_eq!(*res_lte.data, ValueData::Bool(true));

        let expr_greater = Expression::BinaryOp(
            BinaryOp::Greater,
            Box::new(Expression::Variable("m3".to_string(), empty_span())),
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            empty_span(),
        );
        let (res_greater, _) = Evaluator::new(&ctx).evaluate(&expr_greater).unwrap();
        assert_eq!(*res_greater.data, ValueData::Bool(true));

        let expr_greatereq = Expression::BinaryOp(
            BinaryOp::GreaterEq,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("m2".to_string(), empty_span())),
            empty_span(),
        );
        let (res_greatereq, _) = Evaluator::new(&ctx).evaluate(&expr_greatereq).unwrap();
        assert_eq!(*res_greatereq.data, ValueData::Bool(true));

        let expr_eq = Expression::BinaryOp(
            BinaryOp::Eq,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            empty_span(),
        );
        let (res_eq, _) = Evaluator::new(&ctx).evaluate(&expr_eq).unwrap();
        assert_eq!(*res_eq.data, ValueData::Bool(true));

        let expr_noteq = Expression::BinaryOp(
            BinaryOp::NotEq,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("m2".to_string(), empty_span())),
            empty_span(),
        );
        let (res_noteq, _) = Evaluator::new(&ctx).evaluate(&expr_noteq).unwrap();
        assert_eq!(*res_noteq.data, ValueData::Bool(true));

        // 4. Attribute access: .a, .b, .c, .d
        let expr_attr_a = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
                operators: vec![TraversalOperator::GetAttr("a".to_string(), empty_span())],
            }),
            empty_span(),
        );
        let (res_a, _) = Evaluator::new(&ctx).evaluate(&expr_attr_a).unwrap();
        assert_eq!(*res_a.data, ValueData::Number(1.into()));

        // 5. Indexing: [0], [1], [2], [3] and legacy index .0
        let expr_idx_2 = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
                operators: vec![TraversalOperator::Index(
                    Expression::Number(2.into(), empty_span()),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let (res_idx, _) = Evaluator::new(&ctx).evaluate(&expr_idx_2).unwrap();
        assert_eq!(*res_idx.data, ValueData::Number(3.into()));

        let expr_legacy_idx = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
                operators: vec![TraversalOperator::LegacyIndex(3, empty_span())],
            }),
            empty_span(),
        );
        let (res_legacy, _) = Evaluator::new(&ctx).evaluate(&expr_legacy_idx).unwrap();
        assert_eq!(*res_legacy.data, ValueData::Number(4.into()));

        for (attr_name, expected_num) in [("b", 2), ("c", 3), ("d", 4)] {
            let expr_attr = Expression::Traversal(
                Box::new(crate::ast::expr::Traversal {
                    expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
                    operators: vec![TraversalOperator::GetAttr(
                        attr_name.to_string(),
                        empty_span(),
                    )],
                }),
                empty_span(),
            );
            let (res, _) = Evaluator::new(&ctx).evaluate(&expr_attr).unwrap();
            assert_eq!(*res.data, ValueData::Number(expected_num.into()));
        }

        for (idx_val, expected_num) in [(0, 1), (1, 2)] {
            let expr_idx = Expression::Traversal(
                Box::new(crate::ast::expr::Traversal {
                    expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
                    operators: vec![TraversalOperator::Index(
                        Expression::Number(idx_val.into(), empty_span()),
                        empty_span(),
                    )],
                }),
                empty_span(),
            );
            let (res, _) = Evaluator::new(&ctx).evaluate(&expr_idx).unwrap();
            assert_eq!(*res.data, ValueData::Number(expected_num.into()));
        }

        let expr_str_idx = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
                operators: vec![TraversalOperator::Index(
                    Expression::String("not_an_int".to_string(), empty_span()),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let err_str_idx = Evaluator::new(&ctx).evaluate(&expr_str_idx).err().unwrap();
        assert!(
            err_str_idx.errors()[0]
                .error
                .to_string()
                .contains("matrix index must be an integer")
        );

        let expr_m_eq_num = Expression::BinaryOp(
            BinaryOp::Eq,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Number(42.into(), empty_span())),
            empty_span(),
        );
        let (res_m_eq_num, _) = Evaluator::new(&ctx).evaluate(&expr_m_eq_num).unwrap();
        assert_eq!(*res_m_eq_num.data, ValueData::Bool(false));

        let other_capsule = Value::capsule_with_ops(
            "other_capsule",
            Arc::new(CapsuleOps::new(
                "other_capsule",
                Arc::new(|_, _| false),
                Arc::new(|_| 0),
            )),
            999_i32,
        );
        ctx.set_variable("other", other_capsule.clone());
        let expr_m_eq_other = Expression::BinaryOp(
            BinaryOp::Eq,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("other".to_string(), empty_span())),
            empty_span(),
        );
        let (res_other, _) = Evaluator::new(&ctx).evaluate(&expr_m_eq_other).unwrap();
        assert_eq!(*res_other.data, ValueData::Bool(false));

        let corrupt_matrix = Value::new(m1_val.ty().clone(), ValueData::Capsule(Arc::new(999_i32)));
        assert_ne!(m1_val.unmark().0, corrupt_matrix.unmark().0);

        // 6. Direct calls to evaluate_binary_op and evaluate_unary_op
        let mut eval = Evaluator::new(&ctx);
        let res_dir_bin = eval.evaluate_binary_op(
            BinaryOp::Add,
            &Expression::Variable("m1".to_string(), empty_span()),
            &Expression::Variable("m2".to_string(), empty_span()),
            empty_span(),
        );
        assert_eq!(
            res_dir_bin.downcast_ref::<Matrix2x2>(),
            Some(&Matrix2x2(6, 8, 10, 12))
        );

        let res_dir_un = eval.evaluate_unary_op(
            UnaryOp::Neg,
            &Expression::Variable("m1".to_string(), empty_span()),
            empty_span(),
        );
        assert_eq!(
            res_dir_un.downcast_ref::<Matrix2x2>(),
            Some(&Matrix2x2(-1, -2, -3, -4))
        );

        // 7. Error cases & diagnostics
        // Matrix division by singular/zero matrix
        let m_zero = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(0, 0, 0, 0));
        ctx.set_variable("m_zero", m_zero);
        let expr_bad_div = Expression::BinaryOp(
            BinaryOp::Div,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("m_zero".to_string(), empty_span())),
            empty_span(),
        );
        let err_div = Evaluator::new(&ctx).evaluate(&expr_bad_div).err().unwrap();
        assert!(
            err_div.errors()[0]
                .error
                .to_string()
                .contains("matrix division by singular matrix")
        );

        let m_div_d0 = Value::capsule_with_ops("matrix2x2", ops.clone(), Matrix2x2(1, 0, 0, 0));
        ctx.set_variable("m_div_d0", m_div_d0);
        let expr_div_d0 = Expression::BinaryOp(
            BinaryOp::Div,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("m_div_d0".to_string(), empty_span())),
            empty_span(),
        );
        let err_div_d0 = Evaluator::new(&ctx).evaluate(&expr_div_d0).err().unwrap();
        assert!(
            err_div_d0.errors()[0]
                .error
                .to_string()
                .contains("matrix division by singular matrix")
        );

        // Matrix modulo by zero
        let expr_bad_mod = Expression::BinaryOp(
            BinaryOp::Mod,
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            Box::new(Expression::Variable("m_zero".to_string(), empty_span())),
            empty_span(),
        );
        let err_mod = Evaluator::new(&ctx).evaluate(&expr_bad_mod).err().unwrap();
        assert!(
            err_mod.errors()[0]
                .error
                .to_string()
                .contains("matrix modulo by zero")
        );

        // Bad attribute
        let expr_bad_attr = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
                operators: vec![TraversalOperator::GetAttr(
                    "unknown_field".to_string(),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let err_attr = Evaluator::new(&ctx).evaluate(&expr_bad_attr).err().unwrap();
        assert!(
            err_attr.errors()[0]
                .error
                .to_string()
                .contains("unknown matrix field 'unknown_field'")
        );

        // Index out of bounds
        let expr_bad_idx = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("m1".to_string(), empty_span())),
                operators: vec![TraversalOperator::Index(
                    Expression::Number(99.into(), empty_span()),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let err_idx = Evaluator::new(&ctx).evaluate(&expr_bad_idx).err().unwrap();
        assert!(
            err_idx.errors()[0]
                .error
                .to_string()
                .contains("matrix index out of bounds")
        );

        // Unsupported hooks on basic capsule
        let basic_ops = Arc::new(CapsuleOps::new(
            "bare_capsule",
            Arc::new(|_, _| false),
            Arc::new(|_| 0),
        ));
        let bare = Value::capsule_with_ops("bare_capsule", basic_ops, 42_i32);
        ctx.set_variable("bare", bare);

        // Bare negation
        let expr_bare_neg = Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Variable("bare".to_string(), empty_span())),
            empty_span(),
        );
        let err_bare_neg = Evaluator::new(&ctx).evaluate(&expr_bare_neg).err().unwrap();
        assert!(
            err_bare_neg.errors()[0]
                .error
                .to_string()
                .contains("does not support unary negation")
        );

        // Bare attr get
        let expr_bare_attr = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("bare".to_string(), empty_span())),
                operators: vec![TraversalOperator::GetAttr(
                    "field".to_string(),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let err_bare_attr = Evaluator::new(&ctx)
            .evaluate(&expr_bare_attr)
            .err()
            .unwrap();
        assert!(
            err_bare_attr.errors()[0]
                .error
                .to_string()
                .contains("does not support attribute access")
        );

        // Bare index get
        let expr_bare_idx = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("bare".to_string(), empty_span())),
                operators: vec![TraversalOperator::Index(
                    Expression::Number(0.into(), empty_span()),
                    empty_span(),
                )],
            }),
            empty_span(),
        );
        let err_bare_idx = Evaluator::new(&ctx).evaluate(&expr_bare_idx).err().unwrap();
        assert!(
            err_bare_idx.errors()[0]
                .error
                .to_string()
                .contains("does not support indexing")
        );

        // Bare legacy index get
        let expr_bare_legacy = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("bare".to_string(), empty_span())),
                operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
            }),
            empty_span(),
        );
        let err_bare_legacy = Evaluator::new(&ctx)
            .evaluate(&expr_bare_legacy)
            .err()
            .unwrap();
        assert!(
            err_bare_legacy.errors()[0]
                .error
                .to_string()
                .contains("does not support indexing")
        );

        // 8. Unknown capsule propagation
        let unk_cap = Value::unknown(m1_val.ty().clone());
        ctx.set_variable("unk_cap", unk_cap);

        let expr_unk_add = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("unk_cap".to_string(), empty_span())),
            Box::new(Expression::Variable("m1".to_string(), empty_span())),
            empty_span(),
        );
        let (res_unk_add, _) = Evaluator::new(&ctx).evaluate(&expr_unk_add).unwrap();
        assert!(res_unk_add.is_unknown());
        assert_eq!(res_unk_add.ty(), m1_val.ty());

        let expr_unk_neg = Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Variable("unk_cap".to_string(), empty_span())),
            empty_span(),
        );
        let (res_unk_neg, _) = Evaluator::new(&ctx).evaluate(&expr_unk_neg).unwrap();
        assert!(res_unk_neg.is_unknown());
        assert_eq!(res_unk_neg.ty(), m1_val.ty());

        // 9. Mark propagation
        let m1_marked = m1_val.mark(crate::types::val::ValueMark::Sensitive);
        ctx.set_variable("m1_marked", m1_marked);
        let expr_marked_add = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("m1_marked".to_string(), empty_span())),
            Box::new(Expression::Variable("m2".to_string(), empty_span())),
            empty_span(),
        );
        let (res_marked_add, _) = Evaluator::new(&ctx).evaluate(&expr_marked_add).unwrap();
        assert!(res_marked_add.has_mark(&crate::types::val::ValueMark::Sensitive));

        // 10. Direct closure error invocations (when downcast_ref fails on invalid payload)
        let invalid_any: Arc<dyn std::any::Any + Send + Sync> = Arc::new(12345_u32);
        let valid_any: Arc<dyn std::any::Any + Send + Sync> = Arc::new(Matrix2x2(1, 2, 3, 4));

        let add_fn = ops.add.as_ref().unwrap();
        assert!(add_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
        assert!(add_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());

        let sub_fn = ops.sub.as_ref().unwrap();
        assert!(sub_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
        assert!(sub_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());

        let mul_fn = ops.mul.as_ref().unwrap();
        assert!(mul_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
        assert!(mul_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());

        let div_fn = ops.div.as_ref().unwrap();
        assert!(div_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
        assert!(div_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());

        let mod_fn = ops.modulo.as_ref().unwrap();
        assert!(mod_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
        assert!(mod_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());

        let neg_fn = ops.neg.as_ref().unwrap();
        assert!(neg_fn(invalid_any.as_ref()).is_err());

        let cmp_fn = ops.cmp.as_ref().unwrap();
        assert!(cmp_fn(invalid_any.as_ref(), valid_any.as_ref()).is_err());
        assert!(cmp_fn(valid_any.as_ref(), invalid_any.as_ref()).is_err());

        let attr_fn = ops.attr_get.as_ref().unwrap();
        assert!(attr_fn(invalid_any.as_ref(), "a").is_err());

        let idx_fn = ops.index_get.as_ref().unwrap();
        assert!(
            idx_fn(
                invalid_any.as_ref(),
                &Value::new(Type::Number, ValueData::Number(0.into()))
            )
            .is_err()
        );
    }

    #[test]
    fn test_capsule_operator_error_branches_and_edge_cases() {
        use crate::types::ty::CapsuleOps;
        use std::sync::Arc;

        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        struct FailableCapsule(i32);

        let ops = Arc::new(
            CapsuleOps::new(
                "failable_capsule",
                Arc::new(|a, b| {
                    if let (Some(x), Some(y)) = (
                        a.downcast_ref::<FailableCapsule>(),
                        b.downcast_ref::<FailableCapsule>(),
                    ) {
                        x == y
                    } else {
                        false
                    }
                }),
                Arc::new(|_| 0),
            )
            .with_add(Arc::new(|a, b| {
                let x = a.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
                let y = b.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
                if x == 99 {
                    return Err("add failure triggered".to_string());
                }
                if y == 99 {
                    return Err("add failure triggered".to_string());
                }
                Ok(Box::new(FailableCapsule(x + y)))
            }))
            .with_cmp(Arc::new(|a, b| {
                let x = a.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
                let y = b.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
                if x == 99 {
                    return Err("cmp failure triggered".to_string());
                }
                if y == 99 {
                    return Err("cmp failure triggered".to_string());
                }
                Ok(x.cmp(&y))
            }))
            .with_neg(Arc::new(|a| {
                let x = a.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
                if x == 99 {
                    return Err("neg failure triggered".to_string());
                }
                Ok(Box::new(FailableCapsule(-x)))
            }))
            .with_index_get(Arc::new(|a, _idx| {
                let x = a.downcast_ref::<FailableCapsule>().map_or(0, |c| c.0);
                if x == 99 {
                    return Err("index_get failure triggered".to_string());
                }
                Ok(Value::new(Type::Number, ValueData::Number(x.into())))
            })),
        );

        let ok_val = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(1));
        let bad_val = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(99));
        let unk_val = Value::unknown(ok_val.ty().clone());

        let mut ctx = Context::new();
        ctx.set_variable("ok", ok_val.clone());
        ctx.set_variable("bad", bad_val.clone());
        ctx.set_variable("unk", unk_val.clone());
        ctx.set_variable("unk_num", Value::unknown(Type::Number));
        ctx.set_variable("num", Value::new(Type::Number, ValueData::Number(5.into())));

        // 1. Add error
        let expr_bad_add = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("bad".to_string(), empty_span())),
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            empty_span(),
        );
        let err_add = Evaluator::new(&ctx).evaluate(&expr_bad_add).err().unwrap();
        assert!(
            err_add.errors()[0]
                .error
                .to_string()
                .contains("add failure triggered")
        );

        // 2. Cmp error
        let expr_bad_cmp = Expression::BinaryOp(
            BinaryOp::Less,
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            Box::new(Expression::Variable("bad".to_string(), empty_span())),
            empty_span(),
        );
        let err_cmp = Evaluator::new(&ctx).evaluate(&expr_bad_cmp).err().unwrap();
        assert!(
            err_cmp.errors()[0]
                .error
                .to_string()
                .contains("cmp failure triggered")
        );

        let expr_bad_cmp_left = Expression::BinaryOp(
            BinaryOp::Less,
            Box::new(Expression::Variable("bad".to_string(), empty_span())),
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            empty_span(),
        );
        let err_cmp_l = Evaluator::new(&ctx)
            .evaluate(&expr_bad_cmp_left)
            .err()
            .unwrap();
        assert!(
            err_cmp_l.errors()[0]
                .error
                .to_string()
                .contains("cmp failure triggered")
        );

        // 3. Neg error
        let expr_bad_neg = Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Variable("bad".to_string(), empty_span())),
            empty_span(),
        );
        let err_neg = Evaluator::new(&ctx).evaluate(&expr_bad_neg).err().unwrap();
        assert!(
            err_neg.errors()[0]
                .error
                .to_string()
                .contains("neg failure triggered")
        );

        // 4. Legacy index error
        let expr_bad_legacy = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("bad".to_string(), empty_span())),
                operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
            }),
            empty_span(),
        );
        let err_legacy = Evaluator::new(&ctx)
            .evaluate(&expr_bad_legacy)
            .err()
            .unwrap();
        assert!(
            err_legacy.errors()[0]
                .error
                .to_string()
                .contains("index_get failure triggered")
        );

        // 5. Right-hand side unknown capsule
        let expr_rhs_unk_add = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (res_rhs_unk_add, _) = Evaluator::new(&ctx).evaluate(&expr_rhs_unk_add).unwrap();
        assert!(res_rhs_unk_add.is_unknown());
        assert_eq!(res_rhs_unk_add.ty(), ok_val.ty());

        let expr_rhs_unk_sub = Expression::BinaryOp(
            BinaryOp::Sub,
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            empty_span(),
        );
        let (res_rhs_unk_sub, _) = Evaluator::new(&ctx).evaluate(&expr_rhs_unk_sub).unwrap();
        assert!(res_rhs_unk_sub.is_unknown());
        assert_eq!(res_rhs_unk_sub.ty(), ok_val.ty());

        let expr_left_unk_sub = Expression::BinaryOp(
            BinaryOp::Sub,
            Box::new(Expression::Variable("unk".to_string(), empty_span())),
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            empty_span(),
        );
        let (res_left_unk_sub, _) = Evaluator::new(&ctx).evaluate(&expr_left_unk_sub).unwrap();
        assert!(res_left_unk_sub.is_unknown());
        assert_eq!(res_left_unk_sub.ty(), ok_val.ty());

        // Unknown number on left with capsule on right
        let expr_unk_num_add = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("unk_num".to_string(), empty_span())),
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            empty_span(),
        );
        let (res_unk_num_add, _) = Evaluator::new(&ctx).evaluate(&expr_unk_num_add).unwrap();
        assert!(res_unk_num_add.is_unknown());
        assert_eq!(res_unk_num_add.ty(), ok_val.ty());

        let expr_unk_num_sub = Expression::BinaryOp(
            BinaryOp::Sub,
            Box::new(Expression::Variable("unk_num".to_string(), empty_span())),
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            empty_span(),
        );
        let (res_unk_num_sub, _) = Evaluator::new(&ctx).evaluate(&expr_unk_num_sub).unwrap();
        assert!(res_unk_num_sub.is_unknown());
        assert_eq!(res_unk_num_sub.ty(), ok_val.ty());

        // 6. Right-hand side capsule with non-capsule on left: num + ok
        let expr_rhs_cap = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("num".to_string(), empty_span())),
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            empty_span(),
        );
        let (res_rhs_cap, _) = Evaluator::new(&ctx).evaluate(&expr_rhs_cap).unwrap();
        assert_eq!(
            res_rhs_cap.downcast_ref::<FailableCapsule>(),
            Some(&FailableCapsule(1))
        );

        // Capsule equality and downcast failure
        let val1 = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(10));
        let val2 = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(20));
        assert_ne!(val1.unmark().0, val2.unmark().0);
        assert_eq!(val1.unmark().0, val1.unmark().0);
        assert_ne!(
            val1.unmark().0,
            Value::new(Type::Number, ValueData::Number(10.into()))
                .unmark()
                .0
        );

        let other_capsule = Value::capsule_with_ops(
            "other_capsule",
            Arc::new(CapsuleOps::new(
                "other_capsule",
                Arc::new(|_, _| false),
                Arc::new(|_| 0),
            )),
            999_i32,
        );
        ctx.set_variable("other_cap", other_capsule.clone());
        let expr_cap_other = Expression::BinaryOp(
            BinaryOp::Eq,
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            Box::new(Expression::Variable("other_cap".to_string(), empty_span())),
            empty_span(),
        );
        let (res_co, _) = Evaluator::new(&ctx).evaluate(&expr_cap_other).unwrap();
        assert_eq!(*res_co.data, ValueData::Bool(false));

        let corrupt_failable = Value::new(val1.ty().clone(), ValueData::Capsule(Arc::new(999_i32)));
        assert_ne!(val1.unmark().0, corrupt_failable.unmark().0);

        // Add with bad on right (tests y == 99)
        let expr_add_right_bad = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            Box::new(Expression::Variable("bad".to_string(), empty_span())),
            empty_span(),
        );
        let err_add_r = Evaluator::new(&ctx)
            .evaluate(&expr_add_right_bad)
            .err()
            .unwrap();
        assert!(
            err_add_r.errors()[0]
                .error
                .to_string()
                .contains("add failure triggered")
        );

        // Add success
        let expr_add_ok = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            empty_span(),
        );
        let (res_add_ok, _) = Evaluator::new(&ctx).evaluate(&expr_add_ok).unwrap();
        assert_eq!(
            res_add_ok.downcast_ref::<FailableCapsule>(),
            Some(&FailableCapsule(2))
        );

        // Comparison successes
        for (cmp_op, expected) in [
            (BinaryOp::Less, false),
            (BinaryOp::LessEq, true),
            (BinaryOp::Greater, false),
            (BinaryOp::GreaterEq, true),
        ] {
            let expr_cmp_ok = Expression::BinaryOp(
                cmp_op,
                Box::new(Expression::Variable("ok".to_string(), empty_span())),
                Box::new(Expression::Variable("ok".to_string(), empty_span())),
                empty_span(),
            );
            let (res_cmp_ok, _) = Evaluator::new(&ctx).evaluate(&expr_cmp_ok).unwrap();
            assert_eq!(*res_cmp_ok.data, ValueData::Bool(expected));
        }

        // Negation success
        let expr_neg_ok = Expression::UnaryOp(
            UnaryOp::Neg,
            Box::new(Expression::Variable("ok".to_string(), empty_span())),
            empty_span(),
        );
        let (res_neg_ok, _) = Evaluator::new(&ctx).evaluate(&expr_neg_ok).unwrap();
        assert_eq!(
            res_neg_ok.downcast_ref::<FailableCapsule>(),
            Some(&FailableCapsule(-1))
        );

        // Index success
        let expr_idx_ok = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("ok".to_string(), empty_span())),
                operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
            }),
            empty_span(),
        );
        let (res_idx_ok, _) = Evaluator::new(&ctx).evaluate(&expr_idx_ok).unwrap();
        assert_eq!(*res_idx_ok.data, ValueData::Number(1.into()));

        let expr_bad_idx = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Variable("bad".to_string(), empty_span())),
                operators: vec![TraversalOperator::LegacyIndex(0, empty_span())],
            }),
            empty_span(),
        );
        let err_bad_idx = Evaluator::new(&ctx).evaluate(&expr_bad_idx).err().unwrap();
        assert!(
            err_bad_idx.errors()[0]
                .error
                .to_string()
                .contains("index_get failure triggered")
        );

        // All arithmetic ops with non-capsule on left and capsule with all ops on right
        let all_ops = Arc::new(
            CapsuleOps::new("full_capsule", Arc::new(|_, _| false), Arc::new(|_| 0))
                .with_add(Arc::new(|_, _| Ok(Box::new(100_i32))))
                .with_sub(Arc::new(|_, _| Ok(Box::new(101_i32))))
                .with_mul(Arc::new(|_, _| Ok(Box::new(102_i32))))
                .with_div(Arc::new(|_, _| Ok(Box::new(103_i32))))
                .with_modulo(Arc::new(|_, _| Ok(Box::new(104_i32))))
                .with_cmp(Arc::new(|_, _| Ok(std::cmp::Ordering::Equal))),
        );
        let full_val = Value::capsule_with_ops("full_capsule", all_ops, 1_i32);
        ctx.set_variable("full", full_val.clone());

        for (bin_op, exp_val) in [
            (BinaryOp::Add, 100_i32),
            (BinaryOp::Sub, 101_i32),
            (BinaryOp::Mul, 102_i32),
            (BinaryOp::Div, 103_i32),
            (BinaryOp::Mod, 104_i32),
        ] {
            let expr_mixed = Expression::BinaryOp(
                bin_op,
                Box::new(Expression::Variable("num".to_string(), empty_span())),
                Box::new(Expression::Variable("full".to_string(), empty_span())),
                empty_span(),
            );
            let (res, _) = Evaluator::new(&ctx).evaluate(&expr_mixed).unwrap();
            assert_eq!(res.downcast_ref::<i32>(), Some(&exp_val));
        }

        // Comparison with non-capsule on left and capsule on right
        let expr_cmp_left_non_cap = Expression::BinaryOp(
            BinaryOp::LessEq,
            Box::new(Expression::Variable("num".to_string(), empty_span())),
            Box::new(Expression::Variable("full".to_string(), empty_span())),
            empty_span(),
        );
        let (res_cmp_left, _) = Evaluator::new(&ctx)
            .evaluate(&expr_cmp_left_non_cap)
            .unwrap();
        assert_eq!(*res_cmp_left.data, ValueData::Bool(true));

        // Comparison with capsule on left and non-capsule on right
        let expr_cmp_right_non_cap = Expression::BinaryOp(
            BinaryOp::GreaterEq,
            Box::new(Expression::Variable("full".to_string(), empty_span())),
            Box::new(Expression::Variable("num".to_string(), empty_span())),
            empty_span(),
        );
        let (res_cmp_right, _) = Evaluator::new(&ctx)
            .evaluate(&expr_cmp_right_non_cap)
            .unwrap();
        assert_eq!(*res_cmp_right.data, ValueData::Bool(true));

        // Arithmetic with capsule on left and non-capsule on right
        let expr_arith_rnc = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("full".to_string(), empty_span())),
            Box::new(Expression::Variable("num".to_string(), empty_span())),
            empty_span(),
        );
        let (res_arnc, _) = Evaluator::new(&ctx).evaluate(&expr_arith_rnc).unwrap();
        assert_eq!(res_arnc.downcast_ref::<i32>(), Some(&100));

        // 7. Value::cmp ordering check with ops.cmp
        let val1 = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(10));
        let val2 = Value::capsule_with_ops("failable_capsule", ops.clone(), FailableCapsule(20));
        assert_eq!(val1.cmp(&val2), std::cmp::Ordering::Less);
        assert_eq!(val2.cmp(&val1), std::cmp::Ordering::Greater);
        assert_eq!(val1.cmp(&val1), std::cmp::Ordering::Equal);
    }

    /// Tests edge-case branch conditions in Evaluator including variable naming prefixes,
    /// typos, string arithmetic coercion, logical OR, and function signature validation.
    #[test]
    fn test_evaluator_remaining_branch_coverage() {
        use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
        use std::sync::Arc;

        let mut ctx = Context::with_stdlib();

        // 1. Variable starting with var. and local.
        ctx.set_variable(
            "var.x",
            Value::new(Type::String, ValueData::String("val1".to_string())),
        );
        let (val_var, _) = Evaluator::new(&ctx)
            .evaluate(&Expression::Variable("var.x".to_string(), empty_span()))
            .unwrap();
        assert_eq!(
            val_var,
            Value::new(Type::String, ValueData::String("val1".to_string()))
        );

        ctx.set_variable(
            "local.y",
            Value::new(Type::String, ValueData::String("val2".to_string())),
        );
        let (val_loc, _) = Evaluator::new(&ctx)
            .evaluate(&Expression::Variable("local.y".to_string(), empty_span()))
            .unwrap();
        assert_eq!(
            val_loc,
            Value::new(Type::String, ValueData::String("val2".to_string()))
        );

        // 2. Variable typo suggestion
        ctx.set_variable(
            "var.configuration_option",
            Value::new(Type::String, ValueData::String("cfg".to_string())),
        );
        let diags_typo = Evaluator::new(&ctx)
            .evaluate(&Expression::Variable(
                "var.configuraton_option".to_string(),
                empty_span(),
            ))
            .err()
            .unwrap();
        assert!(
            diags_typo.errors()[0]
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("Did you mean var.configuration_option?")
        );

        // 3. Function typo suggestion
        ctx.set_function(
            "target_function",
            Function::new(
                "target_function",
                Arc::new(|_| Ok(Value::new(Type::Number, ValueData::Number(1.into())))),
            ),
        );
        let expr_typo_func = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "target_functon".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        let diags_func = Evaluator::new(&ctx)
            .evaluate(&expr_typo_func)
            .err()
            .unwrap();
        assert!(
            diags_func.errors()[0]
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("target_function")
        );

        let expr_valid_func = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "target_function".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        let (res_valid_fn, _) = Evaluator::new(&ctx).evaluate(&expr_valid_func).unwrap();
        assert_eq!(*res_valid_fn.data, ValueData::Number(1.into()));

        // 4. String addition where left can coerce to number ("123") but right cannot ("abc")
        let expr_str_num_add = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::String("123".to_string(), empty_span())),
            Box::new(Expression::String("abc".to_string(), empty_span())),
            empty_span(),
        );
        let (res_str_add, _) = Evaluator::new(&ctx).evaluate(&expr_str_num_add).unwrap();
        assert_eq!(
            res_str_add,
            Value::new(Type::String, ValueData::String("123abc".to_string()))
        );

        // 5. Logical OR where left is false and right is true
        let expr_or = Expression::BinaryOp(
            BinaryOp::Or,
            Box::new(Expression::Bool(false, empty_span())),
            Box::new(Expression::Bool(true, empty_span())),
            empty_span(),
        );
        let (res_or, _) = Evaluator::new(&ctx).evaluate(&expr_or).unwrap();
        assert_eq!(*res_or.data, ValueData::Bool(true));

        // 6. Function with signature and variadic param where args.len() < expected_fixed
        let sig_var = FunctionSignature::with_static_return_type(
            vec![
                FunctionParamSpec::new("p1", Type::String),
                FunctionParamSpec::new("p2", Type::String),
            ],
            Type::String,
        )
        .with_variadic(FunctionParamSpec::new("rest", Type::String));
        let fn_var = Function::new(
            "fn_var",
            Arc::new(|_| {
                Ok(Value::new(
                    Type::String,
                    ValueData::String("ok".to_string()),
                ))
            }),
        )
        .with_signature(sig_var);
        let mut ctx_fn = Context::new();
        ctx_fn.set_function("fn_var", fn_var);
        let expr_too_few = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "fn_var".into(),
                args: vec![Expression::String("only_one".to_string(), empty_span())],
                expand_final: false,
            }),
            empty_span(),
        );
        assert!(Evaluator::new(&ctx_fn).evaluate(&expr_too_few).is_err());

        let expr_valid_var = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "fn_var".into(),
                args: vec![
                    Expression::String("a".to_string(), empty_span()),
                    Expression::String("b".to_string(), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let (res_fn_var, _) = Evaluator::new(&ctx_fn).evaluate(&expr_valid_var).unwrap();
        assert_eq!(*res_fn_var.data, ValueData::String("ok".to_string()));

        // 7. Template with unknown interpolation
        ctx.set_variable("unk_str", Value::unknown(Type::String));
        let expr_template_unk = Expression::Template(
            vec![
                crate::ast::expr::TemplatePart::Literal("prefix_".to_string(), empty_span()),
                crate::ast::expr::TemplatePart::Interpolation(
                    Expression::Variable("unk_str".to_string(), empty_span()),
                    empty_span(),
                ),
                crate::ast::expr::TemplatePart::Literal("_suffix".to_string(), empty_span()),
            ],
            empty_span(),
        );
        let (res_tpl_unk, _) = Evaluator::new(&ctx).evaluate(&expr_template_unk).unwrap();
        assert!(res_tpl_unk.is_unknown());

        // 8. Built-in special functions try, can, nonsensitive, issensitive
        let expr_try_empty = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "try".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_try_empty).is_err());

        let expr_can_two = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "can".into(),
                args: vec![
                    Expression::Bool(true, empty_span()),
                    Expression::Bool(false, empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_can_two).is_err());

        let expr_can_unk = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "can".into(),
                args: vec![Expression::Variable("unk_str".to_string(), empty_span())],
                expand_final: false,
            }),
            empty_span(),
        );
        let (res_can_unk, _) = Evaluator::new(&ctx).evaluate(&expr_can_unk).unwrap();
        assert!(res_can_unk.is_unknown());

        let expr_nonsens = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "nonsensitive".into(),
                args: vec![Expression::String("secret".to_string(), empty_span())],
                expand_final: false,
            }),
            empty_span(),
        );
        let (res_nonsens, _) = Evaluator::new(&ctx).evaluate(&expr_nonsens).unwrap();
        assert_eq!(*res_nonsens.data, ValueData::String("secret".to_string()));

        let expr_issens = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "issensitive".into(),
                args: vec![Expression::String("secret".to_string(), empty_span())],
                expand_final: false,
            }),
            empty_span(),
        );
        let (res_issens, _) = Evaluator::new(&ctx).evaluate(&expr_issens).unwrap();
        assert_eq!(*res_issens.data, ValueData::Bool(false));

        // 9. expand_final edge cases
        let expr_exp_empty = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "min".into(),
                args: vec![],
                expand_final: true,
            }),
            empty_span(),
        );
        let (res_exp_empty, _) = Evaluator::new(&ctx).evaluate(&expr_exp_empty).unwrap();
        assert!(res_exp_empty.is_unknown());

        let expr_exp_unk = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "min".into(),
                args: vec![Expression::Variable("unk_str".to_string(), empty_span())],
                expand_final: true,
            }),
            empty_span(),
        );
        let (res_exp_unk, _) = Evaluator::new(&ctx).evaluate(&expr_exp_unk).unwrap();
        assert!(res_exp_unk.is_unknown());

        // 10. Div and Mod by zero
        let expr_div_zero = Expression::BinaryOp(
            BinaryOp::Div,
            Box::new(Expression::Number(1.into(), empty_span())),
            Box::new(Expression::Number(0.into(), empty_span())),
            empty_span(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_div_zero).is_err());

        let expr_mod_zero = Expression::BinaryOp(
            BinaryOp::Mod,
            Box::new(Expression::Number(1.into(), empty_span())),
            Box::new(Expression::Number(0.into(), empty_span())),
            empty_span(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_mod_zero).is_err());

        // 11. push_diagnostic with pre-existing callouts
        let mut eval_diag = Evaluator::new(&ctx);
        let mut pre_diag = Diagnostic::error("existing callouts", "", empty_span());
        pre_diag
            .eval_callouts
            .push(crate::diagnostic::EvalCallout::new(empty_span(), "p", "v"));
        eval_diag.push_diagnostic(pre_diag);

        // 12. Traversal with empty operators
        let expr_empty_trav = Expression::Traversal(
            Box::new(crate::ast::expr::Traversal {
                expr: Box::new(Expression::Number(1.into(), empty_span())),
                operators: vec![],
            }),
            empty_span(),
        );
        let (res_et, _) = Evaluator::new(&ctx).evaluate(&expr_empty_trav).unwrap();
        assert_eq!(*res_et.data, ValueData::Number(1.into()));

        // 13. Functions with allow_null and Dynamic parameters
        let sig_null = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("p", Type::String).with_allow_null(true)],
            Type::String,
        )
        .with_variadic(FunctionParamSpec::new("rest", Type::String).with_allow_null(true));
        let fn_null = Function::new(
            "fn_null",
            Arc::new(|_| {
                Ok(Value::new(
                    Type::String,
                    ValueData::String("ok".to_string()),
                ))
            }),
        )
        .with_signature(sig_null);
        let mut ctx_null = Context::new();
        ctx_null.set_function("fn_null", fn_null);
        let expr_call_null = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "fn_null".into(),
                args: vec![
                    Expression::Null(empty_span()),
                    Expression::Null(empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        let (res_null, _) = Evaluator::new(&ctx_null).evaluate(&expr_call_null).unwrap();
        assert_eq!(*res_null.data, ValueData::String("ok".to_string()));

        let sig_dyn = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("p", Type::Dynamic)],
            Type::String,
        )
        .with_variadic(FunctionParamSpec::new("rest", Type::Dynamic));
        let fn_dyn = Function::new(
            "fn_dyn",
            Arc::new(|_| {
                Ok(Value::new(
                    Type::String,
                    ValueData::String("ok".to_string()),
                ))
            }),
        )
        .with_signature(sig_dyn);
        ctx_null.set_function("fn_dyn", fn_dyn);
        let expr_call_dyn = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "fn_dyn".into(),
                args: vec![
                    Expression::Variable("dyn_arg".to_string(), empty_span()),
                    Expression::Variable("dyn_arg".to_string(), empty_span()),
                ],
                expand_final: false,
            }),
            empty_span(),
        );
        ctx_null.set_variable("dyn_arg", Value::unknown(Type::Dynamic));
        let (res_dyn, _) = Evaluator::new(&ctx_null).evaluate(&expr_call_dyn).unwrap();
        assert_eq!(*res_dyn.data, ValueData::String("ok".to_string()));

        // 14. Dynamic return type calculation branches
        let sig_calc = FunctionSignature::with_static_return_type(vec![], Type::Dynamic)
            .with_value_return_type(Arc::new(|_| Ok(Type::Dynamic)));
        let fn_calc = Function::new(
            "fn_calc",
            Arc::new(|_| Ok(Value::new(Type::Number, ValueData::Number(10.into())))),
        )
        .with_signature(sig_calc);
        ctx_null.set_function("fn_calc", fn_calc);
        let expr_calc = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "fn_calc".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        let (res_calc, _) = Evaluator::new(&ctx_null).evaluate(&expr_calc).unwrap();
        assert_eq!(*res_calc.data, ValueData::Number(10.into()));

        let sig_calc_err = FunctionSignature::with_static_return_type(vec![], Type::Dynamic)
            .with_value_return_type(Arc::new(|_| Err("failed".to_string())));
        let fn_calc_err = Function::new(
            "fn_calc_err",
            Arc::new(|_| Ok(Value::new(Type::Number, ValueData::Number(10.into())))),
        )
        .with_signature(sig_calc_err);
        ctx_null.set_function("fn_calc_err", fn_calc_err);
        let expr_calc_err = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "fn_calc_err".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        let (res_calc_err, _) = Evaluator::new(&ctx_null).evaluate(&expr_calc_err).unwrap();
        assert_eq!(*res_calc_err.data, ValueData::Number(10.into()));

        // 15. Non-string on right with string on left in Add
        let expr_str_num_bad = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::String("hello".to_string(), empty_span())),
            Box::new(Expression::Number(123.into(), empty_span())),
            empty_span(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_str_num_bad).is_err());
    }

    #[test]
    fn test_evaluator_missing_branch_coverage() {
        use crate::ast::expr::Traversal;
        use crate::eval::func::{Function, FunctionParamSpec, FunctionSignature};
        use std::sync::Arc;

        let mut ctx = Context::new();

        // 1. Line 576: unknown string + unknown bool in Add
        let ref_alpha = crate::types::refinement::Refinement::new().with_prefix("abc");
        let unk_str_alpha = Value::unknown_refined(Type::String, ref_alpha);
        ctx.set_variable("unk_str", unk_str_alpha);
        ctx.set_variable("unk_bool", Value::unknown(Type::Bool));
        let expr_add = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("unk_str".into(), empty_span())),
            Box::new(Expression::Variable("unk_bool".into(), empty_span())),
            empty_span(),
        );
        let eval_res = Evaluator::new(&ctx).evaluate(&expr_add);
        assert!(eval_res.is_ok());

        // 2. Line 1277: while loop termination on null.*.foo
        ctx.set_variable("null_var", Value::null(Type::Dynamic));
        let splat_trav = Traversal {
            expr: Box::new(Expression::Variable("null_var".into(), empty_span())),
            operators: vec![
                TraversalOperator::FullSplat(empty_span()),
                TraversalOperator::GetAttr("foo".into(), empty_span()),
            ],
        };
        let expr_trav = Expression::Traversal(Box::new(splat_trav), empty_span());
        let splat_res = Evaluator::new(&ctx).evaluate(&expr_trav);
        assert!(splat_res.is_ok());

        // 3. Line 362: allow_null is true and arg is null
        let sig_allow_null = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("arg", Type::Dynamic).with_allow_null(true)],
            Type::Dynamic,
        );
        let fn_allow_null = Function::new(
            "fn_allow_null",
            Arc::new(|_| Ok(Value::new(Type::Number, ValueData::Number(100.into())))),
        )
        .with_signature(sig_allow_null);
        ctx.set_function("fn_allow_null", fn_allow_null);
        ctx.set_variable("val_null", Value::null(Type::Dynamic));
        let expr_null_arg = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "fn_allow_null".into(),
                args: vec![Expression::Variable("val_null".into(), empty_span())],
                expand_final: false,
            }),
            empty_span(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_null_arg).is_ok());

        // 4. Line 371: param_type is not Dynamic, and arg is Dynamic
        let sig_typed_param = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("str_arg", Type::String)],
            Type::Dynamic,
        );
        let fn_typed_param = Function::new(
            "fn_typed_param",
            Arc::new(|_| Ok(Value::new(Type::String, ValueData::String("ok".into())))),
        )
        .with_signature(sig_typed_param);
        ctx.set_function("fn_typed_param", fn_typed_param);
        ctx.set_variable("dyn_arg", Value::unknown(Type::Dynamic));
        let expr_dyn_arg = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "fn_typed_param".into(),
                args: vec![Expression::Variable("dyn_arg".into(), empty_span())],
                expand_final: false,
            }),
            empty_span(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_dyn_arg).is_ok());

        // 5. Line 395: v.ty() == expected_ty (v.ty() != expected_ty is false)
        let sig_match_ret = FunctionSignature::with_static_return_type(vec![], Type::Dynamic)
            .with_value_return_type(Arc::new(|_| Ok(Type::Number)));
        let fn_match_ret = Function::new(
            "fn_match_ret",
            Arc::new(|_| Ok(Value::new(Type::Number, ValueData::Number(42.into())))),
        )
        .with_signature(sig_match_ret);
        ctx.set_function("fn_match_ret", fn_match_ret);
        let expr_match_ret = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "fn_match_ret".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_match_ret).is_ok());

        // 6. Line 397: v cannot be coerced to expected_ty
        let sig_uncoercible_ret = FunctionSignature::with_static_return_type(vec![], Type::Dynamic)
            .with_value_return_type(Arc::new(|_| Ok(Type::Number)));
        let fn_uncoercible_ret = Function::new(
            "fn_uncoercible_ret",
            Arc::new(|_| {
                Ok(Value::new(
                    Type::List(Box::new(Type::Number)),
                    ValueData::Array(vec![Value::new(Type::Number, ValueData::Number(1.into()))]),
                ))
            }),
        )
        .with_signature(sig_uncoercible_ret);
        ctx.set_function("fn_uncoercible_ret", fn_uncoercible_ret);
        let expr_uncoercible_ret = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: "fn_uncoercible_ret".into(),
                args: vec![],
                expand_final: false,
            }),
            empty_span(),
        );
        assert!(Evaluator::new(&ctx).evaluate(&expr_uncoercible_ret).is_ok());
    }
}
