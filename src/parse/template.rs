//! Template, interpolation, directive, and heredoc indentation parsing for HCL.

use crate::ast::expr::Expression;
use crate::diagnostic::Diagnostics;
use crate::error::HclError;
use crate::parse::parser::Parser;
use crate::span::Span;

/// Strips the common leading whitespace prefix from non-empty lines in an indented heredoc.
///
/// # Arguments
/// * `content` - The raw inner content of the heredoc body (excluding the opening `<<-MARKER` line and the closing `MARKER` line).
///
/// # Errors
/// Returns [`HclError::Heredoc`] if indentation contains mixed tabs and spaces.
pub fn strip_heredoc_indentation(content: &str) -> Result<String, HclError> {
    if content.is_empty() {
        return Ok(String::new());
    }
    let raw_lines: Vec<&str> = content.split('\n').collect();
    for line in &raw_lines {
        let clean = line.strip_suffix('\r').unwrap_or(line);
        let leading_ws: String = clean
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if leading_ws.contains(' ') && leading_ws.contains('\t') {
            return Err(HclError::Heredoc(
                "Indented heredoc line contains mixed tabs and spaces in leading whitespace"
                    .to_string(),
            ));
        }
    }
    let mut non_empty_prefixes: Vec<&str> = Vec::new();
    let mut has_space_indent = false;
    let mut has_tab_indent = false;
    for line in &raw_lines {
        let clean = line.strip_suffix('\r').unwrap_or(line);
        if clean.trim().is_empty() {
            continue;
        }
        let ws_len = clean
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();
        let ws_prefix = &clean[..ws_len];
        if ws_prefix.contains(' ') {
            has_space_indent = true;
        }
        if ws_prefix.contains('\t') {
            has_tab_indent = true;
        }
        non_empty_prefixes.push(ws_prefix);
    }
    if has_space_indent && has_tab_indent {
        return Err(HclError::Heredoc(
            "Indented heredoc uses mixed tabs and spaces across lines for indentation".to_string(),
        ));
    }
    let common_prefix = if non_empty_prefixes.is_empty() {
        ""
    } else {
        let first = non_empty_prefixes[0];
        let mut max_len = first.len();
        for other in &non_empty_prefixes[1..] {
            let common_len = first[..max_len]
                .chars()
                .zip(other.chars())
                .take_while(|(a, b)| a == b)
                .count();
            max_len = max_len.min(common_len);
            if max_len == 0 {
                break;
            }
        }
        &first[..max_len]
    };
    let mut result_lines = Vec::with_capacity(raw_lines.len());
    for line in raw_lines {
        let has_cr = line.ends_with('\r');
        let clean = if has_cr {
            &line[..line.len() - 1]
        } else {
            line
        };
        let stripped = if clean.trim().is_empty() {
            clean.strip_prefix(common_prefix).unwrap_or("")
        } else {
            clean.strip_prefix(common_prefix).unwrap_or(clean)
        };
        if has_cr {
            result_lines.push(format!("{stripped}\r"));
        } else {
            result_lines.push(stripped.to_string());
        }
    }
    Ok(result_lines.join("\n"))
}

#[derive(Clone, Debug)]
pub(crate) enum RawDirective {
    If(Expression),
    ElseIf(Expression),
    Else,
    EndIf,
    For(Option<String>, String, Expression),
    EndFor,
}

#[derive(Clone, Debug)]
pub(crate) enum RawItem {
    Literal(String, Span),
    Interpolation(Expression, Span, bool, bool),
    Directive(RawDirective, Span, bool, bool),
}

