//! AST Walker and Visitor infrastructure.
//!
//! Provides the [`AstVisitor`] trait for read-only AST inspection and the
//! [`AstFolder`] trait for AST transformation and rewriting without evaluation.

use crate::ast::expr::{Directive, Expression, TemplatePart, Traversal, TraversalOperator};
use crate::ast::structure::{
    Attribute, Block, Body, DynamicBlock, PostconditionBlock, PreconditionBlock, ValidationBlock,
};

/// A visitor trait for read-only AST inspection without evaluation.
///
/// Each method returns a boolean indicating whether the walker should continue
/// descending into the children of the current node (`true`), or skip children (`false`).
pub trait AstVisitor {
    /// Visits an expression node. Return `true` to descend into sub-expressions.
    fn visit_expression(&mut self, _expr: &Expression) -> bool {
        true
    }

    /// Visits a string template part. Return `true` to descend into nested expressions/directives.
    fn visit_template_part(&mut self, _part: &TemplatePart) -> bool {
        true
    }

    /// Visits a traversal node. Return `true` to descend into the base expression and index sub-expressions.
    fn visit_traversal(&mut self, _trav: &Traversal) -> bool {
        true
    }

    /// Visits a function call. Return `true` to descend into arguments.
    fn visit_func_call(&mut self, _func: &crate::ast::expr::FuncCall) -> bool {
        true
    }

    /// Visits a namespaced identifier.
    fn visit_namespaced_ident(&mut self, _ident: &crate::ast::expr::NamespacedIdent) {}

    /// Visits an attribute. Return `true` to descend into the attribute's value expression.
    fn visit_attribute(&mut self, _attr: &Attribute) -> bool {
        true
    }

    /// Visits a block. Return `true` to descend into the block's inner body.
    fn visit_block(&mut self, _block: &Block) -> bool {
        true
    }

    /// Visits a dynamic block. Return `true` to descend into `for_each`, labels, and `content`.
    fn visit_dynamic_block(&mut self, _dyn_block: &DynamicBlock) -> bool {
        true
    }

    /// Visits a validation block. Return `true` to descend into condition and `error_message` expressions.
    fn visit_validation_block(&mut self, _val: &ValidationBlock) -> bool {
        true
    }

    /// Visits a precondition block. Return `true` to descend into condition and `error_message` expressions.
    fn visit_precondition_block(&mut self, _pre: &PreconditionBlock) -> bool {
        true
    }

    /// Visits a postcondition block. Return `true` to descend into condition and `error_message` expressions.
    fn visit_postcondition_block(&mut self, _post: &PostconditionBlock) -> bool {
        true
    }

    /// Visits an AST body. Return `true` to descend into its attributes, blocks, and dynamic blocks.
    fn visit_body(&mut self, _body: &Body) -> bool {
        true
    }
}

/// Recursively walks an AST [`Expression`] using the provided visitor.
///
/// # Arguments
/// * `visitor` - The visitor to invoke on each AST node.
/// * `expr` - The root expression to walk.
pub fn walk_expression<V: AstVisitor>(visitor: &mut V, expr: &Expression) {
    walk_expression_dyn(visitor, expr);
}

fn walk_expression_dyn(visitor: &mut dyn AstVisitor, expr: &Expression) {
    if !visitor.visit_expression(expr) {
        return;
    }

    match expr {
        Expression::Null(_)
        | Expression::Bool(_, _)
        | Expression::Number(_, _)
        | Expression::String(_, _)
        | Expression::Variable(_, _) => {}

        Expression::Tuple(elements, _) => {
            for elem in elements {
                walk_expression_dyn(visitor, elem);
            }
        }

        Expression::Object(pairs, _) => {
            for (key, val) in pairs {
                walk_expression_dyn(visitor, key);
                walk_expression_dyn(visitor, val);
            }
        }

        Expression::Template(parts, _) => {
            for part in parts {
                walk_template_part_dyn(visitor, part);
            }
        }

        Expression::Traversal(trav, _) => {
            if visitor.visit_traversal(trav) {
                walk_expression_dyn(visitor, &trav.expr);
                for op in &trav.operators {
                    if let TraversalOperator::Index(idx_expr, _) = op {
                        walk_expression_dyn(visitor, idx_expr);
                    }
                }
            }
        }

        Expression::FuncCall(fc, _) => {
            visitor.visit_namespaced_ident(&fc.name);
            if visitor.visit_func_call(fc) {
                for arg in &fc.args {
                    walk_expression_dyn(visitor, arg);
                }
            }
        }

        Expression::Conditional(cond, _) => {
            walk_expression_dyn(visitor, &cond.cond_expr);
            walk_expression_dyn(visitor, &cond.true_expr);
            walk_expression_dyn(visitor, &cond.false_expr);
        }

        Expression::BinaryOp(_, left, right, _) => {
            walk_expression_dyn(visitor, left);
            walk_expression_dyn(visitor, right);
        }

        Expression::UnaryOp(_, inner, _) | Expression::Parentheses(inner, _) => {
            walk_expression_dyn(visitor, inner);
        }

        Expression::ForExpr(for_expr, _) => {
            walk_expression_dyn(visitor, &for_expr.collection);
            if let Some(key_expr) = &for_expr.key_expr {
                walk_expression_dyn(visitor, key_expr);
            }
            walk_expression_dyn(visitor, &for_expr.val_expr);
            if let Some(cond_expr) = &for_expr.cond_expr {
                walk_expression_dyn(visitor, cond_expr);
            }
        }
    }
}

/// Recursively walks a [`TemplatePart`] using the provided visitor.
///
/// # Arguments
/// * `visitor` - The visitor to invoke on each AST node.
/// * `part` - The template part to walk.
pub fn walk_template_part<V: AstVisitor>(visitor: &mut V, part: &TemplatePart) {
    walk_template_part_dyn(visitor, part);
}

