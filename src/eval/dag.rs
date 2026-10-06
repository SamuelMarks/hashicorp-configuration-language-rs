//! Topological DAG Dependency Resolver for HCL Definitions.
//!
//! Resolves interdependent `variable` default values, CLI overrides, and `locals`
//! definitions in deterministic topological dependency order, detecting circular
//! dependency cycles and evaluating variable validation blocks.
use crate::ast::deps::{DependencyGraph, extract_static_references};
use crate::ast::expr::Expression;
use crate::ast::structure::{Body, ValidationBlock};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::eval::context::Context;
use crate::eval::evaluator::Evaluator;
use crate::eval::validation::{evaluate_postcondition, evaluate_precondition};
use crate::span::Span;
use crate::types::Value;
use std::collections::HashMap;
/// Metadata for a declared variable.
#[derive(Debug, Clone)]
struct VarDeclaration {
    /// Default value expression if specified.
    default_expr: Option<Expression>,
    /// Validation blocks associated with this variable.
    validations: Vec<ValidationBlock>,
}
/// Metadata for a declared local value.
#[derive(Debug, Clone)]
struct LocalDeclaration {
    /// Expression computing the local value.
    expr: Expression,
}
/// Topological DAG dependency resolver for configuration definitions.
#[derive(Debug, Default, Clone)]
pub struct DagResolver {
    /// Explicit variable overrides (e.g. from CLI `-var` or `-var-file`).
    pub var_overrides: HashMap<String, Value>,
}
impl DagResolver {
    /// Creates a new `DagResolver` with no initial overrides.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Sets explicit variable overrides.
    ///
    /// # Arguments
    /// * `overrides` - Map of variable name to overridden [`Value`].
    #[must_use]
    pub fn with_var_overrides(mut self, overrides: HashMap<String, Value>) -> Self {
        self.var_overrides = overrides;
        self
    }
    /// Resolves all `variable` and `locals` declarations from `body` into `ctx`.
    ///
    /// Performs static dependency analysis, builds a directed acyclic graph (DAG),
    /// checks for cycles, evaluates expressions in valid dependency order, and
    /// validates variables against their `validation` rules.
    ///
    /// # Arguments
    /// * `body` - The merged AST body containing `variable` and `locals` blocks.
    /// * `ctx` - The context to populate with evaluated `var.*` and `local.*` variables.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if a circular dependency is detected, if expression evaluation
    /// fails, or if any variable validation rule is violated.
    pub fn resolve(&self, body: &Body, ctx: &mut Context<'_>) -> Result<(), Diagnostics> {
        let mut var_decls: HashMap<String, VarDeclaration> = HashMap::new();
        let mut local_decls: HashMap<String, LocalDeclaration> = HashMap::new();
        let mut node_spans: HashMap<String, Span> = HashMap::new();
        for block in &body.blocks {
            if block.block_type == "variable" {
                if let Some(var_name) = block.labels.first() {
                    let default_expr = block.body.attributes.get("default").map(|a| a.expr.clone());
                    let validations = block.body.validations.clone();
                    let node_id = format!("var.{var_name}");
                    node_spans.insert(node_id, block.span.clone());
                    var_decls.insert(
                        var_name.clone(),
                        VarDeclaration {
                            default_expr,
                            validations,
                        },
                    );
                }
            } else if block.block_type == "locals" {
                for (local_name, attr) in &block.body.attributes {
                    let node_id = format!("local.{local_name}");
                    node_spans.insert(node_id, attr.span.clone());
                    local_decls.insert(
                        local_name.clone(),
                        LocalDeclaration {
                            expr: attr.expr.clone(),
                        },
                    );
                }
            }
        }
        let mut dag = DependencyGraph::new();
        for var_name in var_decls.keys() {
            dag.add_node(format!("var.{var_name}"));
        }
        for local_name in local_decls.keys() {
            dag.add_node(format!("local.{local_name}"));
        }
        for (var_name, decl) in &var_decls {
            let from_node = format!("var.{var_name}");
            if !self.var_overrides.contains_key(var_name) {
                if let Some(ref expr) = decl.default_expr {
                    let refs = extract_static_references(expr).unwrap_or_default();
                    for r in refs {
                        let ref_str = r.to_string();
                        if let Some(dep_var) = ref_str.strip_prefix("var.") {
                            let dep_name = dep_var.split('.').next().unwrap_or(dep_var);
                            dag.add_edge(&from_node, format!("var.{dep_name}"));
                        } else if let Some(dep_local) = ref_str.strip_prefix("local.") {
                            let dep_name = dep_local.split('.').next().unwrap_or(dep_local);
                            dag.add_edge(&from_node, format!("local.{dep_name}"));
                        }
                    }
                }
            }
        }
        for (local_name, decl) in &local_decls {
            let from_node = format!("local.{local_name}");
            let refs = extract_static_references(&decl.expr).unwrap_or_default();
            for r in refs {
                let ref_str = r.to_string();
                if let Some(dep_var) = ref_str.strip_prefix("var.") {
                    let dep_name = dep_var.split('.').next().unwrap_or(dep_var);
                    dag.add_edge(&from_node, format!("var.{dep_name}"));
                } else if let Some(dep_local) = ref_str.strip_prefix("local.") {
                    let dep_name = dep_local.split('.').next().unwrap_or(dep_local);
                    dag.add_edge(&from_node, format!("local.{dep_name}"));
                }
            }
        }
        let Ok(sorted_nodes) = dag.topological_sort() else {
            let cycle = dag.detect_cycles().unwrap_or_default();
            let cycle_path = cycle.join(" -> ");
            let span = cycle
                .first()
                .and_then(|node| node_spans.get(node))
                .cloned()
                .unwrap_or(Span::new(0, 0, 1, 1, 1, 1));
            let mut diags = Diagnostics::new();
            diags.push(
                Diagnostic::error(
                    "Circular dependency detected",
                    format!("Cycle path: {cycle_path}"),
                    span,
                )
                .with_error(crate::error::HclError::CyclicDependency(format!(
                    "Circular dependency: {cycle_path}"
                ))),
            );
            return Err(diags);
        };
        let mut diags = Diagnostics::new();
        for node in sorted_nodes {
            if let Some(var_name) = node.strip_prefix("var.") {
                if let Some(decl) = var_decls.get(var_name) {
                    let evaluated_val = if let Some(override_val) = self.var_overrides.get(var_name)
                    {
                        override_val.clone()
                    } else if let Some(ref expr) = decl.default_expr {
                        let eval = Evaluator::new(ctx);
                        match eval.evaluate(expr) {
                            Ok((v, eval_diags)) => {
                                diags.extend(eval_diags);
                                v
                            }
                            Err(e) => {
                                diags.extend(e);
                                continue;
                            }
                        }
                    } else {
                        Value::unknown(crate::types::Type::Dynamic)
                    };
                    ctx.set_var(var_name, evaluated_val);
                    for val_block in &decl.validations {
                        let eval = Evaluator::new(ctx);
                        match eval.evaluate(&val_block.condition) {
                            Ok((cond_val, _)) => {
                                let passed = match &*cond_val.data {
                                    crate::types::ValueData::Bool(b) => *b,
                                    _ => false,
                                };
                                if !passed {
                                    let sub_evaluator = Evaluator::new(ctx);
                                    let err_msg =
                                        match sub_evaluator.evaluate(&val_block.error_message) {
                                            Ok((text_val, _)) => {
                                                text_val.to_string().trim_matches('"').to_string()
                                            }
                                            Err(_) => {
                                                "Variable validation assertion failed".to_string()
                                            }
                                        };
                                    diags.push(
                                        Diagnostic::error(
                                            format!("Validation failed for variable '{var_name}'"),
                                            err_msg.clone(),
                                            val_block.span.clone(),
                                        )
                                        .with_error(crate::error::HclError::Validation(err_msg)),
                                    );
                                }
                            }
                            Err(e) => {
                                diags.extend(e);
                            }
                        }
                    }
                }
            } else {
                let local_name = node.strip_prefix("local.").unwrap_or(&node);
                if let Some(decl) = local_decls.get(local_name) {
                    let eval = Evaluator::new(ctx);
                    match eval.evaluate(&decl.expr) {
                        Ok((v, eval_diags)) => {
                            diags.extend(eval_diags);
                            ctx.set_local(local_name, v);
                        }
                        Err(e) => {
                            diags.extend(e);
                        }
                    }
                }
            }
        }
        for pre in &body.preconditions {
            if let Err(pre_diags) = evaluate_precondition(pre, ctx) {
                diags.extend(pre_diags);
            }
        }
        for post in &body.postconditions {
            if let Err(post_diags) = evaluate_postcondition(post, ctx) {
                diags.extend(post_diags);
            }
        }
        if diags.has_errors() {
            Err(diags)
        } else {
            Ok(())
        }
    }
}
/// Convenience function to resolve `variable` and `locals` definitions from `body` into `ctx`.
///
/// # Arguments
/// * `body` - The AST body containing definitions.
/// * `ctx` - The context to populate.
///
/// # Errors
/// Returns [`Diagnostics`] if resolution fails due to cycles, invalid expressions, or failed validations.
pub fn resolve_definitions(body: &Body, ctx: &mut Context<'_>) -> Result<(), Diagnostics> {
    DagResolver::new().resolve(body, ctx)
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
    use crate::api::parse;
    #[test]
    fn test_dag_resolver_acyclic_and_overrides() {
        let src = r#"
            variable "region" {
                default = "us-east-1"
            }

            variable "zone" {
                default = "${var.region}a"
            }

            variable "instance_count" {
                default = 3
                validation {
                    condition     = var.instance_count > 0
                    error_message = "instance_count must be positive"
                }
            }

            locals {
                name_prefix = "my-cluster"
                full_name   = "${local.name_prefix}-${var.region}"
                total_nodes = var.instance_count * 2
            }

            variable "cluster_domain" {
                default = "${local.name_prefix}.internal"
            }
        "#;
        let body = parse(src).unwrap();
        let mut ctx = Context::new();
        ctx.stdlib_enabled = true;
        resolve_definitions(&body, &mut ctx).unwrap();
        let var_obj = ctx.get_variable("var").unwrap();
        let local_obj = ctx.get_variable("local").unwrap();
        assert!(var_obj.to_string().contains("us-east-1"));
        assert!(local_obj.to_string().contains("my-cluster-us-east-1"));
        let mut overrides = HashMap::new();
        overrides.insert(
            "region".to_string(),
            Value::new(
                crate::types::Type::String,
                crate::types::ValueData::String("eu-west-1".to_string()),
            ),
        );
        let mut ctx2 = Context::new();
        ctx2.stdlib_enabled = true;
        DagResolver::new()
            .with_var_overrides(overrides)
            .resolve(&body, &mut ctx2)
            .unwrap();
        let local_obj2 = ctx2.get_variable("local").unwrap();
        assert!(local_obj2.to_string().contains("my-cluster-eu-west-1"));
        let bad_src = r#"
            variable "bad_default" {
                default = 10 / 0
            }
        "#;
        let bad_body = parse(bad_src).unwrap();
        let mut bad_ctx = Context::new();
        assert!(resolve_definitions(&bad_body, &mut bad_ctx).is_err());
    }
    #[test]
    fn test_dag_resolver_circular_dependency() {
        let src = r"
            locals {
                a = local.b + 1
                b = local.a + 1
            }
        ";
        let body = parse(src).unwrap();
        let mut ctx = Context::new();
        let errs = resolve_definitions(&body, &mut ctx).err().unwrap();
        assert_eq!(errs.errors().len(), 1);
        let diag = &errs.errors()[0];
        assert!(diag.error.to_string().contains("Circular dependency"));
        assert!(diag.detail.as_deref().unwrap_or("").contains("local.a"));
        assert!(diag.detail.as_deref().unwrap_or("").contains("local.b"));
    }
    #[test]
    fn test_dag_resolver_validation_failure() {
        let src = r#"
            variable "port" {
                default = 80
                validation {
                    condition     = var.port > 1024
                    error_message = "Port must be non-privileged (> 1024)"
                }
            }
        "#;
        let body = parse(src).unwrap();
        let mut ctx = Context::new();
        let errs = resolve_definitions(&body, &mut ctx).err().unwrap();
        assert_eq!(errs.errors().len(), 1);
        let diag = &errs.errors()[0];
        assert!(
            diag.summary
                .as_deref()
                .unwrap_or("")
                .contains("Validation failed")
        );
        assert!(
            diag.detail
                .as_deref()
                .unwrap_or("")
                .contains("Port must be non-privileged")
        );
    }
    #[test]
    fn test_dag_resolver_undefined_and_errors() {
        let src = r#"
            variable "no_default" {}

            variable "dep_on_local" {
                default = local.base_val
            }

            locals {
                base_val = "ok"
                bad_expr = 10 / 0
            }

            precondition {
                condition     = false
                error_message = "precondition failed"
            }

            postcondition {
                condition     = false
                error_message = "postcondition failed"
            }
        "#;
        let body = parse(src).unwrap();
        let mut ctx = Context::new();
        let errs = resolve_definitions(&body, &mut ctx).err().unwrap();
        assert!(errs.has_errors());
        let var_val = ctx.get_variable("var").unwrap();
        assert!(var_val.to_string().contains("no_default"));
    }
    #[test]
    fn test_dag_resolver_validation_non_bool_and_msg_error() {
        let src = r#"
            variable "bad_val" {
                default = 10
                validation {
                    condition     = "not a boolean"
                    error_message = 10 / 0
                }
            }
        "#;
        let body = parse(src).unwrap();
        let mut ctx = Context::new();
        let errs = resolve_definitions(&body, &mut ctx).err().unwrap();
        assert!(errs.has_errors());
    }
    #[test]
    fn test_dag_resolver_coverage_gaps() {
        let src = r#"
            output "out1" {
                value = "hello"
            }

            locals {
                pure_local = 42
                cfg = {
                    "mode" = "fast"
                }
                derived_local = local.cfg.mode
            }

            variable "settings" {
                default = {
                    "timeout" = 30
                }
            }

            variable "client_timeout" {
                default = var.settings.timeout
            }

            variable "standalone_num" {
                default = 100
            }

            variable "with_path_ref" {
                default = "${path.root}/file"
            }

            locals {
                with_path_ref_local = "${path.cwd}/dir"
            }

            precondition {
                condition     = true
                error_message = "should not fail"
            }

            postcondition {
                condition     = true
                error_message = "should not fail"
            }
        "#;
        let mut body = parse(src).unwrap();
        body.blocks.push(crate::ast::structure::Block {
            block_type: "variable".to_string(),
            labels: Vec::new(),
            body: Body::new(Span::new(0, 0, 1, 1, 1, 1)),
            span: Span::new(0, 0, 1, 1, 1, 1),
            type_span: Span::new(0, 0, 1, 1, 1, 1),
            label_spans: Vec::new(),
            open_brace_span: Span::new(0, 0, 1, 1, 1, 1),
            close_brace_span: Span::new(0, 0, 1, 1, 1, 1),
            leading_comments: Vec::new(),
            trailing_comment: None,
        });
        let mut ctx = Context::new();
        ctx.stdlib_enabled = true;
        ctx.set_path_scopes("/root", "/cwd");
        let res = resolve_definitions(&body, &mut ctx);
        assert!(res.is_ok());
        let cond_err_src = r#"
            variable "validated_err" {
                default = 5
                validation {
                    condition     = 10 / 0 > 0
                    error_message = "unreachable"
                }
            }
        "#;
        let cond_err_body = parse(cond_err_src).unwrap();
        let mut cond_err_ctx = Context::new();
        let cond_err_res = resolve_definitions(&cond_err_body, &mut cond_err_ctx);
        assert!(cond_err_res.is_err());
        let resolver = DagResolver::new();
        let cloned_resolver = resolver.clone();
        let debug_str = format!("{cloned_resolver:?}");
        assert!(debug_str.contains("DagResolver"));
    }
    #[test]
    fn test_dag_resolver_missing_references() {
        let src = r#"
            variable "ref_missing" {
                default = var.missing_variable
            }

            locals {
                ref_missing_local = local.missing_local_value
            }
        "#;
        let body = parse(src).unwrap();
        let mut ctx = Context::new();
        let res = resolve_definitions(&body, &mut ctx);
        assert!(res.is_err());
    }
}
