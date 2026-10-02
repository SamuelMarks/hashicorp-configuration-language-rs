//! Lazy evaluation of HCL bodies on demand with memoization.

use crate::ast::structure::{Block, Body};
use crate::diagnostic::Diagnostics;
use crate::eval::context::Context;
use crate::eval::evaluator::Evaluator;
use crate::types::{Type, Value, ValueData};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

/// Result of evaluating an attribute, returning the value and warnings on success, or errors.
pub type LazyAttrResult = Result<(Value, Diagnostics), Diagnostics>;

/// Internal memoization cache mapping attribute names to their evaluation results.
type LazyAttrCache = HashMap<String, LazyAttrResult>;

/// A lazy, on-demand evaluator wrapping an unevaluated AST [`Body`] and an evaluation [`Context`].
///
/// Attributes and child blocks are evaluated only when explicitly accessed. Evaluated values are
/// memoized to prevent redundant computation. Any unused attributes containing reference or evaluation
/// errors are ignored unless inspected.
pub struct LazyBody<'a> {
    body: &'a Body,
    ctx: &'a Context<'a>,
    attr_cache: Mutex<LazyAttrCache>,
}

impl std::fmt::Debug for LazyBody<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazyBody")
            .field("body_span", &self.body.span)
            .finish_non_exhaustive()
    }
}

impl<'a> LazyBody<'a> {
    /// Creates a new `LazyBody` wrapping the provided AST [`Body`] and [`Context`].
    ///
    /// # Arguments
    /// * `body` - The unevaluated AST body.
    /// * `ctx` - The evaluation context for variable and function lookup.
    ///
    /// # Returns
    /// A new `LazyBody` with an empty evaluation cache.
    #[must_use]
    pub fn new(body: &'a Body, ctx: &'a Context<'a>) -> Self {
        Self {
            body,
            ctx,
            attr_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Helper to safely acquire the cache mutex lock even if poisoned.
    fn lock_cache(&self) -> std::sync::MutexGuard<'_, LazyAttrCache> {
        match self.attr_cache.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Returns a reference to the wrapped AST [`Body`].
    ///
    /// # Returns
    /// Reference to the underlying AST body.
    #[must_use]
    pub fn body(&self) -> &'a Body {
        self.body
    }

    /// Returns a reference to the evaluation [`Context`].
    ///
    /// # Returns
    /// Reference to the context used for evaluation.
    #[must_use]
    pub fn context(&self) -> &'a Context<'a> {
        self.ctx
    }

    /// Checks if an attribute with the given name exists in the body without evaluating it.
    ///
    /// # Arguments
    /// * `name` - The attribute name to check.
    ///
    /// # Returns
    /// `true` if the attribute is declared in the body.
    #[must_use]
    pub fn has_attribute(&self, name: &str) -> bool {
        self.body.attributes.contains_key(name)
    }

    /// Returns the names of all attributes declared in the body without evaluating them.
    ///
    /// # Returns
    /// A vector of attribute name string slices.
    #[must_use]
    pub fn attribute_names(&self) -> Vec<&str> {
        self.body.attributes.keys().map(String::as_str).collect()
    }

    /// Checks whether an attribute has already been evaluated and cached.
    ///
    /// # Arguments
    /// * `name` - The attribute name to check.
    ///
    /// # Returns
    /// `true` if the attribute evaluation result is currently memoized.
    #[must_use]
    pub fn is_attribute_evaluated(&self, name: &str) -> bool {
        let cache = self.lock_cache();
        cache.contains_key(name)
    }

    /// Returns the count of currently memoized evaluated attributes.
    ///
    /// # Returns
    /// The number of cached attribute results.
    #[must_use]
    pub fn evaluated_attribute_count(&self) -> usize {
        let cache = self.lock_cache();
        cache.len()
    }

    /// Clears the memoized evaluation cache.
    pub fn clear_cache(&self) {
        let mut cache = self.lock_cache();
        cache.clear();
    }

    /// Evaluates a single attribute AST node on-demand, checking and updating the cache.
    ///
    /// # Arguments
    /// * `name` - The attribute name.
    /// * `attr` - The attribute AST node.
    ///
    /// # Returns
    /// The evaluation result.
    fn evaluate_attribute_internal(
        &self,
        name: &str,
        attr: &'a crate::ast::structure::Attribute,
    ) -> LazyAttrResult {
        {
            let cache = self.lock_cache();
            if let Some(cached) = cache.get(name) {
                return cached.clone();
            }
        }

        let eval = Evaluator::new(self.ctx);
        let res = eval.evaluate(&attr.expr);

        let mut cache = self.lock_cache();
        cache.insert(name.to_string(), res.clone());
        res
    }

    /// Evaluates a single attribute on-demand by name.
    ///
    /// If the attribute was already evaluated, the memoized result is returned without re-evaluating.
    /// If the attribute is not declared in the body, returns `None`.
    ///
    /// # Arguments
    /// * `name` - The name of the attribute to evaluate.
    ///
    /// # Returns
    /// `Some(Ok((value, diags)))` on successful evaluation, `Some(Err(diags))` if evaluation failed,
    /// or `None` if the attribute does not exist.
    pub fn evaluate_attribute(&self, name: &str) -> Option<LazyAttrResult> {
        let attr = self.body.attributes.get(name)?;
        Some(self.evaluate_attribute_internal(name, attr))
    }

    /// Evaluates all attributes declared in the body and stores them in the cache.
    ///
    /// # Returns
    /// A map of attribute names to their evaluation results.
    pub fn evaluate_all_attributes(&self) -> HashMap<String, LazyAttrResult> {
        let mut results = HashMap::new();
        for (name, attr) in &self.body.attributes {
            results.insert(name.clone(), self.evaluate_attribute_internal(name, attr));
        }
        results
    }

    /// Evaluates all attributes into an HCL Object [`Value`].
    ///
    /// # Returns
    /// A tuple of the constructed Object [`Value`] and accumulated warnings on success.
    ///
    /// # Errors
    /// Returns accumulated [`Diagnostics`] if any attribute evaluation encounters an error.
    pub fn to_object_value(&self) -> Result<(Value, Diagnostics), Diagnostics> {
        let mut map = BTreeMap::new();
        let mut all_diags = Diagnostics::new();

        for (name, attr) in &self.body.attributes {
            match self.evaluate_attribute_internal(name, attr) {
                Ok((val, diags)) => {
                    for d in &diags {
                        all_diags.push(d.clone());
                    }
                    map.insert(name.clone(), val);
                }
                Err(errs) => {
                    return Err(errs);
                }
            }
        }

        Ok((
            Value::new(Type::object(BTreeMap::new()), ValueData::Object(map)),
            all_diags,
        ))
    }

    /// Returns a slice of all AST [`Block`] items contained in this body.
    ///
    /// # Returns
    /// Slice of child blocks.
    #[must_use]
    pub fn blocks(&self) -> &'a [Block] {
        &self.body.blocks
    }