fn walk_template_part_dyn(visitor: &mut dyn AstVisitor, part: &TemplatePart) {
    if !visitor.visit_template_part(part) {
        return;
    }

    match part {
        TemplatePart::Literal(_, _) => {}
        TemplatePart::Interpolation(expr, _) => {
            walk_expression_dyn(visitor, expr);
        }
        TemplatePart::Directive(directive, _) => match directive {
            Directive::If {
                cond,
                true_expr,
                else_ifs,
                false_expr,
            } => {
                walk_expression_dyn(visitor, cond);
                for p in true_expr {
                    walk_template_part_dyn(visitor, p);
                }
                for (else_cond, else_parts) in else_ifs {
                    walk_expression_dyn(visitor, else_cond);
                    for p in else_parts {
                        walk_template_part_dyn(visitor, p);
                    }
                }
                if let Some(false_parts) = false_expr {
                    for p in false_parts {
                        walk_template_part_dyn(visitor, p);
                    }
                }
            }
            Directive::For {
                collection, body, ..
            } => {
                walk_expression_dyn(visitor, collection);
                for p in body {
                    walk_template_part_dyn(visitor, p);
                }
            }
            Directive::Strip { .. } => {}
        },
    }
}

/// Recursively walks an [`Attribute`] using the provided visitor.
///
/// # Arguments
/// * `visitor` - The visitor to invoke.
/// * `attr` - The attribute to walk.
pub fn walk_attribute<V: AstVisitor>(visitor: &mut V, attr: &Attribute) {
    walk_attribute_dyn(visitor, attr);
}

fn walk_attribute_dyn(visitor: &mut dyn AstVisitor, attr: &Attribute) {
    if visitor.visit_attribute(attr) {
        walk_expression_dyn(visitor, &attr.expr);
    }
}

/// Recursively walks a [`Block`] using the provided visitor.
///
/// # Arguments
/// * `visitor` - The visitor to invoke.
/// * `block` - The block to walk.
pub fn walk_block<V: AstVisitor>(visitor: &mut V, block: &Block) {
    walk_block_dyn(visitor, block);
}

fn walk_block_dyn(visitor: &mut dyn AstVisitor, block: &Block) {
    if visitor.visit_block(block) {
        walk_body_dyn(visitor, &block.body);
    }
}

/// Recursively walks a [`DynamicBlock`] using the provided visitor.
///
/// # Arguments
/// * `visitor` - The visitor to invoke.
/// * `dyn_block` - The dynamic block to walk.
pub fn walk_dynamic_block<V: AstVisitor>(visitor: &mut V, dyn_block: &DynamicBlock) {
    walk_dynamic_block_dyn(visitor, dyn_block);
}

fn walk_dynamic_block_dyn(visitor: &mut dyn AstVisitor, dyn_block: &DynamicBlock) {
    if visitor.visit_dynamic_block(dyn_block) {
        walk_expression_dyn(visitor, &dyn_block.for_each);
        if let Some(labels) = &dyn_block.labels {
            for lbl in labels {
                walk_expression_dyn(visitor, lbl);
            }
        }
        walk_body_dyn(visitor, &dyn_block.content);
    }
}

/// Recursively walks a [`ValidationBlock`] using the provided visitor.
///
/// # Arguments
/// * `visitor` - The visitor to invoke.
/// * `val` - The validation block to walk.
pub fn walk_validation_block<V: AstVisitor>(visitor: &mut V, val: &ValidationBlock) {
    walk_validation_block_dyn(visitor, val);
}

fn walk_validation_block_dyn(visitor: &mut dyn AstVisitor, val: &ValidationBlock) {
    if visitor.visit_validation_block(val) {
        walk_expression_dyn(visitor, &val.condition);
        walk_expression_dyn(visitor, &val.error_message);
    }
}

/// Recursively walks a [`PreconditionBlock`] using the provided visitor.
///
/// # Arguments
/// * `visitor` - The visitor to invoke.
/// * `pre` - The precondition block to walk.
pub fn walk_precondition_block<V: AstVisitor>(visitor: &mut V, pre: &PreconditionBlock) {
    walk_precondition_block_dyn(visitor, pre);
}

fn walk_precondition_block_dyn(visitor: &mut dyn AstVisitor, pre: &PreconditionBlock) {
    if visitor.visit_precondition_block(pre) {
        walk_expression_dyn(visitor, &pre.condition);
        walk_expression_dyn(visitor, &pre.error_message);
    }
}

/// Recursively walks a [`PostconditionBlock`] using the provided visitor.
///
/// # Arguments
/// * `visitor` - The visitor to invoke.
/// * `post` - The postcondition block to walk.
pub fn walk_postcondition_block<V: AstVisitor>(visitor: &mut V, post: &PostconditionBlock) {
    walk_postcondition_block_dyn(visitor, post);
}

fn walk_postcondition_block_dyn(visitor: &mut dyn AstVisitor, post: &PostconditionBlock) {
    if visitor.visit_postcondition_block(post) {
        walk_expression_dyn(visitor, &post.condition);
        walk_expression_dyn(visitor, &post.error_message);
    }
}

/// Recursively walks an entire [`Body`] using the provided visitor.
///
/// # Arguments
/// * `visitor` - The visitor to invoke.
/// * `body` - The body to walk.
pub fn walk_body<V: AstVisitor>(visitor: &mut V, body: &Body) {
    walk_body_dyn(visitor, body);
}

fn walk_body_dyn(visitor: &mut dyn AstVisitor, body: &Body) {
    if !visitor.visit_body(body) {
        return;
    }

    for attr in body.attributes.values() {
        walk_attribute_dyn(visitor, attr);
    }

    for block in &body.blocks {
        walk_block_dyn(visitor, block);
    }

    for dyn_block in &body.dynamic_blocks {
        walk_dynamic_block_dyn(visitor, dyn_block);
    }

    for val in &body.validations {
        walk_validation_block_dyn(visitor, val);
    }

    for pre in &body.preconditions {
        walk_precondition_block_dyn(visitor, pre);
    }

    for post in &body.postconditions {
        walk_postcondition_block_dyn(visitor, post);
    }
}

/// A trait for mutating and rewriting AST nodes.
pub trait AstFolder: Sized {
    /// Transforms an expression. Default recursively transforms child expressions.
    fn fold_expression(&mut self, expr: Expression) -> Expression {
        walk_fold_expression(self, expr)
    }

