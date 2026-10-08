//! Hierarchical document symbols and outline provider.
//!
//! Generates hierarchical symbols for blocks, labels, attributes, and functions
//! according to Language Server Protocol 3.17 (`textDocument/documentSymbol`).
use crate::cache::VirtualDocument;
use crate::protocol::{DocumentSymbol, Location, Position, Range, SymbolKind, WorkspaceSymbol};

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
    for block in &body.blocks {
        symbols.push(block_to_symbol(block));
    }
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

/// Generates flattened workspace symbols for a virtual document.
///
/// # Arguments
/// * `uri` - The URI of the document.
/// * `doc` - The virtual document to analyze.
/// * `query` - Optional query string to filter symbols.
///
/// # Returns
/// A vector of flat [`WorkspaceSymbol`]s.
#[must_use]
pub fn workspace_symbols(uri: &str, doc: &VirtualDocument, query: &str) -> Vec<WorkspaceSymbol> {
    let mut ws_symbols = Vec::new();
    let doc_symbols = document_symbols(doc);
    flatten_symbols(uri, None, &doc_symbols, &mut ws_symbols, query);
    ws_symbols
}

fn flatten_symbols(
    uri: &str,
    container_name: Option<String>,
    symbols: &[DocumentSymbol],
    out: &mut Vec<WorkspaceSymbol>,
    query: &str,
) {
    for sym in symbols {
        let matches = query.is_empty() || sym.name.to_lowercase().contains(&query.to_lowercase());
        if matches {
            out.push(WorkspaceSymbol {
                name: sym.name.clone(),
                kind: sym.kind,
                location: Location {
                    uri: uri.to_string(),
                    range: sym.selection_range,
                },
                container_name: container_name.clone(),
            });
        }
        if let Some(ref children) = sym.children {
            flatten_symbols(uri, Some(sym.name.clone()), children, out, query);
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
        let doc = VirtualDocument::new(item, None);
        let symbols = document_symbols(&doc);
        assert_eq!(symbols.len(), 2);
        let attr_sym = symbols.iter().find(|s| s.name == "region").unwrap();
        assert_eq!(attr_sym.kind, SymbolKind::PROPERTY);
        let block_sym = symbols
            .iter()
            .find(|s| s.name.contains("aws_instance"))
            .unwrap();
        assert_eq!(block_sym.kind, SymbolKind::NAMESPACE);
        let children = block_sym.children.as_ref().unwrap();
        let label_sym = children.iter().find(|c| c.name == "aws_instance").unwrap();
        assert_eq!(label_sym.kind, SymbolKind::STRING);
        assert!(children.iter().any(|c| c.name == "ami"));
        assert!(children.iter().any(|c| c.name == "network_interface"));
    }
    #[test]
    fn test_document_symbols_edge_cases() {
        use hashicorp_configuration_language_rs::ast::structure::Block;
        let item_empty = TextDocumentItem {
            uri: "file:///empty.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: String::new(),
        };
        let mut empty_doc = VirtualDocument::new(item_empty, None);
        empty_doc.parsed_body = None;
        assert_eq!(document_symbols(&empty_doc), []);
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
        let doc = VirtualDocument::new(item, None);
        let symbols = document_symbols(&doc);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "locals");
        assert!(symbols[0].children.is_none());
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
        let mut doc_func = VirtualDocument::new(item_func, None);
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
