//! Static reference extraction and dependency graph analysis.
//!
//! Provides utilities for extracting all [`AbsTraversal`] references from HCL
//! expressions and bodies, taking into account local iterator bindings from
//! `for` expressions and dynamic blocks, as well as a [`DependencyGraph`]
//! for cycle detection and topological sorting.

use crate::ast::expr::{Directive, Expression, TemplatePart, TraversalOperator};
use crate::ast::structure::{Body, DynamicBlock};
use crate::ast::traversal::{AbsTraversal, abs_traversal_for_expr};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::error::HclError;
use crate::span::Span;
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Extracts all static [`AbsTraversal`] dependencies referenced by an [`Expression`].
///
/// Local iterator variables introduced in `for` expressions (`[for x in coll: x.attr]`)
/// or template `for` directives (`%{ for v in coll }...%{ endfor }`) are recognized
/// as scoped bindings and excluded from external static dependencies.
///
/// # Arguments
/// * `expr` - The root expression to analyze.
///
/// # Errors
/// This function is infallible and returns an empty `Ok` on expressions without references.
pub fn extract_static_references(expr: &Expression) -> Result<Vec<AbsTraversal>, Diagnostics> {
    let mut refs = Vec::new();
    let mut seen = HashSet::new();
    let local_bindings = HashSet::new();

    extract_expr_references(expr, &local_bindings, &mut refs, &mut seen);
    Ok(refs)
}

/// Extracts all static [`AbsTraversal`] dependencies from an entire [`Body`].
///
/// Traverses all attributes, regular blocks, and dynamic blocks. In dynamic blocks,
/// the iterator binding variable is scoped to the `content` body and excluded from
/// external static references.
///
/// # Arguments
/// * `body` - The AST body to analyze.
///
/// # Errors
/// Returns [`Diagnostics`] if reference extraction encounters an error.
pub fn extract_static_references_from_body(body: &Body) -> Result<Vec<AbsTraversal>, Diagnostics> {
    let mut refs = Vec::new();
    let mut seen = HashSet::new();
    let local_bindings = HashSet::new();

    extract_body_references(body, &local_bindings, &mut refs, &mut seen);
    Ok(refs)
}

/// Extracts all function invocations and namespaced function identifiers from an [`Expression`].
///
/// # Arguments
/// * `expr` - The expression to inspect.
///
/// # Examples
/// ```rust
/// use hashicorp_configuration_language_rs::ast::deps::extract_function_references;
/// use hashicorp_configuration_language_rs::ast::expr::{Expression, FuncCall, NamespacedIdent};
/// use hashicorp_configuration_language_rs::span::Span;
///
/// let call = Expression::FuncCall(
///     Box::new(FuncCall {
///         name: NamespacedIdent::parse("provider::aws::arn_parse", Span::new(0, 24, 1, 1, 1, 25)),
///         args: vec![],
///         expand_final: false,
///     }),
///     Span::new(0, 26, 1, 1, 1, 27),
/// );
/// let funcs = extract_function_references(&call);
/// assert_eq!(funcs.len(), 1);
/// assert_eq!(funcs[0].to_string(), "provider::aws::arn_parse");
/// ```
#[must_use]
pub fn extract_function_references(expr: &Expression) -> Vec<crate::ast::expr::NamespacedIdent> {
    let mut funcs = Vec::new();
    struct FuncVisitor<'a>(&'a mut Vec<crate::ast::expr::NamespacedIdent>);
    impl crate::ast::walk::AstVisitor for FuncVisitor<'_> {
        fn visit_func_call(&mut self, func: &crate::ast::expr::FuncCall) -> bool {
            self.0.push(func.name.clone());
            true
        }
    }
    crate::ast::walk::walk_expression(&mut FuncVisitor(&mut funcs), expr);
    funcs
}

