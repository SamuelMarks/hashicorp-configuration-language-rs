//! Semantic tokens highlighting provider.
//!
//! Classifies tokens into keywords, block types, block labels, attribute names,
//! functions, numbers, strings, and operators according to LSP 3.17 delta format.

use crate::cache::VirtualDocument;
use crate::protocol::{Range, SemanticTokens, SemanticTokensLegend};
use hashicorp_configuration_language_rs::ast::expr::Expression;
use hashicorp_configuration_language_rs::ast::structure::Body;
use hashicorp_configuration_language_rs::span::Span;

/// Standard token types advertised by this language server.
pub const TOKEN_TYPES: &[&str] = &[
    "keyword",  // 0
    "type",     // 1
    "class",    // 2
    "property", // 3
    "variable", // 4
    "string",   // 5
    "number",   // 6
    "operator", // 7
    "function", // 8
    "comment",  // 9
];

/// Standard token modifiers advertised by this language server.
pub const TOKEN_MODIFIERS: &[&str] = &[
    "declaration",    // 1 (1 << 0)
    "readonly",       // 2 (1 << 1)
    "defaultLibrary", // 4 (1 << 2)
];

/// Returns the semantic tokens legend describing token types and modifiers.
#[must_use]
pub fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.iter().map(|&s| s.to_string()).collect(),
        token_modifiers: TOKEN_MODIFIERS.iter().map(|&s| s.to_string()).collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawSemanticToken {
    /// Zero-based line index.
    line: u32,
    /// Zero-based character offset within the line.
    character: u32,
    /// Length of the token in characters.
    length: u32,
    /// Type index corresponding to `TOKEN_TYPES`.
    token_type: u32,
    /// Modifiers bitmask corresponding to `TOKEN_MODIFIERS`.
    token_modifiers: u32,
}

/// Generates semantic tokens for the entire document.
///
/// # Arguments
/// * `doc` - The virtual document to analyze.
///
/// # Returns
/// A [`SemanticTokens`] structure containing delta-encoded data.
#[must_use]
pub fn semantic_tokens_full(doc: &VirtualDocument) -> SemanticTokens {
    let Some(ref body) = doc.parsed_body else {
        return SemanticTokens::default();
    };

    let mut raw_tokens = Vec::new();
    collect_body_tokens(body, &mut raw_tokens);
    raw_tokens.sort_by_key(|t| (t.line, t.character));

    encode_semantic_tokens(&raw_tokens)
}

/// Generates semantic tokens within a specific document range.
///
/// # Arguments
/// * `doc` - The virtual document to analyze.
/// * `range` - The target range.
///
/// # Returns
/// A [`SemanticTokens`] structure containing delta-encoded data.
#[must_use]
pub fn semantic_tokens_range(doc: &VirtualDocument, range: Range) -> SemanticTokens {
    let Some(ref body) = doc.parsed_body else {
        return SemanticTokens::default();
    };

    let mut raw_tokens = Vec::new();
    collect_body_tokens(body, &mut raw_tokens);
    raw_tokens.retain(|t| {
        t.line >= range.start.line
            && t.line <= range.end.line
            && (t.line > range.start.line || t.character >= range.start.character)
            && (t.line < range.end.line || t.character <= range.end.character)
    });
    raw_tokens.sort_by_key(|t| (t.line, t.character));

    encode_semantic_tokens(&raw_tokens)
}

/// Traverses a body collecting tokens for attributes, blocks, and child expressions.
fn collect_body_tokens(body: &Body, tokens: &mut Vec<RawSemanticToken>) {
    // 1. Attributes
    for (name, attr) in &body.attributes {
        add_token(tokens, &attr.name_span, 3, 1); // property, declaration
        collect_expr_tokens(&attr.expr, tokens);
        let _ = name;
    }

    // 2. Blocks
    for block in &body.blocks {
        add_token(tokens, &block.type_span, 1, 0); // type

        for label_span in &block.label_spans {
            add_token(tokens, label_span, 5, 1); // string, declaration
        }

        collect_body_tokens(&block.body, tokens);
    }
}

/// Recursively traverses an expression adding tokens for sub-expressions.
fn collect_expr_tokens(expr: &Expression, tokens: &mut Vec<RawSemanticToken>) {
    match expr {
        Expression::Number(_, span) => {
            add_token(tokens, span, 6, 0); // number
        }
        Expression::String(_, span) => {
            add_token(tokens, span, 5, 0); // string
        }
        Expression::Bool(_, span) => {
            add_token(tokens, span, 0, 0); // keyword
        }
        Expression::Variable(_, span) => {
            add_token(tokens, span, 4, 0); // variable
        }
        Expression::FuncCall(fc, _) => {
            add_token(tokens, &fc.name.span, 8, 0); // function
            for arg in &fc.args {
                collect_expr_tokens(arg, tokens);
            }
        }
        Expression::Tuple(elems, _) => {
            for el in elems {
                collect_expr_tokens(el, tokens);
            }
        }
        Expression::Object(entries, _) => {
            for (k, v) in entries {
                collect_expr_tokens(k, tokens);
                collect_expr_tokens(v, tokens);
            }
        }
        Expression::BinaryOp(_, l, r, _) => {
            collect_expr_tokens(l, tokens);
            collect_expr_tokens(r, tokens);
        }
        Expression::UnaryOp(_, inner, _) | Expression::Parentheses(inner, _) => {
            collect_expr_tokens(inner, tokens);
        }
        Expression::Conditional(cond, _) => {
            collect_expr_tokens(&cond.cond_expr, tokens);
            collect_expr_tokens(&cond.true_expr, tokens);
            collect_expr_tokens(&cond.false_expr, tokens);
        }
        Expression::Null(_)
        | Expression::Template(_, _)
        | Expression::Traversal(_, _)
        | Expression::ForExpr(_, _) => {}
    }
}