    /// Transforms a traversal. Default recursively transforms base and index expressions.
    fn fold_traversal(&mut self, trav: Traversal) -> Traversal {
        walk_fold_traversal(self, trav)
    }

    /// Transforms a namespaced identifier.
    fn fold_namespaced_ident(
        &mut self,
        ident: crate::ast::expr::NamespacedIdent,
    ) -> crate::ast::expr::NamespacedIdent {
        ident
    }

    /// Transforms a function call.
    fn fold_func_call(&mut self, func: crate::ast::expr::FuncCall) -> crate::ast::expr::FuncCall {
        func
    }

    /// Transforms a template part. Default recursively transforms inner expressions.
    fn fold_template_part(&mut self, part: TemplatePart) -> TemplatePart {
        walk_fold_template_part(self, part)
    }

    /// Transforms an attribute. Default recursively transforms the attribute value.
    fn fold_attribute(&mut self, attr: Attribute) -> Attribute {
        walk_fold_attribute(self, attr)
    }

    /// Transforms a block. Default recursively transforms the block body.
    fn fold_block(&mut self, block: Block) -> Block {
        walk_fold_block(self, block)
    }

    /// Transforms a dynamic block. Default recursively transforms `for_each`, labels, and content.
    fn fold_dynamic_block(&mut self, dyn_block: DynamicBlock) -> DynamicBlock {
        walk_fold_dynamic_block(self, dyn_block)
    }

    /// Transforms a validation block. Default recursively transforms condition and `error_message` expressions.
    fn fold_validation_block(&mut self, val: ValidationBlock) -> ValidationBlock {
        walk_fold_validation_block(self, val)
    }

    /// Transforms a precondition block. Default recursively transforms condition and `error_message` expressions.
    fn fold_precondition_block(&mut self, pre: PreconditionBlock) -> PreconditionBlock {
        walk_fold_precondition_block(self, pre)
    }

    /// Transforms a postcondition block. Default recursively transforms condition and `error_message` expressions.
    fn fold_postcondition_block(&mut self, post: PostconditionBlock) -> PostconditionBlock {
        walk_fold_postcondition_block(self, post)
    }

    /// Transforms an entire body. Default recursively transforms attributes, blocks, dynamic blocks, and validations.
    fn fold_body(&mut self, body: Body) -> Body {
        walk_fold_body(self, body)
    }
}

/// Recursively transforms an [`Expression`] using the given folder.
///
/// # Arguments
/// * `folder` - The folder performing the transformation.
/// * `expr` - The expression to transform.
#[must_use]
#[rustfmt::skip]
pub fn walk_fold_expression<F: AstFolder>(folder: &mut F, expr: Expression) -> Expression {
    match expr {
        Expression::Null(_) | Expression::Bool(_, _) | Expression::Number(_, _) | Expression::String(_, _) | Expression::Variable(_, _) => expr,

        Expression::Tuple(elements, span) => {
            let folded_elements = elements
                .into_iter()
                .map(|e| folder.fold_expression(e))
                .collect();
            Expression::Tuple(folded_elements, span)
        }

        Expression::Object(pairs, span) => {
            let folded_pairs = pairs
                .into_iter()
                .map(|(k, v)| (folder.fold_expression(k), folder.fold_expression(v)))
                .collect();
            Expression::Object(folded_pairs, span)
        }

        Expression::Template(parts, span) => {
            let folded_parts = parts
                .into_iter()
                .map(|p| folder.fold_template_part(p))
                .collect();
            Expression::Template(folded_parts, span)
        }

        Expression::Traversal(trav, span) => {
            let folded_trav = folder.fold_traversal(*trav);
            Expression::Traversal(Box::new(folded_trav), span)
        }

        Expression::FuncCall(fc, span) => {
            let mut fc = folder.fold_func_call(*fc);
            fc.name = folder.fold_namespaced_ident(fc.name);
            fc.args = fc
                .args
                .into_iter()
                .map(|a| folder.fold_expression(a))
                .collect();
            Expression::FuncCall(Box::new(fc), span)
        }

        Expression::Conditional(mut cond, span) => {
            cond.cond_expr = folder.fold_expression(cond.cond_expr);
            cond.true_expr = folder.fold_expression(cond.true_expr);
            cond.false_expr = folder.fold_expression(cond.false_expr);
            Expression::Conditional(cond, span)
        }

        Expression::BinaryOp(op, left, right, span) => {
            let folded_left = folder.fold_expression(*left);
            let folded_right = folder.fold_expression(*right);
            Expression::BinaryOp(op, Box::new(folded_left), Box::new(folded_right), span)
        }

        Expression::UnaryOp(op, inner, span) => {
            let folded_inner = folder.fold_expression(*inner);
            Expression::UnaryOp(op, Box::new(folded_inner), span)
        }

        Expression::Parentheses(inner, span) => {
            let folded_inner = folder.fold_expression(*inner);
            Expression::Parentheses(Box::new(folded_inner), span)
        }

        Expression::ForExpr(mut fe, span) => {
            fe.collection = Box::new(folder.fold_expression(*fe.collection));
            if let Some(key_expr) = fe.key_expr {
                fe.key_expr = Some(Box::new(folder.fold_expression(*key_expr)));
            }
            fe.val_expr = Box::new(folder.fold_expression(*fe.val_expr));
            if let Some(cond_expr) = fe.cond_expr {
                fe.cond_expr = Some(Box::new(folder.fold_expression(*cond_expr)));
            }
            Expression::ForExpr(fe, span)
        }
    }
}

/// Recursively transforms a [`Traversal`] using the given folder.
///
/// # Arguments
/// * `folder` - The folder performing the transformation.
/// * `trav` - The traversal to transform.
#[must_use]
pub fn walk_fold_traversal<F: AstFolder>(folder: &mut F, mut trav: Traversal) -> Traversal {
    trav.expr = Box::new(folder.fold_expression(*trav.expr));
    trav.operators = trav
        .operators
        .into_iter()
        .map(|op| match op {
            TraversalOperator::Index(idx_expr, span) => {
                TraversalOperator::Index(folder.fold_expression(idx_expr), span)
            }
            other => other,
        })
        .collect();
    trav
}

