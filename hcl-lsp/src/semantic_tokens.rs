//! Semantic tokens highlighting provider.
//!
//! Classifies tokens into keywords, block types, block labels, attribute names,
//! functions, numbers, strings, and operators according to LSP 3.17 delta format.
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::cache::VirtualDocument;
use crate::protocol::{Range, SemanticTokens, SemanticTokensLegend};
use hashicorp_configuration_language_rs::ast::expr::{Expression, TemplatePart, TraversalOperator};
use hashicorp_configuration_language_rs::ast::structure::Body;
use hashicorp_configuration_language_rs::span::Span;

/// Standard token types advertised by this language server.
pub const TOKEN_TYPES: &[&str] = &[
    "keyword", "type", "class", "property", "variable", "string", "number", "operator", "function",
    "comment",
];

/// Standard token modifiers advertised by this language server.
pub const TOKEN_MODIFIERS: &[&str] = &["declaration", "readonly", "defaultLibrary"];

/// Strongly-typed semantic token classification matching indices in [`TOKEN_TYPES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SemanticTokenType {
    /// Language keyword or boolean literal (`0`).
    Keyword = 0,
    /// Block type identifier (`1`).
    Type = 1,
    /// Class identifier (`2`).
    Class = 2,
    /// Attribute property identifier (`3`).
    Property = 3,
    /// Variable reference (`4`).
    Variable = 4,
    /// String literal or block label (`5`).
    String = 5,
    /// Numeric literal (`6`).
    Number = 6,
    /// Operator symbol (`7`).
    Operator = 7,
    /// Function identifier (`8`).
    Function = 8,
    /// Comment token (`9`).
    Comment = 9,
}

/// Strongly-typed semantic token modifier bitflags matching [`TOKEN_MODIFIERS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SemanticTokenModifier {
    /// No modifier flags set (`0`).
    None = 0,
    /// Declaration site bitflag (`1 << 0`).
    Declaration = 1,
    /// Readonly symbol bitflag (`1 << 1`).
    Readonly = 2,
    /// Standard library symbol bitflag (`1 << 2`).
    DefaultLibrary = 4,
}

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
    for attr in body.attributes.values() {
        add_token(
            tokens,
            &attr.name_span,
            SemanticTokenType::Property,
            SemanticTokenModifier::Declaration,
        );
        collect_expr_tokens(&attr.expr, tokens);
    }
    for block in &body.blocks {
        add_token(
            tokens,
            &block.type_span,
            SemanticTokenType::Type,
            SemanticTokenModifier::None,
        );
        for label_span in &block.label_spans {
            add_token(
                tokens,
                label_span,
                SemanticTokenType::String,
                SemanticTokenModifier::Declaration,
            );
        }
        collect_body_tokens(&block.body, tokens);
    }
}

