//! Hierarchical document symbols and outline provider.
//!
//! Generates hierarchical symbols for blocks, labels, attributes, and functions
//! according to Language Server Protocol 3.17 (`textDocument/documentSymbol`).

use crate::cache::VirtualDocument;
use crate::protocol::{DocumentSymbol, Position, Range, SymbolKind};
use hashicorp_configuration_language_rs::ast::structure::{Block, Body};
use hashicorp_configuration_language_rs::span::Span;

/// Generates hierarchical document symbols for the outline view of a document.
///
/// # Arguments
/// * `doc` - The virtual document to analyze.
///
/// # Returns
/// A vector of top-level [`DocumentSymbol`]s with nested children.
#[must_use]
pub fn document_symbols(doc: &VirtualDocument) -> Vec<DocumentSymbol> {
    let Some(ref body) = doc.parsed_body else {
        return Vec::new();
    };

    body_to_symbols(body)
}

fn body_to_symbols(body: &Body) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    // 1. Attributes
    for (name, attr) in &body.attributes {
        let range = span_to_range(&attr.name_span);
        symbols.push(DocumentSymbol {
            name: name.clone(),
            detail: None,
            kind: SymbolKind::PROPERTY,
            range,
            selection_range: range,
            children: None,
        });
    }

    // 2. Regular Blocks
    for block in &body.blocks {
        symbols.push(block_to_symbol(block));
    }

    // 3. User Functions
    for func_block in &body.functions {
        let range = span_to_range(&func_block.span);
        symbols.push(DocumentSymbol {
            name: func_block.name.clone(),
            detail: Some("function".to_string()),
            kind: SymbolKind::FUNCTION,
            range,
            selection_range: range,
            children: None,
        });
    }

    symbols
}

fn block_to_symbol(block: &Block) -> DocumentSymbol {
    let range = span_to_range(&block.span);
    let sel_range = span_to_range(&block.type_span);

    let display_name = if block.labels.is_empty() {
        block.block_type.clone()
    } else {
        format!("{} \"{}\"", block.block_type, block.labels.join("\" \""))
    };

    let mut children = Vec::new();

    // Add labels as children
    for (idx, label) in block.labels.iter().enumerate() {
        let label_span = block
            .label_spans
            .get(idx)
            .cloned()
            .unwrap_or(block.type_span.clone());
        let l_range = span_to_range(&label_span);
        children.push(DocumentSymbol {
            name: label.clone(),
            detail: Some("label".to_string()),
            kind: SymbolKind::STRING,
            range: l_range,
            selection_range: l_range,
            children: None,
        });
    }

    // Add nested body symbols
    let inner_symbols = body_to_symbols(&block.body);
    children.extend(inner_symbols);

    DocumentSymbol {
        name: display_name,
        detail: Some(block.block_type.clone()),
        kind: SymbolKind::NAMESPACE,
        range,
        selection_range: sel_range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

/// Converts an internal [`Span`] to an LSP zero-indexed [`Range`].
///
/// # Arguments
/// * `span` - The source span.
///
/// # Returns
/// An LSP [`Range`].
#[must_use]
pub fn span_to_range(span: &Span) -> Range {
    let start_line = span.start_line.saturating_sub(1) as u32;
    let start_col = span.start_col.saturating_sub(1) as u32;
    let end_line = (span.end_line.saturating_sub(1) as u32).max(start_line);
    let end_col = (span.end_col.saturating_sub(1) as u32).max(start_col);
    Range::new(
        Position::new(start_line, start_col),
        Position::new(end_line, end_col),
    )
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

    #[test]
    fn test_document_symbols_hierarchy() {
        let hcl = r#"
            region = "us-east-1"

            resource "aws_instance" "web" {
                ami           = "ami-12345"
                instance_type = "t3.micro"

                network_interface {
                    device_index = 0
                }
            }
        "#;

        let item = TextDocumentItem {
            uri: "file:///main.tf".to_string(),
            language_id: "terraform".to_string(),
            version: 1,
            text: hcl.to_string(),
        };

        let doc = VirtualDocument::new(item);
        let symbols = document_symbols(&doc);

        assert_eq!(symbols.len(), 2); // 1 attribute + 1 block
        let attr_sym = symbols.iter().find(|s| s.name == "region").unwrap();
        assert_eq!(attr_sym.kind, SymbolKind::PROPERTY);

        let block_sym = symbols
            .iter()
            .find(|s| s.name.contains("aws_instance"))
            .unwrap();
        assert_eq!(block_sym.kind, SymbolKind::NAMESPACE);
        let children = block_sym.children.as_ref().unwrap();

        // Has 2 labels ("aws_instance", "web") + 2 attributes + 1 sub-block
        let label_sym = children.iter().find(|c| c.name == "aws_instance").unwrap();
        assert_eq!(label_sym.kind, SymbolKind::STRING);
        assert!(children.iter().any(|c| c.name == "ami"));
        assert!(children.iter().any(|c| c.name == "network_interface"));
    }

    #[test]
    fn test_document_symbols_edge_cases() {
        use hashicorp_configuration_language_rs::ast::structure::Block;

        // 1. None parsed body
        let item_empty = TextDocumentItem {
            uri: "file:///empty.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: String::new(),
        };
        let mut empty_doc = VirtualDocument::new(item_empty);
        empty_doc.parsed_body = None;
        assert_eq!(document_symbols(&empty_doc), []);

        // 2. Block without labels and empty block (children is None)
        let hcl = r"
            locals {
            }
        ";
        let item = TextDocumentItem {
            uri: "file:///test.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };
        let doc = VirtualDocument::new(item);
        let symbols = document_symbols(&doc);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "locals");
        assert!(symbols[0].children.is_none());

        // 3. User function blocks and block with missing label_spans fallback
        let func_hcl = r#"
            function "calculate" {
                params = []
                result = null
            }
        "#;
        let item_func = TextDocumentItem {
            uri: "file:///func.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: func_hcl.to_string(),
        };
        let mut doc_func = VirtualDocument::new(item_func);
        let sp = Span::new(0, 0, 1, 1, 1, 1);
        let mut block_no_label_spans = Block::new(
            "server".to_string(),
            vec!["node1".to_string()],
            Body::new(sp.clone()),
            sp,
        );
        block_no_label_spans.label_spans.clear();
        let b = doc_func.parsed_body.as_mut().unwrap();
        b.blocks.push(block_no_label_spans);

        let syms = document_symbols(&doc_func);
        let calc = syms.iter().find(|s| s.name == "calculate").unwrap();
        assert_eq!(calc.kind, SymbolKind::FUNCTION);
        let srv = syms.iter().find(|s| s.name.contains("server")).unwrap();
        let srv_children = srv.children.as_ref().unwrap();
        assert_eq!(srv_children[0].name, "node1");
    }
}
