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
    pub fn with_allow_undefined_variables(mut self, allow: bool) -> Self {
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
    pub fn with_allow_undefined_functions(mut self, allow: bool) -> Self {
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
    pub fn scope(&self) -> &ScopeSchema {
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
    fn check_template_part(
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
            Expression::Variable(name, span) => {
                if let Some(ty) = scope.get_variable(name) {
                    ty.clone()
                } else {
                    if !scope.allow_undefined_variables {
                        diags.push(Diagnostic::new(
                            HclError::Type(format!("Undefined variable {name:?}")),
                            span.clone(),
                        ));
                    }
                    Type::Dynamic
                }
            }
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
                if let Some(unified) = crate::types::unify::unify(&true_ty, &false_ty) {
                    unified
                } else {
                    diags.push(Diagnostic::new(
                        HclError::Type(format!(
                            "Conditional branches have incompatible types ({true_ty} vs {false_ty})"
                        )),
                        span.clone(),
                    ));
                    Type::Dynamic
                }
            }
            Expression::BinaryOp(op, left, right, span) => {
                let l_ty = self.infer_expression_type_inner(left, scope, diags);
                let r_ty = self.infer_expression_type_inner(right, scope, diags);
                match op {
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod => {
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
            Expression::UnaryOp(op, inner, _) => {
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
            Expression::ForExpr(for_expr, _) => self.infer_for_expr_type(for_expr, scope, diags),
            Expression::Parentheses(inner, _) => {
                self.infer_expression_type_inner(inner, scope, diags)
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
                    let idx_ty = self.infer_expression_type_inner(idx_expr, scope, diags);
                    match curr_ty {
                        Type::Dynamic => {
                            curr_ty = Type::Dynamic;
                        }
                        Type::List(elem_ty) | Type::Set(elem_ty) => {
                            if !is_type_compatible(&idx_ty, &Type::Number) {
                                diags
                                    .push(
                                        Diagnostic::new(
                                            HclError::Traversal(
                                                format!(
                                                    "Invalid index type for collection: expected number, got {idx_ty}"
                                                ),
                                            ),
                                            op_span.clone(),
                                        ),
                                    );
                            }
                            curr_ty = *elem_ty;
                        }
                        Type::Tuple(elems) => {
                            if !is_type_compatible(&idx_ty, &Type::Number) {
                                diags
                                    .push(
                                        Diagnostic::new(
                                            HclError::Traversal(
                                                format!(
                                                    "Invalid index type for tuple: expected number, got {idx_ty}"
                                                ),
                                            ),
                                            op_span.clone(),
                                        ),
                                    );
                            }
                            if let Expression::Number(num, _) = idx_expr {
                                if let Some(i) = num.0.to_u64() {
                                    if (i as usize) < elems.len() {
                                        curr_ty = elems[i as usize].clone();
                                    } else {
                                        diags.push(Diagnostic::new(
                                            HclError::Traversal(format!(
                                                "Index {i} out of bounds for tuple of length {}",
                                                elems.len()
                                            )),
                                            op_span.clone(),
                                        ));
                                        curr_ty = Type::Dynamic;
                                    }
                                } else {
                                    curr_ty = unify_all(&elems);
                                }
                            } else {
                                curr_ty = unify_all(&elems);
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
                            curr_ty = *elem_ty;
                        }
                        Type::Object { attrs, .. } => {
                            if !is_type_compatible(&idx_ty, &Type::String) {
                                diags
                                    .push(
                                        Diagnostic::new(
                                            HclError::Traversal(
                                                format!(
                                                    "Invalid index type for object: expected string, got {idx_ty}"
                                                ),
                                            ),
                                            op_span.clone(),
                                        ),
                                    );
                            }
                            if let Expression::String(k, _) = idx_expr {
                                if let Some(val_ty) = attrs.get(k) {
                                    curr_ty = val_ty.clone();
                                } else {
                                    diags.push(Diagnostic::new(
                                        HclError::Traversal(format!(
                                            "Attribute {k:?} not found on object."
                                        )),
                                        op_span.clone(),
                                    ));
                                    curr_ty = Type::Dynamic;
                                }
                            } else {
                                curr_ty = unify_all(&attrs.values().cloned().collect::<Vec<_>>());
                            }
                        }
                        other => {
                            diags.push(Diagnostic::new(
                                HclError::Traversal(format!("Cannot index value of type {other}.")),
                                op_span.clone(),
                            ));
                            curr_ty = Type::Dynamic;
                        }
                    }
                }
                TraversalOperator::LegacyIndex(idx, op_span) => match curr_ty {
                    Type::Dynamic => {
                        curr_ty = Type::Dynamic;
                    }
                    Type::Tuple(elems) => {
                        if (*idx as usize) < elems.len() {
                            curr_ty = elems[*idx as usize].clone();
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
                    let arg_ty = &arg_types[idx];
                    if !spec.allow_dynamic_type {
                        if arg_ty.is_dynamic() {
                            diags
                                .push(
                                    Diagnostic::new(
                                        HclError::FunctionArgMismatch(
                                            format!(
                                                "Argument {:?} for function {:?} requires exact type {}, got dynamic",
                                                spec.name, name, spec.param_type
                                            ),
                                        ),
                                        arg_expr.span(),
                                    ),
                                );
                        } else if spec.param_type != Type::Dynamic
                            && !arg_ty.is_dynamic()
                            && !is_type_compatible(arg_ty, &spec.param_type)
                        {
                            diags
                                .push(
                                    Diagnostic::new(
                                        HclError::FunctionArgMismatch(
                                            format!(
                                                "Invalid type for argument {:?} in call to {:?}: expected {}, got {}",
                                                spec.name, name, spec.param_type, arg_ty
                                            ),
                                        ),
                                        arg_expr.span(),
                                    ),
                                );
                        }
                    } else if spec.param_type != Type::Dynamic
                        && !arg_ty.is_dynamic()
                        && !is_type_compatible(arg_ty, &spec.param_type)
                    {
                        diags
                            .push(
                                Diagnostic::new(
                                    HclError::FunctionArgMismatch(
                                        format!(
                                            "Invalid type for argument {:?} in call to {:?}: expected {}, got {}",
                                            spec.name, name, spec.param_type, arg_ty
                                        ),
                                    ),
                                    arg_expr.span(),
                                ),
                            );
                    }
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
                        let spec = &signature.params[idx];
                        let arg_ty = &arg_types[idx];
                        if !spec.allow_dynamic_type {
                            if arg_ty.is_dynamic() {
                                diags
                                    .push(
                                        Diagnostic::new(
                                            HclError::FunctionArgMismatch(
                                                format!(
                                                    "Argument {:?} for function {:?} requires exact type {}, got dynamic",
                                                    spec.name, name, spec.param_type
                                                ),
                                            ),
                                            arg_expr.span(),
                                        ),
                                    );
                            } else if spec.param_type != Type::Dynamic
                                && !arg_ty.is_dynamic()
                                && !is_type_compatible(arg_ty, &spec.param_type)
                            {
                                diags
                                    .push(
                                        Diagnostic::new(
                                            HclError::FunctionArgMismatch(
                                                format!(
                                                    "Invalid type for argument {:?} in call to {:?}: expected {}, got {}",
                                                    spec.name, name, spec.param_type, arg_ty
                                                ),
                                            ),
                                            arg_expr.span(),
                                        ),
                                    );
                            }
                        } else if spec.param_type != Type::Dynamic
                            && !arg_ty.is_dynamic()
                            && !is_type_compatible(arg_ty, &spec.param_type)
                        {
                            diags
                                .push(
                                    Diagnostic::new(
                                        HclError::FunctionArgMismatch(
                                            format!(
                                                "Invalid type for argument {:?} in call to {:?}: expected {}, got {}",
                                                spec.name, name, spec.param_type, arg_ty
                                            ),
                                        ),
                                        arg_expr.span(),
                                    ),
                                );
                        }
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
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::pedantic,
        clippy::nursery
    )]
    use super::*;
    use crate::ast::schema::{AttributeSchema, BlockHeaderSchema};
    use crate::eval::func::FunctionParamSpec;
    use crate::number::Number;
    use crate::parse::parser::Parser;
    /// Parses an HCL input string into a Body AST for testing.
    fn parse_body(input: &str) -> Body {
        let mut parser = Parser::new(input);
        parser.parse_body()
    }
    /// Parses an expression string into an AST Expression for testing.
    fn parse_expr(input: &str) -> Expression {
        let body = parse_body(&format!("val = {input}"));
        body.attributes["val"].expr.clone()
    }
    #[test]
    fn test_is_type_compatible_permutations() {
        assert!(is_type_compatible(&Type::String, &Type::String));
        assert!(is_type_compatible(&Type::Number, &Type::Number));
        assert!(is_type_compatible(&Type::Bool, &Type::Bool));
        assert!(is_type_compatible(&Type::Dynamic, &Type::String));
        assert!(is_type_compatible(&Type::String, &Type::Dynamic));
        assert!(is_type_compatible(&Type::String, &Type::Number));
        assert!(is_type_compatible(&Type::Number, &Type::String));
        assert!(is_type_compatible(&Type::String, &Type::Bool));
        assert!(is_type_compatible(&Type::Bool, &Type::String));
        let list_num = Type::List(Box::new(Type::Number));
        let set_num = Type::Set(Box::new(Type::Number));
        let list_str = Type::List(Box::new(Type::String));
        assert!(is_type_compatible(&list_num, &list_num));
        assert!(is_type_compatible(&list_num, &set_num));
        assert!(is_type_compatible(&set_num, &list_num));
        assert!(is_type_compatible(&list_num, &list_str));
        let tup_nums = Type::Tuple(vec![Type::Number, Type::Number]);
        assert!(is_type_compatible(&tup_nums, &list_num));
        assert!(is_type_compatible(&tup_nums, &set_num));
        let tup_mixed = Type::Tuple(vec![Type::Number, Type::String]);
        assert!(is_type_compatible(&tup_mixed, &tup_mixed));
        assert!(!is_type_compatible(
            &tup_nums,
            &Type::Tuple(vec![Type::Number])
        ));
        let map_num = Type::Map(Box::new(Type::Number));
        let map_str = Type::Map(Box::new(Type::String));
        assert!(is_type_compatible(&map_num, &map_num));
        assert!(is_type_compatible(&map_num, &map_str));
        let mut obj_attrs = BTreeMap::new();
        obj_attrs.insert("a".to_string(), Type::Number);
        obj_attrs.insert("b".to_string(), Type::Number);
        let obj_num = Type::Object {
            attrs: obj_attrs.clone(),
            optional_attrs: BTreeSet::new(),
        };
        assert!(is_type_compatible(&obj_num, &map_num));
        let mut target_attrs = BTreeMap::new();
        target_attrs.insert("a".to_string(), Type::Number);
        let mut target_opts = BTreeSet::new();
        target_opts.insert("opt".to_string());
        target_attrs.insert("opt".to_string(), Type::String);
        let target_obj = Type::Object {
            attrs: target_attrs,
            optional_attrs: target_opts,
        };
        assert!(is_type_compatible(&obj_num, &target_obj));
        assert!(!is_type_compatible(&Type::Bool, &list_num));
    }
    #[test]
    fn test_unify_all() {
        assert_eq!(unify_all(&[]), Type::Dynamic);
        assert_eq!(unify_all(&[Type::Number]), Type::Number);
        assert_eq!(
            unify_all(&[Type::Number, Type::Number, Type::Number]),
            Type::Number
        );
        assert_eq!(unify_all(&[Type::Number, Type::String]), Type::String);
        assert_eq!(
            unify_all(&[Type::Bool, Type::List(Box::new(Type::Number))]),
            Type::Dynamic
        );
    }
    #[test]
    fn test_scope_schema_builder() {
        let sig = FunctionSignature::with_static_return_type(vec![], Type::String);
        let scope = ScopeSchema::new()
            .with_variable("x", Type::Number)
            .with_variables(vec![("y".to_string(), Type::Bool)])
            .with_function("my_fn", sig)
            .with_allow_undefined_variables(true)
            .with_allow_undefined_functions(true);
        assert_eq!(scope.get_variable("x"), Some(&Type::Number));
        assert_eq!(scope.get_variable("y"), Some(&Type::Bool));
        assert!(scope.get_function("my_fn").is_some());
        assert!(scope.allow_undefined_variables);
        assert!(scope.allow_undefined_functions);
        let child = scope.child_scope();
        assert_eq!(child.get_variable("x"), Some(&Type::Number));
    }
    #[test]
    fn test_type_checker_primitives_and_collections() {
        let checker = TypeChecker::new();
        let body_null = parse_body("val = null");
        let ty = checker
            .infer_expression_type(&body_null.attributes["val"].expr)
            .unwrap();
        assert_eq!(ty, Type::Dynamic);
        let body_bool = parse_body("val = true");
        let ty = checker
            .infer_expression_type(&body_bool.attributes["val"].expr)
            .unwrap();
        assert_eq!(ty, Type::Bool);
        let body_num = parse_body("val = 42");
        let ty = checker
            .infer_expression_type(&body_num.attributes["val"].expr)
            .unwrap();
        assert_eq!(ty, Type::Number);
        let body_str = parse_body(r#"val = "hello""#);
        let ty = checker
            .infer_expression_type(&body_str.attributes["val"].expr)
            .unwrap();
        assert_eq!(ty, Type::String);
        let body_tup = parse_body("val = [1, 2, 3]");
        let ty = checker
            .infer_expression_type(&body_tup.attributes["val"].expr)
            .unwrap();
        assert_eq!(
            ty,
            Type::Tuple(vec![Type::Number, Type::Number, Type::Number])
        );
        let body_obj = parse_body(r#"val = { a = 1, b = "two" }"#);
        let ty = checker
            .infer_expression_type(&body_obj.attributes["val"].expr)
            .unwrap();
        let mut expected_attrs = BTreeMap::new();
        expected_attrs.insert("a".to_string(), Type::Number);
        expected_attrs.insert("b".to_string(), Type::String);
        assert_eq!(
            ty,
            Type::Object {
                attrs: expected_attrs,
                optional_attrs: BTreeSet::new(),
            }
        );
        let body_paren = parse_body("val = (10)");
        let ty = checker
            .infer_expression_type(&body_paren.attributes["val"].expr)
            .unwrap();
        assert_eq!(ty, Type::Number);
        assert!(
            checker
                .check_expression(&body_num.attributes["val"].expr, &Type::Number)
                .is_ok()
        );
        assert!(
            checker
                .check_expression(&body_str.attributes["val"].expr, &Type::Bool)
                .is_ok()
        );
    }
    #[test]
    fn test_type_checker_binary_and_unary_ops() {
        let checker = TypeChecker::new();
        let b1 = parse_body("val = 1 + 2 * 3 - 4 / 2 % 1");
        assert_eq!(
            checker
                .infer_expression_type(&b1.attributes["val"].expr)
                .unwrap(),
            Type::Number
        );
        let b2 = parse_body("val = 1 < 2");
        assert_eq!(
            checker
                .infer_expression_type(&b2.attributes["val"].expr)
                .unwrap(),
            Type::Bool
        );
        let b3 = parse_body("val = true && false || true");
        assert_eq!(
            checker
                .infer_expression_type(&b3.attributes["val"].expr)
                .unwrap(),
            Type::Bool
        );
        let b4 = parse_body("val = 1 == 2");
        assert_eq!(
            checker
                .infer_expression_type(&b4.attributes["val"].expr)
                .unwrap(),
            Type::Bool
        );
        let b5 = parse_body("val = !true");
        assert_eq!(
            checker
                .infer_expression_type(&b5.attributes["val"].expr)
                .unwrap(),
            Type::Bool
        );
        let b6 = parse_body("val = -42");
        assert_eq!(
            checker
                .infer_expression_type(&b6.attributes["val"].expr)
                .unwrap(),
            Type::Number
        );
    }
    #[test]
    fn test_type_checker_conditionals() {
        let checker = TypeChecker::new();
        let b1 = parse_body("val = true ? 1 : 2");
        assert_eq!(
            checker
                .infer_expression_type(&b1.attributes["val"].expr)
                .unwrap(),
            Type::Number
        );
        let b2 = parse_body(r#"val = false ? "yes" : "no""#);
        assert_eq!(
            checker
                .infer_expression_type(&b2.attributes["val"].expr)
                .unwrap(),
            Type::String
        );
    }
    #[test]
    fn test_type_checker_for_expressions() {
        let span = Span::default();
        let scope = ScopeSchema::new()
            .with_variable("items", Type::List(Box::new(Type::Number)))
            .with_variable("tags", Type::Map(Box::new(Type::String)));
        let checker = TypeChecker::new().with_scope(scope);
        let for_tuple = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "x".to_string(),
                collection: Box::new(Expression::Variable("items".to_string(), span.clone())),
                key_expr: None,
                val_expr: Box::new(Expression::BinaryOp(
                    BinaryOp::Mul,
                    Box::new(Expression::Variable("x".to_string(), span.clone())),
                    Box::new(Expression::Number(Number::from(2), span.clone())),
                    span.clone(),
                )),
                cond_expr: None,
                grouping: false,
            }),
            span.clone(),
        );
        assert_eq!(
            checker.infer_expression_type(&for_tuple).unwrap(),
            Type::List(Box::new(Type::Number))
        );
        let for_obj = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(Expression::Variable("tags".to_string(), span.clone())),
                key_expr: Some(Box::new(Expression::Variable(
                    "k".to_string(),
                    span.clone(),
                ))),
                val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                cond_expr: None,
                grouping: false,
            }),
            span.clone(),
        );
        assert_eq!(
            checker.infer_expression_type(&for_obj).unwrap(),
            Type::Map(Box::new(Type::String))
        );
        let for_group = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(Expression::Variable("tags".to_string(), span.clone())),
                key_expr: Some(Box::new(Expression::Variable(
                    "k".to_string(),
                    span.clone(),
                ))),
                val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                cond_expr: None,
                grouping: true,
            }),
            span.clone(),
        );
        assert_eq!(
            checker.infer_expression_type(&for_group).unwrap(),
            Type::Map(Box::new(Type::List(Box::new(Type::String))))
        );
        let for_cond = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "x".to_string(),
                collection: Box::new(Expression::Variable("items".to_string(), span.clone())),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("x".to_string(), span.clone())),
                cond_expr: Some(Box::new(Expression::BinaryOp(
                    BinaryOp::Greater,
                    Box::new(Expression::Variable("x".to_string(), span.clone())),
                    Box::new(Expression::Number(Number::from(0), span.clone())),
                    span.clone(),
                ))),
                grouping: false,
            }),
            span,
        );
        assert_eq!(
            checker.infer_expression_type(&for_cond).unwrap(),
            Type::List(Box::new(Type::Number))
        );
    }
    #[test]
    fn test_type_checker_traversals() {
        let mut user_attrs = BTreeMap::new();
        user_attrs.insert("name".to_string(), Type::String);
        user_attrs.insert("age".to_string(), Type::Number);
        let scope = ScopeSchema::new()
            .with_variable(
                "user",
                Type::Object {
                    attrs: user_attrs,
                    optional_attrs: BTreeSet::new(),
                },
            )
            .with_variable("scores", Type::List(Box::new(Type::Number)))
            .with_variable("pair", Type::Tuple(vec![Type::String, Type::Bool]))
            .with_variable("config", Type::Map(Box::new(Type::String)));
        let checker = TypeChecker::new().with_scope(scope);
        let b_var = parse_body("val = user");
        assert!(
            checker
                .infer_expression_type(&b_var.attributes["val"].expr)
                .is_ok()
        );
        let b_name = parse_body("val = user.name");
        assert_eq!(
            checker
                .infer_expression_type(&b_name.attributes["val"].expr)
                .unwrap(),
            Type::String
        );
        let b_bad_attr = parse_body("val = user.nonexistent");
        assert!(
            checker
                .infer_expression_type(&b_bad_attr.attributes["val"].expr)
                .is_err()
        );
        let b_list_idx = parse_body("val = scores[0]");
        assert_eq!(
            checker
                .infer_expression_type(&b_list_idx.attributes["val"].expr)
                .unwrap(),
            Type::Number
        );
        let b_tup_0 = parse_body("val = pair[0]");
        assert_eq!(
            checker
                .infer_expression_type(&b_tup_0.attributes["val"].expr)
                .unwrap(),
            Type::String
        );
        let b_tup_1 = parse_body("val = pair.1");
        assert_eq!(
            checker
                .infer_expression_type(&b_tup_1.attributes["val"].expr)
                .unwrap(),
            Type::Bool
        );
        let b_tup_oob = parse_body("val = pair[10]");
        assert!(
            checker
                .infer_expression_type(&b_tup_oob.attributes["val"].expr)
                .is_err()
        );
        let b_map_idx = parse_body(r#"val = config["env"]"#);
        assert_eq!(
            checker
                .infer_expression_type(&b_map_idx.attributes["val"].expr)
                .unwrap(),
            Type::String
        );
        let b_splat = parse_body("val = scores.*");
        assert_eq!(
            checker
                .infer_expression_type(&b_splat.attributes["val"].expr)
                .unwrap(),
            Type::List(Box::new(Type::Number))
        );
        let b_undef = parse_body("val = missing_var");
        assert!(
            checker
                .infer_expression_type(&b_undef.attributes["val"].expr)
                .is_err()
        );
        let perm_checker =
            TypeChecker::new().with_scope(ScopeSchema::new().with_allow_undefined_variables(true));
        assert_eq!(
            perm_checker
                .infer_expression_type(&b_undef.attributes["val"].expr)
                .unwrap(),
            Type::Dynamic
        );
    }
    #[test]
    fn test_type_checker_function_calls() {
        let sig = FunctionSignature::with_static_return_type(
            vec![
                FunctionParamSpec::new("a", Type::Number),
                FunctionParamSpec::new("b", Type::Number),
            ],
            Type::Number,
        );
        let var_sig = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("first", Type::String)],
            Type::String,
        )
        .with_variadic(FunctionParamSpec::new("rest", Type::String));
        let scope = ScopeSchema::new()
            .with_function("add", sig)
            .with_function("concat_all", var_sig);
        let checker = TypeChecker::new().with_scope(scope);
        let b1 = parse_body("val = add(10, 20)");
        assert_eq!(
            checker
                .infer_expression_type(&b1.attributes["val"].expr)
                .unwrap(),
            Type::Number
        );
        let b_few = parse_body("val = add(10)");
        assert!(
            checker
                .infer_expression_type(&b_few.attributes["val"].expr)
                .is_err()
        );
        let b_many = parse_body("val = add(10, 20, 30)");
        assert!(
            checker
                .infer_expression_type(&b_many.attributes["val"].expr)
                .is_err()
        );
        let b_mismatch = parse_body(r#"val = add("not_num", [1, 2])"#);
        assert!(
            checker
                .infer_expression_type(&b_mismatch.attributes["val"].expr)
                .is_err()
        );
        let b_var1 = parse_body(r#"val = concat_all("a")"#);
        assert_eq!(
            checker
                .infer_expression_type(&b_var1.attributes["val"].expr)
                .unwrap(),
            Type::String
        );
        let b_var2 = parse_body(r#"val = concat_all("a", "b", "c")"#);
        assert_eq!(
            checker
                .infer_expression_type(&b_var2.attributes["val"].expr)
                .unwrap(),
            Type::String
        );
        let b_range = parse_body("val = range(5)");
        assert_eq!(
            checker
                .infer_expression_type(&b_range.attributes["val"].expr)
                .unwrap(),
            Type::List(Box::new(Type::Number))
        );
        let b_undef_fn = parse_body("val = nonexistent_function(1)");
        assert!(
            checker
                .infer_expression_type(&b_undef_fn.attributes["val"].expr)
                .is_err()
        );
    }
    #[test]
    fn test_type_checker_body_validation() {
        let schema = BodySchema::new()
            .with_attribute(AttributeSchema::required("name").with_type(Type::String))
            .with_attribute(AttributeSchema::optional("age").with_type(Type::Number))
            .with_block(
                BlockHeaderSchema::new("server", vec!["name".to_string()]).with_body_schema(
                    BodySchema::new()
                        .with_attribute(AttributeSchema::required("ip").with_type(Type::String)),
                ),
            );
        let checker = TypeChecker::new();
        let b_valid = parse_body(
            r#"
            name = "production"
            age = 30
            server "web" {
                ip = "192.168.1.1"
            }
        "#,
        );
        assert!(checker.check_body(&b_valid, &schema).is_ok());
        let b_missing = parse_body(
            r"
            age = 30
        ",
        );
        let err_missing = checker.check_body(&b_missing, &schema).unwrap_err();
        assert!(err_missing.to_string().contains("is required"));
        let b_typo = parse_body(
            r#"
            name = "production"
            nane = "typo"
        "#,
        );
        let err_typo = checker.check_body(&b_typo, &schema).unwrap_err();
        assert!(err_typo.to_string().contains("not expected here"));
        let b_bad_type = parse_body(
            r"
            name = [1, 2, 3]
        ",
        );
        let err_bad_type = checker.check_body(&b_bad_type, &schema).unwrap_err();
        assert!(
            err_bad_type
                .to_string()
                .contains("Invalid value for attribute")
        );
        let b_labels = parse_body(
            r#"
            name = "test"
            server "web" "extra_label" {
                ip = "10.0.0.1"
            }
        "#,
        );
        let err_labels = checker.check_body(&b_labels, &schema).unwrap_err();
        assert!(err_labels.to_string().contains("Wrong number of labels"));
        let b_bad_block = parse_body(
            r#"
            name = "test"
            servr "web" {
                ip = "10.0.0.1"
            }
        "#,
        );
        let err_bad_block = checker.check_body(&b_bad_block, &schema).unwrap_err();
        assert!(err_bad_block.to_string().contains("Blocks of type"));
        assert_eq!(
            err_bad_block.errors()[0].detail.as_deref(),
            Some("Did you mean \"server\"?")
        );
        let b_unrelated = parse_body(
            r#"
            name = "test"
            totally_unrelated_xyz "web" {}
        "#,
        );
        assert!(checker.check_body(&b_unrelated, &schema).is_err());
        let schema_bare =
            BodySchema::new().with_block(BlockHeaderSchema::new("bare_block", vec![]));
        let b_bare = parse_body("bare_block {}");
        assert!(checker.check_body(&b_bare, &schema_bare).is_ok());
        let b_inner_err = parse_body(
            r#"
            name = "test"
            server "web" {
                wrong_attr = "oops"
            }
        "#,
        );
        let err_inner = checker.check_body(&b_inner_err, &schema).unwrap_err();
        assert!(err_inner.to_string().contains("is not expected here"));
    }
    #[test]
    fn test_type_checker_dynamic_blocks() {
        let schema = BodySchema::new().with_block(
            BlockHeaderSchema::new("setting", vec!["name".to_string()]).with_body_schema(
                BodySchema::new()
                    .with_attribute(AttributeSchema::required("val").with_type(Type::Number)),
            ),
        );
        let scope = ScopeSchema::new().with_variable("configs", Type::List(Box::new(Type::Number)));
        let checker = TypeChecker::new().with_scope(scope);
        let b_dyn = parse_body(
            r#"
            dynamic "setting" {
                for_each = configs
                labels = ["dynamic_label"]
                content {
                    val = setting.value
                }
            }
        "#,
        );
        assert!(checker.check_body(&b_dyn, &schema).is_ok());
        let b_dyn_err = parse_body(
            r#"
            dynamic "setting" {
                for_each = 123
                labels = ["lbl"]
                content {
                    val = setting.value
                }
            }
        "#,
        );
        assert!(checker.check_body(&b_dyn_err, &schema).is_err());
        let b_dyn_lbls = parse_body(
            r#"
            dynamic "setting" {
                for_each = configs
                labels = ["lbl1", "lbl2"]
                content {
                    val = setting.value
                }
            }
        "#,
        );
        assert!(checker.check_body(&b_dyn_lbls, &schema).is_err());
        let b_dyn_unexp = parse_body(
            r#"
            dynamic "unknown_block" {
                for_each = configs
                content {
                    val = 1
                }
            }
        "#,
        );
        assert!(checker.check_body(&b_dyn_unexp, &schema).is_err());
    }
    /// Tests compatibility between complex types, including tuple length and element mismatches,
    /// and object attributes, types, and optional attributes.
    #[test]
    fn test_is_type_compatible_edge_cases() {
        let tup1 = Type::Tuple(vec![Type::Number, Type::Bool]);
        let tup2 = Type::Tuple(vec![Type::Number, Type::List(Box::new(Type::Number))]);
        assert!(!is_type_compatible(&tup1, &tup2));
        let mut a_attrs = BTreeMap::new();
        a_attrs.insert("a".to_string(), Type::Number);
        let mut b_attrs = BTreeMap::new();
        b_attrs.insert("a".to_string(), Type::List(Box::new(Type::Bool)));
        let obj_a = Type::Object {
            attrs: a_attrs,
            optional_attrs: BTreeSet::new(),
        };
        let obj_b = Type::Object {
            attrs: b_attrs,
            optional_attrs: BTreeSet::new(),
        };
        assert!(!is_type_compatible(&obj_a, &obj_b));
        let mut req_attrs = BTreeMap::new();
        req_attrs.insert("req".to_string(), Type::String);
        let obj_req = Type::Object {
            attrs: req_attrs,
            optional_attrs: BTreeSet::new(),
        };
        assert!(!is_type_compatible(&obj_a, &obj_req));
        let map_bool = Type::Map(Box::new(Type::List(Box::new(Type::Bool))));
        assert!(!is_type_compatible(&obj_a, &map_bool));
        let list_bool = Type::List(Box::new(Type::List(Box::new(Type::Bool))));
        assert!(!is_type_compatible(&tup1, &list_bool));
        assert!(!is_type_compatible(&Type::Number, &Type::Tuple(vec![])));
    }
    /// Tests [`TypeChecker`] public methods including scope access, [`TypeChecker::check_traversal`],
    /// [`TypeChecker::check_func_call`], and [`TypeChecker::check_expression`].
    #[test]
    fn test_type_checker_scope_and_methods() {
        let mut checker = TypeChecker::new();
        checker
            .scope_mut()
            .variables
            .insert("x".to_string(), Type::Number);
        checker.scope_mut().functions.insert(
            "my_fn".to_string(),
            FunctionSignature::with_static_return_type(
                vec![FunctionParamSpec::new("s", Type::String)],
                Type::String,
            ),
        );
        assert_eq!(checker.scope().get_variable("x"), Some(&Type::Number));
        let expr_num = parse_expr("42");
        assert!(checker.check_expression(&expr_num, &Type::Number).is_ok());
        assert!(
            checker
                .check_expression(&expr_num, &Type::List(Box::new(Type::Bool)))
                .is_err()
        );
        let trav_ok = Traversal {
            expr: Box::new(Expression::Variable(
                "x".to_string(),
                Span::new(0, 0, 0, 0, 0, 0),
            )),
            operators: vec![],
        };
        assert!(checker.check_traversal(&trav_ok).is_ok());
        let trav_err = Traversal {
            expr: Box::new(Expression::Variable(
                "missing".to_string(),
                Span::new(0, 0, 0, 0, 0, 0),
            )),
            operators: vec![],
        };
        assert!(checker.check_traversal(&trav_err).is_err());
        let fc_custom = FuncCall {
            name: "my_fn".into(),
            args: vec![Expression::String(
                "HELLO".to_string(),
                Span::new(0, 0, 0, 0, 0, 0),
            )],
            expand_final: false,
        };
        assert!(
            checker
                .check_func_call(&fc_custom, &Span::new(0, 0, 0, 0, 0, 0))
                .is_ok()
        );
        let fc_range = FuncCall {
            name: "range".into(),
            args: vec![Expression::Number(5.into(), Span::new(0, 0, 0, 0, 0, 0))],
            expand_final: false,
        };
        assert!(
            checker
                .check_func_call(&fc_range, &Span::new(0, 0, 0, 0, 0, 0))
                .is_ok()
        );
        let fc_err = FuncCall {
            name: "nonexistent_fn".into(),
            args: vec![],
            expand_final: false,
        };
        assert!(
            checker
                .check_func_call(&fc_err, &Span::new(0, 0, 0, 0, 0, 0))
                .is_err()
        );
    }
    /// Tests dynamic blocks with various collection types, invalid labels, custom iterators,
    /// and blocks without nested body schemas.
    #[test]
    fn test_type_checker_dynamic_blocks_comprehensive() {
        let schema = BodySchema::new()
            .with_block(
                BlockHeaderSchema::new("item", vec!["id".to_string()]).with_body_schema(
                    BodySchema::new()
                        .with_attribute(AttributeSchema::required("val").with_type(Type::Number)),
                ),
            )
            .with_block(BlockHeaderSchema::new("no_schema_block", vec![]));
        let mut scope = ScopeSchema::new();
        scope
            .variables
            .insert("set_coll".to_string(), Type::Set(Box::new(Type::Number)));
        scope
            .variables
            .insert("map_coll".to_string(), Type::Map(Box::new(Type::Number)));
        scope.variables.insert(
            "tup_coll".to_string(),
            Type::Tuple(vec![Type::Number, Type::Number]),
        );
        let mut obj_attrs = BTreeMap::new();
        obj_attrs.insert("k1".to_string(), Type::Number);
        obj_attrs.insert("k2".to_string(), Type::Number);
        scope.variables.insert(
            "obj_coll".to_string(),
            Type::Object {
                attrs: obj_attrs,
                optional_attrs: BTreeSet::new(),
            },
        );
        scope
            .variables
            .insert("dyn_coll".to_string(), Type::Dynamic);
        let checker = TypeChecker::new().with_scope(scope);
        for coll in ["set_coll", "map_coll", "tup_coll", "obj_coll", "dyn_coll"] {
            let body_str = format!(
                r#"
                dynamic "item" {{
                    for_each = {coll}
                    iterator = my_iter
                    labels = ["lbl"]
                    content {{
                        val = my_iter.value
                    }}
                }}
            "#
            );
            let b = parse_body(&body_str);
            assert!(checker.check_body(&b, &schema).is_ok());
        }
        let b_no_schema = parse_body(
            r#"
            dynamic "no_schema_block" {
                for_each = set_coll
                content {}
            }
        "#,
        );
        assert!(checker.check_body(&b_no_schema, &schema).is_ok());
        let b_bad_lbl = parse_body(
            r#"
            dynamic "item" {
                for_each = set_coll
                labels = [[1, 2]]
                content {
                    val = 1
                }
            }
        "#,
        );
        assert!(checker.check_body(&b_bad_lbl, &schema).is_err());
        let b_dyn_typo = parse_body(
            r#"
            dynamic "itim" {
                for_each = set_coll
                content {}
            }
        "#,
        );
        let err_dyn_typo = checker.check_body(&b_dyn_typo, &schema).unwrap_err();
        assert_eq!(
            err_dyn_typo.errors()[0].detail.as_deref(),
            Some("Did you mean \"item\"?")
        );
    }
    /// Tests template directives including conditional if, `else_if`, else, and for directives
    /// with various collection types and error conditions.
    #[test]
    fn test_type_checker_template_directives_comprehensive() {
        let mut scope = ScopeSchema::new();
        scope.variables.insert("bool_val".to_string(), Type::Bool);
        scope.variables.insert("num_val".to_string(), Type::Number);
        scope
            .variables
            .insert("set_coll".to_string(), Type::Set(Box::new(Type::String)));
        scope
            .variables
            .insert("map_coll".to_string(), Type::Map(Box::new(Type::String)));
        scope.variables.insert(
            "tup_coll".to_string(),
            Type::Tuple(vec![Type::String, Type::String]),
        );
        let mut obj_attrs = BTreeMap::new();
        obj_attrs.insert("k".to_string(), Type::String);
        scope.variables.insert(
            "obj_coll".to_string(),
            Type::Object {
                attrs: obj_attrs,
                optional_attrs: BTreeSet::new(),
            },
        );
        scope
            .variables
            .insert("dyn_coll".to_string(), Type::Dynamic);
        let checker = TypeChecker::new().with_scope(scope);
        let b_if = parse_body(
            r#"
            val = "%{if bool_val}yes%{elif !bool_val}no%{else}maybe%{endif}"
        "#,
        );
        assert!(
            checker
                .infer_expression_type(&b_if.attributes["val"].expr)
                .is_ok()
        );
        let b_if_err = parse_body(
            r#"
            val = "%{if [1]}yes%{endif}"
        "#,
        );
        assert!(
            checker
                .infer_expression_type(&b_if_err.attributes["val"].expr)
                .is_err()
        );
        let b_elif_err = parse_body(
            r#"
            val = "%{if bool_val}yes%{elif [1]}no%{endif}"
        "#,
        );
        assert!(
            checker
                .infer_expression_type(&b_elif_err.attributes["val"].expr)
                .is_err()
        );
        for coll in ["set_coll", "map_coll", "tup_coll", "obj_coll", "dyn_coll"] {
            let body_str = format!(r#"val = "%{{for k, v in {coll}}}${{k}}=${{v}} %{{endfor}}""#);
            let b_for = parse_body(&body_str);
            assert!(
                checker
                    .infer_expression_type(&b_for.attributes["val"].expr)
                    .is_ok()
            );
        }
        let b_for_bad = parse_body(
            r#"
            val = "%{for x in 123}${x}%{endfor}"
        "#,
        );
        assert!(
            checker
                .infer_expression_type(&b_for_bad.attributes["val"].expr)
                .is_err()
        );
        let mut diags = Diagnostics::new();
        let span = Span::default();
        let strip_part = crate::ast::expr::TemplatePart::Directive(
            crate::ast::expr::Directive::Strip {
                strip_left: true,
                strip_right: true,
            },
            span,
        );
        checker.check_template_part(&strip_part, checker.scope(), &mut diags);
        assert!(!diags.has_errors());
    }
    /// Tests static type inference for object keys, undefined variables, conditionals,
    /// binary operations, unary operations, and for expressions.
    #[test]
    fn test_type_checker_expressions_comprehensive() {
        let mut scope = ScopeSchema::new();
        scope.variables.insert("x".to_string(), Type::Number);
        scope.variables.insert("b".to_string(), Type::Bool);
        scope.variables.insert("s".to_string(), Type::String);
        let checker = TypeChecker::new().with_scope(scope);
        let b_obj_key_bad = parse_body("val = { [1] = 2 }");
        assert!(
            checker
                .infer_expression_type(&b_obj_key_bad.attributes["val"].expr)
                .is_err()
        );
        let b_obj_key_good = parse_body("val = { (123) = 2 }");
        assert!(
            checker
                .infer_expression_type(&b_obj_key_good.attributes["val"].expr)
                .is_ok()
        );
        let loose_checker =
            TypeChecker::new().with_scope(ScopeSchema::new().with_allow_undefined_variables(true));
        let b_undef = parse_body("val = undef_var");
        let ty_undef = loose_checker.infer_expression_type(&b_undef.attributes["val"].expr);
        assert_eq!(ty_undef.ok(), Some(Type::Dynamic));
        let b_cond_bad_cond = parse_body("val = [1] ? 1 : 2");
        assert!(
            checker
                .infer_expression_type(&b_cond_bad_cond.attributes["val"].expr)
                .is_err()
        );
        let b_cond_incompat = parse_body("val = b ? 1 : [1]");
        assert!(
            checker
                .infer_expression_type(&b_cond_incompat.attributes["val"].expr)
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] + 1"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("1 + [1]"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] - 1"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] * 1"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] / 1"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] % 1"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] < 1"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("1 < [1]"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] <= 1"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] > 1"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] >= 1"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] && true"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("true && [1]"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("[1] || false"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("false || [1]"))
                .is_err()
        );
        let res_eq_warn = checker.infer_expression_type(&parse_expr("true == [1]"));
        assert_eq!(res_eq_warn.ok(), Some(Type::Bool));
        assert!(checker.infer_expression_type(&parse_expr("![1]")).is_err());
        assert!(checker.infer_expression_type(&parse_expr("-[1]")).is_err());
        let span = Span::default();
        let bad_for_coll = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Number(123.into(), span.clone())),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                cond_expr: None,
                grouping: false,
            }),
            span.clone(),
        );
        assert!(checker.infer_expression_type(&bad_for_coll).is_err());
        let bad_for_cond = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Tuple(
                    vec![Expression::Number(1.into(), span.clone())],
                    span.clone(),
                )),
                key_expr: None,
                val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                cond_expr: Some(Box::new(Expression::Tuple(vec![], span.clone()))),
                grouping: false,
            }),
            span.clone(),
        );
        assert!(checker.infer_expression_type(&bad_for_cond).is_err());
        let bad_for_key = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Tuple(
                    vec![Expression::Number(1.into(), span.clone())],
                    span.clone(),
                )),
                key_expr: Some(Box::new(Expression::Tuple(vec![], span.clone()))),
                val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                cond_expr: None,
                grouping: false,
            }),
            span.clone(),
        );
        assert!(checker.infer_expression_type(&bad_for_key).is_err());
        let for_group = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Tuple(
                    vec![Expression::String("a".to_string(), span.clone())],
                    span.clone(),
                )),
                key_expr: Some(Box::new(Expression::Variable(
                    "v".to_string(),
                    span.clone(),
                ))),
                val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                cond_expr: None,
                grouping: true,
            }),
            span.clone(),
        );
        let res_group = checker.infer_expression_type(&for_group);
        assert_eq!(
            res_group.ok(),
            Some(Type::Map(Box::new(Type::List(Box::new(Type::String)))))
        );
        let mut coll_scope = ScopeSchema::new();
        coll_scope
            .variables
            .insert("s".to_string(), Type::Set(Box::new(Type::Number)));
        coll_scope
            .variables
            .insert("m".to_string(), Type::Map(Box::new(Type::Number)));
        coll_scope.variables.insert(
            "t".to_string(),
            Type::Tuple(vec![Type::Number, Type::Number]),
        );
        let mut o_attrs = BTreeMap::new();
        o_attrs.insert("k".to_string(), Type::Number);
        coll_scope.variables.insert(
            "o".to_string(),
            Type::Object {
                attrs: o_attrs,
                optional_attrs: BTreeSet::new(),
            },
        );
        coll_scope.variables.insert("d".to_string(), Type::Dynamic);
        let coll_checker = TypeChecker::new().with_scope(coll_scope);
        for var in ["s", "m", "t", "o", "d"] {
            let for_expr_direct = Expression::ForExpr(
                Box::new(ForExpr {
                    key_var: Some("k".to_string()),
                    val_var: "v".to_string(),
                    collection: Box::new(Expression::Variable(var.to_string(), span.clone())),
                    key_expr: None,
                    val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                    cond_expr: Some(Box::new(Expression::Bool(true, span.clone()))),
                    grouping: false,
                }),
                span.clone(),
            );
            assert!(coll_checker.infer_expression_type(&for_expr_direct).is_ok());
        }
    }
    /// Tests static type checking for `GetAttr`, `Index`, `LegacyIndex`, and splat operators across
    /// Dynamic, Object, Map, List, Set, Tuple, and unsupported types.
    #[test]
    fn test_type_checker_traversals_comprehensive() {
        let mut scope = ScopeSchema::new();
        scope.variables.insert("d".to_string(), Type::Dynamic);
        scope
            .variables
            .insert("m".to_string(), Type::Map(Box::new(Type::Number)));
        let mut o_attrs = BTreeMap::new();
        o_attrs.insert("name".to_string(), Type::String);
        scope.variables.insert(
            "o".to_string(),
            Type::Object {
                attrs: o_attrs,
                optional_attrs: BTreeSet::new(),
            },
        );
        scope
            .variables
            .insert("l".to_string(), Type::List(Box::new(Type::String)));
        scope
            .variables
            .insert("s".to_string(), Type::Set(Box::new(Type::String)));
        scope.variables.insert(
            "t".to_string(),
            Type::Tuple(vec![Type::String, Type::Number]),
        );
        scope.variables.insert("str_key".to_string(), Type::String);
        scope.variables.insert("n".to_string(), Type::Number);
        scope.variables.insert("b".to_string(), Type::Bool);
        let checker = TypeChecker::new().with_scope(scope);
        assert_eq!(
            checker.infer_expression_type(&parse_expr("d.foo")).ok(),
            Some(Type::Dynamic)
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("m.any_key")).ok(),
            Some(Type::Number)
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("o.name")).ok(),
            Some(Type::String)
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("o.missing"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("n.attr"))
                .is_err()
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("d[0]")).ok(),
            Some(Type::Dynamic)
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("l[0]")).ok(),
            Some(Type::String)
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("s[0]")).ok(),
            Some(Type::String)
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("l[[1]]"))
                .is_err()
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("t[0]")).ok(),
            Some(Type::String)
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("t[1]")).ok(),
            Some(Type::Number)
        );
        assert!(checker.infer_expression_type(&parse_expr("t[99]")).is_err());
        assert!(
            checker
                .infer_expression_type(&parse_expr("t[[1]]"))
                .is_err()
        );
        assert!(checker.infer_expression_type(&parse_expr("t[n]")).is_ok());
        let float_trav = Traversal {
            expr: Box::new(Expression::Variable("t".to_string(), Span::default())),
            operators: vec![TraversalOperator::Index(
                Expression::Number(
                    crate::number::Number(bigdecimal::BigDecimal::from(-1)),
                    Span::default(),
                ),
                Span::default(),
            )],
        };
        assert!(checker.check_traversal(&float_trav).is_ok());
        assert_eq!(
            checker.infer_expression_type(&parse_expr(r#"m["k"]"#)).ok(),
            Some(Type::Number)
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("m[[1]]"))
                .is_err()
        );
        assert_eq!(
            checker
                .infer_expression_type(&parse_expr(r#"o["name"]"#))
                .ok(),
            Some(Type::String)
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"o["missing"]"#))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("o[[1]]"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("o[str_key]"))
                .is_ok()
        );
        assert!(checker.infer_expression_type(&parse_expr("b[0]")).is_err());
        assert_eq!(
            checker.infer_expression_type(&parse_expr("d.0")).ok(),
            Some(Type::Dynamic)
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("t.0")).ok(),
            Some(Type::String)
        );
        assert!(checker.infer_expression_type(&parse_expr("t.99")).is_err());
        assert_eq!(
            checker.infer_expression_type(&parse_expr("l.0")).ok(),
            Some(Type::String)
        );
        assert!(checker.infer_expression_type(&parse_expr("n.0")).is_err());
        assert_eq!(
            checker.infer_expression_type(&parse_expr("d.*")).ok(),
            Some(Type::Dynamic)
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("l.*")).ok(),
            Some(Type::List(Box::new(Type::String)))
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("s.*")).ok(),
            Some(Type::List(Box::new(Type::String)))
        );
        assert_eq!(
            checker.infer_expression_type(&parse_expr("t.*")).ok(),
            Some(Type::List(Box::new(Type::String)))
        );
        assert!(checker.infer_expression_type(&parse_expr("n.*")).is_err());
    }
    /// Tests function call type checking including stdlib lookups, variadic and fixed
    /// signatures, dynamic type allowances, return type computation errors, and undefined functions.
    #[test]
    fn test_type_checker_function_calls_comprehensive() {
        use std::sync::Arc;
        let mut scope = ScopeSchema::new();
        let sig_var = FunctionSignature::with_static_return_type(
            vec![
                FunctionParamSpec::new("p1", Type::String).with_allow_dynamic_type(false),
                FunctionParamSpec::new("p2", Type::Number),
            ],
            Type::String,
        )
        .with_variadic(FunctionParamSpec::new(
            "rest",
            Type::Tuple(vec![Type::Number]),
        ));
        scope.functions.insert("my_var_fn".to_string(), sig_var);
        let sig_fixed = FunctionSignature::with_static_return_type(
            vec![
                FunctionParamSpec::new("f1", Type::Tuple(vec![Type::String])),
                FunctionParamSpec::new("f2", Type::Number).with_allow_dynamic_type(false),
            ],
            Type::Number,
        );
        scope.functions.insert("my_fixed_fn".to_string(), sig_fixed);
        let sig_err = FunctionSignature::new(
            vec![],
            Arc::new(|_| Err("failed return type calc".to_string())),
        );
        scope.functions.insert("my_err_fn".to_string(), sig_err);
        scope.variables.insert("dyn_val".to_string(), Type::Dynamic);
        let checker = TypeChecker::new().with_scope(scope);
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"my_var_fn("a")"#))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("my_var_fn(dyn_val, 10)"))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"my_var_fn("a", [1])"#))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"my_var_fn("a", 10, "not_tuple")"#))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"my_var_fn("a", 10, [1], [2])"#))
                .is_ok()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"])"#))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"], 10, 20)"#))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"], dyn_val)"#))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"my_fixed_fn("not_tuple", 10)"#))
                .is_err()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"my_fixed_fn(["a"], 10)"#))
                .is_ok()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("my_err_fn()"))
                .is_err()
        );
        let loose_checker =
            TypeChecker::new().with_scope(ScopeSchema::new().with_allow_undefined_functions(true));
        assert_eq!(
            loose_checker
                .infer_expression_type(&parse_expr("unknown_fn(1, 2)"))
                .ok(),
            Some(Type::Dynamic)
        );
    }
    #[test]
    fn test_type_check_missing_branch_coverage() {
        let mut attr_schemas = std::collections::HashMap::new();
        attr_schemas.insert(
            "any_attr".to_string(),
            AttributeSchema::new("any_attr", false),
        );
        let body_schema = BodySchema {
            attributes: attr_schemas,
            blocks: std::collections::HashMap::new(),
        };
        let mut parser = Parser::new("any_attr = 123\n");
        let body = parser.parse_body();
        let checker = TypeChecker::new();
        let diags = checker.check_body(&body, &body_schema);
        assert!(diags.is_ok());
        let mut scope = ScopeSchema::new();
        scope.variables.insert("dyn_v".to_string(), Type::Dynamic);
        scope
            .variables
            .insert("list_v".to_string(), Type::List(Box::new(Type::Number)));
        scope
            .variables
            .insert("tuple_v".to_string(), Type::Tuple(vec![Type::Number]));
        let sig_var_dyn = FunctionSignature::with_static_return_type(vec![], Type::String)
            .with_variadic(
                FunctionParamSpec::new("rest", Type::Dynamic).with_allow_dynamic_type(true),
            );
        scope
            .functions
            .insert("var_dyn_fn".to_string(), sig_var_dyn);
        let sig_var_str = FunctionSignature::with_static_return_type(vec![], Type::String)
            .with_variadic(
                FunctionParamSpec::new("rest", Type::String).with_allow_dynamic_type(true),
            );
        scope
            .functions
            .insert("var_str_fn".to_string(), sig_var_str);
        let sig_fixed_dyn = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("p1", Type::Dynamic).with_allow_dynamic_type(true)],
            Type::Number,
        );
        scope
            .functions
            .insert("fixed_dyn_fn".to_string(), sig_fixed_dyn);
        let sig_fixed_str = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("p1", Type::String).with_allow_dynamic_type(true)],
            Type::Number,
        );
        scope
            .functions
            .insert("fixed_str_fn".to_string(), sig_fixed_str);
        let checker = TypeChecker::new().with_scope(scope);
        assert!(
            checker
                .infer_expression_type(&parse_expr("dyn_v == 1"))
                .is_ok()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("1 == dyn_v"))
                .is_ok()
        );
        assert!(checker.infer_expression_type(&parse_expr("1 == 2")).is_ok());
        assert!(
            checker
                .infer_expression_type(&parse_expr("list_v == tuple_v"))
                .is_ok()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr(r#"var_dyn_fn(1, "a")"#))
                .is_ok()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("var_str_fn(dyn_v)"))
                .is_ok()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("fixed_dyn_fn(1)"))
                .is_ok()
        );
        assert!(
            checker
                .infer_expression_type(&parse_expr("fixed_str_fn(dyn_v)"))
                .is_ok()
        );
    }

    #[test]
    fn test_type_checker_function_calls_strict_args() {
        let sig_variadic = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("fixed", Type::Number).with_allow_dynamic_type(false)],
            Type::String,
        )
        .with_variadic(FunctionParamSpec::new("var", Type::String).with_allow_dynamic_type(false));

        let sig_fixed = FunctionSignature::with_static_return_type(
            vec![FunctionParamSpec::new("fixed", Type::Number).with_allow_dynamic_type(false)],
            Type::String,
        );

        let scope = ScopeSchema::new()
            .with_function("strict_var_fn", sig_variadic)
            .with_function("strict_fixed_fn", sig_fixed);

        let checker = TypeChecker::new().with_scope(scope);

        let expr_fixed = crate::parse::parser::Parser::new(r#"strict_fixed_fn([1])"#)
            .parse_expression()
            .unwrap();
        let res_fixed = checker.infer_expression_type(&expr_fixed);
        println!("res_fixed: {:?}", res_fixed);
        assert!(res_fixed.is_err());

        let expr_var = crate::parse::parser::Parser::new(r#"strict_var_fn(1, [2])"#)
            .parse_expression()
            .unwrap();
        let res_var = checker.infer_expression_type(&expr_var);
        assert!(res_var.is_err());
    }
}
