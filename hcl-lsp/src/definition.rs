//! Go-to-definition and reference navigation provider.
//!
//! Enables jumping from variable/local references to declarations (`textDocument/definition`)
//! and finding all occurrences across documents (`textDocument/references`).

use crate::cache::{DocumentCache, VirtualDocument};
use crate::protocol::{Location, Position};
use crate::symbols::span_to_range;
use hashicorp_configuration_language_rs::ast::deps::extract_static_references_from_body;
use hashicorp_configuration_language_rs::ast::expr::{Expression, TemplatePart, TraversalOperator};
use hashicorp_configuration_language_rs::ast::structure::Body;

/// Computes the declaration location for a symbol at the given position.
///
/// # Arguments
/// * `doc` - The active virtual document.
/// * `pos` - The cursor position.
/// * `cache` - The document cache to search across project documents.
///
/// # Returns
/// An optional [`Location`] pointing to the declaring block or attribute.
#[must_use]
pub fn goto_definition(
    doc: &VirtualDocument,
    pos: Position,
    cache: &DocumentCache,
) -> Option<Location> {
    let offset = doc.position_to_offset(pos);
    let body = doc.parsed_body.as_ref()?;

    // Find traversal at cursor offset
    let target_ref = find_traversal_at_offset(doc, body, offset)?;

    // Check what kind of reference it is:
    match (target_ref.root_name.as_str(), target_ref.operators.first()) {
        ("local", Some(TraversalOperator::GetAttr(local_name, _))) => {
            find_local_declaration(local_name, cache)
        }
        ("var", Some(TraversalOperator::GetAttr(var_name, _))) => {
            find_variable_declaration(var_name, cache)
        }
        _ => None,
    }
}

/// Finds all references to a local or variable declared at the given position.
///
/// # Arguments
/// * `doc` - The active virtual document.
/// * `pos` - The cursor position.
/// * `cache` - The document cache to search across project documents.
///
/// # Returns
/// A vector of [`Location`]s referencing the target symbol.
#[must_use]
pub fn find_references(
    doc: &VirtualDocument,
    pos: Position,
    cache: &DocumentCache,
) -> Vec<Location> {
    let offset = doc.position_to_offset(pos);
    let Some(ref body) = doc.parsed_body else {
        return Vec::new();
    };

    // 1. Is the cursor on a `local.<name>` or a declared local inside `locals`?
    let target = find_symbol_target_at_offset(doc, body, offset);
    let Some((root_type, symbol_name)) = target else {
        return Vec::new();
    };

    let mut locations = Vec::new();

    for (uri, vdoc) in &cache.documents {
        let Some(ref doc_body) = vdoc.parsed_body else {
            continue;
        };

        for r in extract_static_references_from_body(doc_body)
            .into_iter()
            .flatten()
        {
            if r.root == root_type
                && let Some(TraversalOperator::GetAttr(name, span)) = r.operators.first()
                && name == &symbol_name
            {
                locations.push(Location {
                    uri: uri.clone(),
                    range: span_to_range(span),
                });
            }
        }
    }

    locations
}

/// Locates the declaration of a local value across cached documents.
fn find_local_declaration(local_name: &str, cache: &DocumentCache) -> Option<Location> {
    for (uri, doc) in &cache.documents {
        let Some(ref body) = doc.parsed_body else {
            continue;
        };
        for block in &body.blocks {
            if block.block_type == "locals"
                && let Some(attr) = block.body.attributes.get(local_name)
            {
                return Some(Location {
                    uri: uri.clone(),
                    range: span_to_range(&attr.name_span),
                });
            }
        }
    }

    None
}

/// Locates the declaration of an input variable across cached documents.
fn find_variable_declaration(var_name: &str, cache: &DocumentCache) -> Option<Location> {
    for (uri, doc) in &cache.documents {
        let Some(ref body) = doc.parsed_body else {
            continue;
        };
        for block in &body.blocks {
            if block.block_type == "variable"
                && let Some(name) = block.labels.first()
                && name == var_name
            {
                let span = block
                    .label_spans
                    .first()
                    .cloned()
                    .unwrap_or(block.type_span.clone());
                return Some(Location {
                    uri: uri.clone(),
                    range: span_to_range(&span),
                });
            }
        }
    }

    None
}

/// A matched variable traversal expression.
#[derive(Debug, Clone)]
struct TraversalMatch {
    /// The root identifier (e.g. "local" or "var").
    root_name: String,
    /// The traversal operators applied to the root.
    operators: Vec<TraversalOperator>,
}

