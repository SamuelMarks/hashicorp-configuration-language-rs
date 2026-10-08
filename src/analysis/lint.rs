//! Static analysis linter for HCL codebases.
//!
//! Detects dead code, unused local variables, unreferenced block attributes,
//! tautological conditional expressions, unreachable template branches,
//! and redundant type conversions.
use crate::analysis::type_check::TypeChecker;
use crate::ast::deps::extract_static_references_from_body;
use crate::ast::expr::{BinaryOp, Directive, Expression, TemplatePart, TraversalOperator};
use crate::ast::structure::Body;
use crate::diagnostic::{Diagnostic, Diagnostics, Severity};
use crate::error::HclError;
use crate::types::Type;
use std::collections::HashSet;
/// Determines whether two AST expressions are syntactically equivalent literals or variables.
///
/// # Arguments
/// * `a` - The first expression.
/// * `b` - The second expression.
///
/// # Returns
/// `true` if equivalent, `false` otherwise.
fn are_expressions_equivalent(a: &Expression, b: &Expression) -> bool {
    match (a, b) {
        (Expression::Null(_), Expression::Null(_)) => true,
        (Expression::Bool(a_val, _), Expression::Bool(b_val, _)) => a_val == b_val,
        (Expression::Number(a_val, _), Expression::Number(b_val, _)) => a_val == b_val,
        (Expression::String(a_val, _), Expression::String(b_val, _))
        | (Expression::Variable(a_val, _), Expression::Variable(b_val, _)) => a_val == b_val,
        _ => false,
    }
}
/// Static linter checking for dead code, tautologies, and stylistic defects.
#[derive(Debug, Clone, Default)]
pub struct Linter {
    /// Optional type checker used for inferring types in redundant conversion checks.
    pub type_checker: Option<TypeChecker>,
}
impl Linter {
    /// Creates a new `Linter` with default configuration.
    ///
    /// # Returns
    /// A new `Linter`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Configures the `Linter` with an active [`TypeChecker`] to enhance type-aware rules.
    ///
    /// # Arguments
    /// * `type_checker` - The type checker instance.
    ///
    /// # Returns
    /// The updated `Linter`.
    #[must_use]
    pub fn with_type_checker(mut self, type_checker: TypeChecker) -> Self {
        self.type_checker = Some(type_checker);
        self
    }
    /// Lints an entire AST [`Body`], collecting all diagnostic warnings and errors.
    ///
    /// # Arguments
    /// * `body` - The AST body to analyze.
    ///
    /// # Returns
    /// A collection of [`Diagnostics`] containing detected issues.
    #[must_use]
    pub fn lint_body(&self, body: &Body) -> Diagnostics {
        let mut diags = Diagnostics::new();
        self.lint_unused_definitions(body, &mut diags);
        self.lint_body_expressions(body, &mut diags);
        diags
    }
    /// Lints a single isolated [`Expression`].
    ///
    /// # Arguments
    /// * `expr` - The expression to analyze.
    ///
    /// # Returns
    /// A collection of [`Diagnostics`] containing detected issues.
    #[must_use]
    pub fn lint_expression(&self, expr: &Expression) -> Diagnostics {
        let mut diags = Diagnostics::new();
        self.lint_expr_recursive(expr, &mut diags);
        diags
    }
    /// Checks for declared local variables and input variables that are never referenced.
    ///
    /// # Arguments
    /// * `body` - The body to inspect.
    /// * `diags` - The diagnostics list to append warnings to.
    pub fn lint_unused_definitions(&self, body: &Body, diags: &mut Diagnostics) {
        let references = extract_static_references_from_body(body).unwrap_or_default();
        let mut referenced_locals = HashSet::new();
        let mut referenced_vars = HashSet::new();
        for trav in &references {
            if trav.root == "local" {
                if let Some(TraversalOperator::GetAttr(name, _)) = trav.operators.first() {
                    referenced_locals.insert(name.clone());
                }
            } else if trav.root == "var" {
                if let Some(TraversalOperator::GetAttr(name, _)) = trav.operators.first() {
                    referenced_vars.insert(name.clone());
                }
            }
        }
        for block in &body.blocks {
            if block.block_type == "locals" {
                for (name, attr) in &block.body.attributes {
                    if !referenced_locals.contains(name) {
                        let mut diag = Diagnostic::new(
                            HclError::Lint(format!("Unused local variable: {name:?}")),
                            attr.name_span.clone(),
                        );
                        diag.severity = Severity::Warning;
                        diag.summary = Some(format!(
                            "Local variable {name:?} is declared but never used"
                        ));
                        diag.detail = Some(
                            "Consider removing this local variable or referencing it as `local.<name>`."
                                .to_string(),
                        );
                        diags.push(diag);
                    }
                }
            } else if block.block_type == "variable" {
                if let Some(var_name) = block.labels.first() {
                    if !referenced_vars.contains(var_name) {
                        let span = block
                            .label_spans
                            .first()
                            .cloned()
                            .unwrap_or_else(|| block.type_span.clone());
                        let mut diag = Diagnostic::new(
                            HclError::Lint(format!("Unused input variable: {var_name:?}")),
                            span,
                        );
                        diag.severity = Severity::Warning;
                        diag.summary = Some(format!(
                            "Variable {var_name:?} is declared but never referenced"
                        ));
                        diag.detail = Some(
                            "Consider removing this variable or referencing it as `var.<name>`."
                                .to_string(),
                        );
                        diags.push(diag);
                    }
                }
            }
        }
    }
    /// Checks an expression for tautological conditional branches (e.g. `true ? a : b`).
    ///
    /// # Arguments
    /// * `expr` - The expression to inspect.
    /// * `diags` - The diagnostics list to append warnings to.
    pub fn lint_tautologies(&self, expr: &Expression, diags: &mut Diagnostics) {
        if let Expression::Conditional(cond, span) = expr {
            match &cond.cond_expr {
                Expression::Bool(true, _) => {
                    let mut diag = Diagnostic::new(
                        HclError::Lint(
                            "Tautological conditional expression: condition is always true"
                                .to_string(),
                        ),
                        span.clone(),
                    );
                    diag.severity = Severity::Warning;
                    diag.summary = Some("Condition is constant true".to_string());
                    diag.detail = Some(
                        "The false branch is unreachable. Consider replacing this ternary with just the true branch."
                            .to_string(),
                    );
                    diags.push(diag);
                }
                Expression::Bool(false, _) => {
                    let mut diag = Diagnostic::new(
                        HclError::Lint(
                            "Tautological conditional expression: condition is always false"
                                .to_string(),
                        ),
                        span.clone(),
                    );
                    diag.severity = Severity::Warning;
                    diag.summary = Some("Condition is constant false".to_string());
                    diag.detail = Some(
                        "The true branch is unreachable. Consider replacing this ternary with just the false branch."
                            .to_string(),
                    );
                    diags.push(diag);
                }
                Expression::BinaryOp(BinaryOp::Eq, left, right, _)
                    if are_expressions_equivalent(left, right) =>
                {
                    let mut diag = Diagnostic::new(
                        HclError::Lint(
                            "Tautological condition: comparison is always true".to_string(),
                        ),
                        span.clone(),
                    );
                    diag.severity = Severity::Warning;
                    diag.summary = Some("Comparison of identical expressions".to_string());
                    diags.push(diag);
                }
                Expression::BinaryOp(BinaryOp::NotEq, left, right, _)
                    if are_expressions_equivalent(left, right) =>
                {
                    let mut diag = Diagnostic::new(
                        HclError::Lint(
                            "Tautological condition: comparison is always false".to_string(),
                        ),
                        span.clone(),
                    );
                    diag.severity = Severity::Warning;
                    diag.summary = Some("Comparison of identical expressions".to_string());
                    diags.push(diag);
                }
                _ => {}
            }
        }
    }
    /// Checks string template directives for unreachable branches (e.g. `%{ if false }...%{ endif }`).
    ///
    /// # Arguments
    /// * `expr` - The expression to inspect.
    /// * `diags` - The diagnostics list to append warnings to.
    pub fn lint_unreachable_templates(&self, expr: &Expression, diags: &mut Diagnostics) {
        if let Expression::Template(parts, _) = expr {
            for part in parts {
                self.lint_template_part(part, diags);
            }
        }
    }
    fn lint_template_part(&self, part: &TemplatePart, diags: &mut Diagnostics) {
        match part {
            TemplatePart::Literal(_, _) => {}
            TemplatePart::Interpolation(expr, _) => {
                self.lint_expr_recursive(expr, diags);
            }
            TemplatePart::Directive(dir, span) => match dir {
                Directive::If {
                    cond,
                    true_expr,
                    else_ifs,
                    false_expr,
                } => {
                    if let Expression::Bool(false, _) = cond {
                        let mut diag = Diagnostic::new(
                            HclError::Lint(
                                "Unreachable template directive: condition is constant false"
                                    .to_string(),
                            ),
                            span.clone(),
                        );
                        diag.severity = Severity::Warning;
                        diag.summary = Some("Template branch is unreachable".to_string());
                        diag.detail = Some(
                                "The condition `%{ if false }` is never satisfied. This template block will never produce output."
                                    .to_string(),
                            );
                        diags.push(diag);
                    } else if let Expression::Bool(true, _) = cond {
                        if !else_ifs.is_empty() || false_expr.is_some() {
                            let mut diag = Diagnostic::new(
                                    HclError::Lint(
                                        "Unreachable else/elif template branches: initial if is constant true"
                                            .to_string(),
                                    ),
                                    span.clone(),
                                );
                            diag.severity = Severity::Warning;
                            diag.summary =
                                Some("Else / elif template branches are unreachable".to_string());
                            diags.push(diag);
                        }
                    }
                    for (elif_cond, elif_body) in else_ifs {
                        if let Expression::Bool(false, _) = elif_cond {
                            let mut diag = Diagnostic::new(
                                    HclError::Lint(
                                        "Unreachable template else-if branch: condition is constant false"
                                            .to_string(),
                                    ),
                                    elif_cond.span(),
                                );
                            diag.severity = Severity::Warning;
                            diags.push(diag);
                        }
                        for p in elif_body {
                            self.lint_template_part(p, diags);
                        }
                    }
                    for p in true_expr {
                        self.lint_template_part(p, diags);
                    }
                    if let Some(fb) = false_expr {
                        for p in fb {
                            self.lint_template_part(p, diags);
                        }
                    }
                }
                Directive::For {
                    collection, body, ..
                } => {
                    self.lint_expr_recursive(collection, diags);
                    for p in body {
                        self.lint_template_part(p, diags);
                    }
                }
                Directive::Strip { .. } => {}
            },
        }
    }
    /// Checks for redundant standard library type conversions (e.g. `tostring("literal")`).
    ///
    /// # Arguments
    /// * `expr` - The expression to inspect.
    /// * `diags` - The diagnostics list to append warnings to.
    pub fn lint_redundant_conversions(&self, expr: &Expression, diags: &mut Diagnostics) {
        if let Expression::FuncCall(fc, span) = expr {
            if let Some(name) = fc.simple_name() {
                if fc.args.len() == 1 {
                    let arg = &fc.args[0];
                    let inferred = self
                        .type_checker
                        .as_ref()
                        .and_then(|tc| tc.infer_expression_type(arg).ok());
                    match name {
                        "tostring" => {
                            let is_str = matches!(
                                arg,
                                Expression::String(_, _) | Expression::Template(_, _)
                            ) || inferred == Some(Type::String);
                            if is_str {
                                let mut diag = Diagnostic::new(
                                    HclError::Lint(
                                        "Redundant type conversion: argument is already a string"
                                            .to_string(),
                                    ),
                                    span.clone(),
                                );
                                diag.severity = Severity::Warning;
                                diag.summary = Some("Redundant `tostring` call".to_string());
                                diag.detail = Some(
                                    "The target expression is already statically known to be a string. Calling `tostring(...)` is redundant."
                                        .to_string(),
                                );
                                diags.push(diag);
                            }
                        }
                        "tonumber" => {
                            let is_num = matches!(arg, Expression::Number(_, _))
                                || inferred == Some(Type::Number);
                            if is_num {
                                let mut diag = Diagnostic::new(
                                    HclError::Lint(
                                        "Redundant type conversion: argument is already a number"
                                            .to_string(),
                                    ),
                                    span.clone(),
                                );
                                diag.severity = Severity::Warning;
                                diag.summary = Some("Redundant `tonumber` call".to_string());
                                diag.detail = Some(
                                    "The target expression is already statically known to be a number. Calling `tonumber(...)` is redundant."
                                        .to_string(),
                                );
                                diags.push(diag);
                            }
                        }
                        "tobool" => {
                            let is_bool = matches!(arg, Expression::Bool(_, _))
                                || inferred == Some(Type::Bool);
                            if is_bool {
                                let mut diag = Diagnostic::new(
                                    HclError::Lint(
                                        "Redundant type conversion: argument is already a bool"
                                            .to_string(),
                                    ),
                                    span.clone(),
                                );
                                diag.severity = Severity::Warning;
                                diag.summary = Some("Redundant `tobool` call".to_string());
                                diag.detail = Some(
                                    "The target expression is already statically known to be a bool. Calling `tobool(...)` is redundant."
                                        .to_string(),
                                );
                                diags.push(diag);
                            }
                        }
                        "tolist" => {
                            if let Some(Type::List(_)) = inferred {
                                let mut diag = Diagnostic::new(
                                    HclError::Lint(
                                        "Redundant type conversion: argument is already a list"
                                            .to_string(),
                                    ),
                                    span.clone(),
                                );
                                diag.severity = Severity::Warning;
                                diag.summary = Some("Redundant `tolist` call".to_string());
                                diags.push(diag);
                            }
                        }
                        "tomap" => {
                            if let Some(Type::Map(_)) = inferred {
                                let mut diag = Diagnostic::new(
                                    HclError::Lint(
                                        "Redundant type conversion: argument is already a map"
                                            .to_string(),
                                    ),
                                    span.clone(),
                                );
                                diag.severity = Severity::Warning;
                                diag.summary = Some("Redundant `tomap` call".to_string());
                                diags.push(diag);
                            }
                        }
                        "toset" => {
                            if let Some(Type::Set(_)) = inferred {
                                let mut diag = Diagnostic::new(
                                    HclError::Lint(
                                        "Redundant type conversion: argument is already a set"
                                            .to_string(),
                                    ),
                                    span.clone(),
                                );
                                diag.severity = Severity::Warning;
                                diag.summary = Some("Redundant `toset` call".to_string());
                                diags.push(diag);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    fn lint_body_expressions(&self, body: &Body, diags: &mut Diagnostics) {
        for attr in body.attributes.values() {
            self.lint_expr_recursive(&attr.expr, diags);
        }
        for block in &body.blocks {
            self.lint_body_expressions(&block.body, diags);
        }
        for dyn_block in &body.dynamic_blocks {
            self.lint_expr_recursive(&dyn_block.for_each, diags);
            if let Some(ref labels) = dyn_block.labels {
                for lbl in labels {
                    self.lint_expr_recursive(lbl, diags);
                }
            }
            self.lint_body_expressions(&dyn_block.content, diags);
        }
    }
    fn lint_expr_recursive(&self, expr: &Expression, diags: &mut Diagnostics) {
        self.lint_tautologies(expr, diags);
        self.lint_unreachable_templates(expr, diags);
        self.lint_redundant_conversions(expr, diags);
        match expr {
            Expression::Tuple(elems, _) => {
                for el in elems {
                    self.lint_expr_recursive(el, diags);
                }
            }
            Expression::Object(entries, _) => {
                for (k, v) in entries {
                    self.lint_expr_recursive(k, diags);
                    self.lint_expr_recursive(v, diags);
                }
            }
            Expression::Conditional(cond, _) => {
                self.lint_expr_recursive(&cond.cond_expr, diags);
                self.lint_expr_recursive(&cond.true_expr, diags);
                self.lint_expr_recursive(&cond.false_expr, diags);
            }
            Expression::BinaryOp(_, left, right, _) => {
                self.lint_expr_recursive(left, diags);
                self.lint_expr_recursive(right, diags);
            }
            Expression::UnaryOp(_, inner, _) | Expression::Parentheses(inner, _) => {
                self.lint_expr_recursive(inner, diags);
            }
            Expression::FuncCall(fc, _) => {
                for arg in &fc.args {
                    self.lint_expr_recursive(arg, diags);
                }
            }
            Expression::ForExpr(for_expr, _) => {
                self.lint_expr_recursive(&for_expr.collection, diags);
                if let Some(ref k) = for_expr.key_expr {
                    self.lint_expr_recursive(k, diags);
                }
                self.lint_expr_recursive(&for_expr.val_expr, diags);
                if let Some(ref cond) = for_expr.cond_expr {
                    self.lint_expr_recursive(cond, diags);
                }
            }
            Expression::Traversal(trav, _) => {
                self.lint_expr_recursive(&trav.expr, diags);
                for op in &trav.operators {
                    if let TraversalOperator::Index(idx_expr, _) = op {
                        self.lint_expr_recursive(idx_expr, diags);
                    }
                }
            }
            Expression::Null(_)
            | Expression::Bool(_, _)
            | Expression::Number(_, _)
            | Expression::String(_, _)
            | Expression::Variable(_, _)
            | Expression::Template(_, _) => {}
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
    use crate::analysis::type_check::ScopeSchema;
    use crate::parse::parser::Parser;
    fn parse_body(input: &str) -> Body {
        let mut parser = Parser::new(input);
        parser.parse_body()
    }
    #[test]
    fn test_lint_unused_locals_and_variables() {
        let input = r#"
            locals {
                used_local   = "hello"
                unused_local = "dead"
            }

            variable "used_var" {
                default = 10
            }

            variable "unused_var" {
                default = 20
            }

            output = "${local.used_local} - ${var.used_var}"
        "#;
        let body = parse_body(input);
        let linter = Linter::new();
        let diags = linter.lint_body(&body);
        let msgs: Vec<String> = diags.iter().map(|d| d.error.to_string()).collect();
        assert!(msgs.iter().any(|m| m.contains("unused_local")));
        assert!(msgs.iter().any(|m| m.contains("unused_var")));
        assert!(!msgs.iter().any(|m| m.contains("\"used_local\"")));
        assert!(!msgs.iter().any(|m| m.contains("\"used_var\"")));
    }
    #[test]
    fn test_lint_tautological_conditionals() {
        let linter = Linter::new();
        let b1 = parse_body("val = true ? 1 : 2");
        let d1 = linter.lint_body(&b1);
        assert!(
            d1.iter()
                .any(|d| d.summary.as_deref() == Some("Condition is constant true"))
        );
        let b2 = parse_body("val = false ? 1 : 2");
        let d2 = linter.lint_body(&b2);
        assert!(
            d2.iter()
                .any(|d| d.summary.as_deref() == Some("Condition is constant false"))
        );
        let b3 = parse_body("val = 1 == 1 ? 1 : 2");
        let d3 = linter.lint_body(&b3);
        assert!(
            d3.iter()
                .any(|d| d.summary.as_deref() == Some("Comparison of identical expressions"))
        );
        let b4 = parse_body("val = 1 != 1 ? 1 : 2");
        let d4 = linter.lint_body(&b4);
        assert!(
            d4.iter()
                .any(|d| d.summary.as_deref() == Some("Comparison of identical expressions"))
        );
        let b_clean = parse_body("val = count > 0 ? 1 : 2");
        let d_clean = linter.lint_body(&b_clean);
        assert!(!d_clean.has_errors());
        assert_eq!(d_clean.iter().count(), 0);
        let b_neq_diff = parse_body("val = 1 != 2 ? 1 : 2");
        let d_neq_diff = linter.lint_body(&b_neq_diff);
        assert_eq!(d_neq_diff.iter().count(), 0);
    }
    #[test]
    fn test_lint_unreachable_template_directives() {
        let linter = Linter::new();
        let b1 = parse_body(r#"val = "hello %{ if false }unreachable%{ endif } world""#);
        let d1 = linter.lint_body(&b1);
        assert!(d1.iter().any(|d| {
            d.error
                .to_string()
                .contains("Unreachable template directive")
        }));
        let b2 = parse_body(r#"val = "hello %{ if true }always%{ else }never%{ endif }""#);
        let d2 = linter.lint_body(&b2);
        assert!(
            d2.iter()
                .any(|d| d.error.to_string().contains("Unreachable else/elif"))
        );
        let b3 = parse_body(r#"val = "hello %{ if cond }a%{ elif false }never%{ endif }""#);
        let d3 = linter.lint_body(&b3);
        assert!(
            d3.iter()
                .any(|d| d.error.to_string().contains("Unreachable template else-if"))
        );
    }
    #[test]
    fn test_lint_redundant_conversions() {
        let scope = ScopeSchema::new()
            .with_variable("my_list", Type::List(Box::new(Type::Number)))
            .with_variable("my_map", Type::Map(Box::new(Type::String)))
            .with_variable("my_set", Type::Set(Box::new(Type::String)));
        let tc = TypeChecker::new().with_scope(scope);
        let linter = Linter::new().with_type_checker(tc);
        let b_str = parse_body(r#"val = tostring("already string")"#);
        let d_str = linter.lint_body(&b_str);
        assert!(
            d_str
                .iter()
                .any(|d| d.summary.as_deref() == Some("Redundant `tostring` call"))
        );
        let b_num = parse_body("val = tonumber(42)");
        let d_num = linter.lint_body(&b_num);
        assert!(
            d_num
                .iter()
                .any(|d| d.summary.as_deref() == Some("Redundant `tonumber` call"))
        );
        let b_bool = parse_body("val = tobool(true)");
        let d_bool = linter.lint_body(&b_bool);
        assert!(
            d_bool
                .iter()
                .any(|d| d.summary.as_deref() == Some("Redundant `tobool` call"))
        );
        let b_list = parse_body("val = tolist(my_list)");
        let d_list = linter.lint_body(&b_list);
        assert!(
            d_list
                .iter()
                .any(|d| d.summary.as_deref() == Some("Redundant `tolist` call"))
        );
        let b_map = parse_body("val = tomap(my_map)");
        let d_map = linter.lint_body(&b_map);
        assert!(
            d_map
                .iter()
                .any(|d| d.summary.as_deref() == Some("Redundant `tomap` call"))
        );
        let b_set = parse_body("val = toset(my_set)");
        let d_set = linter.lint_body(&b_set);
        assert!(
            d_set
                .iter()
                .any(|d| d.summary.as_deref() == Some("Redundant `toset` call"))
        );
        let b_ok = parse_body("val = tostring(123)");
        let d_ok = linter.lint_body(&b_ok);
        assert_eq!(d_ok.iter().count(), 0);
    }
    #[test]
    fn test_lint_clean_run() {
        let input = r#"
            locals {
                app = "web"
            }

            variable "environment" {
                default = "production"
            }

            resource "server" "primary" {
                name = "${local.app}-${var.environment}"
                enabled = var.environment == "production" ? true : false
            }
        "#;
        let body = parse_body(input);
        let linter = Linter::new();
        let diags = linter.lint_body(&body);
        assert_eq!(diags.iter().count(), 0);
    }
    /// Tests `are_expressions_equivalent` covering all variants and mismatches.
    #[test]
    fn test_are_expressions_equivalent() {
        use crate::span::Span;
        let sp = Span::new(0, 0, 1, 1, 1, 1);
        let null_expr = Expression::Null(sp.clone());
        let bool_true = Expression::Bool(true, sp.clone());
        let bool_false = Expression::Bool(false, sp.clone());
        let num_1 = Expression::Number(crate::number::Number::from(1), sp.clone());
        let num_2 = Expression::Number(crate::number::Number::from(2), sp.clone());
        let str_a = Expression::String("a".to_string(), sp.clone());
        let str_b = Expression::String("b".to_string(), sp.clone());
        let var_x = Expression::Variable("x".to_string(), sp.clone());
        let var_y = Expression::Variable("y".to_string(), sp);
        assert!(are_expressions_equivalent(&null_expr, &null_expr));
        assert!(are_expressions_equivalent(&bool_true, &bool_true));
        assert!(!are_expressions_equivalent(&bool_true, &bool_false));
        assert!(are_expressions_equivalent(&num_1, &num_1));
        assert!(!are_expressions_equivalent(&num_1, &num_2));
        assert!(are_expressions_equivalent(&str_a, &str_a));
        assert!(!are_expressions_equivalent(&str_a, &str_b));
        assert!(are_expressions_equivalent(&var_x, &var_x));
        assert!(!are_expressions_equivalent(&var_x, &var_y));
        assert!(!are_expressions_equivalent(&null_expr, &num_1));
        assert!(!are_expressions_equivalent(&str_a, &var_x));
    }
    /// Tests `lint_expression` API directly and `Linter::default`.
    #[test]
    fn test_lint_expression_api_and_default() {
        let linter = Linter::default();
        let sp = crate::span::Span::new(0, 0, 1, 1, 1, 1);
        let tautology = Expression::Conditional(
            Box::new(crate::ast::expr::Conditional {
                cond_expr: Expression::Bool(true, sp.clone()),
                true_expr: Expression::Number(crate::number::Number::from(1), sp.clone()),
                false_expr: Expression::Number(crate::number::Number::from(2), sp.clone()),
            }),
            sp,
        );
        let diags = linter.lint_expression(&tautology);
        assert_eq!(diags.iter().count(), 1);
    }
    /// Tests unused definition edge cases (index on local/var, variable block without labels, empty `label_spans`).
    #[test]
    fn test_lint_unused_definitions_edge_cases() {
        use crate::ast::structure::Block;
        use crate::span::Span;
        let input = r#"
            locals {
                idx_local = "val"
            }
            variable "idx_var" {
                default = 1
            }
            # Referencing via index instead of GetAttr: local[0] and var[0]
            out = "${local[0]} ${var[0]}"
        "#;
        let mut body = parse_body(input);
        let linter = Linter::new();
        let diags = linter.lint_body(&body);
        assert!(
            diags
                .iter()
                .any(|d| d.error.to_string().contains("idx_local"))
        );
        assert!(
            diags
                .iter()
                .any(|d| d.error.to_string().contains("idx_var"))
        );
        let sp = Span::new(0, 0, 1, 1, 1, 1);
        let mut var_no_labels = Block::new(
            "variable".to_string(),
            vec![],
            Body::new(sp.clone()),
            sp.clone(),
        );
        body.blocks.push(var_no_labels.clone());
        var_no_labels.labels.push("unlabeled_var".to_string());
        var_no_labels.label_spans.clear();
        body.blocks.push(var_no_labels);
        let diags2 = linter.lint_body(&body);
        assert!(
            diags2
                .iter()
                .any(|d| d.error.to_string().contains("unlabeled_var"))
        );
    }
    /// Tests template directives: simple if true, elif true, and `Directive::Strip`.
    #[test]
    fn test_lint_templates_exhaustive() {
        let linter = Linter::new();
        let b1 = parse_body(r#"val = "hello %{ if true }world%{ endif }""#);
        let d1 = linter.lint_body(&b1);
        assert_eq!(d1.iter().count(), 0);
        let b_elif_true = parse_body(r#"val = "hello %{ if true }a%{ elif x }b%{ endif }""#);
        let d_elif_true = linter.lint_body(&b_elif_true);
        assert!(
            d_elif_true
                .iter()
                .any(|d| d.error.to_string().contains("Unreachable else/elif"))
        );
        let b2 =
            parse_body(r#"val = "hello %{ if x }a%{ elif false }never%{ elif y }maybe%{ endif }""#);
        let d2 = linter.lint_body(&b2);
        assert!(
            d2.iter()
                .any(|d| d.error.to_string().contains("Unreachable template else-if"))
        );
        let b3 = parse_body(r#"val = "hello %{ for item in [1, 2] }${item}%{ endfor }""#);
        let d3 = linter.lint_body(&b3);
        assert_eq!(d3.iter().count(), 0);
        let mut b4 = parse_body(r#"val = "text""#);
        let sp = crate::span::Span::new(0, 0, 1, 1, 1, 1);
        let attr = b4.attributes.get_mut("val").unwrap();
        attr.expr = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::Strip {
                    strip_left: true,
                    strip_right: false,
                },
                sp.clone(),
            )],
            sp,
        );
        let d4 = linter.lint_body(&b4);
        assert_eq!(d4.iter().count(), 0);
    }
    /// Tests dynamic blocks and all recursive expression types in `lint_body_expressions`.
    #[test]
    fn test_lint_body_expressions_and_recursive_types() {
        let input = r#"
            dynamic "tag" {
                for_each = [1 == 1 ? 1 : 2]
                labels   = ["env"]
                content {
                    inner = 2 == 2 ? 10 : 20
                }
            }

            dynamic "nolabels" {
                for_each = [1 == 1 ? 1 : 2]
                content {
                    inner = 2 == 2 ? 10 : 20
                }
            }

            tuple_val = [1 == 1 ? 1 : 2]
            obj_val   = { (1 == 1 ? "k" : "k2") = 2 == 2 ? 1 : 2 }
            paren_val = (1 == 1 ? 1 : 2)
            unary_val = -(1 == 1 ? 1 : 2)
            trav_val  = my_arr[1 == 1 ? 0 : 1]
        "#;
        let body = parse_body(input);
        let linter = Linter::new();
        let diags = linter.lint_body(&body);
        assert!(diags.iter().count() >= 6);
        let sp = crate::span::Span::new(0, 0, 1, 1, 1, 1);
        let for_expr_some = Expression::ForExpr(
            Box::new(crate::ast::expr::ForExpr {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(Expression::Number(
                    crate::number::Number::from(1),
                    sp.clone(),
                )),
                key_expr: Some(Box::new(Expression::Conditional(
                    Box::new(crate::ast::expr::Conditional {
                        cond_expr: Expression::Bool(true, sp.clone()),
                        true_expr: Expression::Number(crate::number::Number::from(1), sp.clone()),
                        false_expr: Expression::Number(crate::number::Number::from(2), sp.clone()),
                    }),
                    sp.clone(),
                ))),
                val_expr: Box::new(Expression::Number(
                    crate::number::Number::from(3),
                    sp.clone(),
                )),
                cond_expr: Some(Box::new(Expression::Bool(true, sp.clone()))),
                grouping: false,
            }),
            sp.clone(),
        );
        let d_for_some = linter.lint_expression(&for_expr_some);
        assert!(d_for_some.iter().count() >= 1);
        let for_expr_none = Expression::ForExpr(
            Box::new(crate::ast::expr::ForExpr {
                key_var: None,
                val_var: "v".to_string(),
                collection: Box::new(Expression::Number(
                    crate::number::Number::from(1),
                    sp.clone(),
                )),
                key_expr: None,
                val_expr: Box::new(Expression::Number(
                    crate::number::Number::from(3),
                    sp.clone(),
                )),
                cond_expr: None,
                grouping: false,
            }),
            sp.clone(),
        );
        let d_for_none = linter.lint_expression(&for_expr_none);
        assert_eq!(d_for_none.iter().count(), 0);
        let namespaced_func = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: crate::ast::expr::NamespacedIdent {
                    namespace: vec!["provider".to_string()],
                    name: "call".to_string(),
                    span: sp.clone(),
                },
                args: vec![Expression::Number(
                    crate::number::Number::from(1),
                    sp.clone(),
                )],
                expand_final: false,
            }),
            sp,
        );
        let d_ns = linter.lint_expression(&namespaced_func);
        assert_eq!(d_ns.iter().count(), 0);
    }
    /// Tests redundant conversion edge cases when types are mismatched or functions differ.
    #[test]
    fn test_lint_redundant_conversions_edge_cases() {
        let linter = Linter::new();
        let body = parse_body(
            r#"
            v1 = tostring(123)
            v2 = tonumber("123")
            v3 = tobool("true")
            v4 = tolist("not_list")
            v5 = tomap("not_map")
            v6 = toset("not_set")
            v7 = length([1, 2])
            v8 = concat([1], [2])
        "#,
        );
        let diags = linter.lint_body(&body);
        assert_eq!(diags.iter().count(), 0);
    }
}