fn extract_body_references(
    body: &Body,
    local_bindings: &HashSet<String>,
    refs: &mut Vec<AbsTraversal>,
    seen: &mut HashSet<String>,
) {
    for attr in body.attributes.values() {
        extract_expr_references(&attr.expr, local_bindings, refs, seen);
    }

    for block in &body.blocks {
        extract_body_references(&block.body, local_bindings, refs, seen);
    }

    for dyn_block in &body.dynamic_blocks {
        extract_dynamic_block_references(dyn_block, local_bindings, refs, seen);
    }

    for val in &body.validations {
        extract_expr_references(&val.condition, local_bindings, refs, seen);
        extract_expr_references(&val.error_message, local_bindings, refs, seen);
    }

    for pre in &body.preconditions {
        extract_expr_references(&pre.condition, local_bindings, refs, seen);
        extract_expr_references(&pre.error_message, local_bindings, refs, seen);
    }

    for post in &body.postconditions {
        extract_expr_references(&post.condition, local_bindings, refs, seen);
        extract_expr_references(&post.error_message, local_bindings, refs, seen);
    }
}

fn extract_dynamic_block_references(
    dyn_block: &DynamicBlock,
    local_bindings: &HashSet<String>,
    refs: &mut Vec<AbsTraversal>,
    seen: &mut HashSet<String>,
) {
    // The for_each collection expression is in the outer scope
    extract_expr_references(&dyn_block.for_each, local_bindings, refs, seen);

    if let Some(labels) = &dyn_block.labels {
        for lbl in labels {
            extract_expr_references(lbl, local_bindings, refs, seen);
        }
    }

    // Inside content, the iterator variable is scoped locally
    let iter_name = match dyn_block.iterator.as_deref() {
        Some(name) => name,
        None => dyn_block.block_type.as_str(),
    };

    let mut child_bindings = local_bindings.clone();
    child_bindings.insert(iter_name.to_string());

    extract_body_references(&dyn_block.content, &child_bindings, refs, seen);
}

fn extract_expr_references(
    expr: &Expression,
    local_bindings: &HashSet<String>,
    refs: &mut Vec<AbsTraversal>,
    seen: &mut HashSet<String>,
) {
    match expr {
        Expression::Variable(name, span) => {
            if !local_bindings.contains(name) {
                let abs = AbsTraversal::new(name.clone(), span.clone(), Vec::new(), span.clone());
                let key = abs.to_string();
                if seen.insert(key) {
                    refs.push(abs);
                }
            }
        }

        Expression::Traversal(trav, _) => {
            if let Ok(abs) = abs_traversal_for_expr(expr) {
                if !local_bindings.contains(&abs.root) {
                    let key = abs.to_string();
                    if seen.insert(key) {
                        refs.push(abs);
                    }
                }
            } else {
                extract_expr_references(&trav.expr, local_bindings, refs, seen);
            }

            for op in &trav.operators {
                if let TraversalOperator::Index(idx_expr, _) = op {
                    extract_expr_references(idx_expr, local_bindings, refs, seen);
                }
            }
        }

        Expression::Tuple(elements, _) => {
            for elem in elements {
                extract_expr_references(elem, local_bindings, refs, seen);
            }
        }

        Expression::Object(pairs, _) => {
            for (key, val) in pairs {
                extract_expr_references(key, local_bindings, refs, seen);
                extract_expr_references(val, local_bindings, refs, seen);
            }
        }

        Expression::Template(parts, _) => {
            for part in parts {
                extract_template_part_references(part, local_bindings, refs, seen);
            }
        }

        Expression::FuncCall(fc, _) => {
            for arg in &fc.args {
                extract_expr_references(arg, local_bindings, refs, seen);
            }
        }

        Expression::Conditional(cond, _) => {
            extract_expr_references(&cond.cond_expr, local_bindings, refs, seen);
            extract_expr_references(&cond.true_expr, local_bindings, refs, seen);
            extract_expr_references(&cond.false_expr, local_bindings, refs, seen);
        }

        Expression::BinaryOp(_, left, right, _) => {
            extract_expr_references(left, local_bindings, refs, seen);
            extract_expr_references(right, local_bindings, refs, seen);
        }

        Expression::UnaryOp(_, inner, _) | Expression::Parentheses(inner, _) => {
            extract_expr_references(inner, local_bindings, refs, seen);
        }

        Expression::ForExpr(fe, _) => {
            extract_expr_references(&fe.collection, local_bindings, refs, seen);

            let mut child_bindings = local_bindings.clone();
            child_bindings.insert(fe.val_var.clone());
            if let Some(key_var) = &fe.key_var {
                child_bindings.insert(key_var.clone());
            }

            if let Some(key_expr) = &fe.key_expr {
                extract_expr_references(key_expr, &child_bindings, refs, seen);
            }
            extract_expr_references(&fe.val_expr, &child_bindings, refs, seen);
            if let Some(cond_expr) = &fe.cond_expr {
                extract_expr_references(cond_expr, &child_bindings, refs, seen);
            }
        }

        Expression::Null(_)
        | Expression::Bool(_, _)
        | Expression::Number(_, _)
        | Expression::String(_, _) => {}
    }
}

