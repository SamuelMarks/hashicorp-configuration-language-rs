//! Hover documentation provider for HCL and Terraform configurations.
//!
//! Generates Markdown hover documentation for block keywords, attribute keys,
//! built-in standard library functions, and user-defined functions.
use crate::cache::VirtualDocument;
use crate::protocol::{Hover, MarkupContent, Position};
use crate::symbols::span_to_range;
use hashicorp_configuration_language_rs::ast::expr::Expression;
use hashicorp_configuration_language_rs::ast::schema::BodySchema;
use hashicorp_configuration_language_rs::ast::structure::Body;
use hashicorp_configuration_language_rs::eval::stdlib::get_stdlib_function;
/// Computes hover documentation at a specific document position.
///
/// # Arguments
/// * `doc` - The virtual document to inspect.
/// * `pos` - The cursor position where hover was triggered.
/// * `schema` - Optional declarative schema describing expected blocks and attributes.
///
/// # Returns
/// An optional [`Hover`] object containing Markdown documentation and the target range.
#[must_use]
pub fn hover_at_position(
    doc: &VirtualDocument,
    pos: Position,
    schema: Option<&BodySchema>,
) -> Option<Hover> {
    let offset = doc.position_to_offset(pos);
    let body = doc.parsed_body.as_ref()?;
    find_hover_in_body(body, offset, schema)
}
/// Traverses a body looking for block keywords, attribute keys, or user functions matching offset.
fn find_hover_in_body(body: &Body, offset: usize, schema: Option<&BodySchema>) -> Option<Hover> {
    for (name, attr) in &body.attributes {
        if attr.name_span.start_byte <= offset && offset <= attr.name_span.end_byte {
            let attr_schema = schema.and_then(|s| s.attributes.get(name));
            let mut md = format!(
                "### Attribute `{name}`

        "
            );
            if let Some(aschem) = attr_schema {
                if let Some(ref ty) = aschem.expected_type {
                    md.push_str(&format!(
                        "**Type:** `{ty}`

        "
                    ));
                }
                if aschem.required {
                    md.push_str(
                        "**Required:** `true`

        ",
                    );
                } else {
                    md.push_str(
                        "**Required:** `false` (optional)

        ",
                    );
                }
                if let Some(ref desc) = aschem.description {
                    md.push_str(desc);
                }
            } else {
                md.push_str("Configuration attribute.");
            }
            return Some(Hover {
                contents: MarkupContent::markdown(md),
                range: Some(span_to_range(&attr.name_span)),
            });
        }
        if let Some(h) = find_hover_in_expr(&attr.expr, offset) {
            return Some(h);
        }
    }
    for block in &body.blocks {
        if block.type_span.start_byte <= offset && offset <= block.type_span.end_byte {
            let block_schema = schema.and_then(|s| s.blocks.get(&block.block_type));
            let mut md = format!(
                "### Block `{}`

        ",
                block.block_type
            );
            if let Some(bschem) = block_schema {
                if let Some(ref desc) = bschem.description {
                    md.push_str(desc);
                    md.push_str(
                        "

        ",
                    );
                }
                if !bschem.label_names.is_empty() {
                    md.push_str(&format!(
                        "**Labels:** `{}`",
                        bschem.label_names.join("`, `")
                    ));
                }
            } else {
                md.push_str("Configuration block.");
            }
            return Some(Hover {
                contents: MarkupContent::markdown(md),
                range: Some(span_to_range(&block.type_span)),
            });
        }
        let inner_schema = schema
            .and_then(|s| s.blocks.get(&block.block_type))
            .and_then(|bs| bs.body_schema.as_ref());
        if let Some(h) = find_hover_in_body(&block.body, offset, inner_schema) {
            return Some(h);
        }
    }
    for func in &body.functions {
        if func.span.start_byte <= offset && offset <= func.span.end_byte {
            let mut md = format!("### Function `{}`\n\n", func.name);
            let params = func
                .params
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            md.push_str(&format!(
                "`{}({})`\n\nUser-defined function.",
                func.name, params
            ));
            return Some(Hover {
                contents: MarkupContent::markdown(md),
                range: Some(span_to_range(&func.span)),
            });
        }
    }
    None
}
/// Recursively inspects expressions to find function calls or symbols matching cursor offset.
fn find_hover_in_expr(expr: &Expression, offset: usize) -> Option<Hover> {
    match expr {
        Expression::FuncCall(fc, _) => {
            if fc.name.span.start_byte <= offset && offset <= fc.name.span.end_byte {
                let func_name = fc.name.to_string();
                if let Some(stdlib_fn) = get_stdlib_function(&func_name) {
                    let mut md = format!("### Built-in Function `{func_name}`\n\n");
                    if let Some(sig) = &stdlib_fn.signature {
                        if let Some(ref desc) = sig.description {
                            md.push_str(desc);
                            md.push_str("\n\n");
                        }
                        let params = sig
                            .params
                            .iter()
                            .map(|p| format!("{}: {}", p.name, p.param_type))
                            .collect::<Vec<_>>()
                            .join(", ");
                        md.push_str(&format!("```hcl\n{func_name}({params})\n```\n"));
                    }
                    return Some(Hover {
                        contents: MarkupContent::markdown(md),
                        range: Some(span_to_range(&fc.name.span)),
                    });
                }
            }
            for arg in &fc.args {
                if let Some(h) = find_hover_in_expr(arg, offset) {
                    return Some(h);
                }
            }
        }
        Expression::Tuple(elems, _) => {
            for el in elems {
                if let Some(h) = find_hover_in_expr(el, offset) {
                    return Some(h);
                }
            }
        }
        Expression::Object(entries, _) => {
            for (k, v) in entries {
                if let Some(h) = find_hover_in_expr(k, offset) {
                    return Some(h);
                }
                if let Some(h) = find_hover_in_expr(v, offset) {
                    return Some(h);
                }
            }
        }
        Expression::BinaryOp(_, l, r, _) => {
            if let Some(h) = find_hover_in_expr(l, offset) {
                return Some(h);
            }
            if let Some(h) = find_hover_in_expr(r, offset) {
                return Some(h);
            }
        }
        Expression::UnaryOp(_, inner, _) | Expression::Parentheses(inner, _) => {
            if let Some(h) = find_hover_in_expr(inner, offset) {
                return Some(h);
            }
        }
        Expression::Conditional(cond, _) => {
            if let Some(h) = find_hover_in_expr(&cond.cond_expr, offset) {
                return Some(h);
            }
            if let Some(h) = find_hover_in_expr(&cond.true_expr, offset) {
                return Some(h);
            }
            if let Some(h) = find_hover_in_expr(&cond.false_expr, offset) {
                return Some(h);
            }
        }
        Expression::Null(_)
        | Expression::Bool(_, _)
        | Expression::Number(_, _)
        | Expression::String(_, _)
        | Expression::Variable(_, _)
        | Expression::Template(_, _)
        | Expression::Traversal(_, _)
        | Expression::ForExpr(_, _) => {}
    }
    None
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
    use crate::protocol::TextDocumentItem;
    use hashicorp_configuration_language_rs::ast::schema::{AttributeSchema, BlockHeaderSchema};
    use hashicorp_configuration_language_rs::ast::user_func::{FunctionBlock, FunctionParam};
    use hashicorp_configuration_language_rs::span::Span;
    use hashicorp_configuration_language_rs::types::Type;
    #[test]
    fn test_hover_on_attribute_and_function() {
        let hcl = r#"
            name = upper("web")
            resource "server" "srv" {
                ip = "10.0.0.1"
            }
        "#;
        let schema = BodySchema::new()
            .with_attribute(
                AttributeSchema::required("name")
                    .with_type(Type::String)
                    .with_description("The server resource name."),
            )
            .with_block(
                BlockHeaderSchema::new("server", vec!["id".to_string()])
                    .with_description("Server configuration block."),
            );
        let item = TextDocumentItem {
            uri: "file:///main.tf".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };
        let doc = VirtualDocument::new(item);
        let name_pos = Position::new(1, 13);
        let hover_attr = hover_at_position(&doc, name_pos, Some(&schema));
        assert_eq!(
            hover_attr
                .as_ref()
                .map(|h| h.contents.value.contains("The server resource name.")),
            Some(true)
        );
        assert_eq!(
            hover_attr
                .as_ref()
                .map(|h| h.contents.value.contains("Type:")),
            Some(true)
        );
        assert_eq!(
            hover_attr
                .as_ref()
                .map(|h| h.contents.value.contains("**Required:** `true`")),
            Some(true)
        );
        let upper_pos = Position::new(1, 20);
        let hover_fn = hover_at_position(&doc, upper_pos, Some(&schema));
        assert_eq!(
            hover_fn
                .as_ref()
                .map(|h| h.contents.value.contains("upper")),
            Some(true)
        );
        let res_pos = Position::new(2, 14);
        let hover_res = hover_at_position(&doc, res_pos, Some(&schema));
        assert!(hover_res.is_some());
    }
    #[test]
    fn test_hover_unparsed_doc_and_miss() {
        let item = TextDocumentItem {
            uri: "file:///miss.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: "foo = 1".to_string(),
        };
        let mut doc = VirtualDocument::new(item);
        doc.parsed_body = None;
        assert!(hover_at_position(&doc, Position::new(0, 0), None).is_none());
        let doc2 = VirtualDocument::new(TextDocumentItem {
            uri: "file:///miss2.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: "foo = 1".to_string(),
        });
        assert!(hover_at_position(&doc2, Position::new(10, 10), None).is_none());
    }
    #[test]
    fn test_hover_attribute_without_schema_and_optional() {
        let hcl = "tag = \"prod\"\n";
        let doc = VirtualDocument::new(TextDocumentItem {
            uri: "file:///tag.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        });
        let h_no_schema = hover_at_position(&doc, Position::new(0, 1), None);
        assert_eq!(
            h_no_schema
                .as_ref()
                .map(|h| h.contents.value.contains("Configuration attribute.")),
            Some(true)
        );
        let schema = BodySchema::new().with_attribute(AttributeSchema::optional("tag"));
        let h_opt = hover_at_position(&doc, Position::new(0, 1), Some(&schema));
        assert_eq!(
            h_opt.as_ref().map(|h| h
                .contents
                .value
                .contains("**Required:** `false` (optional)")),
            Some(true)
        );
        assert_eq!(
            h_opt
                .as_ref()
                .map(|h| h.contents.value.contains("**Type:**")),
            Some(false)
        );
    }
    #[test]
    fn test_hover_block_variants() {
        let hcl = r#"// leading line 0
            cluster "alpha" {
                size = 3
            }
            untyped {
                enabled = true
            }
        "#;
        let inner_schema = BodySchema::new().with_attribute(
            AttributeSchema::required("size").with_description("Cluster size count."),
        );
        let schema = BodySchema::new()
            .with_block(
                BlockHeaderSchema::new("cluster", vec!["name".to_string()])
                    .with_description("Cluster config")
                    .with_body_schema(inner_schema),
            )
            .with_block(BlockHeaderSchema::new("untyped", vec![]));
        let doc = VirtualDocument::new(TextDocumentItem {
            uri: "file:///block.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        });
        assert!(hover_at_position(&doc, Position::new(0, 0), Some(&schema)).is_none());
        let cluster_offset = doc.text.find("cluster").unwrap_or(0);
        let cluster_pos = doc.offset_to_position(cluster_offset + 2);
        let h_cluster = hover_at_position(&doc, cluster_pos, Some(&schema));
        assert_eq!(
            h_cluster
                .as_ref()
                .map(|h| h.contents.value.contains("Cluster config")),
            Some(true)
        );
        assert_eq!(
            h_cluster
                .as_ref()
                .map(|h| h.contents.value.contains("**Labels:** `name`")),
            Some(true)
        );
        let size_offset = doc.text.find("size").unwrap_or(0);
        let size_pos = doc.offset_to_position(size_offset + 1);
        let h_size = hover_at_position(&doc, size_pos, Some(&schema));
        assert_eq!(
            h_size
                .as_ref()
                .map(|h| h.contents.value.contains("Cluster size count.")),
            Some(true)
        );
        let untyped_offset = doc.text.find("untyped").unwrap_or(0);
        let untyped_pos = doc.offset_to_position(untyped_offset + 2);
        let h_untyped = hover_at_position(&doc, untyped_pos, Some(&schema));
        assert_eq!(
            h_untyped
                .as_ref()
                .map(|h| h.contents.value.contains("**Labels:**")),
            Some(false)
        );
        let h_no_schema = hover_at_position(&doc, untyped_pos, None);
        assert_eq!(
            h_no_schema
                .as_ref()
                .map(|h| h.contents.value.contains("Configuration block.")),
            Some(true)
        );
    }
    #[test]
    fn test_hover_user_functions() {
        let hcl = "x = 1\n          ";
        let mut doc = VirtualDocument::new(TextDocumentItem {
            uri: "file:///func.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        });
        let fn_span = Span::new(10, 15, 1, 11, 1, 16);
        let param_span = Span::new(12, 13, 1, 13, 1, 14);
        let ufunc = FunctionBlock {
            name: "my_sum".to_string(),
            params: vec![FunctionParam {
                name: "val".to_string(),
                type_expr: None,
                span: param_span,
            }],
            variadic_param: None,
            return_type: None,
            body: Expression::Null(fn_span.clone()),
            span: fn_span,
        };
        for has_body in [true, false] {
            let mut target_doc = if has_body {
                doc.clone()
            } else {
                let mut d = doc.clone();
                d.parsed_body = None;
                d
            };
            if let Some(body) = target_doc.parsed_body.as_mut() {
                body.functions.push(ufunc.clone());
                doc = target_doc;
            }
        }
        let h = hover_at_position(&doc, Position::new(1, 6), None);
        assert_eq!(
            h.as_ref().map(|v| v.contents.value.contains("my_sum(val)")),
            Some(true)
        );
        assert_eq!(
            h.as_ref()
                .map(|v| v.contents.value.contains("User-defined function.")),
            Some(true)
        );
        let h_before = hover_at_position(&doc, Position::new(0, 2), None);
        assert!(h_before.is_none());
        let h_after = hover_at_position(&doc, Position::new(1, 10), None);
        assert!(h_after.is_none());
    }
    #[test]
    fn test_hover_in_nested_expressions() {
        let hcl = r#"
            a = [upper("tuple")]
            b = { (upper("k")) = upper("v") }
            c = upper("left") + upper("right")
            d = !upper("unary")
            e = (upper("paren"))
            f = upper("c") ? upper("t") : upper("f")
            g = unknown_func(upper("arg"))
            h = unknown_no_args()
            i = regex_replace("abc", "b", "z")
            j = range(5)
        "#;
        let doc = VirtualDocument::new(TextDocumentItem {
            uri: "file:///expr.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        });
        let mut start_idx = 0;
        while let Some(pos_idx) = doc.text[start_idx..].find("upper") {
            let actual_idx = start_idx + pos_idx;
            let pos = doc.offset_to_position(actual_idx + 2);
            let h = hover_at_position(&doc, pos, None);
            assert_eq!(
                h.as_ref().map(|val| val.contents.value.contains("upper")),
                Some(true)
            );
            start_idx = actual_idx + 5;
        }
        let unknown_idx = doc.text.find("unknown_no_args").unwrap_or(0);
        let unknown_pos = doc.offset_to_position(unknown_idx + 3);
        assert!(hover_at_position(&doc, unknown_pos, None).is_none());
        let reg_idx = doc.text.find("regex_replace").unwrap_or(0);
        let reg_pos = doc.offset_to_position(reg_idx + 2);
        let h_reg = hover_at_position(&doc, reg_pos, None);
        assert_eq!(
            h_reg
                .as_ref()
                .map(|val| val.contents.value.contains("regex_replace(")),
            Some(true)
        );
        assert_eq!(
            h_reg
                .as_ref()
                .map(|val| val.contents.value.contains("Replaces all occurrences")),
            Some(true)
        );
        let range_idx = doc.text.find("range(5)").unwrap_or(0);
        let range_pos = doc.offset_to_position(range_idx + 2);
        let h_range = hover_at_position(&doc, range_pos, None);
        assert_eq!(
            h_range
                .as_ref()
                .map(|val| val.contents.value.contains("range(")),
            Some(true)
        );
    }
}