    /// Returns lazy child body wrappers for all blocks matching the specified block type.
    ///
    /// # Arguments
    /// * `block_type` - The block type name to filter by.
    ///
    /// # Returns
    /// A vector of child `LazyBody` evaluators.
    #[must_use]
    pub fn get_blocks(&self, block_type: &str) -> Vec<LazyBody<'a>> {
        self.body
            .blocks
            .iter()
            .filter(|b| b.block_type == block_type)
            .map(|b| LazyBody::new(&b.body, self.ctx))
            .collect()
    }

    /// Returns a lazy child body wrapper for the first block matching the type and labels.
    ///
    /// # Arguments
    /// * `block_type` - The block type to search for.
    /// * `labels` - The required block labels in order.
    ///
    /// # Returns
    /// `Some(LazyBody)` if a matching block is found, or `None` otherwise.
    #[must_use]
    pub fn get_block(&self, block_type: &str, labels: &[&str]) -> Option<LazyBody<'a>> {
        self.body
            .blocks
            .iter()
            .find(|b| {
                b.block_type == block_type
                    && b.labels.len() == labels.len()
                    && b.labels.iter().zip(labels.iter()).all(|(l1, l2)| l1 == *l2)
            })
            .map(|b| LazyBody::new(&b.body, self.ctx))
    }

    /// Returns a lazy child body wrapper for the block at the given index.
    ///
    /// # Arguments
    /// * `index` - The zero-based index of the block in this body.
    ///
    /// # Returns
    /// `Some(LazyBody)` if `index` is within bounds, or `None` otherwise.
    #[must_use]
    pub fn get_block_by_index(&self, index: usize) -> Option<LazyBody<'a>> {
        self.body
            .blocks
            .get(index)
            .map(|b| LazyBody::new(&b.body, self.ctx))
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
    use crate::api::parse;

    #[test]
    fn test_lazy_body_on_demand_and_memoization() {
        let src = r#"
            valid_num = 10 + 20
            valid_str = "hello " + "world"
            expensive_val = 100 * 200
            err_attr = 10 / 0
            err_ref = undefined_var + 1
        "#;

        let body = parse(src).unwrap();
        let ctx = Context::new();
        let lazy = LazyBody::new(&body, &ctx);

        assert_eq!(lazy.evaluated_attribute_count(), 0);
        assert!(lazy.has_attribute("valid_num"));
        assert!(lazy.has_attribute("err_attr"));
        assert!(!lazy.has_attribute("nonexistent"));

        let names = lazy.attribute_names();
        assert!(names.contains(&"valid_num"));
        assert!(names.contains(&"expensive_val"));

        // 1. Evaluate valid_num on demand
        assert!(!lazy.is_attribute_evaluated("valid_num"));
        let res_num = lazy.evaluate_attribute("valid_num").unwrap().unwrap();
        assert_eq!(
            res_num.0,
            Value::new(Type::Number, ValueData::Number(30.into()))
        );
        assert!(lazy.is_attribute_evaluated("valid_num"));
        assert_eq!(lazy.evaluated_attribute_count(), 1);

        // 2. Second access uses memoized cache
        let res_num_cached = lazy.evaluate_attribute("valid_num").unwrap().unwrap();
        assert_eq!(res_num_cached.0, res_num.0);
        assert_eq!(lazy.evaluated_attribute_count(), 1);

        // 3. Ensure err_attr and err_ref were NEVER evaluated
        assert!(!lazy.is_attribute_evaluated("err_attr"));
        assert!(!lazy.is_attribute_evaluated("err_ref"));

        // 4. Accessing nonexistent returns None
        assert!(lazy.evaluate_attribute("nonexistent").is_none());

        // 5. Evaluate err_attr explicitly triggers error on demand
        let err_res = lazy.evaluate_attribute("err_attr").unwrap().err().unwrap();
        assert!(
            err_res.errors()[0]
                .error
                .to_string()
                .contains("Division by zero")
        );
        assert!(lazy.is_attribute_evaluated("err_attr"));
        assert_eq!(lazy.evaluated_attribute_count(), 2);

        // 6. Clear cache
        lazy.clear_cache();
        assert_eq!(lazy.evaluated_attribute_count(), 0);
        assert!(!lazy.is_attribute_evaluated("valid_num"));
    }

    #[test]
    fn test_lazy_body_blocks_and_object_conversion() {
        let src = r#"
            name = "my_app"
            port = 8080

            server "web" "primary" {
                listen = "0.0.0.0"
                threads = 4
            }

            server "web" "secondary" {
                listen = "127.0.0.1"
                threads = 2
            }

            database "main" {
                host = "localhost"
            }
        "#;

        let body = parse(src).unwrap();
        let ctx = Context::new();
        let lazy = LazyBody::new(&body, &ctx);

        assert_eq!(lazy.blocks().len(), 3);
        assert_eq!(lazy.get_blocks("server").len(), 2);
        assert_eq!(lazy.get_blocks("database").len(), 1);
        assert_eq!(lazy.get_blocks("nonexistent").len(), 0);

        // Block by label
        let web_pri = lazy.get_block("server", &["web", "primary"]).unwrap();
        let listen = web_pri.evaluate_attribute("listen").unwrap().unwrap();
        assert_eq!(
            listen.0,
            Value::new(Type::String, ValueData::String("0.0.0.0".into()))
        );

        assert!(lazy.get_block("server", &["web", "other"]).is_none());
        assert!(lazy.get_block("server", &["web"]).is_none());

        // Block by index
        let b0 = lazy.get_block_by_index(0).unwrap();
        assert_eq!(b0.body().span, web_pri.body().span);
        assert!(lazy.get_block_by_index(99).is_none());

        // Evaluate all attributes
        let all_attrs = lazy.evaluate_all_attributes();
        assert_eq!(all_attrs.len(), 2);
        assert!(all_attrs.contains_key("name"));
        assert!(all_attrs.contains_key("port"));

        // to_object_value with warning propagation
        let mut warn_diags = Diagnostics::new();
        warn_diags.push(crate::diagnostic::Diagnostic::warning(
            "deprecated attr",
            "",
            crate::span::Span::default(),
        ));
        lazy.lock_cache().insert(
            "name".to_string(),
            Ok((
                Value::new(Type::String, ValueData::String("my_app".into())),
                warn_diags,
            )),
        );

        let (obj_val, diags) = lazy.to_object_value().unwrap();
        assert_eq!(diags.errors().len(), 1);
        let mut expected_map = BTreeMap::new();
        expected_map.insert(
            "name".to_string(),
            Value::new(Type::String, ValueData::String("my_app".into())),
        );
        expected_map.insert(
            "port".to_string(),
            Value::new(Type::Number, ValueData::Number(8080.into())),
        );
        let expected_val = Value::new(
            Type::object(BTreeMap::new()),
            ValueData::Object(expected_map),
        );
        assert_eq!(obj_val, expected_val);

        // Debug format
        assert!(format!("{lazy:?}").contains("LazyBody"));
        assert_eq!(lazy.body().span, body.span);
        assert!(std::ptr::eq(lazy.context(), &raw const ctx));

        // Error path in to_object_value
        let bad_src = "bad = 1 / 0";
        let bad_body = parse(bad_src).unwrap();
        let bad_lazy = LazyBody::new(&bad_body, &ctx);
        assert!(bad_lazy.to_object_value().is_err());

        // Test lock_cache with poisoned mutex
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = bad_lazy.attr_cache.lock();
            panic!("poison mutex");
        }));
        assert_eq!(bad_lazy.evaluated_attribute_count(), 1);
    }
}
