//! Ahead-of-time (AOT) static type checking engine.
//!
//! Validates AST structures against declarative [`BodySchema`] specifications
//! and [`ScopeSchema`]s without evaluating runtime variable values.
use crate::ast::expr::{
    BinaryOp, Directive, Expression, ForExpr, FuncCall, TemplatePart, Traversal, TraversalOperator,
    UnaryOp,
};
use crate::ast::schema::BodySchema;
use crate::ast::structure::{Body, DynamicBlock};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::error::HclError;
use crate::eval::func::FunctionSignature;
use crate::span::Span;
use crate::types::Type;
use bigdecimal::ToPrimitive;
use std::collections::{BTreeMap, BTreeSet, HashMap};
/// Determines whether an actual type can be converted or coerced to an expected type.
///
/// Returns `true` if `actual` is identical to `expected`, if either is [`Type::Dynamic`],
/// or if standard HCL / `cty` type coercion rules allow converting `actual` to `expected`.
///
/// # Arguments
/// * `actual` - The actual type to test.
/// * `expected` - The target type required.
///
/// # Returns
/// `true` if `actual` is compatible with `expected`, or `false` otherwise.
#[must_use]
pub fn is_type_compatible(actual: &Type, expected: &Type) -> bool {
    if actual == expected || actual == &Type::Dynamic || expected == &Type::Dynamic {
        return true;
    }
    match (actual, expected) {
        (Type::String, Type::Number | Type::Bool) | (Type::Number | Type::Bool, Type::String) => {
            true
        }
        (Type::List(a) | Type::Set(a), Type::List(b) | Type::Set(b))
        | (Type::Map(a), Type::Map(b)) => is_type_compatible(a, b),
        (Type::Tuple(elems), Type::List(target) | Type::Set(target)) => {
            elems.iter().all(|el| is_type_compatible(el, target))
        }
        (Type::Tuple(a_elems), Type::Tuple(b_elems)) => {
            a_elems.len() == b_elems.len()
                && a_elems
                    .iter()
                    .zip(b_elems.iter())
                    .all(|(a, b)| is_type_compatible(a, b))
        }
        (Type::Object { attrs, .. }, Type::Map(target)) => {
            attrs.values().all(|v| is_type_compatible(v, target))
        }
        (
            Type::Object { attrs: a_attrs, .. },
            Type::Object {
                attrs: b_attrs,
                optional_attrs: b_opt,
            },
        ) => {
            for (b_name, b_ty) in b_attrs {
                if let Some(a_ty) = a_attrs.get(b_name) {
                    if !is_type_compatible(a_ty, b_ty) {
                        return false;
                    }
                } else if !b_opt.contains(b_name) {
                    return false;
                }
            }
            true
        }
        _ => crate::types::unify::unify(actual, expected) == Some(expected.clone()),
    }
}
/// Unifies an arbitrary slice of types into their most general common supertype.
///
/// # Arguments
/// * `types` - The sequence of types to unify.
///
/// # Returns
/// The common unified supertype, or [`Type::Dynamic`] if empty or incompatible.
#[must_use]
pub fn unify_all(types: &[Type]) -> Type {
    let Some(first) = types.first() else {
        return Type::Dynamic;
    };
    let mut current = first.clone();
    for item in &types[1..] {
        if let Some(u) = crate::types::unify::unify(&current, item) {
            current = u;
        } else {
            return Type::Dynamic;
        }
    }
    current
}
/// A static scope specification describing in-scope variables and function signatures.
#[derive(Debug, Clone, Default)]
pub struct ScopeSchema {
    /// In-scope variables mapped to their static types.
    pub variables: HashMap<String, Type>,
    /// In-scope function signatures mapped by name.
    pub functions: HashMap<String, FunctionSignature>,
    /// Whether undefined variables are permitted without emitting diagnostics.
    pub allow_undefined_variables: bool,
    /// Whether undefined functions are permitted without emitting diagnostics.
    pub allow_undefined_functions: bool,
}
impl ScopeSchema {
    /// Creates a new, empty `ScopeSchema` with strict checking enabled.
    ///
    /// # Returns
    /// A new empty `ScopeSchema`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Adds a declared variable with its static type to the scope schema.
    ///
    /// # Arguments
    /// * `name` - The variable name.
    /// * `ty` - The static variable type.
    ///
    /// # Returns
    /// The updated `ScopeSchema`.
    #[must_use]
    pub fn with_variable(mut self, name: impl Into<String>, ty: Type) -> Self {
        self.variables.insert(name.into(), ty);
        self
    }
    /// Adds multiple declared variables to the scope schema.
    ///
    /// # Arguments
    /// * `vars` - Iterator of variable name and type pairs.
    ///
    /// # Returns
    /// The updated `ScopeSchema`.
    #[must_use]
    pub fn with_variables(mut self, vars: impl IntoIterator<Item = (String, Type)>) -> Self {
        self.variables.extend(vars);
        self
    }
    /// Adds a declared function signature to the scope schema.
    ///
    /// # Arguments
    /// * `name` - The function name.
    /// * `sig` - The function signature.
    ///
    /// # Returns
    /// The updated `ScopeSchema`.
    #[must_use]
    pub fn with_function(mut self, name: impl Into<String>, sig: FunctionSignature) -> Self {
        self.functions.insert(name.into(), sig);
        self
    }
    /// Sets whether undefined variables are allowed (permissive mode).
    ///
    /// # Arguments
    /// * `allow` - `true` to allow undefined variables, `false` to enforce strictness.
    ///
    /// # Returns
    /// The updated `ScopeSchema`.
    #[must_use]
    pub const fn with_allow_undefined_variables(mut self, allow: bool) -> Self {
        self.allow_undefined_variables = allow;
        self
    }
    /// Sets whether undefined functions are allowed (permissive mode).
    ///
    /// # Arguments
    /// * `allow` - `true` to allow undefined functions, `false` to enforce strictness.
    ///
    /// # Returns
    /// The updated `ScopeSchema`.
    #[must_use]
    pub const fn with_allow_undefined_functions(mut self, allow: bool) -> Self {
        self.allow_undefined_functions = allow;
        self
    }
    /// Retrieves the static type of a variable by name if registered.
    ///
    /// # Arguments
    /// * `name` - The variable name.
    ///
    /// # Returns
    /// An optional reference to the registered variable type.
    #[must_use]
    pub fn get_variable(&self, name: &str) -> Option<&Type> {
        self.variables.get(name)
    }
    /// Retrieves a function signature by name if registered.
    ///
    /// # Arguments
    /// * `name` - The function name.
    ///
    /// # Returns
    /// An optional reference to the registered function signature.
    #[must_use]
    pub fn get_function(&self, name: &str) -> Option<&FunctionSignature> {
        self.functions.get(name)
    }
    /// Creates a child scope inheriting variables and functions from this scope.
    ///
    /// # Returns
    /// A new cloned child `ScopeSchema`.
    #[must_use]
    pub fn child_scope(&self) -> Self {
        self.clone()
    }
}
/// Static ahead-of-time (AOT) type checker engine.
///
/// Validates an AST [`Body`] against a [`BodySchema`] and a [`ScopeSchema`]
/// without requiring runtime evaluation or concrete variable values.
#[derive(Debug, Clone, Default)]
pub struct TypeChecker {
    scope: ScopeSchema,
}
impl TypeChecker {
    /// Creates a new `TypeChecker` with an empty default scope.
    ///
    /// # Returns
    /// A new `TypeChecker`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Configures this `TypeChecker` with a specified [`ScopeSchema`].
    ///
    /// # Arguments
    /// * `scope` - The static scope schema.
    ///
    /// # Returns
    /// The updated `TypeChecker`.
    #[must_use]
    pub fn with_scope(mut self, scope: ScopeSchema) -> Self {
        self.scope = scope;
        self
    }
    /// Returns a reference to the active [`ScopeSchema`].
    ///
    /// # Returns
    /// A reference to the scope schema.
    #[must_use]
    pub const fn scope(&self) -> &ScopeSchema {
        &self.scope
    }
    /// Returns a mutable reference to the active [`ScopeSchema`].
    ///
    /// # Returns
    /// A mutable reference to the scope schema.
    pub fn scope_mut(&mut self) -> &mut ScopeSchema {
        &mut self.scope
    }
    /// Validates an entire AST [`Body`] against an expected [`BodySchema`].
    ///
    /// Checks for unexpected attributes, missing required attributes, block type
    /// mismatches, label counts, attribute type compatibility, and recursively
    /// validates nested and dynamic blocks.
    ///
    /// # Arguments
    /// * `body` - The AST body to validate.
    /// * `schema` - The structural body schema.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if any validation or type checking errors are discovered.
    pub fn check_body(&self, body: &Body, schema: &BodySchema) -> Result<(), Diagnostics> {
        let mut diags = Diagnostics::new();
        self.check_body_with_scope(body, schema, &self.scope, &mut diags);
        if diags.has_errors() {
            Err(diags)
        } else {
            Ok(())
        }
    }
    /// Infers the static type of an expression and returns any accumulated diagnostics.
    ///
    /// # Arguments
    /// * `expr` - The AST expression to analyze.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if static type inference encounters errors.
    ///
    /// # Returns
    /// The inferred [`Type`] of the expression.
    pub fn infer_expression_type(&self, expr: &Expression) -> Result<Type, Diagnostics> {
        let mut diags = Diagnostics::new();
        let ty = self.infer_expression_type_inner(expr, &self.scope, &mut diags);
        if diags.has_errors() {
            Err(diags)
        } else {
            Ok(ty)
        }
    }
    /// Checks an expression against an expected static type.
    ///
    /// # Arguments
    /// * `expr` - The AST expression to check.
    /// * `expected` - The expected static type.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if type inference fails or if the inferred type is incompatible.
    ///
    /// # Returns
    /// The inferred [`Type`] on success.
    pub fn check_expression(
        &self,
        expr: &Expression,
        expected: &Type,
    ) -> Result<Type, Diagnostics> {
        let mut diags = Diagnostics::new();
        let actual = self.infer_expression_type_inner(expr, &self.scope, &mut diags);
        if !is_type_compatible(&actual, expected) {
            let mut diag = Diagnostic::new(
                HclError::Type(format!(
                    "Invalid expression type: expected {expected}, got {actual}"
                )),
                expr.span(),
            );
            diag.summary = Some("Expression type mismatch".to_string());
            diag.detail = Some(format!("Expected type {expected}, but received {actual}"));
            diags.push(diag);
        }
        if diags.has_errors() {
            Err(diags)
        } else {
            Ok(actual)
        }
    }
    /// Verifies a static traversal against the current scope schema.
    ///
    /// # Arguments
    /// * `trav` - The AST traversal to verify.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if undefined variables or invalid attribute lookups occur.
    ///
    /// # Returns
    /// The inferred result [`Type`].
    pub fn check_traversal(&self, trav: &Traversal) -> Result<Type, Diagnostics> {
        let mut diags = Diagnostics::new();
        let ty = self.check_traversal_inner(trav, &self.scope, &mut diags);
        if diags.has_errors() {
            Err(diags)
        } else {
            Ok(ty)
        }
    }
    /// Checks a function call statically against declared or built-in signatures.
    ///
    /// # Arguments
    /// * `fc` - The function call expression node.
    /// * `call_span` - The source span of the function call.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if argument count or argument types are invalid.
    ///
    /// # Returns
    /// The inferred return [`Type`].
    pub fn check_func_call(&self, fc: &FuncCall, call_span: &Span) -> Result<Type, Diagnostics> {
        let mut diags = Diagnostics::new();
        let ty = self.check_func_call_inner(fc, call_span, &self.scope, &mut diags);
        if diags.has_errors() {
            Err(diags)
        } else {
            Ok(ty)
        }
    }
    fn check_body_with_scope(
        &self,
        body: &Body,
        schema: &BodySchema,
        scope: &ScopeSchema,
        diags: &mut Diagnostics,
    ) {
        for (name, attr) in &body.attributes {
            if let Some(attr_schema) = schema.attributes.get(name) {
                let actual_ty = self.infer_expression_type_inner(&attr.expr, scope, diags);
                if let Some(expected_ty) = &attr_schema.expected_type {
                    if !is_type_compatible(&actual_ty, expected_ty) {
                        let mut diag = Diagnostic::new(
                            HclError::Type(format!(
                                "Invalid value for attribute {name:?}: expected {expected_ty}, got {actual_ty}"
                            )),
                            attr.expr.span(),
                        );
                        diag.summary = Some(format!("Attribute {name:?} type mismatch"));
                        diag.detail = Some(format!(
                            "Expected type {expected_ty}, but received type {actual_ty}"
                        ));
                        diags.push(diag);
                    }
                }
            } else {
                let mut diag = Diagnostic::new(
                    HclError::Schema(format!("An argument named {name:?} is not expected here.")),
                    attr.name_span.clone(),
                );
                if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                    name,
                    schema.attributes.keys().map(String::as_str),
                    2,
                ) {
                    diag = diag.with_detail(format!("Did you mean \"{candidate}\"?"));
                }
                diags.push(diag);
            }
        }
        for attr_schema in schema.attributes.values() {
            if attr_schema.required && !body.attributes.contains_key(&attr_schema.name) {
                diags.push(Diagnostic::new(
                    HclError::Schema(format!(
                        "The argument {:?} is required, but no definition was found.",
                        attr_schema.name
                    )),
                    body.span.clone(),
                ));
            }
        }
        for block in &body.blocks {
            if let Some(block_schema) = schema.blocks.get(&block.block_type) {
                let expected_count = block_schema.label_names.len();
                let actual_count = block.labels.len();
                if actual_count != expected_count {
                    diags.push(Diagnostic::new(
                        HclError::Schema(format!(
                            "Wrong number of labels for block {:?}: expected {}, got {}.",
                            block.block_type, expected_count, actual_count
                        )),
                        block.type_span.clone(),
                    ));
                }
                if let Some(inner_schema) = &block_schema.body_schema {
                    self.check_body_with_scope(&block.body, inner_schema, scope, diags);
                }
            } else {
                let mut diag = Diagnostic::new(
                    HclError::Schema(format!(
                        "Blocks of type {:?} are not expected here.",
                        block.block_type
                    )),
                    block.type_span.clone(),
                );
                if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                    &block.block_type,
                    schema.blocks.keys().map(String::as_str),
                    2,
                ) {
                    diag = diag.with_detail(format!("Did you mean \"{candidate}\"?"));
                }
                diags.push(diag);
            }
        }
        for dyn_block in &body.dynamic_blocks {
            self.check_dynamic_block(dyn_block, schema, scope, diags);
        }
    }
    fn check_dynamic_block(
        &self,
        dyn_block: &DynamicBlock,
        schema: &BodySchema,
        scope: &ScopeSchema,
        diags: &mut Diagnostics,
    ) {
        if let Some(block_schema) = schema.blocks.get(&dyn_block.block_type) {
            let coll_ty = self.infer_expression_type_inner(&dyn_block.for_each, scope, diags);
            let (k_ty, v_ty) = match coll_ty {
                Type::List(inner) | Type::Set(inner) => (Type::Number, *inner),
                Type::Map(inner) => (Type::String, *inner),
                Type::Tuple(elems) => (Type::Number, unify_all(&elems)),
                Type::Object { attrs, .. } => (
                    Type::String,
                    unify_all(&attrs.values().cloned().collect::<Vec<_>>()),
                ),
                Type::Dynamic => (Type::Dynamic, Type::Dynamic),
                other => {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!(
                            "Dynamic block for_each must evaluate to a collection, got {other}"
                        )),
                        dyn_block.for_each.span(),
                    ));
                    (Type::Dynamic, Type::Dynamic)
                }
            };
            if let Some(ref labels) = dyn_block.labels {
                let expected_count = block_schema.label_names.len();
                if labels.len() != expected_count {
                    diags.push(Diagnostic::new(
                        HclError::Schema(format!(
                            "Wrong number of labels for dynamic block {:?}: expected {}, got {}.",
                            dyn_block.block_type,
                            expected_count,
                            labels.len()
                        )),
                        dyn_block.type_span.clone(),
                    ));
                }
                for lbl in labels {
                    let lbl_ty = self.infer_expression_type_inner(lbl, scope, diags);
                    if !is_type_compatible(&lbl_ty, &Type::String) {
                        diags.push(Diagnostic::new(
                            HclError::Type(format!(
                                "Dynamic block label expression must be a string, got {lbl_ty}"
                            )),
                            lbl.span(),
                        ));
                    }
                }
            }
            let iterator_name = dyn_block
                .iterator
                .as_deref()
                .unwrap_or(&dyn_block.block_type);
            let mut child_scope = scope.child_scope();
            let mut iter_attrs = BTreeMap::new();
            iter_attrs.insert("key".to_string(), k_ty);
            iter_attrs.insert("value".to_string(), v_ty);
            child_scope.variables.insert(
                iterator_name.to_string(),
                Type::Object {
                    attrs: iter_attrs,
                    optional_attrs: BTreeSet::new(),
                },
            );
            if let Some(inner_schema) = &block_schema.body_schema {
                self.check_body_with_scope(&dyn_block.content, inner_schema, &child_scope, diags);
            }
        } else {
            let mut diag = Diagnostic::new(
                HclError::Schema(format!(
                    "Blocks of type {:?} are not expected here.",
                    dyn_block.block_type
                )),
                dyn_block.type_span.clone(),
            );
            if let Some(candidate) = crate::diagnostic::suggestion::suggest_closest_name(
                &dyn_block.block_type,
                schema.blocks.keys().map(String::as_str),
                2,
            ) {
                diag = diag.with_detail(format!("Did you mean \"{candidate}\"?"));
            }
            diags.push(diag);
        }
    }
    /// Check template part.
    pub fn check_template_part(
        &self,
        part: &TemplatePart,
        scope: &ScopeSchema,
        diags: &mut Diagnostics,
    ) {
        match part {
            TemplatePart::Literal(_, _) => {}
            TemplatePart::Interpolation(interp_expr, _) => {
                self.infer_expression_type_inner(interp_expr, scope, diags);
            }
            TemplatePart::Directive(dir, _) => match dir {
                Directive::If {
                    cond,
                    true_expr,
                    else_ifs,
                    false_expr,
                } => {
                    let c_ty = self.infer_expression_type_inner(cond, scope, diags);
                    if !is_type_compatible(&c_ty, &Type::Bool) {
                        diags.push(Diagnostic::new(
                            HclError::Type(format!(
                                "Template if condition must be bool, got {c_ty}"
                            )),
                            cond.span(),
                        ));
                    }
                    for p in true_expr {
                        self.check_template_part(p, scope, diags);
                    }
                    for (elif_cond, elif_body) in else_ifs {
                        let ec_ty = self.infer_expression_type_inner(elif_cond, scope, diags);
                        if !is_type_compatible(&ec_ty, &Type::Bool) {
                            diags.push(Diagnostic::new(
                                HclError::Type(format!(
                                    "Template else-if condition must be bool, got {ec_ty}"
                                )),
                                elif_cond.span(),
                            ));
                        }
                        for p in elif_body {
                            self.check_template_part(p, scope, diags);
                        }
                    }
                    if let Some(fb) = false_expr {
                        for p in fb {
                            self.check_template_part(p, scope, diags);
                        }
                    }
                }
                Directive::For {
                    key_var,
                    val_var,
                    collection,
                    body,
                } => {
                    let coll_ty = self.infer_expression_type_inner(collection, scope, diags);
                    let (k_ty, v_ty) = match coll_ty {
                        Type::List(inner) | Type::Set(inner) => (Type::Number, *inner),
                        Type::Map(inner) => (Type::String, *inner),
                        Type::Tuple(elems) => (Type::Number, unify_all(&elems)),
                        Type::Object { attrs, .. } => (
                            Type::String,
                            unify_all(&attrs.values().cloned().collect::<Vec<_>>()),
                        ),
                        Type::Dynamic => (Type::Dynamic, Type::Dynamic),
                        other => {
                            diags.push(Diagnostic::new(
                                HclError::Type(format!(
                                    "Cannot iterate over template collection of type {other}"
                                )),
                                collection.span(),
                            ));
                            (Type::Dynamic, Type::Dynamic)
                        }
                    };
                    let mut child = scope.child_scope();
                    if let Some(k) = key_var {
                        child.variables.insert(k.clone(), k_ty);
                    }
                    child.variables.insert(val_var.clone(), v_ty);
                    for p in body {
                        self.check_template_part(p, &child, diags);
                    }
                }
                Directive::Strip { .. } => {}
            },
        }
    }
    fn infer_expression_type_inner(
        &self,
        expr: &Expression,
        scope: &ScopeSchema,
        diags: &mut Diagnostics,
    ) -> Type {
        match expr {
            Expression::Null(_) => Type::Dynamic,
            Expression::Bool(_, _) => Type::Bool,
            Expression::Number(_, _) => Type::Number,
            Expression::String(_, _) => Type::String,
            Expression::Tuple(elems, _) => {
                let elem_types = elems
                    .iter()
                    .map(|el| self.infer_expression_type_inner(el, scope, diags))
                    .collect();
                Type::Tuple(elem_types)
            }
            Expression::Object(entries, _) => {
                let mut attrs = BTreeMap::new();
                for (k_expr, v_expr) in entries {
                    let k_name = match k_expr {
                        Expression::String(s, _) | Expression::Variable(s, _) => Some(s.clone()),
                        other => {
                            let k_ty = self.infer_expression_type_inner(other, scope, diags);
                            if !is_type_compatible(&k_ty, &Type::String) {
                                diags.push(Diagnostic::new(
                                    HclError::Type(format!(
                                        "Object key must be string, got {k_ty}"
                                    )),
                                    k_expr.span(),
                                ));
                            }
                            None
                        }
                    };
                    let v_ty = self.infer_expression_type_inner(v_expr, scope, diags);
                    if let Some(k) = k_name {
                        attrs.insert(k, v_ty);
                    }
                }
                Type::Object {
                    attrs,
                    optional_attrs: BTreeSet::new(),
                }
            }
            Expression::Template(parts, _) => {
                for part in parts {
                    self.check_template_part(part, scope, diags);
                }
                Type::String
            }
            Expression::Variable(name, span) => scope.get_variable(name).map_or_else(
                || {
                    if !scope.allow_undefined_variables {
                        diags.push(Diagnostic::new(
                            HclError::Type(format!("Undefined variable {name:?}")),
                            span.clone(),
                        ));
                    }
                    Type::Dynamic
                },
                Clone::clone,
            ),
            Expression::Traversal(trav, _) => self.check_traversal_inner(trav, scope, diags),
            Expression::FuncCall(fc, span) => self.check_func_call_inner(fc, span, scope, diags),
            Expression::Conditional(cond, span) => {
                let cond_ty = self.infer_expression_type_inner(&cond.cond_expr, scope, diags);
                if !is_type_compatible(&cond_ty, &Type::Bool) {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!(
                            "Condition in ternary expression must be bool, got {cond_ty}"
                        )),
                        cond.cond_expr.span(),
                    ));
                }
                let true_ty = self.infer_expression_type_inner(&cond.true_expr, scope, diags);
                let false_ty = self.infer_expression_type_inner(&cond.false_expr, scope, diags);
                crate::types::unify::unify(&true_ty, &false_ty).unwrap_or_else(|| {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!(
                            "Conditional branches have incompatible types ({true_ty} vs {false_ty})"
                        )),
                        span.clone(),
                    ));
                    Type::Dynamic
                })
            }
            Expression::BinaryOp(op, left, right, span) => {
                self.infer_binary_op_type(*op, left, right, span, scope, diags)
            }
            Expression::UnaryOp(op, inner, _) => self.infer_unary_op_type(*op, inner, scope, diags),
            Expression::ForExpr(for_expr, _) => self.infer_for_expr_type(for_expr, scope, diags),
            Expression::Parentheses(inner, _) => {
                self.infer_expression_type_inner(inner, scope, diags)
            }
        }
    }

    fn infer_binary_op_type(
        &self,
        op: BinaryOp,
        left: &Expression,
        right: &Expression,
        span: &Span,
        scope: &ScopeSchema,
        diags: &mut Diagnostics,
    ) -> Type {
        let l_ty = self.infer_expression_type_inner(left, scope, diags);
        let r_ty = self.infer_expression_type_inner(right, scope, diags);
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                if !is_type_compatible(&l_ty, &Type::Number) {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!("Left operand must be number, got {l_ty}")),
                        left.span(),
                    ));
                }
                if !is_type_compatible(&r_ty, &Type::Number) {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!("Right operand must be number, got {r_ty}")),
                        right.span(),
                    ));
                }
                Type::Number
            }
            BinaryOp::Less | BinaryOp::LessEq | BinaryOp::Greater | BinaryOp::GreaterEq => {
                if !is_type_compatible(&l_ty, &Type::Number) {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!("Left operand must be number, got {l_ty}")),
                        left.span(),
                    ));
                }
                if !is_type_compatible(&r_ty, &Type::Number) {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!("Right operand must be number, got {r_ty}")),
                        right.span(),
                    ));
                }
                Type::Bool
            }
            BinaryOp::And | BinaryOp::Or => {
                if !is_type_compatible(&l_ty, &Type::Bool) {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!("Left operand must be bool, got {l_ty}")),
                        left.span(),
                    ));
                }
                if !is_type_compatible(&r_ty, &Type::Bool) {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!("Right operand must be bool, got {r_ty}")),
                        right.span(),
                    ));
                }
                Type::Bool
            }
            BinaryOp::Eq | BinaryOp::NotEq => {
                if l_ty != Type::Dynamic
                    && r_ty != Type::Dynamic
                    && !is_type_compatible(&l_ty, &r_ty)
                    && !is_type_compatible(&r_ty, &l_ty)
                {
                    let mut diag = Diagnostic::new(
                        HclError::Type(format!(
                            "Comparison between incompatible types ({l_ty} and {r_ty})"
                        )),
                        span.clone(),
                    );
                    diag.severity = crate::diagnostic::Severity::Warning;
                    diags.push(diag);
                }
                Type::Bool
            }
        }
    }

    fn infer_unary_op_type(
        &self,
        op: UnaryOp,
        inner: &Expression,
        scope: &ScopeSchema,
        diags: &mut Diagnostics,
    ) -> Type {
        let inner_ty = self.infer_expression_type_inner(inner, scope, diags);
        match op {
            UnaryOp::Not => {
                if !is_type_compatible(&inner_ty, &Type::Bool) {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!("Operand must be bool, got {inner_ty}")),
                        inner.span(),
                    ));
                }
                Type::Bool
            }
            UnaryOp::Neg => {
                if !is_type_compatible(&inner_ty, &Type::Number) {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!("Operand must be number, got {inner_ty}")),
                        inner.span(),
                    ));
                }
                Type::Number
            }
        }
    }
    fn infer_for_expr_type(
        &self,
        for_expr: &ForExpr,
        scope: &ScopeSchema,
        diags: &mut Diagnostics,
    ) -> Type {
        let coll_ty = self.infer_expression_type_inner(&for_expr.collection, scope, diags);
        let (k_ty, v_ty) = match coll_ty {
            Type::List(inner) | Type::Set(inner) => (Type::Number, *inner),
            Type::Map(inner) => (Type::String, *inner),
            Type::Tuple(elems) => (Type::Number, unify_all(&elems)),
            Type::Object { attrs, .. } => (
                Type::String,
                unify_all(&attrs.values().cloned().collect::<Vec<_>>()),
            ),
            Type::Dynamic => (Type::Dynamic, Type::Dynamic),
            other => {
                diags.push(Diagnostic::new(
                    HclError::Type(format!("Cannot iterate over value of type {other}")),
                    for_expr.collection.span(),
                ));
                (Type::Dynamic, Type::Dynamic)
            }
        };
        let mut child_scope = scope.child_scope();
        if let Some(ref k_var) = for_expr.key_var {
            child_scope.variables.insert(k_var.clone(), k_ty);
        }
        child_scope.variables.insert(for_expr.val_var.clone(), v_ty);
        if let Some(ref cond) = for_expr.cond_expr {
            let cond_ty = self.infer_expression_type_inner(cond, &child_scope, diags);
            if !is_type_compatible(&cond_ty, &Type::Bool) {
                diags.push(Diagnostic::new(
                    HclError::Type(format!(
                        "Condition in for expression must be bool, got {cond_ty}"
                    )),
                    cond.span(),
                ));
            }
        }
        let val_ty = self.infer_expression_type_inner(&for_expr.val_expr, &child_scope, diags);
        if let Some(ref key_expr) = for_expr.key_expr {
            let key_ty = self.infer_expression_type_inner(key_expr, &child_scope, diags);
            if !is_type_compatible(&key_ty, &Type::String) {
                diags.push(Diagnostic::new(
                    HclError::Type(format!(
                        "Key in for expression must evaluate to string, got {key_ty}"
                    )),
                    key_expr.span(),
                ));
            }
            if for_expr.grouping {
                Type::Map(Box::new(Type::List(Box::new(val_ty))))
            } else {
                Type::Map(Box::new(val_ty))
            }
        } else {
            Type::List(Box::new(val_ty))
        }
    }
    fn check_traversal_inner(
        &self,
        trav: &Traversal,
        scope: &ScopeSchema,
        diags: &mut Diagnostics,
    ) -> Type {
        let mut curr_ty = self.infer_expression_type_inner(&trav.expr, scope, diags);
        for op in &trav.operators {
            match op {
                TraversalOperator::GetAttr(attr_name, op_span) => match curr_ty {
                    Type::Dynamic => {
                        curr_ty = Type::Dynamic;
                    }
                    Type::Object { ref attrs, .. } => {
                        if let Some(val_ty) = attrs.get(attr_name) {
                            curr_ty = val_ty.clone();
                        } else {
                            diags.push(Diagnostic::new(
                                HclError::Traversal(format!(
                                    "This object does not have an attribute named {attr_name:?}."
                                )),
                                op_span.clone(),
                            ));
                            curr_ty = Type::Dynamic;
                        }
                    }
                    Type::Map(val_ty) => {
                        curr_ty = *val_ty;
                    }
                    other => {
                        diags.push(Diagnostic::new(
                            HclError::Traversal(format!(
                                "Cannot access attribute {attr_name:?} on value of type {other}."
                            )),
                            op_span.clone(),
                        ));
                        curr_ty = Type::Dynamic;
                    }
                },
                TraversalOperator::Index(idx_expr, op_span) => {
                    curr_ty =
                        self.check_traversal_index_op(curr_ty, idx_expr, op_span, scope, diags);
                }
                TraversalOperator::LegacyIndex(idx, op_span) => match curr_ty {
                    Type::Dynamic => {
                        curr_ty = Type::Dynamic;
                    }
                    Type::Tuple(elems) => {
                        if let Some(elem_ty) = usize::try_from(*idx)
                            .ok()
                            .and_then(|i| elems.get(i))
                            .cloned()
                        {
                            curr_ty = elem_ty;
                        } else {
                            diags.push(Diagnostic::new(
                                HclError::Traversal(format!(
                                    "Index {idx} out of bounds for tuple of length {}",
                                    elems.len()
                                )),
                                op_span.clone(),
                            ));
                            curr_ty = Type::Dynamic;
                        }
                    }
                    Type::List(elem_ty) => {
                        curr_ty = *elem_ty;
                    }
                    other => {
                        diags.push(Diagnostic::new(
                            HclError::Traversal(format!("Cannot index value of type {other}.")),
                            op_span.clone(),
                        ));
                        curr_ty = Type::Dynamic;
                    }
                },
                TraversalOperator::AttrSplat(op_span) | TraversalOperator::FullSplat(op_span) => {
                    match curr_ty {
                        Type::Dynamic => {
                            curr_ty = Type::Dynamic;
                        }
                        Type::List(elem_ty) | Type::Set(elem_ty) => {
                            curr_ty = Type::List(elem_ty);
                        }
                        Type::Tuple(elems) => {
                            curr_ty = Type::List(Box::new(unify_all(&elems)));
                        }
                        other => {
                            diags.push(Diagnostic::new(
                                HclError::Traversal(format!(
                                    "Cannot apply splat operator to value of type {other}."
                                )),
                                op_span.clone(),
                            ));
                            curr_ty = Type::Dynamic;
                        }
                    }
                }
            }
        }
        curr_ty
    }

    fn check_traversal_index_op(
        &self,
        curr_ty: Type,
        idx_expr: &Expression,
        op_span: &Span,
        scope: &ScopeSchema,
        diags: &mut Diagnostics,
    ) -> Type {
        let idx_ty = self.infer_expression_type_inner(idx_expr, scope, diags);
        match curr_ty {
            Type::Dynamic => Type::Dynamic,
            Type::List(elem_ty) | Type::Set(elem_ty) => {
                if !is_type_compatible(&idx_ty, &Type::Number) {
                    diags.push(Diagnostic::new(
                        HclError::Traversal(format!(
                            "Invalid index type for collection: expected number, got {idx_ty}"
                        )),
                        op_span.clone(),
                    ));
                }
                *elem_ty
            }
            Type::Tuple(elems) => {
                if !is_type_compatible(&idx_ty, &Type::Number) {
                    diags.push(Diagnostic::new(
                        HclError::Traversal(format!(
                            "Invalid index type for tuple: expected number, got {idx_ty}"
                        )),
                        op_span.clone(),
                    ));
                }
                if let Expression::Number(num, _) = idx_expr {
                    num.0.to_u64().map_or_else(
                        || unify_all(&elems),
                        |i| {
                            usize::try_from(i)
                                .ok()
                                .and_then(|idx| elems.get(idx))
                                .map_or_else(
                                    || {
                                        diags.push(Diagnostic::new(
                                            HclError::Traversal(format!(
                                                "Index {i} out of bounds for tuple of length {}",
                                                elems.len()
                                            )),
                                            op_span.clone(),
                                        ));
                                        Type::Dynamic
                                    },
                                    Clone::clone,
                                )
                        },
                    )
                } else {
                    unify_all(&elems)
                }
            }
            Type::Map(elem_ty) => {
                if !is_type_compatible(&idx_ty, &Type::String) {
                    diags.push(Diagnostic::new(
                        HclError::Traversal(format!(
                            "Invalid index type for map: expected string, got {idx_ty}"
                        )),
                        op_span.clone(),
                    ));
                }
                *elem_ty
            }
            Type::Object { attrs, .. } => {
                if !is_type_compatible(&idx_ty, &Type::String) {
                    diags.push(Diagnostic::new(
                        HclError::Traversal(format!(
                            "Invalid index type for object: expected string, got {idx_ty}"
                        )),
                        op_span.clone(),
                    ));
                }
                if let Expression::String(k, _) = idx_expr {
                    attrs.get(k).map_or_else(
                        || {
                            diags.push(Diagnostic::new(
                                HclError::Traversal(format!(
                                    "Attribute {k:?} not found on object."
                                )),
                                op_span.clone(),
                            ));
                            Type::Dynamic
                        },
                        Clone::clone,
                    )
                } else {
                    unify_all(&attrs.values().cloned().collect::<Vec<_>>())
                }
            }
            other => {
                diags.push(Diagnostic::new(
                    HclError::Traversal(format!("Cannot index value of type {other}.")),
                    op_span.clone(),
                ));
                Type::Dynamic
            }
        }
    }

    fn check_func_arg_spec(
        fn_name: &str,
        spec: &crate::eval::func::FunctionParamSpec,
        arg_ty: &Type,
        arg_span: Span,
        diags: &mut Diagnostics,
    ) {
        if !spec.allow_dynamic_type && arg_ty.is_dynamic() {
            diags.push(Diagnostic::new(
                HclError::FunctionArgMismatch(format!(
                    "Argument {:?} for function {:?} requires exact type {}, got dynamic",
                    spec.name, fn_name, spec.param_type
                )),
                arg_span,
            ));
        } else if spec.param_type != Type::Dynamic
            && !arg_ty.is_dynamic()
            && !is_type_compatible(arg_ty, &spec.param_type)
        {
            diags.push(Diagnostic::new(
                HclError::FunctionArgMismatch(format!(
                    "Invalid type for argument {:?} in call to {:?}: expected {}, got {}",
                    spec.name, fn_name, spec.param_type, arg_ty
                )),
                arg_span,
            ));
        }
    }

    fn check_func_call_inner(
        &self,
        fc: &FuncCall,
        call_span: &Span,
        scope: &ScopeSchema,
        diags: &mut Diagnostics,
    ) -> Type {
        let name = fc.name.to_string();
        let mut arg_types = Vec::with_capacity(fc.args.len());
        for arg in &fc.args {
            let ty = self.infer_expression_type_inner(arg, scope, diags);
            arg_types.push(ty);
        }
        let sig = scope.get_function(&name).cloned().or_else(|| {
            crate::eval::stdlib::get_stdlib_function(&name).and_then(|f| f.signature.clone())
        });
        if let Some(signature) = sig {
            let expected_fixed = signature.params.len();
            if let Some(ref variadic) = signature.variadic_param {
                if arg_types.len() < expected_fixed {
                    diags.push(Diagnostic::new(
                        HclError::FunctionArgMismatch(format!(
                            "Too few arguments for function {:?}: expected at least {}, got {}",
                            name,
                            expected_fixed,
                            arg_types.len()
                        )),
                        call_span.clone(),
                    ));
                }
                for (idx, arg_expr) in fc.args.iter().enumerate() {
                    let spec = if idx < expected_fixed {
                        &signature.params[idx]
                    } else {
                        variadic
                    };
                    Self::check_func_arg_spec(&name, spec, &arg_types[idx], arg_expr.span(), diags);
                }
            } else {
                if arg_types.len() != expected_fixed {
                    diags.push(Diagnostic::new(
                        HclError::FunctionArgMismatch(format!(
                            "Wrong number of arguments for function {:?}: expected {}, got {}",
                            name,
                            expected_fixed,
                            arg_types.len()
                        )),
                        call_span.clone(),
                    ));
                }
                for (idx, arg_expr) in fc.args.iter().enumerate() {
                    if idx < expected_fixed {
                        Self::check_func_arg_spec(
                            &name,
                            &signature.params[idx],
                            &arg_types[idx],
                            arg_expr.span(),
                            diags,
                        );
                    }
                }
            }
            match (signature.return_type)(&arg_types) {
                Ok(ret) => ret,
                Err(err) => {
                    diags.push(Diagnostic::new(
                        HclError::FunctionArgMismatch(format!(
                            "Failed to compute return type for function {name:?}: {err}"
                        )),
                        call_span.clone(),
                    ));
                    Type::Dynamic
                }
            }
        } else {
            if !scope.allow_undefined_functions {
                diags.push(Diagnostic::new(
                    HclError::FunctionArgMismatch(format!("Function {name:?} is not defined.")),
                    fc.name.span.clone(),
                ));
            }
            Type::Dynamic
        }
    }
}
#[cfg(test)]
#[path = "type_check_tests.rs"]
mod tests;