/// Recursively transforms a [`TemplatePart`] using the given folder.
///
/// # Arguments
/// * `folder` - The folder performing the transformation.
/// * `part` - The template part to transform.
#[must_use]
#[rustfmt::skip]
pub fn walk_fold_template_part<F: AstFolder>(folder: &mut F, part: TemplatePart) -> TemplatePart {
    match part {
        TemplatePart::Literal(_, _) => part,
        TemplatePart::Interpolation(expr, span) => {
            TemplatePart::Interpolation(folder.fold_expression(expr), span)
        }
        TemplatePart::Directive(directive, span) => {
            let folded_dir = match directive {
                Directive::If { cond, true_expr, else_ifs, false_expr } => {
                    let folded_cond = folder.fold_expression(cond);
                    let folded_true = true_expr
                        .into_iter()
                        .map(|p| folder.fold_template_part(p))
                        .collect();
                    let folded_else_ifs = else_ifs
                        .into_iter()
                        .map(|(c, parts)| {
                            (
                                folder.fold_expression(c),
                                parts
                                    .into_iter()
                                    .map(|p| folder.fold_template_part(p))
                                    .collect(),
                            )
                        })
                        .collect();
                    let folded_false = false_expr.map(|parts| {
                        parts
                            .into_iter()
                            .map(|p| folder.fold_template_part(p))
                            .collect()
                    });
                    Directive::If {
                        cond: folded_cond,
                        true_expr: folded_true,
                        else_ifs: folded_else_ifs,
                        false_expr: folded_false,
                    }
                }
                Directive::For { key_var, val_var, collection, body } => {
                    let folded_collection = folder.fold_expression(collection);
                    let folded_body = body
                        .into_iter()
                        .map(|p| folder.fold_template_part(p))
                        .collect();
                    Directive::For {
                        key_var,
                        val_var,
                        collection: folded_collection,
                        body: folded_body,
                    }
                }
                Directive::Strip { strip_left, strip_right } => Directive::Strip {
                    strip_left,
                    strip_right,
                },
            };
            TemplatePart::Directive(folded_dir, span)
        }
    }
}

/// Recursively transforms an [`Attribute`] using the given folder.
///
/// # Arguments
/// * `folder` - The folder performing the transformation.
/// * `attr` - The attribute to transform.
#[must_use]
pub fn walk_fold_attribute<F: AstFolder>(folder: &mut F, mut attr: Attribute) -> Attribute {
    attr.expr = folder.fold_expression(attr.expr);
    attr
}

/// Recursively transforms a [`Block`] using the given folder.
///
/// # Arguments
/// * `folder` - The folder performing the transformation.
/// * `block` - The block to transform.
#[must_use]
pub fn walk_fold_block<F: AstFolder>(folder: &mut F, mut block: Block) -> Block {
    block.body = folder.fold_body(block.body);
    block
}

/// Recursively transforms a [`DynamicBlock`] using the given folder.
///
/// # Arguments
/// * `folder` - The folder performing the transformation.
/// * `dyn_block` - The dynamic block to transform.
#[must_use]
pub fn walk_fold_dynamic_block<F: AstFolder>(
    folder: &mut F,
    mut dyn_block: DynamicBlock,
) -> DynamicBlock {
    dyn_block.for_each = folder.fold_expression(dyn_block.for_each);
    if let Some(labels) = dyn_block.labels {
        dyn_block.labels = Some(
            labels
                .into_iter()
                .map(|lbl| folder.fold_expression(lbl))
                .collect(),
        );
    }
    dyn_block.content = folder.fold_body(dyn_block.content);
    dyn_block
}

/// Recursively transforms a [`ValidationBlock`] using the given folder.
///
/// # Arguments
/// * `folder` - The folder performing the transformation.
/// * `val` - The validation block to transform.
#[must_use]
pub fn walk_fold_validation_block<F: AstFolder>(
    folder: &mut F,
    mut val: ValidationBlock,
) -> ValidationBlock {
    val.condition = folder.fold_expression(val.condition);
    val.error_message = folder.fold_expression(val.error_message);
    val
}

/// Recursively transforms a [`PreconditionBlock`] using the given folder.
///
/// # Arguments
/// * `folder` - The folder performing the transformation.
/// * `pre` - The precondition block to transform.
#[must_use]
pub fn walk_fold_precondition_block<F: AstFolder>(
    folder: &mut F,
    mut pre: PreconditionBlock,
) -> PreconditionBlock {
    pre.condition = folder.fold_expression(pre.condition);
    pre.error_message = folder.fold_expression(pre.error_message);
    pre
}

/// Recursively transforms a [`PostconditionBlock`] using the given folder.
///
/// # Arguments
/// * `folder` - The folder performing the transformation.
/// * `post` - The postcondition block to transform.
#[must_use]
pub fn walk_fold_postcondition_block<F: AstFolder>(
    folder: &mut F,
    mut post: PostconditionBlock,
) -> PostconditionBlock {
    post.condition = folder.fold_expression(post.condition);
    post.error_message = folder.fold_expression(post.error_message);
    post
}

