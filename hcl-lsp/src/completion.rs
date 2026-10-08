//! Schema-driven autocompletion engine for HCL.
//!
//! Delivers context-aware autocompletions for block types, block labels,
//! attribute names, standard library functions, and in-scope variables.
use crate::cache::VirtualDocument;
use crate::protocol::{
    CompletionItem, CompletionItemKind, CompletionList, MarkupContent, Position,
};
use hashicorp_configuration_language_rs::ast::schema::BodySchema;
use hashicorp_configuration_language_rs::eval::stdlib::stdlib_map;
/// Computes autocompletions at a specified document position.
///
/// # Arguments
/// * `doc` - The virtual document to inspect.
/// * `_pos` - The cursor position where completion was triggered.
/// * `schema` - Optional declarative schema providing block and attribute specs.
///
/// # Returns
/// A [`CompletionList`] containing candidate items.
#[must_use]
pub fn completions_at_position(
    doc: &VirtualDocument,
    _pos: Position,
    schema: Option<&BodySchema>,
) -> CompletionList {
    let mut items = Vec::new();
    if let Some(s) = schema {
        for (b_type, b_spec) in &s.blocks {
            let label_placeholder = if b_spec.label_names.is_empty() {
                String::new()
            } else {
                format!(
                    " \"{}\"",
                    b_spec
                        .label_names
                        .iter()
                        .map(|l| format!("<{l}>"))
                        .collect::<Vec<_>>()
                        .join("\" \"")
                )
            };
            let insert_text = format!(
                "{b_type}{label_placeholder} {{
  $0
}}"
            );
            items.push(CompletionItem {
                label: b_type.clone(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some("Block".to_string()),
                documentation: b_spec
                    .description
                    .as_ref()
                    .map(|d| MarkupContent::markdown(d.clone())),
                insert_text: Some(insert_text),
            });
        }
        for (a_name, a_spec) in &s.attributes {
            let detail = if a_spec.required {
                "Required Attribute".to_string()
            } else {
                "Optional Attribute".to_string()
            };
            items.push(CompletionItem {
                label: a_name.clone(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(detail),
                documentation: a_spec
                    .description
                    .as_ref()
                    .map(|d| MarkupContent::markdown(d.clone())),
                insert_text: Some(format!("{a_name} = ")),
            });
        }
    }
    for (fn_name, fn_obj) in stdlib_map() {
        let sig_str = fn_obj.signature.as_ref().map_or_else(
            || format!("{fn_name}()"),
            |sig| {
                let params = sig
                    .params
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{fn_name}({params})")
            },
        );
        let doc_comment = fn_obj
            .signature
            .as_ref()
            .and_then(|s| s.description.clone())
            .map(MarkupContent::markdown);
        items.push(CompletionItem {
            label: fn_name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(sig_str),
            documentation: doc_comment,
            insert_text: Some(format!("{fn_name}($0)")),
        });
    }
    if let Some(ref body) = doc.parsed_body {
        for block in &body.blocks {
            if block.block_type == "locals" {
                for name in block.body.attributes.keys() {
                    items.push(CompletionItem {
                        label: format!("local.{name}"),
                        kind: Some(CompletionItemKind::VARIABLE),
                        detail: Some("Local variable".to_string()),
                        documentation: None,
                        insert_text: Some(format!("local.{name}")),
                    });
                }
            } else if block.block_type == "variable" {
                if let Some(var_name) = block.labels.first() {
                    items.push(CompletionItem {
                        label: format!("var.{var_name}"),
                        kind: Some(CompletionItemKind::VARIABLE),
                        detail: Some("Input variable".to_string()),
                        documentation: None,
                        insert_text: Some(format!("var.{var_name}")),
                    });
                }
            }
        }
    }
    CompletionList {
        is_incomplete: false,
        items,
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
    use crate::protocol::TextDocumentItem;
    use hashicorp_configuration_language_rs::ast::schema::{AttributeSchema, BlockHeaderSchema};
    #[test]
    fn test_completion_items() {
        let hcl = r#"
            locals {
                my_local = 10
            }
            variable "region" {
                default = "us-west-2"
            }
            output "endpoint" {
                value = "https://example.com"
            }
        "#;
        let schema = BodySchema::new()
            .with_attribute(
                AttributeSchema::required("instance_type").with_description("Instance type"),
            )
            .with_attribute(
                AttributeSchema::optional("disk_size").with_description("Disk size in GB"),
            )
            .with_block(
                BlockHeaderSchema::new("resource", vec!["type".to_string(), "name".to_string()])
                    .with_description("Resource block"),
            )
            .with_block(BlockHeaderSchema::new("server", vec![]));
        let item = TextDocumentItem {
            uri: "file:///test.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };
        let doc = VirtualDocument::new(item, None);
        let list = completions_at_position(&doc, Position::new(0, 0), Some(&schema));
        assert_ne!(list.items.len(), 0);
        assert!(list.items.iter().any(|i| i.label == "instance_type"));
        assert!(list.items.iter().any(|i| i.label == "disk_size"));
        assert!(list.items.iter().any(|i| i.label == "resource"));
        assert!(list.items.iter().any(|i| i.label == "server"));
        assert!(list.items.iter().any(|i| i.label == "abs"));
        assert!(list.items.iter().any(|i| i.label == "local.my_local"));
        assert!(list.items.iter().any(|i| i.label == "var.region"));
        let item_empty = TextDocumentItem {
            uri: "file:///empty.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: String::new(),
        };
        let mut unparsed_doc = VirtualDocument::new(item_empty, None);
        unparsed_doc.parsed_body = None;
        let list_no_schema = completions_at_position(&unparsed_doc, Position::new(0, 0), None);
        assert!(list_no_schema.items.iter().any(|i| i.label == "abs"));
        let item_unlabeled_var = TextDocumentItem {
            uri: "file:///unlabeled.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: "variable { default = 1 }".to_string(),
        };
        let doc_unlabeled = VirtualDocument::new(item_unlabeled_var, None);
        let list_unlabeled = completions_at_position(&doc_unlabeled, Position::new(0, 0), None);
        assert!(
            !list_unlabeled
                .items
                .iter()
                .any(|i| i.label.starts_with("var."))
        );
    }
}