/// Converts a source span into a raw semantic token if valid.
fn add_token(
    tokens: &mut Vec<RawSemanticToken>,
    span: &Span,
    token_type: u32,
    token_modifiers: u32,
) {
    if span.end_byte <= span.start_byte {
        return;
    }
    let line = span.start_line.saturating_sub(1) as u32;
    let character = span.start_col.saturating_sub(1) as u32;
    let length = (span.end_byte - span.start_byte) as u32;

    tokens.push(RawSemanticToken {
        line,
        character,
        length,
        token_type,
        token_modifiers,
    });
}

/// Encodes raw semantic tokens into LSP delta format.
fn encode_semantic_tokens(tokens: &[RawSemanticToken]) -> SemanticTokens {
    let mut data = Vec::with_capacity(tokens.len() * 5);
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    for token in tokens {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.character.saturating_sub(prev_char)
        } else {
            token.character
        };

        data.push(delta_line);
        data.push(delta_start);
        data.push(token.length);
        data.push(token.token_type);
        data.push(token.token_modifiers);

        prev_line = token.line;
        prev_char = token.character;
    }

    SemanticTokens {
        result_id: None,
        data,
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
    use crate::protocol::{Position, TextDocumentItem};

    #[test]
    fn test_semantic_tokens_full_and_range() {
        let hcl = r#"
            count = 42
            name  = "web"
            block "id" {
                val = abs(-5)
            }
        "#;

        let item = TextDocumentItem {
            uri: "file:///test.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };

        let doc = VirtualDocument::new(item);
        let legend = semantic_tokens_legend();
        assert_eq!(legend.token_types.len(), 10);
        assert_eq!(legend.token_modifiers.len(), 3);

        let tokens = semantic_tokens_full(&doc);
        assert_ne!(tokens.data.len(), 0);
        assert_eq!(tokens.data.len() % 5, 0);

        let range_tokens =
            semantic_tokens_range(&doc, Range::new(Position::new(1, 0), Position::new(2, 50)));
        assert_ne!(range_tokens.data.len(), 0);
    }

    #[test]
    fn test_semantic_tokens_unparsed_doc() {
        let item = TextDocumentItem {
            uri: "file:///empty.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: String::new(),
        };
        let mut doc = VirtualDocument::new(item);
        doc.parsed_body = None;

        let full = semantic_tokens_full(&doc);
        assert_eq!(full.data.len(), 0);

        let range =
            semantic_tokens_range(&doc, Range::new(Position::new(0, 0), Position::new(5, 5)));
        assert_eq!(range.data.len(), 0);
    }

    #[test]
    fn test_semantic_tokens_all_expr_variants() {
        let hcl = r#"
            flag = true
            flag2 = false
            v = my_var
            tpl = [1, 2, "three"]
            obj = { a = 1, b = 2 }
            math = 10 + 20
            neg = -(5)
            ternary = true ? 1 : 0
            empty_tmpl = ""
            trav = foo.bar
            null_val = null
            comprehension = [for x in [1]: x]
        "#;

        let item = TextDocumentItem {
            uri: "file:///all.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };

        let doc = VirtualDocument::new(item);
        let tokens = semantic_tokens_full(&doc);
        assert_ne!(tokens.data.len(), 0);
    }

    #[test]
    fn test_add_token_zero_and_inverted_span() {
        let mut tokens = Vec::new();
        let zero_span = Span::new(10, 10, 1, 1, 1, 1);
        add_token(&mut tokens, &zero_span, 0, 0);
        assert_eq!(tokens.len(), 0);

        let inv_span = Span::new(10, 5, 1, 1, 1, 1);
        add_token(&mut tokens, &inv_span, 0, 0);
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_semantic_tokens_range_filtering_branches() {
        let hcl = "a = 1\nb = 2\nc = 3\nd = 4\ne = 5\n";
        let item = TextDocumentItem {
            uri: "file:///lines.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };
        let doc = VirtualDocument::new(item);

        // Filter range targeting lines 1 to 3 with specific character boundaries
        // line 0 ("a = 1") is t.line < range.start.line
        // line 1 ("b = 2"): token at char 0 ("b") and char 4 ("2")
        // line 2 ("c = 3"): middle line, t.line > range.start.line && t.line < range.end.line
        // line 3 ("d = 4"): token at char 0 ("d") and char 4 ("4")
        // line 4 ("e = 5"): t.line > range.end.line
        let range1 = Range::new(Position::new(1, 2), Position::new(3, 2));
        let tokens1 = semantic_tokens_range(&doc, range1);
        assert_ne!(tokens1.data.len(), 0);

        // Range covering boundary edge where char on start line is rejected
        let range2 = Range::new(Position::new(1, 10), Position::new(1, 12));
        let tokens2 = semantic_tokens_range(&doc, range2);
        assert_eq!(tokens2.data.len(), 0);

        // Range covering boundary edge where tokens on line 1 are outside [2, 3]
        let range3 = Range::new(Position::new(1, 2), Position::new(1, 3));
        let tokens3 = semantic_tokens_range(&doc, range3);
        assert_eq!(tokens3.data.len(), 0);
    }
}