/// Recursively transforms a [`Body`] using the given folder.
///
/// # Arguments
/// * `folder` - The folder performing the transformation.
/// * `body` - The body to transform.
#[must_use]
pub fn walk_fold_body<F: AstFolder>(folder: &mut F, mut body: Body) -> Body {
    body.attributes = body
        .attributes
        .into_iter()
        .map(|(k, attr)| (k, folder.fold_attribute(attr)))
        .collect();

    body.blocks = body
        .blocks
        .into_iter()
        .map(|b| folder.fold_block(b))
        .collect();

    body.dynamic_blocks = body
        .dynamic_blocks
        .into_iter()
        .map(|db| folder.fold_dynamic_block(db))
        .collect();

    body.validations = body
        .validations
        .into_iter()
        .map(|v| folder.fold_validation_block(v))
        .collect();

    body.preconditions = body
        .preconditions
        .into_iter()
        .map(|p| folder.fold_precondition_block(p))
        .collect();

    body.postconditions = body
        .postconditions
        .into_iter()
        .map(|p| folder.fold_postcondition_block(p))
        .collect();

    body
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
    use crate::ast::expr::{BinaryOp, Conditional, ForExpr, FuncCall};
    use crate::span::Span;
    use std::str::FromStr;

    #[derive(Default)]
    struct CounterVisitor {
        exprs: usize,
        attrs: usize,
        blocks: usize,
        dyns: usize,
        vals: usize,
        pres: usize,
        posts: usize,
        bodies: usize,
    }

    impl AstVisitor for CounterVisitor {
        fn visit_expression(&mut self, _expr: &Expression) -> bool {
            self.exprs += 1;
            true
        }

        fn visit_attribute(&mut self, _attr: &Attribute) -> bool {
            self.attrs += 1;
            true
        }

        fn visit_block(&mut self, _block: &Block) -> bool {
            self.blocks += 1;
            true
        }

        fn visit_dynamic_block(&mut self, _dyn_block: &DynamicBlock) -> bool {
            self.dyns += 1;
            true
        }

        fn visit_validation_block(&mut self, _val: &ValidationBlock) -> bool {
            self.vals += 1;
            true
        }

        fn visit_precondition_block(&mut self, _pre: &PreconditionBlock) -> bool {
            self.pres += 1;
            true
        }

        fn visit_postcondition_block(&mut self, _post: &PostconditionBlock) -> bool {
            self.posts += 1;
            true
        }

        fn visit_body(&mut self, _body: &Body) -> bool {
            self.bodies += 1;
            true
        }
    }

    #[test]
    fn test_visitor_walks_all_nodes() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut body = Body::new(span.clone());

        let attr = Attribute {
            name: "test".to_string(),
            name_span: span.clone(),
            equals_span: span.clone(),
            expr: Expression::Tuple(
                vec![
                    Expression::Null(span.clone()),
                    Expression::Bool(true, span.clone()),
                    Expression::Object(
                        vec![(
                            Expression::String("k".to_string(), span.clone()),
                            Expression::Variable("v".to_string(), span.clone()),
                        )],
                        span.clone(),
                    ),
                    Expression::Template(
                        vec![
                            TemplatePart::Literal("hello ".to_string(), span.clone()),
                            TemplatePart::Interpolation(
                                Expression::Variable("x".to_string(), span.clone()),
                                span.clone(),
                            ),
                            TemplatePart::Directive(
                                Directive::If {
                                    cond: Expression::Bool(true, span.clone()),
                                    true_expr: vec![TemplatePart::Literal(
                                        "true".to_string(),
                                        span.clone(),
                                    )],
                                    else_ifs: vec![(
                                        Expression::Bool(false, span.clone()),
                                        vec![TemplatePart::Literal(
                                            "elif".to_string(),
                                            span.clone(),
                                        )],
                                    )],
                                    false_expr: Some(vec![TemplatePart::Literal(
                                        "false".to_string(),
                                        span.clone(),
                                    )]),
                                },
                                span.clone(),
                            ),
                            TemplatePart::Directive(
                                Directive::Strip {
                                    strip_left: true,
                                    strip_right: false,
                                },
                                span.clone(),
                            ),
                            TemplatePart::Directive(
                                Directive::For {
                                    key_var: Some("k".to_string()),
                                    val_var: "v".to_string(),
                                    collection: Expression::Variable(
                                        "list".to_string(),
                                        span.clone(),
                                    ),
                                    body: vec![TemplatePart::Literal(
                                        "item".to_string(),
                                        span.clone(),
                                    )],
                                },
                                span.clone(),
                            ),
                        ],
                        span.clone(),
                    ),
                    Expression::FuncCall(
                        Box::new(FuncCall {
                            name: "func".into(),
                            args: vec![Expression::Variable("a".to_string(), span.clone())],
                            expand_final: false,
                        }),
                        span.clone(),
                    ),
                    Expression::Conditional(
                        Box::new(Conditional {
                            cond_expr: Expression::Bool(true, span.clone()),
                            true_expr: Expression::Number(
                                crate::number::Number::from_str("1").unwrap(),
                                span.clone(),
                            ),
                            false_expr: Expression::Number(
                                crate::number::Number::from_str("0").unwrap(),
                                span.clone(),
                            ),
                        }),
                        span.clone(),
                    ),
                    Expression::BinaryOp(
                        BinaryOp::Add,
                        Box::new(Expression::Variable("b".to_string(), span.clone())),
                        Box::new(Expression::Variable("c".to_string(), span.clone())),
                        span.clone(),
                    ),
                    Expression::UnaryOp(
                        crate::ast::expr::UnaryOp::Not,
                        Box::new(Expression::Bool(false, span.clone())),
                        span.clone(),
                    ),
                    Expression::Parentheses(
                        Box::new(Expression::Variable("p".to_string(), span.clone())),
                        span.clone(),
                    ),
                    Expression::ForExpr(
                        Box::new(ForExpr {
                            key_var: Some("i".to_string()),
                            val_var: "val".to_string(),
                            collection: Box::new(Expression::Variable(
                                "items".to_string(),
                                span.clone(),
                            )),
                            key_expr: Some(Box::new(Expression::Variable(
                                "i".to_string(),
                                span.clone(),
                            ))),
                            val_expr: Box::new(Expression::Variable(
                                "val".to_string(),
                                span.clone(),
                            )),
                            cond_expr: Some(Box::new(Expression::Bool(true, span.clone()))),
                            grouping: false,
                        }),
                        span.clone(),
                    ),
                    Expression::Traversal(
                        Box::new(Traversal {
                            expr: Box::new(Expression::Variable("trav".to_string(), span.clone())),
                            operators: vec![
                                TraversalOperator::GetAttr("attr".to_string(), span.clone()),
                                TraversalOperator::Index(
                                    Expression::Variable("idx".to_string(), span.clone()),
                                    span.clone(),
                                ),
                            ],
                        }),
                        span.clone(),
                    ),
                ],
                span.clone(),
            ),
            span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        body.attributes.insert("test".to_string(), attr);

        let block = Block {
            block_type: "resource".to_string(),
            type_span: span.clone(),
            labels: vec!["foo".to_string()],
            label_spans: vec![span.clone()],
            body: Body::new(span.clone()),
            span: span.clone(),
            open_brace_span: span.clone(),
            close_brace_span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        body.blocks.push(block);

        let dyn_block = DynamicBlock::new(
            "dyn".to_string(),
            Expression::Variable("coll".to_string(), span.clone()),
            None,
            Some(vec![Expression::Variable("lbl".to_string(), span.clone())]),
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        );
        body.dynamic_blocks.push(dyn_block);

        body.validations.push(ValidationBlock::new(
            Expression::Variable("val_cond".to_string(), span.clone()),
            Expression::Variable("val_msg".to_string(), span.clone()),
            span.clone(),
        ));
        body.preconditions.push(PreconditionBlock::new(
            Expression::Variable("pre_cond".to_string(), span.clone()),
            Expression::Variable("pre_msg".to_string(), span.clone()),
            span.clone(),
        ));
        body.postconditions.push(PostconditionBlock::new(
            Expression::Variable("post_cond".to_string(), span.clone()),
            Expression::Variable("post_msg".to_string(), span.clone()),
            span.clone(),
        ));

        let mut visitor = CounterVisitor::default();
        walk_body(&mut visitor, &body);

        assert!(visitor.exprs > 20);
        assert_eq!(visitor.attrs, 1);
        assert_eq!(visitor.blocks, 1);
        assert_eq!(visitor.dyns, 1);
        assert_eq!(visitor.vals, 1);
        assert_eq!(visitor.pres, 1);
        assert_eq!(visitor.posts, 1);
        assert_eq!(visitor.bodies, 3); // root body, block body, dynamic block body

        let mut visitor2 = CounterVisitor::default();
        walk_body(&mut visitor2, &build_comprehensive_ast(span));
        assert!(visitor2.exprs > 10);
    }

    struct VariableRenamer {
        from: String,
        to: String,
    }

    impl AstFolder for VariableRenamer {
        fn fold_expression(&mut self, expr: Expression) -> Expression {
            match expr {
                Expression::Variable(name, span) if name == self.from => {
                    Expression::Variable(self.to.clone(), span)
                }
                other => walk_fold_expression(self, other),
            }
        }
    }

    #[test]
    fn test_folder_rewrites_variable() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut body = Body::new(span.clone());

        let attr = Attribute {
            name: "val".to_string(),
            name_span: span.clone(),
            equals_span: span.clone(),
            expr: Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::Variable("old_var".to_string(), span.clone())),
                Box::new(Expression::Variable("keep_var".to_string(), span.clone())),
                span.clone(),
            ),
            span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        body.attributes.insert("val".to_string(), attr);

        body.validations.push(ValidationBlock::new(
            Expression::Variable("old_var".to_string(), span.clone()),
            Expression::Variable("msg".to_string(), span.clone()),
            span.clone(),
        ));
        body.preconditions.push(PreconditionBlock::new(
            Expression::Variable("old_var".to_string(), span.clone()),
            Expression::Variable("msg".to_string(), span.clone()),
            span.clone(),
        ));
        body.postconditions.push(PostconditionBlock::new(
            Expression::Variable("old_var".to_string(), span.clone()),
            Expression::Variable("msg".to_string(), span.clone()),
            span.clone(),
        ));

        let mut renamer = VariableRenamer {
            from: "old_var".to_string(),
            to: "new_var".to_string(),
        };

        let folded_body = renamer.fold_body(body);
        let expr = &folded_body.attributes["val"].expr;

        assert_eq!(
            *expr,
            Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::Variable("new_var".to_string(), span.clone())),
                Box::new(Expression::Variable("keep_var".to_string(), span.clone())),
                span.clone(),
            )
        );

        assert_eq!(
            folded_body.validations[0].condition,
            Expression::Variable("new_var".to_string(), span.clone())
        );
        assert_eq!(
            folded_body.preconditions[0].condition,
            Expression::Variable("new_var".to_string(), span.clone())
        );
        assert_eq!(
            folded_body.postconditions[0].condition,
            Expression::Variable("new_var".to_string(), span.clone())
        );

        let folded_comp = renamer.fold_body(build_comprehensive_ast(span));
        assert_eq!(folded_comp.attributes.len(), 1);
    }

    struct DefaultVisitor;
    impl AstVisitor for DefaultVisitor {}

    struct PruningVisitor;
    impl AstVisitor for PruningVisitor {
        fn visit_expression(&mut self, _expr: &Expression) -> bool {
            false
        }
        fn visit_template_part(&mut self, _part: &TemplatePart) -> bool {
            false
        }
        fn visit_traversal(&mut self, _trav: &Traversal) -> bool {
            false
        }
        fn visit_attribute(&mut self, _attr: &Attribute) -> bool {
            false
        }
        fn visit_block(&mut self, _block: &Block) -> bool {
            false
        }
        fn visit_dynamic_block(&mut self, _dyn_block: &DynamicBlock) -> bool {
            false
        }
        fn visit_validation_block(&mut self, _val: &ValidationBlock) -> bool {
            false
        }
        fn visit_precondition_block(&mut self, _pre: &PreconditionBlock) -> bool {
            false
        }
        fn visit_postcondition_block(&mut self, _post: &PostconditionBlock) -> bool {
            false
        }
        fn visit_body(&mut self, _body: &Body) -> bool {
            true
        }
    }

    struct StopBodyVisitor;
    impl AstVisitor for StopBodyVisitor {
        fn visit_body(&mut self, _body: &Body) -> bool {
            false
        }
    }

    struct IdentityFolder;
    impl AstFolder for IdentityFolder {}

    fn build_comprehensive_ast(span: Span) -> Body {
        let mut body = Body::new(span.clone());

        let expr = Expression::Tuple(
            vec![
                Expression::Null(span.clone()),
                Expression::Bool(true, span.clone()),
                Expression::Number(crate::number::Number::from_str("42").unwrap(), span.clone()),
                Expression::String("str".to_string(), span.clone()),
                Expression::Variable("var".to_string(), span.clone()),
                Expression::Object(
                    vec![(
                        Expression::String("key".to_string(), span.clone()),
                        Expression::Variable("val".to_string(), span.clone()),
                    )],
                    span.clone(),
                ),
                Expression::Template(
                    vec![
                        TemplatePart::Literal("lit".to_string(), span.clone()),
                        TemplatePart::Interpolation(
                            Expression::Variable("x".to_string(), span.clone()),
                            span.clone(),
                        ),
                        TemplatePart::Directive(
                            Directive::If {
                                cond: Expression::Bool(true, span.clone()),
                                true_expr: vec![TemplatePart::Literal(
                                    "t".to_string(),
                                    span.clone(),
                                )],
                                else_ifs: vec![(
                                    Expression::Bool(false, span.clone()),
                                    vec![TemplatePart::Literal("elif_t".to_string(), span.clone())],
                                )],
                                false_expr: Some(vec![TemplatePart::Literal(
                                    "f".to_string(),
                                    span.clone(),
                                )]),
                            },
                            span.clone(),
                        ),
                        TemplatePart::Directive(
                            Directive::If {
                                cond: Expression::Bool(false, span.clone()),
                                true_expr: vec![TemplatePart::Literal(
                                    "only_true".to_string(),
                                    span.clone(),
                                )],
                                else_ifs: vec![],
                                false_expr: None,
                            },
                            span.clone(),
                        ),
                        TemplatePart::Directive(
                            Directive::Strip {
                                strip_left: false,
                                strip_right: true,
                            },
                            span.clone(),
                        ),
                        TemplatePart::Directive(
                            Directive::For {
                                key_var: Some("k".to_string()),
                                val_var: "v".to_string(),
                                collection: Expression::Variable("items".to_string(), span.clone()),
                                body: vec![TemplatePart::Literal("body".to_string(), span.clone())],
                            },
                            span.clone(),
                        ),
                    ],
                    span.clone(),
                ),
                Expression::Traversal(
                    Box::new(Traversal {
                        expr: Box::new(Expression::Variable("base".to_string(), span.clone())),
                        operators: vec![
                            TraversalOperator::GetAttr("attr".to_string(), span.clone()),
                            TraversalOperator::Index(
                                Expression::Variable("idx".to_string(), span.clone()),
                                span.clone(),
                            ),
                        ],
                    }),
                    span.clone(),
                ),
                Expression::FuncCall(
                    Box::new(FuncCall {
                        name: "my_func".into(),
                        args: vec![Expression::Variable("arg".to_string(), span.clone())],
                        expand_final: true,
                    }),
                    span.clone(),
                ),
                Expression::Conditional(
                    Box::new(Conditional {
                        cond_expr: Expression::Bool(true, span.clone()),
                        true_expr: Expression::Number(
                            crate::number::Number::from_str("1").unwrap(),
                            span.clone(),
                        ),
                        false_expr: Expression::Number(
                            crate::number::Number::from_str("0").unwrap(),
                            span.clone(),
                        ),
                    }),
                    span.clone(),
                ),
                Expression::BinaryOp(
                    BinaryOp::Add,
                    Box::new(Expression::Variable("a".to_string(), span.clone())),
                    Box::new(Expression::Variable("b".to_string(), span.clone())),
                    span.clone(),
                ),
                Expression::UnaryOp(
                    crate::ast::expr::UnaryOp::Neg,
                    Box::new(Expression::Number(
                        crate::number::Number::from_str("1").unwrap(),
                        span.clone(),
                    )),
                    span.clone(),
                ),
                Expression::Parentheses(
                    Box::new(Expression::Variable("p".to_string(), span.clone())),
                    span.clone(),
                ),
                Expression::ForExpr(
                    Box::new(ForExpr {
                        key_var: Some("k".to_string()),
                        val_var: "v".to_string(),
                        collection: Box::new(Expression::Variable(
                            "coll".to_string(),
                            span.clone(),
                        )),
                        key_expr: Some(Box::new(Expression::Variable(
                            "k".to_string(),
                            span.clone(),
                        ))),
                        val_expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
                        cond_expr: Some(Box::new(Expression::Bool(true, span.clone()))),
                        grouping: true,
                    }),
                    span.clone(),
                ),
                Expression::ForExpr(
                    Box::new(ForExpr {
                        key_var: None,
                        val_var: "elem".to_string(),
                        collection: Box::new(Expression::Variable(
                            "list".to_string(),
                            span.clone(),
                        )),
                        key_expr: None,
                        val_expr: Box::new(Expression::Variable("elem".to_string(), span.clone())),
                        cond_expr: None,
                        grouping: false,
                    }),
                    span.clone(),
                ),
            ],
            span.clone(),
        );

        let attr = Attribute {
            name: "full".to_string(),
            name_span: span.clone(),
            equals_span: span.clone(),
            expr,
            span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        body.attributes.insert("full".to_string(), attr);

        body.blocks.push(Block {
            block_type: "b".to_string(),
            type_span: span.clone(),
            labels: vec![],
            label_spans: vec![],
            body: Body::new(span.clone()),
            span: span.clone(),
            open_brace_span: span.clone(),
            close_brace_span: span.clone(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        });

        body.dynamic_blocks.push(DynamicBlock::new(
            "dyn1".to_string(),
            Expression::Variable("coll".to_string(), span.clone()),
            None,
            Some(vec![Expression::Variable("lbl".to_string(), span.clone())]),
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        ));

        body.dynamic_blocks.push(DynamicBlock::new(
            "dyn2".to_string(),
            Expression::Variable("coll2".to_string(), span.clone()),
            None,
            None,
            Body::new(span.clone()),
            span.clone(),
            span.clone(),
        ));

        body.validations.push(ValidationBlock::new(
            Expression::Bool(true, span.clone()),
            Expression::String("err".to_string(), span.clone()),
            span.clone(),
        ));

        body.preconditions.push(PreconditionBlock::new(
            Expression::Bool(true, span.clone()),
            Expression::String("pre err".to_string(), span.clone()),
            span.clone(),
        ));

        body.postconditions.push(PostconditionBlock::new(
            Expression::Bool(true, span.clone()),
            Expression::String("post err".to_string(), span.clone()),
            span,
        ));

        body
    }

    #[test]
    fn test_walk_comprehensive_default_visitor_and_folder() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let body = build_comprehensive_ast(span);

        // 1. Walk with default visitor
        let mut visitor = DefaultVisitor;
        walk_body(&mut visitor, &body);

        // 2. Fold with identity folder
        let mut id_folder = IdentityFolder;
        let folded = id_folder.fold_body(body.clone());
        assert_eq!(folded, body);
    }

    #[test]
    fn test_walk_pruning_visitor() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let body = build_comprehensive_ast(span.clone());

        let mut pruning = PruningVisitor;
        walk_body(&mut pruning, &body);

        // Test root body walk when visit_body returns false
        let mut stop = StopBodyVisitor;
        walk_body(&mut stop, &body);

        // Directly test walk functions with pruning visitor
        let attr = &body.attributes["full"];
        walk_attribute(&mut pruning, attr);
        walk_expression(&mut pruning, &attr.expr);

        let tpl = TemplatePart::Literal("lit".to_string(), span.clone());
        walk_template_part(&mut pruning, &tpl);

        let trav = Traversal {
            expr: Box::new(Expression::Variable("v".to_string(), span.clone())),
            operators: vec![],
        };
        assert!(!pruning.visit_traversal(&trav));
        let expr_trav = Expression::Traversal(Box::new(trav), span.clone());
        walk_expression(&mut pruning, &expr_trav);

        struct TraversalSkipVisitor;
        impl AstVisitor for TraversalSkipVisitor {
            fn visit_traversal(&mut self, _trav: &Traversal) -> bool {
                false
            }
        }
        let mut trav_skip = TraversalSkipVisitor;
        walk_expression(&mut trav_skip, &expr_trav);

        struct FuncCallSkipVisitor;
        impl AstVisitor for FuncCallSkipVisitor {
            fn visit_func_call(&mut self, _fc: &FuncCall) -> bool {
                false
            }
        }
        let fc_expr = Expression::FuncCall(
            Box::new(FuncCall {
                name: "test".into(),
                args: vec![Expression::Null(span.clone())],
                expand_final: false,
            }),
            span.clone(),
        );
        let mut fc_skip = FuncCallSkipVisitor;
        walk_expression(&mut fc_skip, &fc_expr);

        walk_block(&mut pruning, &body.blocks[0]);
        walk_dynamic_block(&mut pruning, &body.dynamic_blocks[0]);
        walk_validation_block(&mut pruning, &body.validations[0]);
        walk_precondition_block(&mut pruning, &body.preconditions[0]);
        walk_postcondition_block(&mut pruning, &body.postconditions[0]);
    }

    #[test]
    fn test_walk_fold_direct() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let mut id_folder = IdentityFolder;

        // Directly call walk_fold_expression on primitives to exercise match arms
        let null_expr = Expression::Null(span.clone());
        assert_eq!(
            walk_fold_expression(&mut id_folder, null_expr.clone()),
            null_expr
        );

        let bool_expr = Expression::Bool(true, span.clone());
        assert_eq!(
            walk_fold_expression(&mut id_folder, bool_expr.clone()),
            bool_expr
        );

        let num_expr = Expression::Number(
            crate::number::Number::from_str("123").unwrap(),
            span.clone(),
        );
        assert_eq!(
            walk_fold_expression(&mut id_folder, num_expr.clone()),
            num_expr
        );

        let str_expr = Expression::String("hello".to_string(), span.clone());
        assert_eq!(
            walk_fold_expression(&mut id_folder, str_expr.clone()),
            str_expr
        );

        let var_expr = Expression::Variable("v".to_string(), span.clone());
        assert_eq!(
            walk_fold_expression(&mut id_folder, var_expr.clone()),
            var_expr
        );

        // Directly call walk_fold_template_part on Directive variants
        let if_part = TemplatePart::Directive(
            Directive::If {
                cond: Expression::Bool(true, span.clone()),
                true_expr: vec![TemplatePart::Literal("true".to_string(), span.clone())],
                else_ifs: vec![(
                    Expression::Bool(false, span.clone()),
                    vec![TemplatePart::Literal("elif".to_string(), span.clone())],
                )],
                false_expr: Some(vec![TemplatePart::Literal(
                    "false".to_string(),
                    span.clone(),
                )]),
            },
            span.clone(),
        );
        assert_eq!(
            walk_fold_template_part(&mut id_folder, if_part.clone()),
            if_part
        );

        let for_part = TemplatePart::Directive(
            Directive::For {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Expression::Variable("coll".to_string(), span.clone()),
                body: vec![TemplatePart::Literal("b".to_string(), span.clone())],
            },
            span.clone(),
        );
        assert_eq!(
            walk_fold_template_part(&mut id_folder, for_part.clone()),
            for_part
        );

        let strip_part = TemplatePart::Directive(
            Directive::Strip {
                strip_left: true,
                strip_right: false,
            },
            span.clone(),
        );
        assert_eq!(
            walk_fold_template_part(&mut id_folder, strip_part.clone()),
            strip_part
        );

        let mut renamer = VariableRenamer {
            from: "coll".to_string(),
            to: "new_coll".to_string(),
        };
        assert_eq!(
            walk_fold_template_part(&mut renamer, if_part.clone()),
            if_part
        );
        let expected_for = TemplatePart::Directive(
            Directive::For {
                key_var: Some("k".to_string()),
                val_var: "v".to_string(),
                collection: Expression::Variable("new_coll".to_string(), span.clone()),
                body: vec![TemplatePart::Literal("b".to_string(), span.clone())],
            },
            span.clone(),
        );
        let folded_for = walk_fold_template_part(&mut renamer, for_part);
        assert_eq!(folded_for, expected_for);
        assert_eq!(
            walk_fold_template_part(&mut renamer, strip_part.clone()),
            strip_part
        );
    }
}