/// Finds the traversal expression at the specified byte offset.
fn find_traversal_at_offset(
    doc: &VirtualDocument,
    body: &Body,
    offset: usize,
) -> Option<TraversalMatch> {
    for attr in body.attributes.values() {
        if attr.span.start_byte <= offset && offset <= attr.span.end_byte {
            let mut matches = Vec::new();
            find_traversals_in_expr(&attr.expr, &mut matches);

            for m in matches {
                let pattern =
                    if let Some(TraversalOperator::GetAttr(attr_name, _)) = m.operators.first() {
                        format!("{}.{attr_name}", m.root_name)
                    } else {
                        m.root_name.clone()
                    };
                let slice = &doc.text[attr.span.start_byte..attr.span.end_byte];
                if let Some(pos_in_slice) = slice.find(&pattern) {
                    let match_start = attr.span.start_byte + pos_in_slice;
                    let match_end = match_start + pattern.len();
                    if match_start <= offset && offset <= match_end {
                        return Some(m);
                    }
                }
            }
        }
    }

    for block in &body.blocks {
        if let Some(m) = find_traversal_at_offset(doc, &block.body, offset) {
            return Some(m);
        }
    }

    None
}

/// Recursively gathers traversal expressions from an expression tree.
fn find_traversals_in_expr(expr: &Expression, out: &mut Vec<TraversalMatch>) {
    match expr {
        Expression::Traversal(trav, _) => {
            if let Expression::Variable(ref root_name, _) = *trav.expr {
                out.push(TraversalMatch {
                    root_name: root_name.clone(),
                    operators: trav.operators.clone(),
                });
            }
        }
        Expression::Template(parts, _) => {
            for part in parts {
                if let TemplatePart::Interpolation(interp, _) = part {
                    find_traversals_in_expr(interp, out);
                }
            }
        }
        Expression::Tuple(elems, _) => {
            for el in elems {
                find_traversals_in_expr(el, out);
            }
        }
        Expression::Object(entries, _) => {
            for (k, v) in entries {
                find_traversals_in_expr(k, out);
                find_traversals_in_expr(v, out);
            }
        }
        Expression::FuncCall(fc, _) => {
            for arg in &fc.args {
                find_traversals_in_expr(arg, out);
            }
        }
        Expression::Conditional(cond, _) => {
            find_traversals_in_expr(&cond.cond_expr, out);
            find_traversals_in_expr(&cond.true_expr, out);
            find_traversals_in_expr(&cond.false_expr, out);
        }
        Expression::BinaryOp(_, l, r, _) => {
            find_traversals_in_expr(l, out);
            find_traversals_in_expr(r, out);
        }
        Expression::UnaryOp(_, inner, _) | Expression::Parentheses(inner, _) => {
            find_traversals_in_expr(inner, out);
        }
        _ => {}
    }
}