fn extract_template_part_references(
    part: &TemplatePart,
    local_bindings: &HashSet<String>,
    refs: &mut Vec<AbsTraversal>,
    seen: &mut HashSet<String>,
) {
    match part {
        TemplatePart::Literal(_, _) => {}
        TemplatePart::Interpolation(expr, _) => {
            extract_expr_references(expr, local_bindings, refs, seen);
        }
        TemplatePart::Directive(directive, _) => match directive {
            Directive::If {
                cond,
                true_expr,
                else_ifs,
                false_expr,
            } => {
                extract_expr_references(cond, local_bindings, refs, seen);
                for p in true_expr {
                    extract_template_part_references(p, local_bindings, refs, seen);
                }
                for (else_cond, else_parts) in else_ifs {
                    extract_expr_references(else_cond, local_bindings, refs, seen);
                    for p in else_parts {
                        extract_template_part_references(p, local_bindings, refs, seen);
                    }
                }
                if let Some(false_parts) = false_expr {
                    for p in false_parts {
                        extract_template_part_references(p, local_bindings, refs, seen);
                    }
                }
            }
            Directive::For {
                key_var,
                val_var,
                collection,
                body,
            } => {
                extract_expr_references(collection, local_bindings, refs, seen);

                let mut child_bindings = local_bindings.clone();
                child_bindings.insert(val_var.clone());
                if let Some(k) = key_var {
                    child_bindings.insert(k.clone());
                }

                for p in body {
                    extract_template_part_references(p, &child_bindings, refs, seen);
                }
            }
            Directive::Strip { .. } => {}
        },
    }
}

/// A directed graph representing dependencies between symbols, blocks, or variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyGraph {
    nodes: BTreeSet<String>,
    dependencies: BTreeMap<String, BTreeSet<String>>,
    dependents: BTreeMap<String, BTreeSet<String>>,
}

impl DependencyGraph {
    /// Creates a new, empty `DependencyGraph`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node identifier to the graph.
    ///
    /// # Arguments
    /// * `node` - The identifier of the node.
    pub fn add_node(&mut self, node: impl Into<String>) {
        let n = node.into();
        self.dependencies.entry(n.clone()).or_default();
        self.dependents.entry(n.clone()).or_default();
        self.nodes.insert(n);
    }

    /// Adds a directed dependency edge indicating that `from` depends on `to`.
    ///
    /// # Arguments
    /// * `from` - The dependent node identifier.
    /// * `to` - The dependency node identifier.
    pub fn add_edge(&mut self, from: impl Into<String>, to: impl Into<String>) {
        let f = from.into();
        let t = to.into();

        self.add_node(f.clone());
        self.add_node(t.clone());

        self.dependencies
            .entry(f.clone())
            .or_default()
            .insert(t.clone());
        self.dependents.entry(t).or_default().insert(f);
    }

    /// Checks if a node exists in the dependency graph.
    #[must_use]
    pub fn has_node(&self, node: &str) -> bool {
        self.nodes.contains(node)
    }

    /// Returns the immediate dependencies of a node.
    #[must_use]
    pub fn dependencies(&self, node: &str) -> Option<Vec<String>> {
        self.dependencies
            .get(node)
            .map(|deps| deps.iter().cloned().collect())
    }

    /// Returns the immediate dependents that rely on a node.
    #[must_use]
    pub fn dependents(&self, node: &str) -> Vec<String> {
        match self.dependents.get(node) {
            Some(deps) => deps.iter().cloned().collect(),
            None => Vec::new(),
        }
    }