fn scan_balanced_template_seq(
    content: &str,
    start: usize,
    strip_prefix: &str,
) -> (bool, bool, usize, usize, usize) {
    let is_strip_left = content[start..].starts_with(strip_prefix);
    let mut i = start + if is_strip_left { 3 } else { 2 };
    let mut depth = 1;
    while i < content.len() && depth > 0 {
        if content[i..].starts_with('{') {
            depth += 1;
            i += 1;
        } else if content[i..].starts_with('}') {
            depth -= 1;
            if depth > 0 {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    let is_strip_right = content.get(i.saturating_sub(1)..=i) == Some("~}");
    let end = if depth == 0 { i + 1 } else { i };
    let inner_start = start + if is_strip_left { 3 } else { 2 };
    let inner_end = if is_strip_right { end - 2 } else { end - 1 };
    (is_strip_left, is_strip_right, end, inner_start, inner_end)
}

fn parse_raw_directive(
    diags: &mut Diagnostics,
    directive_str: &str,
    span: &Span,
) -> Option<RawDirective> {
    if directive_str == "endif" {
        Some(RawDirective::EndIf)
    } else if directive_str == "endfor" {
        Some(RawDirective::EndFor)
    } else if directive_str == "else" {
        Some(RawDirective::Else)
    } else if let Some(stripped) = directive_str.strip_prefix("if ") {
        Parser::new(stripped.trim())
            .parse_expression()
            .map(RawDirective::If)
    } else if let Some(stripped) = directive_str
        .strip_prefix("else if ")
        .or_else(|| directive_str.strip_prefix("elif "))
    {
        Parser::new(stripped.trim())
            .parse_expression()
            .map(RawDirective::ElseIf)
    } else if let Some(stripped) = directive_str.strip_prefix("for ") {
        let body_str = stripped.trim();
        body_str.find(" in ").map_or_else(
            || {
                diags.push(crate::diagnostic::Diagnostic::error(
                    "Invalid %{ for } directive".to_string(),
                    "Expected `in` keyword in for directive, e.g., `%{ for val in collection }`"
                        .to_string(),
                    span.clone(),
                ));
                None
            },
            |in_pos| {
                let var_part = body_str[..in_pos].trim();
                let coll_part = body_str[in_pos + 4..].trim();
                let (key_var, val_var) = var_part.find(',').map_or_else(
                    || (None, var_part.to_string()),
                    |comma_pos| {
                        (
                            Some(var_part[..comma_pos].trim().to_string()),
                            var_part[comma_pos + 1..].trim().to_string(),
                        )
                    },
                );
                Parser::new(coll_part)
                    .parse_expression()
                    .map(|collection| RawDirective::For(key_var, val_var, collection))
            },
        )
    } else {
        diags.push(crate::diagnostic::Diagnostic::error(
            "Unknown template directive".to_string(),
            format!("Unknown directive `%{directive_str}`"),
            span.clone(),
        ));
        None
    }
}

fn parse_template_escape(
    diags: &mut Diagnostics,
    content: &str,
    i: &mut usize,
    literal_content: &mut String,
    span: &Span,
) {
    *i += 1;
    let Some(c) = content[*i..].chars().next() else {
        literal_content.push('\\');
        *i += 1;
        return;
    };
    match c {
        'n' => {
            literal_content.push('\n');
            *i += 1;
        }
        'r' => {
            literal_content.push('\r');
            *i += 1;
        }
        't' => {
            literal_content.push('\t');
            *i += 1;
        }
        '\\' => {
            literal_content.push('\\');
            *i += 1;
        }
        '"' => {
            literal_content.push('"');
            *i += 1;
        }
        'u' | 'U' => {
            let digits = if c == 'u' { 4 } else { 8 };
            *i += 1;
            if *i + digits <= content.len() {
                let code = u32::from_str_radix(&content[*i..*i + digits], 16).unwrap_or(0);
                if let Some(ch) = char::from_u32(code) {
                    literal_content.push(ch);
                } else {
                    diags.push(crate::diagnostic::Diagnostic::error(
                        "Invalid Unicode escape".to_string(),
                        "Invalid Unicode scalar value.".to_string(),
                        span.clone(),
                    ));
                }
                *i += digits;
            } else {
                diags.push(crate::diagnostic::Diagnostic::error(
                    "Invalid Unicode escape".to_string(),
                    format!("Expected {digits} hexadecimal digits after \\{c}."),
                    span.clone(),
                ));
                *i = content.len();
            }
        }
        _ => {
            literal_content.push(c);
            *i += c.len_utf8();
        }
    }
}

pub(crate) fn tokenize_template_raw_items(
    diags: &mut Diagnostics,
    content: &str,
    is_heredoc: bool,
    span: &Span,
) -> Vec<RawItem> {
    let mut raw_items = Vec::new();
    let mut i = 0;
    while i < content.len() {
        if content[i..].starts_with("${") {
            let (is_strip_left, is_strip_right, end, expr_start, expr_end) =
                scan_balanced_template_seq(content, i, "${~");
            let expr_str_raw = &content[expr_start..expr_end];
            let expr_str = if is_heredoc {
                expr_str_raw.to_string()
            } else {
                expr_str_raw.replace("\\\"", "\"")
            };
            if let Some(expr) = Parser::new(&expr_str).parse_expression() {
                raw_items.push(RawItem::Interpolation(
                    expr,
                    span.clone(),
                    is_strip_left,
                    is_strip_right,
                ));
            }
            i = end;
        } else if content[i..].starts_with("%{") {
            let (is_strip_left, is_strip_right, end, dir_start, dir_end) =
                scan_balanced_template_seq(content, i, "%{~");
            let dir_str_raw = content[dir_start..dir_end].trim();
            let dir_str = if is_heredoc {
                dir_str_raw.to_string()
            } else {
                dir_str_raw.replace("\\\"", "\"")
            };
            if let Some(d) = parse_raw_directive(diags, &dir_str, span) {
                raw_items.push(RawItem::Directive(
                    d,
                    span.clone(),
                    is_strip_left,
                    is_strip_right,
                ));
            }
            i = end;
        } else {
            let mut literal_content = String::new();
            while i < content.len() {
                if content[i..].starts_with("$${") {
                    literal_content.push_str("${");
                    i += 3;
                    continue;
                }
                if content[i..].starts_with("%%{") {
                    literal_content.push_str("%{");
                    i += 3;
                    continue;
                }
                if content[i..].starts_with("${") || content[i..].starts_with("%{") {
                    break;
                }
                if !is_heredoc && content[i..].starts_with('\\') {
                    parse_template_escape(diags, content, &mut i, &mut literal_content, span);
                } else {
                    let c = content[i..].chars().next().unwrap_or_default();
                    literal_content.push(c);
                    i += c.len_utf8();
                }
            }
            if !literal_content.is_empty() {
                raw_items.push(RawItem::Literal(literal_content, span.clone()));
            }
        }
    }
    raw_items
}

pub(crate) fn apply_template_strip_markers(raw_items: &mut [RawItem]) {
    for idx in 0..raw_items.len() {
        let (strip_left, strip_right) = match &raw_items[idx] {
            RawItem::Literal(_, _) => (false, false),
            RawItem::Interpolation(_, _, sl, sr) | RawItem::Directive(_, _, sl, sr) => (*sl, *sr),
        };
        if strip_left && idx > 0 {
            if let RawItem::Literal(ref mut s, _) = raw_items[idx - 1] {
                *s = s.trim_end().to_string();
            }
        }
        if strip_right && idx + 1 < raw_items.len() {
            if let RawItem::Literal(ref mut s, _) = raw_items[idx + 1] {
                *s = s.trim_start().to_string();
            }
        }
    }
}

const fn is_end_if(item: Option<&RawItem>) -> bool {
    matches!(item, Some(RawItem::Directive(RawDirective::EndIf, ..)))
}

const fn is_end_for(item: Option<&RawItem>) -> bool {
    matches!(item, Some(RawItem::Directive(RawDirective::EndFor, ..)))
}

fn build_if_template_part(
    items: &[RawItem],
    pos: &mut usize,
    diags: &mut Diagnostics,
    cond: Expression,
    if_span: Span,
) -> crate::ast::expr::TemplatePart {
    use crate::ast::expr::{Directive, TemplatePart};
    *pos += 1;
    let true_expr = build_template_tree(items, pos, diags, &["elseif", "else", "endif"]);
    let mut else_ifs = Vec::new();
    while *pos < items.len() {
        if let RawItem::Directive(RawDirective::ElseIf(elif_cond), ..) = &items[*pos] {
            let elif_cond = elif_cond.clone();
            *pos += 1;
            let elif_body = build_template_tree(items, pos, diags, &["elseif", "else", "endif"]);
            else_ifs.push((elif_cond, elif_body));
        } else {
            break;
        }
    }
    let mut false_expr = None;
    if *pos < items.len() {
        if let RawItem::Directive(RawDirective::Else, ..) = &items[*pos] {
            *pos += 1;
            false_expr = Some(build_template_tree(items, pos, diags, &["endif"]));
        }
    }
    if is_end_if(items.get(*pos)) {
        *pos += 1;
    } else {
        diags.push(crate::diagnostic::Diagnostic::error(
            "Unclosed %{ if } directive",
            "Expected %{ endif } to close %{ if } directive",
            if_span.clone(),
        ));
    }
    TemplatePart::Directive(
        Directive::If {
            cond,
            true_expr,
            else_ifs,
            false_expr,
        },
        if_span,
    )
}

pub(crate) fn build_template_tree(
    items: &[RawItem],
    pos: &mut usize,
    diags: &mut Diagnostics,
    terminators: &[&str],
) -> Vec<crate::ast::expr::TemplatePart> {
    use crate::ast::expr::{Directive, TemplatePart};
    let mut parts = Vec::new();
    while *pos < items.len() {
        match &items[*pos] {
            RawItem::Literal(s, sp) => {
                if !s.is_empty() {
                    parts.push(TemplatePart::Literal(s.clone(), sp.clone()));
                }
                *pos += 1;
            }
            RawItem::Interpolation(expr, sp, _, _) => {
                parts.push(TemplatePart::Interpolation(expr.clone(), sp.clone()));
                *pos += 1;
            }
            RawItem::Directive(directive, sp, _, _) => match directive {
                RawDirective::If(cond) => {
                    parts.push(build_if_template_part(
                        items,
                        pos,
                        diags,
                        cond.clone(),
                        sp.clone(),
                    ));
                }
                RawDirective::For(k, v, c) => {
                    *pos += 1;
                    let body = build_template_tree(items, pos, diags, &["endfor"]);
                    if is_end_for(items.get(*pos)) {
                        *pos += 1;
                    } else {
                        diags.push(crate::diagnostic::Diagnostic::error(
                            "Unclosed %{ for } directive",
                            "Expected %{ endfor } to close %{ for } directive",
                            sp.clone(),
                        ));
                    }
                    parts.push(TemplatePart::Directive(
                        Directive::For {
                            key_var: k.clone(),
                            val_var: v.clone(),
                            collection: c.clone(),
                            body,
                        },
                        sp.clone(),
                    ));
                }
                RawDirective::ElseIf(_) if terminators.contains(&"elseif") => return parts,
                RawDirective::Else if terminators.contains(&"else") => return parts,
                RawDirective::EndIf if terminators.contains(&"endif") => return parts,
                RawDirective::EndFor if terminators.contains(&"endfor") => return parts,
                _ => {
                    diags.push(crate::diagnostic::Diagnostic::error(
                        "Unexpected template directive".to_string(),
                        "Directive is unexpected in this position".to_string(),
                        sp.clone(),
                    ));
                    *pos += 1;
                }
            },
        }
    }
    parts
}