/// Identifies the symbol target (kind and name) under the cursor at `offset`.
fn find_symbol_target_at_offset(
    doc: &VirtualDocument,
    body: &Body,
    offset: usize,
) -> Option<(String, String)> {
    // 1. Check if cursor is on declaration of a local
    for block in &body.blocks {
        if block.block_type == "locals" {
            for (name, attr) in &block.body.attributes {
                if attr.name_span.start_byte <= offset && offset <= attr.name_span.end_byte {
                    return Some(("local".to_string(), name.clone()));
                }
            }
        } else if block.block_type == "variable"
            && let Some(var_name) = block.labels.first()
        {
            let label_span = block
                .label_spans
                .first()
                .cloned()
                .unwrap_or(block.type_span.clone());
            if label_span.start_byte <= offset && offset <= label_span.end_byte {
                return Some(("var".to_string(), var_name.clone()));
            }
        }
    }

    // 2. Check if cursor is on a traversal reference
    let trav = find_traversal_at_offset(doc, body, offset)?;
    if (trav.root_name == "local" || trav.root_name == "var")
        && let Some(TraversalOperator::GetAttr(name, _)) = trav.operators.first()
    {
        return Some((trav.root_name, name.clone()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::TextDocumentItem;
    use hashicorp_configuration_language_rs::ast::expr::Traversal;
    use hashicorp_configuration_language_rs::span::Span;

    #[test]
    fn test_goto_definition_and_references() {
        let hcl = r#"
            locals {
                my_setting = "enabled"
            }

            variable "env" {
                default = "prod"
            }

            output = "${local.my_setting}-${var.env}"
        "#;

        let mut cache = DocumentCache::new();
        let item = TextDocumentItem {
            uri: "file:///test.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };
        let doc = cache.open_document(item).clone();

        // 1. Definition of local.my_setting
        let local_ref_offset = doc.text.find("local.my_setting").unwrap_or(0);
        let pos = doc.offset_to_position(local_ref_offset + 6);
        let def = goto_definition(&doc, pos, &cache);
        assert_eq!(
            def.as_ref().map(|d| d.uri.as_str()),
            Some("file:///test.hcl")
        );

        // 2. Definition of var.env
        let var_ref_offset = doc.text.find("var.env").unwrap_or(0);
        let pos_var = doc.offset_to_position(var_ref_offset + 4);
        let def_var = goto_definition(&doc, pos_var, &cache);
        assert_eq!(
            def_var.as_ref().map(|d| d.uri.as_str()),
            Some("file:///test.hcl")
        );

        // 3. References of local.my_setting from declaration
        let decl_offset = doc.text.find("my_setting =").unwrap_or(0);
        let pos_decl = doc.offset_to_position(decl_offset);
        let refs = find_references(&doc, pos_decl, &cache);
        assert_ne!(refs.len(), 0);

        // 4. References of var.env from declaration
        let var_decl_offset = doc.text.find("\"env\"").unwrap_or(0);
        let pos_var_decl = doc.offset_to_position(var_decl_offset + 1);
        let var_refs = find_references(&doc, pos_var_decl, &cache);
        assert_ne!(var_refs.len(), 0);

        // 5. References from traversal reference position
        let refs_from_trav = find_references(&doc, pos, &cache);
        assert_ne!(refs_from_trav.len(), 0);

        let refs_from_var_trav = find_references(&doc, pos_var, &cache);
        assert_ne!(refs_from_var_trav.len(), 0);

        // 6. References at cursor before locals attribute starts (offset 0)
        assert_eq!(find_references(&doc, Position::new(0, 0), &cache).len(), 0);
    }

    #[test]
    fn test_goto_definition_unparsed_and_misses() {
        let mut cache = DocumentCache::new();
        let item = TextDocumentItem {
            uri: "file:///empty.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: "foo = 1".to_string(),
        };
        let mut doc = cache.open_document(item).clone();
        doc.parsed_body = None;

        // 1. Unparsed document
        assert!(goto_definition(&doc, Position::new(0, 0), &cache).is_none());
        assert_eq!(find_references(&doc, Position::new(0, 0), &cache).len(), 0);

        // 2. Position with no traversal
        let item_valid = TextDocumentItem {
            uri: "file:///valid.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: "val = 42\n".to_string(),
        };
        let doc_valid = cache.open_document(item_valid).clone();
        assert!(goto_definition(&doc_valid, Position::new(0, 2), &cache).is_none());
        assert_eq!(
            find_references(&doc_valid, Position::new(0, 2), &cache).len(),
            0
        );
    }

    #[test]
    fn test_goto_definition_undefined_and_other_roots() {
        let hcl = r#"
            locals {
                existing = 123
            }
            resource "aws_s3_bucket" "b" {
                bucket = "b"
            }
            a = local.missing
            b = var.missing
            c = data.aws_ami.ubuntu
            d = local[0]
            e = var[0]
            spaced = local . x
            variable {
                default = "no label"
            }
            variable "empty_var" {
                description = "no labels span target"
            }
        "#;
        let mut cache = DocumentCache::new();
        let item = TextDocumentItem {
            uri: "file:///miss.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };
        let doc = cache.open_document(item).clone();

        // Add broken doc with parsed_body = None to cache
        let broken_item = TextDocumentItem {
            uri: "file:///broken_cache.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: String::new(),
        };
        let mut broken_doc = cache.open_document(broken_item).clone();
        broken_doc.parsed_body = None;
        cache
            .documents
            .insert("file:///broken_cache.hcl".to_string(), broken_doc);

        // 1. Undefined local (checks continue on broken cache doc in find_local_declaration)
        let off_local = doc.text.find("local.missing").unwrap_or(0);
        assert!(goto_definition(&doc, doc.offset_to_position(off_local + 6), &cache).is_none());

        // 2. Undefined var (checks continue on broken cache doc in find_variable_declaration)
        let off_var = doc.text.find("var.missing").unwrap_or(0);
        assert!(goto_definition(&doc, doc.offset_to_position(off_var + 4), &cache).is_none());

        // 3. Other root (data)
        let off_data = doc.text.find("data.aws_ami").unwrap_or(0);
        assert!(goto_definition(&doc, doc.offset_to_position(off_data + 5), &cache).is_none());
        assert_eq!(
            find_references(&doc, doc.offset_to_position(off_data + 5), &cache).len(),
            0
        );

        // 4. Index traversal on local and var (non-GetAttr operator)
        let off_idx_local = doc.text.find("local[0]").unwrap_or(0);
        assert!(goto_definition(&doc, doc.offset_to_position(off_idx_local + 2), &cache).is_none());
        assert_eq!(
            find_references(&doc, doc.offset_to_position(off_idx_local + 2), &cache).len(),
            0
        );

        let off_idx_var = doc.text.find("var[0]").unwrap_or(0);
        assert!(goto_definition(&doc, doc.offset_to_position(off_idx_var + 2), &cache).is_none());

        // 5. Spaced traversal where slice.find(&pattern) is None
        let off_spaced = doc.text.find("local . x").unwrap_or(0);
        assert!(goto_definition(&doc, doc.offset_to_position(off_spaced + 2), &cache).is_none());

        // 6. Cursor inside variable block but not on label
        let off_desc = doc.text.find("description =").unwrap_or(0);
        assert_eq!(
            find_references(&doc, doc.offset_to_position(off_desc), &cache).len(),
            0
        );

        // 7. Variable block without label spans (fallback)
        let off_empty_var = doc.text.find("empty_var").unwrap_or(0);
        let refs = find_references(&doc, doc.offset_to_position(off_empty_var + 1), &cache);
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn test_traversals_in_all_expr_types_and_nested_blocks() {
        let hcl = r#"
            locals {
                k = "key"
                v = "val"
            }
            nested {
                sub = local.k
            }
            list = [local.k]
            map = { my_key = local.k }
            call = upper(local.k)
            ternary = true ? local.k : local.v
            neg = !local.k
        "#;
        let mut cache = DocumentCache::new();
        let item = TextDocumentItem {
            uri: "file:///nested.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };
        let doc = cache.open_document(item).clone();

        // Test definition lookup for occurrences in all expression variants
        let mut start_idx = 0;
        while let Some(pos_idx) = doc.text[start_idx..].find("local.k") {
            let actual_idx = start_idx + pos_idx;
            let pos = doc.offset_to_position(actual_idx + 6);
            let def = goto_definition(&doc, pos, &cache);
            assert_eq!(
                def.as_ref().map(|d| d.uri.as_str()),
                Some("file:///nested.hcl")
            );
            start_idx = actual_idx + 7;
        }

        // Add a document in cache with parsed_body = None to test loop continuation
        let item_broken = TextDocumentItem {
            uri: "file:///broken.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: "invalid hcl".to_string(),
        };
        let mut broken_doc = cache.open_document(item_broken).clone();
        broken_doc.parsed_body = None;
        cache
            .documents
            .insert("file:///broken.hcl".to_string(), broken_doc);

        let k_decl = doc.text.find("k =").unwrap_or(0);
        let refs = find_references(&doc, doc.offset_to_position(k_decl), &cache);
        assert_ne!(refs.len(), 0);
    }

    #[test]
    fn test_non_variable_traversal_and_offset_boundaries() {
        let sp = Span::new(0, 10, 1, 1, 1, 11);
        let non_var_trav = Expression::Traversal(
            Box::new(Traversal {
                expr: Box::new(Expression::Number(
                    hashicorp_configuration_language_rs::number::Number::from(1),
                    sp.clone(),
                )),
                operators: vec![TraversalOperator::GetAttr("attr".to_string(), sp.clone())],
            }),
            sp.clone(),
        );

        let mut matches = Vec::new();
        find_traversals_in_expr(&non_var_trav, &mut matches);
        assert!(matches.is_empty());

        // Traversal with Index operator (non-GetAttr in find_traversal_at_offset)
        let idx_trav = Expression::Traversal(
            Box::new(Traversal {
                expr: Box::new(Expression::Variable("my_arr".to_string(), sp.clone())),
                operators: vec![TraversalOperator::Index(
                    Expression::Number(
                        hashicorp_configuration_language_rs::number::Number::from(0),
                        sp.clone(),
                    ),
                    sp.clone(),
                )],
            }),
            sp.clone(),
        );
        let mut idx_matches = Vec::new();
        find_traversals_in_expr(&idx_trav, &mut idx_matches);
        assert_eq!(idx_matches.len(), 1);

        // Test offset boundaries in find_traversal_at_offset:
        let hcl = "val = local.x + 10\n";
        let item = TextDocumentItem {
            uri: "file:///bound.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };
        for has_body in [true, false] {
            let mut doc = VirtualDocument::new(item.clone());
            if !has_body {
                doc.parsed_body = None;
            }
            if let Some(ref body) = doc.parsed_body {
                // Offset before pattern match (on '=')
                assert!(find_traversal_at_offset(&doc, body, 4).is_none());
                // Offset inside pattern match
                assert!(find_traversal_at_offset(&doc, body, 8).is_some());
                // Offset after pattern match (on '0' of 10)
                assert!(find_traversal_at_offset(&doc, body, 17).is_none());
            }
        }
    }
}