    /// Detects if there are any cycles in the dependency graph.
    ///
    /// Returns `Some(cycle)` containing the ordered sequence of nodes forming
    /// the cycle (e.g. `["a", "b", "c", "a"]`), or `None` if the graph is acyclic.
    #[must_use]
    pub fn detect_cycles(&self) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut on_stack = HashSet::new();
        let mut path = Vec::new();

        for node in &self.nodes {
            if !visited.contains(node)
                && let Some(cycle) = self.dfs_cycle(node, &mut visited, &mut on_stack, &mut path)
            {
                return Some(cycle);
            }
        }

        None
    }

    fn dfs_cycle(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        on_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node.to_string());
        on_stack.insert(node.to_string());
        path.push(node.to_string());

        for dep in self.dependencies.get(node).into_iter().flatten() {
            if on_stack.contains(dep) {
                let mut cycle: Vec<String> =
                    path.iter().skip_while(|&n| n != dep).cloned().collect();
                cycle.push(dep.clone());
                return Some(cycle);
            }

            if !visited.contains(dep)
                && let Some(cycle) = self.dfs_cycle(dep, visited, on_stack, path)
            {
                return Some(cycle);
            }
        }

        path.pop();
        on_stack.remove(node);
        None
    }

    /// Produces a topologically sorted sequence of nodes in dependency-order
    /// (independent nodes first, followed by dependents).
    ///
    /// # Errors
    /// Returns [`Diagnostics`] containing [`HclError::CyclicDependency`] if a cycle is detected.
    pub fn topological_sort(&self) -> Result<Vec<String>, Diagnostics> {
        if let Some(cycle) = self.detect_cycles() {
            let diag = Diagnostic::new(
                HclError::CyclicDependency(format!("Cycle detected: {}", cycle.join(" -> "))),
                Span::new(0, 0, 0, 0, 0, 0),
            );
            return Err(Diagnostics::from(diag));
        }

        let mut in_degrees: BTreeMap<String, usize> = BTreeMap::new();
        for node in &self.nodes {
            let deg = self.dependencies.get(node).map_or(0, BTreeSet::len);
            in_degrees.insert(node.clone(), deg);
        }

        let mut queue: Vec<String> = in_degrees
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(k, _)| k.clone())
            .collect();

        let mut result = Vec::new();

        while let Some(node) = queue.pop() {
            result.push(node.clone());

            for dependent in self.dependents.get(&node).into_iter().flatten() {
                let deg = in_degrees.entry(dependent.clone()).or_default();
                *deg = deg.saturating_sub(1);
                if *deg == 0 {
                    queue.push(dependent.clone());
                }
            }
        }

        Ok(result)
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
    use crate::ast::expr::{BinaryOp, ForExpr, Traversal};

    #[test]
    fn test_extract_static_references_simple_and_nested() {
        let span = Span::new(0, 10, 1, 1, 1, 11);

        // a + b.c[d]
        let trav_b = Traversal {
            expr: Box::new(Expression::Variable("b".to_string(), span.clone())),
            operators: vec![
                TraversalOperator::GetAttr("c".to_string(), span.clone()),
                TraversalOperator::Index(
                    Expression::Variable("d".to_string(), span.clone()),
                    span.clone(),
                ),
            ],
        };

        let expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Variable("a".to_string(), span.clone())),
            Box::new(Expression::Traversal(Box::new(trav_b), span.clone())),
            span,
        );

        let refs = extract_static_references(&expr).unwrap();
        let ref_strings: Vec<String> = refs.iter().map(ToString::to_string).collect();

        assert_eq!(ref_strings, vec!["a", "b.c[...]", "d"]);
    }

    #[test]
    fn test_extract_static_references_for_expr_scopes() {
        let span = Span::new(0, 10, 1, 1, 1, 11);

        // [for k, v in items: external_var + v.id if k != filter_val]
        let for_expr = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(Expression::Variable("items".to_string(), span.clone())),
                key_expr: None,
                val_expr: Box::new(Expression::BinaryOp(
                    BinaryOp::Add,
                    Box::new(Expression::Variable(
                        "external_var".to_string(),
                        span.clone(),
                    )),
                    Box::new(Expression::Traversal(
                        Box::new(Traversal {
                            expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                            operators: vec![TraversalOperator::GetAttr(
                                "id".to_string(),
                                span.clone(),
                            )],
                        }),
                        span.clone(),
                    )),
                    span.clone(),
                )),
                cond_expr: Some(Box::new(Expression::BinaryOp(
                    BinaryOp::NotEq,
                    Box::new(Expression::Variable("k".to_string(), span.clone())),
                    Box::new(Expression::Variable("filter_val".to_string(), span.clone())),
                    span.clone(),
                ))),
                grouping: false,
            }),
            span,
        );

        let refs = extract_static_references(&for_expr).unwrap();
        let ref_strings: Vec<String> = refs.iter().map(ToString::to_string).collect();

        // Should include `items`, `external_var`, and `filter_val`, but NOT `v` or `k`
        assert_eq!(ref_strings, vec!["items", "external_var", "filter_val"]);
    }

    #[test]
    fn test_extract_static_references_template_directives() {
        let span = Span::new(0, 10, 1, 1, 1, 11);

        // %{ for item in source_list }${ external_prefix }-${ item }%{ endfor }
        let tpl = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::For {
                    key_var: None,
                    val_var: "item".to_string(),
                    collection: Expression::Variable("source_list".to_string(), span.clone()),
                    body: vec![
                        TemplatePart::Interpolation(
                            Expression::Variable("external_prefix".to_string(), span.clone()),
                            span.clone(),
                        ),
                        TemplatePart::Interpolation(
                            Expression::Variable("item".to_string(), span.clone()),
                            span.clone(),
                        ),
                    ],
                },
                span.clone(),
            )],
            span,
        );

        let refs = extract_static_references(&tpl).unwrap();
        let ref_strings: Vec<String> = refs.iter().map(ToString::to_string).collect();

        assert_eq!(ref_strings, vec!["source_list", "external_prefix"]);
    }

    #[test]
    fn test_extract_static_references_dynamic_blocks() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut body = Body::new(span.clone());

        let mut content = Body::new(span.clone());
        content.attributes.insert(
            "attr".to_string(),
            crate::ast::structure::Attribute {
                name: "attr".to_string(),
                name_span: span.clone(),
                equals_span: span.clone(),
                expr: Expression::BinaryOp(
                    BinaryOp::Add,
                    Box::new(Expression::Traversal(
                        Box::new(Traversal {
                            expr: Box::new(Expression::Variable("tag".to_string(), span.clone())),
                            operators: vec![TraversalOperator::GetAttr(
                                "value".to_string(),
                                span.clone(),
                            )],
                        }),
                        span.clone(),
                    )),
                    Box::new(Expression::Variable(
                        "global_setting".to_string(),
                        span.clone(),
                    )),
                    span.clone(),
                ),
                span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );

        let dyn_block = DynamicBlock::new(
            "tag".to_string(),
            Expression::Variable("tag_list".to_string(), span.clone()),
            Some("tag".to_string()),
            Some(vec![Expression::Variable(
                "tag_label".to_string(),
                span.clone(),
            )]),
            content,
            span.clone(),
            span.clone(),
        );
        body.dynamic_blocks.push(dyn_block);

        let dyn_block_no_labels = DynamicBlock::new(
            "tag2".to_string(),
            Expression::Variable("tag_list2".to_string(), span.clone()),
            None,
            None,
            Body::new(span.clone()),
            span.clone(),
            span,
        );
        body.dynamic_blocks.push(dyn_block_no_labels);

        let refs = extract_static_references_from_body(&body).unwrap();
        let ref_strings: Vec<String> = refs.iter().map(ToString::to_string).collect();

        // `tag` is the dynamic block iterator name, so it should not appear as an external reference
        assert_eq!(
            ref_strings,
            vec!["tag_list", "tag_label", "global_setting", "tag_list2"]
        );
    }

    #[test]
    fn test_dependency_graph_acyclic_and_topological_sort() {
        let mut graph = DependencyGraph::new();
        graph.add_edge("a", "sub_a");
        graph.add_edge("c", "b");
        graph.add_edge("b", "a");

        assert!(graph.has_node("a"));
        assert!(graph.has_node("b"));
        assert!(graph.has_node("c"));
        assert!(!graph.has_node("d"));

        assert_eq!(graph.dependencies("c"), Some(vec!["b".to_string()]));
        assert_eq!(graph.dependencies("d"), None);
        assert_eq!(graph.dependents("a"), vec!["b".to_string()]);
        assert_eq!(graph.dependents("z"), Vec::<String>::new());

        assert!(graph.detect_cycles().is_none());

        let sorted = graph.topological_sort().unwrap();
        assert_eq!(sorted, vec!["sub_a", "a", "b", "c"]);
    }

    #[test]
    fn test_dependency_graph_cycle_detection() {
        let mut graph = DependencyGraph::new();
        graph.add_edge("start", "a");
        graph.add_edge("a", "dead_end");
        graph.add_edge("a", "b");
        graph.add_edge("b", "c");
        graph.add_edge("c", "a");

        let cycle = graph.detect_cycles().unwrap();
        assert!(cycle.len() >= 3);
        assert_eq!(cycle.first(), cycle.last());
        assert!(!cycle.contains(&"start".to_string()));

        let res = graph.topological_sort();
        assert!(res.is_err());
        let err_str = format!("{res:?}");
        assert!(err_str.contains("CyclicDependency"));
    }

    #[test]
    fn test_extract_static_references_validations_and_assertions() {
        use crate::ast::structure::{PostconditionBlock, PreconditionBlock, ValidationBlock};

        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut body = Body::new(span.clone());

        body.validations.push(ValidationBlock::new(
            Expression::Variable("var_a".to_string(), span.clone()),
            Expression::Variable("msg_a".to_string(), span.clone()),
            span.clone(),
        ));
        body.preconditions.push(PreconditionBlock::new(
            Expression::Variable("var_b".to_string(), span.clone()),
            Expression::Variable("msg_b".to_string(), span.clone()),
            span.clone(),
        ));
        body.postconditions.push(PostconditionBlock::new(
            Expression::Variable("var_c".to_string(), span.clone()),
            Expression::Variable("msg_c".to_string(), span.clone()),
            span,
        ));

        let refs = extract_static_references_from_body(&body).unwrap();
        let ref_strings: Vec<String> = refs.iter().map(ToString::to_string).collect();
        assert_eq!(
            ref_strings,
            vec!["var_a", "msg_a", "var_b", "msg_b", "var_c", "msg_c"]
        );
    }

    #[test]
    fn test_extract_static_references_more_expr_types() {
        use crate::ast::expr::{Conditional, FuncCall, UnaryOp};
        use crate::ast::structure::Block;

        let span = Span::new(0, 10, 1, 1, 1, 11);

        // 1. Nested regular block in body
        let mut body = Body::new(span.clone());
        let mut inner_body = Body::new(span.clone());
        inner_body.attributes.insert(
            "attr".to_string(),
            crate::ast::structure::Attribute {
                name: "attr".to_string(),
                name_span: span.clone(),
                equals_span: span.clone(),
                expr: Expression::Variable("block_var".to_string(), span.clone()),
                span: span.clone(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        body.blocks.push(Block {
            block_type: "my_block".to_string(),
            type_span: span.clone(),
            labels: vec![],
            label_spans: vec![],
            body: inner_body,
            span: span.clone(),
            open_brace_span: span.clone(),
            close_brace_span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        });

        let refs_body = extract_static_references_from_body(&body).unwrap();
        assert_eq!(
            refs_body
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["block_var"]
        );

        // 2. Traversal whose root expr is not a simple variable (abs traversal fails)
        let non_abs_trav = Expression::Traversal(
            Box::new(Traversal {
                expr: Box::new(Expression::Parentheses(
                    Box::new(Expression::BinaryOp(
                        BinaryOp::Add,
                        Box::new(Expression::Variable("lhs".to_string(), span.clone())),
                        Box::new(Expression::Variable("rhs".to_string(), span.clone())),
                        span.clone(),
                    )),
                    span.clone(),
                )),
                operators: vec![TraversalOperator::GetAttr(
                    "field".to_string(),
                    span.clone(),
                )],
            }),
            span.clone(),
        );
        let refs_trav = extract_static_references(&non_abs_trav).unwrap();
        assert_eq!(
            refs_trav
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["lhs", "rhs"]
        );

        // 2b. Duplicate traversal reference to cover seen.insert returning false
        let trav_dup = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::Traversal(
                Box::new(Traversal {
                    expr: Box::new(Expression::Variable("dup_trav".to_string(), span.clone())),
                    operators: vec![TraversalOperator::GetAttr(
                        "field".to_string(),
                        span.clone(),
                    )],
                }),
                span.clone(),
            )),
            Box::new(Expression::Traversal(
                Box::new(Traversal {
                    expr: Box::new(Expression::Variable("dup_trav".to_string(), span.clone())),
                    operators: vec![TraversalOperator::GetAttr(
                        "field".to_string(),
                        span.clone(),
                    )],
                }),
                span.clone(),
            )),
            span.clone(),
        );
        let refs_dup = extract_static_references(&trav_dup).unwrap();
        assert_eq!(refs_dup.len(), 1);

        // 3. Tuple, Object, FuncCall, Conditional, UnaryOp, Parentheses, Literals
        let complex_expr = Expression::Tuple(
            vec![
                Expression::Object(
                    vec![(
                        Expression::Variable("obj_k".to_string(), span.clone()),
                        Expression::Variable("obj_v".to_string(), span.clone()),
                    )],
                    span.clone(),
                ),
                Expression::FuncCall(
                    Box::new(FuncCall {
                        name: "func".into(),
                        args: vec![Expression::Variable("fn_arg".to_string(), span.clone())],
                        expand_final: false,
                    }),
                    span.clone(),
                ),
                Expression::Conditional(
                    Box::new(Conditional {
                        cond_expr: Expression::Variable("c_cond".to_string(), span.clone()),
                        true_expr: Expression::Variable("c_true".to_string(), span.clone()),
                        false_expr: Expression::Variable("c_false".to_string(), span.clone()),
                    }),
                    span.clone(),
                ),
                Expression::UnaryOp(
                    UnaryOp::Not,
                    Box::new(Expression::Variable("u_var".to_string(), span.clone())),
                    span.clone(),
                ),
                Expression::Null(span.clone()),
                Expression::Bool(true, span.clone()),
                Expression::Number(crate::number::Number::from(1), span.clone()),
                Expression::String("literal".to_string(), span.clone()),
            ],
            span.clone(),
        );
        let refs_complex = extract_static_references(&complex_expr).unwrap();
        assert_eq!(
            refs_complex
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec![
                "obj_k", "obj_v", "fn_arg", "c_cond", "c_true", "c_false", "u_var"
            ]
        );

        // 4. ForExpr with key_expr and cond_expr
        let for_expr = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Box::new(Expression::Variable("for_coll".to_string(), span.clone())),
                key_expr: Some(Box::new(Expression::Variable(
                    "for_k_extra".to_string(),
                    span.clone(),
                ))),
                val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                cond_expr: Some(Box::new(Expression::Variable(
                    "for_cond_extra".to_string(),
                    span.clone(),
                ))),
                grouping: false,
            }),
            span.clone(),
        );
        let refs_for = extract_static_references(&for_expr).unwrap();
        assert_eq!(
            refs_for.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["for_coll", "for_k_extra", "for_cond_extra"]
        );

        // 4b. ForExpr without key_var, without key_expr, and without cond_expr
        // and with duplicate variable/traversal references + local traversal reference
        let for_expr_simple = Expression::ForExpr(
            Box::new(ForExpr {
                key_var: None,
                val_var: "item".to_string(),
                collection: Box::new(Expression::Variable("for_coll".to_string(), span.clone())), // duplicate
                key_expr: None,
                val_expr: Box::new(Expression::BinaryOp(
                    BinaryOp::Add,
                    Box::new(Expression::Traversal(
                        Box::new(Traversal {
                            expr: Box::new(Expression::Variable("item".to_string(), span.clone())), // local binding traversal
                            operators: vec![TraversalOperator::GetAttr(
                                "id".to_string(),
                                span.clone(),
                            )],
                        }),
                        span.clone(),
                    )),
                    Box::new(Expression::BinaryOp(
                        BinaryOp::Add,
                        Box::new(Expression::Variable("dup_var".to_string(), span.clone())),
                        Box::new(Expression::Variable("dup_var".to_string(), span.clone())), // duplicate
                        span.clone(),
                    )),
                    span.clone(),
                )),
                cond_expr: None,
                grouping: false,
            }),
            span.clone(),
        );
        let refs_for_simple = extract_static_references(&for_expr_simple).unwrap();
        assert_eq!(
            refs_for_simple
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["for_coll", "dup_var"]
        );

        // 5. Template with Directive::If (with true, else_if, and false parts) and Literal
        let tpl_if = Expression::Template(
            vec![
                TemplatePart::Literal("lit".to_string(), span.clone()),
                TemplatePart::Directive(
                    Directive::If {
                        cond: Expression::Variable("if_cond".to_string(), span.clone()),
                        true_expr: vec![TemplatePart::Interpolation(
                            Expression::Variable("if_true".to_string(), span.clone()),
                            span.clone(),
                        )],
                        else_ifs: vec![(
                            Expression::Variable("if_elif_cond".to_string(), span.clone()),
                            vec![TemplatePart::Interpolation(
                                Expression::Variable("if_elif_body".to_string(), span.clone()),
                                span.clone(),
                            )],
                        )],
                        false_expr: Some(vec![TemplatePart::Interpolation(
                            Expression::Variable("if_false".to_string(), span.clone()),
                            span.clone(),
                        )]),
                    },
                    span.clone(),
                ),
                TemplatePart::Directive(
                    Directive::If {
                        cond: Expression::Variable("if_cond2".to_string(), span.clone()),
                        true_expr: vec![],
                        else_ifs: vec![],
                        false_expr: None, // without false parts
                    },
                    span.clone(),
                ),
                TemplatePart::Directive(
                    Directive::Strip {
                        strip_left: true,
                        strip_right: true,
                    },
                    span.clone(),
                ),
            ],
            span.clone(),
        );
        let refs_tpl = extract_static_references(&tpl_if).unwrap();
        assert_eq!(
            refs_tpl.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec![
                "if_cond",
                "if_true",
                "if_elif_cond",
                "if_elif_body",
                "if_false",
                "if_cond2"
            ]
        );

        // 6. Template with Directive::For with key_var
        let tpl_for = Expression::Template(
            vec![TemplatePart::Directive(
                Directive::For {
                    key_var: Some("k".to_string()),
                    val_var: "v".to_string(),
                    collection: Expression::Variable("tpl_coll".to_string(), span.clone()),
                    body: vec![TemplatePart::Interpolation(
                        Expression::Variable("tpl_body_extra".to_string(), span.clone()),
                        span.clone(),
                    )],
                },
                span.clone(),
            )],
            span,
        );
        let refs_tpl_for = extract_static_references(&tpl_for).unwrap();
        assert_eq!(
            refs_tpl_for
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["tpl_coll", "tpl_body_extra"]
        );
    }

    #[test]
    fn test_dependency_graph_diamond_and_disconnected() {
        let mut graph = DependencyGraph::new();
        // Diamond: d depends on b and c, b depends on a, c depends on a
        graph.add_edge("d", "b");
        graph.add_edge("d", "c");
        graph.add_edge("b", "a");
        graph.add_edge("c", "a");

        // Disconnected component
        graph.add_edge("y", "x");

        // Isolated node
        graph.add_node("solo");

        assert!(graph.detect_cycles().is_none());

        let sorted = graph.topological_sort().unwrap();
        assert_eq!(sorted.len(), 7);
        let mut pos_map = std::collections::HashMap::new();
        for (i, node) in sorted.iter().enumerate() {
            pos_map.insert(node.as_str(), i);
        }
        // "a" must precede "b", "c", "d"
        assert!(pos_map["a"] < pos_map["b"]);
        assert!(pos_map["a"] < pos_map["c"]);
        assert!(pos_map["b"] < pos_map["d"]);
        assert!(pos_map["c"] < pos_map["d"]);
        assert!(pos_map["x"] < pos_map["y"]);
    }

    #[test]
    fn test_extract_function_references() {
        let expr = Expression::FuncCall(
            Box::new(crate::ast::expr::FuncCall {
                name: crate::ast::expr::NamespacedIdent::from("provider::aws::arn_parse"),
                args: vec![Expression::String(
                    "arn:aws:...".to_string(),
                    Span::default(),
                )],
                expand_final: false,
            }),
            Span::default(),
        );
        let funcs = extract_function_references(&expr);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].to_string(), "provider::aws::arn_parse");
    }
}