/// Recursively traverses an expression adding tokens for sub-expressions.
fn collect_expr_tokens(expr: &Expression, tokens: &mut Vec<RawSemanticToken>) {
    match expr {
        Expression::Number(_, span) => {
            add_token(
                tokens,
                span,
                SemanticTokenType::Number,
                SemanticTokenModifier::None,
            );
        }
        Expression::String(_, span) => {
            add_token(
                tokens,
                span,
                SemanticTokenType::String,
                SemanticTokenModifier::None,
            );
        }
        Expression::Bool(_, span) | Expression::Null(span) => {
            add_token(
                tokens,
                span,
                SemanticTokenType::Keyword,
                SemanticTokenModifier::None,
            );
        }
        Expression::Variable(_, span) => {
            add_token(
                tokens,
                span,
                SemanticTokenType::Variable,
                SemanticTokenModifier::None,
            );
        }
        Expression::FuncCall(fc, _) => {
            add_token(
                tokens,
                &fc.name.span,
                SemanticTokenType::Function,
                SemanticTokenModifier::None,
            );
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
        Expression::Template(parts, span) => {
            collect_template_tokens(parts, span, tokens);
        }
        Expression::Traversal(trav, _) => {
            collect_traversal_tokens(trav, tokens);
        }
        Expression::ForExpr(for_expr, _) => {
            collect_expr_tokens(&for_expr.collection, tokens);
            if let Some(ref key_expr) = for_expr.key_expr {
                collect_expr_tokens(key_expr, tokens);
            }
            collect_expr_tokens(&for_expr.val_expr, tokens);
            if let Some(ref cond_expr) = for_expr.cond_expr {
                collect_expr_tokens(cond_expr, tokens);
            }
        }
    }
}

/// Collects semantic tokens from a template expression.
fn collect_template_tokens(
    parts: &[TemplatePart],
    span: &Span,
    tokens: &mut Vec<RawSemanticToken>,
) {
    if parts.is_empty() {
        add_token(
            tokens,
            span,
            SemanticTokenType::String,
            SemanticTokenModifier::None,
        );
    } else {
        for part in parts {
            match part {
                TemplatePart::Literal(_, lit_span) => {
                    add_token(
                        tokens,
                        lit_span,
                        SemanticTokenType::String,
                        SemanticTokenModifier::None,
                    );
                }
                TemplatePart::Interpolation(inner, _) => {
                    collect_expr_tokens(inner, tokens);
                }
                TemplatePart::Directive(_, _) => {}
            }
        }
    }
}

/// Collects semantic tokens from a traversal expression.
fn collect_traversal_tokens(
    trav: &hashicorp_configuration_language_rs::ast::expr::Traversal,
    tokens: &mut Vec<RawSemanticToken>,
) {
    collect_expr_tokens(&trav.expr, tokens);
    for op in &trav.operators {
        match op {
            TraversalOperator::GetAttr(_, attr_span) => {
                add_token(
                    tokens,
                    attr_span,
                    SemanticTokenType::Property,
                    SemanticTokenModifier::None,
                );
            }
            TraversalOperator::Index(idx_expr, _) => {
                collect_expr_tokens(idx_expr, tokens);
            }
            TraversalOperator::LegacyIndex(_, idx_span) => {
                add_token(
                    tokens,
                    idx_span,
                    SemanticTokenType::Number,
                    SemanticTokenModifier::None,
                );
            }
            TraversalOperator::AttrSplat(splat_span) | TraversalOperator::FullSplat(splat_span) => {
                add_token(
                    tokens,
                    splat_span,
                    SemanticTokenType::Operator,
                    SemanticTokenModifier::None,
                );
            }
        }
    }
}

/// Converts a source span into a raw semantic token if valid.
fn add_token(
    tokens: &mut Vec<RawSemanticToken>,
    span: &Span,
    token_type: SemanticTokenType,
    token_modifiers: SemanticTokenModifier,
) {
    if span.end_byte <= span.start_byte {
        return;
    }
    let line = u32::try_from(span.start_line.saturating_sub(1)).unwrap_or(u32::MAX);
    let character = u32::try_from(span.start_col.saturating_sub(1)).unwrap_or(u32::MAX);
    let length = u32::try_from(span.end_byte - span.start_byte).unwrap_or(u32::MAX);
    tokens.push(RawSemanticToken {
        line,
        character,
        length,
        token_type: token_type as u32,
        token_modifiers: token_modifiers as u32,
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
        let doc = VirtualDocument::new(item, None);
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
        let mut doc = VirtualDocument::new(item, None);
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
            interp_tmpl = "hello ${my_var} %{if true}ok%{endif}"
            trav = foo.bar[0].0.*.baz[*]
            null_val = null
            comprehension = [for x in [1]: x if true]
            map_comp = {for k, v in {a = 1}: k => v}
        "#;
        let item = TextDocumentItem {
            uri: "file:///all.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };
        let doc = VirtualDocument::new(item, None);
        let tokens = semantic_tokens_full(&doc);
        assert_ne!(tokens.data.len(), 0);
        assert_eq!(SemanticTokenType::Class as u32, 2);
        assert_eq!(SemanticTokenType::Comment as u32, 9);
        assert_eq!(SemanticTokenModifier::Readonly as u32, 2);
        assert_eq!(SemanticTokenModifier::DefaultLibrary as u32, 4);
    }

    #[test]
    fn test_add_token_zero_and_inverted_span() {
        let mut tokens = Vec::new();
        let zero_span = Span::new(10, 10, 1, 1, 1, 1);
        add_token(
            &mut tokens,
            &zero_span,
            SemanticTokenType::Keyword,
            SemanticTokenModifier::None,
        );
        assert_eq!(tokens.len(), 0);
        let inv_span = Span::new(10, 5, 1, 1, 1, 1);
        add_token(
            &mut tokens,
            &inv_span,
            SemanticTokenType::Keyword,
            SemanticTokenModifier::None,
        );
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_semantic_tokens_range_filtering_branches() {
        let hcl = "a = 1
b = 2
c = 3
d = 4
e = 5
";
        let item = TextDocumentItem {
            uri: "file:///lines.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: hcl.to_string(),
        };
        let doc = VirtualDocument::new(item, None);
        let range1 = Range::new(Position::new(1, 2), Position::new(3, 2));
        let tokens1 = semantic_tokens_range(&doc, range1);
        assert_ne!(tokens1.data.len(), 0);
        let range2 = Range::new(Position::new(1, 10), Position::new(1, 12));
        let tokens2 = semantic_tokens_range(&doc, range2);
        assert_eq!(tokens2.data.len(), 0);
        let range3 = Range::new(Position::new(1, 2), Position::new(1, 3));
        let tokens3 = semantic_tokens_range(&doc, range3);
        assert_eq!(tokens3.data.len(), 0);
    }
}
